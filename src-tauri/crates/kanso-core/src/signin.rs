//! Signing in on a store's own page, shown in a window of Kanso.
//!
//! Ubisoft, EA and Battle.net have no sign-in that ends on a code to paste: their pages keep
//! the session themselves. For these stores Kanso opens **the store's own sign-in page** in a
//! separate window and waits for the store to say the user is signed in. This module is the
//! whole contract of that window; the code that opens it (`signin_window.rs` in the app) does
//! only what a [`SignInFlow`] tells it, one [`Step`] at a time.
//!
//! What Kanso does and does not do there:
//!
//! - **The password is never read, kept or sent by Kanso.** It is typed into the store's page
//!   and travels from that page to the store, as in any browser. No script of Kanso runs
//!   while the user is signing in, nor ever on a page the user has typed into; nothing watches
//!   the keyboard, no content of a sign-in page is read and no request it makes is
//!   intercepted.
//! - **Kanso sees the address of each page the window opens**, and nothing more of it: that is
//!   how a flow knows the store is done ([`SignInFlow::page_loaded`]). Addresses are not kept;
//!   what may be shown in a message is the site and the path, never the query
//!   ([`page_name`]).
//! - **Only the scripts in [`SCRIPTS`] ever run in the window**, and only *after* the store
//!   has signed the user in. Each reads one thing the store just handed out: the session the
//!   site keeps for itself, or the answer the store gave to a question about the account.
//!   Where each may run is said next to it, below.
//! - **What is read goes to the store that issued it and nowhere else.** Kanso has no server:
//!   there is no telemetry and no relay. The session is used from this PC to ask that same
//!   store for the list of games.
//! - **The window is cut off from Kanso.** It has its own browser profile (cookies of one
//!   store only, apart from the app's and from the other stores'), may open `https` pages
//!   only, and has no access to Kanso's commands, settings, keys or files. (Tauri, the
//!   framework, adds its own startup code to every window, this one included: it reads
//!   nothing of the page, and the commands it exists to call are refused here.)
//! - **Signing out removes everything**: the account entry and the store's browser profile.

use serde_json::Value;
use url::Url;

/// What the window does next.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// Nothing: the user is signing in, or the store is passing through a page of its own.
    Wait,
    /// Open this address of the store.
    Go(String),
    /// Run this script (always one of [`SCRIPTS`]) and hand what it returns to
    /// [`SignInFlow::script_returned`].
    Read(&'static str),
    /// The page was not ready: run the same script again in a moment.
    Retry,
    /// Done: what the store handed over.
    Captured(String),
    /// The store shows its sign-in form. A window opened for the user leaves them to it; one
    /// opened unseen to renew a session stops here: the user has to sign in again.
    SignInNeeded,
    /// The store answered something this code does not expect.
    Failed(String),
}

/// The steps of one store's sign-in.
pub trait SignInFlow: Send {
    /// The address the window opens on.
    fn start_url(&self) -> String;
    /// The window is setting off for `url` (sent there by the store's page, or by a step of
    /// this flow). Only the address is known: nothing of that page has loaded yet. Most flows
    /// wait for the page; one may need to know where a store sends the window even if the
    /// page there never shows.
    fn setting_off(&mut self, _url: &Url) -> Step {
        Step::Wait
    }
    /// A page finished loading.
    fn page_loaded(&mut self, url: &Url) -> Step;
    /// The script asked for with [`Step::Read`] returned `result` (JSON text) on `url`.
    fn script_returned(&mut self, url: &Url, result: &str) -> Step;
}

/// Reads the session Ubisoft's site keeps for itself once the user is signed in: three
/// entries of the site's own storage, and nothing else.
///
/// Run only after Ubisoft has sent the window to its "ready" address, which it does once the
/// sign-in is complete: on a blank page of the site (`connect.ubisoft.com/change_domain/`)
/// or, when that page cannot be read, on the sign-in address loaded afresh (a new page, in
/// which nothing has been typed), once, the moment it shows. See
/// [`crate::ubisoft_account::UbisoftFlow`].
pub const READ_UBISOFT_SESSION_SCRIPT: &str = r#"(() => { try { return JSON.stringify([localStorage.getItem("PRODloginData"), localStorage.getItem("PRODrememberMe"), localStorage.getItem("PRODlastProfile")]); } catch (_) { return null; } })()"#;

