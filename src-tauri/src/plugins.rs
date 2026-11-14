//! Plugins as the desktop app runs them: listing, installing, running on demand and filling
//! in the game-card labels in the background.

use crate::{game_not_found, load_config, lock_db, resolve_config_path, AppState, SharedDb};
use kanso_core::plugins::output::{is_safe_link, PluginView};
use kanso_core::plugins::registry::{self, load_plugins, Plugin};
use kanso_core::plugins::{
    describe, installed_state, is_enabled, shows_badge, BadgeEntry, GameDataAnswer, PluginEngine,
    PluginInfo,
};
use kanso_core::{current_language, now_timestamp_ms, AppConfig, T};
use serde::Serialize;
use std::cmp::Reverse;
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_opener::OpenerExt;

/// Sent whenever a plugin produced a new label for a game card.
const BADGES_UPDATED_EVENT: &str = "plugins:badges-updated";
/// Pause between two background requests of the same plugin: a library is filled in over
/// minutes, never as a burst against someone's server.
const FILL_PAUSE: Duration = Duration::from_millis(2500);
/// How often the background fill looks again even if nothing changed (cached answers expire).
const FILL_RECHECK: Duration = Duration::from_secs(15 * 60);
/// Failures in a row after which a plugin is left alone until the next pass.
const FILL_MAX_FAILURES: u32 = 3;

/// The plugin engine plus what the app needs around it.
pub(crate) struct PluginHost {
    engine: PluginEngine,
    /// The user's plugins folder.
    dir: PathBuf,
    /// One lock per plugin: its runs never overlap, so its storage is never written twice at once.
    locks: Mutex<HashMap<String, Arc<Mutex<()>>>>,
    /// Bumped whenever the library or the settings changed; wakes the background fill.
    generation: Mutex<u64>,
    wake: Condvar,
}

impl PluginHost {
    pub(crate) fn new(app_version: &str, dir: PathBuf) -> Self {
        Self {
            engine: PluginEngine::new(app_version),
            dir,
            locks: Mutex::new(HashMap::new()),
            generation: Mutex::new(0),
            wake: Condvar::new(),
        }
    }

    pub(crate) fn wake_background_fill(&self) {
        *self.generation.lock().unwrap_or_else(|p| p.into_inner()) += 1;
        self.wake.notify_all();
    }

