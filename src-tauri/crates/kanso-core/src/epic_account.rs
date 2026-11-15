//! The Epic Games account: signing in, and the library it owns.
//!
//! Epic has no public API for a user's library. Like every other launcher that lists Epic
//! games (Heroic, Legendary, Playnite), Kanso talks to the services the Epic Games Launcher
//! itself uses and presents itself with the launcher's own client id and secret, which those
//! projects publish in their source code. They identify the *program*, not the user: nothing
//! of the user's is in this file.
//!
//! Signing in never shows Kanso the password. The user signs in on Epic's own page, in the
//! browser; that page ends on a short-lived authorization code, which the user pastes into
//! Kanso and Kanso exchanges for a session ([`EpicApi::exchange_code`]). The session's
//! refresh token is what is kept between runs (see [`crate::accounts`]); each use replaces it
//! with a new one ([`EpicApi::refresh`]).
//!
//! With a session, the library service lists what the account owns (names of apps, not
//! titles) and the catalog service describes each item in the same shape the launcher's own
//! catalog cache uses, so the rules of [`crate::scanner::epic_catalog`] decide what is a game.

use base64::Engine as _;
use serde_json::Value;
use std::time::Duration;

/// Identity of the Epic Games Launcher as an OAuth client, as published by the open-source
/// launchers that list Epic libraries.
const LAUNCHER_CLIENT_ID: &str = "34a02cf8f4414e29b15921876da36f9a";
const LAUNCHER_CLIENT_SECRET: &str = "daafbccc737745039dffe53d94fc76cf";

const ACCOUNT_SERVICE: &str = "https://account-public-service-prod03.ol.epicgames.com";
const LIBRARY_SERVICE: &str = "https://library-service.live.use1a.on.epicgames.com";
const CATALOG_SERVICE: &str = "https://catalog-public-service-prod06.ol.epicgames.com";

/// Pages of the library read at most: a guard against a cursor that never ends.
const MAX_LIBRARY_PAGES: usize = 400;

/// Where the user signs in: Epic's own page, which ends on the authorization code.
pub fn sign_in_url() -> String {
    format!(
        "https://www.epicgames.com/id/login?redirectUrl=https%3A%2F%2Fwww.epicgames.com%2Fid%2Fapi%2Fredirect%3FclientId%3D{LAUNCHER_CLIENT_ID}%26responseType%3Dcode"
    )
}

/// Why a call to Epic's services gave nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EpicError {
    /// Epic refused the code or the session (the text is Epic's error code): the user has to
    /// sign in again.
    Refused(String),
    /// Epic could not be reached.
    Unreachable,
    /// Epic answered something this code does not expect: which call, and what came back.
    Unexpected(String),
}

impl std::fmt::Display for EpicError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EpicError::Refused(code) => write!(f, "refused by Epic ({code})"),
            EpicError::Unreachable => write!(f, "Epic could not be reached"),
            EpicError::Unexpected(detail) => write!(f, "unexpected answer from Epic: {detail}"),
        }
    }
}

/// A signed-in session.
#[derive(Clone, PartialEq, Eq)]
pub struct EpicSession {
    pub account_id: String,
    pub display_name: String,
    access_token: String,
    token_type: String,
    /// Keeps the session going between runs; replaced at every [`EpicApi::refresh`].
    pub refresh_token: String,
}

impl std::fmt::Debug for EpicSession {
    /// Tokens never reach a log or an error message.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EpicSession")
            .field("account_id", &self.account_id)
            .field("display_name", &self.display_name)
            .finish_non_exhaustive()
    }
}

/// An item of the account's library, as the library service lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EpicAsset {
    /// The name the launcher starts and installs the item by.
    pub app_name: String,
    pub namespace: String,
    pub catalog_item_id: String,
}

