mod ddg;
mod open_library;
mod qwant;
mod wikipedia;

pub(crate) use ddg::DuckDuckGo;
pub(crate) use open_library::OpenLibrary;
pub(crate) use qwant::Qwant;
pub(crate) use wikipedia::Wikipedia;

use crate::{Error, Result};

pub(crate) async fn checked_text(request: primp::RequestBuilder) -> Result<String> {
    let response = request.send().await?;
    let status = response.status();
    let url = response.url().to_string();
    if !status.is_success() {
        return Err(Error::Status {
            status: status.as_u16(),
            url,
        });
    }
    Ok(response.text().await?)
}

pub(crate) fn selector(value: &str) -> scraper::Selector {
    scraper::Selector::parse(value).expect("static CSS selector must be valid")
}

pub(crate) fn clean_text(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}
