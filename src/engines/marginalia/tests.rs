use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use super::*;

// Synthetic local fixtures, not recorded upstream results.
fn response(rows: Value) -> String {
    json!({"query":"small web", "license":"fixture-license", "results":rows}).to_string()
}

fn serve(status: &str, headers: &str, body: String) -> (String, thread::JoinHandle<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let address = listener.local_addr().unwrap();
    let status = status.to_owned();
    let headers = headers.to_owned();
    let task = thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut stream = loop {
            match listener.accept() {
                Ok((stream, _)) => break stream,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(Instant::now() < deadline, "local request did not arrive");
                    thread::sleep(Duration::from_millis(5));
                }
                Err(error) => panic!("fixture accept: {error}"),
            }
        };
        stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        let mut request = Vec::new();
        let mut buf = [0; 1024];
        while !request.windows(4).any(|window| window == b"\r\n\r\n") {
            let size = stream.read(&mut buf).unwrap();
            assert!(size > 0);
            request.extend_from_slice(&buf[..size]);
            assert!(request.len() < 65536);
        }
        write!(
            stream,
            "HTTP/1.1 {status}\r\nContent-Type: application/json\r\n{headers}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
        String::from_utf8(request).unwrap()
    });
    (format!("http://{address}/search"), task)
}

fn adapter(key: &str) -> Marginalia {
    let client = primp::Client::builder().no_proxy().redirect(primp::redirect::Policy::none()).timeout(Duration::from_secs(5)).build().unwrap();
    let mut key = HeaderValue::from_str(key).unwrap();
    key.set_sensitive(true);
    Marginalia::new(client, key)
}

#[tokio::test]
async fn request_encodes_unicode_and_sends_sensitive_header_not_url_key() {
    let (endpoint, task) = serve(
        "200 OK",
        "",
        response(json!([
            {"url":"https://example.com/A","title":"A title","description":"A snippet","quality":0.9}
        ])),
    );
    let api = adapter("fixture-secret");
    assert!(api.api_key.is_sensitive());
    assert!(!format!("{:?}", api.api_key).contains("fixture-secret"));
    let result = api.search_at(&endpoint, "Rust & C++/árvíz?", 5).await.unwrap();
    assert_eq!(result.license.as_deref(), Some("fixture-license"));
    assert_eq!(result.results[0].source, "marginalia");
    assert_eq!(result.results[0].snippet, "A snippet");
    let request = task.join().unwrap();
    let line = request.lines().next().unwrap();
    assert!(!line.contains("fixture-secret"));
    let target = line.split_whitespace().nth(1).unwrap();
    let parsed = url::Url::parse(&format!("http://localhost{target}")).unwrap();
    let params: std::collections::HashMap<_, _> = parsed.query_pairs().collect();
    assert_eq!(params.get("query").unwrap(), "Rust & C++/árvíz?");
    assert_eq!(params.get("count").unwrap(), "5");
    assert!(request.to_ascii_lowercase().contains("api-key: fixture-secret"));
    assert!(request.to_ascii_lowercase().contains("accept: application/json"));
}

#[tokio::test]
async fn count_is_capped_and_returned_results_obey_limit() {
    let rows: Vec<_> = (0..105).map(|n| json!({"url":format!("https://example.com/{n}")})).collect();
    let (endpoint, task) = serve("200 OK", "", response(json!(rows)));
    let result = adapter("public").search_at(&endpoint, "test", usize::MAX).await.unwrap();
    assert_eq!(result.results.len(), 100);
    assert!(task.join().unwrap().lines().next().unwrap().contains("count=100"));
    let (endpoint, task) = serve(
        "200 OK",
        "",
        response(json!([
            {"url":"https://example.com/a"},{"url":"https://example.com/b"}
        ])),
    );
    assert_eq!(adapter("public").search_at(&endpoint, "test", 1).await.unwrap().results.len(), 1);
    task.join().unwrap();
}

#[tokio::test]
async fn filters_unsafe_urls_and_handles_missing_optional_text() {
    let (endpoint, task) = serve(
        "200 OK",
        "",
        response(json!([
            {"url":"javascript:alert(1)"},{"url":"/relative"},
            {"url":"https://user:password@example.com/"},
            {"url":"https://example.com/","title":null,"description":null},
            {"url":"http://example.net/","title":"  "}
        ])),
    );
    let result = adapter("public").search_at(&endpoint, "test", 5).await.unwrap();
    assert_eq!(result.results.len(), 2);
    assert_eq!(result.results[0].title, "https://example.com/");
    assert_eq!(result.results[0].snippet, "");
    assert_eq!(result.results[1].title, "http://example.net/");
    task.join().unwrap();
}

#[tokio::test]
async fn empty_results_still_preserve_license() {
    let (endpoint, task) = serve("200 OK", "", response(json!([])));
    let result = adapter("public").search_at(&endpoint, "test", 5).await.unwrap();
    assert!(result.results.is_empty());
    assert_eq!(result.license.as_deref(), Some("fixture-license"));
    task.join().unwrap();
}

#[tokio::test]
async fn malformed_json_schema_and_license_are_errors() {
    for body in [
        "<html>private-content</html>".to_owned(),
        json!({"query":"test","license":"fixture-license"}).to_string(),
        json!({"query":"test","results":[]}).to_string(),
        json!({"query":"test","license":"  ","results":[]}).to_string(),
        response(json!([{"title":"missing URL"}])),
    ] {
        let (endpoint, task) = serve("200 OK", "", body);
        let error = adapter("public").search_at(&endpoint, "test", 5).await.unwrap_err();
        assert!(matches!(error, Error::InvalidResponse { engine: "marginalia", .. }));
        assert!(!error.to_string().contains("private-content"));
        task.join().unwrap();
    }
}

#[tokio::test]
async fn status_errors_are_not_empty_successes() {
    for status in ["401 Unauthorized", "429 Too Many Requests", "503 Service Unavailable"] {
        let (endpoint, task) = serve(status, "", "not-json".into());
        let error = adapter("public").search_at(&endpoint, "test", 5).await.unwrap_err();
        let expected: u16 = status[..3].parse().unwrap();
        assert!(matches!(error,Error::Status{status,..} if status==expected));
        task.join().unwrap();
    }
}

#[tokio::test]
async fn redirects_never_receive_the_key() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let target = format!("http://{}/steal", listener.local_addr().unwrap());
    let (endpoint, task) = serve("302 Found", &format!("Location: {target}\r\n"), String::new());
    let error = adapter("fixture-secret").search_at(&endpoint, "test", 1).await.unwrap_err();
    assert!(matches!(error, Error::Status { status: 302, .. }));
    assert!(listener.accept().is_err(), "redirect destination must not be contacted");
    task.join().unwrap();
}
