//! A tiny HTTP server for tests: one handler, one answer per request.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;

pub(crate) struct Request {
    pub method: String,
    /// Path and query, as sent.
    pub path: String,
    /// Header names are lower-cased.
    headers: HashMap<String, String>,
    pub body: String,
}

impl Request {
    /// The value of a header (name in lower case), or "".
    pub fn header(&self, name: &str) -> &str {
        self.headers.get(name).map(String::as_str).unwrap_or_default()
    }
}

/// Serves `handler` on a local port and returns its base address. The handler returns the
/// status code and the JSON body of each answer.
pub(crate) fn spawn(handler: impl Fn(&Request) -> (u16, String) + Send + 'static) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock server");
    let base_url = format!("http://{}", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let mut reader = BufReader::new(&stream);
            let mut request_line = String::new();
            if reader.read_line(&mut request_line).is_err() || request_line.is_empty() {
                continue;
            }
            let mut headers = HashMap::new();
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).is_err() || line.trim().is_empty() {
                    break;
                }
                if let Some((name, value)) = line.split_once(':') {
                    headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_string());
                }
            }
            let length = headers.get("content-length").and_then(|n| n.parse().ok()).unwrap_or(0);
            let mut body = vec![0u8; length];
            let _ = reader.read_exact(&mut body);
            let mut parts = request_line.split_whitespace();
            let request = Request {
                method: parts.next().unwrap_or_default().to_string(),
                path: parts.next().unwrap_or("/").to_string(),
                headers,
                body: String::from_utf8_lossy(&body).to_string(),
            };

            let (status, answer) = handler(&request);
            let header = format!(
                "HTTP/1.1 {status} Mock\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                answer.len()
            );
            let mut writer = &stream;
            let _ = writer.write_all(header.as_bytes());
            let _ = writer.write_all(answer.as_bytes());
            let _ = writer.flush();
        }
    });
    base_url
}
