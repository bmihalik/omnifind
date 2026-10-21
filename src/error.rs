use thiserror::Error;

/// Errors returned by omnifind.
#[derive(Debug, Error)]
pub enum Error {
    #[error("HTTP request failed: {0}")]
    Http(#[from] primp::Error),
    #[error("remote service returned HTTP {status} for {url}")]
    Status { status: u16, url: String },
    #[error("invalid response from {engine}: {message}")]
    InvalidResponse { engine: &'static str, message: String },
    #[error("URL error: {0}")]
    Url(#[from] url::ParseError),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("configuration error: {0}")]
    Configuration(String),
}

pub type Result<T> = std::result::Result<T, Error>;
