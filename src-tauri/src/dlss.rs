//! DLSS in the user's games: reading the versions, and, on the user's request, bringing a
//! game's libraries up to date.
//!
//! Reading is local and always available. A newer library can come from another of the
//! user's games or from a file the user chose, with no network at all; or from NVIDIA's own
//! public repository, only when the user turned that on in Settings. Whatever the source, a
//! library goes into a game only if Windows says NVIDIA signed it, and the game's original is
//! kept beside it so that it can be put back. Which folders are read and written is decided
//! from the library, never taken from the webview.

use crate::{game_not_found, load_config, lock_db, resolve_app_data_dir, AppState};
use kanso_core::authenticode::signature_of;
use kanso_core::dlss::{
    find_dlss_files, newest_of, replace_part, restore, summarize, DlssError, DlssFiles, DlssLibrary, DlssPart, NvidiaDlss,
    NVIDIA_DLSS_PAGE,
};
use kanso_core::pe_version::{file_version, FileVersion};
use kanso_core::{now_timestamp_ms, DetectedGame, GamePlatform, T};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;

/// How long NVIDIA's answer about its newest version is good for.
const LATEST_TTL_MS: u64 = 12 * 60 * 60 * 1000;

/// What is remembered while Kanso runs: NVIDIA's newest version and when it was asked, and
/// the games found to carry DLSS.
#[derive(Default)]
pub(crate) struct DlssMemory {
    latest: Option<(u64, FileVersion)>,
    games: Option<Vec<GameDlss>>,
}
pub(crate) type DlssCache = Arc<Mutex<DlssMemory>>;

/// A game that carries DLSS, with what it carries.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GameDlss {
    game_id: String,
    title: String,
    libraries: Vec<DlssLibrary>,
}

/// What the interface needs to know about DLSS across the library.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DlssStatus {
    /// Whether the user lets Kanso ask NVIDIA's repository.
    online: bool,
    /// NVIDIA's newest version, when known.
    latest: Option<String>,
    checked_at: Option<u64>,
    /// The parts of the newest version already downloaded on this PC.
    downloaded: Vec<DlssPart>,
    games: Vec<GameDlss>,
    /// Why the newest version could not be asked, when it could not.
    error: Option<String>,
}

/// What changing a game's libraries did.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DlssChange {
    /// How many files were replaced, or put back.
    changed: u32,
    libraries: Vec<DlssLibrary>,
}

/// Where a newer library comes from.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub(crate) enum DlssSource {
    /// NVIDIA's public repository (only with the setting on).
    Nvidia,
    /// Another game of the library, by id.
    #[serde(rename_all = "camelCase")]
    Game { game_id: String },
    /// A file the user chose.
    File { path: String },
}

fn failed(error: impl std::fmt::Display) -> String {
    format!("{}: {error}", T::OperationFailed.s())
}

/// Whether a game has files of its own on this PC to look DLSS up in.
fn has_own_folder(game: &DetectedGame) -> bool {
    game.is_installed && game.platform != GamePlatform::Emulator && !game.install_dir.trim().is_empty()
}

fn files_of(game: &DetectedGame) -> DlssFiles {
    if has_own_folder(game) {
        find_dlss_files(Path::new(&game.install_dir))
    } else {
        DlssFiles::default()
    }
}

/// Where the libraries downloaded from NVIDIA are kept: one folder per version.
fn downloads_dir(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(resolve_app_data_dir(app)?.join("dlss"))
}

fn version_dir(downloads: &Path, version: FileVersion) -> PathBuf {
    let [major, minor, build, _] = version.0;
    downloads.join(format!("v{major}.{minor}.{build}"))
}

/// The parts of `version` already on this PC, whole.
fn downloaded_parts(downloads: &Path, version: FileVersion) -> Vec<DlssPart> {
    let dir = version_dir(downloads, version);
    DlssPart::ALL
        .into_iter()
        .filter(|part| file_version(&dir.join(part.file_name())) == Some(version))
        .collect()
}

/// A part of `version` from NVIDIA, downloaded if it is not on this PC yet. The file is kept
/// only if it declares that version and Windows says NVIDIA signed it.
fn nvidia_library(downloads: &Path, version: FileVersion, part: DlssPart) -> Result<PathBuf, String> {
    let path = version_dir(downloads, version).join(part.file_name());
    if file_version(&path) != Some(version) {
        NvidiaDlss::new()
            .download(version, part, &path)
            .map_err(|error| format!("{}: {error}", T::DlssDownloadFailed.s()))?;
    }
    if !signature_of(&path).is_nvidias() {
        let _ = std::fs::remove_file(&path);
        return Err(T::DlssNotNvidia.s().to_string());
    }
    Ok(path)
}

