use std::{fmt, time::Duration};

use primp::header::HeaderValue;

use futures::future::join_all;
use scraper::Html;

use crate::engines::{self, DuckDuckGo, Marginalia, OpenLibrary, Qwant, Wikipedia};
use crate::{
    BookResult, Engine, Error, ExtractedPage, ImageResult, MarginaliaResponse, Result, SearchResult,
};

/// Builder for [`Omnifind`].
#[derive(Clone)]
pub struct OmnifindBuilder {
    timeout: Duration,
    proxy: Option<String>,
    impersonate: primp::Impersonate,
    marginalia_api_key: Option<String>,
}

impl fmt::Debug for OmnifindBuilder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OmnifindBuilder")
            .field("timeout", &self.timeout)
            .field("proxy", &self.proxy.as_ref().map(|_| "[REDACTED]"))
            .field("impersonate", &self.impersonate)
            .field(
                "marginalia_api_key",
                &self.marginalia_api_key.as_ref().map(|_| "[REDACTED]"),
            )
            .finish()
    }
}

impl Default for OmnifindBuilder {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(20),
            proxy: None,
            impersonate: primp::Impersonate::ChromeV146,
            marginalia_api_key: None,
        }
    }
}

impl OmnifindBuilder {
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn proxy(mut self, proxy: impl Into<String>) -> Self {
        self.proxy = Some(proxy.into());
        self
    }

    pub fn impersonate(mut self, browser: primp::Impersonate) -> Self {
        self.impersonate = browser;
        self
    }

    /// Set a Marginalia API key. Defaults to the shared `public` test key.
    ///
    /// The library does not read environment variables. Pass a personal key here
    /// for regular use. The key is sent only to the Marginalia API, in a header.
    pub fn marginalia_api_key(mut self, api_key: impl Into<String>) -> Self {
        self.marginalia_api_key = Some(api_key.into());
        self
    }

    pub fn build(self) -> Result<Omnifind> {
        if self.timeout.is_zero() {
            return Err(Error::Configuration(
                "timeout must be greater than zero".into(),
            ));
        }

        let key = self.marginalia_api_key.as_deref().unwrap_or("public");
        if key.is_empty() || !key.bytes().all(|byte| byte.is_ascii_graphic()) {
            return Err(Error::Configuration(
                "Marginalia API key must contain only nonempty visible ASCII without whitespace"
                    .into(),
            ));
        }
        let mut api_key = HeaderValue::from_str(key)
            .map_err(|_| Error::Configuration("invalid Marginalia API key header".into()))?;
        api_key.set_sensitive(true);

        let make_builder = || -> Result<primp::ClientBuilder> {
            let mut builder = primp::Client::builder()
                .impersonate(self.impersonate)
                .timeout(self.timeout)
                .cookie_store(true);
            if let Some(proxy) = &self.proxy {
                builder = builder.proxy(primp::Proxy::all(proxy)?);
            }
            Ok(builder)
        };
        let client = make_builder()?.build()?;
        let marginalia_client = make_builder()?
            .redirect(primp::redirect::Policy::none())
            .build()?;
        Ok(Omnifind {
            client,
            marginalia: Marginalia::new(marginalia_client, api_key),
        })
    }
}

/// Asynchronous metasearch and extraction client.
#[derive(Clone)]
pub struct Omnifind {
    client: primp::Client,
    marginalia: Marginalia,
}

impl Omnifind {
    pub fn builder() -> OmnifindBuilder {
        OmnifindBuilder::default()
    }

    pub fn new() -> Result<Self> {
        Self::builder().build()
    }

    /// Search one engine. For Marginalia license metadata use [`Self::marginalia`].
    ///
    /// Marginalia results remain subject to its API terms; this generic method
    /// returns only normalized rows, not the upstream response-level license.
    pub async fn search(
        &self,
        engine: Engine,
        query: &str,
        limit: usize,
    ) -> Result<Vec<SearchResult>> {
        validate_query(query)?;
        match engine {
            Engine::DuckDuckGo => {
                DuckDuckGo::new(self.client.clone())
                    .search(query, limit)
                    .await
            }
            Engine::Marginalia => Ok(self.marginalia(query, limit).await?.results),
            Engine::Qwant => Qwant::new(self.client.clone()).search(query, limit).await,
            Engine::Wikipedia => {
                Wikipedia::new(self.client.clone())
                    .search(query, limit)
                    .await
            }
        }
    }

    /// Query Marginalia and preserve the upstream response-level license notice.
    ///
    /// Limits over 100 are clamped to 100. Zero avoids any network request.
    /// The shared public key is intended only for experimentation; configure a
    /// personal/commercial key as appropriate for your usage.
    pub async fn marginalia(&self, query: &str, limit: usize) -> Result<MarginaliaResponse> {
        validate_query(query)?;
        self.marginalia.search(query, limit).await
    }

    /// Queries several engines concurrently. Failed engines are omitted.
    ///
    /// This includes Marginalia failures; use `marginalia()` separately if you
    /// need its license metadata or reliable per-engine failure reporting.
    pub async fn metasearch(
        &self,
        query: &str,
        engines: &[Engine],
        per_engine: usize,
    ) -> Result<Vec<SearchResult>> {
        validate_query(query)?;
        let tasks = engines
            .iter()
            .copied()
            .map(|engine| async move { self.search(engine, query, per_engine).await });
        let mut output = Vec::new();
        for mut rows in join_all(tasks).await.into_iter().flatten() {
            output.append(&mut rows);
        }
        deduplicate(&mut output);
        Ok(output)
    }

    pub async fn images(&self, query: &str, limit: usize) -> Result<Vec<ImageResult>> {
        validate_query(query)?;
        DuckDuckGo::new(self.client.clone())
            .images(query, limit)
            .await
    }

    pub async fn books(&self, query: &str, limit: usize) -> Result<Vec<BookResult>> {
        validate_query(query)?;
        OpenLibrary::new(self.client.clone())
            .search(query, limit)
            .await
    }

    pub async fn extract(&self, url: &str) -> Result<ExtractedPage> {
        let parsed = url::Url::parse(url)?;
        if !matches!(parsed.scheme(), "http" | "https") {
            return Err(Error::Configuration(
                "only http and https URLs are supported".into(),
            ));
        }

        let body = engines::checked_text(self.client.get(parsed.as_str())).await?;
        let document = Html::parse_document(&body);
        let title_selector = engines::selector("title");
        let content_selector = engines::selector("main, article, body");
        let link_selector = engines::selector("a[href]");

        let title = document
            .select(&title_selector)
            .next()
            .map(|node| engines::clean_text(&node.text().collect::<String>()))
            .filter(|value| !value.is_empty());
        let text = document
            .select(&content_selector)
            .next()
            .map(|node| engines::clean_text(&node.text().collect::<String>()))
            .unwrap_or_default();
        let links = document
            .select(&link_selector)
            .filter_map(|node| node.value().attr("href"))
            .filter_map(|href| parsed.join(href).ok())
            .filter(|link| matches!(link.scheme(), "http" | "https"))
            .map(|link| link.to_string())
            .collect();

        Ok(ExtractedPage {
            url: parsed.to_string(),
            title,
            text,
            links,
        })
    }
}

fn validate_query(query: &str) -> Result<()> {
    if query.trim().is_empty() {
        Err(Error::Configuration("query must not be empty".into()))
    } else {
        Ok(())
    }
}

fn deduplicate(rows: &mut Vec<SearchResult>) {
    let mut seen = std::collections::HashSet::new();
    rows.retain(|row| seen.insert(row.url.trim_end_matches('/').to_ascii_lowercase()));
}
