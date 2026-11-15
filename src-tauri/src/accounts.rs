//! Signing in to store accounts, and bringing their libraries in.
//!
//! Two ways of signing in, and in neither does Kanso see, keep or send the password: it is
//! typed on the store's own page and goes to the store.
//!
//! - **Epic and GOG** sign in in the user's browser and end on a code, which the user pastes
//!   here. Kanso exchanges the code with the store for a session token.
//! - **Ubisoft, EA and Battle.net** have no such code: the store's own page is shown in a
//!   window of Kanso ([`crate::signin_window`]), and once the store has signed the user in
//!   Kanso reads what the page was handed ([`kanso_core::signin`] says exactly what).
//!
//! What stays on the PC afterwards, and nowhere else: for Epic, GOG and Ubisoft a session
//! token in `accounts.json`, encrypted by Windows for the current user; for EA and Battle.net
//! the cookies of the store's site, in the browser profile of that store's window. Each is
//! sent only to the store it came from, to renew the session and to read the list of games.
//! Signing out deletes it.
//!
//! The interface never handles a token: it asks for a sign-in and is told who is signed in.

use crate::signin_window::{self, Outcome};
use crate::{resolve_app_data_dir, AppState, SharedDb, GAMES_UPDATED_EVENT};
use kanso_core::accounts::{
    authorization_code_in, AccountStore, StoreAccount, StoreError, SESSION_IN_WINDOW_PROFILE,
};
use kanso_core::battlenet_account::{self, BattleNetFlow};
use kanso_core::ea_account::{EaApi, EaFlow};
use kanso_core::epic_account::{self, EpicApi};
use kanso_core::gog_account::{self, GogApi};
use kanso_core::models::{now_timestamp_ms, DetectedGame};
use kanso_core::owned::{merge_owned_games, OwnedGame};
use kanso_core::signin::SignInFlow;
use kanso_core::ubisoft_account::{UbisoftApi, UbisoftFlow, UbisoftLibrary, UbisoftSession};
use kanso_core::T;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_opener::OpenerExt;

/// A store's account changed state (signed in, signed out, session refused).
const ACCOUNTS_CHANGED_EVENT: &str = "accounts:changed";
/// Pause between two questions to a store's catalog while reading an account's library.
const CATALOG_PAUSE: Duration = Duration::from_millis(150);
/// A store read through its window is not read again this soon.
const WINDOW_SYNC_INTERVAL: Duration = Duration::from_secs(10 * 60);

const EPIC: &str = "epic";
const GOG: &str = "gog";
const UBISOFT: &str = "ubisoft";
const EA: &str = "ea";
const BATTLENET: &str = "Battle.net";

/// How the user signs in to a store.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
enum Method {
    /// In the browser, ending on a code to paste.
    Code,
    /// On the store's page, in a window of Kanso.
    Window,
}

/// A store Kanso can sign in to.
struct Store {
    /// The store's name in the library and in `accounts.json`.
    platform: &'static str,
    /// The name of its sign-in window and of that window's browser profile folder.
    slug: &'static str,
    name: &'static str,
    method: Method,
}

const STORES: &[Store] = &[
    Store { platform: EPIC, slug: "epic", name: "Epic Games", method: Method::Code },
    Store { platform: GOG, slug: "gog", name: "GOG", method: Method::Code },
    Store { platform: UBISOFT, slug: "ubisoft", name: "Ubisoft Connect", method: Method::Window },
    Store { platform: EA, slug: "ea", name: "EA app", method: Method::Window },
    Store { platform: BATTLENET, slug: "battlenet", name: "Battle.net", method: Method::Window },
];

/// What the interface shows about a store's account.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StoreAccountInfo {
    platform: String,
    method: Method,
    /// Someone signed in (even if the session has since been refused).
    connected: bool,
    display_name: Option<String>,
    /// The store no longer accepts the saved session.
    needs_sign_in: bool,
    /// What the last reading of the account's library found, in words for the user: how
    /// many games, how many of them through a subscription, how many left out and why.
    /// `None` until the library has been read in this run of Kanso.
    library: Option<String>,
}

