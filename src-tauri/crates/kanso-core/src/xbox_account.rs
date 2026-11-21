//! The Xbox (Microsoft) account: signing in, and the PC games it has played.
//!
//! Signing in never shows Kanso the password: the user signs in on Microsoft's own page, in
//! their own browser. When they are done, Microsoft sends the browser back to Kanso on this
//! very PC, at `http://localhost` (an address that never leaves the machine), with a
//! short-lived authorization code. This is how Microsoft wants desktop programs to sign in;
//! its page for programs that ask the user to copy the code instead removes the code from
//! the address, so that nobody can be talked into copying it. Kanso exchanges the code for a
//! session ([`XboxApi::exchange_code`]). The session's refresh token is what is kept between
//! runs, protected by Windows for the current user (see [`crate::accounts`]). It is sent to
//! Microsoft's sign-in service only, to renew the session.
//!
//! Two things keep the way back honest ([`SignInAttempt`]): a random mark Microsoft has to
//! hand back, so that Kanso only listens to the answer to the sign-in it started, and a
//! secret that never leaves Kanso and that Microsoft checks before turning the code into a
//! session (PKCE), so that a code read by another program on the PC is useless to it.
//!
//! With a session, Xbox Live is asked two things: who signed in (the gamertag, to show it)
//! and the account's title history. Everything goes to Microsoft's own services and nowhere
//! else. The requests are the ones the open-source launcher Playnite makes, in the same
//! order. Kanso presents itself with its own application id ([`CLIENT_ID`]), registered with
//! Microsoft by the project's owner: Microsoft's consent page names Kanso. The id is public
//! and names the program, not a user; there is no secret to go with it.
//!
//! **What this list is and is not.** The title history holds the games the account has
//! *started* at least once, on any device; the ones started on a PC are kept. A game bought
//! and never started is not in it, and Microsoft gives a third-party program no list of
//! purchases. Nor does it say reliably whether the account has Game Pass: the games of the
//! Game Pass catalog come from Microsoft's public catalog instead (see [`crate::gamepass`]),
//! when the user asks for them in the settings.

use crate::accounts::StoreError;
use crate::cleaner::clean_game_title;
use crate::deals::parse_iso8601_ms;
use crate::models::GamePlatform;
use crate::owned::OwnedGame;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use serde_json::{json, Value};
use std::time::Duration;

/// Kanso's application id, as registered with Microsoft by the project's owner (a public
/// client for personal Microsoft accounts: no secret exists). A fork that registers its own
/// application only has to change this constant.
pub const CLIENT_ID: &str = "2b0d074d-ab89-4874-9ea9-43e9d2f51994";
/// The port Kanso listens on, on this PC only, while a sign-in is under way: where Microsoft
/// sends the browser back. It is part of the address registered with Microsoft for
/// [`CLIENT_ID`], so it cannot change without changing the registration.
pub const CALLBACK_PORT: u16 = 42813;
const CALLBACK_PATH: &str = "/xbox";
/// Where Microsoft sends the browser after a sign-in, with the code in the address: Kanso
/// itself, on this PC.
const REDIRECT_URI: &str = "http://localhost:42813/xbox";
/// Signing in to Xbox Live, and staying signed in: nothing else of the Microsoft account.
const SCOPE: &str = "Xboxlive.signin Xboxlive.offline_access";

const SIGN_IN_SERVICE: &str = "https://login.live.com";
const USER_SERVICE: &str = "https://user.auth.xboxlive.com";
const XSTS_SERVICE: &str = "https://xsts.auth.xboxlive.com";
const TITLE_SERVICE: &str = "https://titlehub.xboxlive.com";

/// How an address of the Microsoft Store that opens a game's page begins.
pub const STORE_PAGE_PREFIX: &str = "ms-windows-store://pdp/?PFN=";

/// One sign-in the user started: what Microsoft has to hand back, and what it has to be
/// shown, for the answer to count.
#[derive(Clone)]
pub struct SignInAttempt {
    /// Sent along and expected back unchanged: an answer without it is not to this attempt.
    state: String,
    /// Never sent until the code is exchanged; Microsoft only gets its fingerprint first.
    verifier: String,
}

impl std::fmt::Debug for SignInAttempt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SignInAttempt").finish_non_exhaustive()
    }
}

