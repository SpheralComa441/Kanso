use kanso_core::{
    scan_all_installed_games, AppConfig, DetectedGame, GameManualUpdate, LibraryStore,
    MetadataClient, ScanReport, SgdbGameMatch,
};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
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
                    !g.metadata_synced || missing_cover || missing_desc
                })
                .collect()
        };

        let client = MetadataClient::new();
        let api_key_opt = config
            .has_api_key()
            .then_some(config.steamgriddb_api_key.as_str());

        for mut game in candidates {
            let _ = client.enrich_game_automatically(&mut game, api_key_opt, &covers_dir);

            let emitted_game = {
                let _guard = lock.lock().unwrap_or_else(|e| e.into_inner());
                let mut store = LibraryStore::load_from_file(&store_path);
                // Do not overwrite if user concurrently saved a manual override while HTTP was in flight
                if let Some(existing) = store.games.iter().find(|g| g.id == game.id) {
                    if existing.custom_override && !game.custom_override {
                        continue;
                    }
                }
                store.upsert_game(game.clone());
                let _ = store.save_to_file(&store_path);
                game
            };

            let _ = app_handle.emit("library:game-updated", emitted_game);
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

        {
            let _guard = lock.lock().unwrap_or_else(|e| e.into_inner());
            let mut store = LibraryStore::load_from_file(&store_path);
            store.upsert_game(game.clone());
            store.save_to_file(&store_path)?;
        }

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
fn launch_game(app: AppHandle, game: DetectedGame) -> Result<(), String> {
    let config_path = resolve_config_path(&app).ok();
    let minimize_after = config_path
        .and_then(|p| AppConfig::load_from_file(&p).ok())
        .is_some_and(|c| c.minimize_to_tray_on_launch);

    let launch_result = launch_game_inner(&app, &game);
    if launch_result.is_ok() && minimize_after {
        if let Some(win) = app.get_webview_window("main") {
            let _ = win.minimize();
        }
    }
    launch_result
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
        .manage(AppState::default())
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
            launch_game
        ])
        .run(tauri::generate_context!())
        .expect("Errore critico durante l'avvio di Kanso");
}
