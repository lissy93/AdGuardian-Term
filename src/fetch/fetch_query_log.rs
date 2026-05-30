use reqwest::{
  header::{HeaderValue, CONTENT_LENGTH, AUTHORIZATION},
};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct QueryResponse {
    pub data: Vec<Query>,
}

#[derive(Deserialize)]
pub struct Query {
    pub cached: bool,
    pub client: String,
    pub upstream: String,
    #[serde(rename = "elapsedMs")]
    pub elapsed_ms: String,
    pub question: Question,
    #[serde(default)]
    pub client_info: Option<ClientInfo>,
    pub reason: String,
    pub time: String,
}

#[derive(Deserialize)]
pub struct Question {
    pub class: String,
    pub name: String,
    #[serde(rename = "type")]
    pub question_type: String,
}

#[derive(Deserialize)]
pub struct ClientInfo {
    pub name: Option<String>,
}

pub async fn fetch_adguard_query_log(
  client: &reqwest::Client,
  endpoint: &str,
  username: &str,
  password: &str,
) -> Result<QueryResponse, anyhow::Error> {
  let auth_string = format!("{}:{}", username, password);
  let auth_header_value = format!("Basic {}", base64::encode(&auth_string));
  let mut headers = reqwest::header::HeaderMap::new();
  headers.insert(AUTHORIZATION, auth_header_value.parse()?);
  headers.insert(CONTENT_LENGTH, HeaderValue::from_static("0"));

  let url = format!("{}/control/querylog", endpoint);
  let response = client.get(&url).headers(headers).send().await?;
  if !response.status().is_success() {
      return Err(anyhow::anyhow!("Request failed with status code {}", response.status()));
  }

  let data = response.json().await?;
  Ok(data)
}

