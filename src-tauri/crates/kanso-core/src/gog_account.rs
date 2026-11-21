//! The GOG account: signing in, and the library it owns.
//!
//! For people who do not use GOG Galaxy (whose local database Kanso reads without any
//! sign-in, see [`crate::scanner::gog_galaxy`]). Like the other launchers that list GOG
//! libraries (Lutris, Minigalaxy, Heroic), Kanso signs in as the Galaxy client, with the
//! client id and secret those projects publish in their source code. They identify the
//! *program*, not the user.
//!
//! Signing in never shows Kanso the password: the user signs in on GOG's own page, in the
//! browser, and ends on a page whose address carries a short-lived authorization code. The
//! user pastes that address into Kanso, which exchanges the code for a session
//! ([`GogApi::exchange_code`]). The session's refresh token is what is kept between runs
//! (see [`crate::accounts`]).
//!
//! With a session, GOG lists the account's games page by page, each with its title and
//! artwork, so nothing else has to be asked.

use crate::accounts::StoreError;
use crate::cleaner::clean_game_title;
use crate::models::GamePlatform;
use crate::owned::OwnedGame;
use serde_json::Value;
use std::time::Duration;

/// Identity of GOG Galaxy as an OAuth client, as published by the open-source launchers
/// that list GOG libraries.
const GALAXY_CLIENT_ID: &str = "46899977096215655";
const GALAXY_CLIENT_SECRET: &str = "9d85c43b1482497dbbce61f6e4aa173a433796eeae2ca8c5f6129f2dc4de46d9";
/// Where GOG sends the browser after a sign-in, with the code in the address.
const REDIRECT_URI: &str = "https://embed.gog.com/on_login_success?origin=client";

const AUTH_SERVICE: &str = "https://auth.gog.com";
const EMBED_SERVICE: &str = "https://embed.gog.com";

/// Pages of the library read at most: a guard against a count that never ends.
const MAX_LIBRARY_PAGES: u64 = 400;

/// Where the user signs in: GOG's own page, which ends on an address carrying the code.
pub fn sign_in_url() -> String {
    format!(
        "{AUTH_SERVICE}/auth?client_id={GALAXY_CLIENT_ID}&redirect_uri={}&response_type=code&layout=client2",
        percent_encode(REDIRECT_URI)
    )
}

/// A signed-in session.
#[derive(Clone, PartialEq, Eq)]
pub struct GogSession {
    access_token: String,
    /// Keeps the session going between runs; whatever the latest answer carried.
    pub refresh_token: String,
}

impl std::fmt::Debug for GogSession {
    /// Tokens never reach a log or an error message.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GogSession").finish_non_exhaustive()
    }
}

/// Client for the two GOG services Kanso uses.
#[derive(Debug, Clone)]
pub struct GogApi {
    auth_service: String,
    embed_service: String,
    agent: ureq::Agent,
}

impl Default for GogApi {
    fn default() -> Self {
        Self::new()
    }
}

impl GogApi {
    pub fn new() -> Self {
        Self::with_services(AUTH_SERVICE, EMBED_SERVICE)
    }

    /// Client against other endpoints (tests).
    pub fn with_services(auth: &str, embed: &str) -> Self {
        Self {
            auth_service: auth.trim_end_matches('/').to_string(),
            embed_service: embed.trim_end_matches('/').to_string(),
            agent: ureq::AgentBuilder::new()
                .timeout_connect(Duration::from_secs(8))
                .timeout_read(Duration::from_secs(20))
                .user_agent(crate::USER_AGENT)
                .build(),
        }
    }

    /// Turns the authorization code of GOG's sign-in into a session.
    pub fn exchange_code(&self, code: &str) -> Result<GogSession, StoreError> {
        self.token(&[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", REDIRECT_URI),
        ])
    }

    /// Renews a session from its refresh token.
    pub fn refresh(&self, refresh_token: &str) -> Result<GogSession, StoreError> {
        self.token(&[("grant_type", "refresh_token"), ("refresh_token", refresh_token)])
    }

