//! Minimal Marginalia CLI: cargo run --example marginalia -- "small web".
use omnifind::{Omnifind, Result};

#[tokio::main]
async fn main() -> Result<()> {
    let query = std::env::args().skip(1).collect::<Vec<_>>().join(" ");
    let query = if query.is_empty() {
        "small web"
    } else {
        &query
    };
    let mut builder = Omnifind::builder();
    if let Ok(key) = std::env::var("MARGINALIA_API_KEY") {
        builder = builder.marginalia_api_key(key);
    }
    // JSON includes the upstream license notice, including an empty result set.
    let response = builder.build()?.marginalia(query, 5).await?;
    println!("{}", serde_json::to_string_pretty(&response)?);
    Ok(())
}
