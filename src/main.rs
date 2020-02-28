mod api;
mod meta;

use std::time::{Duration, Instant};

use actix_session::Session;
use actix_web::{
    client::Client, get, http::StatusCode, App, HttpRequest, HttpResponse,
    HttpServer, Result,
};
use awc::error::JsonPayloadError;
use futures::{future::join_all, try_join};

use crate::{
    api::{search, Document, Engine},
    meta::{generate as generate_meta, Meta},
};

async fn process_api(
    engine: Engine,
    client: &Client,
    query: &str,
    with_meta: bool,
) -> Result<(Vec<Document>, Duration, Option<Vec<Meta>>), JsonPayloadError> {
    let start = Instant::now();
    let docs = search(engine, client, query).await?;
    let dur = start.elapsed();

    let metas = if with_meta {
        let futs: Vec<_> =
            docs.iter().map(|doc| generate_meta(client, doc)).collect();
        Some(join_all(futs).await)
    } else {
        None
    };

    Ok((docs, dur, metas))
}

#[get("/search")]
async fn handle_search(
    _session: Session,
    _req: HttpRequest,
) -> Result<HttpResponse> {
    let start = Instant::now();
    let client = Client::default();

    let query = "pizza";
    let with_meta = true;
    let gfut = process_api(Engine::Google, &client, query, with_meta);
    let bfut = process_api(Engine::Bing, &client, query, with_meta);
    let ((gdocs, gdur, gmetas), (bdocs, bdur, bmetas)) = try_join!(gfut, bfut)?;

    let mut docs = vec![];
    docs.extend(gdocs);
    docs.extend(bdocs);

    let mut metas = vec![];
    metas.extend(gmetas);
    metas.extend(bmetas);

    let dur = start.elapsed();

    Ok(HttpResponse::build(StatusCode::OK)
        .content_type("text/html; charset=utf-8")
        .body(format!(
            "resp: {:?} {:?} {:?} {:?} {:?}",
            gdur, bdur, dur, docs, metas
        )))
}

#[actix_rt::main]
async fn main() -> std::io::Result<()> {
    HttpServer::new(|| App::new().service(handle_search))
        .bind("127.0.0.1:8080")?
        .run()
        .await
}
