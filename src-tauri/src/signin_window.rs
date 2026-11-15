//! The window a store's own sign-in page is shown in.
//!
//! This is the half of [`kanso_core::signin`] that touches a window: read that module first,
//! it is the contract. Here a window is opened on the address a [`SignInFlow`] names, and each
//! time a page finishes loading the flow is asked what to do, one [`Step`] at a time. Nothing
//! else happens in the window:
//!
//! - the only scripts ever run are the ones listed in [`kanso_core::signin::SCRIPTS`]
//!   (anything else is refused here, whatever a flow asks), and a flow asks for them only on
//!   the pages a store shows *after* the sign-in. Kanso adds no script of its own to the
//!   pages, intercepts no request and sees no key press: what the user types goes from the
//!   store's page to the store. What this code does see is the address of each page that
//!   finishes loading, which is what the flow goes by; only its host and path are remembered,
//!   for the message of a sign-in that fails;
//! - the window has a browser profile of its own, one folder per store under
//!   `signin/` in Kanso's data folder: its cookies are not the app's, nor another store's;
//! - it may open `https` addresses only (so may the pop-ups a store's page opens by itself,
//!   "sign in with..."), downloads nothing, and cannot call any command of Kanso: commands are
//!   granted to the main window alone (`capabilities/default.json`), and Tauri refuses them
//!   to pages that are not the app's own. Tauri's own startup code is in this window as in
//!   every other; with no command to call, it has nothing to do here.

use kanso_core::signin::{is_sign_in_page, page_name, SignInFlow, Step, SCRIPTS};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;
use tauri::webview::{NewWindowResponse, PageLoadEvent};
use tauri::{AppHandle, Manager, Url, WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent};

/// How long the user has to sign in before the window gives up.
const SIGN_IN_TIMEOUT: Duration = Duration::from_secs(20 * 60);
/// How long an unseen window may take to renew a session.
const RENEWAL_TIMEOUT: Duration = Duration::from_secs(45);
/// A page that has just shown may write what is to be read a moment later.
const READ_RETRY_PAUSE: Duration = Duration::from_millis(500);
const MAX_READS: u8 = 30;
/// An unseen window that meets a sign-in page waits this long before concluding that the
/// session is gone: a store may pass through such a page on its way to renewing it.
const SIGN_IN_PAGE_GRACE: Duration = Duration::from_secs(8);
/// Pages remembered for the message of a sign-in that did not go through.
const PAGES_REMEMBERED: usize = 8;

const LABEL_PREFIX: &str = "signin-";

/// How a run of the window ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Outcome {
    /// What the store handed over.
    Captured(String),
    /// The user closed the window.
    Cancelled,
    /// An unseen window met the store's sign-in form: the session is gone.
    SignInNeeded,
    /// Nothing came in time. The text names the last pages seen (no queries).
    TimedOut(String),
    /// The store answered something unexpected; the text says what, and on which pages.
    Failed(String),
    /// A window for the same store is already open.
    Busy,
}

/// Where the browser profile of a store's sign-in window lives.
pub(crate) fn profile_dir(data_dir: &Path, store: &str) -> PathBuf {
    data_dir.join("signin").join(store)
}

fn label_of(store: &str) -> String {
    format!("{LABEL_PREFIX}{store}")
}

/// Whether the window may open `url`: secure pages only. A debug build may also be pointed
/// at a stand-in for the stores on this very PC (see `accounts::Sites`).
fn may_open(url: &Url) -> bool {
    url.scheme() == "https"
        || (cfg!(debug_assertions)
            && url.scheme() == "http"
            && matches!(url.host_str(), Some("127.0.0.1" | "localhost")))
}

/// One run of a window: the flow it follows and where its outcome goes.
struct Run {
    flow: Mutex<Box<dyn SignInFlow>>,
    /// The window is shown to the user, who may sign in.
    visible: bool,
    outcome: Mutex<Sender<Outcome>>,
    /// The last pages that finished loading, as host and path.
    pages: Mutex<Vec<String>>,
}

fn locked<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

