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
