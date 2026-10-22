use std::error::Error as StdError;
use std::time::Duration;

use omnifind::{Error, Omnifind};

#[tokio::test]
async fn client_builder_returns_a_client() {
    let result = Omnifind::builder().build();
    assert!(result.is_ok(), "default client should build successfully");
}

#[test]
fn zero_timeout_returns_a_configuration_error() {
    assert!(matches!(
        Omnifind::builder().timeout(Duration::ZERO).build(),
        Err(Error::Configuration(_))
    ));
}

#[test]
fn invalid_response_engine_is_context_not_an_error_source() {
    let error = Error::InvalidResponse {
        engine: "wikipedia",
        message: "missing query object".into(),
    };
    assert_eq!(
        error.to_string(),
        "invalid response from wikipedia: missing query object"
    );
    assert!(StdError::source(&error).is_none());
}

#[test]
fn http_errors_convert_into_crate_errors() {
    let http_error = primp::Proxy::all("http://[").expect_err("invalid proxy must fail");
    let error: Error = http_error.into();
    assert!(matches!(&error, Error::Http(_)));
    assert!(StdError::source(&error).is_some());
}

#[test]
fn marginalia_is_registered_and_serializes_by_name() {
    use omnifind::Engine;
    assert!(Engine::ALL.contains(&Engine::Marginalia));
    assert_eq!(Engine::Marginalia.as_str(), "marginalia");
    assert_eq!(
        serde_json::to_string(&Engine::Marginalia).unwrap(),
        "\"marginalia\""
    );
    assert_eq!(
        serde_json::from_str::<Engine>("\"marginalia\"").unwrap(),
        Engine::Marginalia
    );
}

#[test]
fn marginalia_keys_are_validated_without_echoing_credentials() {
    for key in ["", " ", "secret\nheader", "non-ascii-é"] {
        let error = Omnifind::builder()
            .marginalia_api_key(key)
            .build()
            .err()
            .unwrap();
        assert!(matches!(error, Error::Configuration(_)));
        assert!(!error.to_string().contains("secret"));
    }
    let builder = Omnifind::builder()
        .marginalia_api_key("private-api-key")
        .proxy("http://user:private-proxy-password@example.com:8080");
    let debug = format!("{builder:?}");
    assert!(!debug.contains("private-api-key"));
    assert!(!debug.contains("private-proxy-password"));
}

#[tokio::test]
async fn marginalia_zero_limit_never_needs_network_and_empty_queries_fail() {
    use omnifind::Engine;
    let client = Omnifind::new().unwrap();
    let response = client.marginalia("Rust", 0).await.unwrap();
    assert_eq!(response.query, "Rust");
    assert!(response.results.is_empty());
    assert!(response.license.is_none());
    assert!(
        client
            .search(Engine::Marginalia, "Rust", 0)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(matches!(
        client.marginalia(" ", 0).await,
        Err(Error::Configuration(_))
    ));
    assert!(matches!(
        client.search(Engine::Marginalia, " ", 1).await,
        Err(Error::Configuration(_))
    ));
}
