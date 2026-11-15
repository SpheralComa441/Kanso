use crate::cleaner::clean_game_title;
use crate::i18n::T;
use crate::models::{DetectedGame, GamePlatform, SourceScanOutput};
use serde::Deserialize;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

/// Raw Epic Games Launcher `.item` manifest JSON schema.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct EpicItemManifest {
    #[serde(default)]
    display_name: String,
    #[serde(default)]
    install_location: String,
    #[serde(default)]
    launch_executable: String,
    #[serde(default)]
    app_name: String,
    #[serde(default)]
    main_game_app_name: String,
    #[serde(default)]
    catalog_namespace: String,
    #[serde(default)]
    catalog_item_id: String,
    #[serde(rename = "bIsIncompleteInstall", default)]
    is_incomplete_install: bool,
}

/// Returns the default Epic Games Launcher manifests folder on Windows:
/// `C:\ProgramData\Epic\EpicGamesLauncher\Data\Manifests`.
pub fn default_epic_manifests_dir() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        let program_data =
            std::env::var("PROGRAMDATA").unwrap_or_else(|_| r"C:\ProgramData".to_string());
        let dir = PathBuf::from(program_data)
            .join("Epic")
            .join("EpicGamesLauncher")
            .join("Data")
            .join("Manifests");
        if dir.is_dir() {
            return Some(dir);
        }
        None
    }

    #[cfg(not(target_os = "windows"))]
    {
        None
    }
}

/// Parses a single Epic Games `.item` JSON manifest into a [`DetectedGame`].
/// Returns `Ok(None)` if the manifest represents an incomplete install or a non-launchable DLC/add-on.
pub fn parse_epic_item_manifest(content: &str) -> Result<Option<DetectedGame>, String> {
    let stripped = content.strip_prefix('\u{feff}').unwrap_or(content);
    let manifest: EpicItemManifest = serde_json::from_str(stripped)
        .map_err(|e| format!("{} (.item): {e}", T::InvalidManifest.s()))?;

    if manifest.is_incomplete_install {
        return Ok(None);
    }

    let display_name = manifest.display_name.trim();
    let install_location = manifest.install_location.trim();
    let launch_executable = manifest.launch_executable.trim();

    // DLCs or non-executable content plugins lack LaunchExecutable
    if display_name.is_empty() || install_location.is_empty() || launch_executable.is_empty() {
        return Ok(None);
    }

    let app_name = manifest.app_name.trim();
    let main_game = manifest.main_game_app_name.trim();
    if !main_game.is_empty() && !app_name.is_empty() && !main_game.eq_ignore_ascii_case(app_name) {
        // Add-on / DLC attached to another base game
        return Ok(None);
    }

    let exe_path = join_install_and_executable(install_location, launch_executable);
    let id_suffix = if !app_name.is_empty() {
        app_name.to_string()
    } else {
        slugify_id(display_name)
    };

    let launch_uri = build_epic_launch_uri(
        manifest.catalog_namespace.trim(),
        manifest.catalog_item_id.trim(),
        app_name,
    );

    Ok(Some(DetectedGame::new_detected(
        format!("epic:{id_suffix}"),
        display_name.to_string(),
        clean_game_title(display_name),
        GamePlatform::Epic,
        (!app_name.is_empty()).then(|| app_name.to_string()),
        install_location.to_string(),
        Some(exe_path),
        launch_uri,
    )))
}

