use kanso_core::{scan_all_installed_games, AppConfig, DetectedGame, ScanReport};
use std::path::{Path, PathBuf};
use std::process::Command;
use tauri::{AppHandle, Manager};
use tauri_plugin_opener::OpenerExt;
use tauri_plugin_sql::{Migration, MigrationKind};

fn resolve_config_path(app: &AppHandle) -> Result<PathBuf, String> {
    let app_data_dir = app
        .path()
        .app_local_data_dir()
        .map_err(|e| format!("Impossibile risolvere la cartella AppData locale: {e}"))?;
    Ok(app_data_dir.join("config.json"))
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
async fn scan_games(app: AppHandle) -> Result<ScanReport, String> {
    let config_path = resolve_config_path(&app)?;
    let config = AppConfig::load_from_file(&config_path)?;

    tauri::async_runtime::spawn_blocking(move || scan_all_installed_games(&config))
        .await
        .map_err(|e| format!("Errore durante la scansione della libreria: {e}"))
}

#[tauri::command]
fn launch_game(app: AppHandle, game: DetectedGame) -> Result<(), String> {
    // Prefer protocol URI when available (Steam `steam://rungameid/{appid}`, Epic launcher URI)
    if let Some(uri) = game.launch_uri.as_deref().filter(|u| !u.trim().is_empty()) {
        match app.opener().open_url(uri, None::<&str>) {
            Ok(()) => return Ok(()),
            Err(uri_err) => {
                // Fallback to direct executable if Epic/custom launcher URI isn't registered
                if let Some(exe) = game
                    .executable_path
                    .as_deref()
                    .filter(|p| !p.trim().is_empty())
                {
                    if Path::new(exe).is_file() {
                        return spawn_executable(exe);
                    }
                }
                return Err(format!("Impossibile avviare '{}' tramite URI: {uri_err}", game.title));
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
            launch_game
        ])
        .run(tauri::generate_context!())
        .expect("Errore critico durante l'avvio di Kanso");
}
