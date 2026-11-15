//! Signing in to store accounts, and bringing their libraries in.
//!
//! The interface never handles a token: it asks for the sign-in page to be opened in the
//! browser, hands over the code the user pasted, and is told who is signed in.

use crate::{resolve_app_data_dir, SharedDb};
use kanso_core::accounts::{AccountStore, StoreAccount};
use kanso_core::epic_account::{self, EpicApi, EpicError};
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
/// The stores Kanso can sign in to.
const STORES: &[&str] = &[EPIC];

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
        display_name: summary.map(|summary| summary.display_name),
    }
}

fn supported(platform: &str) -> Result<&'static str, String> {
    STORES
        .iter()
        .copied()
        .find(|store| *store == platform)
        .ok_or_else(|| T::StoreNotSupported.s().to_string())
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
        _ => return Err(T::StoreNotSupported.s().to_string()),
    };
    app.opener()
        .open_url(&url, None::<&str>)
        .map_err(|e| format!("{}: {e}", T::OperationFailed.s()))
}

/// Signs in with the code the store's page showed. `pasted` may be the code alone or the
/// whole text of that page.
#[tauri::command]
pub(crate) async fn store_sign_in(
    app: AppHandle,
    platform: String,
    pasted: String,
) -> Result<StoreAccountInfo, String> {
    supported(&platform)?;
    let store = account_store(&app)?;
    let code = epic_account::authorization_code_in(&pasted)
        .ok_or_else(|| T::SignInCodeMissing.s().to_string())?;

    let session = tauri::async_runtime::spawn_blocking(move || EpicApi::new().exchange_code(&code))
        .await
        .map_err(|e| format!("{}: {e}", T::SignInFailed.s()))?
        .map_err(|error| match error {
            EpicError::Refused(detail) => format!("{} ({detail})", T::SignInRefused.s()),
            EpicError::Unreachable => T::StoreUnreachable.s().to_string(),
            EpicError::Unexpected(detail) => format!("{}: {detail}", T::SignInFailed.s()),
        })?;

    store.save(
        EPIC,
        &StoreAccount {
            account_id: session.account_id.clone(),
            display_name: session.display_name.clone(),
            refresh_token: session.refresh_token.clone(),
            connected_at: now_timestamp_ms(),
        },
    )?;
    let _ = app.emit(ACCOUNTS_CHANGED_EVENT, ());
    Ok(info_of(&store, EPIC))
}

/// Signs out: the session is forgotten. Games already in the library stay.
#[tauri::command]
pub(crate) fn store_sign_out(app: AppHandle, platform: String) -> Result<(), String> {
    let platform = supported(&platform)?;
    account_store(&app)?.remove(platform)?;
    let _ = app.emit(ACCOUNTS_CHANGED_EVENT, ());
    Ok(())
}

/// Brings in the games of the Epic account, when someone is signed in: renews the session,
/// reads the account's library and asks the catalog about what the library does not know, a
/// few items at a time; `on_added` gets each group of games as it is stored.
///
/// A session Epic no longer accepts is dropped (the interface then asks to sign in again); an
/// Epic that cannot be reached leaves everything for the next time.
pub(crate) fn sync_epic_account(app: &AppHandle, db: &SharedDb, on_added: &dyn Fn(&[DetectedGame])) {
    let Ok(store) = account_store(app) else {
        return;
    };
    let Some(account) = store.load(EPIC) else {
        return;
    };
    let api = EpicApi::new();
    let refused = |store: &AccountStore| {
        let _ = store.mark_session_refused(EPIC);
        let _ = app.emit(ACCOUNTS_CHANGED_EVENT, ());
    };

    let session = match api.refresh(&account.refresh_token) {
        Ok(session) => session,
        Err(EpicError::Refused(_)) => return refused(&store),
        Err(_) => return,
    };
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
        return;
    }

    let assets = match api.library(&session) {
        Ok(assets) => assets,
        Err(EpicError::Refused(_)) => return refused(&store),
        Err(_) => return,
    };
    loop {
        let step = kanso_core::owned::sync_epic_library_step(
            db,
            &api,
            &session,
            &assets,
            CATALOG_PAUSE,
            now_timestamp_ms(),
        );
        match step {
            Ok(step) => {
                if !step.added.is_empty() {
                    on_added(&step.added);
                }
                if step.remaining == 0 {
                    break;
                }
            }
            Err(EpicError::Refused(_)) => return refused(&store),
            Err(_) => break,
        }
        std::thread::sleep(CATALOG_PAUSE);
    }
}