/// Older versions downloaded before are of no use once a newer one is there.
fn forget_other_versions(downloads: &Path, keep: FileVersion) {
    let keep = version_dir(downloads, keep);
    for entry in std::fs::read_dir(downloads).into_iter().flatten().flatten() {
        if entry.path() != keep && entry.file_type().is_ok_and(|kind| kind.is_dir()) {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

/// NVIDIA's newest version: remembered for half a day, asked again after that or on request.
fn latest_version(cache: &DlssCache, refresh: bool) -> Result<(u64, FileVersion), String> {
    let remembered = cache.lock().unwrap_or_else(|p| p.into_inner()).latest;
    if let Some((asked_at, version)) = remembered {
        if !refresh && now_timestamp_ms().saturating_sub(asked_at) < LATEST_TTL_MS {
            return Ok((asked_at, version));
        }
    }
    let version = NvidiaDlss::new().latest()?;
    let answer = (now_timestamp_ms(), version);
    cache.lock().unwrap_or_else(|p| p.into_inner()).latest = Some(answer);
    Ok(answer)
}

/// The versions of the DLSS libraries a game carries (Super Resolution, Frame Generation, Ray
/// Reconstruction), read from the files in its folder. Only reads; contacts nobody.
#[tauri::command]
pub(crate) async fn game_dlss(state: State<'_, AppState>, game_id: String) -> Result<Vec<DlssLibrary>, String> {
    let game = lock_db(&state.db)
        .get_game(&game_id)?
        .ok_or_else(|| game_not_found(&game_id))?;
    tauri::async_runtime::spawn_blocking(move || summarize(&files_of(&game)))
        .await
        .map_err(failed)
}

/// The games of the library that carry DLSS, each with what it carries.
fn survey(games: &[DetectedGame]) -> Vec<GameDlss> {
    games
        .iter()
        .filter(|game| has_own_folder(game))
        .filter_map(|game| {
            let libraries = summarize(&files_of(game));
            (!libraries.is_empty()).then(|| GameDlss { game_id: game.id.clone(), title: game.title.clone(), libraries })
        })
        .collect()
}

/// After a game's libraries changed, what is remembered about it is brought up to date.
fn remember_game(cache: &DlssCache, game: &DetectedGame, libraries: &[DlssLibrary]) {
    if let Some(games) = cache.lock().unwrap_or_else(|p| p.into_inner()).games.as_mut() {
        games.retain(|entry| entry.game_id != game.id);
        if !libraries.is_empty() {
            games.push(GameDlss { game_id: game.id.clone(), title: game.title.clone(), libraries: libraries.to_vec() });
            games.sort_by_key(|game| game.title.to_lowercase());
        }
    }
}

fn status(cache: &DlssCache, downloads: &Path, online: bool, refresh: bool, games: Vec<GameDlss>) -> DlssStatus {
    let (latest, error) = if online {
        match latest_version(cache, refresh) {
            Ok(answer) => (Some(answer), None),
            // What was known before is still shown; the page says the check did not go through
            Err(error) => (cache.lock().unwrap_or_else(|p| p.into_inner()).latest, Some(error)),
        }
    } else {
        (None, None)
    };
    DlssStatus {
        online,
        latest: latest.map(|(_, version)| version.short()),
        checked_at: latest.map(|(asked_at, _)| asked_at),
        downloaded: latest.map(|(_, version)| downloaded_parts(downloads, version)).unwrap_or_default(),
        games,
        error,
    }
}

/// DLSS across the library: the games that carry it and, when the user turned the setting
/// on, NVIDIA's newest version. With the setting off nobody is contacted. The games' folders
/// are looked through once per run of Kanso, or again with `refresh`.
#[tauri::command]
pub(crate) async fn dlss_status(app: AppHandle, state: State<'_, AppState>, refresh: bool) -> Result<DlssStatus, String> {
    let online = load_config(&app).dlss_updates;
    let downloads = downloads_dir(&app)?;
    let cache = Arc::clone(&state.dlss_cache);
    let remembered = cache.lock().unwrap_or_else(|p| p.into_inner()).games.clone().filter(|_| !refresh);
    let library = if remembered.is_none() { lock_db(&state.db).load_games()? } else { Vec::new() };

    tauri::async_runtime::spawn_blocking(move || {
        let games = remembered.unwrap_or_else(|| {
            let mut games = survey(&library);
            games.sort_by_key(|game| game.title.to_lowercase());
            games
        });
        cache.lock().unwrap_or_else(|p| p.into_inner()).games = Some(games.clone());
        status(&cache, &downloads, online, refresh, games)
    })
    .await
    .map_err(failed)
}

/// Downloads the libraries of NVIDIA's newest version to this PC, so that updating a game
/// later needs no network. Only with the setting on. Each file is kept only if it declares
/// that version and Windows says NVIDIA signed it; older downloads are removed.
#[tauri::command]
pub(crate) async fn dlss_download(app: AppHandle, state: State<'_, AppState>) -> Result<DlssStatus, String> {
    if !load_config(&app).dlss_updates {
        return Err(T::DlssUpdatesOff.s().to_string());
    }
    let downloads = downloads_dir(&app)?;
    let cache = Arc::clone(&state.dlss_cache);
    tauri::async_runtime::spawn_blocking(move || {
        let (_, version) = latest_version(&cache, false).map_err(|error| format!("{}: {error}", T::DlssDownloadFailed.s()))?;
        for part in DlssPart::ALL {
            nvidia_library(&downloads, version, part)?;
        }
        forget_other_versions(&downloads, version);
        let games = cache.lock().unwrap_or_else(|p| p.into_inner()).games.clone().unwrap_or_default();
        Ok(status(&cache, &downloads, true, false, games))
    })
    .await
    .map_err(failed)?
}

/// Brings a game's DLSS libraries up to date from `source`: every part the game has, where
/// the source has a newer one. Each original is kept beside the new file. A library goes in
/// only if Windows says NVIDIA signed it.
#[tauri::command]
pub(crate) async fn dlss_update(
    app: AppHandle,
    state: State<'_, AppState>,
    game_id: String,
    source: DlssSource,
) -> Result<DlssChange, String> {
    let (game, other) = {
        let db = lock_db(&state.db);
        let game = db.get_game(&game_id)?.ok_or_else(|| game_not_found(&game_id))?;
        let other = match &source {
            DlssSource::Game { game_id: other_id } => Some(db.get_game(other_id)?.ok_or_else(|| game_not_found(other_id))?),
            _ => None,
        };
        (game, other)
    };
    if matches!(source, DlssSource::Nvidia) && !load_config(&app).dlss_updates {
        return Err(T::DlssUpdatesOff.s().to_string());
    }
    let downloads = downloads_dir(&app)?;
    let cache = Arc::clone(&state.dlss_cache);

    tauri::async_runtime::spawn_blocking(move || {
        let files = files_of(&game);
        let other_files = other.as_ref().map(files_of);
        let nvidia = match source {
            DlssSource::Nvidia => Some(latest_version(&cache, false).map_err(|error| format!("{}: {error}", T::DlssDownloadFailed.s()))?.1),
            _ => None,
        };
        let mut changed = 0;
        for part in DlssPart::ALL {
            // What the game has of this part that could be brought up to date
            let Some(oldest) = files.libraries.iter().filter(|file| file.part == part && file.version.0[0] != 1).map(|file| file.version).min() else {
                continue;
            };
            let library: PathBuf = match (&source, nvidia, &other_files) {
                (DlssSource::Nvidia, Some(version), _) => {
                    // Nothing is downloaded for a part the game already has as new
                    if oldest >= version {
                        continue;
                    }
                    nvidia_library(&downloads, version, part)?
                }
                (DlssSource::Game { .. }, _, Some(other_files)) => match newest_of(other_files, part) {
                    Some(newest) if newest.version > oldest => newest.path.clone(),
                    _ => continue,
                },
                (DlssSource::File { path }, _, _) => {
                    let path = PathBuf::from(path);
                    // A file stands in for the part its name says, and for no other
                    match path.file_name().and_then(|name| name.to_str()).and_then(DlssPart::of_file) {
                        Some(named) if named == part => path,
                        Some(_) => continue,
                        None => return Err(T::DlssNotNvidia.s().to_string()),
                    }
                }
                _ => continue,
            };
            if !signature_of(&library).is_nvidias() {
                return Err(T::DlssNotNvidia.s().to_string());
            }
            match replace_part(&files, part, &library) {
                Ok(replaced) => changed += replaced,
                Err(DlssError::NothingOlder) => {}
                Err(DlssError::NotALibrary) => return Err(T::DlssNotNvidia.s().to_string()),
                Err(DlssError::Busy(error)) => return Err(format!("{}: {error}", T::DlssBusy.s())),
            }
        }
        if changed == 0 {
            return Err(T::DlssNothingOlder.s().to_string());
        }
        let libraries = summarize(&files_of(&game));
        remember_game(&cache, &game, &libraries);
        Ok(DlssChange { changed, libraries })
    })
    .await
    .map_err(failed)?
}

/// Puts back the libraries a game shipped with, wherever Kanso replaced one.
#[tauri::command]
pub(crate) async fn dlss_restore(state: State<'_, AppState>, game_id: String) -> Result<DlssChange, String> {
    let game = lock_db(&state.db)
        .get_game(&game_id)?
        .ok_or_else(|| game_not_found(&game_id))?;
    let cache = Arc::clone(&state.dlss_cache);
    tauri::async_runtime::spawn_blocking(move || {
        let changed = match restore(&files_of(&game), None) {
            Ok(changed) => changed,
            Err(DlssError::Busy(error)) => return Err(format!("{}: {error}", T::DlssBusy.s())),
            Err(_) => 0,
        };
        let libraries = summarize(&files_of(&game));
        remember_game(&cache, &game, &libraries);
        Ok(DlssChange { changed, libraries })
    })
    .await
    .map_err(failed)?
}

/// Opens NVIDIA's page of the DLSS libraries, credited where they are downloaded from.
#[tauri::command]
pub(crate) fn open_dlss_source(app: AppHandle) -> Result<(), String> {
    app.opener().open_url(NVIDIA_DLSS_PAGE, None::<&str>).map_err(failed)
}