impl SignInAttempt {
    /// A new attempt, with marks nobody can guess. `None` only if the system gives no
    /// random numbers.
    pub fn new() -> Option<Self> {
        let random = |length: usize| {
            let mut bytes = vec![0u8; length];
            ring::rand::SecureRandom::fill(&ring::rand::SystemRandom::new(), &mut bytes).ok()?;
            Some(URL_SAFE_NO_PAD.encode(bytes))
        };
        Some(Self { state: random(18)?, verifier: random(48)? })
    }

    /// Where the user signs in: Microsoft's own page. The page is asked to let the user
    /// choose the account each time: a browser already signed in to one Microsoft account
    /// would otherwise go on with it without asking, and the account with the games is
    /// often another one.
    pub fn sign_in_url(&self) -> String {
        let challenge = URL_SAFE_NO_PAD.encode(ring::digest::digest(&ring::digest::SHA256, self.verifier.as_bytes()));
        format!(
            "{SIGN_IN_SERVICE}/oauth20_authorize.srf?client_id={CLIENT_ID}&response_type=code&prompt=select_account&scope={}&redirect_uri={}&state={}&code_challenge={challenge}&code_challenge_method=S256",
            percent_encode(SCOPE),
            percent_encode(REDIRECT_URI),
            self.state
        )
    }

    /// What a request received on [`CALLBACK_PORT`] means for this attempt. `request_line`
    /// is the first line of the request (`GET /xbox?code=...&state=... HTTP/1.1`).
    pub fn read_callback(&self, request_line: &str) -> Callback {
        let mut parts = request_line.split_whitespace();
        let (Some("GET"), Some(target)) = (parts.next(), parts.next()) else {
            return Callback::NotOurs;
        };
        let Some(query) = target.strip_prefix(CALLBACK_PATH).and_then(|rest| rest.strip_prefix('?')) else {
            return Callback::NotOurs;
        };
        let field = |name: &str| {
            query
                .split('&')
                .filter_map(|pair| pair.split_once('='))
                .find(|(key, _)| *key == name)
                .map(|(_, value)| percent_decode(value))
        };
        // Anything on this PC can ask this address for a page: only the answer that carries
        // the mark of this attempt is Microsoft's answer to it
        if field("state").as_deref() != Some(self.state.as_str()) {
            return Callback::NotOurs;
        }
        if let Some(error) = field("error") {
            let plain: String = error.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '_').take(60).collect();
            return Callback::Refused(plain);
        }
        match field("code") {
            Some(code)
                if (8..=2048).contains(&code.len())
                    && code.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'!' | b'*' | b'$')) =>
            {
                Callback::Code(code)
            }
            _ => Callback::NotOurs,
        }
    }
}

/// What a request on the callback port turned out to be.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Callback {
    /// Microsoft's answer to this attempt: the authorization code.
    Code(String),
    /// Microsoft's answer to this attempt: no (the user declined, or Microsoft refused), with
    /// its own short name for why.
    Refused(String),
    /// Something else: not answered to, not acted upon.
    NotOurs,
}

/// A signed-in Microsoft session.
#[derive(Clone, PartialEq, Eq)]
pub struct XboxSession {
    access_token: String,
    /// Keeps the session going between runs; whatever the latest answer carried.
    pub refresh_token: String,
}

impl std::fmt::Debug for XboxSession {
    /// Tokens never reach a log or an error message.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("XboxSession").finish_non_exhaustive()
    }
}

/// Who signed in, as Xbox Live knows them, and what its services accept as proof.
#[derive(Clone, PartialEq, Eq)]
pub struct XboxIdentity {
    /// The account's number on Xbox Live.
    pub xuid: String,
    pub gamertag: String,
    user_hash: String,
    token: String,
}

impl std::fmt::Debug for XboxIdentity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("XboxIdentity").field("gamertag", &self.gamertag).finish_non_exhaustive()
    }
}

/// Client for the Microsoft services Kanso uses.
#[derive(Debug, Clone)]
pub struct XboxApi {
    sign_in: String,
    user: String,
    xsts: String,
    titles: String,
    agent: ureq::Agent,
}

impl Default for XboxApi {
    fn default() -> Self {
        Self::new()
    }
}

impl XboxApi {
    pub fn new() -> Self {
        Self::with_services(SIGN_IN_SERVICE, USER_SERVICE, XSTS_SERVICE, TITLE_SERVICE)
    }