/// How a sign-in in a window ended.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WindowSignIn {
    /// `false`: the user closed the window before signing in.
    signed_in: bool,
    account: StoreAccountInfo,
    /// Signed in, but the first reading of the library did not go through: why.
    note: Option<String>,
}

fn account_store(app: &AppHandle) -> Result<AccountStore, String> {
    Ok(AccountStore::new(&resolve_app_data_dir(app)?))
}

fn info_of(accounts: &AccountStore, store: &Store) -> StoreAccountInfo {
    let summary = accounts.summaries().into_iter().find(|summary| summary.platform == store.platform);
    StoreAccountInfo {
        platform: store.platform.to_string(),
        method: store.method,
        connected: summary.is_some(),
        needs_sign_in: summary.as_ref().is_some_and(|summary| summary.needs_sign_in),
        library: summary.as_ref().and_then(|_| library_notes().as_ref()?.get(store.platform).cloned()),
        display_name: summary.map(|summary| summary.display_name).filter(|name| !name.is_empty()),
    }
}

/// What the last reading of each account's library found (see [`StoreAccountInfo::library`]).
/// Kept for this run only: it is a report, not a setting.
static LIBRARY_NOTES: Mutex<Option<HashMap<&'static str, String>>> = Mutex::new(None);

fn library_notes() -> std::sync::MutexGuard<'static, Option<HashMap<&'static str, String>>> {
    LIBRARY_NOTES.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Notes how many games the account of `store` lists.
fn note_library(store: &Store, games: usize) {
    let note = T::AccountGames.s().replace("{n}", &games.to_string());
    library_notes().get_or_insert_with(HashMap::new).insert(store.platform, note);
}

/// Notes what was made of the Ubisoft account's list: the games, the ones a subscription
/// brought, the ones left out as not playable on a PC.
fn note_ubisoft_library(store: &Store, library: &UbisoftLibrary) {
    let mut parts = vec![T::AccountGames.s().replace("{n}", &library.games.len().to_string())];
    if let Some((mine, offered)) = library.subscription {
        parts.push(
            T::AccountSubscriptionGames
                .s()
                .replace("{n}", &mine.to_string())
                .replace("{total}", &offered.to_string()),
        );
    }
    if !library.left_out.is_empty() {
        let total: usize = library.left_out.iter().map(|(_, count)| count).sum();
        let platforms: Vec<String> =
            library.left_out.iter().take(6).map(|(platform, count)| format!("{platform} {count}")).collect();
        parts.push(
            T::AccountGamesLeftOut
                .s()
                .replace("{n}", &total.to_string())
                .replace("{platforms}", &platforms.join(", ")),
        );
    }
    library_notes().get_or_insert_with(HashMap::new).insert(store.platform, parts.join(" · "));
}

fn supported(platform: &str, method: Method) -> Result<&'static Store, String> {
    STORES
        .iter()
        .find(|store| store.platform == platform && store.method == method)
        .ok_or_else(|| T::StoreNotSupported.s().to_string())
}

/// The message for a pasted code that did not go through.
fn code_failure(error: StoreError) -> String {
    match error {
        StoreError::Refused(detail) => format!("{} ({detail})", T::SignInRefused.s()),
        StoreError::Unreachable => T::StoreUnreachable.s().to_string(),
        StoreError::Unexpected(detail) => format!("{}: {detail}", T::SignInFailed.s()),
    }
}

/// The message for a store that did not answer as expected after a sign-in in its window.
fn store_failure(error: &StoreError) -> String {
    match error {
        StoreError::Unreachable => T::StoreUnreachable.s().to_string(),
        other => format!("{}: {other}", T::SignInFailed.s()),
    }
}

