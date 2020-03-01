use std::fmt;

use actix_web::{client::Client, error::ResponseError};
use awc::error::{JsonPayloadError, SendRequestError};
use serde::Deserialize;
use url::Url;

#[derive(Debug, Deserialize)]
pub struct Document {
    #[serde(alias = "url")]
    pub link: String,
    #[serde(alias = "name")]
    pub title: String,
    pub snippet: String,
}

#[derive(Debug, Deserialize)]
struct Wrapper {
    #[serde(alias = "value")]
    pub items: Vec<Document>,
}

#[derive(Deserialize)]
struct Wrapper2 {
    #[serde(rename = "webPages")]
    web_pages: Wrapper,
}

#[derive(Debug)]
pub enum Error {
    SendRequest(SendRequestError),
    JsonPayload(JsonPayloadError),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl ResponseError for Error {}

impl From<SendRequestError> for Error {
    fn from(error: SendRequestError) -> Self {
        Error::SendRequest(error)
    }
}

impl From<JsonPayloadError> for Error {
    fn from(error: JsonPayloadError) -> Self {
        Error::JsonPayload(error)
    }
}

const GOOGLE_URL: &'static str = "https://www.googleapis.com/customsearch/v1";
const GOOGLE_KEY: &'static str = "AIzaSyCKZXnEw9whbifuySpwncm584Op59e7Z5U";
const GOOGLE_CX: &'static str = "002842975906381566051:zico5aaukxe";

pub async fn search_google(
    client: &Client,
    query: &str,
) -> Result<Vec<Document>, Error> {
    let url = Url::parse_with_params(
        GOOGLE_URL,
        &[("key", GOOGLE_KEY), ("cx", GOOGLE_CX), ("q", query)],
    )
    .unwrap();

    let mut resp = client.get(url.as_str()).send().await?;

    let wrap = resp.json::<Wrapper>().await?;

    return Ok(wrap.items);
}

const BING_URL: &'static str =
    "https://api.cognitive.microsoft.com/bing/v7.0/search";
const BING_KEY: &'static str = "8442e9e17ece414bac0f45d3dce2264d";

pub async fn search_bing(
    client: &Client,
    query: &str,
) -> Result<Vec<Document>, Error> {
    let url = Url::parse_with_params(BING_URL, &[("q", query)]).unwrap();

    let mut resp = client
        .get(url.as_str())
        .header("ocp-apim-subscription-key", BING_KEY)
        .send()
        .await?;

    let wrap = resp.json::<Wrapper2>().await?;

    return Ok(wrap.web_pages.items);
}

// Need this to workaround the lack of async trait support.
pub enum Engine {
    Google,
    Bing,
}

pub async fn search(
    client: &Client,
    engine: Engine,
    query: &str,
) -> Result<Vec<Document>, Error> {
    match engine {
        Engine::Google => search_google(client, query).await,
        Engine::Bing => search_bing(client, query).await,
    }
}