/// What the user pasted, reduced to the authorization code: the code itself, the whole text
/// Epic's page shows (`{"authorizationCode":"..."}`) or an address carrying `code=`.
pub fn authorization_code_in(pasted: &str) -> Option<String> {
    let pasted = pasted.trim();
    let candidate = serde_json::from_str::<Value>(pasted)
        .ok()
        .and_then(|json| json.get("authorizationCode")?.as_str().map(str::to_string))
        .or_else(|| {
            let (_, after) = pasted.split_once("code=")?;
            Some(after.split(['&', '#', '"', ' ']).next().unwrap_or_default().to_string())
        })
        .unwrap_or_else(|| pasted.trim_matches('"').to_string());
    let plain = (8..=128).contains(&candidate.len())
        && candidate.bytes().all(|b| b.is_ascii_alphanumeric());
    plain.then_some(candidate)
}

/// Client for the three Epic services Kanso uses.
#[derive(Debug, Clone)]
pub struct EpicApi {
    account_service: String,
    library_service: String,
    catalog_service: String,
    agent: ureq::Agent,
}

impl Default for EpicApi {
    fn default() -> Self {
        Self::new()
    }
}

impl EpicApi {
    pub fn new() -> Self {
        Self::with_services(ACCOUNT_SERVICE, LIBRARY_SERVICE, CATALOG_SERVICE)
    }

    /// Client against other endpoints (tests).
    pub fn with_services(account: &str, library: &str, catalog: &str) -> Self {
        Self {
            account_service: account.trim_end_matches('/').to_string(),
            library_service: library.trim_end_matches('/').to_string(),
            catalog_service: catalog.trim_end_matches('/').to_string(),
            agent: ureq::AgentBuilder::new()
                .timeout_connect(Duration::from_secs(8))
                .timeout_read(Duration::from_secs(20))
                .user_agent("KansoLauncher/0.1.0")
                .build(),
        }
    }

    /// Turns the authorization code of Epic's sign-in page into a session.
    pub fn exchange_code(&self, code: &str) -> Result<EpicSession, EpicError> {
        self.token(&[("grant_type", "authorization_code"), ("code", code), ("token_type", "eg1")])
    }

