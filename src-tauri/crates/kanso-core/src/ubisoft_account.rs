//! The Ubisoft Connect account: the session its sign-in page hands out, and the library.
//!
//! A current Ubisoft Connect client encrypts its local list of games (see
//! [`crate::scanner::ubisoft_cache`]), so the games of the account that are not installed can
//! only come from the account itself. Ubisoft has no public API for that and no sign-in that
//! ends on a code to paste: its sign-in page keeps the session it obtained in the page's own
//! storage. Kanso therefore shows that page in a window of its own (see [`crate::signin`] for
//! what that window may and may not do) and, **once Ubisoft has signed the user in**, reads
//! the session from the page with [`READ_UBISOFT_SESSION_SCRIPT`]. The password is typed into
//! Ubisoft's page and goes to Ubisoft alone: Kanso does not read the form, and nothing here
//! ever sees or stores it.
//!
//! What is kept is the session: the tickets Ubisoft issued, protected by Windows for the
//! current user in `accounts.json` (see [`crate::accounts`]). They are sent to Ubisoft's
//! services only, to renew the session and to ask for the list of games.
//!
//! What is read is what the open-source launcher Lutris reads, from the same place, and the
//! requests that follow are the ones it makes, with the application id of Ubisoft's own
//! "Club" web client ([`CLUB_APP_ID`]): it names the program, not the user.

use crate::accounts::StoreError;
use crate::cleaner::clean_game_title;
use crate::models::GamePlatform;
use crate::owned::OwnedGame;
use crate::signin::{on_site, SignInFlow, Step, READ_UBISOFT_SESSION_SCRIPT};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Duration;
use url::Url;

/// Application id of Ubisoft's "Club" web client, sent with every request.
pub const CLUB_APP_ID: &str = "82b650c0-6cb3-40c0-9f41-25a53b62b206";
const CLUB_GENOME_ID: &str = "42d07c95-9914-4450-8b38-267c4e462b21";
/// Ubisoft's services answer browsers: the requests name a common one.
const BROWSER_USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36";

const UBISERVICES: &str = "https://public-ubiservices.ubi.com";

/// Ubisoft's sign-in site.
const CONNECT_SITE: &str = "https://connect.ubisoft.com";
/// The address of that site Ubisoft sends the browser to once the user is signed in. Nothing
/// is there to read (it answers "not found"): reaching it is the sign that the sign-in is
/// over.
const SIGNED_IN_PATH: &str = "/ready";
/// A page of the same site with nothing on it, where the session the site keeps can be read
/// (the one Lutris reads it on).
const BLANK_PATH: &str = "/change_domain/";
const SIGN_IN_PATH: &str = "/login";
/// Readings of a page that is not one of the site's (an error page) before moving on.
const UNREADABLE_LIMIT: u8 = 2;
/// Readings of a page of the site that holds no session yet before moving on.
const EMPTY_LIMIT: u8 = 6;
/// Times the sign-in address is loaded afresh to read the session there.
const FRESH_SIGN_IN_PAGES: u8 = 3;
/// Times the window is sent to the blank page.
const BLANK_PAGE_REQUESTS: u8 = 2;

/// Ubisoft's own sign-in page, asked to end on its "ready" address.
pub fn sign_in_url() -> String {
    sign_in_url_at(CONNECT_SITE)
}

fn sign_in_url_at(site: &str) -> String {
    let ready: String = url::form_urlencoded::byte_serialize(format!("{site}{SIGNED_IN_PATH}").as_bytes()).collect();
    format!("{site}{SIGN_IN_PATH}?appId={CLUB_APP_ID}&genomeId={CLUB_GENOME_ID}&lang=en-US&nextUrl={ready}")
}

/// Where the sign-in stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stage {
    /// The user is on Ubisoft's sign-in page: nothing is run, nothing is read.
    SigningIn,
    /// Signed in; the window was sent to the blank page to read the session there.
    BlankPage,
    /// The blank page could not be read: the sign-in address was loaded afresh (a new page,
    /// in which nothing has been typed), to read the session once, as soon as it shows.
    FreshSignInPage(u8),
    /// Nothing more to try.
    GivenUp,
}

