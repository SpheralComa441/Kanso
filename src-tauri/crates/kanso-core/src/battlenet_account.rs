//! The Battle.net account: the list of games its own account page shows.
//!
//! Battle.net has no sign-in for other programs at all: only its website's, which keeps the
//! user signed in with cookies. Kanso shows Battle.net's own sign-in page in a window of its
//! own (see [`crate::signin`] for what that window may and may not do) and, once the user is
//! signed in, opens **in the same window** the address the account page itself reads its list
//! of games from. The password is typed into Battle.net's page and goes to Blizzard alone.
//!
//! What is kept: nothing of the account in Kanso's files. The session is the cookies of
//! Battle.net's site, which stay in the browser profile of that window, apart from everything
//! else, until the user signs out. No token is involved and nothing is sent anywhere: the
//! list is read where Battle.net shows it.
//!
//! The addresses are the ones the Battle.net library of Playnite uses.

use crate::cleaner::clean_game_title;
use crate::models::GamePlatform;
use crate::owned::OwnedGame;
use crate::scanner::exe_pick::slugify;
use crate::signin::{is_sign_in_page, on_site, script_text, SignInFlow, Step, READ_ANSWER_SCRIPT};
use serde_json::Value;
use url::Url;

/// Battle.net's account site.
const ACCOUNT_SITE: &str = "https://account.battle.net";
/// What the account page reads its list of games from: answered with JSON to a browser that
/// is signed in. One that is not is sent back to the site's first page.
const GAMES_PATH: &str = "/api/games-and-subs";
/// Where the site starts a sign-in (or renews the one it has) and comes back from.
const SIGN_IN_PATH: &str = "/oauth2/authorization/account-settings";
/// Where the site ends a session, which leads to its sign-in form: the way Playnite gets to
/// the form. Only ever opened in the window the user signs in through, after the site has
/// said twice that nobody is signed in.
const SIGN_OUT_PATH: &str = "/api/logout";
/// How many times the window is sent to the list before giving up.
const MAX_LIST_REQUESTS: u8 = 8;

/// The sign-in on Battle.net's site.
///
/// The window first asks for the list of games. If the site does not answer with it (nobody
/// is signed in), the window is sent to the site's sign-in, and asks for the list again once
/// it is back on the account site. Nothing is run on Battle.net's sign-in pages.
///
/// A page of the account site showing in place of the list means "not signed in" only the
/// second time in a row: a page load of the site's own can cross the request for the list,
/// and must not be mistaken for a refusal.
pub struct BattleNetFlow {
    site: String,
    /// The window is there for the user to sign in; otherwise it only renews a session.
    visible: bool,
    /// The window was sent to the list and has not got there yet.
    awaiting_list: bool,
    /// Times in a row a page of the account site showed in place of the list.
    sent_back: u8,
    /// The window has been to the site's sign-in (sent there, or shown its form) since the
    /// list was last asked for.
    signing_in: bool,
    /// The site's sign-in form has shown in this window.
    form_shown: bool,
    /// Ways of reaching the site's sign-in tried so far.
    sign_ins: u8,
    list_requests: u8,
}

impl BattleNetFlow {
    /// `visible`: the window is shown to the user, who may sign in.
    pub fn new(visible: bool) -> Self {
        Self::at(ACCOUNT_SITE, visible)
    }

    /// The same steps against another site (tests).
    pub fn at(site: &str, visible: bool) -> Self {
        Self {
            site: site.trim_end_matches('/').to_string(),
            visible,
            awaiting_list: true,
            sent_back: 0,
            signing_in: false,
            form_shown: false,
            sign_ins: 0,
            list_requests: 0,
        }
    }

    fn games_url(&self) -> String {
        format!("{}{GAMES_PATH}", self.site)
    }

    fn ask_for_the_list(&mut self) -> Step {
        if self.list_requests >= MAX_LIST_REQUESTS {
            return Step::Failed("Battle.net keeps returning to the account page without a list of games".into());
        }
        self.list_requests += 1;
        self.awaiting_list = true;
        Step::Go(self.games_url())
    }