    /// Client against other endpoints (tests).
    pub fn with_services(sign_in: &str, user: &str, xsts: &str, titles: &str) -> Self {
        let clean = |address: &str| address.trim_end_matches('/').to_string();
        Self {
            sign_in: clean(sign_in),
            user: clean(user),
            xsts: clean(xsts),
            titles: clean(titles),
            agent: ureq::AgentBuilder::new()
                .timeout_connect(Duration::from_secs(8))
                .timeout_read(Duration::from_secs(25))
                .user_agent(crate::USER_AGENT)
                .build(),
        }
    }

    /// Turns the authorization code of Microsoft's sign-in into a session, showing Microsoft
    /// the secret of the attempt the code answers.
    pub fn exchange_code(&self, code: &str, attempt: &SignInAttempt) -> Result<XboxSession, StoreError> {
        self.token(&[("grant_type", "authorization_code"), ("code", code), ("code_verifier", &attempt.verifier)])
    }

    /// Renews a session from its refresh token.
    pub fn refresh(&self, refresh_token: &str) -> Result<XboxSession, StoreError> {
        self.token(&[("grant_type", "refresh_token"), ("refresh_token", refresh_token)])
    }

    fn token(&self, grant: &[(&str, &str)]) -> Result<XboxSession, StoreError> {
        let mut form: Vec<(&str, &str)> = grant.to_vec();
        form.extend([("scope", SCOPE), ("client_id", CLIENT_ID), ("redirect_uri", REDIRECT_URI)]);
        let response = self.agent.post(&format!("{}/oauth20_token.srf", self.sign_in)).send_form(&form);
        let json = read(response, "sign-in")?;
        let text = |key: &str| json.get(key).and_then(Value::as_str).unwrap_or_default().to_string();
        let session = XboxSession { access_token: text("access_token"), refresh_token: text("refresh_token") };
        if session.access_token.is_empty() || session.refresh_token.is_empty() {
            return Err(StoreError::Unexpected("sign-in: the answer carries no session".into()));
        }
        Ok(session)
    }

