use std::time::{Duration, Instant};

use awc::error::JsonPayloadError;
use awc::Client;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Item {
    #[serde(alias = "url")]
    pub link: String,
    #[serde(alias = "name")]
    pub title: String,
    pub snippet: String,
}

#[derive(Debug, Deserialize)]
pub struct Response {
    #[serde(default)]
    pub duration: Duration,
    #[serde(alias = "value")]
    pub items: Vec<Item>,
}

#[derive(Deserialize)]
struct BingWrapper {
    #[serde(rename = "webPages")]
    web_pages: Response,
}

const GOOGLE_URL: &'static str = "https://www.googleapis.com/customsearch/v1";
const GOOGLE_KEY: &'static str = "AIzaSyCKZXnEw9whbifuySpwncm584Op59e7Z5U";
const GOOGLE_CX: &'static str = "002842975906381566051:zico5aaukxe";

const BING_URL: &'static str =
    "https://api.cognitive.microsoft.com/bing/v7.0/search";
const BING_KEY: &'static str = "8442e9e17ece414bac0f45d3dce2264d";

pub async fn with_google(
    client: &Client,
    query: &str,
) -> Result<Response, JsonPayloadError> {
    let start = Instant::now();

    let mut resp = client
        .get(format!(
            "{}?key={}&cx={}&q={}",
            GOOGLE_URL, GOOGLE_KEY, GOOGLE_CX, query
        ))
        .send()
        .await
        .unwrap();

    let mut resp = resp.json::<Response>().await?;
    resp.duration = start.elapsed();
    return Ok(resp);
}

pub async fn with_bing(
    client: &Client,
    query: &str,
) -> Result<Response, JsonPayloadError> {
    let start = Instant::now();

    let mut resp = client
        .get(format!("{}?q={}", BING_URL, query))
        .header("ocp-apim-subscription-key", BING_KEY)
        .send()
        .await
        .unwrap();

    let mut wrap = resp.json::<BingWrapper>().await?;
    wrap.web_pages.duration = start.elapsed();
    return Ok(wrap.web_pages);
}
