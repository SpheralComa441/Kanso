mod accounts;
mod emulators;
mod playtime;
mod plugins;
mod browser_signin;
mod gamepass;
mod signin_window;
mod dlss;
mod news;
mod weather;

use kanso_core::deals::is_allowed_store_url;
use kanso_core::theme::{
    delete_user_theme, export_theme_file, import_theme_file, list_user_themes, save_user_theme,
};
use kanso_core::{
    current_language, is_absent_cover_due, is_metadata_sync_due, now_timestamp_ms, scan_library,
    set_language, AppConfig, DealStore, DealsClient, DealsReport, DetectedGame, GameManualUpdate, GamePlatform, GameStatus,
    Lang, LibraryDb, LibraryStore, MetadataClient, ScanReport, SgdbGameMatch, Theme, T,
};
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex, MutexGuard};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_opener::OpenerExt;

pub(crate) const GAME_UPDATED_EVENT: &str = "library:game-updated";
/// Several games changed at once (payload: the games as stored).
pub(crate) const GAMES_UPDATED_EVENT: &str = "library:games-updated";
/// Games left the library (entries a store's list was wrongly read into): their ids.
pub(crate) const GAMES_REMOVED_EVENT: &str = "library:games-removed";
/// Pause between two searches of the Steam store while matching a library to its pages.
const STORE_SEARCH_PAUSE: std::time::Duration = std::time::Duration::from_millis(250);

/// Shared handle to the library database; commands lock it only around reads and writes,
/// never across network requests.
pub(crate) type SharedDb = Arc<Mutex<LibraryDb>>;

/// How long fetched deals are reused before the stores are asked again.
const DEALS_CACHE_TTL_MS: u64 = 30 * 60 * 1000;

struct CachedDeals {
    lang: Lang,
    country: String,
    /// The stores that were asked: a store shown again asks afresh.
    stores: Vec<DealStore>,
    report: DealsReport,
}

pub(crate) struct AppState {
    pub(crate) db: SharedDb,
    /// Problems found while opening the library at startup, reported with the first scan.
    startup_warnings: Mutex<Vec<String>>,
    deals_cache: Arc<Mutex<Option<CachedDeals>>>,
    /// The last answer about the weather for the Home page.
    weather_cache: weather::WeatherCache,
    /// What is known about DLSS while Kanso runs: NVIDIA's newest version, the games that carry it.
    pub(crate) dlss_cache: dlss::DlssCache,
    /// The last news read from the feeds, and for which sources.
    pub(crate) news_cache: news::NewsCache,
    /// Games Kanso currently sees running, kept by the play time tracker.
    pub(crate) running_games: Arc<Mutex<Vec<String>>>,
    pub(crate) plugins: Arc<plugins::PluginHost>,
    /// The background pass that completes the library is under way: one at a time.
    completing_library: Arc<std::sync::atomic::AtomicBool>,
    /// It was asked for again while under way (someone just signed in to a store): once done,
    /// it starts over.
    complete_library_again: Arc<std::sync::atomic::AtomicBool>,
    /// The emulators the last scan found.
    pub(crate) emulators: emulators::SharedEmulators,
    /// Emulated games started from Kanso, for the play time tracker.
    pub(crate) emulated_sessions: emulators::EmulatedSessions,
}

/// Language used when none is configured yet (the UI picks one from the OS on first run).
pub(crate) const FALLBACK_LANGUAGE: Lang = Lang::En;

pub(crate) fn lock_db(db: &SharedDb) -> MutexGuard<'_, LibraryDb> {
    db.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub(crate) fn resolve_app_data_dir(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_local_data_dir()
        .map_err(|e| format!("{}: {e}", T::AppDataUnavailable.s()))
}

pub(crate) fn resolve_config_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(resolve_app_data_dir(app)?.join("config.json"))
}

/// The saved settings, or the defaults when they cannot be read.
pub(crate) fn load_config(app: &AppHandle) -> AppConfig {
    resolve_config_path(app)
        .ok()
        .and_then(|path| AppConfig::load_from_file(&path).ok())
        .unwrap_or_default()
}

fn resolve_covers_dir(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(resolve_app_data_dir(app)?.join("covers"))
}

fn resolve_themes_dir(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(resolve_app_data_dir(app)?.join("themes"))
}

pub(crate) fn game_not_found(game_id: &str) -> String {
    format!("{}: {game_id}", T::GameNotFound.s())
}

/// Loads the one game a command works on as a single-entry store, so the merge rules in
/// [`LibraryStore`] apply to exactly what is in the database right now.
fn load_single_game_store(db: &LibraryDb, game_id: &str) -> Result<LibraryStore, String> {
    let game = db
        .get_game(game_id)?
        .ok_or_else(|| game_not_found(game_id))?;
    Ok(LibraryStore { games: vec![game] })
}

#[tauri::command]
fn get_settings(app: AppHandle) -> Result<AppConfig, String> {
    let config_path = resolve_config_path(&app)?;
    AppConfig::load_from_file(&config_path)
}

