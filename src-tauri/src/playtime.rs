//! Play time, achievements and what is running right now.

use crate::emulators::EmulatedSessions;
use crate::{game_not_found, load_config, lock_db, AppState, SharedDb, GAME_UPDATED_EVENT};
use kanso_core::achievements::read_achievements_from_config_dir;
use kanso_core::playtime::{
    path_key, ProcessLister, ProcessMatcher, SessionTracker, TrackerEvent, POLL_INTERVAL_SECS, SOURCE_KANSO,
};
use kanso_core::scanner::steam::{detect_steam_root, steam_account_config_dir};
use kanso_core::{now_timestamp_ms, GameAchievements, GamePlatform, PlaySession};
use std::collections::{BTreeSet, HashMap};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Emitter, State};

/// Sent with the ids of the games seen running whenever that list changes.
const RUNNING_GAMES_EVENT: &str = "playtime:running";

/// The achievements Steam lists for a game, read from its client's cache at this moment.
/// `null` for games of other stores, without achievements, or never opened in Steam's library.
#[tauri::command]
pub(crate) fn get_game_achievements(
    state: State<'_, AppState>,
    game_id: String,
) -> Result<Option<GameAchievements>, String> {
    let game = lock_db(&state.db)
        .get_game(&game_id)?
        .ok_or_else(|| game_not_found(&game_id))?;
    if game.platform != GamePlatform::Steam {
        return Ok(None);
    }
    Ok(game.platform_app_id.as_deref().and_then(|appid| {
        let config_dir = steam_account_config_dir(&detect_steam_root()?)?;
        read_achievements_from_config_dir(&config_dir, appid)
    }))
}

/// Play sessions that started at or after `since_ms`, oldest first, for the statistics.
#[tauri::command]
pub(crate) fn get_play_sessions(
    state: State<'_, AppState>,
    since_ms: u64,
) -> Result<Vec<PlaySession>, String> {
    lock_db(&state.db).load_play_sessions(since_ms)
}

#[tauri::command]
pub(crate) fn get_running_games(state: State<'_, AppState>) -> Vec<String> {
    state
        .running_games
        .lock()
        .map(|running| running.clone())
        .unwrap_or_default()
}

/// Starts the thread that measures play time for as long as Kanso runs (the tray counts).
///
/// A store game is recognised by the folder its process runs from. An emulated game is not a
/// process of its own: it is credited while the emulator Kanso started it with keeps running
/// (`emulated`), and no longer once that emulator has closed.
pub(crate) fn spawn_tracker(
    app: AppHandle,
    db: SharedDb,
    running: Arc<Mutex<Vec<String>>>,
    emulated: EmulatedSessions,
) {
    let spawned = std::thread::Builder::new()
        .name("kanso-playtime".to_string())
        .spawn(move || {
            let mut lister = ProcessLister::new();
            let mut tracker = SessionTracker::new();
            // Database rows of the sessions in progress
            let mut rows: HashMap<String, i64> = HashMap::new();
            loop {
                std::thread::sleep(Duration::from_secs(POLL_INTERVAL_SECS));
                let now = now_timestamp_ms();

                let events = if load_config(&app).track_playtime {
                    let games = lock_db(&db).load_games().unwrap_or_default();
                    let matcher = ProcessMatcher::new(&games);
                    let watching_emulators = emulated.lock().map(|e| !e.is_empty()).unwrap_or(false);
                    let mut seen = BTreeSet::new();
                    if !matcher.is_empty() || watching_emulators {
                        let executables = lister.executables();
                        if watching_emulators {
                            seen.extend(emulated_running(&emulated, &executables, now));
                        }
                        seen.extend(matcher.running_games(executables));
                    }
                    tracker.observe(now, &seen)
                } else {
                    tracker.finish_all()
                };

                for event in events {
                    record(&app, &db, &mut rows, event, now);
                }

                let now_running = tracker.running();
                let changed = running
                    .lock()
                    .map(|mut shared| {
                        let changed = *shared != now_running;
                        *shared = now_running.clone();
                        changed
                    })
                    .unwrap_or(false);
                if changed {
                    let _ = app.emit(RUNNING_GAMES_EVENT, now_running);
                }
            }
        });
    if let Err(err) = spawned {
        eprintln!("Kanso: play time will not be measured: {err}");
    }
}

/// How long after starting an emulator its process may still be missing from the list.
const EMULATOR_START_GRACE_MS: u64 = 90_000;

/// The emulated games whose emulator is running. A game whose emulator has closed is
/// forgotten: opening the emulator by hand later is not playing that game.
fn emulated_running(emulated: &EmulatedSessions, executables: &[String], now: u64) -> Vec<String> {
    let running: std::collections::HashSet<String> = executables.iter().map(|exe| path_key(exe)).collect();
    let Ok(mut sessions) = emulated.lock() else {
        return Vec::new();
    };
    sessions.retain(|exe, (_, started)| running.contains(exe) || now.saturating_sub(*started) < EMULATOR_START_GRACE_MS);
    sessions
        .iter()
        .filter(|(exe, _)| running.contains(*exe))
        .map(|(_, (game_id, _))| game_id.clone())
        .collect()
}

/// Writes what a tracker event means to the database and tells the interface.
fn record(
    app: &AppHandle,
    db: &SharedDb,
    rows: &mut HashMap<String, i64>,
    event: TrackerEvent,
    now: u64,
) {
    let (game_id, started_at, seconds, add_minutes, ended) = match event {
        TrackerEvent::Started { .. } => return,
        TrackerEvent::Progress {
            game_id,
            started_at,
            session_seconds,
            add_minutes,
        } => (game_id, started_at, session_seconds, add_minutes, false),
        TrackerEvent::Ended {
            game_id,
            started_at,
            session_seconds,
            add_minutes,
        } => (game_id, started_at, session_seconds, add_minutes, true),
    };

    let db = lock_db(db);
    match rows.get(&game_id) {
        Some(row) => {
            let _ = db.set_play_session_seconds(*row, seconds);
        }
        // A session shorter than half a minute (a launcher, a crash) leaves no trace
        None if add_minutes > 0 => {
            let session = PlaySession {
                game_id: game_id.clone(),
                started_at,
                seconds,
                source: SOURCE_KANSO.to_string(),
            };
            if let Ok(row) = db.insert_play_session(&session) {
                rows.insert(game_id.clone(), row);
            }
        }
        None => {}
    }
    if ended {
        rows.remove(&game_id);
    }
    if add_minutes > 0 {
        if let Ok(Some(updated)) = db.add_playtime(&game_id, add_minutes, now) {
            let _ = app.emit(GAME_UPDATED_EVENT, updated);
        }
    }
}
