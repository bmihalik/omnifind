//! CLI example. Run with `cargo run --example cli -- --help`.
//! Concept and project direction: Béla István MIHALIK.
//! CLI code generation: Perplexity AI. See AUTHORS.md and the project licenses.

use std::collections::HashSet;
use std::error::Error;
use std::io::{self, Write};
use std::num::{NonZeroU64, NonZeroUsize};
use std::process::ExitCode;
use std::time::Duration;

use clap::{Args, Parser, Subcommand, ValueEnum};
use futures::future::join_all;
use omnifind::{BookResult, Engine, ExtractedPage, ImageResult, Omnifind, SearchResult};
use serde::Serialize;

#[derive(Debug, Parser)]
#[command(
    name = "omnifind-cli",
    version,
    about = "Search the web, images, and books, or extract a page using omnifind",
    after_help = "Quote multi-word queries. Network failures remain errors; meta reports partial failures on stderr."
)]
struct Cli {
    /// Emit pretty-printed JSON to stdout instead of human-readable text
    #[arg(long, global = true)]
    json: bool,

    /// Per-request timeout in seconds (must be positive)
    #[arg(long, global = true, default_value = "20", value_name = "SECONDS")]
    timeout: NonZeroU64,

    /// Explicit HTTP/SOCKS proxy URL (shell history/process listings may expose credentials)
    #[arg(long, global = true, value_name = "URL", value_parser = proxy_url)]
    proxy: Option<String>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Search one web engine
    Search {
        #[arg(short, long, value_enum, default_value = "duckduckgo")]
        engine: Backend,
        #[command(flatten)]
        query: Query,
    },
    /// Search multiple engines concurrently, deduplicate URLs, and report failures
    Meta {
        /// Comma-separated engines; may also be specified repeatedly
        #[arg(short, long, value_enum, value_delimiter = ',', default_value = "duckduckgo,qwant,wikipedia")]
        engines: Vec<Backend>,
        #[command(flatten)]
        query: Query,
    },
    /// Search DuckDuckGo Images
    Images(Query),
    /// Search Open Library books
    Books(Query),
    /// Extract a page's title, text, and links
    Extract {
        #[arg(value_name = "URL", value_parser = page_url)]
        url: String,
    },
}