impl Run {
    fn finish(&self, outcome: Outcome) {
        // Only the first outcome is listened to
        let _ = locked(&self.outcome).send(outcome);
    }

    fn remember(&self, url: &Url) {
        let mut pages = locked(&self.pages);
        let page = page_name(url);
        if pages.last() != Some(&page) {
            pages.push(page);
            if pages.len() > PAGES_REMEMBERED {
                pages.remove(0);
            }
        }
    }

    fn pages_seen(&self) -> String {
        locked(&self.pages).join(" > ")
    }

    fn failed(&self, why: &str) {
        self.finish(Outcome::Failed(format!("{why} [{}]", self.pages_seen())));
    }
}

/// Does what the flow asked for.
fn act(window: &WebviewWindow, run: &Arc<Run>, url: &Url, step: Step) {
    match step {
        Step::Wait | Step::Retry => {}
        Step::Go(address) => match address.parse::<Url>() {
            Ok(target) if may_open(&target) => {
                if let Err(e) = window.navigate(target) {
                    run.failed(&format!("the page could not be opened: {e}"));
                }
            }
            _ => run.failed("the store's address is not a secure one"),
        },
        Step::Read(script) => read(window, run, url, script, 1),
        Step::Captured(handed_over) => run.finish(Outcome::Captured(handed_over)),
        Step::SignInNeeded => {
            // Shown to the user, the form is theirs to fill in; unseen, the run ends here
            if run.visible {
                return;
            }
            if !is_sign_in_page(url) {
                // The store said so itself
                run.finish(Outcome::SignInNeeded);
                return;
            }
            std::thread::sleep(SIGN_IN_PAGE_GRACE);
            if window.url().is_ok_and(|now| is_sign_in_page(&now)) {
                run.finish(Outcome::SignInNeeded);
            }
        }
        Step::Failed(why) => run.failed(&why),
    }
}

/// Runs one of the listed scripts on the page and hands the flow what it returned.
fn read(window: &WebviewWindow, run: &Arc<Run>, url: &Url, script: &'static str, attempt: u8) {
    if !SCRIPTS.contains(&script) {
        run.failed("a script that is not one of the listed ones was asked for");
        return;
    }
    let (window_then, run_then, url_then) = (window.clone(), Arc::clone(run), url.clone());
    let asked = window.eval_with_callback(script, move |result| {
        // The answer arrives on the web view's own thread: the rest happens off it
        let (window, run, url) = (window_then.clone(), Arc::clone(&run_then), url_then.clone());
        std::thread::spawn(move || {
            let step = locked(&run.flow).script_returned(&url, &result);
            match step {
                Step::Retry if attempt < MAX_READS => {
                    std::thread::sleep(READ_RETRY_PAUSE);
                    match window.url() {
                        // Still on that page (whatever it did to its own query or fragment)
                        Ok(now) if page_name(&now) == page_name(&url) => {
                            read(&window, &run, &url, script, attempt + 1);
                        }
                        // It moved on without a page load: the flow decides afresh
                        Ok(now) => {
                            run.remember(&now);
                            let step = locked(&run.flow).page_loaded(&now);
                            act(&window, &run, &now, step);
                        }
                        // The window is gone
                        Err(_) => {}
                    }
                }
                Step::Retry => run.failed("the page did not hand over what was expected"),
                step => act(&window, &run, &url, step),
            }
        });
    });
    if let Err(e) = asked {
        run.failed(&format!("the page could not be read: {e}"));
    }
}

