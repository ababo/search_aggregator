use std::cmp::Ordering;

use actix_web::client::Client;
use html5ever::{
    tendril::StrTendril,
    tokenizer::{
        BufferQueue, Tag, TagKind, Token, TokenSink, TokenSinkResult,
        Tokenizer, TokenizerOpts,
    },
};
use stats::mean;
use strsim::jaro_winkler;

use crate::api::Document;

const NON_TEXTUAL_TAGS: &'static [&'static str] = &["script", "style"];

struct TokenHandler {
    words: Vec<String>,
    skip: isize,
}

impl TokenSink for TokenHandler {
    type Handle = ();

    fn process_token(
        &mut self,
        token: Token,
        _line_number: u64,
    ) -> TokenSinkResult<()> {
        match token {
            Token::TagToken(Tag {
                kind,
                name,
                self_closing: _,
                attrs: _,
            }) => {
                let tag: &str = &name.to_ascii_lowercase();
                if NON_TEXTUAL_TAGS.contains(&tag) {
                    self.skip += if kind == TagKind::StartTag { 1 } else { -1 }
                }
            }
            Token::CharacterTokens(tendril) => {
                if self.skip == 0 {
                    let words: Vec<String> = tendril
                        .to_string()
                        .split_whitespace()
                        .map(|s| s.to_string())
                        .collect();
                    self.words.extend(words);
                }
            }
            _ => (),
        }
        TokenSinkResult::Continue
    }
}

async fn load_doc_words(client: &Client, link: &str) -> Option<Vec<String>> {
    let mut resp = client.get(link).send().await.ok()?;

    // TODO: Support streaming instead.
    let body = resp.body().await.ok()?;

    let mut tok = Tokenizer::new(
        TokenHandler {
            words: vec![],
            skip: 0,
        },
        TokenizerOpts {
            profile: true,
            ..Default::default()
        },
    );

    let mut input = BufferQueue::new();
    let html = StrTendril::try_from_byte_slice(&body).ok()?;
    input.push_back(html);
    let _ = tok.feed(&mut input);

    Some(tok.sink.words)
}

#[derive(Debug)]
struct Match {
    index: usize,
    count: usize,
    score: f64,
}

fn match_keywords(keywords: &Vec<String>, words: &Vec<String>) -> Vec<Match> {
    let mut matches = vec![];
    for keyword in keywords {
        let klen = keyword.split(" ").count();
        if klen > words.len() {
            continue;
        }

        let kmatch = (0..words.len() - klen)
            .map(|i| {
                let part = &words[i..i + klen].join(" ").to_lowercase();
                let score = jaro_winkler(part, keyword);
                Match {
                    index: i,
                    count: klen,
                    score,
                }
            })
            .max_by(|a, b| {
                if a.score > b.score {
                    Ordering::Greater
                } else {
                    Ordering::Less
                }
            });
        matches.push(kmatch.unwrap());
    }

    matches
}

fn generate_snippet(
    words: &Vec<String>,
    matches: &Vec<Match>,
    max_context_words_per_match: usize,
) -> String {
    let mut snippet = String::new();

    let mut i = 0;
    for j in 0..words.len() {
        if j >= matches[i].index
            + matches[i].count
            + max_context_words_per_match
            && matches.len() > i + 1
        {
            i += 1;
        }

        if (j as isize)
            < (matches[i].index as isize)
                - (max_context_words_per_match as isize)
        {
            continue;
        }

        if j < matches[i].index {
            if i == 0
                && (j as isize)
                    == (matches[i].index as isize)
                        - (max_context_words_per_match as isize)
            {
                snippet.push_str("... ");
            }

            snippet.push_str(&words[j]);
            snippet.push_str(" ");

            continue;
        }

        if j < matches[i].index + matches[i].count {
            if j == matches[i].index {
                snippet.push_str("<b>");
            }

            snippet.push_str(&words[j]);

            if j == matches[i].index + matches[i].count - 1 {
                snippet.push_str("</b>");
            }

            snippet.push_str(" ");

            continue;
        }

        if j < matches[i].index + matches[i].count + max_context_words_per_match
        {
            snippet.push_str(&words[j]);
            snippet.push_str(" ");

            // Switch to a next match, if it starts from the next word.
            if matches.len() > i + 1 && matches[i + 1].index == j + 1 {
                i += 1
            } else if j < words.len() - 1
                && j == matches[i].index
                    + matches[i].count
                    + max_context_words_per_match
                    - 1
            {
                snippet.push_str("... ");
            }

            continue;
        }
    }

    snippet.trim_end().to_string()
}

