//! The store accounts the user signed in to: who they are, and what keeps each session going.
//!
//! One small file, `accounts.json`, next to the settings, on this PC only.
//!
//! **Passwords are never here, nor anywhere else in Kanso.** Signing in always happens on the
//! store's own page: in the browser for Epic and GOG (Kanso receives the code that page ends
//! on), in a window showing the store's page for Ubisoft, EA and Battle.net (see
//! [`crate::signin`]). What the user types there goes to the store and to nobody else.
//!
//! What the file holds for each account is its name and, for the stores that issue one, the
//! token that renews the session (Epic, GOG) or the session tickets (Ubisoft). A token is
//! never written as it is: it goes through [`crate::secrets`], so the file is useless to
//! another Windows user or on another PC. EA and Battle.net issue nothing to keep: their
//! session is the cookies of the store's site, which stay in the browser profile of that
//! store's sign-in window, and the file only records that someone signed in
//! ([`SESSION_IN_WINDOW_PROFILE`]).
//!
//! A token is sent to the store that issued it and to nothing else. Signing out removes the
//! account from the file (and deletes the window's browser profile).

use crate::secrets::{self, Scheme};
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

pub const ACCOUNTS_FILE: &str = "accounts.json";

/// What is kept in place of a token for the stores whose session is the cookies of their
/// site, held by the browser profile of the store's sign-in window: a note that someone is
/// signed in, of no use to anyone.
pub const SESSION_IN_WINDOW_PROFILE: &str = "session-kept-by-the-sign-in-window";

/// Why a call to a store's services gave nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoreError {
    /// The store refused the code or the session (the text is the store's own error code):
    /// the user has to sign in again.
    Refused(String),
    /// The store could not be reached.
    Unreachable,
    /// The store answered something this code does not expect: which call, and what came back.
    Unexpected(String),
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StoreError::Refused(code) => write!(f, "refused by the store ({code})"),
            StoreError::Unreachable => write!(f, "the store could not be reached"),
            StoreError::Unexpected(detail) => write!(f, "unexpected answer from the store: {detail}"),
        }
    }
}

/// What the user pasted at the end of a sign-in, reduced to the authorization code: the code
/// itself, the text the store's page shows (`{"authorizationCode":"..."}`), or the address of
/// the page the browser landed on, which carries it as `code=`.
pub fn authorization_code_in(pasted: &str) -> Option<String> {
    let pasted = pasted.trim();
    let candidate = serde_json::from_str::<serde_json::Value>(pasted)
        .ok()
        .and_then(|json| json.get("authorizationCode")?.as_str().map(str::to_string))
        .or_else(|| {
            let (_, after) = pasted.split_once("code=")?;
            Some(after.split(['&', '#', '"', ' ']).next().unwrap_or_default().to_string())
        })
        .unwrap_or_else(|| pasted.trim_matches('"').to_string());
    let plain = (8..=1024).contains(&candidate.len())
        && candidate
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'!' | b'*' | b'$'));
    plain.then_some(candidate)
}

/// An account as kept on disk.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredAccount {
    account_id: String,
    display_name: String,
    /// The refresh token, protected and base64-encoded; empty once the store refused it.
    #[serde(default)]
    token: String,
    scheme: Scheme,
    #[serde(default)]
    connected_at: u64,
}

/// An account ready to be used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoreAccount {
    pub account_id: String,
    pub display_name: String,
    pub refresh_token: String,
    pub connected_at: u64,
}

/// What the interface may know about an account: never the token.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountSummary {
    pub platform: String,
    pub display_name: String,
    pub connected_at: u64,
    /// The store no longer accepts the saved session: the user has to sign in again.
    pub needs_sign_in: bool,
}

/// The accounts file of one Kanso data folder.
#[derive(Debug, Clone)]
pub struct AccountStore {
    path: PathBuf,
}

impl AccountStore {
    pub fn new(data_dir: &Path) -> Self {
        Self { path: data_dir.join(ACCOUNTS_FILE) }
    }

    fn read(&self) -> BTreeMap<String, StoredAccount> {
        fs::read_to_string(&self.path)
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default()
    }

