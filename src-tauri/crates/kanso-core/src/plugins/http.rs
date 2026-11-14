//! The only door a plugin has to the network.
//!
//! Every request a plugin makes passes through [`check_request`]: `https` only, only to the
//! hosts its manifest declares (and the user saw before installing), default port, no
//! credentials in the address. Redirects are never followed on the plugin's behalf, so an
//! allowed host cannot bounce a request to one that is not.

use serde_json::{json, Map, Value};
use std::io::Read;
use std::time::Duration;
use url::Url;

const MAX_URL_CHARS: usize = 4000;
const MAX_HEADERS: usize = 24;
const MAX_HEADER_VALUE_CHARS: usize = 4000;
const MAX_BODY_BYTES: usize = 256 * 1024;
const METHODS: [&str; 6] = ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD"];
/// Headers that describe the connection itself: never a plugin's business.
const FORBIDDEN_HEADERS: [&str; 9] = [
    "host",
    "content-length",
    "connection",
    "transfer-encoding",
    "upgrade",
    "te",
    "trailer",
    "expect",
    "keep-alive",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpRequest {
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpResponse {
    pub status: u16,
    /// Lower-case header names.
    pub headers: Vec<(String, String)>,
    pub body: String,
}

impl HttpResponse {
    pub fn to_json(&self) -> Value {
        let headers: Map<String, Value> = self
            .headers
            .iter()
            .map(|(name, value)| (name.clone(), Value::String(value.clone())))
            .collect();
        json!({ "status": self.status, "headers": headers, "body": self.body })
    }
}

/// Sends an already checked request. The real one is [`UreqTransport`]; tests plug in their own.
pub trait HttpTransport: Send + Sync {
    fn send(
        &self,
        request: &HttpRequest,
        timeout: Duration,
        max_response_bytes: usize,
    ) -> Result<HttpResponse, String>;
}

/// Reads a request as a plugin wrote it (`{method, url, headers, body}`) and checks it against
/// the hosts the plugin declared. The error is shown to the plugin (and its author).
pub fn check_request(allowed_hosts: &[String], raw: &Value) -> Result<HttpRequest, String> {
    let method = raw
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or("GET")
        .to_ascii_uppercase();
    if !METHODS.contains(&method.as_str()) {
        return Err(format!("method {method} is not allowed"));
    }

    let url_text = raw.get("url").and_then(Value::as_str).unwrap_or_default().trim();
    if url_text.is_empty() || url_text.len() > MAX_URL_CHARS {
        return Err("the request has no usable url".to_string());
    }
    let url = Url::parse(url_text).map_err(|e| format!("invalid url: {e}"))?;
    if url.scheme() != "https" {
        return Err("only https:// addresses are allowed".to_string());
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("credentials in the address are not allowed".to_string());
    }
    if url.port().is_some_and(|port| port != 443) {
        return Err("only the default https port is allowed".to_string());
    }
    let host = url.host_str().unwrap_or_default().to_ascii_lowercase();
    if !allowed_hosts.iter().any(|allowed| allowed == &host) {
        return Err(format!(
            "host {host} is not listed in the plugin's \"hosts\""
        ));
    }

    let mut headers = Vec::new();
    if let Some(raw_headers) = raw.get("headers").and_then(Value::as_object) {
        if raw_headers.len() > MAX_HEADERS {
            return Err(format!("at most {MAX_HEADERS} headers"));
        }
        for (name, value) in raw_headers {
            let value = value.as_str().unwrap_or_default();
            let lower = name.to_ascii_lowercase();
            let name_ok = !name.is_empty()
                && name.len() <= 64
                && name.bytes().all(|b| b.is_ascii_alphanumeric() || b"-_".contains(&b));
            let value_ok = value.len() <= MAX_HEADER_VALUE_CHARS
                && value.chars().all(|c| !c.is_control());
            if !name_ok || !value_ok {
                return Err(format!("header {name:?} is not valid"));
            }
            if FORBIDDEN_HEADERS.contains(&lower.as_str()) || lower.starts_with("proxy-") {
                return Err(format!("header {name} cannot be set by a plugin"));
            }
            headers.push((name.clone(), value.to_string()));
        }
    }

    let body = match raw.get("body") {
        None | Some(Value::Null) => None,
        Some(Value::String(body)) if body.len() <= MAX_BODY_BYTES => Some(body.clone()),
        Some(Value::String(_)) => return Err("the request body is too large".to_string()),
        Some(_) => return Err("the request body must be a string".to_string()),
    };
    if body.is_some() && matches!(method.as_str(), "GET" | "HEAD") {
        return Err(format!("a {method} request cannot have a body"));
    }

    Ok(HttpRequest {
        method,
        url: url.into(),
        headers,
        body,
    })
}

/// The real network, through `ureq`.
pub struct UreqTransport {
    agent: ureq::Agent,
}

impl UreqTransport {
    pub fn new(user_agent: &str) -> Self {
        Self {
            agent: ureq::AgentBuilder::new()
                // A redirect is handed back to the plugin, which may follow it only towards
                // a host it declared
                .redirects(0)
                .timeout_connect(Duration::from_secs(8))
                .user_agent(user_agent)
                .build(),
        }
    }
}

impl HttpTransport for UreqTransport {
    fn send(
        &self,
        request: &HttpRequest,
        timeout: Duration,
        max_response_bytes: usize,
    ) -> Result<HttpResponse, String> {
        let mut call = self.agent.request(&request.method, &request.url).timeout(timeout);
        for (name, value) in &request.headers {
            call = call.set(name, value);
        }
        let outcome = match &request.body {
            Some(body) => call.send_string(body),
            None => call.call(),
        };
        let response = match outcome {
            Ok(response) => response,
            // 4xx and 5xx are answers too: the plugin decides what they mean
            Err(ureq::Error::Status(_, response)) => response,
            Err(ureq::Error::Transport(transport)) => return Err(transport.to_string()),
        };

        let status = response.status();
        let headers: Vec<(String, String)> = response
            .headers_names()
            .into_iter()
            .take(48)
            .filter_map(|name| {
                let value = response.header(&name)?.to_string();
                Some((name.to_ascii_lowercase(), value))
            })
            .collect();

        let mut bytes = Vec::new();
        response
            .into_reader()
            .take(max_response_bytes as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| format!("reading the response failed: {e}"))?;
        if bytes.len() > max_response_bytes {
            return Err(format!(
                "the response is larger than {} kB",
                max_response_bytes / 1024
            ));
        }
        Ok(HttpResponse {
            status,
            headers,
            body: String::from_utf8_lossy(&bytes).into_owned(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hosts() -> Vec<String> {
        vec!["api.example.org".to_string(), "example.org".to_string()]
    }

    #[test]
    fn accepts_plain_https_requests_to_declared_hosts() {
        let request = check_request(
            &hosts(),
            &json!({
                "method": "post",
                "url": "https://API.example.org/search?q=portal%202",
                "headers": {"Content-Type": "application/json", "X-Token": "abc"},
                "body": "{\"q\":1}"
            }),
        )
        .expect("allowed");
        assert_eq!(request.method, "POST");
        assert_eq!(request.url, "https://api.example.org/search?q=portal%202");
        assert_eq!(request.headers.len(), 2);
        assert_eq!(request.body.as_deref(), Some("{\"q\":1}"));

        let get = check_request(&hosts(), &json!({"url": "https://example.org:443/"})).expect("allowed");
        assert_eq!((get.method.as_str(), get.body), ("GET", None));
    }

    #[test]
    fn refuses_everything_outside_the_declared_hosts_and_plain_https() {
        let refuse = |request: Value| check_request(&hosts(), &request).expect_err(&request.to_string());

        assert!(refuse(json!({"url": "https://evil.example/steal"})).contains("not listed"));
        assert!(refuse(json!({"url": "https://sub.api.example.org/"})).contains("not listed"));
        assert!(refuse(json!({"url": "https://example.org.evil.example/"})).contains("not listed"));
        assert!(refuse(json!({"url": "https://127.0.0.1/"})).contains("not listed"));
        assert!(refuse(json!({"url": "http://example.org/"})).contains("only https"));
        assert!(refuse(json!({"url": "file:///C:/Users/me/AppData/config.json"})).contains("only https"));
        assert!(refuse(json!({"url": "https://example.org:8443/"})).contains("default https port"));
        assert!(refuse(json!({"url": "https://user:pw@example.org/"})).contains("credentials"));
        // What looks like the allowed host before an @ is only a user name
        assert!(refuse(json!({"url": "https://example.org@evil.example/"})).contains("credentials"));
        assert!(refuse(json!({"url": ""})).contains("no usable url"));
        assert!(refuse(json!({"method": "CONNECT", "url": "https://example.org/"})).contains("not allowed"));
        assert!(refuse(json!({"url": "https://example.org/", "headers": {"Host": "evil.example"}}))
            .contains("cannot be set"));
        assert!(refuse(json!({"url": "https://example.org/", "headers": {"X-A": "a\r\nHost: evil.example"}}))
            .contains("not valid"));
        assert!(refuse(json!({"url": "https://example.org/", "body": "x"})).contains("cannot have a body"));
        assert!(refuse(json!({"method": "POST", "url": "https://example.org/", "body": {"a": 1}}))
            .contains("must be a string"));

        // A plugin that declares no hosts has no network at all
        assert!(check_request(&[], &json!({"url": "https://example.org/"})).is_err());
    }
}