/// Reads the text of the page. Run only on the two addresses that are nothing but a store's
/// answer in JSON: EA's token answer and Battle.net's list of games.
pub const READ_ANSWER_SCRIPT: &str =
    r#"(() => { try { return document.body ? document.body.innerText : null; } catch (_) { return null; } })()"#;

/// Every script Kanso may run in a sign-in window.
pub const SCRIPTS: &[&str] = &[READ_UBISOFT_SESSION_SCRIPT, READ_ANSWER_SCRIPT];

/// The text a script returned. A web view hands a script's result back as JSON: text arrives
/// quoted, nothing as `null`.
pub fn script_text(result: &str) -> Option<String> {
    match serde_json::from_str::<Value>(result).ok()? {
        Value::String(text) => Some(text).filter(|text| !text.trim().is_empty()),
        _ => None,
    }
}

/// Whether `url` is a page of `site` (same scheme, host and port).
pub fn on_site(url: &Url, site: &str) -> bool {
    Url::parse(site).is_ok_and(|site| site.origin() == url.origin())
}

/// A page as it may be named in a message or a log: host and path, never the query, which
/// may carry a code or a token.
pub fn page_name(url: &Url) -> String {
    format!("{}{}", url.host_str().unwrap_or_default(), url.path())
}

/// Whether the address is one a store's sign-in form lives at.
pub fn is_sign_in_page(url: &Url) -> bool {
    let path = url.path().to_ascii_lowercase();
    path.contains("/login") || path.contains("/signin") || path.contains("/sign-in")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_scripts_only_read_what_the_store_handed_out() {
        // Nothing that touches a form, the keyboard, the cookies or the network, and nothing
        // that writes: the list is short enough to be read by anyone
        let forbidden = [
            "password", "input", "value", "querySelector", "getElement", "forms", "addEventListener", "onkey",
            "cookie", "fetch", "XMLHttpRequest", "sendBeacon", "WebSocket", "setItem", "removeItem", "eval(",
            "Function(", "import(", "src=", "href", "location", "open(", "postMessage", "__TAURI",
        ];
        for script in SCRIPTS {
            for word in forbidden {
                assert!(!script.contains(word), "{word} in {script}");
            }
            assert!(script.len() < 300, "short enough to audit at a glance");
        }
        assert_eq!(SCRIPTS.len(), 2);
        assert_eq!(READ_UBISOFT_SESSION_SCRIPT.matches("localStorage.getItem").count(), 3);
        assert_eq!(READ_UBISOFT_SESSION_SCRIPT.matches("localStorage").count(), 3);
        assert!(!READ_UBISOFT_SESSION_SCRIPT.contains("document"));
        assert!(!READ_ANSWER_SCRIPT.contains("localStorage"));
    }

    #[test]
    fn script_results_and_pages_are_read_carefully() {
        assert_eq!(script_text("\"hello\"").as_deref(), Some("hello"));
        assert_eq!(script_text("\"{\\\"a\\\":1}\"").as_deref(), Some("{\"a\":1}"));
        assert_eq!(script_text("null"), None);
        assert_eq!(script_text("\"  \""), None);
        assert_eq!(script_text("{\"a\":1}"), None);
        assert_eq!(script_text(""), None);

        let url = Url::parse("https://connect.ubisoft.com/ready?code=secret#frag").unwrap();
        assert!(on_site(&url, "https://connect.ubisoft.com"));
        assert!(on_site(&url, "https://connect.ubisoft.com/"));
        assert!(!on_site(&url, "http://connect.ubisoft.com"));
        assert!(!on_site(&url, "https://ubisoft.com"));
        assert!(!on_site(&url, "https://connect.ubisoft.com.example.org"));
        assert!(!on_site(&url, "not an address"));
        assert_eq!(page_name(&url), "connect.ubisoft.com/ready", "no query in what gets shown or logged");

        for page in ["https://eu.account.battle.net/login/it/", "https://signin.ea.com/p/juno/login?x=1", "https://x.example/SignIn"] {
            assert!(is_sign_in_page(&Url::parse(page).unwrap()), "{page}");
        }
        assert!(!is_sign_in_page(&Url::parse("https://account.battle.net/overview").unwrap()));
    }
}