/// The sign-in on Ubisoft's page.
///
/// Nothing happens until Ubisoft sends the window to its "ready" address, which it does once
/// the user is signed in. Only then is the session read, where the site keeps it for itself
/// (the page's own storage): on a blank page of the site or, if that page cannot be read,
/// on the sign-in address loaded afresh, once, the moment it shows. A page the user has
/// typed into is never read: the sign-in address is read only after another page (the blank
/// one, or the browser's error page in its place) has taken the place of the one the user
/// signed in on.
pub struct UbisoftFlow {
    site: String,
    stage: Stage,
    blank_requests: u8,
    unreadable: u8,
    empty: u8,
    /// What the storage held the last time it was read (entries and field names, no values).
    seen: String,
}

impl Default for UbisoftFlow {
    fn default() -> Self {
        Self::new()
    }
}

impl UbisoftFlow {
    pub fn new() -> Self {
        Self::at(CONNECT_SITE)
    }

    /// The same steps against another site (tests).
    pub fn at(site: &str) -> Self {
        Self {
            site: site.trim_end_matches('/').to_string(),
            stage: Stage::SigningIn,
            blank_requests: 0,
            unreadable: 0,
            empty: 0,
            seen: "never read".into(),
        }
    }

    fn at_path(&self, url: &Url, path: &str) -> bool {
        on_site(url, &self.site) && url.path().trim_end_matches('/') == path.trim_end_matches('/')
    }

    fn blank_page(&mut self) -> Step {
        if self.blank_requests >= BLANK_PAGE_REQUESTS {
            return self.fresh_sign_in_page(0);
        }
        self.blank_requests += 1;
        self.stage = Stage::BlankPage;
        Step::Go(format!("{}{BLANK_PATH}", self.site))
    }

    /// Loads the sign-in address afresh to read the session there, or gives up.
    fn fresh_sign_in_page(&mut self, tried: u8) -> Step {
        if tried >= FRESH_SIGN_IN_PAGES {
            self.stage = Stage::GivenUp;
            return Step::Failed(format!("Ubisoft's page keeps no session where it is expected ({})", self.seen));
        }
        self.stage = Stage::FreshSignInPage(tried + 1);
        Step::Go(sign_in_url_at(&self.site))
    }
}

impl SignInFlow for UbisoftFlow {
    fn start_url(&self) -> String {
        sign_in_url_at(&self.site)
    }

    /// Ubisoft sends the window to its "ready" address once the user is signed in. The page
    /// there may never show as such (it answers "not found"): setting off for it is enough.
    fn setting_off(&mut self, url: &Url) -> Step {
        if self.stage == Stage::SigningIn && self.at_path(url, SIGNED_IN_PATH) {
            return self.blank_page();
        }
        Step::Wait
    }

    fn page_loaded(&mut self, url: &Url) -> Step {
        let at_ready = self.at_path(url, SIGNED_IN_PATH);
        let at_blank = on_site(url, &self.site) && url.path().starts_with(BLANK_PATH.trim_end_matches('/'));
        match self.stage {
            // Signed in: Ubisoft sent the window to its "ready" address
            Stage::SigningIn if at_ready => self.blank_page(),
            Stage::SigningIn => Step::Wait,
            Stage::BlankPage if at_blank => Step::Read(READ_UBISOFT_SESSION_SCRIPT),
            Stage::BlankPage if at_ready => self.blank_page(),
            // The sign-in page reported once more as the window leaves it: it is not a new
            // page, and is not read
            Stage::BlankPage if self.at_path(url, SIGN_IN_PATH) => Step::Wait,
            // The blank page led somewhere else
            Stage::BlankPage => self.fresh_sign_in_page(0),
            Stage::FreshSignInPage(_) if self.at_path(url, SIGN_IN_PATH) => Step::Read(READ_UBISOFT_SESSION_SCRIPT),
            // The fresh page went on its way before the session could be read on it
            Stage::FreshSignInPage(tried) if at_ready => self.fresh_sign_in_page(tried),
            Stage::FreshSignInPage(_) | Stage::GivenUp => Step::Wait,
        }
    }

