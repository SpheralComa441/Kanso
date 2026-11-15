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
/// What the account page reads its list of games from: answered with JSON.
const GAMES_PATH: &str = "/api/games-and-subs";
/// Where the site starts a sign-in (or renews the one it has) and comes back from.
const SIGN_IN_PATH: &str = "/oauth2/authorization/account-settings";
/// How many times the window is sent back to the list before giving up.
const MAX_LIST_REQUESTS: u8 = 3;

/// The sign-in on Battle.net's site. The window first asks for the list of games; if the site
/// does not know who is asking, it is sent through Battle.net's sign-in and asks again once
/// the account page shows.
pub struct BattleNetFlow {
    site: String,
    /// Times the window was sent through the site's sign-in.
    sign_ins: u8,
    /// Times the window was sent back to the list from the account page.
    list_requests: u8,
}

impl Default for BattleNetFlow {
    fn default() -> Self {
        Self::new()
    }
}

impl BattleNetFlow {
    pub fn new() -> Self {
        Self::at(ACCOUNT_SITE)
    }

    /// The same steps against another site (tests).
    pub fn at(site: &str) -> Self {
        Self { site: site.trim_end_matches('/').to_string(), sign_ins: 0, list_requests: 0 }
    }

    fn games_url(&self) -> String {
        format!("{}{GAMES_PATH}", self.site)
    }
}

impl SignInFlow for BattleNetFlow {
    fn start_url(&self) -> String {
        self.games_url()
    }

    fn page_loaded(&mut self, url: &Url) -> Step {
        if on_site(url, &self.site) && url.path() == GAMES_PATH {
            return Step::Read(READ_ANSWER_SCRIPT);
        }
        if is_sign_in_page(url) {
            return Step::SignInNeeded;
        }
        // The account page: the user is signed in
        if on_site(url, &self.site) && matches!(url.path().trim_end_matches('/'), "" | "/overview") {
            if self.list_requests >= MAX_LIST_REQUESTS {
                return Step::Failed("Battle.net keeps returning to the account page without a list of games".into());
            }
            self.list_requests += 1;
            return Step::Go(self.games_url());
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
        // The site does not know who is asking: through its sign-in (again, a few times, if
        // the account page showed and the list still did not come)
        if self.sign_ins >= MAX_LIST_REQUESTS {
            return Step::Failed("Battle.net gave no list of games after the sign-in".into());
        }
        self.sign_ins += 1;
        Step::Go(format!("{}{SIGN_IN_PATH}", self.site))
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
        let mut flow = BattleNetFlow::new();
        let start = flow.start_url();
        assert_eq!(start, "https://account.battle.net/api/games-and-subs");
        assert_eq!(flow.page_loaded(&page(&start)), Step::Read(READ_ANSWER_SCRIPT));
        assert_eq!(flow.script_returned(&page(&start), "null"), Step::Retry);
        assert_eq!(flow.script_returned(&page(&start), &body(GAMES_ANSWER)), Step::Captured(GAMES_ANSWER.to_string()));
    }

    #[test]
    fn without_a_session_the_site_signs_the_user_in_and_the_list_is_asked_again() {
        let mut flow = BattleNetFlow::new();
        let list = page(&flow.start_url());
        let unknown = body(r#"{"error":"unauthorized","logoutUri":"https://account.battle.net/api/logout"}"#);
        assert_eq!(
            flow.script_returned(&list, &unknown),
            Step::Go("https://account.battle.net/oauth2/authorization/account-settings".into())
        );

        // Battle.net's sign-in, wherever it lives: hands off
        for address in [
            "https://eu.account.battle.net/login/it/?ref=https://account.battle.net/",
            "https://account.battle.net/login/en/",
            "https://eu.account.battle.net/login/en/authenticator",
        ] {
            assert_eq!(flow.page_loaded(&page(address)), Step::SignInNeeded, "{address}");
        }
        assert_eq!(flow.page_loaded(&page("https://oauth.battle.net/authorize")), Step::Wait);
        assert_eq!(flow.page_loaded(&page("https://account.battle.net.example.org/overview")), Step::Wait);
        assert_eq!(flow.page_loaded(&page("https://account.battle.net/details")), Step::Wait);

        // Signed in: the account page, then the list
        assert_eq!(flow.page_loaded(&page("https://account.battle.net/overview")), Step::Go(flow.games_url()));
        assert_eq!(flow.page_loaded(&list), Step::Read(READ_ANSWER_SCRIPT));
        assert!(matches!(flow.script_returned(&list, &body(GAMES_ANSWER)), Step::Captured(_)));
    }

    #[test]
    fn nothing_is_asked_forever() {
        let mut flow = BattleNetFlow::new();
        let list = page(&flow.start_url());
        let unknown = body(r#"{"error":"unauthorized"}"#);
        for _ in 0..MAX_LIST_REQUESTS {
            assert!(matches!(flow.script_returned(&list, &unknown), Step::Go(_)));
        }
        assert!(matches!(flow.script_returned(&list, &unknown), Step::Failed(_)), "the sign-in is offered a few times");

        for _ in 0..MAX_LIST_REQUESTS {
            assert!(matches!(flow.page_loaded(&page("https://account.battle.net/")), Step::Go(_)));
        }
        assert!(matches!(flow.page_loaded(&page("https://account.battle.net/overview")), Step::Failed(_)));
    }
}
