//! The emulators of this PC, for the interface: listing them, recognising an executable,
//! starting one, and starting a game with its emulator.

use crate::{load_config, resolve_app_data_dir, AppState};
use kanso_core::emulators::{self, Emulator, EmulatorIdentity, EmulatorInfo, LaunchCommand};
use kanso_core::playtime::path_key;
use kanso_core::{find_emulators, now_timestamp_ms, DetectedGame, T};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, State};

/// The emulators the last scan found. Empty until the first scan of this run.
pub(crate) type SharedEmulators = Arc<Mutex<Vec<Emulator>>>;

/// Emulated games started from Kanso: executable of the emulator → (game, when). The play
/// time tracker credits the game for as long as that emulator keeps running.
pub(crate) type EmulatedSessions = Arc<Mutex<HashMap<String, (String, u64)>>>;

/// Where the emulators found by the scans are remembered: a list of paths in Kanso's own
/// data folder.
pub(crate) fn remembered_file(app: &AppHandle) -> Option<PathBuf> {
    resolve_app_data_dir(app).ok().map(|dir| dir.join("emulators.json"))
}

/// What the last scan found, or, before any scan, what can be told without one.
fn known_emulators(app: &AppHandle, state: &AppState) -> Vec<Emulator> {
    let cached = state.emulators.lock().map(|list| list.clone()).unwrap_or_default();
    if cached.is_empty() {
        find_emulators(&load_config(app), remembered_file(app).as_deref())
    } else {
        cached
    }
}

#[tauri::command]
pub(crate) fn list_emulators(app: AppHandle, state: State<'_, AppState>) -> Vec<EmulatorInfo> {
    known_emulators(&app, &state).iter().map(Emulator::info).collect()
}

/// What Kanso knows the executable as; `null` for a program it does not know.
#[tauri::command]
pub(crate) fn identify_emulator(exe_path: String) -> Option<EmulatorIdentity> {
    emulators::identify(Path::new(exe_path.trim())).map(EmulatorIdentity::of)
}

fn spawn(command: &LaunchCommand) -> Result<(), String> {
    if !command.program.is_file() {
        return Err(format!("{}: {}", T::ExeNotFound.s(), command.program.display()));
    }
    Command::new(&command.program)
        .args(&command.args)
        .current_dir(&command.working_dir)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("{} \"{}\": {e}", T::LaunchFailed.s(), command.program.display()))
}

/// Opens the emulator itself, with no game.
#[tauri::command]
pub(crate) fn launch_emulator(app: AppHandle, state: State<'_, AppState>, emulator_id: String) -> Result<(), String> {
    let emulator = known_emulators(&app, &state)
        .into_iter()
        .find(|emulator| emulator.id == emulator_id)
        .ok_or_else(|| T::EmulatorNotFound.s().to_string())?;
    spawn(&LaunchCommand {
        working_dir: emulator.exe_path.parent().map(Path::to_path_buf).unwrap_or_default(),
        program: emulator.exe_path,
        args: Vec::new(),
    })
}

/// Starts an emulated game: its emulator, with the game file as an argument. Which emulator
/// and which file come from the library, not from the webview.
pub(crate) fn launch_emulated_game(app: &AppHandle, state: &AppState, game: &DetectedGame) -> Result<(), String> {
    let emulator_id = game.platform_app_id.as_deref().unwrap_or_default();
    let emulator = known_emulators(app, state)
        .into_iter()
        .find(|emulator| emulator.id == emulator_id)
        .ok_or_else(|| T::EmulatorNotFound.s().to_string())?;
    let file = game
        .executable_path
        .as_deref()
        .filter(|path| !path.trim().is_empty())
        .ok_or_else(|| format!("{} \"{}\".", T::NoLaunchTarget.s(), game.title))?;
    spawn(&emulators::launch_command(&emulator, file)?)?;

    if let Ok(mut sessions) = state.emulated_sessions.lock() {
        sessions.insert(path_key(&emulator.exe_path.to_string_lossy()), (game.id.clone(), now_timestamp_ms()));
    }
    Ok(())
}
