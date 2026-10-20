use serde::{Deserialize, Serialize};

/// A normalized web search result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchResult {
    pub title: String,
    pub url: String,
    pub snippet: String,
    pub source: String,
}

/// A normalized image search result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImageResult {
    pub title: String,
    pub image_url: String,
    pub thumbnail_url: Option<String>,
    pub page_url: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub source: String,
}

/// A normalized book search result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BookResult {
    pub title: String,
    pub authors: Vec<String>,
    pub first_publish_year: Option<i32>,
    pub edition_count: Option<u32>,
    pub key: Option<String>,
    pub cover_url: Option<String>,
    pub source: String,
}

/// Extracted content from a URL.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtractedPage {
    pub url: String,
    pub title: Option<String>,
    pub text: String,
    pub links: Vec<String>,
}

/// Supported search backends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Engine {
    DuckDuckGo,
    Qwant,
    Wikipedia,
}

impl Engine {
    pub const ALL: [Self; 3] = [Self::DuckDuckGo, Self::Qwant, Self::Wikipedia];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::DuckDuckGo => "duckduckgo",
            Self::Qwant => "qwant",
            Self::Wikipedia => "wikipedia",
        }
    }
}