#[tauri::command]
fn save_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    config: AppConfig,
) -> Result<AppConfig, String> {
    let config_path = resolve_config_path(&app)?;
    let before = load_config(&app);
    let (had_game_pass, listed_catalog) = (before.has_game_pass, before.game_pass_catalog);
    let saved = config.save_to_file(&config_path)?;
    apply_language(&app, saved.lang().unwrap_or(FALLBACK_LANGUAGE));
    if saved.has_game_pass != had_game_pass {
        accounts::apply_game_pass(&app, &state.db, saved.has_game_pass);
    }
    if saved.game_pass_catalog != listed_catalog {
        gamepass::apply_setting(&app, &state.db, saved.game_pass_catalog);
    }
    // A plugin may have been switched on, or its settings changed
    state.plugins.wake_background_fill();
    Ok(saved)
}

/// Makes `lang` the language of backend messages and of the tray menu.
fn apply_language(app: &AppHandle, lang: Lang) {
    if lang == current_language() && app.tray_by_id(TRAY_ICON_ID).is_some() {
        return;
    }
    set_language(lang);
    if let (Some(tray), Ok(menu)) = (app.tray_by_id(TRAY_ICON_ID), build_tray_menu(app)) {
        let _ = tray.set_menu(Some(menu));
    }
}

#[tauri::command]
async fn scan_games(
    app: AppHandle,
    state: State<'_, AppState>,
    // The user asked for this scan: emulators are searched for on the disks as well
    thorough: Option<bool>,
) -> Result<ScanReport, String> {
    let config_path = resolve_config_path(&app)?;
    let config = AppConfig::load_from_file(&config_path)?;
    let db = Arc::clone(&state.db);
    let startup_warnings: Vec<String> = state
        .startup_warnings
        .lock()
        .map(|mut pending| std::mem::take(&mut *pending))
        .unwrap_or_default();

    let emulators = Arc::clone(&state.emulators);
    let emulators_file = emulators::remembered_file(&app);

    tauri::async_runtime::spawn_blocking(move || {
        let (raw_report, found_emulators) =
            scan_library(&config, emulators_file.as_deref(), thorough.unwrap_or(false));
        if let Ok(mut known) = emulators.lock() {
            *known = found_emulators;
        }

        let mut db = lock_db(&db);
        let mut store = LibraryStore::load(&db)?;
        let before = store.games.clone();
        let mut merged = store.merge_with_scan(raw_report);
        store.save_all(&mut db)?;

        // What Steam's own counters gained since the last scan becomes play history
        let played = kanso_core::playtime::steam_sessions_between(
            &before,
            &merged.games,
            now_timestamp_ms(),
        );
        for session in &played {
            let _ = db.insert_play_session(session);
        }

        merged.warnings.splice(0..0, startup_warnings);
        Ok(merged)
    })
    .await
    .map_err(|e| format!("{}: {e}", T::ScanFailed.s()))?
}

/// Fills in missing covers, descriptions, genres and official names.
///
/// Games that are still incomplete after an attempt are retried at most once a day; pass
/// `retry_now` when the user explicitly asked (manual scan, new API key).
#[tauri::command]
async fn sync_library_metadata(
    app: AppHandle,
    state: State<'_, AppState>,
    retry_now: bool,
) -> Result<ScanReport, String> {
    let config_path = resolve_config_path(&app)?;
    let covers_dir = resolve_covers_dir(&app)?;
    let config = AppConfig::load_from_file(&config_path)?;
    let db = Arc::clone(&state.db);
    let app_handle = app.clone();
    let plugin_host = Arc::clone(&state.plugins);

    tauri::async_runtime::spawn_blocking(move || {
        let has_api_key = config.has_api_key();
        let lang = config.lang().unwrap_or(FALLBACK_LANGUAGE);
        let now = now_timestamp_ms();

        let client = MetadataClient::new().with_language(lang);
        let api_key = has_api_key.then_some(config.steamgriddb_api_key.as_str());

        let candidates: Vec<DetectedGame> = lock_db(&db)
            .load_games()?
            .into_iter()
            .filter(|game| is_metadata_sync_due(game, has_api_key, lang, now, retry_now))
            .collect();

        for mut game in candidates {
            let game_id = game.id.clone();
            let outcome = client.enrich_game_automatically(&mut game, api_key, &covers_dir);

            // Write back against the game's current row: favorites, visibility and manual
            // overrides changed while the requests were in flight are kept
            let committed = {
                let db = lock_db(&db);
                let Ok(mut current) = load_single_game_store(&db, &game_id) else {
                    continue;
                };
                let committed = current.commit_enriched_game(game, outcome.auto_hidden);
                if let Some(updated) = &committed {
                    let _ = db.upsert_game(updated);
                }
                committed
            };

            if let Some(updated) = committed.filter(|_| outcome.changed) {
                let _ = app_handle.emit(GAME_UPDATED_EVENT, updated);
            }
        }

        // Steam games the account owns or has played but that are not on disk join the
        // library as "not installed", from the Steam client's own records. They come with
        // name and cover; the report below carries them to the interface in one go.
        if let Some(steam_root) = kanso_core::scanner::steam::detect_steam_root() {
            let play_stats = kanso_core::scanner::steam::read_steam_play_stats(&steam_root);
            let owned_apps = kanso_core::scanner::steam_packages::read_owned_steam_apps(&steam_root);
            kanso_core::owned::sync_steam_library(&db, &client, &play_stats, &owned_apps, now);
        }
        // The clients of Epic, GOG Galaxy and Ubisoft Connect name the account's games in
        // their own local records: those join too, with no sign-in and no request
        kanso_core::owned::merge_owned_games(
            &db,
            kanso_core::scanner::read_owned_games_from_clients(),
            now,
        );
        // With Game Pass, as the user says in the settings: the games it makes installable
        // on Battle.net, from the list Kanso ships with
        if config.has_game_pass {
            kanso_core::owned::merge_owned_games(&db, kanso_core::battlenet_account::game_pass_games(), now);
        }
        // What is not on disk is left out of the loop above: its genres, tags and description
        // come fifty games per request, so that it can be browsed like the rest
        kanso_core::owned::sync_absent_game_details(&db, &client, now);

        let final_store = LibraryStore::load(&lock_db(&db))?;
        // The library is settled: plugins that label the game cards can catch up quietly
        plugin_host.wake_background_fill();
        Ok(final_store.to_report())
    })
    .await
    .map_err(|e| format!("{}: {e}", T::SyncFailed.s()))?
}

