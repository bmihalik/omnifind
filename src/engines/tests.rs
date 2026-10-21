use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use super::checked_json;
use crate::Error;

// Pre-compressed test-only fixtures containing the JSON object {"ok":true}.
const GZIP: &[u8] = &[31, 139, 8, 0, 0, 0, 0, 0, 2, 3, 171, 86, 202, 207, 86, 178, 42, 41, 42, 77, 173, 5, 0, 144, 95, 212, 167, 11, 0, 0, 0];
const DEFLATE: &[u8] = &[120, 156, 171, 86, 202, 207, 86, 178, 42, 41, 42, 77, 173, 5, 0, 22, 227, 4, 17];
const BROTLI: &[u8] = &[11, 5, 128, 123, 34, 111, 107, 34, 58, 116, 114, 117, 101, 125, 3];
const ZSTD: &[u8] = &[40, 181, 47, 253, 32, 11, 89, 0, 0, 123, 34, 111, 107, 34, 58, 116, 114, 117, 101, 125];

fn serve(status: &'static str, content_type: &'static str, encoding: Option<&'static str>, body: Vec<u8>) -> (String, JoinHandle<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let worker = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        let mut request = Vec::new();
        let mut buffer = [0; 1024];
        while !request.windows(4).any(|window| window == b"\r\n\r\n") {
            let size = stream.read(&mut buffer).unwrap();
            assert!(size > 0, "request ended before its headers");
            request.extend_from_slice(&buffer[..size]);
            assert!(request.len() < 65536, "request headers too large");
        }
        let encoding_header = encoding.map(|value| format!("Content-Encoding: {value}\r\n")).unwrap_or_default();
        let headers = format!("HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\n{encoding_header}Content-Length: {}\r\nConnection: close\r\n\r\n", body.len());
        stream.write_all(headers.as_bytes()).unwrap();
        stream.write_all(&body).unwrap();
        String::from_utf8(request).unwrap()
    });
    (format!("http://{address}/test"), worker)
}

fn client() -> primp::Client {
    primp::Client::builder().impersonate(primp::Impersonate::ChromeV146).timeout(Duration::from_secs(5)).no_proxy().build().unwrap()
}

async fn assert_decodes(encoding: &'static str, body: &[u8]) {
    let (url, worker) = serve("200 OK", "application/json", Some(encoding), body.to_vec());
    let result: serde_json::Value = checked_json("fixture", client().get(url)).await.unwrap();
    assert_eq!(result, serde_json::json!({"ok": true}));
    let request = worker.join().unwrap().to_ascii_lowercase();
    assert!(request.contains("accept: application/json"));
    let advertised = request.lines().find_map(|line| line.strip_prefix("accept-encoding:")).unwrap();
    assert!(advertised.split(',').any(|value| value.trim() == encoding));
}

#[tokio::test]
async fn decodes_gzip() {
    assert_decodes("gzip", GZIP).await;
}

#[tokio::test]
async fn decodes_deflate() {
    assert_decodes("deflate", DEFLATE).await;
}

#[tokio::test]
async fn decodes_brotli() {
    assert_decodes("br", BROTLI).await;
}

#[tokio::test]
async fn decodes_zstd() {
    assert_decodes("zstd", ZSTD).await;
}

#[tokio::test]
async fn decodes_plain_json() {
    let (url, worker) = serve("200 OK", "application/json", None, br#"{"ok":true}"#.to_vec());
    let value: serde_json::Value = checked_json("fixture", client().get(url)).await.unwrap();
    assert_eq!(value["ok"], true);
    worker.join().unwrap();
}

#[tokio::test]
async fn html_has_context_without_exposing_response_body() {
    let (url, worker) = serve("200 OK", "text/html", None, b"<html>private-response-content</html>".to_vec());
    let error = checked_json::<serde_json::Value>("fixture", client().get(url)).await.unwrap_err();
    assert!(matches!(&error, Error::InvalidResponse { engine: "fixture", .. }));
    let message = error.to_string();
    assert!(message.contains("text/html"));
    assert!(message.contains("expected JSON"));
    assert!(!message.contains("private-response-content"));
    worker.join().unwrap();
}

#[tokio::test]
async fn non_success_status_is_not_a_json_error() {
    let (url, worker) = serve("429 Too Many Requests", "text/html", None, b"slow down".to_vec());
    let error = checked_json::<serde_json::Value>("fixture", client().get(url)).await.unwrap_err();
    assert!(matches!(error, Error::Status { status: 429, .. }));
    worker.join().unwrap();
}
