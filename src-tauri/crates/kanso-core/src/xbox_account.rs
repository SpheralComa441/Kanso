//! The Xbox (Microsoft) account: signing in, and the PC games it has played.
//!
//! Signing in never shows Kanso the password: the user signs in on Microsoft's own page, in
//! the browser, and ends on a blank page whose address carries a short-lived authorization
//! code. The user pastes that address into Kanso, which exchanges the code for a session
//! ([`XboxApi::exchange_code`]). The session's refresh token is what is kept between runs,
//! protected by Windows for the current user (see [`crate::accounts`]). It is sent to
//! Microsoft's sign-in service only, to renew the session.
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
use serde_json::{json, Value};
use std::time::Duration;

/// Kanso's application id, as registered with Microsoft by the project's owner (a public
/// client for personal Microsoft accounts: no secret exists). A fork that registers its own
/// application only has to change this constant.
pub const CLIENT_ID: &str = "2b0d074d-ab89-4874-9ea9-43e9d2f51994";
/// Where Microsoft sends the browser after a sign-in, with the code in the address: its own
/// blank page for desktop programs.
const REDIRECT_URI: &str = "https://login.live.com/oauth20_desktop.srf";
/// Signing in to Xbox Live, and staying signed in: nothing else of the Microsoft account.
const SCOPE: &str = "Xboxlive.signin Xboxlive.offline_access";

const SIGN_IN_SERVICE: &str = "https://login.live.com";
const USER_SERVICE: &str = "https://user.auth.xboxlive.com";
const XSTS_SERVICE: &str = "https://xsts.auth.xboxlive.com";
const TITLE_SERVICE: &str = "https://titlehub.xboxlive.com";

/// How an address of the Microsoft Store that opens a game's page begins.
pub const STORE_PAGE_PREFIX: &str = "ms-windows-store://pdp/?PFN=";

/// Where the user signs in: Microsoft's own page, which ends on an address carrying the code.
/// The page is asked to let the user choose the account each time: a browser already signed
/// in to one Microsoft account would otherwise go on with it without asking, and the account
/// with the games is often another one.
pub fn sign_in_url() -> String {
    format!(
        "{SIGN_IN_SERVICE}/oauth20_authorize.srf?client_id={CLIENT_ID}&response_type=code&approval_prompt=auto&prompt=select_account&scope={}&redirect_uri={}",
        percent_encode(SCOPE),
        percent_encode(REDIRECT_URI)
    )
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
                .user_agent("KansoLauncher/0.1.0")
                .build(),
        }
    }

    /// Turns the authorization code of Microsoft's sign-in into a session.
    pub fn exchange_code(&self, code: &str) -> Result<XboxSession, StoreError> {
        self.token(&[("grant_type", "authorization_code"), ("code", code)])
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
                && form("redirect_uri", "https%3A%2F%2Flogin.live.com%2Foauth20_desktop.srf");
            match (request.method.as_str(), request.path.as_str()) {
                ("POST", "/oauth20_token.srf") if ours && form("grant_type", "authorization_code") && form("code", "M.C507_BAY.2.U.code-1") => (
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
        let url = sign_in_url();
        assert!(url.starts_with("https://login.live.com/oauth20_authorize.srf?"));
        assert!(url.contains("response_type=code"));
        assert!(url.contains("prompt=select_account"), "the user chooses which account, each time");
        assert!(url.contains("scope=Xboxlive.signin%20Xboxlive.offline_access"));
        assert!(url.contains("redirect_uri=https%3A%2F%2Flogin.live.com%2Foauth20_desktop.srf"));
        // What the user pastes back: the address Microsoft's blank page ends on
        let pasted = "https://login.live.com/oauth20_desktop.srf?code=M.C507_BAY.2.U.code-1&lc=1040";
        assert_eq!(crate::accounts::authorization_code_in(pasted).as_deref(), Some("M.C507_BAY.2.U.code-1"));
    }

    #[test]
    fn a_code_becomes_a_session_and_the_session_lists_the_games_played_on_pc() {
        let base_url = spawn_mock_xbox();
        let api = api(&base_url);

        let session = api.exchange_code("M.C507_BAY.2.U.code-1").expect("signed in");
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

        assert_eq!(api.exchange_code("wrong-code"), Err(StoreError::Refused("sign-in: invalid_grant".into())));
        assert_eq!(api.refresh("spent"), Err(StoreError::Refused("sign-in: invalid_grant".into())));

        // A Microsoft account with no Xbox profile: Xbox Live says so with its own number
        let renewed = api.refresh("refresh-1").expect("renewed");
        assert_eq!(renewed.refresh_token, "refresh-2", "the latest refresh token is the one to keep");
        assert_eq!(api.identity(&renewed), Err(StoreError::Refused("xbox authorization: XErr 2148916233".into())));

        // A proof Xbox Live no longer accepts
        let stale = XboxIdentity { xuid: "2533274800000001".into(), gamertag: String::new(), user_hash: "hash-9".into(), token: "old".into() };
        assert_eq!(api.library(&stale), Err(StoreError::Refused("library: HTTP 401".into())));

        let dead = XboxApi::with_services("http://127.0.0.1:9", "http://127.0.0.1:9", "http://127.0.0.1:9", "http://127.0.0.1:9");
        assert_eq!(dead.exchange_code("M.C507_BAY.2.U.code-1"), Err(StoreError::Unreachable));
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