/// Looks up whatever a single game still lacks (description, genres, tags, a local copy of
/// its cover), whatever its state. Games that are not on disk are left out of the automatic
/// sync and are completed through this when their sheet is opened.
#[tauri::command]
async fn complete_game_details(
    app: AppHandle,
    state: State<'_, AppState>,
    game_id: String,
) -> Result<DetectedGame, String> {
    let config_path = resolve_config_path(&app)?;
    let covers_dir = resolve_covers_dir(&app)?;
    let config = AppConfig::load_from_file(&config_path)?;
    let db = Arc::clone(&state.db);
    let app_handle = app.clone();

    tauri::async_runtime::spawn_blocking(move || {
        let mut game = lock_db(&db)
            .get_game(&game_id)?
            .ok_or_else(|| game_not_found(&game_id))?;
        let client =
            MetadataClient::new().with_language(config.lang().unwrap_or(FALLBACK_LANGUAGE));
        let api_key = config
            .has_api_key()
            .then_some(config.steamgriddb_api_key.as_str());
        let outcome = client.enrich_game_automatically(&mut game, api_key, &covers_dir);

        let stored = {
            let db = lock_db(&db);
            let mut current = load_single_game_store(&db, &game_id)?;
            match current.commit_enriched_game(game, outcome.auto_hidden) {
                Some(updated) => {
                    db.upsert_game(&updated)?;
                    updated
                }
                // The user edited the game by hand meanwhile: their version stands
                None => current.games.remove(0),
            }
        };
        if outcome.changed {
            let _ = app_handle.emit(GAME_UPDATED_EVENT, stored.clone());
        }
        Ok(stored)
    })
    .await
    .map_err(|e| format!("{}: {e}", T::SyncFailed.s()))?
}

/// Covers looked for in one pass for games that are not on disk and came without one.
const ABSENT_COVERS_PER_PASS: usize = 60;

/// Looks for a cover for the games that are not installed and have none at all: their store
/// listed them without art. SteamGridDB is asked when a key is set, then Steam, for the page
/// carrying exactly the game's title. A game nobody has art for is asked about again after a
/// day. Each cover found reaches the interface as it is stored.
fn find_covers_for_absent_games(
    app: &AppHandle,
    db: &SharedDb,
    client: &MetadataClient,
    config: &AppConfig,
    covers_dir: &Path,
) {
    let now = now_timestamp_ms();
    let api_key = config.has_api_key().then_some(config.steamgriddb_api_key.as_str());
    let due: Vec<DetectedGame> = lock_db(db)
        .load_games()
        .unwrap_or_default()
        .into_iter()
        .filter(|game| is_absent_cover_due(game, now, false))
        .take(ABSENT_COVERS_PER_PASS)
        .collect();

    for mut game in due {
        let found = client.find_missing_cover(&mut game, api_key, covers_dir);
        // Written against the game as it is now: only the cover and the note that it was
        // looked for; whatever else changed meanwhile stays
        let db = lock_db(db);
        let Ok(Some(mut current)) = db.get_game(&game.id) else {
            continue;
        };
        current.metadata_attempted_at = game.metadata_attempted_at;
        let stored = found && current.cover_path.is_none();
        if stored {
            current.cover_path = game.cover_path;
            current.steamgriddb_id = current.steamgriddb_id.or(game.steamgriddb_id);
            current.updated_at = game.updated_at;
        }
        if db.upsert_game(&current).is_ok() && stored {
            let _ = app.emit(GAME_UPDATED_EVENT, current);
        }
    }
}