/// The addresses of the stores' sites and services. Always the real ones in the program that
/// is distributed; a debug build can be pointed at stand-ins on this PC, to try the sign-in
/// windows without an account (`KANSO_SIGNIN_TEST_SITES`, a list of `name=address`).
fn test_site(name: &str) -> Option<String> {
    if !cfg!(debug_assertions) {
        return None;
    }
    let sites = std::env::var("KANSO_SIGNIN_TEST_SITES").ok()?;
    sites.split(',').find_map(|site| {
        let (key, address) = site.split_once('=')?;
        (key.trim() == name).then(|| address.trim().to_string())
    })
}

fn ubisoft_api() -> UbisoftApi {
    test_site("ubisoft_api").map_or_else(UbisoftApi::new, |site| UbisoftApi::with_services(&site, &site))
}

fn ea_api() -> EaApi {
    test_site("ea_api").map_or_else(EaApi::new, |site| EaApi::with_service(&site))
}

/// The steps of a store's sign-in window. `visible`: shown to the user, who may sign in.
fn flow_of(store: &Store, visible: bool) -> Box<dyn SignInFlow> {
    match store.platform {
        UBISOFT => Box::new(test_site("ubisoft").map_or_else(UbisoftFlow::new, |site| UbisoftFlow::at(&site))),
        EA => Box::new(match (test_site("ea_www"), test_site("ea_accounts")) {
            (Some(www), Some(accounts)) => EaFlow::at(&www, &accounts, visible),
            _ => EaFlow::new(visible),
        }),
        _ => Box::new(match test_site("battlenet") {
            Some(site) => BattleNetFlow::at(&site, visible),
            None => BattleNetFlow::new(visible),
        }),
    }
}

/// When each store was last read through its window, so that it is not read twice in a row.
static WINDOW_SYNCS: Mutex<Option<HashMap<&'static str, Instant>>> = Mutex::new(None);

