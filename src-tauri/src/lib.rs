use kanso_core::{
    scan_all_installed_games, AppConfig, DetectedGame, GameManualUpdate, LibraryStore,
    MetadataClient, ScanReport, SgdbGameMatch,
};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_opener::OpenerExt;
use tauri_plugin_sql::{Migration, MigrationKind};

#[derive(Default)]
struct AppState {
    store_lock: Arc<Mutex<()>>,
}

fn resolve_app_data_dir(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_local_data_dir()
        .map_err(|e| format!("Impossibile risolvere la cartella AppData locale: {e}"))
}

fn resolve_config_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(resolve_app_data_dir(app)?.join("config.json"))
}

fn resolve_library_store_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(resolve_app_data_dir(app)?.join("library.json"))
}

fn resolve_covers_dir(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(resolve_app_data_dir(app)?.join("covers"))
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
    let store_path = resolve_library_store_path(&app)?;
    let config = AppConfig::load_from_file(&config_path)?;
    let lock = Arc::clone(&state.store_lock);

    tauri::async_runtime::spawn_blocking(move || {
        let raw_report = scan_all_installed_games(&config);
        let _guard = lock.lock().unwrap_or_else(|e| e.into_inner());
        let mut store = LibraryStore::load_from_file(&store_path);
        let merged = store.merge_with_scan(raw_report);
        let _ = store.save_to_file(&store_path);
        merged
    })
    .await
    .map_err(|e| format!("Errore durante la scansione della libreria: {e}"))
}