    fn script_returned(&mut self, _url: &Url, result: &str) -> Step {
        if let Some(session) = session_from_page(result).and_then(|session| serde_json::to_string(&session).ok()) {
            return Step::Captured(session);
        }
        let entries = storage_entries(result);
        self.seen = describe_storage(entries.as_deref());
        match self.stage {
            Stage::BlankPage => {
                // An error page is not a page of the site: its storage cannot be read at all
                match entries {
                    None => self.unreadable += 1,
                    Some(_) => self.empty += 1,
                }
                if self.unreadable >= UNREADABLE_LIMIT || self.empty >= EMPTY_LIMIT {
                    return self.fresh_sign_in_page(0);
                }
                Step::Retry
            }
            // One reading per fresh page, the moment it shows: a page the user could start
            // typing into is not read again
            Stage::FreshSignInPage(tried) => self.fresh_sign_in_page(tried),
            Stage::SigningIn | Stage::GivenUp => Step::Wait,
        }
    }
}

/// The three storage entries in what [`READ_UBISOFT_SESSION_SCRIPT`] returned, each parsed
/// (or null); `None` when the script could not read the storage at all. The value arrives as
/// JSON text of a list of JSON texts (or nulls); a web view may wrap it once more.
fn storage_entries(result: &str) -> Option<Vec<Value>> {
    let mut value: Value = serde_json::from_str(result).ok()?;
    // Unwrap the layers of JSON-in-a-string until the list shows
    for _ in 0..3 {
        match value {
            Value::String(text) => value = serde_json::from_str(&text).ok()?,
            _ => break,
        }
    }
    let entries = value
        .as_array()?
        .iter()
        .map(|entry| match entry {
            Value::String(text) => serde_json::from_str(text).unwrap_or(Value::Null),
            Value::Object(_) => entry.clone(),
            _ => Value::Null,
        })
        .collect();
    Some(entries)
}

/// What the storage held, for the message of a sign-in that fails: which entries were there
/// and the names of their fields. Never a value.
fn describe_storage(entries: Option<&[Value]>) -> String {
    let Some(entries) = entries else {
        return "the page's storage could not be read".into();
    };
    let described: Vec<String> = ["loginData", "rememberMe", "lastProfile"]
        .iter()
        .zip(entries)
        .map(|(name, entry)| match entry.as_object() {
            Some(fields) => {
                let names: Vec<String> = fields
                    .keys()
                    .filter(|key| key.len() <= 32 && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'))
                    .take(16)
                    .cloned()
                    .collect();
                format!("{name}: {}", names.join(" "))
            }
            None => format!("{name}: none"),
        })
        .collect();
    described.join("; ")
}

/// A signed-in session, in the form it is kept between runs.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UbisoftSession {
    pub ticket: String,
    pub session_id: String,
    #[serde(default)]
    pub user_id: String,
    #[serde(default)]
    pub name: String,
    /// Lets the session be renewed without signing in again, when the user ticked
    /// "remember me" on Ubisoft's page.
    #[serde(default)]
    pub remember_me_ticket: Option<String>,
}

impl UbisoftSession {
    /// The session as it is kept (protected) between runs.
    pub fn to_kept(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    /// The session kept by [`UbisoftSession::to_kept`].
    pub fn from_kept(kept: &str) -> Option<Self> {
        serde_json::from_str::<Self>(kept)
            .ok()
            .filter(|session| !session.ticket.is_empty() && !session.session_id.is_empty())
    }

    /// Whether the session can be renewed without the user signing in again.
    pub fn renewable(&self) -> bool {
        self.remember_me_ticket.as_deref().is_some_and(|ticket| !ticket.is_empty())
    }
}

impl std::fmt::Debug for UbisoftSession {
    /// Tickets never reach a log or an error message.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UbisoftSession").field("name", &self.name).finish_non_exhaustive()
    }
}

/// The session in what [`READ_UBISOFT_SESSION_SCRIPT`] returned, when the page holds one.
pub fn session_from_page(result: &str) -> Option<UbisoftSession> {
    let entries = storage_entries(result)?;
    let field = |name: &str| {
        entries
            .iter()
            .find_map(|entry| entry.get(name)?.as_str().map(str::trim).filter(|text| !text.is_empty()))
            .map(str::to_string)
    };
    Some(UbisoftSession {
        ticket: field("ticket")?,
        session_id: field("sessionId")?,
        user_id: field("userId").unwrap_or_default(),
        name: field("nameOnPlatform").or_else(|| field("username")).unwrap_or_default(),
        remember_me_ticket: field("rememberMeTicket"),
    })
}

/// Client for Ubisoft's services.
#[derive(Debug, Clone)]
pub struct UbisoftApi {
    services: String,
    agent: ureq::Agent,
}