/// Opens the sign-in window of `store` and follows `flow` until the store hands something
/// over, the user closes the window, or time runs out. `visible` shows the window to the
/// user; otherwise it stays unseen and only renews a session the profile already holds.
///
/// Waits for the outcome: call it from a background thread, never from the main one.
pub(crate) fn run(
    app: &AppHandle,
    data_dir: &Path,
    store: &str,
    title: &str,
    flow: Box<dyn SignInFlow>,
    visible: bool,
) -> Outcome {
    let label = label_of(store);
    if let Some(open) = app.get_webview_window(&label) {
        // The user asked again while their sign-in window is open: bring it forward
        if visible && open.is_visible().unwrap_or(false) {
            let _ = open.set_focus();
        }
        return Outcome::Busy;
    }
    let start = match flow.start_url().parse::<Url>() {
        Ok(start) if may_open(&start) => start,
        _ => return Outcome::Failed("the store's address is not a secure one".into()),
    };
    let profile = profile_dir(data_dir, store);
    if let Err(e) = std::fs::create_dir_all(&profile) {
        return Outcome::Failed(format!("the browser profile could not be created: {e}"));
    }

    let (sender, outcome) = mpsc::channel();
    let run = Arc::new(Run {
        flow: Mutex::new(flow),
        visible,
        outcome: Mutex::new(sender),
        pages: Mutex::new(Vec::new()),
    });

    let run_on_load = Arc::clone(&run);
    let built = WebviewWindowBuilder::new(app, &label, WebviewUrl::External(start))
        .title(title)
        .inner_size(520.0, 760.0)
        .min_inner_size(380.0, 480.0)
        .center()
        .visible(visible)
        .focused(visible)
        .skip_taskbar(!visible)
        .data_directory(profile)
        .on_navigation(may_open)
        // A store's page may open a window of its own ("sign in with..."): secure pages only
        .on_new_window(|url, _| if may_open(&url) { NewWindowResponse::Allow } else { NewWindowResponse::Deny })
        .on_download(|_, _| false)
        .on_page_load(move |window, payload| {
            if payload.event() != PageLoadEvent::Finished {
                return;
            }
            let (run, url) = (Arc::clone(&run_on_load), payload.url().clone());
            // Off the web view's own thread, like everything that follows
            std::thread::spawn(move || {
                run.remember(&url);
                let step = locked(&run.flow).page_loaded(&url);
                act(&window, &run, &url, step);
            });
        })
        .build();
    let window = match built {
        Ok(window) => window,
        Err(e) => return Outcome::Failed(format!("the window could not be opened: {e}")),
    };

    let run_on_close = Arc::clone(&run);
    window.on_window_event(move |event| {
        if matches!(event, WindowEvent::CloseRequested { .. } | WindowEvent::Destroyed) {
            run_on_close.finish(Outcome::Cancelled);
        }
    });

    let timeout = if visible { SIGN_IN_TIMEOUT } else { RENEWAL_TIMEOUT };
    let outcome = outcome
        .recv_timeout(timeout)
        .unwrap_or_else(|_| Outcome::TimedOut(run.pages_seen()));
    let _ = window.destroy();
    outcome
}

/// Closes every sign-in window (the app is closing: an unseen one must not keep it alive).
pub(crate) fn close_all(app: &AppHandle) {
    for (label, window) in app.webview_windows() {
        if label.starts_with(LABEL_PREFIX) {
            let _ = window.destroy();
        }
    }
}

/// Closes the sign-in window of `store`, if one is open.
pub(crate) fn close(app: &AppHandle, store: &str) {
    if let Some(window) = app.get_webview_window(&label_of(store)) {
        let _ = window.destroy();
    }
}

/// Deletes the browser profile of a store's window: its cookies and everything else the
/// store's pages kept there. The browser lets go of the folder a moment after its window
/// closes, so this insists for a while, in the background; what could not be deleted now is
/// deleted at the next start (see [`remove_orphan_profiles`]).
pub(crate) fn remove_profile(data_dir: &Path, store: &str) {
    let profile = profile_dir(data_dir, store);
    std::thread::spawn(move || {
        for _ in 0..20 {
            if !profile.exists() || std::fs::remove_dir_all(&profile).is_ok() {
                return;
            }
            std::thread::sleep(Duration::from_millis(500));
        }
    });
}

/// Deletes the browser profiles left behind by stores nobody is signed in to.
pub(crate) fn remove_orphan_profiles(data_dir: &Path, signed_in: &[String]) {
    let Ok(entries) = std::fs::read_dir(data_dir.join("signin")) else {
        return;
    };
    for entry in entries.flatten() {
        let store = entry.file_name().to_string_lossy().to_string();
        if !signed_in.contains(&store) {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}