    /// GOG's token service takes everything in the address of a GET request.
    fn token(&self, grant: &[(&str, &str)]) -> Result<GogSession, StoreError> {
        let mut request = self
            .agent
            .get(&format!("{}/token", self.auth_service))
            .query("client_id", GALAXY_CLIENT_ID)
            .query("client_secret", GALAXY_CLIENT_SECRET);
        for (name, value) in grant {
            request = request.query(name, value);
        }
        let json = match request.call() {
            Ok(response) => response
                .into_json::<Value>()
                .map_err(|_| StoreError::Unexpected("sign-in: the answer is not JSON".into()))?,
            Err(ureq::Error::Status(status @ (400 | 401 | 403), response)) => {
                return Err(StoreError::Refused(error_of(response, status)));
            }
            Err(ureq::Error::Status(status, _)) => {
                return Err(StoreError::Unexpected(format!("sign-in: HTTP {status}")));
            }
            Err(ureq::Error::Transport(_)) => return Err(StoreError::Unreachable),
        };
        let text = |key: &str| json.get(key).and_then(Value::as_str).unwrap_or_default().to_string();
        let session = GogSession {
            access_token: text("access_token"),
            refresh_token: text("refresh_token"),
        };
        if session.access_token.is_empty() || session.refresh_token.is_empty() {
            return Err(StoreError::Unexpected("sign-in: the answer carries no session".into()));
        }
        Ok(session)
    }

    fn get(&self, session: &GogSession, url: &str, what: &str) -> Result<Value, StoreError> {
        let response = self
            .agent
            .get(url)
            .set("Authorization", &format!("Bearer {}", session.access_token))
            .call();
        match response {
            // A session GOG no longer accepts is answered with its sign-in page, not an error
            Ok(response) => response
                .into_json::<Value>()
                .map_err(|_| StoreError::Refused(format!("{what}: the answer is not JSON"))),
            Err(ureq::Error::Status(status @ (401 | 403), _)) => {
                Err(StoreError::Refused(format!("{what}: HTTP {status}")))
            }
            Err(ureq::Error::Status(status, _)) => {
                Err(StoreError::Unexpected(format!("{what}: HTTP {status}")))
            }
            Err(ureq::Error::Transport(_)) => Err(StoreError::Unreachable),
        }
    }

    /// The name of the account, to show who is signed in.
    pub fn username(&self, session: &GogSession) -> Result<String, StoreError> {
        let data = self.get(session, &format!("{}/userData.json", self.embed_service), "account")?;
        data.get("username")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_string)
            .ok_or_else(|| StoreError::Refused("account: no user name in the answer".into()))
    }

    /// The games of the account, ready to join the library as not installed. Movies, entries
    /// the user hid on GOG and games that do not run on Windows are left out.
    pub fn library(&self, session: &GogSession) -> Result<Vec<OwnedGame>, StoreError> {
        let mut games: Vec<OwnedGame> = Vec::new();
        let mut page = 1u64;
        loop {
            let url = format!(
                "{}/account/getFilteredProducts?mediaType=1&page={page}",
                self.embed_service
            );
            let answer = self.get(session, &url, "library")?;
            let products = answer
                .get("products")
                .and_then(Value::as_array)
                .ok_or_else(|| StoreError::Unexpected("library: the answer has no products".into()))?;
            for product in products {
                if let Some(game) = game_of(product) {
                    if !games.iter().any(|known| known.game.id == game.game.id) {
                        games.push(game);
                    }
                }
            }
            let total_pages = answer.get("totalPages").and_then(Value::as_u64).unwrap_or(1);
            if page >= total_pages.min(MAX_LIBRARY_PAGES) {
                break;
            }
            page += 1;
        }
        Ok(games)
    }
}

/// The library entry for a product of the account's list, if it is a game for Windows.
fn game_of(product: &Value) -> Option<OwnedGame> {
    let flag = |key: &str| product.get(key).and_then(Value::as_bool);
    if flag("isMovie") == Some(true) || flag("isGame") == Some(false) || flag("isHidden") == Some(true) {
        return None;
    }
    if product.get("worksOn").and_then(|works| works.get("Windows")).and_then(Value::as_bool) == Some(false) {
        return None;
    }
    let id = match product.get("id")? {
        Value::Number(number) => number.as_u64()?.to_string(),
        Value::String(text) if !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit()) => text.clone(),
        _ => return None,
    };
    let title = product.get("title")?.as_str()?.trim();
    if title.is_empty() {
        return None;
    }
    Some(OwnedGame::new(
        GamePlatform::Gog,
        format!("gog:{id}"),
        title,
        clean_game_title(title),
        id,
        None,
        product.get("image").and_then(Value::as_str).and_then(cover_url),
    ))
}