    fn write(&self, accounts: &BTreeMap<String, StoredAccount>) -> Result<(), String> {
        let save_error = |e: std::io::Error| format!("{ACCOUNTS_FILE}: {e}");
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(save_error)?;
        }
        let serialized = serde_json::to_string_pretty(accounts).map_err(|e| e.to_string())?;
        // A sibling file renamed over the target: a crash mid-write cannot lose a session
        let tmp_path = self.path.with_extension("json.tmp");
        fs::write(&tmp_path, serialized)
            .and_then(|()| fs::rename(&tmp_path, &self.path))
            .map_err(save_error)
    }

    /// The account signed in to `platform`, when there is one whose session can still be used
    /// (it was saved by this Windows user and the store has not refused it).
    pub fn load(&self, platform: &str) -> Option<StoreAccount> {
        let stored = self.read().remove(platform)?;
        if stored.token.is_empty() {
            return None;
        }
        let protected = base64::engine::general_purpose::STANDARD.decode(&stored.token).ok()?;
        let token = secrets::unprotect(&protected, stored.scheme).ok()?;
        Some(StoreAccount {
            account_id: stored.account_id,
            display_name: stored.display_name,
            refresh_token: String::from_utf8(token).ok().filter(|token| !token.is_empty())?,
            connected_at: stored.connected_at,
        })
    }

    pub fn save(&self, platform: &str, account: &StoreAccount) -> Result<(), String> {
        let protected = secrets::protect(account.refresh_token.as_bytes())?;
        let mut accounts = self.read();
        accounts.insert(
            platform.to_string(),
            StoredAccount {
                account_id: account.account_id.clone(),
                display_name: account.display_name.clone(),
                token: base64::engine::general_purpose::STANDARD.encode(protected),
                scheme: secrets::SCHEME,
                connected_at: account.connected_at,
            },
        );
        self.write(&accounts)
    }

    /// Forgets the session of `platform` but remembers whose it was, so that the interface can
    /// say the account has to be signed in to again.
    pub fn mark_session_refused(&self, platform: &str) -> Result<(), String> {
        let mut accounts = self.read();
        match accounts.get_mut(platform) {
            Some(stored) => stored.token.clear(),
            None => return Ok(()),
        }
        self.write(&accounts)
    }

    /// Signs out: nothing about the account is kept.
    pub fn remove(&self, platform: &str) -> Result<(), String> {
        let mut accounts = self.read();
        if accounts.remove(platform).is_none() {
            return Ok(());
        }
        self.write(&accounts)
    }

    pub fn summaries(&self) -> Vec<AccountSummary> {
        self.read()
            .into_iter()
            .map(|(platform, stored)| AccountSummary {
                needs_sign_in: self.load(&platform).is_none(),
                platform,
                display_name: stored.display_name,
                connected_at: stored.connected_at,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn account(token: &str) -> StoreAccount {
        StoreAccount {
            account_id: "0123abcd".into(),
            display_name: "Player One".into(),
            refresh_token: token.into(),
            connected_at: 1_700_000_000_000,
        }
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
        // The address of GOG's last page, and Microsoft's, whose codes carry other signs
        assert_eq!(
            authorization_code_in("https://embed.gog.com/on_login_success?origin=client&code=Ab3-_xYz0123456789").as_deref(),
            Some("Ab3-_xYz0123456789")
        );
        assert_eq!(
            authorization_code_in("https://login.live.com/oauth20_desktop.srf?code=M.C123_BAY.2.U.abc!def*gh$&lc=1040").as_deref(),
            Some("M.C123_BAY.2.U.abc!def*gh$")
        );
        // Signed out in the browser: the page has no code yet
        assert_eq!(authorization_code_in(r#"{"redirectUrl":null,"authorizationCode":null,"sid":null}"#), None);
        assert_eq!(authorization_code_in(""), None);
        assert_eq!(authorization_code_in("short"), None);
        assert_eq!(authorization_code_in("not a code: has spaces and/slashes"), None);
        assert_eq!(authorization_code_in("https://auth.gog.com/auth?client_id=1&response_type=code"), None);
        assert_eq!(authorization_code_in(&"a".repeat(2000)), None);
    }

    #[test]
    fn an_account_is_saved_read_back_and_forgotten() {
        let dir = tempfile::tempdir().unwrap();
        let store = AccountStore::new(dir.path());
        assert_eq!(store.load("epic"), None);
        assert!(store.summaries().is_empty());
        store.remove("epic").unwrap();

        store.save("epic", &account("eg1~first")).unwrap();
        assert_eq!(store.load("epic"), Some(account("eg1~first")));
        assert_eq!(store.load("gog"), None);
        assert_eq!(
            store.summaries(),
            [AccountSummary {
                platform: "epic".into(),
                display_name: "Player One".into(),
                connected_at: 1_700_000_000_000,
                needs_sign_in: false,
            }]
        );

        // A session renewed: the new token replaces the old one
        store.save("epic", &account("eg1~second")).unwrap();
        assert_eq!(store.load("epic").unwrap().refresh_token, "eg1~second");

        store.remove("epic").unwrap();
        assert_eq!(store.load("epic"), None);
        assert!(store.summaries().is_empty());
    }

    #[test]
    fn a_refused_session_is_dropped_but_the_account_is_still_named() {
        let dir = tempfile::tempdir().unwrap();
        let store = AccountStore::new(dir.path());
        store.mark_session_refused("epic").unwrap();
        store.save("epic", &account("eg1~token")).unwrap();
        store.mark_session_refused("epic").unwrap();

        assert_eq!(store.load("epic"), None);
        let summaries = store.summaries();
        assert_eq!(summaries.len(), 1);
        assert!(summaries[0].needs_sign_in);
        assert_eq!(summaries[0].display_name, "Player One");
        assert!(!fs::read_to_string(dir.path().join(ACCOUNTS_FILE)).unwrap().contains("eg1~token"));
    }

    #[test]
    fn a_file_that_is_damaged_or_from_elsewhere_is_no_account() {
        let dir = tempfile::tempdir().unwrap();
        let store = AccountStore::new(dir.path());
        let path = dir.path().join(ACCOUNTS_FILE);

        fs::write(&path, "not json").unwrap();
        assert_eq!(store.load("epic"), None);
        assert!(store.summaries().is_empty());

        // A token that is not base64, and one protected with a scheme this build does not use
        let other = if secrets::SCHEME == Scheme::Dpapi { "plain" } else { "dpapi" };
        fs::write(
            &path,
            format!(
                r#"{{"epic":{{"accountId":"a","displayName":"A","token":"***","scheme":"{}","connectedAt":1}},
                    "gog":{{"accountId":"b","displayName":"B","token":"dG9rZW4=","scheme":"{other}","connectedAt":2}}}}"#,
                if secrets::SCHEME == Scheme::Dpapi { "dpapi" } else { "plain" },
            ),
        )
        .unwrap();
        assert_eq!(store.load("epic"), None);
        assert_eq!(store.load("gog"), None);
        assert!(store.summaries().iter().all(|summary| summary.needs_sign_in));

        // Saving over a damaged file starts afresh
        fs::write(&path, "[]").unwrap();
        store.save("epic", &account("eg1~new")).unwrap();
        assert_eq!(store.load("epic").unwrap().refresh_token, "eg1~new");
    }
}
