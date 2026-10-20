use serde::Deserialize;

use super::checked_json;
use crate::{Error, Result, SearchResult};

#[derive(Clone)]
pub(crate) struct Wikipedia {
    client: primp::Client,
}

impl Wikipedia {
    pub(crate) fn new(client: primp::Client) -> Self {
        Self { client }
    }

    pub(crate) async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        if limit == 0 {
            return Ok(Vec::new());
        }
        let endpoint = "https://en.wikipedia.org/w/api.php";
        let limit_string = limit.min(50).to_string();
        let params = [
            ("action", "query"),
            ("list", "search"),
            ("srsearch", query),
            ("srlimit", limit_string.as_str()),
            ("format", "json"),
            ("utf8", "1"),
        ];
        let parsed: ApiResponse =
            checked_json("wikipedia", self.client.get(endpoint).query(&params)).await?;
        let rows = parsed
            .query
            .ok_or_else(|| Error::InvalidResponse {
                engine: "wikipedia",
                message: "missing query object".into(),
            })?
            .search;

        Ok(rows
            .into_iter()
            .take(limit)
            .map(|row| SearchResult {
                url: format!("https://en.wikipedia.org/wiki/{}", encode_title(&row.title)),
                title: row.title,
                snippet: strip_tags(&row.snippet),
                source: "wikipedia".into(),
            })
            .collect())
    }
}

#[derive(Deserialize)]
struct ApiResponse {
    query: Option<Query>,
}

#[derive(Deserialize)]
struct Query {
    search: Vec<Row>,
}

#[derive(Deserialize)]
struct Row {
    title: String,
    snippet: String,
}

fn encode_title(title: &str) -> String {
    let replaced = title.replace(' ', "_");
    url::form_urlencoded::byte_serialize(replaced.as_bytes()).collect()
}

fn strip_tags(html: &str) -> String {
    scraper::Html::parse_fragment(html)
        .root_element()
        .text()
        .collect::<Vec<_>>()
        .join("")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}