    /// The site did not answer with the list: nobody is signed in.
    fn not_signed_in(&mut self) -> Step {
        self.awaiting_list = false;
        self.sent_back = 0;
        self.signing_in = false;
        let way = match self.sign_ins {
            // The site's own start of a sign-in: renews a session that only needs renewing,
            // shows the form otherwise
            0 => SIGN_IN_PATH,
            // An unseen window stops here: the user has to sign in again. It never opens
            // the sign-out address
            _ if !self.visible => return Step::SignInNeeded,
            // The first way showed no form: the one Playnite takes. Not once the form has
            // shown: whoever has been through it is not signed out by Kanso
            1 if !self.form_shown => SIGN_OUT_PATH,
            // The user has been through the form and the list still does not come: said,
            // rather than left to a window in which nothing happens
            _ if self.form_shown => {
                return Step::Failed("Battle.net does not give the list of games to this browser after the sign-in".into())
            }
            // Out of ways: the site is left alone, to show its form by itself or at the
            // user's click
            _ => return Step::Wait,
        };
        self.sign_ins += 1;
        self.signing_in = true;
        Step::Go(format!("{}{way}", self.site))
    }
}

impl SignInFlow for BattleNetFlow {
    fn start_url(&self) -> String {
        self.games_url()
    }

    fn page_loaded(&mut self, url: &Url) -> Step {
        if on_site(url, &self.site) && url.path() == GAMES_PATH {
            self.awaiting_list = false;
            self.sent_back = 0;
            return Step::Read(READ_ANSWER_SCRIPT);
        }
        if is_sign_in_page(url) {
            // Whoever comes back from the form is signed in
            self.awaiting_list = false;
            self.sent_back = 0;
            self.signing_in = true;
            self.form_shown = true;
            return Step::SignInNeeded;
        }
        if !on_site(url, &self.site) {
            return Step::Wait;
        }
        // A page of the account site
        if self.awaiting_list {
            // In place of the list: once may be chance, twice is the site's answer
            self.sent_back += 1;
            if self.sent_back < 2 {
                return self.ask_for_the_list();
            }
            return self.not_signed_in();
        }
        if self.signing_in {
            // Back from the sign-in
            self.signing_in = false;
            return self.ask_for_the_list();
        }
        Step::Wait
    }

    fn script_returned(&mut self, _url: &Url, result: &str) -> Step {
        let Some(text) = script_text(result) else {
            return Step::Retry;
        };
        if games_in(&text).is_some() {
            return Step::Captured(text);
        }
        // An answer, but not the list: the site does not know who is asking
        match self.not_signed_in() {
            // Not left on a page that shows nothing but the site's refusal
            Step::Wait => Step::Go(format!("{}/", self.site)),
            step => step,
        }
    }
}

/// The games in Battle.net's answer, ready to join the library as not installed; `None` when
/// the text is not that answer (the site asked to sign in, or changed).
///
/// An account lists a game once per game account (a second World of Warcraft licence, another
/// region): each game is kept once. A game's id is the one the scanner gives it once
/// installed, from its name.
pub fn games_in(answer: &str) -> Option<Vec<OwnedGame>> {
    let json: Value = serde_json::from_str(answer).ok()?;
    let accounts = json.get("gameAccounts")?.as_array()?;
    let mut games: Vec<OwnedGame> = Vec::new();
    for account in accounts {
        let Some(name) = account.get("localizedGameName").and_then(Value::as_str).map(str::trim).filter(|name| !name.is_empty())
        else {
            continue;
        };
        let clean = clean_game_title(name);
        let slug = slugify(&clean);
        if slug.is_empty() || name.len() > 200 {
            continue;
        }
        let id = format!("battlenet:{slug}");
        if games.iter().any(|known| known.game.id == id) {
            continue;
        }
        let code = account.get("titleId").and_then(title_id).and_then(product_code);
        games.push(OwnedGame::new(
            GamePlatform::BattleNet,
            id,
            name,
            clean,
            code.clone().unwrap_or_default(),
            code.map(|code| format!("battlenet://{code}")),
            None,
        ));
    }
    Some(games)
}

/// A title's number, which the site writes as a number or as text.
fn title_id(value: &Value) -> Option<u32> {
    match value {
        Value::Number(number) => number.as_u64().and_then(|id| u32::try_from(id).ok()),
        Value::String(text) => text.trim().parse().ok(),
        _ => None,
    }
}

