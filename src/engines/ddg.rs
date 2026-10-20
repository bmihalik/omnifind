use scraper::Html;
use serde_json::Value;
use url::Url;

use super::{checked_text, clean_text, selector};
use crate::{Error, ImageResult, Result, SearchResult};

#[derive(Clone)]
pub(crate) struct DuckDuckGo {
    client: primp::Client,
}

impl DuckDuckGo {
    pub(crate) fn new(client: primp::Client) -> Self {
        Self { client }
    }

    pub(crate) async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        if limit == 0 {
            return Ok(Vec::new());
        }
        let body = checked_text(
            self.client
                .post("https://html.duckduckgo.com/html/")
                .form(&[("q", query)]),
        )
        .await?;
        let document = Html::parse_document(&body);
        let item_selector = selector(".result");
        let link_selector = selector("a.result__a");
        let snippet_selector = selector(".result__snippet");

        Ok(document
            .select(&item_selector)
            .filter_map(|node| {
                let anchor = node.select(&link_selector).next()?;
                let href = anchor.value().attr("href")?;
                Some(SearchResult {
                    title: clean_text(&anchor.text().collect::<String>()),
                    url: unwrap_redirect(href),
                    snippet: node
                        .select(&snippet_selector)
                        .next()
                        .map(|value| clean_text(&value.text().collect::<String>()))
                        .unwrap_or_default(),
                    source: "duckduckgo".into(),
                })
            })
            .take(limit)
            .collect())
    }

    pub(crate) async fn images(&self, query: &str, limit: usize) -> Result<Vec<ImageResult>> {
        if limit == 0 {
            return Ok(Vec::new());
        }
        let landing = checked_text(
            self.client
                .get("https://duckduckgo.com/")
                .query(&[("q", query)]),
        )
        .await?;
        let token = extract_vqd(&landing).ok_or_else(|| Error::InvalidResponse {
            engine: "duckduckgo",
            message: "missing image-search token".into(),
        })?;
        let params = [
            ("l", "wt-wt"),
            ("o", "json"),
            ("q", query),
            ("vqd", token.as_str()),
            ("f", ",,,"),
            ("p", "1"),
        ];
        let body = checked_text(
            self.client
                .get("https://duckduckgo.com/i.js")
                .header("Referer", "https://duckduckgo.com/")
                .query(&params),
        )
        .await?;
        let value: Value = serde_json::from_str(&body)?;
        let rows = value
            .get("results")
            .and_then(Value::as_array)
            .ok_or_else(|| Error::InvalidResponse {
                engine: "duckduckgo",
                message: "missing image results".into(),
            })?;

        Ok(rows
            .iter()
            .map(|value| ImageResult {
                title: field(value, "title"),
                image_url: field(value, "image"),
                thumbnail_url: optional_field(value, "thumbnail"),
                page_url: optional_field(value, "url"),
                width: value
                    .get("width")
                    .and_then(Value::as_u64)
                    .and_then(|number| u32::try_from(number).ok()),
                height: value
                    .get("height")
                    .and_then(Value::as_u64)
                    .and_then(|number| u32::try_from(number).ok()),
                source: "duckduckgo".into(),
            })
            .filter(|result| !result.image_url.is_empty())
            .take(limit)
            .collect())
    }
}

fn field(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

fn optional_field(value: &Value, key: &str) -> Option<String> {
    let value = field(value, key);
    (!value.is_empty()).then_some(value)
}

fn extract_vqd(html: &str) -> Option<String> {
    ["vqd='", "vqd=\"", "vqd="].iter().find_map(|needle| {
        let start = html.find(needle)? + needle.len();
        let rest = &html[start..];
        let end = rest.find(['\'', '"', '&', ';'])?;
        (!rest[..end].is_empty()).then(|| rest[..end].to_owned())
    })
}

fn unwrap_redirect(href: &str) -> String {
    let parsed = Url::parse(href).or_else(|_| Url::parse(&format!("https://duckduckgo.com{href}")));
    parsed
        .ok()
        .and_then(|url| {
            url.query_pairs()
                .find(|(key, _)| key == "uddg")
                .map(|(_, value)| value.into_owned())
        })
        .unwrap_or_else(|| href.to_owned())
}

#[cfg(test)]
mod tests {
    #[test]
    fn unwraps_redirect() {
        let input = "//duckduckgo.com/l/?uddg=https%3A%2F%2Fexample.com%2Fa%3Fb%3D1";
        assert_eq!(super::unwrap_redirect(input), "https://example.com/a?b=1");
    }
}
