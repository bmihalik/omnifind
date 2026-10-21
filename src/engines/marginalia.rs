use primp::header::HeaderValue;
use serde::Deserialize;

use super::checked_json;
use crate::{Error, MarginaliaResponse, Result, SearchResult};

const ENDPOINT: &str = "https://api2.marginalia-search.com/search";
const MAX_RESULTS: usize = 100;

#[derive(Clone)]
pub(crate) struct Marginalia {
    // A dedicated client with redirects disabled prevents API-Key forwarding.
    client: primp::Client,
    api_key: HeaderValue,
}

impl Marginalia {
    pub(crate) fn new(client: primp::Client, api_key: HeaderValue) -> Self {
        Self { client, api_key }
    }

    pub(crate) async fn search(&self, query: &str, limit: usize) -> Result<MarginaliaResponse> {
        if limit == 0 {
            // No upstream response exists, so do not invent license metadata.
            return Ok(MarginaliaResponse {
                query: query.into(),
                license: None,
                results: Vec::new(),
            });
        }
        self.search_at(ENDPOINT, query, limit).await
    }

    pub(crate) async fn search_at(&self, endpoint: &str, query: &str, limit: usize) -> Result<MarginaliaResponse> {
        let count = limit.min(MAX_RESULTS);
        let count_string = count.to_string();
        let parsed: ApiResponse = checked_json(
            "marginalia",
            self.client.get(endpoint).header("API-Key", self.api_key.clone()).query(&[("query", query), ("count", count_string.as_str())]),
        )
        .await?;
        if parsed.license.trim().is_empty() {
            return Err(Error::InvalidResponse {
                engine: "marginalia",
                message: "missing nonempty license metadata".into(),
            });
        }
        // Ignore extra API fields, but reject malformed rows instead of claiming
        // a successful empty response. Never return unsafe/non-web result URLs.
        let mut results = Vec::new();
        for row in parsed.results {
            let Ok(url) = url::Url::parse(&row.url) else {
                continue;
            };
            if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() || !url.username().is_empty() || url.password().is_some() {
                continue;
            }
            let title = row.title.filter(|title| !title.trim().is_empty()).unwrap_or_else(|| row.url.clone());
            results.push(SearchResult {
                title,
                url: row.url,
                snippet: row.description.unwrap_or_default(),
                source: "marginalia".into(),
            });
            if results.len() == count {
                break;
            }
        }
        Ok(MarginaliaResponse {
            query: parsed.query,
            license: Some(parsed.license),
            results,
        })
    }
}

#[derive(Deserialize)]
struct ApiResponse {
    query: String,
    license: String,
    results: Vec<Row>,
}

#[derive(Deserialize)]
struct Row {
    url: String,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    description: Option<String>,
}

#[cfg(test)]
mod tests;
