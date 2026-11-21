//! The EA account: the session its site hands out, and the library.
//!
//! EA has no sign-in that ends on a code to paste. Its site signs the user in with cookies,
//! and hands a signed-in browser a short-lived token on request. Kanso shows EA's own sign-in
//! page in a window of its own (see [`crate::signin`] for what that window may and may not
//! do), and asks EA for that token **in the same window**, the way EA's own site does. The
//! password is typed into EA's page and goes to EA alone.
//!
//! What is kept: nothing of the account in Kanso's files but its name. The session is the
//! cookies of EA's site, which stay in the browser profile of that window, apart from
//! everything else, until the user signs out. The token is used straight away, from this PC,
//! to ask EA for the list of games, and is never written anywhere.
//!
//! The addresses and the questions are the ones the EA library of Playnite uses.

use crate::accounts::StoreError;
use crate::cleaner::clean_game_title;
use crate::models::GamePlatform;
use crate::owned::OwnedGame;
use crate::scanner::exe_pick::slugify;
use crate::signin::{is_sign_in_page, on_site, script_text, SignInFlow, Step, READ_ANSWER_SCRIPT};
use serde_json::{json, Value};
use std::time::Duration;
use url::Url;

/// EA's site, whose sign-in page the window shows.
const WWW_SITE: &str = "https://www.ea.com";
/// EA's accounts service, which answers a signed-in browser with a token.
const ACCOUNTS_SITE: &str = "https://accounts.ea.com";
const SIGN_IN_PATH: &str = "/login";
/// The request for a token: answered with JSON, `prompt=none` meaning "never show a form".
const TOKEN_PATH: &str = "/connect/auth";
const TOKEN_QUERY: &str = "client_id=ORIGIN_JS_SDK&response_type=token&redirect_uri=nucleus:rest&prompt=none";

const GRAPHQL: &str = "https://service-aggregation-layer.juno.ea.com/graphql";
/// Games asked about per request, as EA's own client does.
const PAGE: usize = 100;
/// More pages than any account has: a guard against an answer that never ends.
const MAX_PAGES: usize = 50;
/// How many times the window is sent back to the token request before giving up.
const MAX_TOKEN_REQUESTS: u8 = 3;

/// The sign-in on EA's site. The window first asks EA whether this browser profile is signed
/// in (the token request); if it is not, it shows EA's sign-in page and asks again once the
/// user has been through EA's sign-in form and is back on EA's site.
pub struct EaFlow {
    www: String,
    accounts: String,
    /// The window is there for the user to sign in; otherwise it only renews a session.
    visible: bool,
    asked_to_sign_in: bool,
    /// The window has been to EA's sign-in site since the token was last asked for.
    form_shown: bool,
    token_requests: u8,
}

impl EaFlow {
    /// `visible`: the window is shown to the user, who may sign in.
    pub fn new(visible: bool) -> Self {
        Self::at(WWW_SITE, ACCOUNTS_SITE, visible)
    }

    /// The same steps against other sites (tests).
    pub fn at(www: &str, accounts: &str, visible: bool) -> Self {
        Self {
            www: www.trim_end_matches('/').to_string(),
            accounts: accounts.trim_end_matches('/').to_string(),
            visible,
            asked_to_sign_in: false,
            form_shown: false,
            token_requests: 0,
        }
    }

    fn token_url(&self) -> String {
        format!("{}{TOKEN_PATH}?{TOKEN_QUERY}", self.accounts)
    }

    /// Whether the page is the answer to the token request, and nothing else: the same path
    /// is also a stop on the way to the sign-in form, with another query.
    fn is_token_answer(&self, url: &Url) -> bool {
        let fields = |query: &str| {
            let mut fields: Vec<(String, String)> = url::form_urlencoded::parse(query.as_bytes()).into_owned().collect();
            fields.sort();
            fields
        };
        on_site(url, &self.accounts)
            && url.path() == TOKEN_PATH
            && url.query().is_some_and(|query| fields(query) == fields(TOKEN_QUERY))
    }
}

