use omnifind::{Engine, Omnifind};

#[tokio::test]
#[ignore = "live network test"]
async fn wikipedia_search_works() {
    let client = Omnifind::new().unwrap();
    let rows = client
        .search(Engine::Wikipedia, "Rust programming language", 3)
        .await
        .unwrap();
    assert!(!rows.is_empty());
}

#[tokio::test]
#[ignore = "live network test"]
async fn books_work() {
    let client = Omnifind::new().unwrap();
    let rows = client.books("The Hobbit", 3).await.unwrap();
    assert!(!rows.is_empty());
}