/// The short code Battle.net knows a game by ("WoW", "D3", "Pro"...), which is what the
/// title's number spells when read as letters. The Battle.net app opens on the game at
/// `battlenet://<code>`, where it offers the download.
fn product_code(title_id: u32) -> Option<String> {
    let letters: Vec<u8> = title_id.to_be_bytes().into_iter().skip_while(|byte| *byte == 0).collect();
    let plain = (2..=4).contains(&letters.len()) && letters.iter().all(u8::is_ascii_alphanumeric);
    plain.then(|| String::from_utf8_lossy(&letters).to_string())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) const GAMES_ANSWER: &str = r#"{"gameAccounts":[
        {"titleId":5730135,"localizedGameName":"World of Warcraft®","gameAccountName":"WoW1","gameAccountRegion":"EU","regionalGameFranchiseIconFilename":"wow.svg","titleHasSubscriptions":true},
        {"titleId":5730135,"localizedGameName":"World of Warcraft®","gameAccountName":"WoW2","gameAccountRegion":"US"},
        {"titleId":"4613486","localizedGameName":"Diablo® IV","gameAccountName":"Fen1"},
        {"titleId":5272175,"localizedGameName":"Overwatch® 2"},
        {"titleId":17459,"localizedGameName":"Diablo® III"},
        {"titleId":123456789012,"localizedGameName":"Unknown Number"},
        {"titleId":1,"localizedGameName":"  "},
        {"localizedGameName":"No Number"},
        {"titleId":21298}
    ],"gameSubscriptions":[]}"#;

    fn page(address: &str) -> Url {
        Url::parse(address).unwrap()
    }

    fn body(text: &str) -> String {
        serde_json::to_string(text).unwrap()
    }

    #[test]
    fn lists_each_game_of_the_account_once() {
        let games = games_in(GAMES_ANSWER).expect("the answer lists games");
        let listed: Vec<(&str, &str, Option<&str>)> = games
            .iter()
            .map(|owned| (owned.game.id.as_str(), owned.game.title.as_str(), owned.game.launch_uri.as_deref()))
            .collect();
        assert_eq!(
            listed,
            [
                ("battlenet:world-of-warcraft", "World of Warcraft\u{ae}", Some("battlenet://WoW")),
                ("battlenet:diablo-iv", "Diablo\u{ae} IV", Some("battlenet://Fen")),
                ("battlenet:overwatch-2", "Overwatch\u{ae} 2", Some("battlenet://Pro")),
                ("battlenet:diablo-iii", "Diablo\u{ae} III", Some("battlenet://D3")),
                ("battlenet:unknown-number", "Unknown Number", None),
                ("battlenet:no-number", "No Number", None),
            ]
        );
        let wow = &games[0].game;
        assert_eq!(wow.platform, GamePlatform::BattleNet);
        assert_eq!(wow.clean_title, "World of Warcraft");
        assert!(!wow.is_installed);
        assert_eq!(wow.install_uri().as_deref(), Some("battlenet://WoW"));
        assert_eq!(games[4].game.install_uri(), None);

        assert_eq!(games_in(r#"{"gameAccounts":[]}"#).map(|games| games.len()), Some(0));
        // Not signed in, or not the answer at all
        assert!(games_in(r#"{"error":"unauthorized"}"#).is_none());
        assert!(games_in(r#"{"gameAccounts":"none"}"#).is_none());
        assert!(games_in("<html>Log in</html>").is_none());

        assert_eq!(product_code(1465140039).as_deref(), Some("WTCG"));
        assert_eq!(product_code(21298).as_deref(), Some("S2"));
        assert_eq!(product_code(87), None, "one letter spells no game");
        assert_eq!(product_code(0x57_2F_57), None, "not letters");
    }

    #[test]
    fn a_signed_in_profile_gets_its_list_straight_away() {
        for visible in [true, false] {
            let mut flow = BattleNetFlow::new(visible);
            let start = flow.start_url();
            assert_eq!(start, "https://account.battle.net/api/games-and-subs");
            assert_eq!(flow.page_loaded(&page(&start)), Step::Read(READ_ANSWER_SCRIPT));
            assert_eq!(flow.script_returned(&page(&start), "null"), Step::Retry);
            assert_eq!(flow.script_returned(&page(&start), &body(GAMES_ANSWER)), Step::Captured(GAMES_ANSWER.to_string()));
        }
    }

    const SIGN_IN: &str = "https://account.battle.net/oauth2/authorization/account-settings";
    const SIGN_OUT: &str = "https://account.battle.net/api/logout";
    const FORM: &str = "https://eu.account.battle.net/login/it/?ref=https://account.battle.net/";

    /// The window is sent back to the site's first page twice in a row: what the real site
    /// does to a browser that is not signed in.
    fn sent_back_twice(flow: &mut BattleNetFlow) -> Step {
        let first_page = page("https://account.battle.net/");
        assert_eq!(flow.page_loaded(&first_page), Step::Go(flow.games_url()), "once may be chance");
        flow.page_loaded(&first_page)
    }

    #[test]
    fn sent_back_to_the_first_page_the_window_goes_to_the_sign_in_and_asks_again() {
        let mut flow = BattleNetFlow::new(true);
        let list = page(&flow.start_url());
        assert_eq!(sent_back_twice(&mut flow), Step::Go(SIGN_IN.into()));

        // Battle.net's sign-in, wherever it lives: hands off
        for address in [FORM, "https://account.battle.net/login/en/", "https://eu.account.battle.net/login/en/authenticator"] {
            assert_eq!(flow.page_loaded(&page(address)), Step::SignInNeeded, "{address}");
        }
        assert_eq!(flow.page_loaded(&page("https://oauth.battle.net/authorize")), Step::Wait);
        assert_eq!(flow.page_loaded(&page("https://account.battle.net.example.org/overview")), Step::Wait);

        // Signed in: the account page, then the list
        assert_eq!(flow.page_loaded(&page("https://account.battle.net/")), Step::Go(flow.games_url()));
        // The site moves from its first page to the overview by itself, across the request
        // for the list: asked again, and nobody is signed out
        assert_eq!(flow.page_loaded(&page("https://account.battle.net/overview")), Step::Go(flow.games_url()));
        assert_eq!(flow.page_loaded(&list), Step::Read(READ_ANSWER_SCRIPT));
        assert!(matches!(flow.script_returned(&list, &body(GAMES_ANSWER)), Step::Captured(_)));
    }

    #[test]
    fn a_refusal_in_place_of_the_list_leads_to_the_sign_in_too() {
        let mut flow = BattleNetFlow::new(true);
        let list = page(&flow.start_url());
        assert_eq!(flow.page_loaded(&list), Step::Read(READ_ANSWER_SCRIPT));
        let refusal = body(r#"{"error":"unauthorized","logoutUri":"https://account.battle.net/api/logout"}"#);
        assert_eq!(flow.script_returned(&list, &refusal), Step::Go(SIGN_IN.into()));
        assert_eq!(flow.page_loaded(&page(FORM)), Step::SignInNeeded);
        assert_eq!(flow.page_loaded(&page("https://account.battle.net/")), Step::Go(flow.games_url()));
    }

    #[test]
    fn when_the_sign_in_address_shows_no_form_the_other_ways_are_tried() {
        let mut flow = BattleNetFlow::new(true);
        let first_page = page("https://account.battle.net/");
        assert_eq!(sent_back_twice(&mut flow), Step::Go(SIGN_IN.into()));
        // The sign-in address comes back without a form: it may have renewed a session
        assert_eq!(flow.page_loaded(&first_page), Step::Go(flow.games_url()));
        // Still no list: the way Playnite reaches the form
        assert_eq!(sent_back_twice(&mut flow), Step::Go(SIGN_OUT.into()));
        assert_eq!(flow.page_loaded(&first_page), Step::Go(flow.games_url()));
        // Still nothing: the site is left alone, and the user can reach the form from it
        assert_eq!(sent_back_twice(&mut flow), Step::Wait);
        assert_eq!(flow.page_loaded(&first_page), Step::Wait);
        assert_eq!(flow.page_loaded(&page(FORM)), Step::SignInNeeded);
        assert_eq!(flow.page_loaded(&page("https://account.battle.net/overview")), Step::Go(flow.games_url()));
        let list = page(&flow.start_url());
        assert_eq!(flow.page_loaded(&list), Step::Read(READ_ANSWER_SCRIPT));
        assert!(matches!(flow.script_returned(&list, &body(GAMES_ANSWER)), Step::Captured(_)));
    }

    #[test]
    fn whoever_has_been_through_the_form_is_never_signed_out() {
        let mut flow = BattleNetFlow::new(true);
        assert_eq!(sent_back_twice(&mut flow), Step::Go(SIGN_IN.into()));
        assert_eq!(flow.page_loaded(&page(FORM)), Step::SignInNeeded);
        assert_eq!(flow.page_loaded(&page("https://account.battle.net/")), Step::Go(flow.games_url()));
        // The list keeps not coming: it is said; the sign-out address is not opened
        assert!(matches!(sent_back_twice(&mut flow), Step::Failed(_)));
        let list = page(&flow.start_url());
        let refusal = body(r#"{"error":"unauthorized"}"#);
        assert!(matches!(flow.script_returned(&list, &refusal), Step::Failed(_)));
    }

    #[test]
    fn an_unseen_window_never_signs_out_and_stops_where_a_sign_in_is_needed() {
        let first_page = page("https://account.battle.net/");

        // A session that only needed renewing
        let mut flow = BattleNetFlow::new(false);
        assert_eq!(sent_back_twice(&mut flow), Step::Go(SIGN_IN.into()));
        assert_eq!(flow.page_loaded(&page("https://account.battle.net/overview")), Step::Go(flow.games_url()));
        let list = page(&flow.start_url());
        assert_eq!(flow.page_loaded(&list), Step::Read(READ_ANSWER_SCRIPT));

        // No session: the form shows
        let mut flow = BattleNetFlow::new(false);
        assert_eq!(sent_back_twice(&mut flow), Step::Go(SIGN_IN.into()));
        assert_eq!(flow.page_loaded(&page(FORM)), Step::SignInNeeded);

        // No session and no form: the list is refused again
        let mut flow = BattleNetFlow::new(false);
        assert_eq!(sent_back_twice(&mut flow), Step::Go(SIGN_IN.into()));
        assert_eq!(flow.page_loaded(&first_page), Step::Go(flow.games_url()));
        assert_eq!(sent_back_twice(&mut flow), Step::SignInNeeded, "never to the sign-out address");

        // A refusal in place of the list, twice
        let mut flow = BattleNetFlow::new(false);
        let refusal = body(r#"{"error":"unauthorized"}"#);
        assert_eq!(flow.script_returned(&list, &refusal), Step::Go(SIGN_IN.into()));
        assert_eq!(flow.script_returned(&list, &refusal), Step::SignInNeeded);
    }

    #[test]
    fn nothing_is_asked_forever() {
        let mut flow = BattleNetFlow::new(true);
        let list = page(&flow.start_url());
        let refusal = body(r#"{"error":"unauthorized"}"#);
        assert_eq!(flow.script_returned(&list, &refusal), Step::Go(SIGN_IN.into()));
        assert_eq!(flow.script_returned(&list, &refusal), Step::Go(SIGN_OUT.into()));
        // Out of ways: shown the site's first page, and left to it
        assert_eq!(flow.script_returned(&list, &refusal), Step::Go("https://account.battle.net/".into()));
        assert_eq!(flow.page_loaded(&page("https://account.battle.net/")), Step::Wait);

        // Coming back from the form time after time without a list
        let mut flow = BattleNetFlow::new(true);
        for _ in 0..MAX_LIST_REQUESTS {
            assert_eq!(flow.page_loaded(&page(FORM)), Step::SignInNeeded);
            assert!(matches!(flow.page_loaded(&page("https://account.battle.net/")), Step::Go(_)));
            assert_eq!(flow.page_loaded(&list), Step::Read(READ_ANSWER_SCRIPT));
        }
        assert_eq!(flow.page_loaded(&page(FORM)), Step::SignInNeeded);
        assert!(matches!(flow.page_loaded(&page("https://account.battle.net/overview")), Step::Failed(_)));
    }
}