#[derive(Clone, Debug)]
pub struct Meta {
    pub snippet: String,
    pub score: f64,
}

impl Meta {
    pub fn new() -> Meta {
        Meta {
            snippet: "<no snippet available>".to_string(),
            score: -1.0,
        }
    }
}

const SCORE_EPSILON: f64 = 0.05;
const MAX_CONTEXT_WORDS_PER_MATCH: usize = 10;
const MAX_MATCHES_PER_SNIPPET: usize = 3;

pub async fn generate(
    client: &Client,
    keywords: &Vec<String>,
    doc: &Document,
) -> Meta {
    let mut meta = Meta::new();

    let words = load_doc_words(client, &doc.link).await;
    if words == None {
        // The link is invalid, so it will probably be discarded by its score.
        return meta;
    }

    let mut matches = match_keywords(keywords, words.as_ref().unwrap());

    if matches.len() > MAX_MATCHES_PER_SNIPPET {
        // Retain no more than MAX_MATCHES_PER_SNIPPET with highest score.
        matches.sort_by(|a, b| {
            // Prefer longer sequences for more or less equal scores.
            if (a.score - b.score).abs() < SCORE_EPSILON {
                b.count.partial_cmp(&a.count)
            } else {
                b.score.partial_cmp(&a.score)
            }
            .unwrap()
        });
        matches.truncate(MAX_MATCHES_PER_SNIPPET);
    }

    matches.sort_by(|a, b| a.index.partial_cmp(&b.index).unwrap());

    let snippet = generate_snippet(
        words.as_ref().unwrap(),
        &matches,
        MAX_CONTEXT_WORDS_PER_MATCH,
    );
    let score = mean(matches.iter().map(|m| m.score));

    meta.snippet = snippet;
    meta.score = score;
    meta
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_snippet_with_single_match() {
        let words =
            vec!["abc", "def", "ghi", "jkl", "mno", "pqr", "stu", "vwx", "yz"]
                .into_iter()
                .map(String::from)
                .collect();

        let matches = vec![Match {
            index: 3,
            count: 2,
            score: 0.0,
        }];

        assert_eq!(
            generate_snippet(&words, &matches, 2),
            "... def ghi <b>jkl mno</b> pqr stu ..."
        );
    }

    #[test]
    fn test_generate_snippet_with_separate_matches() {
        let words =
            vec!["abc", "def", "ghi", "jkl", "mno", "pqr", "stu", "vwx", "yz"]
                .into_iter()
                .map(String::from)
                .collect();

        let matches = vec![
            Match {
                index: 1,
                count: 1,
                score: 0.0,
            },
            Match {
                index: 6,
                count: 1,
                score: 0.0,
            },
        ];

        assert_eq!(
            generate_snippet(&words, &matches, 2),
            "abc <b>def</b> ghi jkl ... mno pqr <b>stu</b> vwx yz"
        );
    }

    #[test]
    fn test_generate_snippet_with_intersecting_matches() {
        let words =
            vec!["abc", "def", "ghi", "jkl", "mno", "pqr", "stu", "vwx", "yz"]
                .into_iter()
                .map(String::from)
                .collect();

        let matches = vec![
            Match {
                index: 1,
                count: 2,
                score: 0.0,
            },
            Match {
                index: 5,
                count: 1,
                score: 0.0,
            },
        ];

        assert_eq!(
            generate_snippet(&words, &matches, 2),
            "abc <b>def ghi</b> jkl mno <b>pqr</b> stu vwx ..."
        );
    }
}