fn join_install_and_executable(install_location: &str, launch_executable: &str) -> String {
    let uses_backslash = install_location.contains('\\') && !install_location.contains('/');
    if uses_backslash {
        let base = install_location.trim_end_matches(['\\', '/']);
        let rel = launch_executable
            .trim_start_matches(['\\', '/'])
            .replace('/', r"\");
        format!(r"{base}\{rel}")
    } else {
        Path::new(install_location)
            .join(launch_executable)
            .to_string_lossy()
            .to_string()
    }
}

pub(crate) fn build_epic_launch_uri(namespace: &str, item_id: &str, app_name: &str) -> Option<String> {
    if app_name.is_empty() {
        return None;
    }
    if !namespace.is_empty() && !item_id.is_empty() {
        Some(format!(
            "com.epicgames.launcher://apps/{namespace}%3A{item_id}%3A{app_name}?action=launch&silent=true"
        ))
    } else {
        Some(format!(
            "com.epicgames.launcher://apps/{app_name}?action=launch&silent=true"
        ))
    }
}

fn slugify_id(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect()
}

/// Scans all `.item` manifests inside an Epic Games `Manifests` directory.
pub fn scan_epic_manifests_dir(manifests_dir: &Path) -> SourceScanOutput {
    let mut output = SourceScanOutput::default();
    if !manifests_dir.is_dir() {
        return output;
    }

    let entries = match fs::read_dir(manifests_dir) {
        Ok(entries) => entries,
        Err(err) => {
            output.warnings.push(format!(
                "{} {}: {err}",
                T::CannotRead.s(),
                manifests_dir.display()
            ));
            return output;
        }
    };

    let mut seen_ids = HashSet::new();

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let is_item = path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("item"));
        if !is_item {
            continue;
        }

        match fs::read_to_string(&path) {
            Ok(content) => match parse_epic_item_manifest(&content) {
                Ok(Some(game)) => {
                    if seen_ids.insert(game.id.clone()) {
                        output.games.push(game);
                    }
                }
                Ok(None) => {}
                Err(err) => output
                    .warnings
                    .push(format!("Manifest Epic ({}): {err}", path.display())),
            },
            Err(err) => output
                .warnings
                .push(format!("{} {}: {err}", T::CannotRead.s(), path.display())),
        }
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn parses_mock_epic_item_manifest() {
        let mock_item = r#"{
            "FormatVersion": 0,
            "bIsIncompleteInstall": false,
            "LaunchCommand": "",
            "LaunchExecutable": "Binaries/Win64/AlanWake2.exe",
            "AppName": "dc9d2e595d0e4650b35d659f90d41059",
            "CatalogNamespace": "da155068a9034375839136b4697593c1",
            "CatalogItemId": "235b28b9c6e2469da792fbabbc2fd25e",
            "DisplayName": "Alan Wake 2 Deluxe Edition",
            "InstallLocation": "D:\\Epic Games\\AlanWake2",
            "MainGameAppName": "dc9d2e595d0e4650b35d659f90d41059"
        }"#;

        let game = parse_epic_item_manifest(mock_item)
            .expect("valid json")
            .expect("should produce a game");

        assert_eq!(game.id, "epic:dc9d2e595d0e4650b35d659f90d41059");
        assert_eq!(game.title, "Alan Wake 2 Deluxe Edition");
        assert_eq!(game.clean_title, "Alan Wake 2");
        assert_eq!(game.platform, GamePlatform::Epic);
        assert_eq!(game.install_dir, r"D:\Epic Games\AlanWake2");
        assert_eq!(
            game.executable_path.as_deref(),
            Some(r"D:\Epic Games\AlanWake2\Binaries\Win64\AlanWake2.exe")
        );
        assert_eq!(
            game.launch_uri.as_deref(),
            Some("com.epicgames.launcher://apps/da155068a9034375839136b4697593c1%3A235b28b9c6e2469da792fbabbc2fd25e%3Adc9d2e595d0e4650b35d659f90d41059?action=launch&silent=true")
        );
    }

    #[test]
    fn skips_incomplete_or_dlc_epic_manifests() {
        let incomplete = r#"{
            "bIsIncompleteInstall": true,
            "DisplayName": "Control",
            "InstallLocation": "D:\\Epic\\Control",
            "LaunchExecutable": "Control.exe",
            "AppName": "Calluna"
        }"#;
        assert!(parse_epic_item_manifest(incomplete).unwrap().is_none());

        let dlc = r#"{
            "bIsIncompleteInstall": false,
            "DisplayName": "Control Foundation DLC",
            "InstallLocation": "D:\\Epic\\Control",
            "LaunchExecutable": "Control.exe",
            "AppName": "CallunaDLC1",
            "MainGameAppName": "Calluna"
        }"#;
        assert!(parse_epic_item_manifest(dlc).unwrap().is_none());
    }

    #[test]
    fn scans_epic_manifests_directory() {
        let dir = tempdir().expect("tempdir");
        fs::write(
            dir.path().join("Sifu.item"),
            r#"{
                "bIsIncompleteInstall": false,
                "DisplayName": "Sifu™",
                "InstallLocation": "D:\\Epic Games\\Sifu",
                "LaunchExecutable": "Sifu.exe",
                "AppName": "Turquoise"
            }"#,
        )
        .unwrap();
        // Non-.item files should be ignored silently
        fs::write(dir.path().join("notes.txt"), "not a manifest").unwrap();

        let out = scan_epic_manifests_dir(dir.path());
        assert!(out.warnings.is_empty());
        assert_eq!(out.games.len(), 1);
        assert_eq!(out.games[0].clean_title, "Sifu");
    }
}