/// GOG names an image by an address without scheme or size (`//images-4.gog-statics.com/<id>`):
/// the size is a suffix. This is the largest of the list's landscape images; the vertical
/// box art is not part of this answer.
fn cover_url(image: &str) -> Option<String> {
    let path = image.trim().strip_prefix("//")?;
    let host = path.split('/').next()?;
    let plain = path.len() > host.len() + 1
        && (host.ends_with(".gog-statics.com") || host.ends_with(".gog.com"))
        && path
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_' | b'/'));
    plain.then(|| format!("https://{path}_392.jpg"))
}

/// GOG's own name for what went wrong (`error`), or the status when there is none.
fn error_of(response: ureq::Response, status: u16) -> String {
    response
        .into_json::<Value>()
        .ok()
        .and_then(|json| json.get("error")?.as_str().map(str::to_string))
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
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};

    /// What the mock services saw: (path with its query, Authorization header).
    pub(crate) type Seen = Arc<Mutex<Vec<(String, String)>>>;

    /// GOG's two services on one local port. Sessions are `access-N`/`refresh-N`, N growing at
    /// every sign-in or renewal; only the latest access token is accepted.
    pub(crate) fn spawn_mock_gog() -> (String, Seen) {
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
                let mut authorization = String::new();
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).is_err() || line.trim().is_empty() {
                        break;
                    }
                    if line.to_ascii_lowercase().starts_with("authorization:") {
                        authorization = line["authorization:".len()..].trim().to_string();
                    }
                }
                let path = request_line.split_whitespace().nth(1).unwrap_or("/").to_string();
                recorded.lock().unwrap().push((path.clone(), authorization.clone()));

                let identified = path.contains("client_id=46899977096215655")
                    && path.contains("client_secret=9d85c43b1482497dbbce61f6e4aa173a433796eeae2ca8c5f6129f2dc4de46d9");
                let authorized = sessions > 0 && authorization == format!("Bearer access-{sessions}");
                let (status, kind, answer): (&str, &str, String) = if path.starts_with("/token?") {
                    let good_code = path.contains("grant_type=authorization_code")
                        && path.contains("&code=good-code_123&")
                        && path.contains("redirect_uri=https%3A%2F%2Fembed.gog.com%2Fon_login_success%3Forigin%3Dclient");
                    let good_refresh = path.contains("grant_type=refresh_token")
                        && sessions > 0
                        && path.ends_with(&format!("refresh_token=refresh-{sessions}"));
                    if identified && (good_code || good_refresh) {
                        sessions += 1;
                        ("200 OK", "application/json", format!(
                            r#"{{"access_token":"access-{sessions}","refresh_token":"refresh-{sessions}","expires_in":3600,"user_id":"48628349957132247","session_id":"1"}}"#
                        ))
                    } else {
                        ("400 Bad Request", "application/json", r#"{"error":"invalid_grant","error_description":"The authorization code or refresh token is invalid"}"#.into())
                    }
                } else if !authorized {
                    // As the real service does: a web page asking to sign in, not an error code
                    ("200 OK", "text/html", "<!DOCTYPE html><html><body>Log in</body></html>".into())
                } else if path == "/userData.json" {
                    ("200 OK", "application/json", r#"{"country":"IT","isLoggedIn":true,"username":"player_one","userId":"48628349957132247"}"#.into())
                } else if path == "/account/getFilteredProducts?mediaType=1&page=1" {
                    ("200 OK", "application/json", r#"{"page":1,"totalPages":2,"totalProducts":7,"products":[
                        {"id":1207664643,"title":"The Witcher 3: Wild Hunt - Game of the Year Edition","image":"//images-4.gog-statics.com/abc123def","isGame":true,"isMovie":false,"isHidden":false,"worksOn":{"Windows":true,"Mac":false,"Linux":false}},
                        {"id":1423049311,"title":"Cyberpunk 2077","image":"//images-1.gog-statics.com/ffee00","isGame":true,"worksOn":{"Windows":true}},
                        {"id":1207666883,"title":"A Movie","image":"//images-1.gog-statics.com/movie","isGame":false,"isMovie":true},
                        {"id":1444444444,"title":"Hidden By The User","image":"//images-1.gog-statics.com/hidden","isGame":true,"isHidden":true}
                    ]}"#.into())
                } else if path == "/account/getFilteredProducts?mediaType=1&page=2" {
                    ("200 OK", "application/json", r#"{"page":2,"totalPages":2,"totalProducts":7,"products":[
                        {"id":"1207659093","title":"Beneath a Steel Sky","image":"//elsewhere.example/steal","isGame":true,"worksOn":{"Windows":true}},
                        {"id":1207658930,"title":"Mac Only","image":"//images-1.gog-statics.com/mac","isGame":true,"worksOn":{"Windows":false,"Mac":true}},
                        {"id":1423049311,"title":"Cyberpunk 2077 (again)","image":"","isGame":true},
                        {"id":0.5,"title":"Bad id"},
                        {"id":7,"title":"  "}
                    ]}"#.into())
                } else {
                    ("404 Not Found", "application/json", "{}".into())
                };

                let header = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
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

    pub(crate) fn mock_api(base_url: &str) -> GogApi {
        GogApi::with_services(base_url, base_url)
    }

    #[test]
    fn the_sign_in_page_is_gogs_own_and_asks_for_a_code() {
        assert_eq!(
            sign_in_url(),
            "https://auth.gog.com/auth?client_id=46899977096215655&redirect_uri=https%3A%2F%2Fembed.gog.com%2Fon_login_success%3Forigin%3Dclient&response_type=code&layout=client2"
        );
        assert!(!sign_in_url().contains(GALAXY_CLIENT_SECRET));
    }

    #[test]
    fn a_code_becomes_a_session_that_renews_itself() {
        let (base_url, seen) = spawn_mock_gog();
        let api = mock_api(&base_url);

        assert_eq!(api.exchange_code("wrong"), Err(StoreError::Refused("invalid_grant".into())));
        let session = api.exchange_code("good-code_123").expect("signed in");
        assert_eq!(session.refresh_token, "refresh-1");
        assert!(!format!("{session:?}").contains("access-1"), "tokens stay out of anything printed");
        assert_eq!(api.username(&session).as_deref(), Ok("player_one"));

        let renewed = api.refresh(&session.refresh_token).expect("renewed");
        assert_eq!(renewed.refresh_token, "refresh-2");
        assert_eq!(api.refresh("refresh-1"), Err(StoreError::Refused("invalid_grant".into())));

        // The service is asked the way it wants: GET, everything in the address
        let seen = seen.lock().unwrap();
        assert!(seen[1].0.starts_with("/token?client_id=46899977096215655&client_secret="));
        assert!(seen[1].0.contains("grant_type=authorization_code&code=good-code_123&redirect_uri="));
    }

    #[test]
    fn the_library_is_read_page_by_page_and_only_games_for_windows_are_kept() {
        let (base_url, _seen) = spawn_mock_gog();
        let api = mock_api(&base_url);
        let session = api.exchange_code("good-code_123").unwrap();

        let games = api.library(&session).expect("library");
        let titles: Vec<&str> = games.iter().map(|owned| owned.game.title.as_str()).collect();
        assert_eq!(
            titles,
            [
                "The Witcher 3: Wild Hunt - Game of the Year Edition",
                "Cyberpunk 2077",
                "Beneath a Steel Sky",
            ],
            "both pages; no movie, nothing hidden, nothing for another system, nothing twice or nameless"
        );

        let witcher = &games[0].game;
        assert_eq!(witcher.id, "gog:1207664643");
        assert_eq!(witcher.clean_title, "The Witcher 3: Wild Hunt");
        assert_eq!(witcher.platform, GamePlatform::Gog);
        assert!(!witcher.is_installed);
        assert_eq!(witcher.install_uri().as_deref(), Some("goggalaxy://openGameView/1207664643"));
        assert_eq!(
            witcher.cover_path.as_deref(),
            Some("https://images-4.gog-statics.com/abc123def_392.jpg")
        );
        // An id written as text is the same id; an image from elsewhere is not a cover
        assert_eq!(games[2].game.id, "gog:1207659093");
        assert_eq!(games[2].game.cover_path, None);
    }

    #[test]
    fn a_session_gog_no_longer_accepts_is_told_apart_from_a_gog_that_cannot_be_reached() {
        let (base_url, _seen) = spawn_mock_gog();
        let api = mock_api(&base_url);
        let stale = api.exchange_code("good-code_123").unwrap();
        let _newer = api.refresh(&stale.refresh_token).unwrap();

        // GOG answers a stale session with its sign-in page
        assert!(matches!(api.library(&stale), Err(StoreError::Refused(_))));
        assert!(matches!(api.username(&stale), Err(StoreError::Refused(_))));

        let dead = mock_api("http://127.0.0.1:9");
        assert_eq!(dead.exchange_code("good-code_123"), Err(StoreError::Unreachable));
        assert_eq!(dead.library(&stale), Err(StoreError::Unreachable));
    }
}
