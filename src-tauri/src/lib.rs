mod playtime;
mod plugins;

use kanso_core::deals::is_allowed_store_url;
use kanso_core::theme::{
    delete_user_theme, export_theme_file, import_theme_file, list_user_themes, save_user_theme,
};
use kanso_core::{
    current_language, is_metadata_sync_due, now_timestamp_ms, scan_all_installed_games,
    set_language, AppConfig, DealsClient, DealsReport, DetectedGame, GameManualUpdate, GameStatus,
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
const GAMES_UPDATED_EVENT: &str = "library:games-updated";
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
    report: DealsReport,
}

pub(crate) struct AppState {
    pub(crate) db: SharedDb,
    /// Problems found while opening the library at startup, reported with the first scan.
    startup_warnings: Mutex<Vec<String>>,
    deals_cache: Arc<Mutex<Option<CachedDeals>>>,
    /// Games Kanso currently sees running, kept by the play time tracker.
    pub(crate) running_games: Arc<Mutex<Vec<String>>>,
    pub(crate) plugins: Arc<plugins::PluginHost>,
    /// A pass matching games to their Steam store pages is under way: one at a time.
    matching_store_pages: Arc<std::sync::atomic::AtomicBool>,
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
    let saved = config.save_to_file(&config_path)?;
    apply_language(&app, saved.lang().unwrap_or(FALLBACK_LANGUAGE));
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
async fn scan_games(app: AppHandle, state: State<'_, AppState>) -> Result<ScanReport, String> {
    let config_path = resolve_config_path(&app)?;
    let config = AppConfig::load_from_file(&config_path)?;
    let db = Arc::clone(&state.db);
    let startup_warnings: Vec<String> = state
        .startup_warnings
        .lock()
        .map(|mut pending| std::mem::take(&mut *pending))
        .unwrap_or_default();

    tauri::async_runtime::spawn_blocking(move || {
        let raw_report = scan_all_installed_games(&config);

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

/// Finds the Steam store page of the games of other stores that are not installed, a few at
/// a time, and gives them the genres, tags and description found there. A library of a few
/// hundred games takes some minutes, so this runs apart from the sync: the games it completes
/// reach the interface in batches through [`GAMES_UPDATED_EVENT`]. Stops quietly while Steam
/// cannot be reached; what is left is picked up by the next call.
#[tauri::command]
async fn match_absent_games(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    use std::sync::atomic::Ordering;

    let running = Arc::clone(&state.matching_store_pages);
    if running.swap(true, Ordering::SeqCst) {
        return Ok(());
    }
    let config = load_config(&app);
    let db = Arc::clone(&state.db);

    let outcome = tauri::async_runtime::spawn_blocking(move || {
        let client =
            MetadataClient::new().with_language(config.lang().unwrap_or(FALLBACK_LANGUAGE));
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

    launch_game_inner(&app, &game)?;

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

/// Current discounts on Steam, Epic and GOG. Results are reused for half an hour unless
/// `force_refresh`; `country` (ISO code) selects the regional prices.
#[tauri::command]
async fn fetch_deals(
    app: AppHandle,
    state: State<'_, AppState>,
    country: String,
    force_refresh: bool,
) -> Result<DealsReport, String> {
    let lang = resolve_config_path(&app)
        .ok()
        .and_then(|p| AppConfig::load_from_file(&p).ok())
        .and_then(|c| c.lang())
        .unwrap_or(FALLBACK_LANGUAGE);
    let country = kanso_core::deals::normalize_country(&country);
    let cache = Arc::clone(&state.deals_cache);

    if !force_refresh {
        let cached = cache.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(entry) = cached.as_ref() {
            let fresh = now_timestamp_ms().saturating_sub(entry.report.fetched_at) < DEALS_CACHE_TTL_MS;
            if fresh && entry.lang == lang && entry.country == country {
                return Ok(entry.report.clone());
            }
        }
    }

    tauri::async_runtime::spawn_blocking(move || {
        let report = DealsClient::new().fetch_all(lang, &country);
        // A fully failed fetch (offline) is not worth caching: the next visit retries
        if !report.deals.is_empty() {
            *cache.lock().unwrap_or_else(|p| p.into_inner()) = Some(CachedDeals {
                lang,
                country,
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
    let plugin_host = Arc::new(plugins::PluginHost::new(
        &app.package_info().version.to_string(),
        data_dir.join("plugins"),
    ));
    app.manage(AppState {
        db: Arc::clone(&db),
        startup_warnings: Mutex::new(startup_warnings),
        deals_cache: Arc::new(Mutex::new(None)),
        running_games: Arc::clone(&running_games),
        plugins: Arc::clone(&plugin_host),
        matching_store_pages: Arc::default(),
    });
    remove_legacy_sql_mirror(app);
    playtime::spawn_tracker(app.clone(), Arc::clone(&db), running_games);
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
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            get_settings,
            save_settings,
            scan_games,
            sync_library_metadata,
            complete_game_details,
            match_absent_games,
            search_steamgriddb_games,
            apply_steamgriddb_match,
            update_game_details,
            set_game_local_cover,
            set_game_favorite,
            set_game_hidden,
            launch_game,
            install_game,
            fetch_deals,
            open_store_page,
            list_themes,
            import_theme,
            save_theme,
            delete_theme,
            export_theme,
            open_themes_folder,
            set_game_status,
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
            plugins::open_plugin_link
        ])
        .run(tauri::generate_context!())
        .expect("Kanso failed to start");
}