/// The slow part of completing the library, apart from the sync so that it never holds it up:
/// 1. the games of the store accounts the user signed in to join the library (one question to
///    the store's catalog per item not seen before);
/// 2. the games of other stores that are not installed are matched to their Steam store page,
///    a few at a time, and given the genres, tags and description found there.
///
/// A library of a few hundred games takes some minutes the first time. What is completed
/// reaches the interface in batches through [`GAMES_UPDATED_EVENT`]. Each part stops quietly
/// while its store cannot be reached; what is left is picked up by the next call.
#[tauri::command]
async fn complete_library(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    use std::sync::atomic::Ordering;

    let running = Arc::clone(&state.completing_library);
    let again = Arc::clone(&state.complete_library_again);
    if running.swap(true, Ordering::SeqCst) {
        again.store(true, Ordering::SeqCst);
        return Ok(());
    }
    let covers_dir = resolve_covers_dir(&app)?;
    let db = Arc::clone(&state.db);

    let outcome = tauri::async_runtime::spawn_blocking(move || loop {
        // Read at each pass: a pass asked for while another runs follows a change of settings
        let config = load_config(&app);
        accounts::sync_store_accounts(&app, &db, &|added| {
            let _ = app.emit(GAMES_UPDATED_EVENT, added);
        });
        // The Game Pass catalog, where the user asked for it: public, read once a day
        if config.game_pass_catalog {
            gamepass::sync_catalog(&app, &db, config.lang().unwrap_or(FALLBACK_LANGUAGE));
        }

        let client =
            MetadataClient::new().with_language(config.lang().unwrap_or(FALLBACK_LANGUAGE));
        // Before the long matching below: the games a store listed without any cover
        // (Battle.net gives none) are few, and are what the eye falls on first
        find_covers_for_absent_games(&app, &db, &client, &config, &covers_dir);
        loop {
            let step = kanso_core::owned::match_absent_games_step(
                &db,
                &client,
                STORE_SEARCH_PAUSE,
                now_timestamp_ms(),
            );
            if !step.changed.is_empty() {
                let _ = app.emit(GAMES_UPDATED_EVENT, &step.changed);
            }
            if step.unreachable || step.remaining == 0 {
                break;
            }
            std::thread::sleep(STORE_SEARCH_PAUSE);
        }

        if !again.swap(false, Ordering::SeqCst) {
            break;
        }
    })
    .await;
    running.store(false, Ordering::SeqCst);
    outcome.map_err(|e| format!("{}: {e}", T::SyncFailed.s()))
}

#[tauri::command]
async fn search_steamgriddb_games(
    app: AppHandle,
    query: String,
) -> Result<Vec<SgdbGameMatch>, String> {
    let config_path = resolve_config_path(&app)?;
    let config = AppConfig::load_from_file(&config_path)?;
    if !config.has_api_key() {
        return Err(T::SgdbKeyMissing.s().to_string());
    }

    tauri::async_runtime::spawn_blocking(move || {
        let client = MetadataClient::new();
        client.search_steamgriddb(&config.steamgriddb_api_key, &query)
    })
    .await
    .map_err(|e| format!("{}: {e}", T::OperationFailed.s()))?
}

#[tauri::command]
async fn apply_steamgriddb_match(
    app: AppHandle,
    state: State<'_, AppState>,
    game_id: String,
    sgdb_game_id: u64,
    matched_name: String,
) -> Result<DetectedGame, String> {
    let config_path = resolve_config_path(&app)?;
    let covers_dir = resolve_covers_dir(&app)?;
    let config = AppConfig::load_from_file(&config_path)?;

    if !config.has_api_key() {
        return Err(T::SgdbKeyMissing.s().to_string());
    }

    let db = Arc::clone(&state.db);
    let app_handle = app.clone();

    tauri::async_runtime::spawn_blocking(move || {
        let mut game = lock_db(&db)
            .get_game(&game_id)?
            .ok_or_else(|| game_not_found(&game_id))?;

        let client =
            MetadataClient::new().with_language(config.lang().unwrap_or(FALLBACK_LANGUAGE));
        client.apply_manual_sgdb_selection(
            &mut game,
            &config.steamgriddb_api_key,
            sgdb_game_id,
            &matched_name,
            &covers_dir,
        )?;

        let game = {
            let db = lock_db(&db);
            let mut current = load_single_game_store(&db, &game_id)?;
            let committed = current
                .commit_enriched_game(game, false)
                .ok_or_else(|| game_not_found(&game_id))?;
            db.upsert_game(&committed)?;
            committed
        };

        let _ = app_handle.emit(GAME_UPDATED_EVENT, game.clone());
        Ok(game)
    })
    .await
    .map_err(|e| format!("{}: {e}", T::OperationFailed.s()))?
}

/// Applies `change` to the stored game `game_id`, saves it and notifies the UI.
fn update_stored_game(
    app: &AppHandle,
    state: &State<'_, AppState>,
    game_id: &str,
    change: impl FnOnce(&mut LibraryStore) -> Result<DetectedGame, String>,
) -> Result<DetectedGame, String> {
    let updated = {
        let db = lock_db(&state.db);
        let mut current = load_single_game_store(&db, game_id)?;
        let updated = change(&mut current)?;
        db.upsert_game(&updated)?;
        updated
    };
    let _ = app.emit(GAME_UPDATED_EVENT, updated.clone());
    Ok(updated)
}

