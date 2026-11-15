//! The Game Pass catalog for PC in the library, when the user asks for it in the settings.
//!
//! The catalog is Microsoft's public one (see [`kanso_core::gamepass`]): reading it needs no
//! account and sends nothing about the user. It is read at most once a day, and again when
//! the language of the interface changes; what it no longer lists leaves the library unless
//! it is installed.

use crate::accounts::test_site;
use crate::{resolve_app_data_dir, SharedDb, GAMES_REMOVED_EVENT, GAMES_UPDATED_EVENT};
use kanso_core::gamepass::GamePassApi;
use kanso_core::models::now_timestamp_ms;
use kanso_core::owned::{remove_game_pass_catalog, sync_game_pass_catalog};
use kanso_core::Lang;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::{AppHandle, Emitter};

/// When the catalog was last read, and for which language: a note, not a setting.
const STAMP_FILE: &str = "gamepass-catalog.json";
/// The catalog changes a few times a month: once a day is plenty.
const READ_AGAIN_AFTER_MS: u64 = 24 * 60 * 60 * 1000;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Stamp {
    read_at: u64,
    language: String,
}

fn stamp_path(app: &AppHandle) -> Option<PathBuf> {
    Some(resolve_app_data_dir(app).ok()?.join(STAMP_FILE))
}

fn read_recently(app: &AppHandle, lang: Lang, now: u64) -> bool {
    stamp_path(app)
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|text| serde_json::from_str::<Stamp>(&text).ok())
        .is_some_and(|stamp| stamp.language == lang.code() && now.saturating_sub(stamp.read_at) < READ_AGAIN_AFTER_MS)
}

fn forget_stamp(app: &AppHandle) {
    if let Some(path) = stamp_path(app) {
        let _ = std::fs::remove_file(path);
    }
}

fn api() -> GamePassApi {
    // A debug build can be pointed at a stand-in on this PC
    test_site("gamepass").map_or_else(GamePassApi::new, |site| GamePassApi::with_services(&site, &site))
}

/// Brings the library up to date with the catalog, if it has not been read today. A catalog
/// that cannot be read changes nothing: the next pass tries again.
pub(crate) fn sync_catalog(app: &AppHandle, db: &SharedDb, lang: Lang) {
    let now = now_timestamp_ms();
    if read_recently(app, lang, now) {
        return;
    }
    let catalog = match api().pc_catalog(lang) {
        Ok(catalog) => catalog,
        Err(error) => {
            eprintln!("Kanso: Game Pass catalog: {error}");
            return;
        }
    };
    let (added, removed) = sync_game_pass_catalog(db, catalog, now);
    if !removed.is_empty() {
        let _ = app.emit(GAMES_REMOVED_EVENT, &removed);
    }
    if !added.is_empty() {
        let _ = app.emit(GAMES_UPDATED_EVENT, &added);
    }
    if let Some(path) = stamp_path(app) {
        let stamp = Stamp { read_at: now, language: lang.code().to_string() };
        let _ = std::fs::write(path, serde_json::to_string(&stamp).unwrap_or_default());
    }
}

/// The user switched the catalog on or off in the settings. Off: what it brought and is not
/// installed leaves the library at once. On: the next pass over the library reads it.
pub(crate) fn apply_setting(app: &AppHandle, db: &SharedDb, listed: bool) {
    forget_stamp(app);
    if !listed {
        let removed = remove_game_pass_catalog(db);
        if !removed.is_empty() {
            let _ = app.emit(GAMES_REMOVED_EVENT, &removed);
        }
    }
}