    /// Renews a session from its refresh token. The session returned carries a new refresh
    /// token: the old one is spent.
    pub fn refresh(&self, refresh_token: &str) -> Result<EpicSession, EpicError> {
        self.token(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
            ("token_type", "eg1"),
        ])
    }

    fn token(&self, form: &[(&str, &str)]) -> Result<EpicSession, EpicError> {
        let basic = base64::engine::general_purpose::STANDARD
            .encode(format!("{LAUNCHER_CLIENT_ID}:{LAUNCHER_CLIENT_SECRET}"));
        let response = self
            .agent
            .post(&format!("{}/account/api/oauth/token", self.account_service))
            .set("Authorization", &format!("basic {basic}"))
            .send_form(form);
        let json = match response {
            Ok(response) => response
                .into_json::<Value>()
                .map_err(|_| EpicError::Unexpected("sign-in: the answer is not JSON".into()))?,
            // The token service answers a bad code or a spent session with 400
            Err(ureq::Error::Status(status @ (400 | 401 | 403), response)) => {
                return Err(EpicError::Refused(error_code_of(response, status)));
            }
            Err(ureq::Error::Status(status, _)) => {
                return Err(EpicError::Unexpected(format!("sign-in: HTTP {status}")));
            }
            Err(ureq::Error::Transport(_)) => return Err(EpicError::Unreachable),
        };

        let text = |key: &str| json.get(key).and_then(Value::as_str).unwrap_or_default().to_string();
        let session = EpicSession {
            account_id: text("account_id"),
            display_name: text("displayName"),
            access_token: text("access_token"),
            token_type: Some(text("token_type")).filter(|kind| !kind.is_empty()).unwrap_or_else(|| "bearer".into()),
            refresh_token: text("refresh_token"),
        };
        if session.access_token.is_empty() || session.refresh_token.is_empty() || session.account_id.is_empty() {
            return Err(EpicError::Unexpected("sign-in: the answer carries no session".into()));
        }
        Ok(session)
    }

    fn get(&self, session: &EpicSession, url: &str, what: &str) -> Result<Value, EpicError> {
        let response = self
            .agent
            .get(url)
            .set("Authorization", &format!("{} {}", session.token_type, session.access_token))
            .call();
        match response {
            Ok(response) => response
                .into_json::<Value>()
                .map_err(|_| EpicError::Unexpected(format!("{what}: the answer is not JSON"))),
            Err(ureq::Error::Status(401, response)) => Err(EpicError::Refused(error_code_of(response, 401))),
            Err(ureq::Error::Status(status, _)) => Err(EpicError::Unexpected(format!("{what}: HTTP {status}"))),
            Err(ureq::Error::Transport(_)) => Err(EpicError::Unreachable),
        }
    }

    /// Everything the account owns for Windows, as the library service lists it: games,
    /// add-ons and extra content alike. Unreal Engine content and private sandboxes (builds a
    /// developer has access to) are left out, as other launchers do.
    pub fn library(&self, session: &EpicSession) -> Result<Vec<EpicAsset>, EpicError> {
        let mut assets: Vec<EpicAsset> = Vec::new();
        let mut cursor: Option<String> = None;
        for _ in 0..MAX_LIBRARY_PAGES {
            let mut url = format!(
                "{}/library/api/public/items?includeMetadata=true&platform=Windows",
                self.library_service
            );
            if let Some(cursor) = &cursor {
                url.push_str("&cursor=");
                url.push_str(&percent_encode(cursor));
            }
            let page = self.get(session, &url, "library")?;
            let records = page
                .get("records")
                .and_then(Value::as_array)
                .ok_or_else(|| EpicError::Unexpected("library: the answer has no records".into()))?;
            for record in records {
                let text = |key: &str| record.get(key).and_then(Value::as_str).unwrap_or_default().trim();
                let (app_name, namespace) = (text("appName"), text("namespace"));
                if app_name.is_empty()
                    || namespace.eq_ignore_ascii_case("ue")
                    || text("sandboxType").eq_ignore_ascii_case("PRIVATE")
                    || assets.iter().any(|known| known.app_name == app_name)
                {
                    continue;
                }
                assets.push(EpicAsset {
                    app_name: app_name.to_string(),
                    namespace: namespace.to_string(),
                    catalog_item_id: text("catalogItemId").to_string(),
                });
            }
            cursor = page
                .get("responseMetadata")
                .and_then(|metadata| metadata.get("nextCursor"))
                .and_then(Value::as_str)
                .filter(|next| !next.is_empty())
                .map(str::to_string);
            if cursor.is_none() {
                break;
            }
        }
        Ok(assets)
    }

    /// The catalog's description of an item, in the shape of the launcher's catalog cache.
    /// `Ok(None)`: the catalog has no such item.
    pub fn catalog_item(&self, session: &EpicSession, asset: &EpicAsset) -> Result<Option<Value>, EpicError> {
        let url = format!(
            "{}/catalog/api/shared/namespace/{}/bulk/items?id={}&country=US&locale=en-US&includeMainGameDetails=true",
            self.catalog_service,
            percent_encode(&asset.namespace),
            percent_encode(&asset.catalog_item_id),
        );
        let answer = self.get(session, &url, "catalog")?;
        Ok(answer.get(&asset.catalog_item_id).filter(|item| item.is_object()).cloned())
    }
}

/// Epic's own name for what went wrong (`errorCode`), or the status when there is none.
fn error_code_of(response: ureq::Response, status: u16) -> String {
    response
        .into_json::<Value>()
        .ok()
        .and_then(|json| json.get("errorCode")?.as_str().map(str::to_string))
        .filter(|code| !code.is_empty() && code.len() <= 160)
        .unwrap_or_else(|| format!("HTTP {status}"))
}

