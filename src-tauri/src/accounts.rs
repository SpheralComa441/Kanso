//! Signing in to store accounts, and bringing their libraries in.
//!
//! The interface never handles a token: it asks for the sign-in page to be opened in the
//! browser, hands over what the user pasted at the end of it, and is told who is signed in.

use crate::{resolve_app_data_dir, SharedDb};
use kanso_core::accounts::{authorization_code_in, AccountStore, StoreAccount, StoreError};
use kanso_core::epic_account::{self, EpicApi};
use kanso_core::gog_account::{self, GogApi};
use kanso_core::models::{now_timestamp_ms, DetectedGame};
use kanso_core::T;
use serde::Serialize;
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use tauri_plugin_opener::OpenerExt;

/// A store's account changed state (signed in, signed out, session refused).
const ACCOUNTS_CHANGED_EVENT: &str = "accounts:changed";
/// Pause between two questions to a store's catalog while reading an account's library.
const CATALOG_PAUSE: Duration = Duration::from_millis(150);

const EPIC: &str = "epic";
const GOG: &str = "gog";
/// The stores Kanso can sign in to.
const STORES: &[&str] = &[EPIC, GOG];

/// What the interface shows about a store's account.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StoreAccountInfo {
    platform: String,
    /// Someone signed in (even if the session has since been refused).
    connected: bool,
    display_name: Option<String>,
    /// The store no longer accepts the saved session.
    needs_sign_in: bool,
}

fn account_store(app: &AppHandle) -> Result<AccountStore, String> {
    Ok(AccountStore::new(&resolve_app_data_dir(app)?))
}

fn info_of(store: &AccountStore, platform: &str) -> StoreAccountInfo {
    let summary = store.summaries().into_iter().find(|summary| summary.platform == platform);
    StoreAccountInfo {
        platform: platform.to_string(),
        connected: summary.is_some(),
        needs_sign_in: summary.as_ref().is_some_and(|summary| summary.needs_sign_in),
        display_name: summary.map(|summary| summary.display_name).filter(|name| !name.is_empty()),
    }
}

fn supported(platform: &str) -> Result<&'static str, String> {
    STORES
        .iter()
        .copied()
        .find(|store| *store == platform)
        .ok_or_else(|| T::StoreNotSupported.s().to_string())
}

/// The message for a sign-in that did not go through.
fn sign_in_failure(error: StoreError) -> String {
    match error {
        StoreError::Refused(detail) => format!("{} ({detail})", T::SignInRefused.s()),
        StoreError::Unreachable => T::StoreUnreachable.s().to_string(),
        StoreError::Unexpected(detail) => format!("{}: {detail}", T::SignInFailed.s()),
    }
}

#[tauri::command]
pub(crate) fn list_store_accounts(app: AppHandle) -> Result<Vec<StoreAccountInfo>, String> {
    let store = account_store(&app)?;
    Ok(STORES.iter().map(|platform| info_of(&store, platform)).collect())
}

/// Opens the store's own sign-in page in the browser. The address is built here, never taken
/// from the interface.
#[tauri::command]
pub(crate) fn open_store_sign_in(app: AppHandle, platform: String) -> Result<(), String> {
    let url = match supported(&platform)? {
        EPIC => epic_account::sign_in_url(),
        GOG => gog_account::sign_in_url(),
        _ => return Err(T::StoreNotSupported.s().to_string()),
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
    let platform = supported(&platform)?;
    let store = account_store(&app)?;
    let code = authorization_code_in(&pasted).ok_or_else(|| T::SignInCodeMissing.s().to_string())?;

    // Who signed in, and the token that keeps the session
    let signed_in = tauri::async_runtime::spawn_blocking(move || match platform {
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
    .map_err(sign_in_failure)?;

    let (account_id, display_name, refresh_token) = signed_in;
    store.save(
        platform,
        &StoreAccount { account_id, display_name, refresh_token, connected_at: now_timestamp_ms() },
    )?;
    let _ = app.emit(ACCOUNTS_CHANGED_EVENT, ());
    Ok(info_of(&store, platform))
}

/// Signs out: the session is forgotten. Games already in the library stay.
#[tauri::command]
pub(crate) fn store_sign_out(app: AppHandle, platform: String) -> Result<(), String> {
    let platform = supported(&platform)?;
    account_store(&app)?.remove(platform)?;
    let _ = app.emit(ACCOUNTS_CHANGED_EVENT, ());
    Ok(())
}

/// Brings in the games of every store account someone is signed in to; `on_added` gets each
/// group of games as it is stored. A session a store no longer accepts is dropped (the
/// interface then asks to sign in again); a store that cannot be reached is left for the
/// next time.
pub(crate) fn sync_store_accounts(app: &AppHandle, db: &SharedDb, on_added: &dyn Fn(&[DetectedGame])) {
    let Ok(store) = account_store(app) else {
        return;
    };
    for (platform, outcome) in [
        (EPIC, sync_epic_account(&store, db, on_added)),
        (GOG, sync_gog_account(&store, db, on_added)),
    ] {
        if let Err(StoreError::Refused(_)) = outcome {
            let _ = store.mark_session_refused(platform);
            let _ = app.emit(ACCOUNTS_CHANGED_EVENT, ());
        }
    }
}

/// Epic: renews the session, reads the account's library and asks the catalog about what the
/// library does not know, a few items at a time.
fn sync_epic_account(
    store: &AccountStore,
    db: &SharedDb,
    on_added: &dyn Fn(&[DetectedGame]),
) -> Result<(), StoreError> {
    let Some(account) = store.load(EPIC) else {
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
    if store.save(EPIC, &renewed).is_err() {
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
    store: &AccountStore,
    db: &SharedDb,
    on_added: &dyn Fn(&[DetectedGame]),
) -> Result<(), StoreError> {
    let Some(account) = store.load(GOG) else {
        return Ok(());
    };
    let api = GogApi::new();
    let session = api.refresh(&account.refresh_token)?;
    let renewed = StoreAccount { refresh_token: session.refresh_token.clone(), ..account };
    if store.save(GOG, &renewed).is_err() {
        return Ok(());
    }

    let added = kanso_core::owned::merge_owned_games(db, api.library(&session)?, now_timestamp_ms());
    if !added.is_empty() {
        on_added(&added);
    }
    Ok(())
}