impl Default for UbisoftApi {
    fn default() -> Self {
        Self::new()
    }
}

impl UbisoftApi {
    pub fn new() -> Self {
        Self::with_services(UBISERVICES)
    }

    /// Client against another endpoint (tests).
    pub fn with_services(services: &str) -> Self {
        Self {
            services: services.trim_end_matches('/').to_string(),
            agent: ureq::AgentBuilder::new()
                .timeout_connect(Duration::from_secs(8))
                .timeout_read(Duration::from_secs(20))
                .user_agent(BROWSER_USER_AGENT)
                .build(),
        }
    }

    /// A fresh session from the "remember me" ticket of an earlier one. The answer carries a
    /// new such ticket, which replaces the one used.
    pub fn renew(&self, previous: &UbisoftSession) -> Result<UbisoftSession, StoreError> {
        let Some(remember_me) = previous.remember_me_ticket.as_deref() else {
            return Err(StoreError::Refused("no remember-me ticket to renew the session with".into()));
        };
        let response = self
            .agent
            .post(&format!("{}/v3/profiles/sessions", self.services))
            .set("Authorization", &format!("rm_v1 t={remember_me}"))
            .set("Ubi-AppId", CLUB_APP_ID)
            .send_json(serde_json::json!({ "rememberMe": true }));
        let json = read(response, "session")?;
        let text = |key: &str| json.get(key).and_then(Value::as_str).unwrap_or_default().to_string();
        let session = UbisoftSession {
            ticket: text("ticket"),
            session_id: text("sessionId"),
            user_id: Some(text("userId")).filter(|id| !id.is_empty()).unwrap_or_else(|| previous.user_id.clone()),
            name: Some(text("nameOnPlatform")).filter(|name| !name.is_empty()).unwrap_or_else(|| previous.name.clone()),
            remember_me_ticket: Some(text("rememberMeTicket"))
                .filter(|ticket| !ticket.is_empty())
                .or_else(|| previous.remember_me_ticket.clone()),
        };
        if session.ticket.is_empty() || session.session_id.is_empty() {
            return Err(StoreError::Unexpected("session: the answer carries no ticket".into()));
        }
        Ok(session)
    }

    /// The games of the account, ready to join the library as not installed.
    pub fn owned_games(&self, session: &UbisoftSession) -> Result<Vec<OwnedGame>, StoreError> {
        let response = self
            .agent
            .post(&format!("{}/v1/profiles/me/uplay/graphql", self.services))
            .set("Authorization", &format!("Ubi_v1 t={}", session.ticket))
            .set("Ubi-SessionId", &session.session_id)
            .set("Ubi-AppId", CLUB_APP_ID)
            .send_json(serde_json::json!({
                "operationName": "AllGames",
                "variables": { "owned": true },
                "query": OWNED_GAMES_QUERY,
            }));
        let json = read(response, "library")?;
        let nodes = json
            .pointer("/data/viewer/ownedGames/nodes")
            .and_then(Value::as_array)
            .ok_or_else(|| StoreError::Unexpected("library: the answer lists no games".into()))?;

        let mut games: Vec<OwnedGame> = Vec::new();
        for node in nodes {
            if let Some(game) = game_of(node) {
                if !games.iter().any(|known| known.game.id == game.game.id) {
                    games.push(game);
                }
            }
        }
        Ok(games)
    }
}

/// The question Ubisoft's own web client asks for the games of the account.
const OWNED_GAMES_QUERY: &str = "query AllGames {viewer {    id    ...ownedGamesList  }}fragment gameProps on Game {  id  spaceId  name  coverUrl  backgroundUrl  bannerUrl}fragment ownedGameProps on Game {  ...gameProps  viewer {    meta {      id      ownedPlatformGroups {        id        name        type      }    }  }}fragment ownedGamesList on User {  ownedGames: games(filterBy: {isOwned: true}) {    totalCount    nodes {      ...ownedGameProps    }  }}";