#[tauri::command]
fn update_game_details(
    app: AppHandle,
    state: State<'_, AppState>,
    payload: GameManualUpdate,
) -> Result<DetectedGame, String> {
    let game_id = payload.id.clone();
    update_stored_game(&app, &state, &game_id, |store| {
        store.apply_manual_update(payload)
    })
}

#[tauri::command]
fn set_game_local_cover(
    app: AppHandle,
    state: State<'_, AppState>,
    game_id: String,
    image_path: String,
) -> Result<DetectedGame, String> {
    let covers_dir = resolve_covers_dir(&app)?;
    update_stored_game(&app, &state, &game_id, |store| {
        store.set_local_cover_file(&game_id, Path::new(&image_path), &covers_dir)
    })
}

#[tauri::command]
fn set_game_favorite(
    app: AppHandle,
    state: State<'_, AppState>,
    game_id: String,
    is_favorite: bool,
) -> Result<DetectedGame, String> {
    update_stored_game(&app, &state, &game_id, |store| {
        store.set_favorite(&game_id, is_favorite)
    })
}

#[tauri::command]
fn set_game_hidden(
    app: AppHandle,
    state: State<'_, AppState>,
    game_id: String,
    is_hidden: bool,
) -> Result<DetectedGame, String> {
    update_stored_game(&app, &state, &game_id, |store| {
        store.set_hidden(&game_id, is_hidden)
    })
}

/// Sets where the user stands with a game (`backlog`, `playing`, `finished`, `abandoned`), or
/// clears it with `null`.
#[tauri::command]
fn set_game_status(
    app: AppHandle,
    state: State<'_, AppState>,
    game_id: String,
    status: Option<String>,
) -> Result<DetectedGame, String> {
    let status = match status.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(raw) => Some(
            GameStatus::parse(raw).ok_or_else(|| format!("{}: {raw}", T::OperationFailed.s()))?,
        ),
        None => None,
    };
    update_stored_game(&app, &state, &game_id, |store| {
        store.set_status(&game_id, status)
    })
}

/// Sets the genres the user gives a game besides the ones its store lists.
#[tauri::command]
fn set_game_genres(
    app: AppHandle,
    state: State<'_, AppState>,
    game_id: String,
    genres: Vec<String>,
) -> Result<DetectedGame, String> {
    update_stored_game(&app, &state, &game_id, |store| store.set_custom_genres(&game_id, &genres))
}

/// Launches a game by library id. The launch target is always read from the local store,
/// never taken from the webview.
#[tauri::command]
fn launch_game(
    app: AppHandle,
    state: State<'_, AppState>,
    game_id: String,
) -> Result<(), String> {
    let game = lock_db(&state.db)
        .get_game(&game_id)?
        .ok_or_else(|| game_not_found(&game_id))?;
    if !game.is_installed {
        return Err(format!("{}: {}", T::GameNotInstalled.s(), game.title));
    }

    if game.platform == GamePlatform::Emulator {
        emulators::launch_emulated_game(&app, &state, &game)?;
    } else {
        launch_game_inner(&app, &game)?;
    }

    // Play history is a nicety: failing to record it must not turn a good launch into an error
    let _ = update_stored_game(&app, &state, &game_id, |store| {
        store.record_launch(&game_id, now_timestamp_ms())
    });

    let hide_to_tray = resolve_config_path(&app)
        .ok()
        .and_then(|p| AppConfig::load_from_file(&p).ok())
        .is_some_and(|c| c.minimize_to_tray_on_launch);
    if hide_to_tray {
        hide_main_window_to_tray(&app);
    }
    Ok(())
}

/// Asks the game's own client (Steam, Epic, GOG Galaxy, Ubisoft Connect) to install a game
/// that is in the library but not on disk. The address is built from what is stored for the
/// game, never taken from the webview.
#[tauri::command]
fn install_game(app: AppHandle, state: State<'_, AppState>, game_id: String) -> Result<(), String> {
    let game = lock_db(&state.db)
        .get_game(&game_id)?
        .ok_or_else(|| game_not_found(&game_id))?;
    let uri = game
        .install_uri()
        .ok_or_else(|| T::NoInstallTarget.s().to_string())?;
    app.opener()
        .open_url(&uri, None::<&str>)
        .map_err(|e| format!("{} \"{}\": {e}", T::LaunchFailed.s(), game.title))
}

/// Asks the game's own client to uninstall it, or, for a client that has no such request, to
/// show the game so that it can be uninstalled there. Kanso deletes nothing itself, and the
/// address is built from what the library stores for the game.
#[tauri::command]
fn uninstall_game(app: AppHandle, state: State<'_, AppState>, game_id: String) -> Result<(), String> {
    let game = lock_db(&state.db)
        .get_game(&game_id)?
        .ok_or_else(|| game_not_found(&game_id))?;
    let uri = game
        .uninstall_uri()
        .ok_or_else(|| T::NoUninstallTarget.s().to_string())?;
    app.opener()
        .open_url(&uri, None::<&str>)
        .map_err(|e| format!("{} \"{}\": {e}", T::LaunchFailed.s(), game.title))
}

