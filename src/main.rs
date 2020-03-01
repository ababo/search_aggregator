mod api;
mod meta;

use std::{
    env,
    io::{Error, ErrorKind},
    time::{Duration, Instant},
};

use actix_web::{
    client::Client,
    get,
    http::StatusCode,
    web::{Data as WebData, Query},
    App, HttpRequest, HttpResponse, HttpServer, Result,
};
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
    engine: Engine,
    query: &str,
    keywords: &Vec<String>,
    with_meta: bool,
) -> Result<(Vec<Document>, Duration, Vec<Meta>), ApiError> {
    let start = Instant::now();
    let docs = search(&data.client, engine, query).await?;
    let dur = start.elapsed();

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

#[derive(Debug, Deserialize)]
pub struct SearchRequest {
    query: String,
    meta: bool,
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

const MAX_NUM_OF_DOCS: usize = 10;

#[get("/search")]
async fn handle_search(
    req: HttpRequest,
    Query(params): Query<SearchRequest>,
) -> Result<HttpResponse> {
    let start = Instant::now();
    info!("{:?}", params);

    let data = req.app_data::<WebData<Data>>().unwrap();
    let keywords = get_keywords(&data.rake, &params.query);
    info!("keywords: {:?}", keywords);

    let gfut = process_api(
        &data,
        Engine::Google,
        &params.query,
        &keywords,
        params.meta,
    );
    let bfut =
        process_api(&data, Engine::Bing, &params.query, &keywords, params.meta);
    let ((gdocs, gdur, gmetas), (bdocs, bdur, bmetas)) = try_join!(gfut, bfut)?;

    let mut docs = vec![];
    docs.extend(gdocs);
    docs.extend(bdocs);

    let mut metas = vec![];
    metas.extend(gmetas);
    metas.extend(bmetas);

    let mut zipped: Vec<(Document, Meta)> =
        docs.into_iter().zip(metas.into_iter()).collect();

    if params.meta {
        zipped.sort_by(|a, b| b.1.score.partial_cmp(&a.1.score).unwrap());
    }

    if zipped.len() > MAX_NUM_OF_DOCS {
        zipped.truncate(MAX_NUM_OF_DOCS);
    }

    let dur = start.elapsed();

    Ok(HttpResponse::build(StatusCode::OK)
        .content_type("text/html; charset=utf-8")
        .body(format!(
            "resp: {:?} {:?} {:?} {:?}",
            gdur, bdur, dur, zipped
        )))
}

#[actix_rt::main]
async fn main() -> std::io::Result<()> {
    env_logger::init();

    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        return Err(Error::new(
            ErrorKind::Other,
            "no stop words file provided",
        ));
    }

    let sw = StopWords::from_file(&args[1])?;

    HttpServer::new(move || {
        App::new()
            .data(Data {
                client: Client::default(),
                rake: Rake::new(sw.clone()),
            })
            .service(handle_search)
    })
    .bind("127.0.0.1:8080")?
    .run()
    .await
}
