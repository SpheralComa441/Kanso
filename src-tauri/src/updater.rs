//! In-app update commands for the desktop shell.
//!
//! Checks GitHub Releases via [`kanso_core::updater`], caches the validated release metadata in
//! Rust memory (so the webview never passes an executable path or download URL), streams the
//! verified installer with progress events, and starts the Windows installer in unattended
//! update mode before exiting Kanso cleanly.

use crate::{lock_db, resolve_app_data_dir, signin_window, AppState, TRAY_ICON_ID};
use kanso_core::updater::{
    cleanup_old_installers, launch_installer, AppVersion, CheckedRelease, InstallerKind,
    UpdateClient, UpdateInfo, RELEASES_PAGE_URL,
};
use kanso_core::{now_timestamp_ms, T};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::utils::config::BundleType;
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_opener::OpenerExt;

pub(crate) const UPDATER_PROGRESS_EVENT: &str = "updater:progress";

/// How long a release check answer is reused before asking GitHub again (unless forced).
const CHECK_CACHE_TTL_MS: u64 = 60 * 60 * 1000;
/// Minimum interval between consecutive download progress events sent to the webview.
const PROGRESS_EMIT_INTERVAL: Duration = Duration::from_millis(60);

#[derive(Default)]
struct UpdaterMemory {
    checked_at: Option<u64>,
    release: Option<CheckedRelease>,
}

/// Shared state for in-app update checks and downloads.
#[derive(Default)]
pub(crate) struct UpdaterCache {
    memory: Mutex<UpdaterMemory>,
    installing: Arc<AtomicBool>,
    cancel: Arc<AtomicBool>,
}

/// Answer returned to the interface by `check_for_update`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UpdateCheckResult {
    current_version: String,
    checked_at: u64,
    update: Option<UpdateInfo>,
}

/// Progress event emitted on [`UPDATER_PROGRESS_EVENT`] during download and install.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct UpdateProgressEvent {
    phase: &'static str,
    downloaded: u64,
    total: Option<u64>,
}

struct InstallGuard(Arc<AtomicBool>);

impl Drop for InstallGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

fn failed(error: impl std::fmt::Display) -> String {
    format!("{}: {error}", T::OperationFailed.s())
}

fn resolve_updates_dir(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(resolve_app_data_dir(app)?.join("updates"))
}

fn preferred_installer() -> InstallerKind {
    match tauri::utils::platform::bundle_type() {
        Some(BundleType::Msi) => InstallerKind::Msi,
        _ => InstallerKind::Nsis,
    }
}

/// Removes leftover installers from previous updates. Called quietly on startup.
pub(crate) fn cleanup_on_startup(app: &AppHandle) {
    if let Ok(dir) = resolve_updates_dir(app) {
        cleanup_old_installers(&dir);
    }
}

/// Checks GitHub Releases for a newer stable version of Kanso.
#[tauri::command]
pub(crate) async fn check_for_update(
    state: State<'_, AppState>,
    force: Option<bool>,
) -> Result<UpdateCheckResult, String> {
    let force = force.unwrap_or(false);
    let current = AppVersion::current();

    if !force {
        let guard = state
            .updater_cache
            .memory
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        if let Some(checked_at) = guard.checked_at {
            if now_timestamp_ms().saturating_sub(checked_at) < CHECK_CACHE_TTL_MS {
                return Ok(UpdateCheckResult {
                    current_version: current.to_string(),
                    checked_at,
                    update: guard.release.as_ref().map(|r| r.info.clone()),
                });
            }
        }
    }

    let preferred = preferred_installer();
    let cache = Arc::clone(&state.updater_cache);

    tauri::async_runtime::spawn_blocking(move || {
        let release = UpdateClient::new().check(current, preferred)?;
        let checked_at = now_timestamp_ms();
        let update = release.as_ref().map(|r| r.info.clone());
        let mut guard = cache.memory.lock().unwrap_or_else(|p| p.into_inner());
        guard.checked_at = Some(checked_at);
        guard.release = release;
        Ok(UpdateCheckResult {
            current_version: current.to_string(),
            checked_at,
            update,
        })
    })
    .await
    .map_err(failed)?
}