    fn generation(&self) -> u64 {
        *self.generation.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// Blocks until the generation differs from `seen` or `timeout` passes; returns the
    /// current generation.
    fn wait_for_change(&self, seen: u64, timeout: Duration) -> u64 {
        let guard = self.generation.lock().unwrap_or_else(|p| p.into_inner());
        let (guard, _) = self
            .wake
            .wait_timeout_while(guard, timeout, |generation| *generation == seen)
            .unwrap_or_else(|p| p.into_inner());
        *guard
    }

    fn lock_for(&self, plugin_id: &str) -> Arc<Mutex<()>> {
        let mut locks = self.locks.lock().unwrap_or_else(|p| p.into_inner());
        Arc::clone(locks.entry(plugin_id.to_string()).or_default())
    }

    fn find(&self, plugin_id: &str) -> Result<Plugin, String> {
        registry::find_plugin(&self.dir, plugin_id)
            .ok_or_else(|| format!("{}: {plugin_id}", T::PluginNotFound.s()))
    }

    /// The plugin, provided the user has it switched on.
    fn find_enabled(&self, plugin_id: &str, config: &AppConfig) -> Result<Plugin, String> {
        let plugin = self.find(plugin_id)?;
        if !is_enabled(&plugin, config) {
            return Err(format!("{}: {plugin_id}", T::PluginNotFound.s()));
        }
        Ok(plugin)
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PluginList {
    plugins: Vec<PluginInfo>,
    /// One entry per folder in the plugins folder that is not a valid plugin.
    warnings: Vec<String>,
}

#[tauri::command]
pub(crate) fn list_plugins(app: AppHandle, state: State<'_, AppState>) -> PluginList {
    let config = load_config(&app);
    let lang = current_language();
    let (plugins, warnings) = load_plugins(&state.plugins.dir);
    PluginList {
        plugins: plugins
            .iter()
            .map(|plugin| describe(plugin, &config, lang))
            .collect(),
        warnings,
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PluginPreview {
    plugin: PluginInfo,
    /// Version this plugin would replace, if one with the same id is already available.
    replaces_version: Option<String>,
}

/// Reads a plugin from disk without installing it, so the user can see what it is and which
/// hosts it will contact before agreeing.
#[tauri::command]
pub(crate) fn inspect_plugin(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> Result<PluginPreview, String> {
    let plugin = registry::read_plugin_folder(Path::new(&path))
        .map_err(|error| format!("{}: {error}", T::PluginInvalid.s()))?;
    let replaces_version =
        registry::find_plugin(&state.plugins.dir, &plugin.manifest.id).map(|p| p.manifest.version);
    Ok(PluginPreview {
        plugin: describe(&plugin, &load_config(&app), current_language()),
        replaces_version,
    })
}

/// Installs the plugin at `path` and switches it on. Returns the settings as saved.
#[tauri::command]
pub(crate) fn install_plugin(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> Result<AppConfig, String> {
    let host = &state.plugins;
    let plugin = registry::install_plugin(&host.dir, Path::new(&path))?;
    let id = plugin.manifest.id;

    let config_path = resolve_config_path(&app)?;
    let mut config = AppConfig::load_from_file(&config_path)?;
    let new_state = installed_state(config.plugins.get(&id));
    config.plugins.insert(id.clone(), new_state);
    let saved = config.save_to_file(&config_path)?;

    // Answers of the version it replaces are not this version's
    let _ = lock_db(&state.db).clear_plugin_data(&id, false);
    host.wake_background_fill();
    Ok(saved)
}

/// Removes a plugin the user installed, with everything it cached and stored.
#[tauri::command]
pub(crate) fn remove_plugin(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<AppConfig, String> {
    registry::remove_plugin(&state.plugins.dir, &id)?;
    let _ = lock_db(&state.db).clear_plugin_data(&id, true);

    let config_path = resolve_config_path(&app)?;
    let mut config = AppConfig::load_from_file(&config_path)?;
    config.plugins.remove(&id);
    let saved = config.save_to_file(&config_path)?;
    let _ = app.emit(BADGES_UPDATED_EVENT, ());
    Ok(saved)
}

/// Opens the plugins folder in the file manager: a plugin's folder can simply be dropped in.
#[tauri::command]
pub(crate) fn open_plugins_folder(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let dir = &state.plugins.dir;
    fs::create_dir_all(dir)
        .map_err(|e| format!("{} {}: {e}", T::CannotCreateFolder.s(), dir.display()))?;
    app.opener()
        .open_path(dir.to_string_lossy(), None::<&str>)
        .map_err(|e| format!("{}: {e}", T::OperationFailed.s()))
}

/// What a plugin shows in a game's sheet: cached while fresh, asked again with `force`.
#[tauri::command]
pub(crate) async fn plugin_game_data(
    app: AppHandle,
    state: State<'_, AppState>,
    plugin_id: String,
    game_id: String,
    force: bool,
) -> Result<GameDataAnswer, String> {
    let config = load_config(&app);
    let host = Arc::clone(&state.plugins);
    let db = Arc::clone(&state.db);

    tauri::async_runtime::spawn_blocking(move || {
        let plugin = host.find_enabled(&plugin_id, &config)?;
        if !plugin.manifest.game_details {
            return Err(format!("{}: {plugin_id}", T::PluginNotFound.s()));
        }
        let game = lock_db(&db)
            .get_game(&game_id)?
            .ok_or_else(|| game_not_found(&game_id))?;

        let lock = host.lock_for(&plugin_id);
        let _running = lock.lock().unwrap_or_else(|p| p.into_inner());
        let answer = host.engine.game_data(
            &db,
            &plugin,
            &config,
            current_language(),
            &game,
            force,
            now_timestamp_ms(),
        );
        if answer.data.as_ref().is_some_and(|data| data.badge.is_some()) {
            let _ = app.emit(BADGES_UPDATED_EVENT, ());
        }
        Ok(answer)
    })
    .await
    .map_err(|e| format!("{}: {e}", T::OperationFailed.s()))?
}

/// The labels plugins have for the game cards, by game id.
#[tauri::command]
pub(crate) fn plugin_badges(
    app: AppHandle,
    state: State<'_, AppState>,
) -> BTreeMap<String, Vec<BadgeEntry>> {
    let config = load_config(&app);
    let (plugins, _) = load_plugins(&state.plugins.dir);
    state
        .plugins
        .engine
        .badges(&lock_db(&state.db), &plugins, &config, current_language())
}

/// Runs a command of the Plugins menu and returns what its dialog shows.
#[tauri::command]
pub(crate) async fn run_plugin_command(
    app: AppHandle,
    state: State<'_, AppState>,
    plugin_id: String,
    command_id: String,
) -> Result<PluginView, String> {
    let config = load_config(&app);
    let host = Arc::clone(&state.plugins);
    let db = Arc::clone(&state.db);

    tauri::async_runtime::spawn_blocking(move || {
        let plugin = host.find_enabled(&plugin_id, &config)?;
        let games = lock_db(&db).load_games()?;
        let lock = host.lock_for(&plugin_id);
        let _running = lock.lock().unwrap_or_else(|p| p.into_inner());
        host.engine.run_command(
            &db,
            &plugin,
            &config,
            current_language(),
            &command_id,
            &games,
            now_timestamp_ms(),
        )
    })
    .await
    .map_err(|e| format!("{}: {e}", T::OperationFailed.s()))?
}

/// Opens a link shown by a plugin in the default browser. Only plain `https` addresses.
#[tauri::command]
pub(crate) fn open_plugin_link(app: AppHandle, url: String) -> Result<(), String> {
    if !is_safe_link(&url) {
        return Err(format!("{}: {url}", T::UrlNotAllowed.s()));
    }
    app.opener()
        .open_url(&url, None::<&str>)
        .map_err(|e| format!("{}: {e}", T::OperationFailed.s()))
}

/// Starts the thread that, whenever the library or the settings change, quietly asks the
/// plugins that label the game cards about the games they have no fresh answer for.
pub(crate) fn spawn_background_fill(app: AppHandle, db: SharedDb, host: Arc<PluginHost>) {
    let spawned = std::thread::Builder::new()
        .name("kanso-plugins".to_string())
        .spawn(move || {
            let mut seen = host.generation();
            loop {
                seen = host.wait_for_change(seen, FILL_RECHECK);
                fill_once(&app, &db, &host, seen);
            }
        });
    if let Err(err) = spawned {
        eprintln!("Kanso: plugins will not update in the background: {err}");
    }
}

fn fill_once(app: &AppHandle, db: &SharedDb, host: &PluginHost, generation: u64) {
    let config = load_config(app);
    let lang = current_language();
    let (plugins, _) = load_plugins(&host.dir);
    let workers: Vec<&Plugin> = plugins
        .iter()
        .filter(|plugin| {
            plugin.manifest.background && is_enabled(plugin, &config) && shows_badge(plugin, &config)
        })
        .collect();
    if workers.is_empty() {
        return;
    }

    let mut games = lock_db(db).load_games().unwrap_or_default();
    games.retain(|game| !game.is_hidden);
    // What was played last gets its labels first
    games.sort_by_key(|game| Reverse(game.last_played_at));

    for plugin in workers {
        let lock = host.lock_for(&plugin.manifest.id);
        let mut failures = 0;
        for game in &games {
            // The library or the settings changed: start over with the new state
            if host.generation() != generation {
                return;
            }
            let now = now_timestamp_ms();
            if !host
                .engine
                .needs_game_data(&lock_db(db), plugin, &config, lang, &game.id, now)
            {
                continue;
            }
            let answer = {
                let _running = lock.lock().unwrap_or_else(|p| p.into_inner());
                host.engine.game_data(db, plugin, &config, lang, game, false, now)
            };
            if answer.error.is_some() {
                failures += 1;
                if failures >= FILL_MAX_FAILURES {
                    break;
                }
            } else {
                failures = 0;
                if answer.data.is_some_and(|data| data.badge.is_some()) {
                    let _ = app.emit(BADGES_UPDATED_EVENT, ());
                }
            }
            std::thread::sleep(FILL_PAUSE);
        }
    }
}