/// The library entry for a game of the account's list, if it is owned for PC.
fn game_of(node: &Value) -> Option<OwnedGame> {
    let text = |key: &str| node.get(key).and_then(Value::as_str).map(str::trim).unwrap_or_default();
    let id = Some(text("spaceId")).filter(|id| !id.is_empty()).unwrap_or(text("id"));
    let name = text("name");
    let plain_id = !id.is_empty()
        && id.len() <= 64
        && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-');
    if !plain_id || name.is_empty() {
        return None;
    }
    // Owned on consoles only: not a game for this PC
    let groups = node.pointer("/viewer/meta/ownedPlatformGroups").and_then(Value::as_array);
    if let Some(groups) = groups.filter(|groups| !groups.is_empty()) {
        let for_pc = groups
            .iter()
            .filter_map(|group| group.get("type")?.as_str())
            .any(|kind| kind.eq_ignore_ascii_case("PC"));
        if !for_pc {
            return None;
        }
    }
    let cover = Some(text("coverUrl")).filter(|url| url.starts_with("https://")).map(str::to_string);
    Some(OwnedGame::new(
        GamePlatform::Ubisoft,
        format!("ubisoft:{id}"),
        name,
        clean_game_title(name),
        id.to_string(),
        None,
        cover,
    ))
}

