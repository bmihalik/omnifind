use omnifind::{Engine, Omnifind, Result};

#[tokio::main]
async fn main() -> Result<()> {
    let client = Omnifind::new()?;
    let results = client
        .metasearch(
            "Rust async programming",
            &[Engine::DuckDuckGo, Engine::Wikipedia],
            5,
        )
        .await?;

    for item in results {
        println!("[{}] {}\n{}\n", item.source, item.title, item.url);
    }
    Ok(())
}
