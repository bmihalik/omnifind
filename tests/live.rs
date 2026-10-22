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

#[tokio::test]
#[ignore = "live network test; public Marginalia key is shared and rate limited"]
async fn marginalia_search_works() -> Result<()> {
    let mut builder = Omnifind::builder();
    if let Ok(key) = std::env::var("MARGINALIA_API_KEY") {
        builder = builder.marginalia_api_key(key);
        let response = builder.build()?.marginalia("small web", 3).await?;
        assert!(
            response.license.is_some(),
            "Marginalia omitted its license notice"
        );
        assert!(
            !response.results.is_empty(),
            "Marginalia returned no results"
        );
    } else {
        eprint!("Skipping: no API has been provided! ... ");
    }
    Ok(())
}
