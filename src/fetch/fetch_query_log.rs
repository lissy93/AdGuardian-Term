use base64::{engine::general_purpose::STANDARD, Engine as _};
use reqwest::header::{HeaderValue, AUTHORIZATION, CONTENT_LENGTH};
use serde::Deserialize;

#[derive(Default, Deserialize)]
#[serde(default)]
pub struct QueryResponse {
  pub data: Vec<Query>,
}

#[derive(Default, Deserialize)]
#[serde(default)]
pub struct Query {
  pub cached: bool,
  pub client: String,
  pub client_info: Option<ClientInfo>,
  pub upstream: String,
  #[serde(rename = "elapsedMs")]
  pub elapsed_ms: String,
  pub question: Question,
  pub reason: String,
  pub time: String,
}

#[derive(Default, Deserialize)]
#[serde(default)]
pub struct ClientInfo {
  pub name: String,
}

#[derive(Default, Deserialize)]
#[serde(default)]
pub struct Question {
  pub class: String,
  pub name: String,
  #[serde(rename = "type")]
  pub question_type: String,
}

pub async fn fetch_adguard_query_log(
  client: &reqwest::Client,
  endpoint: &str,
  username: &str,
  password: &str,
  limit: u32,
) -> Result<QueryResponse, anyhow::Error> {
  let auth_string = format!("{}:{}", username, password);
  let auth_header_value = format!("Basic {}", STANDARD.encode(&auth_string));
  let mut headers = reqwest::header::HeaderMap::new();
  headers.insert(AUTHORIZATION, auth_header_value.parse()?);
  headers.insert(CONTENT_LENGTH, HeaderValue::from_static("0"));

  let url = format!("{}/control/querylog?limit={}", endpoint, limit);
  let response = client.get(&url).headers(headers).send().await?;
  if !response.status().is_success() {
    return Err(anyhow::anyhow!(
      "Request failed with status code {}",
      response.status()
    ));
  }

  let data = response.json().await?;
  Ok(data)
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::fetch::fetch_stats::StatsResponse;
  use crate::fetch::fetch_status::StatusResponse;

  // Missing or partial fields get decoded to defaults, instead of erroring
  #[test]
  fn empty_and_partial_json_decode_to_defaults() {
    serde_json::from_str::<QueryResponse>("{}").unwrap();
    serde_json::from_str::<StatsResponse>("{}").unwrap();
    serde_json::from_str::<StatusResponse>("{}").unwrap();
    serde_json::from_str::<StatsResponse>(r#"{"num_dns_queries":5}"#).unwrap();

    // A blocked query has no `upstream`, default to empty
    let q = r#"{"cached":false,"client":"1.2.3.4","elapsedMs":"0.1",
      "question":{"class":"IN","name":"x.com","type":"A"},"reason":"x","time":"t"}"#;
    assert_eq!(serde_json::from_str::<Query>(q).unwrap().upstream, "");
  }
}
