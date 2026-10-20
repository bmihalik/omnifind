use omnifind::{Engine, Omnifind, Result};

#[tokio::test]
#[ignore = "live network test"]
async fn wikipedia_search_works() -> Result<()> {
    let client = Omnifind::new()?;
    let rows = client
        .search(Engine::Wikipedia, "Rust programming language", 3)
        .await?;
    assert!(!rows.is_empty(), "Wikipedia returned no search results");
    Ok(())
}

#[tokio::test]
#[ignore = "live network test"]
async fn books_work() -> Result<()> {
    let client = Omnifind::new()?;
    let rows = client.books("The Hobbit", 3).await?;
    assert!(!rows.is_empty(), "Open Library returned no book results");
    Ok(())
}