impl SignInFlow for EaFlow {
    fn start_url(&self) -> String {
        self.token_url()
    }

    fn page_loaded(&mut self, url: &Url) -> Step {
        if self.is_token_answer(url) {
            return Step::Read(READ_ANSWER_SCRIPT);
        }
        // The form lives on a site of its own, which EA's "/login" leads to: having left EA's
        // site and come back is what a sign-in looks like from here
        if self.asked_to_sign_in && !on_site(url, &self.www) {
            self.form_shown = true;
        }
        if is_sign_in_page(url) {
            return Step::SignInNeeded;
        }
        // Back on EA's site after the form: ask for the token again
        if self.asked_to_sign_in && self.form_shown && on_site(url, &self.www) {
            self.form_shown = false;
            self.token_requests += 1;
            return Step::Go(self.token_url());
        }
        Step::Wait
    }

    fn script_returned(&mut self, _url: &Url, result: &str) -> Step {
        let Some(answer) = script_text(result).and_then(|text| serde_json::from_str::<Value>(&text).ok()) else {
            return Step::Retry;
        };
        if let Some(token) = answer.get("access_token").and_then(Value::as_str).filter(|token| plain_token(token)) {
            return Step::Captured(token.to_string());
        }
        // No session in this profile ("login_required")
        if !self.visible {
            return Step::SignInNeeded;
        }
        if self.token_requests >= MAX_TOKEN_REQUESTS {
            let code = answer.get("error").and_then(Value::as_str).unwrap_or("no token");
            return Step::Failed(format!("EA gave no session after the sign-in ({})", plain(code)));
        }
        // To EA's sign-in page (again, if the user came back without having signed in)
        self.asked_to_sign_in = true;
        self.form_shown = false;
        Step::Go(format!("{}{SIGN_IN_PATH}", self.www))
    }
}

/// A token is letters, digits and a few signs: anything else is not handed on.
fn plain_token(token: &str) -> bool {
    (16..=4096).contains(&token.len())
        && token.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~' | b'+' | b'/' | b'='))
}

/// A store's error code, cut down to what is safe to show.
fn plain(code: &str) -> String {
    code.chars().filter(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | ' ')).take(60).collect()
}

/// Who is signed in.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EaPlayer {
    pub id: String,
    pub name: String,
}

/// Client for EA's library service.
#[derive(Debug, Clone)]
pub struct EaApi {
    graphql: String,
    agent: ureq::Agent,
}

impl Default for EaApi {
    fn default() -> Self {
        Self::new()
    }
}

const PLAYER_QUERY: &str = "query { me { player { pd psd displayName } } }";

const ENTITLEMENTS_QUERY: &str = "query getEntitlements($limit: Int, $next: String) { me { ownedGameProducts(locale: \"DEFAULT\", entitlementEnabled: true, storefronts: [EA], type: [DIGITAL_FULL_GAME, PACKAGED_FULL_GAME], platforms: [PC], paging: {limit: $limit, next: $next}) { next items { originOfferId product { baseItem { gameType } } } } } }";

const OFFERS_QUERY: &str = "query getOffers($offerIds: [String!]!) { gameProducts(offerIds: $offerIds, locale: \"DEFAULT\") { items { id originOfferId gameSlug baseItem { title packArt { largestImage { path } } keyArt { largestImage { path } } } } } }";

impl EaApi {
    pub fn new() -> Self {
        Self::with_service(GRAPHQL)
    }

    /// Client against another endpoint (tests).
    pub fn with_service(graphql: &str) -> Self {
        Self {
            graphql: graphql.to_string(),
            agent: ureq::AgentBuilder::new()
                .timeout_connect(Duration::from_secs(8))
                .timeout_read(Duration::from_secs(25))
                .user_agent(crate::USER_AGENT)
                .build(),
        }
    }