const TRAY_ICON_ID: &str = "kanso-tray";
const TRAY_MENU_SHOW_ID: &str = "tray-show";
const TRAY_MENU_QUIT_ID: &str = "tray-quit";

fn show_main_window(app: &AppHandle) {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.show();
        let _ = win.unminimize();
        let _ = win.set_focus();
    }
}

/// Hides the main window, leaving Kanso reachable from its system tray icon. Falls back to a
/// plain minimize when the tray icon could not be created, so the window is never lost.
fn hide_main_window_to_tray(app: &AppHandle) {
    let Some(win) = app.get_webview_window("main") else {
        return;
    };
    if app.tray_by_id(TRAY_ICON_ID).is_some() {
        let _ = win.hide();
    } else {
        let _ = win.minimize();
    }
}

fn build_tray_menu(app: &AppHandle) -> tauri::Result<Menu<tauri::Wry>> {
    let show_item = MenuItem::with_id(app, TRAY_MENU_SHOW_ID, T::TrayOpen.s(), true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, TRAY_MENU_QUIT_ID, T::TrayQuit.s(), true, None::<&str>)?;
    Menu::with_items(app, &[&show_item, &quit_item])
}

/// Creates the system tray icon: left click (or the first menu entry) restores the window,
/// the second entry quits.
fn setup_system_tray(app: &AppHandle) -> tauri::Result<()> {
    let menu = build_tray_menu(app)?;

    let mut builder = TrayIconBuilder::with_id(TRAY_ICON_ID)
        .tooltip("Kanso")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            TRAY_MENU_SHOW_ID => show_main_window(app),
            TRAY_MENU_QUIT_ID => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main_window(tray.app_handle());
            }
        });

    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }

    builder.build(app)?;
    Ok(())
}

/// Current discounts on Steam, Epic, GOG and, through CheapShark, the other shops. A store
/// the user left out of the Store page is not asked. Results are reused for half an hour
/// unless `force_refresh`; `country` (ISO code) selects the regional prices.
#[tauri::command]
async fn fetch_deals(
    app: AppHandle,
    state: State<'_, AppState>,
    country: String,
    force_refresh: bool,
) -> Result<DealsReport, String> {
    let config = resolve_config_path(&app).ok().and_then(|p| AppConfig::load_from_file(&p).ok());
    let lang = config.as_ref().and_then(|c| c.lang()).unwrap_or(FALLBACK_LANGUAGE);
    let hidden = config.map(|c| c.store.hidden_stores).unwrap_or_default();
    let stores: Vec<DealStore> = DealStore::ALL
        .into_iter()
        .filter(|store| !hidden.iter().any(|name| name == store.name()))
        .collect();
    let country = kanso_core::deals::normalize_country(&country);
    let cache = Arc::clone(&state.deals_cache);

    if !force_refresh {
        let cached = cache.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(entry) = cached.as_ref() {
            let fresh = now_timestamp_ms().saturating_sub(entry.report.fetched_at) < DEALS_CACHE_TTL_MS;
            if fresh && entry.lang == lang && entry.country == country && entry.stores == stores {
                return Ok(entry.report.clone());
            }
        }
    }

    tauri::async_runtime::spawn_blocking(move || {
        let mut report = DealsClient::new().fetch_stores(lang, &country, &stores);
        // Steam's list of discounts does not say what kind of game each one is: its store
        // service does, for all of them in one request (and for the games of the other shops
        // that are on Steam too). Without an answer the deals are listed all the same, just
        // not among the suggestions by genre. With Steam left out, Steam is not asked this
        // either.
        let mut steam_ids: Vec<u64> = report
            .deals
            .iter()
            .filter_map(|deal| deal.steam_appid.as_deref()?.parse().ok())
            .collect();
        steam_ids.sort_unstable();
        steam_ids.dedup();
        if !steam_ids.is_empty() && stores.contains(&DealStore::Steam) {
            let details = MetadataClient::new().with_language(lang).fetch_steam_app_details(&steam_ids);
            for detail in details.unwrap_or_default() {
                let appid = detail.appid.to_string();
                for deal in report.deals.iter_mut().filter(|deal| deal.steam_appid.as_deref() == Some(appid.as_str())) {
                    deal.genres = detail.genres.clone();
                }
            }
        }
        // A fully failed fetch (offline) is not worth caching: the next visit retries
        if !report.deals.is_empty() {
            *cache.lock().unwrap_or_else(|p| p.into_inner()) = Some(CachedDeals {
                lang,
                country,
                stores,
                report: report.clone(),
            });
        }
        report
    })
    .await
    .map_err(|e| format!("{}: {e}", T::OperationFailed.s()))
}

