use serde::Deserialize;

use super::checked_json;
use crate::{BookResult, Result};

#[derive(Clone)]
pub(crate) struct OpenLibrary {
    client: primp::Client,
}

impl OpenLibrary {
    pub(crate) fn new(client: primp::Client) -> Self {
        Self { client }
    }

    pub(crate) async fn search(&self, query: &str, limit: usize) -> Result<Vec<BookResult>> {
        if limit == 0 {
            return Ok(Vec::new());
        }
        let limit_string = limit.min(100).to_string();
        let params = [
            ("q", query),
            ("limit", limit_string.as_str()),
            (
                "fields",
                "key,title,author_name,first_publish_year,edition_count,cover_i",
            ),
        ];
        let parsed: ApiResponse = checked_json(
            "open_library",
            self.client
                .get("https://openlibrary.org/search.json")
                .query(&params),
        )
        .await?;

        Ok(parsed
            .docs
            .into_iter()
            .take(limit)
            .map(|row| BookResult {
                title: row.title,
                authors: row.author_name.unwrap_or_default(),
                first_publish_year: row.first_publish_year,
                edition_count: row.edition_count,
                key: row.key,
                cover_url: row
                    .cover_i
                    .map(|id| format!("https://covers.openlibrary.org/b/id/{id}-M.jpg")),
                source: "open_library".into(),
            })
            .collect())
    }
}

#[derive(Deserialize)]
struct ApiResponse {
    docs: Vec<Row>,
}

#[derive(Deserialize)]
struct Row {
    title: String,
    author_name: Option<Vec<String>>,
    first_publish_year: Option<i32>,
    edition_count: Option<u32>,
    key: Option<String>,
    cover_i: Option<i64>,
}