    fn ask(&self, token: &str, what: &str, query: &str, variables: Value) -> Result<Value, StoreError> {
        let response = self
            .agent
            .post(&self.graphql)
            .set("Authorization", &format!("Bearer {token}"))
            .send_json(json!({ "query": query, "variables": variables }));
        let answer: Value = match response {
            Ok(response) => response
                .into_json()
                .map_err(|_| StoreError::Unexpected(format!("{what}: the answer is not JSON")))?,
            Err(ureq::Error::Status(401 | 403, _)) => return Err(StoreError::Refused(format!("{what}: session not accepted"))),
            Err(ureq::Error::Status(status, _)) => return Err(StoreError::Unexpected(format!("{what}: HTTP {status}"))),
            Err(ureq::Error::Transport(_)) => return Err(StoreError::Unreachable),
        };
        if answer.get("data").is_none_or(Value::is_null) {
            let code = answer
                .pointer("/errors/0/extensions/code")
                .or_else(|| answer.pointer("/errors/0/message"))
                .and_then(Value::as_str)
                .unwrap_or("no data");
            let code = plain(code);
            return Err(if code.to_ascii_uppercase().contains("UNAUTH") {
                StoreError::Refused(format!("{what}: {code}"))
            } else {
                StoreError::Unexpected(format!("{what}: {code}"))
            });
        }
        Ok(answer)
    }

    /// Who the token belongs to.
    pub fn player(&self, token: &str) -> Result<EaPlayer, StoreError> {
        let answer = self.ask(token, "player", PLAYER_QUERY, json!({}))?;
        let player = answer
            .pointer("/data/me/player")
            .ok_or_else(|| StoreError::Unexpected("player: the answer names nobody".into()))?;
        let text = |key: &str| player.get(key).and_then(Value::as_str).unwrap_or_default().trim().to_string();
        Ok(EaPlayer { id: text("pd"), name: text("displayName") })
    }

    /// The base games of the account for PC, ready to join the library as not installed.
    pub fn owned_games(&self, token: &str) -> Result<Vec<OwnedGame>, StoreError> {
        // The offers the account owns, a page at a time
        let mut offers: Vec<String> = Vec::new();
        let mut next: Option<String> = None;
        for _ in 0..MAX_PAGES {
            let answer = self.ask(token, "library", ENTITLEMENTS_QUERY, json!({ "limit": PAGE, "next": next }))?;
            let owned = answer
                .pointer("/data/me/ownedGameProducts")
                .ok_or_else(|| StoreError::Unexpected("library: the answer lists no games".into()))?;
            for item in owned.get("items").and_then(Value::as_array).into_iter().flatten() {
                let base_game = item.pointer("/product/baseItem/gameType").and_then(Value::as_str) == Some("BASE_GAME");
                let offer = item.get("originOfferId").and_then(Value::as_str).unwrap_or_default();
                if base_game && plain_offer_id(offer) && !offers.iter().any(|known| known == offer) {
                    offers.push(offer.to_string());
                }
            }
            next = owned.get("next").and_then(Value::as_str).filter(|next| !next.is_empty()).map(str::to_string);
            if next.is_none() {
                break;
            }
        }

        // Their names and art
        let mut games: Vec<OwnedGame> = Vec::new();
        for chunk in offers.chunks(PAGE) {
            let answer = self.ask(token, "games", OFFERS_QUERY, json!({ "offerIds": chunk }))?;
            for item in answer.pointer("/data/gameProducts/items").and_then(Value::as_array).into_iter().flatten() {
                if let Some(game) = game_of(item) {
                    if !games.iter().any(|known| known.game.id == game.game.id) {
                        games.push(game);
                    }
                }
            }
        }
        Ok(games)
    }
}

fn plain_offer_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 96 && id.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b':' | b'-' | b'_'))
}