/// The JSON of an answer, or why there is none. Ubisoft names what went wrong in `errorCode`.
fn read(response: Result<ureq::Response, ureq::Error>, what: &str) -> Result<Value, StoreError> {
    match response {
        Ok(response) => {
            let json: Value = response
                .into_json()
                .map_err(|_| StoreError::Unexpected(format!("{what}: the answer is not JSON")))?;
            match json.get("errorCode") {
                Some(code) if json.get("message").is_some() => Err(StoreError::Refused(format!("{what}: error {code}"))),
                _ => Ok(json),
            }
        }
        Err(ureq::Error::Status(status @ (400 | 401 | 403), response)) => {
            let code = response
                .into_json::<Value>()
                .ok()
                .and_then(|json| json.get("errorCode").map(Value::to_string))
                .unwrap_or_else(|| format!("HTTP {status}"));
            Err(StoreError::Refused(format!("{what}: {code}")))
        }
        Err(ureq::Error::Status(status, _)) => Err(StoreError::Unexpected(format!("{what}: HTTP {status}"))),
        Err(ureq::Error::Transport(_)) => Err(StoreError::Unreachable),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::test_http::{spawn, Request};

    fn page_result() -> String {
        let login = serde_json::json!({
            "ticket": "ticket-from-page", "sessionId": "session-1", "userId": "user-guid",
            "nameOnPlatform": "PlayerOne", "expiration": "2026-11-15T19:00:00Z"
        })
        .to_string();
        let remember = serde_json::json!({ "rememberMeTicket": "remember-1" }).to_string();
        // The script returns JSON text of [text, text, null]; a web view hands that back as
        // JSON once more
        let list = serde_json::json!([login, remember, null]).to_string();
        serde_json::to_string(&list).unwrap()
    }

    fn page(address: &str) -> Url {
        Url::parse(address).unwrap()
    }

    const NOTHING_STORED: &str = "\"[null,null,null]\"";

    #[test]
    fn nothing_is_read_until_ubisoft_says_the_user_is_signed_in() {
        let url = sign_in_url();
        assert!(url.starts_with("https://connect.ubisoft.com/login?appId="));
        assert!(url.ends_with("nextUrl=https%3A%2F%2Fconnect.ubisoft.com%2Fready"));

        let mut flow = UbisoftFlow::new();
        assert_eq!(flow.start_url(), url);
        // The sign-in page, the blank page before the sign-in, and anything else on the way
        for address in [
            url.as_str(),
            "https://connect.ubisoft.com/login",
            "https://connect.ubisoft.com/change_domain/",
            "https://account.ubisoft.com/ready",
            "http://connect.ubisoft.com/ready",
            "https://connect.ubisoft.com.example.org/ready",
            "https://accounts.google.com/o/oauth2/auth",
        ] {
            assert_eq!(flow.page_loaded(&page(address)), Step::Wait, "{address}");
        }
    }

    #[test]
    fn setting_off_for_the_ready_address_is_enough_to_know_the_sign_in_is_over() {
        let mut flow = UbisoftFlow::new();
        // While signing in, wherever the window goes: nothing
        for address in [sign_in_url().as_str(), "https://connect.ubisoft.com/change_domain/", "https://accounts.google.com/"] {
            assert_eq!(flow.setting_off(&page(address)), Step::Wait, "{address}");
        }
        // The page at "ready" may never show (a browser puts its own error page there)
        assert_eq!(
            flow.setting_off(&page("https://connect.ubisoft.com/ready")),
            Step::Go("https://connect.ubisoft.com/change_domain/".into())
        );
        // Said once: the window's own moves after that are not taken for the store's
        assert_eq!(flow.setting_off(&page("https://connect.ubisoft.com/ready")), Step::Wait);
        assert_eq!(flow.setting_off(&page("https://connect.ubisoft.com/change_domain/")), Step::Wait);
        let blank = page("https://connect.ubisoft.com/change_domain/");
        assert_eq!(flow.page_loaded(&blank), Step::Read(READ_UBISOFT_SESSION_SCRIPT));
        assert!(matches!(flow.script_returned(&blank, &page_result()), Step::Captured(_)));
    }

    #[test]
    fn once_signed_in_the_session_is_read_on_the_blank_page() {
        let mut flow = UbisoftFlow::new();
        // "ready" answers "not found": nothing is read there, the window moves on
        assert_eq!(
            flow.page_loaded(&page("https://connect.ubisoft.com/ready")),
            Step::Go("https://connect.ubisoft.com/change_domain/".into())
        );
        let blank = page("https://connect.ubisoft.com/change_domain/");
        assert_eq!(flow.page_loaded(&blank), Step::Read(READ_UBISOFT_SESSION_SCRIPT));

        // Not written yet, then there
        assert_eq!(flow.script_returned(&blank, NOTHING_STORED), Step::Retry);
        let Step::Captured(kept) = flow.script_returned(&blank, &page_result()) else {
            panic!("the session is captured");
        };
        let session = UbisoftSession::from_kept(&kept).expect("kept form reads back");
        assert_eq!(session, session_from_page(&page_result()).unwrap());
        assert!(session.renewable());
        assert_eq!(UbisoftSession::from_kept(&session.to_kept()), Some(session));
        assert_eq!(UbisoftSession::from_kept("window"), None);
        assert_eq!(UbisoftSession::from_kept(r#"{"ticket":"","sessionId":"s"}"#), None);
    }

    #[test]
    fn a_blank_page_that_cannot_be_read_gives_way_to_a_fresh_sign_in_page() {
        let ready = page("https://connect.ubisoft.com/ready");
        let blank = page("https://connect.ubisoft.com/change_domain/");
        let fresh = page(&sign_in_url());

        // The blank page is an error page too: its storage cannot be read at all
        let mut flow = UbisoftFlow::new();
        assert!(matches!(flow.page_loaded(&ready), Step::Go(_)));
        assert_eq!(flow.page_loaded(&blank), Step::Read(READ_UBISOFT_SESSION_SCRIPT));
        assert_eq!(flow.script_returned(&blank, "null"), Step::Retry);
        assert_eq!(flow.script_returned(&blank, "null"), Step::Go(sign_in_url()));
        // A new page, in which nothing has been typed: read as soon as it shows
        assert_eq!(flow.page_loaded(&fresh), Step::Read(READ_UBISOFT_SESSION_SCRIPT));
        assert!(matches!(flow.script_returned(&fresh, &page_result()), Step::Captured(_)));

        // A blank page that can be read but holds nothing: a few tries, then the same way out
        let mut flow = UbisoftFlow::new();
        assert!(matches!(flow.page_loaded(&ready), Step::Go(_)));
        for _ in 1..EMPTY_LIMIT {
            assert_eq!(flow.script_returned(&blank, NOTHING_STORED), Step::Retry);
        }
        assert_eq!(flow.script_returned(&blank, NOTHING_STORED), Step::Go(sign_in_url()));

        // The fresh page holds no session either: it is loaded once more, read once more,
        // and then it is said, with what the storage held (names only). Never a second
        // reading of the same page: the user could be typing in it by then
        assert_eq!(flow.page_loaded(&fresh), Step::Read(READ_UBISOFT_SESSION_SCRIPT));
        let half = serde_json::to_string(&serde_json::json!([r#"{"sessionId":"s","secret value":"x"}"#, null, r#"{"userId":"u"}"#]).to_string()).unwrap();
        for _ in 1..FRESH_SIGN_IN_PAGES {
            assert_eq!(flow.script_returned(&fresh, &half), Step::Go(sign_in_url()));
            assert_eq!(flow.page_loaded(&fresh), Step::Read(READ_UBISOFT_SESSION_SCRIPT));
        }
        let Step::Failed(message) = flow.script_returned(&fresh, &half) else {
            panic!("fails in the end");
        };
        assert!(message.contains("loginData: sessionId; rememberMe: none; lastProfile: userId"), "{message}");
        assert!(!message.contains('"') && !message.contains("secret"), "{message}");
        assert_eq!(flow.page_loaded(&ready), Step::Wait);
        assert_eq!(flow.page_loaded(&fresh), Step::Wait);
        assert_eq!(flow.script_returned(&fresh, &page_result()).clone(), Step::Captured(session_from_page(&page_result()).unwrap().to_kept()), "a reading already under way still counts");

        // The fresh page went on to "ready" before it could be read: loaded again, then said
        let mut flow = UbisoftFlow::new();
        assert!(matches!(flow.page_loaded(&ready), Step::Go(_)));
        assert_eq!(flow.script_returned(&blank, "null"), Step::Retry);
        assert_eq!(flow.script_returned(&blank, "null"), Step::Go(sign_in_url()));
        for _ in 1..FRESH_SIGN_IN_PAGES {
            assert_eq!(flow.page_loaded(&ready), Step::Go(sign_in_url()));
        }
        assert!(matches!(flow.page_loaded(&ready), Step::Failed(_)));

        // The blank page leads somewhere else, or back to "ready": the same way out. The
        // page the user signed in on, reported again while the window leaves it, is not read
        let mut flow = UbisoftFlow::new();
        assert!(matches!(flow.page_loaded(&ready), Step::Go(_)));
        assert_eq!(flow.page_loaded(&fresh), Step::Wait);
        assert_eq!(flow.page_loaded(&page("https://www.ubisoft.com/en-us/")), Step::Go(sign_in_url()));
        let mut flow = UbisoftFlow::new();
        assert_eq!(flow.page_loaded(&ready), Step::Go("https://connect.ubisoft.com/change_domain/".into()));
        assert_eq!(flow.page_loaded(&ready), Step::Go("https://connect.ubisoft.com/change_domain/".into()));
        assert_eq!(flow.page_loaded(&ready), Step::Go(sign_in_url()));
    }

    #[test]
    fn reads_the_session_the_page_hands_over() {
        let session = session_from_page(&page_result()).expect("a session");
        assert_eq!(session.ticket, "ticket-from-page");
        assert_eq!(session.session_id, "session-1");
        assert_eq!(session.user_id, "user-guid");
        assert_eq!(session.name, "PlayerOne");
        assert_eq!(session.remember_me_ticket.as_deref(), Some("remember-1"));
        assert!(!format!("{session:?}").contains("ticket-from-page"), "tickets stay out of anything printed");

        // Handed back without the extra layer, and without "remember me"
        let plain = serde_json::json!([r#"{"ticket":"t","sessionId":"s"}"#, null, null]).to_string();
        let session = session_from_page(&plain).expect("a session");
        assert_eq!((session.ticket.as_str(), session.remember_me_ticket), ("t", None));

        // Not signed in yet, or not what the script returns
        assert_eq!(session_from_page("\"[null,null,null]\""), None);
        assert_eq!(session_from_page(r#"["{\"sessionId\":\"s\"}",null,null]"#), None);
        assert_eq!(session_from_page("null"), None);
        assert_eq!(session_from_page("not json"), None);
    }

    /// Ubisoft's services on a local port, answering as the real ones do.
    pub(crate) fn spawn_mock_ubisoft() -> String {
        spawn(|request: &Request| {
            let app = request.header("ubi-appid") == CLUB_APP_ID;
            let authorization = request.header("authorization");
            if !app {
                return (400, r#"{"errorCode":1,"message":"missing app id"}"#.to_string());
            }
            match (request.method.as_str(), request.path.as_str()) {
                ("POST", "/v3/profiles/sessions") if authorization == "rm_v1 t=remember-1" && request.body == r#"{"rememberMe":true}"# => (
                    200,
                    r#"{"ticket":"ticket-2","sessionId":"session-2","userId":"user-guid","nameOnPlatform":"PlayerOne","rememberMeTicket":"remember-2","expiration":"2026-11-15T19:00:00Z","serverTime":"2026-11-15T16:00:00Z"}"#.to_string(),
                ),
                ("POST", "/v3/profiles/sessions") => (401, r#"{"errorCode":3,"message":"Ticket is invalid","httpCode":401}"#.to_string()),
                ("POST", "/v1/profiles/me/uplay/graphql")
                    if authorization == "Ubi_v1 t=ticket-2"
                        && request.header("ubi-sessionid") == "session-2"
                        && request.body.contains("AllGames")
                        && request.body.contains("isOwned: true") =>
                {
                    (200, r#"{"data":{"viewer":{"id":"user-guid","ownedGames":{"totalCount":6,"nodes":[
                        {"id":"g1","spaceId":"f40e304d-8e8a-4a2b-b4a4-000000000001","name":"Assassin's Creed® Odyssey","coverUrl":"https://ubiservices.cdn.ubi.com/covers/odyssey.jpg","viewer":{"meta":{"id":"m1","ownedPlatformGroups":[{"id":"p1","name":"PC","type":"PC"},{"id":"p2","name":"PS4","type":"PLAYSTATION"}]}}},
                        {"id":"g2","spaceId":"f40e304d-8e8a-4a2b-b4a4-000000000002","name":"Anno 1800","coverUrl":"http://insecure.example/anno.jpg","viewer":{"meta":{"id":"m2","ownedPlatformGroups":[]}}},
                        {"id":"g3","spaceId":"f40e304d-8e8a-4a2b-b4a4-000000000003","name":"Console Only","coverUrl":"https://ubiservices.cdn.ubi.com/covers/console.jpg","viewer":{"meta":{"id":"m3","ownedPlatformGroups":[{"id":"p3","name":"Xbox","type":"XBOX"}]}}},
                        {"id":"only-id","name":"Rayman Legends"},
                        {"id":"g5","spaceId":"f40e304d-8e8a-4a2b-b4a4-000000000001","name":"Odyssey again"},
                        {"id":"bad id!","name":"Bad id"},
                        {"id":"g7","spaceId":"s7","name":"  "}
                    ]}}}}"#.to_string())
                }
                ("POST", "/v1/profiles/me/uplay/graphql") => (401, r#"{"errorCode":3,"message":"Ticket is invalid","httpCode":401}"#.to_string()),
                _ => (404, "{}".to_string()),
            }
        })
    }

    #[test]
    fn a_session_is_renewed_and_lists_the_games_owned_for_pc() {
        let base_url = spawn_mock_ubisoft();
        let api = UbisoftApi::with_services(&base_url);
        let from_page = session_from_page(&page_result()).unwrap();

        let session = api.renew(&from_page).expect("renewed");
        assert_eq!(session.ticket, "ticket-2");
        assert_eq!(session.remember_me_ticket.as_deref(), Some("remember-2"), "the ticket used is replaced");
        assert_eq!(session.name, "PlayerOne");

        let games = api.owned_games(&session).expect("library");
        let titles: Vec<&str> = games.iter().map(|owned| owned.game.title.as_str()).collect();
        assert_eq!(titles, ["Assassin's Creed\u{ae} Odyssey", "Anno 1800", "Rayman Legends"]);
        let odyssey = &games[0].game;
        assert_eq!(odyssey.id, "ubisoft:f40e304d-8e8a-4a2b-b4a4-000000000001");
        assert_eq!(odyssey.clean_title, "Assassin's Creed Odyssey");
        assert_eq!(odyssey.platform, GamePlatform::Ubisoft);
        assert!(!odyssey.is_installed);
        assert_eq!(odyssey.cover_path.as_deref(), Some("https://ubiservices.cdn.ubi.com/covers/odyssey.jpg"));
        assert_eq!(odyssey.install_uri(), None, "the account does not know the launcher's own number");
        assert_eq!(games[1].game.cover_path, None, "only secure addresses become covers");
        assert_eq!(games[2].game.id, "ubisoft:only-id");

        // The session of the page itself is no longer good, and so is a spent ticket
        assert!(matches!(api.owned_games(&from_page), Err(StoreError::Refused(_))));
        assert!(matches!(api.renew(&session), Err(StoreError::Refused(_))));
        let mut without = from_page.clone();
        without.remember_me_ticket = None;
        assert!(matches!(api.renew(&without), Err(StoreError::Refused(_))));

        let dead = UbisoftApi::with_services("http://127.0.0.1:9");
        assert_eq!(dead.renew(&from_page), Err(StoreError::Unreachable));
    }
}
