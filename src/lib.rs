//! `omnifind` is an asynchronous Rust metasearch and web-content extraction crate.
//!
//! It normalizes results from DuckDuckGo, Qwant, Wikipedia, and Open Library,
//! while using a shared browser-impersonating [`primp::Client`].

mod client;
mod engines;
mod error;
mod types;

pub use client::{Omnifind, OmnifindBuilder};
pub use error::{Error, Result};
pub use types::{BookResult, Engine, ExtractedPage, ImageResult, SearchResult};