/// The library entry for a game EA describes. Its id is the one the scanner gives the same
/// game once installed (from its name), so that installing it turns this entry into that one.
fn game_of(item: &Value) -> Option<OwnedGame> {
    let offer = item.get("originOfferId").and_then(Value::as_str).filter(|id| plain_offer_id(id))?;
    let title = item.pointer("/baseItem/title").and_then(Value::as_str).map(str::trim).filter(|title| !title.is_empty())?;
    let clean = clean_game_title(title);
    let slug = Some(slugify(&clean)).filter(|slug| !slug.is_empty())?;
    let art = |kind: &str| {
        item.pointer(&format!("/baseItem/{kind}/largestImage/path"))
            .and_then(Value::as_str)
            .filter(|url| url.starts_with("https://"))
            .map(str::to_string)
    };
    let mut game = OwnedGame::new(
        GamePlatform::Ea,
        format!("ea:{slug}"),
        title,
        clean,
        offer.to_string(),
        None,
        art("packArt").or_else(|| art("keyArt")),
    );
    game.other_ids = vec![format!("ea:{}", slugify(title))];
    game.other_ids.retain(|id| *id != game.game.id);
    Some(game)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::test_http::{spawn, Request};

    const TOKEN: &str = "QVQxOjIuMDozLjA6NjA6abcdefghijklmnop";

    fn page(address: &str) -> Url {
        Url::parse(address).unwrap()
    }

    fn body(text: &str) -> String {
        serde_json::to_string(text).unwrap()
    }

    #[test]
    fn a_signed_in_profile_gets_its_token_without_showing_anything() {
        for visible in [false, true] {
            let mut flow = EaFlow::new(visible);
            let start = flow.start_url();
            assert_eq!(start, "https://accounts.ea.com/connect/auth?client_id=ORIGIN_JS_SDK&response_type=token&redirect_uri=nucleus:rest&prompt=none");
            assert_eq!(flow.page_loaded(&page(&start)), Step::Read(READ_ANSWER_SCRIPT));
            // The same request, as a web view may spell it
            let spelled = "https://accounts.ea.com/connect/auth?client_id=ORIGIN_JS_SDK&response_type=token&redirect_uri=nucleus%3Arest&prompt=none";
            assert_eq!(flow.page_loaded(&page(spelled)), Step::Read(READ_ANSWER_SCRIPT));
            let answer = body(&format!(r#"{{"access_token":"{TOKEN}","token_type":"Bearer","expires_in":"14399"}}"#));
            assert_eq!(flow.script_returned(&page(&start), &answer), Step::Captured(TOKEN.to_string()));
        }
    }

    #[test]
    fn without_a_session_the_user_signs_in_on_eas_page_and_the_token_is_asked_again() {
        let mut flow = EaFlow::new(true);
        let token_page = page(&flow.start_url());
        let no_session = body(r#"{"error":"login_required","error_description":"..."}"#);
        // The answer has not rendered yet
        assert_eq!(flow.script_returned(&token_page, "null"), Step::Retry);
        assert_eq!(flow.script_returned(&token_page, &body("<html>")), Step::Retry);
        assert_eq!(flow.script_returned(&token_page, &no_session), Step::Go("https://www.ea.com/login".into()));

        // On the way to the form and on the form: hands off. The accounts service is passed
        // through on the same path as the token request, with another query: never read
        for address in [
            "https://www.ea.com/login",
            "https://accounts.ea.com/connect/auth?client_id=EADOTCOM-WEB-SERVER&response_type=code&redirect_uri=https://www.ea.com/login_check",
            "https://accounts.ea.com/connect/auth?client_id=ORIGIN_JS_SDK&response_type=token&redirect_uri=nucleus:rest",
            "https://signin.ea.com/p/juno/login?execution=e1s1",
            "https://signin.ea.com/p/juno/tfa",
        ] {
            let step = flow.page_loaded(&page(address));
            assert!(matches!(step, Step::SignInNeeded | Step::Wait), "{address}: {step:?}");
        }
        assert_eq!(flow.page_loaded(&page("https://www.ea.com.example.org/")), Step::Wait);

        // Signed in: EA's site again
        assert_eq!(flow.page_loaded(&page("https://www.ea.com/it-it/")), Step::Go(flow.token_url()));
        assert_eq!(flow.page_loaded(&page("https://www.ea.com/it-it/games")), Step::Wait, "asked once per sign-in");
        assert_eq!(flow.page_loaded(&token_page), Step::Read(READ_ANSWER_SCRIPT));
        let answer = body(&format!(r#"{{"access_token":"{TOKEN}"}}"#));
        assert_eq!(flow.script_returned(&token_page, &answer), Step::Captured(TOKEN.to_string()));
    }

    #[test]
    fn a_renewal_stops_where_a_sign_in_would_be_needed_and_nothing_loops() {
        // Unseen window: no session means the user has to sign in again
        let mut flow = EaFlow::new(false);
        let token_page = page(&flow.start_url());
        assert_eq!(flow.script_returned(&token_page, &body(r#"{"error":"login_required"}"#)), Step::SignInNeeded);
        assert_eq!(flow.page_loaded(&page("https://www.ea.com/")), Step::Wait, "never sent anywhere unseen");

        // EA's site shows without its form having been seen (its "/login" led nowhere): the
        // token is not asked for in a loop, the user can still reach the form from the site
        let mut flow = EaFlow::new(true);
        let no_session = body(r#"{"error":"login_required <script>"}"#);
        assert!(matches!(flow.script_returned(&token_page, &no_session), Step::Go(_)));
        assert_eq!(flow.page_loaded(&page("https://www.ea.com/it-it/")), Step::Wait);
        assert_eq!(flow.page_loaded(&page("https://www.ea.com/login")), Step::SignInNeeded);
        assert_eq!(flow.page_loaded(&page("https://www.ea.com/it-it/")), Step::Wait, "EA's own /login is not the form");

        // Through the form and back, and still no session: the form is offered again, a few
        // times, and then it is said
        for attempt in 1..=MAX_TOKEN_REQUESTS {
            assert_eq!(flow.page_loaded(&page("https://signin.ea.com/p/juno/login")), Step::SignInNeeded);
            assert_eq!(flow.page_loaded(&page("https://www.ea.com/")), Step::Go(flow.token_url()));
            let step = flow.script_returned(&token_page, &no_session);
            if attempt < MAX_TOKEN_REQUESTS {
                assert_eq!(step, Step::Go("https://www.ea.com/login".into()), "attempt {attempt}");
            } else {
                let Step::Failed(message) = step else { panic!("fails in the end") };
                assert!(message.contains("login_required script"), "{message}");
            }
        }

        // A token that is not one is not handed on
        let mut flow = EaFlow::new(true);
        assert!(matches!(flow.script_returned(&token_page, &body(r#"{"access_token":"a b\nc"}"#)), Step::Go(_)));
    }

    /// EA's library service on a local port, answering as the real one does.
    pub(crate) fn spawn_mock_ea() -> String {
        spawn(|request: &Request| {
            if request.method != "POST" || request.path != "/graphql" {
                return (404, "{}".to_string());
            }
            if request.header("authorization") != format!("Bearer {TOKEN}") {
                return (401, r#"{"errors":[{"message":"Unauthorized"}]}"#.to_string());
            }
            let asked: Value = serde_json::from_str(&request.body).unwrap_or_default();
            let query = asked["query"].as_str().unwrap_or_default();
            let variables = &asked["variables"];
            if query.contains("displayName") {
                return (200, r#"{"data":{"me":{"player":{"pd":"1000123456789","psd":"1000987654321","displayName":"PlayerOne"}}}}"#.to_string());
            }
            if query.contains("ownedGameProducts") {
                assert_eq!(variables["limit"], 100);
                return match variables["next"].as_str() {
                    None => (200, r#"{"data":{"me":{"ownedGameProducts":{"next":"100","items":[
                        {"originOfferId":"Origin.OFR.50.0002694","product":{"baseItem":{"gameType":"BASE_GAME"}}},
                        {"originOfferId":"Origin.OFR.50.0001111","product":{"baseItem":{"gameType":"EXPANSION"}}},
                        {"originOfferId":"bad offer!","product":{"baseItem":{"gameType":"BASE_GAME"}}},
                        {"originOfferId":"Origin.OFR.50.0002694","product":{"baseItem":{"gameType":"BASE_GAME"}}}
                    ]}}}}"#.to_string()),
                    Some("100") => (200, r#"{"data":{"me":{"ownedGameProducts":{"next":null,"items":[
                        {"originOfferId":"DR:225064100","product":{"baseItem":{"gameType":"BASE_GAME"}}},
                        {"originOfferId":"Origin.OFR.50.0004444","product":{"baseItem":{"gameType":"BASE_GAME"}}},
                        {"originOfferId":"Origin.OFR.50.0005555","product":null}
                    ]}}}}"#.to_string()),
                    Some(_) => (200, r#"{"data":null,"errors":[{"message":"bad cursor"}]}"#.to_string()),
                };
            }
            if query.contains("gameProducts") {
                let ids: Vec<&str> = variables["offerIds"].as_array().unwrap().iter().filter_map(Value::as_str).collect();
                assert_eq!(ids, ["Origin.OFR.50.0002694", "DR:225064100", "Origin.OFR.50.0004444"]);
                return (200, r#"{"data":{"gameProducts":{"items":[
                    {"id":"p1","originOfferId":"Origin.OFR.50.0002694","gameSlug":"it-takes-two","baseItem":{"title":"It Takes Two","packArt":{"largestImage":{"path":"https://drop-assets.ea.com/images/it-takes-two-pack.jpg"}},"keyArt":{"largestImage":{"path":"https://drop-assets.ea.com/images/it-takes-two-key.jpg"}}}},
                    {"id":"p2","originOfferId":"DR:225064100","gameSlug":"mass-effect","baseItem":{"title":"Mass Effect™ Legendary Edition","packArt":null,"keyArt":{"largestImage":{"path":"https://drop-assets.ea.com/images/mele-key.jpg"}}}},
                    {"id":"p3","originOfferId":"Origin.OFR.50.0004444","gameSlug":"untitled","baseItem":{"title":" ","packArt":null,"keyArt":null}}
                ]}}}"#.to_string());
            }
            (400, r#"{"errors":[{"message":"unknown question"}]}"#.to_string())
        })
    }

    #[test]
    fn the_token_names_the_player_and_lists_the_base_games() {
        let base_url = spawn_mock_ea();
        let api = EaApi::with_service(&format!("{base_url}/graphql"));

        assert_eq!(api.player(TOKEN), Ok(EaPlayer { id: "1000123456789".into(), name: "PlayerOne".into() }));

        let games = api.owned_games(TOKEN).expect("library");
        let titles: Vec<&str> = games.iter().map(|owned| owned.game.title.as_str()).collect();
        assert_eq!(titles, ["It Takes Two", "Mass Effect\u{2122} Legendary Edition"]);
        let first = &games[0].game;
        assert_eq!(first.id, "ea:it-takes-two", "the id the scanner gives the installed game");
        assert_eq!(first.platform, GamePlatform::Ea);
        assert_eq!(first.platform_app_id.as_deref(), Some("Origin.OFR.50.0002694"));
        assert!(!first.is_installed);
        assert_eq!(first.cover_path.as_deref(), Some("https://drop-assets.ea.com/images/it-takes-two-pack.jpg"));
        let second = &games[1];
        assert_eq!(second.game.id, "ea:mass-effect-legendary-edition");
        assert_eq!(second.game.clean_title, "Mass Effect Legendary Edition");
        assert_eq!(second.game.cover_path.as_deref(), Some("https://drop-assets.ea.com/images/mele-key.jpg"));

        // A token EA no longer accepts, and a service that is not there
        assert!(matches!(api.owned_games("expired-token-0123456789"), Err(StoreError::Refused(_))));
        assert!(matches!(api.player("expired-token-0123456789"), Err(StoreError::Refused(_))));
        let dead = EaApi::with_service("http://127.0.0.1:9/graphql");
        assert_eq!(dead.player(TOKEN), Err(StoreError::Unreachable));
    }
}
