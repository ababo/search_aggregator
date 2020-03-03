mod api;
mod meta;

use std::{
    env,
    io::{Error, ErrorKind},
    str::from_utf8,
    time::{Duration, Instant},
};

use actix_web::{
    client::Client,
    get,
    http::StatusCode,
    web::{Data as WebData, Query},
    App, HttpRequest, HttpResponse, HttpServer, Result,
};
use askama::Template;
use futures::{future::join_all, try_join};
use log::info;
use rake::{Rake, StopWords};
use serde::Deserialize;

use crate::{
    api::{search, Document, Engine, Error as ApiError},
    meta::{generate as generate_meta, Meta},
};

struct Data {
    client: Client,
    rake: Rake,
}

async fn process_api(
    data: &Data,
    engine: &Engine,
    query: &str,
    keywords: &Vec<String>,
    with_meta: bool,
) -> Result<(Vec<Document>, Duration, Vec<Meta>), ApiError> {
    let start = Instant::now();
    let docs = search(&data.client, engine, query).await?;
    let dur = start.elapsed();

    info!(
        "received data from {:?} API ({:?} docs)",
        engine,
        docs.len()
    );

    let metas = if with_meta {
        let futs: Vec<_> = docs
            .iter()
            .map(|doc| generate_meta(&data.client, keywords, doc))
            .collect();
        join_all(futs).await
    } else {
        vec![Meta::new(); docs.len()]
    };

    Ok((docs, dur, metas))
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct SearchParams {
    #[serde(default)]
    query: String,
    #[serde(default)]
    meta: bool,
}

#[derive(Default, Template)]
#[template(path = "search.html")]
struct SearchTemplate<'a> {
    name: &'a str,
    params: SearchParams,
    google_dur: Duration,
    bing_dur: Duration,
    total_dur: Duration,
    items: Vec<(Document, Meta)>,
}

fn get_keywords(rake: &Rake, text: &str) -> Vec<String> {
    let keywords = rake.run(text);
    let mut extracted: Vec<String> = if keywords.len() > 0 {
        keywords
            .into_iter()
            .map(|kws| kws.keyword.to_lowercase())
            .collect()
    } else {
        text.split(" ")
            .map(|s| String::from(s).to_lowercase())
            .collect()
    };
    // Add the whole text for a chance of exact match.
    extracted.push(text.to_string());
    extracted
}

const PAGE_NAME: &'static str = "Search Aggregator";
const MAX_NUM_OF_ITEMS: usize = 10;
const MIN_ITEM_SCORE: f64 = 0.7;

async fn process_search(
    data: &Data,
    params: &SearchParams,
) -> Result<SearchTemplate<'static>> {
    let start = Instant::now();
    info!("request params: {:?}", params);

    let keywords = get_keywords(&data.rake, &params.query);
    info!("query keywords: {:?}", keywords);

    let gfut = process_api(
        &data,
        &Engine::Google,
        &params.query,
        &keywords,
        params.meta,
    );

    let bfut = process_api(
        &data,
        &Engine::Bing,
        &params.query,
        &keywords,
        params.meta,
    );

    let ((gdocs, gdur, gmetas), (bdocs, bdur, bmetas)) = try_join!(gfut, bfut)?;

    let mut docs = vec![];
    docs.extend(gdocs);
    docs.extend(bdocs);

    let mut metas = vec![];
    metas.extend(gmetas);
    metas.extend(bmetas);

    let mut zipped: Vec<(Document, Meta)> = docs
        .into_iter()
        .zip(metas.into_iter())
        .filter(|item| {
            item.1.snippet.len() > 0
                && (!params.meta || item.1.score >= MIN_ITEM_SCORE)
        })
        .collect();

    if params.meta {
        zipped.sort_by(|a, b| b.1.score.partial_cmp(&a.1.score).unwrap());
    }

    if zipped.len() > MAX_NUM_OF_ITEMS {
        zipped.truncate(MAX_NUM_OF_ITEMS);
    }

    Ok(SearchTemplate {
        name: PAGE_NAME,
        params: params.clone(),
        google_dur: gdur,
        bing_dur: bdur,
        total_dur: start.elapsed(),
        items: zipped,
        ..Default::default()
    })
}

#[get("/search")]
async fn handle_search(
    req: HttpRequest,
    Query(params): Query<SearchParams>,
) -> Result<HttpResponse> {
    let template = if params.query != "" {
        let data = req.app_data::<WebData<Data>>().unwrap();
        process_search(&data, &params).await?
    } else {
        SearchTemplate {
            name: PAGE_NAME,
            params,
            ..Default::default()
        }
    };

    Ok(HttpResponse::build(StatusCode::OK)
        .content_type("text/html; charset=utf-8")
        .body(template.render().unwrap()))
}

#[actix_rt::main]
async fn main() -> std::io::Result<()> {
    env_logger::init();

    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        return Err(Error::new(ErrorKind::Other, "no address provided"));
    }

    let mut sw = StopWords::new();
    let bytes = include_bytes!("../stop_words.en.txt");
    for word in bytes.split(|b| *b == ('\n' as u8)) {
        sw.insert(from_utf8(word).unwrap().to_string());
    }

    HttpServer::new(move || {
        App::new()
            .data(Data {
                client: Client::default(),
                rake: Rake::new(sw.clone()),
            })
            .service(handle_search)
    })
    .bind(&args[1])?
    .run()
    .await
}
