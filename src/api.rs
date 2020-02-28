use actix_web::client::Client;
use awc::error::JsonPayloadError; // Incompatible with the one in actix_web.
use serde::Deserialize;

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

const GOOGLE_URL: &'static str = "https://www.googleapis.com/customsearch/v1";
const GOOGLE_KEY: &'static str = "AIzaSyCKZXnEw9whbifuySpwncm584Op59e7Z5U";
const GOOGLE_CX: &'static str = "002842975906381566051:zico5aaukxe";

pub async fn search_google(
    client: &Client,
    query: &str,
) -> Result<Vec<Document>, JsonPayloadError> {
    let mut resp = client
        .get(format!(
            "{}?key={}&cx={}&q={}",
            GOOGLE_URL, GOOGLE_KEY, GOOGLE_CX, query
        ))
        .send()
        .await
        .unwrap();

    let wrap = resp.json::<Wrapper>().await?;

    return Ok(wrap.items);
}

const BING_URL: &'static str =
    "https://api.cognitive.microsoft.com/bing/v7.0/search";
const BING_KEY: &'static str = "8442e9e17ece414bac0f45d3dce2264d";

pub async fn search_bing(
    client: &Client,
    query: &str,
) -> Result<Vec<Document>, JsonPayloadError> {
    let mut resp = client
        .get(format!("{}?q={}", BING_URL, query))
        .header("ocp-apim-subscription-key", BING_KEY)
        .send()
        .await
        .unwrap();

    let wrap = resp.json::<Wrapper2>().await?;

    return Ok(wrap.web_pages.items);
}

// Need this to workaround the lack of async trait support.
pub enum Engine {
    Google,
    Bing,
}

pub async fn search(
    engine: Engine,
    client: &Client,
    query: &str,
) -> Result<Vec<Document>, JsonPayloadError> {
    match engine {
        Engine::Google => search_google(client, query).await,
        Engine::Bing => search_bing(client, query).await,
    }
}
