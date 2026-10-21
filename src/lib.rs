//! `omnifind` is an asynchronous Rust metasearch and web-content extraction crate.
//!
//! It normalizes results from DuckDuckGo, Qwant, Wikipedia, Marginalia, and Open Library,
//! using browser-impersonating [`primp::Client`] instances. Marginalia uses a
//! separate no-redirect client to keep its credential header scoped to the API.

mod client;
mod engines;
mod error;
mod types;

pub use client::{Omnifind, OmnifindBuilder};
pub use error::{Error, Result};
pub use types::{BookResult, Engine, ExtractedPage, ImageResult, MarginaliaResponse, SearchResult};