fn window_syncs() -> std::sync::MutexGuard<'static, Option<HashMap<&'static str, Instant>>> {
    WINDOW_SYNCS.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn mark_read_now(store: &Store) {
    window_syncs().get_or_insert_with(HashMap::new).insert(store.slug, Instant::now());
}

fn read_recently(store: &Store) -> bool {
    window_syncs()
        .as_ref()
        .and_then(|syncs| syncs.get(store.slug))
        .is_some_and(|at| at.elapsed() < WINDOW_SYNC_INTERVAL)
}

#[tauri::command]
pub(crate) fn list_store_accounts(app: AppHandle) -> Result<Vec<StoreAccountInfo>, String> {
    let accounts = account_store(&app)?;
    Ok(STORES.iter().map(|store| info_of(&accounts, store)).collect())
}

/// Opens the store's own sign-in page in the browser. The address is built here, never taken
/// from the interface.
#[tauri::command]
pub(crate) fn open_store_sign_in(app: AppHandle, platform: String) -> Result<(), String> {
    let url = match supported(&platform, Method::Code)?.platform {
        EPIC => epic_account::sign_in_url(),
        _ => gog_account::sign_in_url(),
    };
    app.opener()
        .open_url(&url, None::<&str>)
        .map_err(|e| format!("{}: {e}", T::OperationFailed.s()))
}

/// Signs in with what the store's page ended on. `pasted` may be the code alone, the text of
/// the page or the address of the page.
#[tauri::command]
pub(crate) async fn store_sign_in(
    app: AppHandle,
    platform: String,
    pasted: String,
) -> Result<StoreAccountInfo, String> {
    let store = supported(&platform, Method::Code)?;
    let accounts = account_store(&app)?;
    let code = authorization_code_in(&pasted).ok_or_else(|| T::SignInCodeMissing.s().to_string())?;

    // Who signed in, and the token that keeps the session
    let signed_in = tauri::async_runtime::spawn_blocking(move || match store.platform {
        EPIC => EpicApi::new().exchange_code(&code).map(|session| {
            (session.account_id.clone(), session.display_name.clone(), session.refresh_token.clone())
        }),
        _ => {
            let api = GogApi::new();
            let session = api.exchange_code(&code)?;
            // The name is a courtesy: a sign-in does not fail for want of it
            let name = api.username(&session).unwrap_or_default();
            Ok((name.clone(), name, session.refresh_token.clone()))
        }
    })
    .await
    .map_err(|e| format!("{}: {e}", T::SignInFailed.s()))?
    .map_err(code_failure)?;

    let (account_id, display_name, refresh_token) = signed_in;
    accounts.save(
        store.platform,
        &StoreAccount { account_id, display_name, refresh_token, connected_at: now_timestamp_ms() },
    )?;
    let _ = app.emit(ACCOUNTS_CHANGED_EVENT, ());
    Ok(info_of(&accounts, store))
}

/// Shows the store's own sign-in page in a window and waits for the user to sign in there
/// (or to close the window). The password is typed on the store's page: it never passes
/// through here. What the store hands over once the user is signed in is kept as described
/// at the top of this file, and the account's games are read straight away.
#[tauri::command]
pub(crate) async fn store_sign_in_window(
    app: AppHandle,
    state: State<'_, AppState>,
    platform: String,
) -> Result<WindowSignIn, String> {
    let store = supported(&platform, Method::Window)?;
    let db = Arc::clone(&state.db);
    let background = app.clone();
    tauri::async_runtime::spawn_blocking(move || sign_in_with_window(&background, &db, store))
        .await
        .map_err(|e| format!("{}: {e}", T::SignInFailed.s()))?
}

fn sign_in_with_window(app: &AppHandle, db: &SharedDb, store: &'static Store) -> Result<WindowSignIn, String> {
    let data_dir = resolve_app_data_dir(app)?;
    let accounts = AccountStore::new(&data_dir);
    let title = format!("Kanso · {}", T::SignInOn.s().replace("{store}", store.name));

    let outcome = signin_window::run(app, &data_dir, store.slug, &title, flow_of(store, true), true);
    // Nobody signed in: whatever the store's pages left in the window's profile goes
    if !matches!(outcome, Outcome::Captured(_) | Outcome::Busy) && accounts.load(store.platform).is_none() {
        signin_window::remove_profile(&data_dir, store.slug);
    }
    let handed_over = match outcome {
        Outcome::Captured(handed_over) => handed_over,
        Outcome::Cancelled => {
            return Ok(WindowSignIn { signed_in: false, account: info_of(&accounts, store), note: None })
        }
        Outcome::Busy => return Err(T::SignInWindowBusy.s().to_string()),
        Outcome::TimedOut(pages) => return Err(format!("{} [{pages}]", T::SignInTimedOut.s())),
        Outcome::SignInNeeded => return Err(T::SignInFailed.s().to_string()),
        Outcome::Failed(why) => return Err(format!("{}: {why}", T::SignInFailed.s())),
    };

    // Who signed in, what keeps the session, and the games of the account
    let (account_id, display_name, kept, games) = match store.platform {
        UBISOFT => {
            let session = UbisoftSession::from_kept(&handed_over)
                .ok_or_else(|| format!("{}: no session", T::SignInFailed.s()))?;
            // The session is in `accounts.json` from here on: the window's profile, with
            // the cookies of Ubisoft's page, is not needed again
            signin_window::remove_profile(&data_dir, store.slug);
            let games = ubisoft_api().library(&session).map(|library| {
                note_ubisoft_library(store, &library);
                library.games
            });
            (session.user_id.clone(), session.name.clone(), session.to_kept(), games)
        }
        EA => {
            // The token is used here and now, and kept nowhere: the session is the cookies
            // of EA's site in the window's profile
            let api = ea_api();
            let player = match api.player(&handed_over) {
                Ok(player) => player,
                Err(error @ (StoreError::Refused(_) | StoreError::Unreachable)) => return Err(store_failure(&error)),
                // The name is a courtesy: a sign-in does not fail for want of it
                Err(StoreError::Unexpected(_)) => Default::default(),
            };
            (player.id, player.name, SESSION_IN_WINDOW_PROFILE.to_string(), api.owned_games(&handed_over))
        }
        _ => {
            let games = battlenet_account::games_in(&handed_over)
                .ok_or_else(|| StoreError::Unexpected("library: the answer lists no games".into()));
            (String::new(), String::new(), SESSION_IN_WINDOW_PROFILE.to_string(), games)
        }
    };

    accounts.save(
        store.platform,
        &StoreAccount { account_id, display_name, refresh_token: kept, connected_at: now_timestamp_ms() },
    )?;
    let _ = app.emit(ACCOUNTS_CHANGED_EVENT, ());

    let note = match games {
        Ok(games) => {
            if store.platform != UBISOFT {
                note_library(store, games.len());
            }
            add_to_library(db, games, &|added| {
                let _ = app.emit(GAMES_UPDATED_EVENT, added);
            });
            mark_read_now(store);
            None
        }
        Err(error) => Some(format!("{}: {error}", T::LibraryNotListed.s())),
    };
    Ok(WindowSignIn { signed_in: true, account: info_of(&accounts, store), note })
}

/// Signs out: the session is forgotten, and with it the browser profile of the store's
/// window. Games already in the library stay.
#[tauri::command]
pub(crate) fn store_sign_out(app: AppHandle, platform: String) -> Result<(), String> {
    let store = STORES
        .iter()
        .find(|store| store.platform == platform)
        .ok_or_else(|| T::StoreNotSupported.s().to_string())?;
    let data_dir = resolve_app_data_dir(&app)?;
    AccountStore::new(&data_dir).remove(store.platform)?;
    if store.method == Method::Window {
        signin_window::close(&app, store.slug);
        signin_window::remove_profile(&data_dir, store.slug);
        window_syncs().get_or_insert_with(HashMap::new).remove(store.slug);
    }
    library_notes().get_or_insert_with(HashMap::new).remove(store.platform);
    let _ = app.emit(ACCOUNTS_CHANGED_EVENT, ());
    Ok(())
}

/// At startup: deletes the browser profiles of sign-in windows nobody is signed in through
/// (a sign-out whose folder was still in use, a sign-in window closed halfway).
pub(crate) fn remove_unused_window_profiles(app: &AppHandle) {
    let Ok(data_dir) = resolve_app_data_dir(app) else {
        return;
    };
    let accounts = AccountStore::new(&data_dir);
    let in_use: Vec<String> = STORES
        .iter()
        // Ubisoft's session lives in `accounts.json`: its window keeps nothing
        .filter(|store| store.method == Method::Window && store.platform != UBISOFT)
        .filter(|store| accounts.load(store.platform).is_some())
        .map(|store| store.slug.to_string())
        .collect();
    signin_window::remove_orphan_profiles(&data_dir, &in_use);
}

fn add_to_library(db: &SharedDb, games: Vec<OwnedGame>, on_added: &dyn Fn(&[DetectedGame])) {
    let added = merge_owned_games(db, games, now_timestamp_ms());
    if !added.is_empty() {
        on_added(&added);
    }
}

/// Brings in the games of every store account someone is signed in to; `on_added` gets each
/// group of games as it is stored. A session a store no longer accepts is dropped (the
/// interface then asks to sign in again); a store that cannot be reached is left for the
/// next time.
pub(crate) fn sync_store_accounts(app: &AppHandle, db: &SharedDb, on_added: &dyn Fn(&[DetectedGame])) {
    let Ok(data_dir) = resolve_app_data_dir(app) else {
        return;
    };
    let accounts = AccountStore::new(&data_dir);
    for store in STORES {
        let outcome = match store.platform {
            EPIC => sync_epic_account(&accounts, db, on_added),
            GOG => sync_gog_account(&accounts, db, on_added),
            UBISOFT => sync_ubisoft_account(&accounts, db, on_added),
            _ => sync_window_account(app, &accounts, db, store, on_added),
        };
        match outcome {
            Ok(()) => {}
            Err(StoreError::Refused(_)) => {
                let _ = accounts.mark_session_refused(store.platform);
                let _ = app.emit(ACCOUNTS_CHANGED_EVENT, ());
            }
            Err(StoreError::Unreachable) => {}
            Err(StoreError::Unexpected(detail)) => eprintln!("Kanso: {}: {detail}", store.name),
        }
    }
}

/// Epic: renews the session, reads the account's library and asks the catalog about what the
/// library does not know, a few items at a time.
fn sync_epic_account(
    accounts: &AccountStore,
    db: &SharedDb,
    on_added: &dyn Fn(&[DetectedGame]),
) -> Result<(), StoreError> {
    let Some(account) = accounts.load(EPIC) else {
        return Ok(());
    };
    let api = EpicApi::new();
    let session = api.refresh(&account.refresh_token)?;
    // Renewing spent the old token: the new one has to be kept before anything else
    let renewed = StoreAccount {
        account_id: session.account_id.clone(),
        display_name: if session.display_name.is_empty() {
            account.display_name.clone()
        } else {
            session.display_name.clone()
        },
        refresh_token: session.refresh_token.clone(),
        connected_at: account.connected_at,
    };
    if accounts.save(EPIC, &renewed).is_err() {
        return Ok(());
    }

    let assets = api.library(&session)?;
    loop {
        let step = kanso_core::owned::sync_epic_library_step(
            db,
            &api,
            &session,
            &assets,
            CATALOG_PAUSE,
            now_timestamp_ms(),
        )?;
        if !step.added.is_empty() {
            on_added(&step.added);
        }
        if step.remaining == 0 {
            return Ok(());
        }
        std::thread::sleep(CATALOG_PAUSE);
    }
}

/// GOG: renews the session and reads the account's library, which names every game.
fn sync_gog_account(
    accounts: &AccountStore,
    db: &SharedDb,
    on_added: &dyn Fn(&[DetectedGame]),
) -> Result<(), StoreError> {
    let Some(account) = accounts.load(GOG) else {
        return Ok(());
    };
    let api = GogApi::new();
    let session = api.refresh(&account.refresh_token)?;
    let renewed = StoreAccount { refresh_token: session.refresh_token.clone(), ..account };
    if accounts.save(GOG, &renewed).is_err() {
        return Ok(());
    }

    let games = api.library(&session)?;
    if let Some(store) = STORES.iter().find(|store| store.platform == GOG) {
        note_library(store, games.len());
    }
    add_to_library(db, games, on_added);
    Ok(())
}

/// Ubisoft: renews the session with the ticket Ubisoft issued for that (when the user chose
/// to be remembered on Ubisoft's page) and reads the account's library. No window is opened.
fn sync_ubisoft_account(
    accounts: &AccountStore,
    db: &SharedDb,
    on_added: &dyn Fn(&[DetectedGame]),
) -> Result<(), StoreError> {
    let Some(account) = accounts.load(UBISOFT) else {
        return Ok(());
    };
    let kept = UbisoftSession::from_kept(&account.refresh_token)
        .ok_or_else(|| StoreError::Refused("the saved session cannot be read".into()))?;
    let api = ubisoft_api();
    let session = if kept.renewable() {
        let session = api.renew(&kept)?;
        // Renewing replaced the ticket: the new one has to be kept before anything else
        let renewed = StoreAccount { refresh_token: session.to_kept(), ..account };
        if accounts.save(UBISOFT, &renewed).is_err() {
            return Ok(());
        }
        session
    } else {
        // Not remembered: the session lasts a few hours, then Ubisoft refuses it
        kept
    };

    let library = api.library(&session)?;
    if let Some(store) = STORES.iter().find(|store| store.platform == UBISOFT) {
        note_ubisoft_library(store, &library);
    }
    add_to_library(db, library.games, on_added);
    Ok(())
}

/// EA and Battle.net: the session is the cookies in the store's window profile, so the
/// library is read through that window, opened unseen. If the store shows its sign-in form
/// instead, the session is gone and the user has to sign in again.
fn sync_window_account(
    app: &AppHandle,
    accounts: &AccountStore,
    db: &SharedDb,
    store: &'static Store,
    on_added: &dyn Fn(&[DetectedGame]),
) -> Result<(), StoreError> {
    if accounts.load(store.platform).is_none() || read_recently(store) {
        return Ok(());
    }
    let Ok(data_dir) = resolve_app_data_dir(app) else {
        return Ok(());
    };
    let handed_over = match signin_window::run(app, &data_dir, store.slug, store.name, flow_of(store, false), false) {
        Outcome::Captured(handed_over) => handed_over,
        Outcome::SignInNeeded => return Err(StoreError::Refused("the store asks to sign in again".into())),
        // The user is signing in right now, or closed the app
        Outcome::Busy | Outcome::Cancelled => return Ok(()),
        Outcome::TimedOut(pages) => return Err(StoreError::Unexpected(format!("no answer in time [{pages}]"))),
        Outcome::Failed(why) => return Err(StoreError::Unexpected(why)),
    };
    let games = match store.platform {
        EA => ea_api().owned_games(&handed_over)?,
        _ => battlenet_account::games_in(&handed_over)
            .ok_or_else(|| StoreError::Unexpected("library: the answer lists no games".into()))?,
    };
    note_library(store, games.len());
    add_to_library(db, games, on_added);
    mark_read_now(store);
    Ok(())
}

/// Debug builds only: signs in to the three window stores, reads their libraries again
/// through the unseen windows and signs out, printing each outcome. With
/// `KANSO_SIGNIN_TEST_SITES` naming stand-ins for the stores, this exercises the sign-in
/// windows end to end without an account. Never compiled into the distributed program.
#[cfg(debug_assertions)]
pub(crate) fn run_self_test_if_asked(app: &AppHandle, db: &SharedDb) {
    if std::env::var("KANSO_SIGNIN_SELFTEST").is_err() {
        return;
    }
    let (app, db) = (app.clone(), Arc::clone(db));
    std::thread::spawn(move || {
        let Ok(data_dir) = resolve_app_data_dir(&app) else {
            return;
        };
        let count = |platform: &str| {
            crate::lock_db(&db)
                .load_games()
                .unwrap_or_default()
                .iter()
                .filter(|game| game.platform.as_str() == platform)
                .count()
        };
        std::thread::sleep(Duration::from_secs(2));
        let stores = || STORES.iter().filter(|store| store.method == Method::Window);
        for store in stores() {
            let outcome = sign_in_with_window(&app, &db, store);
            let profile = signin_window::profile_dir(&data_dir, store.slug);
            eprintln!("selftest sign-in {}: {outcome:?} games={} profile={}", store.slug, count(store.platform), profile.exists());
        }
        *window_syncs() = None;
        sync_store_accounts(&app, &db, &|added| eprintln!("selftest sync added {}", added.len()));
        let accounts = AccountStore::new(&data_dir);
        for store in stores() {
            eprintln!("selftest after sync {}: {:?} games={}", store.slug, info_of(&accounts, store), count(store.platform));
            let signed_out = store_sign_out(app.clone(), store.platform.to_string());
            std::thread::sleep(Duration::from_secs(3));
            let profile = signin_window::profile_dir(&data_dir, store.slug);
            eprintln!("selftest sign-out {}: {signed_out:?} account={} profile={}", store.slug, accounts.load(store.platform).is_some(), profile.exists());
        }
        eprintln!("selftest done");
        app.exit(0);
    });
}