    /// Signs the Microsoft session in to Xbox Live: who it is, and the proof Xbox Live's
    /// services ask for. An account that never set up an Xbox profile, or a child's account
    /// without consent, is refused here by Microsoft, with its own error number.
    pub fn identity(&self, session: &XboxSession) -> Result<XboxIdentity, StoreError> {
        let user = read(
            self.agent
                .post(&format!("{}/user/authenticate", self.user))
                .set("x-xbl-contract-version", "1")
                .send_json(json!({
                    "RelyingParty": "http://auth.xboxlive.com",
                    "TokenType": "JWT",
                    "Properties": {
                        "AuthMethod": "RPS",
                        "SiteName": "user.auth.xboxlive.com",
                        "RpsTicket": format!("d={}", session.access_token),
                    },
                })),
            "xbox sign-in",
        )?;
        let user_token = user
            .get("Token")
            .and_then(Value::as_str)
            .filter(|token| !token.is_empty())
            .ok_or_else(|| StoreError::Unexpected("xbox sign-in: the answer carries no token".into()))?;

        let authorized = read(
            self.agent
                .post(&format!("{}/xsts/authorize", self.xsts))
                .set("x-xbl-contract-version", "1")
                .send_json(json!({
                    "RelyingParty": "http://xboxlive.com",
                    "TokenType": "JWT",
                    "Properties": { "SandboxId": "RETAIL", "UserTokens": [user_token] },
                })),
            "xbox authorization",
        )?;
        let claim = |key: &str| {
            authorized
                .pointer("/DisplayClaims/xui/0")
                .and_then(|claims| claims.get(key))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        let identity = XboxIdentity {
            xuid: claim("xid"),
            gamertag: claim("gtg"),
            user_hash: claim("uhs"),
            token: authorized.get("Token").and_then(Value::as_str).unwrap_or_default().to_string(),
        };
        let plain_xuid = !identity.xuid.is_empty() && identity.xuid.bytes().all(|b| b.is_ascii_digit());
        if !plain_xuid || identity.user_hash.is_empty() || identity.token.is_empty() {
            return Err(StoreError::Unexpected("xbox authorization: the answer names no account".into()));
        }
        Ok(identity)
    }

    /// The PC games the account has started at least once, ready to join the library as not
    /// installed. Console-only titles, apps and what has no Store package are left out.
    pub fn library(&self, identity: &XboxIdentity) -> Result<Vec<OwnedGame>, StoreError> {
        let url = format!(
            "{}/users/xuid({})/titles/titlehistory/decoration/detail,image",
            self.titles, identity.xuid
        );
        let answer = read(
            self.agent
                .get(&url)
                .set("x-xbl-contract-version", "2")
                .set("Authorization", &format!("XBL3.0 x={};{}", identity.user_hash, identity.token))
                .set("Accept-Language", "en-US")
                .call(),
            "library",
        )?;
        let titles = answer
            .get("titles")
            .and_then(Value::as_array)
            .ok_or_else(|| StoreError::Unexpected("library: the answer lists no titles".into()))?;
        let mut games: Vec<OwnedGame> = Vec::new();
        for title in titles {
            if let Some(game) = game_of(title) {
                if !games.iter().any(|known| known.game.id.eq_ignore_ascii_case(&game.game.id)) {
                    games.push(game);
                }
            }
        }
        Ok(games)
    }
}

/// The part of a package family name that names the package (`Publisher.Game` of
/// `Publisher.Game_8wekyb3d8bbwe`): what the scanner knows an installed game by. `None` when
/// the name is not a plain package family name.
pub fn package_identity(family_name: &str) -> Option<&str> {
    let family_name = family_name.trim();
    let plain = (3..=128).contains(&family_name.len())
        && family_name.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_'));
    let identity = family_name.split('_').next().unwrap_or_default();
    (plain && !identity.is_empty()).then_some(identity)
}

/// The library entry for a title of the account's history, if it is a game played on a PC.
fn game_of(title: &Value) -> Option<OwnedGame> {
    let text = |key: &str| title.get(key).and_then(Value::as_str).map(str::trim).unwrap_or_default();
    if !text("type").eq_ignore_ascii_case("Game") {
        return None;
    }
    let on_pc = title
        .get("devices")
        .and_then(Value::as_array)
        .is_some_and(|devices| devices.iter().any(|device| device.as_str() == Some("PC")));
    if !on_pc {
        return None;
    }
    // The Store package: what the game is installed and found by
    let family_name = text("pfn");
    let identity = package_identity(family_name)?;
    let name = text("name");
    if name.is_empty() || name.len() > 200 {
        return None;
    }
    let cover = Some(text("displayImage"))
        .filter(|url| url.starts_with("https://") && url.len() <= 400)
        .map(str::to_string);
    let mut owned = OwnedGame::new(
        GamePlatform::Xbox,
        format!("xbox:{identity}"),
        name,
        clean_game_title(name),
        family_name.to_string(),
        Some(format!("{STORE_PAGE_PREFIX}{family_name}")),
        cover,
    );
    owned.game.last_played_at = title
        .pointer("/titleHistory/lastTimePlayed")
        .and_then(Value::as_str)
        .and_then(parse_iso8601_ms);
    Some(owned)
}

/// The JSON of an answer, or why there is none. Microsoft's sign-in names what went wrong in
/// `error`; Xbox Live in `XErr`, a number.
fn read(response: Result<ureq::Response, ureq::Error>, what: &str) -> Result<Value, StoreError> {
    match response {
        Ok(response) => response
            .into_json::<Value>()
            .map_err(|_| StoreError::Unexpected(format!("{what}: the answer is not JSON"))),
        Err(ureq::Error::Status(status @ (400 | 401 | 403), response)) => {
            let json = response.into_json::<Value>().ok();
            let named = json.as_ref().and_then(|json| {
                json.get("error")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .or_else(|| json.get("XErr").and_then(Value::as_u64).map(|code| format!("XErr {code}")))
            });
            let detail = named
                .filter(|code| !code.is_empty() && code.len() <= 80)
                .unwrap_or_else(|| format!("HTTP {status}"));
            Err(StoreError::Refused(format!("{what}: {detail}")))
        }
        Err(ureq::Error::Status(status, _)) => Err(StoreError::Unexpected(format!("{what}: HTTP {status}"))),
        Err(ureq::Error::Transport(_)) => Err(StoreError::Unreachable),
    }
}

/// Text with the `%XX` escapes of an address turned back into the characters they stand for.
fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut position = 0;
    while position < bytes.len() {
        let escaped = (bytes[position] == b'%')
            .then(|| text.get(position + 1..position + 3))
            .flatten()
            .and_then(|hex| u8::from_str_radix(hex, 16).ok());
        match escaped {
            Some(byte) => {
                out.push(byte);
                position += 3;
            }
            None => {
                out.push(bytes[position]);
                position += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
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
    use crate::test_http::{spawn, Request};

    /// Microsoft's services on a local port, answering as the real ones do.
    pub(crate) fn spawn_mock_xbox() -> String {
        spawn(|request: &Request| {
            let form = |name: &str, value: &str| request.body.split('&').any(|pair| pair == format!("{name}={value}"));
            let ours = form("client_id", CLIENT_ID)
                && form("scope", "Xboxlive.signin+Xboxlive.offline_access")
                && form("redirect_uri", "http%3A%2F%2Flocalhost%3A42813%2Fxbox");
            // The secret of the attempt: forty-eight random bytes, as sixty-four characters
            let verified = request.body.split('&').any(|pair| pair.strip_prefix("code_verifier=").is_some_and(|secret| secret.len() == 64));
            match (request.method.as_str(), request.path.as_str()) {
                ("POST", "/oauth20_token.srf") if ours && verified && form("grant_type", "authorization_code") && form("code", "M.C507_BAY.2.U.code-1") => (
                    200,
                    r#"{"token_type":"bearer","expires_in":3600,"scope":"XboxLive.signin XboxLive.offline_access","access_token":"access-1","refresh_token":"refresh-1","user_id":"abc"}"#.to_string(),
                ),
                ("POST", "/oauth20_token.srf") if ours && form("grant_type", "refresh_token") && form("refresh_token", "refresh-1") => (
                    200,
                    r#"{"token_type":"bearer","expires_in":3600,"access_token":"access-2","refresh_token":"refresh-2"}"#.to_string(),
                ),
                ("POST", "/oauth20_token.srf") => (400, r#"{"error":"invalid_grant","error_description":"The provided value for the 'code' parameter is not valid."}"#.to_string()),
                ("POST", "/user/authenticate") if request.header("x-xbl-contract-version") == "1" => {
                    let body: Value = serde_json::from_str(&request.body).unwrap_or_default();
                    let ticket = body.pointer("/Properties/RpsTicket").and_then(Value::as_str).unwrap_or_default();
                    let well_formed = body["RelyingParty"] == "http://auth.xboxlive.com"
                        && body["TokenType"] == "JWT"
                        && body["Properties"]["AuthMethod"] == "RPS"
                        && body["Properties"]["SiteName"] == "user.auth.xboxlive.com";
                    match ticket {
                        "d=access-1" | "d=access-2" if well_formed => (200, format!(r#"{{"Token":"user-token-{}","DisplayClaims":{{"xui":[{{"uhs":"hash-9"}}]}}}}"#, &ticket[9..])),
                        _ => (401, String::new()),
                    }
                }
                ("POST", "/xsts/authorize") => {
                    let body: Value = serde_json::from_str(&request.body).unwrap_or_default();
                    let well_formed = body["RelyingParty"] == "http://xboxlive.com" && body["Properties"]["SandboxId"] == "RETAIL";
                    match body.pointer("/Properties/UserTokens/0").and_then(Value::as_str) {
                        Some("user-token-1") if well_formed => (
                            200,
                            r#"{"Token":"xsts-token","DisplayClaims":{"xui":[{"gtg":"PlayerOne","xid":"2533274800000001","uhs":"hash-9","agg":"Adult"}]}}"#.to_string(),
                        ),
                        // An account with no Xbox profile
                        Some("user-token-2") if well_formed => (401, r#"{"Identity":"0","XErr":2148916233,"Message":"","Redirect":"https://start.ui.xboxlive.com/CreateAccount"}"#.to_string()),
                        _ => (400, String::new()),
                    }
                }
                ("GET", "/users/xuid(2533274800000001)/titles/titlehistory/decoration/detail,image")
                    if request.header("authorization") == "XBL3.0 x=hash-9;xsts-token" && request.header("x-xbl-contract-version") == "2" =>
                {
                    (200, r#"{"xuid":"2533274800000001","titles":[
                        {"titleId":"1717113201","pfn":"Microsoft.624F8B84B80_8wekyb3d8bbwe","type":"Game","name":"Halo Infinite","devices":["XboxSeries","PC","XboxOne"],"displayImage":"https://store-images.s-microsoft.com/image/apps.21536.jpg","titleHistory":{"lastTimePlayed":"2026-10-02T18:22:11.123Z"},"gamePass":{"isGamePass":true}},
                        {"titleId":"2","pfn":"Microsoft.SeaofThieves_8wekyb3d8bbwe","type":"Game","name":"Sea of Thieves: 2026 Edition","devices":["PC"],"displayImage":"http://insecure.example/sot.jpg","titleHistory":null},
                        {"titleId":"3","pfn":"Microsoft.Forza_8wekyb3d8bbwe","type":"Game","name":"Forza Horizon 5","devices":["XboxSeries"]},
                        {"titleId":"4","pfn":"4DF9E0F8.Netflix_mcm4njqhnhss8","type":"Application","name":"Netflix","devices":["PC"]},
                        {"titleId":"5","pfn":null,"type":"Game","name":"A Game Outside The Store","devices":["Win32"]},
                        {"titleId":"6","pfn":"MICROSOFT.624F8B84B80_8wekyb3d8bbwe","type":"Game","name":"Halo Infinite again","devices":["PC"]},
                        {"titleId":"7","pfn":"Bad family name!","type":"Game","name":"Bad package","devices":["PC"]},
                        {"titleId":"8","pfn":"Publisher.Nameless_abc","type":"Game","name":"  ","devices":["PC"]}
                    ]}"#.to_string())
                }
                ("GET", _) => (401, String::new()),
                _ => (404, "{}".to_string()),
            }
        })
    }

    fn api(base_url: &str) -> XboxApi {
        XboxApi::with_services(base_url, base_url, base_url, base_url)
    }

    #[test]
    fn the_sign_in_address_is_microsofts_own_and_asks_for_xbox_live_only() {
        let attempt = SignInAttempt::new().expect("random numbers");
        let url = attempt.sign_in_url();
        assert!(url.starts_with("https://login.live.com/oauth20_authorize.srf?"));
        assert!(url.contains("response_type=code"));
        assert!(url.contains("prompt=select_account"), "the user chooses which account, each time");
        assert!(url.contains("scope=Xboxlive.signin%20Xboxlive.offline_access"));
        assert!(url.contains("redirect_uri=http%3A%2F%2Flocalhost%3A42813%2Fxbox"), "back to this PC, nowhere else");
        assert_eq!(REDIRECT_URI, format!("http://localhost:{CALLBACK_PORT}{CALLBACK_PATH}"));
        assert!(url.contains(&format!("state={}", attempt.state)));
        assert!(url.contains("code_challenge_method=S256"));
        assert!(!url.contains(&attempt.verifier), "the secret stays in Kanso until the code is exchanged");
        assert_eq!(format!("{attempt:?}"), "SignInAttempt { .. }");

        // Each attempt has its own marks
        let other = SignInAttempt::new().unwrap();
        assert_ne!(attempt.state, other.state);
        assert_ne!(attempt.verifier, other.verifier);
    }

    #[test]
    fn only_microsofts_answer_to_this_attempt_is_listened_to() {
        let attempt = SignInAttempt { state: "mark-1".into(), verifier: "v".repeat(64) };
        let line = |target: &str| format!("GET {target} HTTP/1.1");

        assert_eq!(
            attempt.read_callback(&line("/xbox?code=M.C507_BAY.2.U.code-1&state=mark-1")),
            Callback::Code("M.C507_BAY.2.U.code-1".into())
        );
        assert_eq!(
            attempt.read_callback(&line("/xbox?state=mark-1&code=M.C507%21abc%2Adef")),
            Callback::Code("M.C507!abc*def".into()),
            "escapes of the address are undone"
        );
        assert_eq!(
            attempt.read_callback(&line("/xbox?error=access_denied&error_description=The%20user%20has%20denied%20access&state=mark-1")),
            Callback::Refused("access_denied".into())
        );

        // Another attempt's answer, no mark, another page, another kind of request, junk
        for other in [
            "/xbox?code=M.C507_BAY.2.U.code-1&state=mark-2",
            "/xbox?code=M.C507_BAY.2.U.code-1",
            "/xbox?error=access_denied",
            "/favicon.ico",
            "/?code=M.C507_BAY.2.U.code-1&state=mark-1",
            "/xbox?state=mark-1",
            "/xbox?state=mark-1&code=short",
            "/xbox?state=mark-1&code=with%20spaces%20inside",
        ] {
            assert_eq!(attempt.read_callback(&line(other)), Callback::NotOurs, "{other}");
        }
        assert_eq!(attempt.read_callback("POST /xbox?code=M.C507_BAY.2.U.code-1&state=mark-1 HTTP/1.1"), Callback::NotOurs);
        assert_eq!(attempt.read_callback(""), Callback::NotOurs);
    }

    #[test]
    fn a_code_becomes_a_session_and_the_session_lists_the_games_played_on_pc() {
        let base_url = spawn_mock_xbox();
        let api = api(&base_url);

        let attempt = SignInAttempt::new().unwrap();
        let session = api.exchange_code("M.C507_BAY.2.U.code-1", &attempt).expect("signed in");
        assert_eq!(session.refresh_token, "refresh-1");
        assert_eq!(format!("{session:?}"), "XboxSession { .. }", "tokens are never printed");

        let identity = api.identity(&session).expect("an Xbox profile");
        assert_eq!(identity.gamertag, "PlayerOne");
        assert_eq!(identity.xuid, "2533274800000001");
        assert!(!format!("{identity:?}").contains("xsts-token"));

        let games = api.library(&identity).expect("library");
        let listed: Vec<(&str, &str)> = games.iter().map(|owned| (owned.game.id.as_str(), owned.game.title.as_str())).collect();
        assert_eq!(
            listed,
            [
                ("xbox:Microsoft.624F8B84B80", "Halo Infinite"),
                ("xbox:Microsoft.SeaofThieves", "Sea of Thieves: 2026 Edition"),
            ],
            "games started on a PC, each once; no console-only titles, apps or games outside the Store"
        );
        let halo = &games[0].game;
        assert_eq!(halo.platform, GamePlatform::Xbox);
        assert!(!halo.is_installed);
        assert_eq!(halo.platform_app_id.as_deref(), Some("Microsoft.624F8B84B80_8wekyb3d8bbwe"));
        assert_eq!(halo.install_uri().as_deref(), Some("ms-windows-store://pdp/?PFN=Microsoft.624F8B84B80_8wekyb3d8bbwe"));
        assert_eq!(halo.cover_path.as_deref(), Some("https://store-images.s-microsoft.com/image/apps.21536.jpg"));
        assert_eq!(halo.last_played_at, parse_iso8601_ms("2026-10-02T18:22:11Z"));
        assert_eq!(games[1].game.cover_path, None, "only secure addresses become covers");
        assert_eq!(games[1].game.last_played_at, None);
    }

    #[test]
    fn what_microsoft_refuses_is_told_apart_from_what_cannot_be_reached() {
        let base_url = spawn_mock_xbox();
        let api = api(&base_url);

        let attempt = SignInAttempt::new().unwrap();
        assert_eq!(api.exchange_code("wrong-code", &attempt), Err(StoreError::Refused("sign-in: invalid_grant".into())));
        assert_eq!(api.refresh("spent"), Err(StoreError::Refused("sign-in: invalid_grant".into())));

        // A Microsoft account with no Xbox profile: Xbox Live says so with its own number
        let renewed = api.refresh("refresh-1").expect("renewed");
        assert_eq!(renewed.refresh_token, "refresh-2", "the latest refresh token is the one to keep");
        assert_eq!(api.identity(&renewed), Err(StoreError::Refused("xbox authorization: XErr 2148916233".into())));

        // A proof Xbox Live no longer accepts
        let stale = XboxIdentity { xuid: "2533274800000001".into(), gamertag: String::new(), user_hash: "hash-9".into(), token: "old".into() };
        assert_eq!(api.library(&stale), Err(StoreError::Refused("library: HTTP 401".into())));

        let dead = XboxApi::with_services("http://127.0.0.1:9", "http://127.0.0.1:9", "http://127.0.0.1:9", "http://127.0.0.1:9");
        assert_eq!(dead.exchange_code("M.C507_BAY.2.U.code-1", &attempt), Err(StoreError::Unreachable));
    }

    #[test]
    fn a_package_is_known_by_the_name_before_the_publishers_mark() {
        assert_eq!(package_identity("Microsoft.624F8B84B80_8wekyb3d8bbwe"), Some("Microsoft.624F8B84B80"));
        assert_eq!(package_identity(" BethesdaSoftworks.Starfield_3275kfvn8vcwc "), Some("BethesdaSoftworks.Starfield"));
        assert_eq!(package_identity("NoMark"), Some("NoMark"));
        for odd in ["", "_mark", "a b_c", "../x_y", "x/y_z"] {
            assert_eq!(package_identity(odd), None, "{odd:?}");
        }
    }
}
