use kanso_core::{
    is_metadata_sync_due, now_timestamp_ms, scan_all_installed_games, AppConfig, DetectedGame,
    GameManualUpdate, LibraryDb, LibraryStore, MetadataClient, ScanReport, SgdbGameMatch,
};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex, MutexGuard};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_opener::OpenerExt;

const GAME_UPDATED_EVENT: &str = "library:game-updated";

/// Shared handle to the library database; commands lock it only around reads and writes,
/// never across network requests.
type SharedDb = Arc<Mutex<LibraryDb>>;

struct AppState {
    db: SharedDb,
    /// Problems found while opening the library at startup, reported with the first scan.
    startup_warnings: Mutex<Vec<String>>,
}

fn lock_db(db: &SharedDb) -> MutexGuard<'_, LibraryDb> {
    db.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn resolve_app_data_dir(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_local_data_dir()
        .map_err(|e| format!("Impossibile risolvere la cartella AppData locale: {e}"))
}

fn resolve_config_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(resolve_app_data_dir(app)?.join("config.json"))
}

fn resolve_covers_dir(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(resolve_app_data_dir(app)?.join("covers"))
}

/// Loads the one game a command works on as a single-entry store, so the merge rules in
/// [`LibraryStore`] apply to exactly what is in the database right now.
fn load_single_game_store(db: &LibraryDb, game_id: &str) -> Result<LibraryStore, String> {
    let game = db
        .get_game(game_id)?
        .ok_or_else(|| format!("Gioco '{game_id}' non trovato nella libreria."))?;
    Ok(LibraryStore { games: vec![game] })
}

#[tauri::command]
fn get_settings(app: AppHandle) -> Result<AppConfig, String> {
    let config_path = resolve_config_path(&app)?;
    AppConfig::load_from_file(&config_path)
}

#[tauri::command]
fn save_settings(app: AppHandle, config: AppConfig) -> Result<AppConfig, String> {
    let config_path = resolve_config_path(&app)?;
    config.save_to_file(&config_path)
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
        let mut merged = store.merge_with_scan(raw_report);
        store.save_all(&mut db)?;

        merged.warnings.splice(0..0, startup_warnings);
        Ok(merged)
    })
    .await
    .map_err(|e| format!("Errore durante la scansione della libreria: {e}"))?
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

    tauri::async_runtime::spawn_blocking(move || {
        let has_api_key = config.has_api_key();
        let now = now_timestamp_ms();

        let candidates: Vec<DetectedGame> = lock_db(&db)
            .load_games()?
            .into_iter()
            .filter(|game| is_metadata_sync_due(game, has_api_key, now, retry_now))
            .collect();

        let client = MetadataClient::new();
        let api_key = has_api_key.then_some(config.steamgriddb_api_key.as_str());

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

        let final_store = LibraryStore::load(&lock_db(&db))?;
        Ok(final_store.to_report())
    })
    .await
    .map_err(|e| format!("Errore sincronizzazione metadata: {e}"))?
}

#[tauri::command]
async fn search_steamgriddb_games(
    app: AppHandle,
    query: String,
) -> Result<Vec<SgdbGameMatch>, String> {
    let config_path = resolve_config_path(&app)?;
    let config = AppConfig::load_from_file(&config_path)?;
    if !config.has_api_key() {
        return Err(
            "Configura prima la tua SteamGridDB API Key nelle Impostazioni.".to_string(),
        );
    }

    tauri::async_runtime::spawn_blocking(move || {
        let client = MetadataClient::new();
        client.search_steamgriddb(&config.steamgriddb_api_key, &query)
    })
    .await
    .map_err(|e| format!("Errore ricerca SteamGridDB: {e}"))?
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
        return Err(
            "Configura prima la tua SteamGridDB API Key nelle Impostazioni.".to_string(),
        );
    }

    let db = Arc::clone(&state.db);
    let app_handle = app.clone();

    tauri::async_runtime::spawn_blocking(move || {
        let mut game = lock_db(&db)
            .get_game(&game_id)?
            .ok_or_else(|| format!("Gioco '{game_id}' non trovato nella libreria."))?;

        let client = MetadataClient::new();
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
                .ok_or_else(|| format!("Gioco '{game_id}' non più presente nella libreria."))?;
            db.upsert_game(&committed)?;
            committed
        };

        let _ = app_handle.emit(GAME_UPDATED_EVENT, game.clone());
        Ok(game)
    })
    .await
    .map_err(|e| format!("Errore applicazione copertina SteamGridDB: {e}"))?
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
        .ok_or_else(|| format!("Gioco '{game_id}' non trovato nella libreria."))?;

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

/// Creates the system tray icon: left click (or "Apri Kanso") restores the window, "Esci" quits.
fn setup_system_tray(app: &AppHandle) -> tauri::Result<()> {
    let show_item = MenuItem::with_id(app, TRAY_MENU_SHOW_ID, "Apri Kanso", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, TRAY_MENU_QUIT_ID, "Esci", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show_item, &quit_item])?;

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
                    "Impossibile avviare '{}' tramite URI: {uri_err}",
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

    Err(format!(
        "Nessun percorso eseguibile o URI di avvio configurato per '{}'.",
        game.title
    ))
}

fn spawn_executable(exe_path: &str) -> Result<(), String> {
    let path = Path::new(exe_path);
    if !path.is_file() {
        return Err(format!(
            "Eseguibile non trovato sul disco: {}",
            path.display()
        ));
    }

    let mut cmd = Command::new(path);
    if let Some(parent) = path.parent() {
        cmd.current_dir(parent);
    }

    cmd.spawn()
        .map(|_| ())
        .map_err(|e| format!("Impossibile avviare {}: {e}", path.display()))
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
    let (db, startup_warnings) = LibraryDb::open_or_recover(&data_dir);
    app.manage(AppState {
        db: Arc::new(Mutex::new(db)),
        startup_warnings: Mutex::new(startup_warnings),
    });
    remove_legacy_sql_mirror(app);

    // The webview may load local files from the covers folder only. The folder must exist
    // before it is allowed, so the scope also learns its canonical form.
    let covers_dir = data_dir.join("covers");
    fs::create_dir_all(&covers_dir).map_err(|e| {
        format!(
            "Impossibile creare la cartella copertine {}: {e}",
            covers_dir.display()
        )
    })?;
    app.asset_protocol_scope()
        .allow_directory(&covers_dir, false)
        .map_err(|e| format!("Impossibile autorizzare la cartella copertine: {e}"))?;

    // The tray is a convenience: failing to create it must not prevent startup
    if let Err(err) = setup_system_tray(app) {
        eprintln!("Kanso: impossibile creare l'icona nella system tray: {err}");
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
            search_steamgriddb_games,
            apply_steamgriddb_match,
            update_game_details,
            set_game_local_cover,
            set_game_favorite,
            set_game_hidden,
            launch_game
        ])
        .run(tauri::generate_context!())
        .expect("Errore critico durante l'avvio di Kanso");
}