#[derive(Debug, Args)]
struct Query {
    #[arg(value_name = "QUERY", value_parser = nonblank)]
    query: String,
    /// Maximum results requested (per engine for meta; service caps still apply)
    #[arg(short = 'n', long, default_value = "5", value_name = "COUNT")]
    limit: NonZeroUsize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum Backend {
    #[value(name = "duckduckgo", alias = "ddg")]
    DuckDuckGo,
    Qwant,
    #[value(alias = "wiki")]
    Wikipedia,
}

impl From<Backend> for Engine {
    fn from(value: Backend) -> Self {
        match value {
            Backend::DuckDuckGo => Self::DuckDuckGo,
            Backend::Qwant => Self::Qwant,
            Backend::Wikipedia => Self::Wikipedia,
        }
    }
}

fn nonblank(value: &str) -> Result<String, String> {
    if value.trim().is_empty() {
        Err("query must not be blank".into())
    } else {
        Ok(value.to_owned())
    }
}

fn validate_url(value: &str, schemes: &[&str]) -> Result<String, String> {
    let parsed = url::Url::parse(value).map_err(|_| "expected an absolute URL".to_string())?;
    if !schemes.contains(&parsed.scheme()) || parsed.host_str().is_none() {
        return Err(format!("expected an absolute {} URL with a host", schemes.join("/")));
    }
    Ok(value.to_owned())
}

fn page_url(value: &str) -> Result<String, String> {
    validate_url(value, &["http", "https"])
}

fn proxy_url(value: &str) -> Result<String, String> {
    validate_url(value, &["http", "https", "socks4", "socks4a", "socks5", "socks5h"])
}

#[derive(Debug)]
enum Output {
    Search(Vec<SearchResult>),
    Images(Vec<ImageResult>),
    Books(Vec<BookResult>),
    Page(ExtractedPage),
}

struct MetaReport {
    rows: Vec<SearchResult>,
    warnings: Vec<String>,
    successful_engines: usize,
}

// Do not use Omnifind::metasearch here: its current API omits engine failures.
// CLI users need to distinguish empty results from an unavailable service.
fn collect_meta(results: Vec<(Engine, omnifind::Result<Vec<SearchResult>>)>) -> MetaReport {
    let mut report = MetaReport {
        rows: Vec::new(),
        warnings: Vec::new(),
        successful_engines: 0,
    };
    let mut seen = HashSet::new();
    for (engine, result) in results {
        match result {
            Ok(rows) => {
                report.successful_engines += 1;
                for row in rows {
                    // Keep URL paths and queries case-sensitive; deduplicate exact URLs only.
                    if seen.insert(row.url.clone()) {
                        report.rows.push(row);
                    }
                }
            }
            Err(error) => report.warnings.push(format!("{}: {error}", engine.as_str())),
        }
    }
    report
}

async fn execute(cli: &Cli) -> Result<Output, Box<dyn Error>> {
    let mut builder = Omnifind::builder().timeout(Duration::from_secs(cli.timeout.get()));
    if let Some(proxy) = &cli.proxy {
        builder = builder.proxy(proxy.clone());
    }
    let client = builder.build()?;
    let output = match &cli.command {
        Command::Search { engine, query } => Output::Search(client.search((*engine).into(), &query.query, query.limit.get()).await?),
        Command::Meta { engines, query } => {
            let mut unique = Vec::new();
            for backend in engines {
                let engine: Engine = (*backend).into();
                if !unique.contains(&engine) {
                    unique.push(engine);
                }
            }
            let requests = unique.into_iter().map(|engine| {
                let client = &client;
                async move { (engine, client.search(engine, &query.query, query.limit.get()).await) }
            });
            let report = collect_meta(join_all(requests).await);
            {
                let mut stderr = io::stderr().lock();
                for warning in &report.warnings {
                    writeln!(stderr, "warning: {}", safe(warning))?;
                }
            }
            if report.successful_engines == 0 {
                return Err(io::Error::other("all selected search engines failed").into());
            }
            Output::Search(report.rows)
        }
        Command::Images(query) => Output::Images(client.images(&query.query, query.limit.get()).await?),
        Command::Books(query) => Output::Books(client.books(&query.query, query.limit.get()).await?),
        Command::Extract { url } => Output::Page(client.extract(url).await?),
    };
    Ok(output)
}

// Remote text must not inject terminal control sequences. JSON uses serde escaping.
fn safe(value: &str) -> String {
    value
        .chars()
        .flat_map(|ch| {
            if ch.is_control() || ('\u{202a}'..='\u{202e}').contains(&ch) || ('\u{2066}'..='\u{2069}').contains(&ch) {
                ch.escape_default().collect::<Vec<_>>()
            } else {
                vec![ch]
            }
        })
        .collect()
}

fn write_json(writer: &mut impl Write, value: &impl Serialize) -> io::Result<()> {
    serde_json::to_writer_pretty(&mut *writer, value).map_err(|error| io::Error::new(error.io_error_kind().unwrap_or(io::ErrorKind::InvalidData), error))?;
    writeln!(writer)
}

fn render(output: &Output, json: bool, writer: &mut impl Write) -> io::Result<()> {
    if json {
        return match output {
            Output::Search(rows) => write_json(writer, rows),
            Output::Images(rows) => write_json(writer, rows),
            Output::Books(rows) => write_json(writer, rows),
            Output::Page(page) => write_json(writer, page),
        };
    }
    match output {
        Output::Search(rows) => {
            if rows.is_empty() {
                writeln!(writer, "No results.")?;
            }
            for (index, row) in rows.iter().enumerate() {
                writeln!(writer, "{}. [{}] {}\n   {}\n   {}\n", index + 1, safe(&row.source), safe(&row.title), safe(&row.url), safe(&row.snippet))?;
            }
        }
        Output::Images(rows) => {
            if rows.is_empty() {
                writeln!(writer, "No results.")?;
            }
            for (index, row) in rows.iter().enumerate() {
                writeln!(writer, "{}. [{}] {}\n   Image: {}", index + 1, safe(&row.source), safe(&row.title), safe(&row.image_url))?;
                if let (Some(width), Some(height)) = (row.width, row.height) {
                    writeln!(writer, "   Size: {width} x {height}")?;
                }
                if let Some(url) = &row.thumbnail_url {
                    writeln!(writer, "   Thumbnail: {}", safe(url))?;
                }
                if let Some(url) = &row.page_url {
                    writeln!(writer, "   Page: {}", safe(url))?;
                }
                writeln!(writer)?;
            }
        }
        Output::Books(rows) => {
            if rows.is_empty() {
                writeln!(writer, "No results.")?;
            }
            for (index, row) in rows.iter().enumerate() {
                writeln!(writer, "{}. [{}] {}", index + 1, safe(&row.source), safe(&row.title))?;
                if !row.authors.is_empty() {
                    writeln!(writer, "   Authors: {}", safe(&row.authors.join(", ")))?;
                }
                if let Some(year) = row.first_publish_year {
                    writeln!(writer, "   First published: {year}")?;
                }
                if let Some(count) = row.edition_count {
                    writeln!(writer, "   Editions: {count}")?;
                }
                if let Some(key) = &row.key {
                    writeln!(writer, "   Key: {}", safe(key))?;
                }
                if let Some(cover) = &row.cover_url {
                    writeln!(writer, "   Cover: {}", safe(cover))?;
                }
                writeln!(writer)?;
            }
        }
        Output::Page(page) => {
            writeln!(writer, "Title: {}\nURL: {}\n\n{}\n\nLinks:", safe(page.title.as_deref().unwrap_or("(untitled)")), safe(&page.url), safe(&page.text))?;
            for link in &page.links {
                writeln!(writer, "  {}", safe(link))?;
            }
        }
    }
    Ok(())
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match execute(&cli).await {
        Ok(output) => render(&output, cli.json, &mut io::stdout().lock()).map_err(|error| Box::new(error) as Box<dyn Error>),
        Err(error) => Err(error),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) if error.downcast_ref::<io::Error>().is_some_and(|error| error.kind() == io::ErrorKind::BrokenPipe) => ExitCode::SUCCESS,
        Err(error) => {
            let _ = writeln!(io::stderr().lock(), "error: {}", safe(&error.to_string()));
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    fn row(url: &str) -> SearchResult {
        SearchResult {
            title: "Example".into(),
            url: url.into(),
            snippet: "Snippet".into(),
            source: "fixture".into(),
        }
    }

    #[test]
    fn command_definition_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn parses_search_with_global_flags_after_subcommand() {
        let cli = Cli::try_parse_from(["cli", "search", "-e", "wiki", "Rust language", "--limit", "3", "--json", "--timeout", "8"]).unwrap();
        assert!(cli.json);
        assert_eq!(cli.timeout.get(), 8);
        assert!(matches!(cli.command, Command::Search { engine: Backend::Wikipedia, query: Query { query, limit } } if query == "Rust language" && limit.get() == 3));
    }

    #[test]
    fn defaults_to_five_duckduckgo_results() {
        let cli = Cli::try_parse_from(["cli", "search", "Rust"]).unwrap();
        assert!(matches!(cli.command, Command::Search { engine: Backend::DuckDuckGo, query: Query { limit, .. } } if limit.get() == 5));
    }

    #[test]
    fn parses_comma_separated_and_repeated_engines() {
        let cli = Cli::try_parse_from(["cli", "meta", "Rust", "--engines", "ddg,wiki", "--engines", "qwant"]).unwrap();
        assert!(matches!(cli.command, Command::Meta { engines, .. } if engines == vec![Backend::DuckDuckGo, Backend::Wikipedia, Backend::Qwant]));
    }

    #[test]
    fn parses_all_default_meta_engines() {
        let cli = Cli::try_parse_from(["cli", "meta", "Rust"]).unwrap();
        assert!(matches!(cli.command, Command::Meta { engines, .. } if engines.len() == 3));
    }

    #[test]
    fn rejects_bad_arguments_before_network_access() {
        for args in [
            vec!["cli"],
            vec!["cli", "search"],
            vec!["cli", "search", "  "],
            vec!["cli", "books", "Rust", "--limit", "0"],
            vec!["cli", "images", "Rust", "--limit", "-1"],
            vec!["cli", "search", "Rust", "--timeout", "0"],
            vec!["cli", "search", "Rust", "--engine", "unknown"],
            vec!["cli", "extract", "file:///etc/passwd"],
            vec!["cli", "extract", "not-a-url"],
            vec!["cli", "extract", "https://example.com", "--limit", "2"],
            vec!["cli", "books", "Rust", "--proxy", "file:///tmp/proxy"],
        ] {
            assert!(Cli::try_parse_from(&args).is_err(), "accepted {args:?}");
        }
    }

    #[test]
    fn parses_remaining_commands_and_proxy() {
        assert!(matches!(Cli::try_parse_from(["cli", "images", "nebula"]).unwrap().command, Command::Images(_)));
        assert!(matches!(Cli::try_parse_from(["cli", "books", "The Hobbit"]).unwrap().command, Command::Books(_)));
        let cli = Cli::try_parse_from(["cli", "--proxy", "socks5h://localhost:1080", "extract", "https://example.com"]).unwrap();
        assert_eq!(cli.proxy.as_deref(), Some("socks5h://localhost:1080"));
        assert!(matches!(cli.command, Command::Extract { .. }));
    }

    #[test]
    fn meta_deduplicates_exact_urls_but_keeps_case_sensitive_paths() {
        let report = collect_meta(vec![
            (Engine::DuckDuckGo, Ok(vec![row("https://example.com/A")])),
            (Engine::Wikipedia, Ok(vec![row("https://example.com/A"), row("https://example.com/a")])),
        ]);
        assert_eq!(report.successful_engines, 2);
        assert_eq!(report.rows.len(), 2);
        assert!(report.warnings.is_empty());
    }

    #[test]
    fn meta_preserves_partial_failure_information() {
        let report = collect_meta(vec![
            (Engine::Wikipedia, Err(omnifind::Error::Configuration("fixture failure".into()))),
            (Engine::Qwant, Ok(vec![row("https://example.com")])),
        ]);
        assert_eq!(report.successful_engines, 1);
        assert_eq!(report.rows.len(), 1);
        assert!(report.warnings[0].contains("wikipedia"));
    }

    #[test]
    fn meta_distinguishes_all_failed_from_successful_empty_results() {
        let failed = collect_meta(vec![(Engine::Wikipedia, Err(omnifind::Error::Configuration("fixture failure".into())))]);
        assert_eq!(failed.successful_engines, 0);
        let empty = collect_meta(vec![(Engine::Wikipedia, Ok(Vec::new()))]);
        assert_eq!(empty.successful_engines, 1);
        assert!(empty.rows.is_empty());
    }

    #[test]
    fn json_preserves_the_library_result_schema() {
        let mut bytes = Vec::new();
        render(&Output::Search(vec![row("https://example.com")]), true, &mut bytes).unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json[0]["url"], "https://example.com");
        assert!(bytes.ends_with(b"\n"));
    }

    #[test]
    fn json_renders_every_output_variant() {
        for output in [
            Output::Images(Vec::new()),
            Output::Books(Vec::new()),
            Output::Page(ExtractedPage {
                url: "https://example.com".into(),
                title: None,
                text: String::new(),
                links: Vec::new(),
            }),
        ] {
            let mut bytes = Vec::new();
            render(&output, true, &mut bytes).unwrap();
            assert!(serde_json::from_slice::<serde_json::Value>(&bytes).is_ok());
        }
    }

    #[test]
    fn text_escapes_terminal_controls() {
        assert_eq!(safe("x\u{1b}[31m\n\t\u{202e}é"), "x\\u{1b}[31m\\n\\t\\u{202e}é");
        let mut item = row("https://example.com");
        item.title = "title\u{1b}[2J".into();
        let mut bytes = Vec::new();
        render(&Output::Search(vec![item]), false, &mut bytes).unwrap();
        assert!(!bytes.contains(&0x1b));
    }

    #[test]
    fn empty_results_are_reported_in_text_and_json() {
        let mut text = Vec::new();
        render(&Output::Books(Vec::new()), false, &mut text).unwrap();
        assert_eq!(text, b"No results.\n");
        let mut json = Vec::new();
        render(&Output::Search(Vec::new()), true, &mut json).unwrap();
        assert_eq!(json, b"[]\n");
    }

    #[test]
    fn output_errors_are_propagated() {
        struct Closed;
        impl Write for Closed {
            fn write(&mut self, _: &[u8]) -> io::Result<usize> {
                Err(io::ErrorKind::BrokenPipe.into())
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let error = render(&Output::Search(Vec::new()), true, &mut Closed).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::BrokenPipe);
    }
}