/// Downloads, SHA-256 verifies, and starts the installer for the checked release matching
/// `version`. On Windows, once the installer starts cleanly, Kanso exits immediately so the
/// installer can replace the binary and relaunch Kanso into the new version.
#[tauri::command]
pub(crate) async fn install_app_update(
    app: AppHandle,
    state: State<'_, AppState>,
    version: String,
) -> Result<(), String> {
    let cache = Arc::clone(&state.updater_cache);
    if cache
        .installing
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Err(T::UpdateInProgress.s().to_string());
    }
    cache.cancel.store(false, Ordering::SeqCst);

    let release = {
        let guard = cache.memory.lock().unwrap_or_else(|p| p.into_inner());
        guard.release.clone()
    };
    let Some(release) = release.filter(|r| r.info.version == version.trim()) else {
        cache.installing.store(false, Ordering::SeqCst);
        return Err(T::UpdateVersionMismatch.s().to_string());
    };

    let updates_dir = match resolve_updates_dir(&app) {
        Ok(dir) => dir,
        Err(err) => {
            cache.installing.store(false, Ordering::SeqCst);
            return Err(err);
        }
    };

    let db = Arc::clone(&state.db);
    let app_handle = app.clone();

    tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
        let _guard = InstallGuard(Arc::clone(&cache.installing));
        let cancel = Arc::clone(&cache.cancel);

        let mut last_emit: Option<Instant> = None;
        let installer_path =
            UpdateClient::new().download_and_verify(&release, &updates_dir, &cancel, |progress| {
                let done = progress.total.is_some_and(|total| progress.downloaded >= total);
                let due = last_emit.is_none_or(|t| t.elapsed() >= PROGRESS_EMIT_INTERVAL);
                if progress.downloaded == 0 || done || due {
                    last_emit = Some(Instant::now());
                    let _ = app_handle.emit(
                        UPDATER_PROGRESS_EVENT,
                        UpdateProgressEvent {
                            phase: if done { "verifying" } else { "downloading" },
                            downloaded: progress.downloaded,
                            total: progress.total,
                        },
                    );
                }
            })?;

        let final_size = release.info.asset_size;
        let _ = app_handle.emit(
            UPDATER_PROGRESS_EVENT,
            UpdateProgressEvent {
                phase: "installing",
                downloaded: final_size,
                total: (final_size > 0).then_some(final_size),
            },
        );
        // Give the webview a brief moment to paint the "installing" state before launching.
        std::thread::sleep(Duration::from_millis(180));

        launch_installer(release.info.installer_kind, &installer_path)?;

        // Installer launched: close auxiliary windows, drop the tray icon, ensure no SQLite
        // write is in flight, and exit immediately so the installer can replace kanso.exe.
        signin_window::close_all(&app_handle);
        let _ = app_handle.remove_tray_by_id(TRAY_ICON_ID);
        let _db_lock = lock_db(&db);
        std::process::exit(0)
    })
    .await
    .map_err(failed)?
}

/// Signals an in-progress update download to stop cleanly.
#[tauri::command]
pub(crate) fn cancel_app_update(state: State<'_, AppState>) {
    state.updater_cache.cancel.store(true, Ordering::SeqCst);
}

/// Opens the verified GitHub Releases page for the latest release (or the repository's Releases
/// index if none is cached) in the user's default browser.
#[tauri::command]
pub(crate) fn open_release_page(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let url = state
        .updater_cache
        .memory
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .release
        .as_ref()
        .map(|r| r.info.release_url.clone())
        .unwrap_or_else(|| RELEASES_PAGE_URL.to_string());
    app.opener()
        .open_url(&url, None::<&str>)
        .map_err(failed)
}
