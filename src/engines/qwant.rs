use serde_json::Value;

use super::checked_text;
use crate::{Error, Result, SearchResult};

#[derive(Clone)]
pub(crate) struct Qwant {
    client: primp::Client,
}

impl Qwant {
    pub(crate) fn new(client: primp::Client) -> Self {
        Self { client }
    }

    pub(crate) async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        if limit == 0 {
            return Ok(Vec::new());
        }
        let count = limit.min(10).to_string();
        let params = [
            ("q", query),
            ("count", count.as_str()),
            ("safesearch", "1"),
            ("locale", "en_US"),
            ("offset", "0"),
            ("device", "desktop"),
        ];
        let body = checked_text(
            self.client
                .get("https://api.qwant.com/v3/search/web")
                .header("Referer", "https://www.qwant.com/")
                .query(&params),
        )
        .await?;
        let value: Value = serde_json::from_str(&body)?;
        let rows = value
            .pointer("/data/result/items/mainline/0/items")
            .or_else(|| value.pointer("/data/result/items"))
            .and_then(Value::as_array)
            .ok_or_else(|| Error::InvalidResponse {
                source: "qwant",
                message: "missing result items".into(),
            })?;

        Ok(rows
            .iter()
            .filter_map(|value| {
                let url = text(value, "url");
                if url.is_empty() {
                    return None;
                }
                Some(SearchResult {
                    title: text(value, "title"),
                    url,
                    snippet: text(value, "desc"),
                    source: "qwant".into(),
                })
            })
            .take(limit)
            .collect())
    }
}

fn text(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}