#[tauri::command]
async fn sync_library_metadata(
    app: AppHandle,
    state: State<'_, AppState>,
    force_refresh: bool,
) -> Result<ScanReport, String> {
    let config_path = resolve_config_path(&app)?;
    let store_path = resolve_library_store_path(&app)?;
    let covers_dir = resolve_covers_dir(&app)?;
    let config = AppConfig::load_from_file(&config_path)?;
    let lock = Arc::clone(&state.store_lock);
    let app_handle = app.clone();

    tauri::async_runtime::spawn_blocking(move || {
        let candidates: Vec<DetectedGame> = {
            let _guard = lock.lock().unwrap_or_else(|e| e.into_inner());
            let store = LibraryStore::load_from_file(&store_path);
            store
                .games
                .into_iter()
                .filter(|g| {
                    if force_refresh && !g.custom_override {
                        return true;
                    }
                    let missing_cover = config.has_api_key()
                        && g.cover_path
                            .as_deref()
                            .is_none_or(|p| !Path::new(p).is_file());
                    let missing_desc = g.short_description.is_none() && g.genres.is_empty();
                    // `title_provisional`: the Steam manifest only had an `appid_<id>`
                    // placeholder, so the official name still has to be fetched
                    !g.metadata_synced || missing_cover || missing_desc || g.title_provisional
                })
                .collect()
        };

        let client = MetadataClient::new();
        let api_key_opt = config
            .has_api_key()
            .then_some(config.steamgriddb_api_key.as_str());

        for mut game in candidates {
            let _ = client.enrich_game_automatically(&mut game, api_key_opt, &covers_dir);

            let committed = {
                let _guard = lock.lock().unwrap_or_else(|e| e.into_inner());
                let mut store = LibraryStore::load_from_file(&store_path);
                // Keeps favorites toggled, and skips manual overrides saved, while HTTP was in flight
                let committed = store.commit_enriched_game(game);
                if committed.is_some() {
                    let _ = store.save_to_file(&store_path);
                }
                committed
            };

            if let Some(updated) = committed {
                let _ = app_handle.emit("library:game-updated", updated);
            }
        }

        let _guard = lock.lock().unwrap_or_else(|e| e.into_inner());
        let final_store = LibraryStore::load_from_file(&store_path);
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
    let store_path = resolve_library_store_path(&app)?;
    let covers_dir = resolve_covers_dir(&app)?;
    let config = AppConfig::load_from_file(&config_path)?;

    if !config.has_api_key() {
        return Err(
            "Configura prima la tua SteamGridDB API Key nelle Impostazioni.".to_string(),
        );
    }

    let lock = Arc::clone(&state.store_lock);
    let app_handle = app.clone();

    tauri::async_runtime::spawn_blocking(move || {
        let mut game = {
            let _guard = lock.lock().unwrap_or_else(|e| e.into_inner());
            let store = LibraryStore::load_from_file(&store_path);
            store
                .games
                .into_iter()
                .find(|g| g.id == game_id)
                .ok_or_else(|| format!("Gioco '{game_id}' non trovato nella libreria."))?
        };

        let client = MetadataClient::new();
        client.apply_manual_sgdb_selection(
            &mut game,
            &config.steamgriddb_api_key,
            sgdb_game_id,
            &matched_name,
            &covers_dir,
        )?;

        let game = {
            let _guard = lock.lock().unwrap_or_else(|e| e.into_inner());
            let mut store = LibraryStore::load_from_file(&store_path);
            let committed = store.commit_enriched_game(game).ok_or_else(|| {
                format!("Gioco '{game_id}' non più presente nella libreria.")
            })?;
            store.save_to_file(&store_path)?;
            committed
        };

        let _ = app_handle.emit("library:game-updated", game.clone());
        Ok(game)
    })
    .await
    .map_err(|e| format!("Errore applicazione copertina SteamGridDB: {e}"))?
}

#[tauri::command]
fn update_game_details(
    app: AppHandle,
    state: State<'_, AppState>,
    payload: GameManualUpdate,
) -> Result<DetectedGame, String> {
    let store_path = resolve_library_store_path(&app)?;
    let _guard = state.store_lock.lock().unwrap_or_else(|e| e.into_inner());
    let mut store = LibraryStore::load_from_file(&store_path);
    let updated = store.apply_manual_update(payload)?;
    store.save_to_file(&store_path)?;
    let _ = app.emit("library:game-updated", updated.clone());
    Ok(updated)
}

#[tauri::command]
fn set_game_local_cover(
    app: AppHandle,
    state: State<'_, AppState>,
    game_id: String,
    image_path: String,
) -> Result<DetectedGame, String> {
    let store_path = resolve_library_store_path(&app)?;
    let covers_dir = resolve_covers_dir(&app)?;
    let _guard = state.store_lock.lock().unwrap_or_else(|e| e.into_inner());
    let mut store = LibraryStore::load_from_file(&store_path);
    let updated = store.set_local_cover_file(&game_id, Path::new(&image_path), &covers_dir)?;
    store.save_to_file(&store_path)?;
    let _ = app.emit("library:game-updated", updated.clone());
    Ok(updated)
}

#[tauri::command]
fn set_game_favorite(
    app: AppHandle,
    state: State<'_, AppState>,
    game_id: String,
    is_favorite: bool,
) -> Result<DetectedGame, String> {
    let store_path = resolve_library_store_path(&app)?;
    let _guard = state.store_lock.lock().unwrap_or_else(|e| e.into_inner());
    let mut store = LibraryStore::load_from_file(&store_path);
    let updated = store.set_favorite(&game_id, is_favorite)?;
    store.save_to_file(&store_path)?;
    let _ = app.emit("library:game-updated", updated.clone());
    Ok(updated)
}

/// Launches a game by library id. The launch target is always read from the local store,
/// never taken from the webview.
#[tauri::command]
fn launch_game(
    app: AppHandle,
    state: State<'_, AppState>,
    game_id: String,
) -> Result<(), String> {
    let store_path = resolve_library_store_path(&app)?;
    let game = {
        let _guard = state.store_lock.lock().unwrap_or_else(|e| e.into_inner());
        LibraryStore::load_from_file(&store_path)
            .games
            .into_iter()
            .find(|g| g.id == game_id)
            .ok_or_else(|| format!("Gioco '{game_id}' non trovato nella libreria."))?
    };

    launch_game_inner(&app, &game)?;

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

fn database_migrations() -> Vec<Migration> {
    vec![Migration {
        version: 1,
        description: "create_initial_tables",
        sql: "
            CREATE TABLE IF NOT EXISTS settings (
                key TEXT PRIMARY KEY NOT NULL,
                value TEXT NOT NULL,
                updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            );

            CREATE TABLE IF NOT EXISTS games (
                id TEXT PRIMARY KEY NOT NULL,
                title TEXT NOT NULL,
                clean_title TEXT NOT NULL,
                platform TEXT NOT NULL,
                platform_app_id TEXT,
                install_dir TEXT NOT NULL,
                executable_path TEXT,
                launch_uri TEXT,
                cover_path TEXT,
                short_description TEXT,
                genres TEXT,
                is_favorite INTEGER NOT NULL DEFAULT 0,
                updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            );
        ",
        kind: MigrationKind::Up,
    }]
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // Must be the first plugin: a second launch (e.g. from the Start menu while Kanso sits
        // in the tray) just brings the running window back instead of starting a new process.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_main_window(app);
        }))
        .manage(AppState::default())
        .setup(|app| {
            // The tray is a convenience: failing to create it must not prevent startup
            if let Err(err) = setup_system_tray(app.handle()) {
                eprintln!("Kanso: impossibile creare l'icona nella system tray: {err}");
            }
            Ok(())
        })
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(
            tauri_plugin_sql::Builder::default()
                .add_migrations("sqlite:kanso.db", database_migrations())
                .build(),
        )
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
            launch_game
        ])
        .run(tauri::generate_context!())
        .expect("Errore critico durante l'avvio di Kanso");
}