/// Opens a deal's store page in the default browser. Only `https` pages on the stores' own
/// domains are accepted: the URLs originate from remote JSON.
#[tauri::command]
fn open_store_page(app: AppHandle, url: String) -> Result<(), String> {
    if !is_allowed_store_url(&url) {
        return Err(format!("{}: {url}", T::UrlNotAllowed.s()));
    }
    app.opener()
        .open_url(&url, None::<&str>)
        .map_err(|e| format!("{}: {e}", T::OperationFailed.s()))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct UserThemes {
    themes: Vec<Theme>,
    /// One entry per file in the themes folder that is not a valid theme.
    warnings: Vec<String>,
}

#[tauri::command]
fn list_themes(app: AppHandle) -> Result<UserThemes, String> {
    let (themes, warnings) = list_user_themes(&resolve_themes_dir(&app)?);
    Ok(UserThemes { themes, warnings })
}

#[tauri::command]
fn import_theme(app: AppHandle, path: String) -> Result<Theme, String> {
    import_theme_file(&resolve_themes_dir(&app)?, Path::new(&path))
}

/// Saves a theme made in the editor; with `replace_id` it overwrites that installed theme.
#[tauri::command]
fn save_theme(app: AppHandle, theme: Theme, replace_id: Option<String>) -> Result<Theme, String> {
    save_user_theme(&resolve_themes_dir(&app)?, &theme, replace_id.as_deref())
}

#[tauri::command]
fn delete_theme(app: AppHandle, id: String) -> Result<(), String> {
    delete_user_theme(&resolve_themes_dir(&app)?, &id)
}

#[tauri::command]
fn export_theme(theme: Theme, path: String) -> Result<(), String> {
    export_theme_file(&theme, Path::new(&path))
}

/// Opens the themes folder in the file manager, so theme files can simply be dropped in.
#[tauri::command]
fn open_themes_folder(app: AppHandle) -> Result<(), String> {
    let dir = resolve_themes_dir(&app)?;
    fs::create_dir_all(&dir)
        .map_err(|e| format!("{} {}: {e}", T::CannotCreateFolder.s(), dir.display()))?;
    app.opener()
        .open_path(dir.to_string_lossy(), None::<&str>)
        .map_err(|e| format!("{}: {e}", T::OperationFailed.s()))
}

/// Opens the folder a game is in, in the file manager: the folder of its executable (or of
/// its game file), else its install folder. Which folder is read from the library, never
/// taken from the webview, and nothing in it is touched.
#[tauri::command]
fn open_game_folder(app: AppHandle, state: State<'_, AppState>, game_id: String) -> Result<(), String> {
    let game = lock_db(&state.db)
        .get_game(&game_id)?
        .ok_or_else(|| game_not_found(&game_id))?;
    let beside_executable = game
        .executable_path
        .as_deref()
        // A RetroArch game can be a file inside an archive: "archive.zip#game.sfc"
        .map(|path| path.split('#').next().unwrap_or(path))
        .and_then(|path| Path::new(path).parent())
        .filter(|dir| dir.is_dir());
    let dir = beside_executable
        .or_else(|| Some(Path::new(&game.install_dir)).filter(|dir| !game.install_dir.trim().is_empty() && dir.is_dir()))
        .ok_or_else(|| T::FolderNotFound.s().to_string())?;
    app.opener()
        .open_path(dir.to_string_lossy(), None::<&str>)
        .map_err(|e| format!("{}: {e}", T::OperationFailed.s()))
}

fn launch_game_inner(app: &AppHandle, game: &DetectedGame) -> Result<(), String> {
    // Prefer protocol URI when available (Steam `steam://rungameid/{appid}`, Epic launcher URI)
    if let Some(uri) = game.launch_uri.as_deref().filter(|u| !u.trim().is_empty()) {
        match app.opener().open_url(uri, None::<&str>) {
            Ok(()) => return Ok(()),
            Err(uri_err) => {
                if let Some(exe) = game
                    .executable_path
                    .as_deref()
                    .filter(|p| !p.trim().is_empty())
                {
                    if Path::new(exe).is_file() {
                        return spawn_executable(exe);
                    }
                }
                return Err(format!(
                    "{} \"{}\": {uri_err}",
                    T::LaunchFailed.s(),
                    game.title
                ));
            }
        }
    }

    if let Some(exe) = game
        .executable_path
        .as_deref()
        .filter(|p| !p.trim().is_empty())
    {
        return spawn_executable(exe);
    }

    Err(format!("{} \"{}\".", T::NoLaunchTarget.s(), game.title))
}

fn spawn_executable(exe_path: &str) -> Result<(), String> {
    let path = Path::new(exe_path);
    if !path.is_file() {
        return Err(format!("{}: {}", T::ExeNotFound.s(), path.display()));
    }

    let mut cmd = Command::new(path);
    if let Some(parent) = path.parent() {
        cmd.current_dir(parent);
    }

    cmd.spawn()
        .map(|_| ())
        .map_err(|e| format!("{} {}: {e}", T::LaunchFailed.s(), path.display()))
}

/// Removes the SQLite mirror older versions kept through `tauri-plugin-sql`. It was never read
/// back; the library now lives in `library.db`.
fn remove_legacy_sql_mirror(app: &AppHandle) {
    let Ok(config_dir) = app.path().app_config_dir() else {
        return;
    };
    for name in ["kanso.db", "kanso.db-wal", "kanso.db-shm"] {
        let _ = fs::remove_file(config_dir.join(name));
    }
}

/// Opens the library and prepares everything the commands rely on.
fn setup_app(app: &AppHandle) -> Result<(), String> {
    let data_dir = resolve_app_data_dir(app)?;

    // Language first: everything below may produce user-facing messages
    let configured_lang = AppConfig::load_from_file(&data_dir.join("config.json"))
        .ok()
        .and_then(|config| config.lang());
    set_language(configured_lang.unwrap_or(FALLBACK_LANGUAGE));

    let (db, startup_warnings) = LibraryDb::open_or_recover(&data_dir);
    let db: SharedDb = Arc::new(Mutex::new(db));
    let running_games = Arc::new(Mutex::new(Vec::new()));
    let emulated_sessions: emulators::EmulatedSessions = Arc::default();
    let plugin_host = Arc::new(plugins::PluginHost::new(
        &app.package_info().version.to_string(),
        data_dir.join("plugins"),
    ));
    app.manage(AppState {
        db: Arc::clone(&db),
        startup_warnings: Mutex::new(startup_warnings),
        deals_cache: Arc::new(Mutex::new(None)),
        weather_cache: Arc::default(),
        dlss_cache: Arc::default(),
        news_cache: Arc::default(),
        running_games: Arc::clone(&running_games),
        plugins: Arc::clone(&plugin_host),
        completing_library: Arc::default(),
        complete_library_again: Arc::default(),
        emulators: Arc::default(),
        emulated_sessions: Arc::clone(&emulated_sessions),
    });
    remove_legacy_sql_mirror(app);
    accounts::remove_unused_window_profiles(app);
    #[cfg(debug_assertions)]
    accounts::run_self_test_if_asked(app, &db);
    playtime::spawn_tracker(app.clone(), Arc::clone(&db), running_games, emulated_sessions);
    plugins::spawn_background_fill(app.clone(), db, plugin_host);

    // The webview may load local files from the covers folder only. The folder must exist
    // before it is allowed, so the scope also learns its canonical form.
    let covers_dir = data_dir.join("covers");
    fs::create_dir_all(&covers_dir)
        .map_err(|e| format!("{} {}: {e}", T::CannotCreateFolder.s(), covers_dir.display()))?;
    app.asset_protocol_scope()
        .allow_directory(&covers_dir, false)
        .map_err(|e| format!("{}: {e}", T::OperationFailed.s()))?;

    // The tray is a convenience: failing to create it must not prevent startup
    if let Err(err) = setup_system_tray(app) {
        eprintln!("Kanso: could not create the system tray icon: {err}");
    }
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // Must be the first plugin: a second launch (e.g. from the Start menu while Kanso sits
        // in the tray) just brings the running window back instead of starting a new process.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_main_window(app);
        }))
        .setup(|app| setup_app(app.handle()).map_err(Into::into))
        // A sign-in window left open (or one renewing a session unseen) must not keep Kanso
        // running once its own window is gone
        .on_window_event(|window, event| {
            if window.label() == "main" && matches!(event, tauri::WindowEvent::Destroyed) {
                signin_window::close_all(window.app_handle());
            }
        })
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            get_settings,
            save_settings,
            scan_games,
            emulators::list_emulators,
            emulators::identify_emulator,
            emulators::launch_emulator,
            sync_library_metadata,
            complete_game_details,
            complete_library,
            search_steamgriddb_games,
            apply_steamgriddb_match,
            update_game_details,
            set_game_local_cover,
            set_game_favorite,
            set_game_hidden,
            launch_game,
            install_game,
            uninstall_game,
            weather::home_weather,
            weather::search_weather_places,
            weather::open_weather_source,
            fetch_deals,
            open_store_page,
            list_themes,
            import_theme,
            save_theme,
            delete_theme,
            export_theme,
            open_themes_folder,
            set_game_status,
            set_game_genres,
            open_game_folder,
            dlss::game_dlss,
            dlss::dlss_status,
            dlss::dlss_download,
            dlss::dlss_update,
            dlss::dlss_restore,
            dlss::open_dlss_source,
            news::news_sources,
            news::fetch_news,
            news::open_news_link,
            playtime::get_game_achievements,
            playtime::get_play_sessions,
            playtime::get_running_games,
            plugins::list_plugins,
            plugins::inspect_plugin,
            plugins::install_plugin,
            plugins::remove_plugin,
            plugins::open_plugins_folder,
            plugins::plugin_game_data,
            plugins::plugin_badges,
            plugins::run_plugin_command,
            plugins::open_plugin_link,
            accounts::list_store_accounts,
            accounts::open_store_sign_in,
            accounts::store_sign_in,
            accounts::store_sign_in_window,
            accounts::store_sign_in_browser,
            accounts::cancel_store_sign_in_browser,
            accounts::store_sign_out
        ])
        .run(tauri::generate_context!())
        .expect("Kanso failed to start");
}
