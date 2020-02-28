use actix_web::client::Client;
use html5ever::{
    tendril::StrTendril,
    tokenizer::{
        BufferQueue, Tag, TagKind, Token, TokenSink, TokenSinkResult,
        Tokenizer, TokenizerOpts,
    },
};

use crate::api::Document;

const NON_TEXTUAL_TAGS: &'static [&'static str] = &["script"];

#[derive(Debug)]
pub struct Meta {
    pub snippet: String,
    pub score: f64,
}

struct TokenHandler {
    text: String,
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
                    let trimmed: Vec<String> = tendril
                        .to_string()
                        .split_whitespace()
                        .map(|s| s.to_string())
                        .collect();
                    let joined = trimmed.join(" ");
                    if joined.len() > 0 {
                        self.text += " ";
                        self.text += &joined;
                    }
                }
            }
            _ => (),
        }
        TokenSinkResult::Continue
    }
}

async fn load_doc_text(client: &Client, link: &str) -> Option<String> {
    let mut resp = client.get(link).send().await.ok()?;

    // TODO: Support streaming instead.
    let body = resp.body().await.ok()?;

    let mut tok = Tokenizer::new(
        TokenHandler {
            text: "".to_string(),
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

    Some(tok.sink.text.trim().to_string())
}

pub async fn generate(client: &Client, doc: &Document) -> Meta {
    let meta = Meta {
        snippet: "".to_string(),
        score: 0.0,
    };

    let text = load_doc_text(client, &doc.link).await;
    if text == None {
        // The link is invalid, so it will be discarded by score.
        return meta;
    }

    //println!("text {:?}", text.unwrap());

    meta
}
