mod ddg;
mod marginalia;
mod open_library;
mod qwant;
mod wikipedia;

pub(crate) use ddg::DuckDuckGo;
pub(crate) use marginalia::Marginalia;
pub(crate) use open_library::OpenLibrary;
pub(crate) use qwant::Qwant;
pub(crate) use wikipedia::Wikipedia;

use crate::{Error, Result};

async fn checked_response(request: primp::RequestBuilder) -> Result<primp::Response> {
    let response = request.send().await?;
    if !response.status().is_success() {
        return Err(Error::Status {
            status: response.status().as_u16(),
            url: response.url().to_string(),
        });
    }
    Ok(response)
}

pub(crate) async fn checked_text(request: primp::RequestBuilder) -> Result<String> {
    Ok(checked_response(request).await?.text().await?)
}

/// Decode a JSON response without exposing response bodies in errors.
pub(crate) async fn checked_json<T: serde::de::DeserializeOwned>(
    engine: &'static str,
    request: primp::RequestBuilder,
) -> Result<T> {
    let response = checked_response(request.header("Accept", "application/json")).await?;
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("unspecified")
        .to_owned();
    let content_encoding = response
        .headers()
        .get("content-encoding")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("identity/decoded")
        .to_owned();
    let body = response.text().await?;
    serde_json::from_str(&body).map_err(|error| Error::InvalidResponse {
        engine,
        message: format!("expected JSON (content-type: {content_type}, content-encoding: {content_encoding}): {error}"),
    })
}

pub(crate) fn selector(value: &str) -> scraper::Selector {
    scraper::Selector::parse(value).expect("static CSS selector must be valid")
}

pub(crate) fn clean_text(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests;