fn percent_encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for byte in input.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};

    /// What the mock services saw: (method and path, Authorization header, body).
    pub(crate) type Seen = Arc<Mutex<Vec<(String, String, String)>>>;

    /// Epic's three services on one local port, answering as the real ones do. The session
    /// handed out is `access-N`/`refresh-N`, N growing at every sign-in or renewal; only the
    /// latest refresh token is accepted, as on the real service.
    pub(crate) fn spawn_mock_epic() -> (String, Seen) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock server");
        let base_url = format!("http://{}", listener.local_addr().unwrap());
        let seen: Seen = Arc::default();
        let recorded = Arc::clone(&seen);

        std::thread::spawn(move || {
            let mut sessions = 0u32;
            for stream in listener.incoming().flatten() {
                let mut reader = BufReader::new(&stream);
                let mut request_line = String::new();
                if reader.read_line(&mut request_line).is_err() || request_line.is_empty() {
                    continue;
                }
                let (mut authorization, mut length) = (String::new(), 0usize);
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).is_err() || line.trim().is_empty() {
                        break;
                    }
                    let lower = line.to_ascii_lowercase();
                    if let Some(value) = lower.strip_prefix("authorization:") {
                        authorization = line[line.len() - value.len()..].trim().to_string();
                    } else if let Some(value) = lower.strip_prefix("content-length:") {
                        length = value.trim().parse().unwrap_or(0);
                    }
                }
                let mut body = vec![0u8; length];
                let _ = reader.read_exact(&mut body);
                let body = String::from_utf8_lossy(&body).to_string();
                let mut parts = request_line.split_whitespace();
                let (method, path) = (parts.next().unwrap_or(""), parts.next().unwrap_or("/"));
                recorded.lock().unwrap().push((format!("{method} {path}"), authorization.clone(), body.clone()));

                let session = |n: u32| {
                    format!(
                        r#"{{"access_token":"access-{n}","token_type":"bearer","refresh_token":"refresh-{n}","account_id":"acc0123","displayName":"Player One","expires_in":7200}}"#
                    )
                };
                let authorized = authorization == format!("bearer access-{sessions}");
                let (status, answer): (&str, String) = if path == "/account/api/oauth/token" {
                    let launcher = authorization == "basic MzRhMDJjZjhmNDQxNGUyOWIxNTkyMTg3NmRhMzZmOWE6ZGFhZmJjY2M3Mzc3NDUwMzlkZmZlNTNkOTRmYzc2Y2Y=";
                    let good_code = body.contains("grant_type=authorization_code") && body.contains("code=goodcode1234");
                    let good_refresh = body.contains("grant_type=refresh_token")
                        && sessions > 0
                        && body.contains(&format!("refresh_token=refresh-{sessions}&"));
                    if !launcher {
                        ("400 Bad Request", r#"{"errorCode":"errors.com.epicgames.common.oauth.invalid_client"}"#.into())
                    } else if !body.contains("token_type=eg1") {
                        ("400 Bad Request", r#"{"errorCode":"test.missing_token_type"}"#.into())
                    } else if good_code || good_refresh {
                        sessions += 1;
                        ("200 OK", session(sessions))
                    } else if body.contains("grant_type=refresh_token") {
                        ("400 Bad Request", r#"{"errorCode":"errors.com.epicgames.account.auth_token.invalid_refresh_token"}"#.into())
                    } else {
                        ("400 Bad Request", r#"{"errorCode":"errors.com.epicgames.account.oauth.authorization_code_not_found"}"#.into())
                    }
                } else if !authorized {
                    ("401 Unauthorized", r#"{"errorCode":"errors.com.epicgames.common.authentication.token_verification_failed"}"#.into())
                } else if path == "/library/api/public/items?includeMetadata=true&platform=Windows" {
                    ("200 OK", r#"{"responseMetadata":{"nextCursor":"page two"},"records":[
                        {"appName":"dc9d2e595d0e4650b35d659f90d41059","namespace":"alanwake2","catalogItemId":"cat-alan","sandboxType":"PUBLIC"},
                        {"appName":"nightsprings","namespace":"alanwake2","catalogItemId":"cat-dlc","sandboxType":"PUBLIC"},
                        {"appName":"UE_5.4","namespace":"ue","catalogItemId":"cat-ue","sandboxType":"PUBLIC"},
                        {"appName":"DevBuild","namespace":"studio","catalogItemId":"cat-dev","sandboxType":"PRIVATE"},
                        {"appName":"","namespace":"x","catalogItemId":"cat-empty"}
                    ]}"#.into())
                } else if path == "/library/api/public/items?includeMetadata=true&platform=Windows&cursor=page%20two" {
                    ("200 OK", r#"{"responseMetadata":{},"records":[
                        {"appName":"Sugar","namespace":"rocket","catalogItemId":"cat-rocket","sandboxType":"PUBLIC"},
                        {"appName":"Sugar","namespace":"rocket","catalogItemId":"cat-rocket","sandboxType":"PUBLIC"},
                        {"appName":"Calluna","namespace":"control","catalogItemId":"cat-control","sandboxType":"PUBLIC"},
                        {"appName":"Gone","namespace":"gone","catalogItemId":"cat-gone","sandboxType":"PUBLIC"}
                    ]}"#.into())
                } else if path.starts_with("/catalog/api/shared/namespace/alanwake2/bulk/items?id=cat-alan&") {
                    ("200 OK", r#"{"cat-alan":{"id":"cat-alan","title":"Alan Wake 2","namespace":"alanwake2","categories":[{"path":"games"},{"path":"applications"}],"keyImages":[{"type":"DieselGameBoxTall","url":"https://cdn1.epicgames.com/alan/tall.jpg"}],"releaseInfo":[{"appId":"dc9d2e595d0e4650b35d659f90d41059","platform":["Windows"]}]}}"#.into())
                } else if path.starts_with("/catalog/api/shared/namespace/alanwake2/bulk/items?id=cat-dlc&") {
                    ("200 OK", r#"{"cat-dlc":{"id":"cat-dlc","title":"Night Springs","namespace":"alanwake2","categories":[{"path":"addons"},{"path":"applications"}],"mainGameItem":{"id":"cat-alan"}}}"#.into())
                } else if path.starts_with("/catalog/api/shared/namespace/rocket/bulk/items?id=cat-rocket&") {
                    ("200 OK", "{\"cat-rocket\":{\"id\":\"cat-rocket\",\"title\":\"Rocket League\u{ae}\",\"namespace\":\"rocket\",\"categories\":[{\"path\":\"games\"},{\"path\":\"applications\"}],\"keyImages\":[{\"type\":\"DieselGameBox\",\"url\":\"https://cdn1.epicgames.com/rl/wide.jpg\"}]}}".into())
                } else if path.starts_with("/catalog/api/shared/namespace/control/bulk/items?id=cat-control&") {
                    ("200 OK", r#"{"cat-control":{"id":"cat-control","title":"Control","namespace":"control","categories":[{"path":"games"},{"path":"applications"}]}}"#.into())
                } else if path.starts_with("/catalog/api/shared/namespace/gone/") {
                    ("200 OK", "{}".into())
                } else {
                    ("404 Not Found", "{}".into())
                };

                let header = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    answer.len()
                );
                let mut writer = &stream;
                let _ = writer.write_all(header.as_bytes());
                let _ = writer.write_all(answer.as_bytes());
                let _ = writer.flush();
            }
        });
        (base_url, seen)
    }

    pub(crate) fn mock_api(base_url: &str) -> EpicApi {
        EpicApi::with_services(base_url, base_url, base_url)
    }

    #[test]
    fn the_sign_in_page_is_epics_own_and_asks_for_a_code() {
        let url = sign_in_url();
        assert!(url.starts_with("https://www.epicgames.com/id/login?redirectUrl="));
        assert!(url.contains("responseType%3Dcode"));
        assert!(url.contains(LAUNCHER_CLIENT_ID));
        assert!(!url.contains(LAUNCHER_CLIENT_SECRET));
    }

    #[test]
    fn finds_the_code_in_whatever_the_user_pasted() {
        let code = "0123456789abcdef0123456789abcdef";
        assert_eq!(authorization_code_in(code).as_deref(), Some(code));
        assert_eq!(authorization_code_in(&format!("  \"{code}\"\n")).as_deref(), Some(code));
        assert_eq!(
            authorization_code_in(&format!(r#"{{"redirectUrl":"https://localhost/launcher/authorized?code={code}","authorizationCode":"{code}","sid":null}}"#)).as_deref(),
            Some(code)
        );
        assert_eq!(
            authorization_code_in(&format!("https://localhost/launcher/authorized?code={code}&state=x")).as_deref(),
            Some(code)
        );
        // Signed out in the browser: the page has no code yet
        assert_eq!(authorization_code_in(r#"{"redirectUrl":null,"authorizationCode":null,"sid":null}"#), None);
        assert_eq!(authorization_code_in(""), None);
        assert_eq!(authorization_code_in("short"), None);
        assert_eq!(authorization_code_in("not a code: has spaces and/slashes"), None);
        assert_eq!(authorization_code_in(&"a".repeat(200)), None);
    }

    #[test]
    fn a_code_becomes_a_session_and_a_session_renews_itself_once() {
        let (base_url, seen) = spawn_mock_epic();
        let api = mock_api(&base_url);

        assert_eq!(
            api.exchange_code("wrongcode0000"),
            Err(EpicError::Refused("errors.com.epicgames.account.oauth.authorization_code_not_found".into()))
        );

        let session = api.exchange_code("goodcode1234").expect("signed in");
        assert_eq!(session.account_id, "acc0123");
        assert_eq!(session.display_name, "Player One");
        assert_eq!(session.refresh_token, "refresh-1");
        // Tokens stay out of anything printed
        assert!(!format!("{session:?}").contains("access-1"));
        assert!(!format!("{session:?}").contains("refresh-1"));

        let renewed = api.refresh(&session.refresh_token).expect("renewed");
        assert_eq!(renewed.refresh_token, "refresh-2");
        // The token just used is spent
        assert_eq!(
            api.refresh("refresh-1"),
            Err(EpicError::Refused("errors.com.epicgames.account.auth_token.invalid_refresh_token".into()))
        );

        // Every sign-in call presented the launcher's identity, as a form
        let seen = seen.lock().unwrap();
        assert!(seen.iter().all(|(request, _, _)| request == "POST /account/api/oauth/token"));
        assert!(seen.iter().all(|(_, authorization, _)| authorization.starts_with("basic ")));
        assert_eq!(seen[1].2, "grant_type=authorization_code&code=goodcode1234&token_type=eg1");
    }

    #[test]
    fn the_library_is_read_page_by_page_and_the_catalog_describes_each_item() {
        let (base_url, _seen) = spawn_mock_epic();
        let api = mock_api(&base_url);
        let session = api.exchange_code("goodcode1234").unwrap();

        let assets = api.library(&session).expect("library");
        let names: Vec<&str> = assets.iter().map(|asset| asset.app_name.as_str()).collect();
        assert_eq!(
            names,
            ["dc9d2e595d0e4650b35d659f90d41059", "nightsprings", "Sugar", "Calluna", "Gone"],
            "both pages, no Unreal Engine content, no private builds, nothing nameless or twice"
        );

        let alan = api.catalog_item(&session, &assets[0]).unwrap().expect("described");
        assert_eq!(alan["title"], "Alan Wake 2");
        assert_eq!(api.catalog_item(&session, &assets[4]), Ok(None), "the catalog has no such item");

        // A session Epic no longer accepts
        let mut stale = session.clone();
        stale.access_token = "access-0".into();
        assert!(matches!(api.library(&stale), Err(EpicError::Refused(_))));
        assert!(matches!(api.catalog_item(&stale, &assets[0]), Err(EpicError::Refused(_))));
    }

    #[test]
    fn an_epic_that_cannot_be_reached_is_told_apart_from_one_that_refuses() {
        let api = mock_api("http://127.0.0.1:9");
        assert_eq!(api.exchange_code("goodcode1234"), Err(EpicError::Unreachable));
        assert_eq!(api.refresh("refresh-1"), Err(EpicError::Unreachable));
    }
}
