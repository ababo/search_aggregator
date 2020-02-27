use futures::join;

use actix_session::Session;
use actix_web::http::StatusCode;
use actix_web::{get, App, HttpRequest, HttpResponse, HttpServer, Result};
use awc::Client;

mod search;

#[get("/search")]
async fn handle_search(
    _session: Session,
    _req: HttpRequest,
) -> Result<HttpResponse> {
    let client = Client::default();

    let google_resp = search::with_google(&client, "pizza");
    let bing_resp = search::with_bing(&client, "pizza");
    let both = join!(google_resp, bing_resp);

    Ok(HttpResponse::build(StatusCode::OK)
        .content_type("text/html; charset=utf-8")
        .body(format!("resp: {:?}", both)))
}

#[actix_rt::main]
async fn main() -> std::io::Result<()> {
    HttpServer::new(|| App::new().service(handle_search))
        .bind("127.0.0.1:8080")?
        .run()
        .await
}
