//! GOG / GOG Galaxy: `HKLM\SOFTWARE\WOW6432Node\GOG.com\Games\<gameID>`.

use super::registry::{read_registry_subkeys, RegistryEntry, RegistryHive};
use crate::cleaner::clean_game_title;
use crate::models::{DetectedGame, GamePlatform, SourceScanOutput};
use std::collections::HashSet;
use std::path::Path;

const GOG_GAMES_KEYS: &[&str] = &[
    r"SOFTWARE\WOW6432Node\GOG.com\Games",
    r"SOFTWARE\GOG.com\Games",
];

/// Scans the games GOG (Galaxy or the offline installers) registered on this PC.
pub fn scan_gog_installed() -> SourceScanOutput {
    let entries: Vec<RegistryEntry> = GOG_GAMES_KEYS
        .iter()
        .flat_map(|path| read_registry_subkeys(RegistryHive::LocalMachine, path))
        .collect();
    scan_gog_from_entries(&entries)
}

/// Each GOG game key carries `gameID`, `gameName`, `path` and the full `exe` path. DLC keys
/// have a non-empty `dependsOn` and are skipped; GOG games start by running their exe directly.
pub fn scan_gog_from_entries(entries: &[RegistryEntry]) -> SourceScanOutput {
    let mut output = SourceScanOutput::default();
    let mut seen_ids = HashSet::new();

    for entry in entries {
        if !entry.get("dependsOn").is_empty() {
            continue;
        }

        let name = entry.get("gameName");
        let install_dir = entry.get("path");
        if name.is_empty() || install_dir.is_empty() || !Path::new(install_dir).is_dir() {
            continue;
        }

        let game_id = match entry.get("gameID") {
            "" => entry.subkey_name.trim(),
            id => id,
        };
        if game_id.is_empty() || !seen_ids.insert(game_id.to_string()) {
            continue;
        }

        let exe = match (entry.get("exe"), entry.get("exeFile")) {
            ("", "") => None,
            ("", file) => Some(
                Path::new(install_dir)
                    .join(file)
                    .to_string_lossy()
                    .to_string(),
            ),
            (full_path, _) => Some(full_path.to_string()),
        };

        let mut game = DetectedGame::new_detected(
            format!("gog:{game_id}"),
            name.to_string(),
            clean_game_title(name),
            GamePlatform::Gog,
            Some(game_id.to_string()),
            install_dir.to_string(),
            exe,
            None,
        );
        // GOG writes the version it installed next to the game's other values
        game.version = super::plain_version(entry.get("ver"));
        output.games.push(game);
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn scans_gog_games_and_skips_dlc_and_leftover_keys() {
        let tmp = tempdir().unwrap();
        let witcher_dir = tmp.path().join("The Witcher 3");
        std::fs::create_dir_all(&witcher_dir).unwrap();
        let witcher = witcher_dir.to_string_lossy().to_string();
        let exe = witcher_dir.join("bin").join("x64").join("witcher3.exe");
        let exe = exe.to_string_lossy().to_string();

        let entries = vec![
            RegistryEntry::new(
                "1495134320",
                &[
                    ("gameID", "1495134320"),
                    ("gameName", "The Witcher 3: Wild Hunt - Game of the Year Edition"),
                    ("path", &witcher),
                    ("exe", &exe),
                    ("dependsOn", ""),
                ],
            ),
            // DLC of the game above
            RegistryEntry::new(
                "1495134321",
                &[
                    ("gameName", "The Witcher 3 - Blood and Wine"),
                    ("path", &witcher),
                    ("dependsOn", "1495134320"),
                ],
            ),
            // Uninstalled game whose key was left behind
            RegistryEntry::new(
                "1207658930",
                &[
                    ("gameName", "Gone Game"),
                    ("path", r"Z:\not\here"),
                    ("exe", r"Z:\not\here\gone.exe"),
                ],
            ),
            // Same game registered in both registry views
            RegistryEntry::new(
                "1495134320",
                &[("gameName", "The Witcher 3"), ("path", &witcher)],
            ),
        ];

        let out = scan_gog_from_entries(&entries);
        assert!(out.warnings.is_empty());
        assert_eq!(out.games.len(), 1);

        let game = &out.games[0];
        assert_eq!(game.id, "gog:1495134320");
        assert_eq!(game.platform, GamePlatform::Gog);
        assert_eq!(game.clean_title, "The Witcher 3: Wild Hunt");
        assert_eq!(game.executable_path.as_deref(), Some(exe.as_str()));
        assert_eq!(game.launch_uri, None);
        assert_eq!(game.install_dir, witcher);
    }

    #[test]
    fn builds_exe_path_from_exefile_when_full_path_is_missing() {
        let tmp = tempdir().unwrap();
        let dir = tmp.path().to_string_lossy().to_string();
        let entries = vec![RegistryEntry::new(
            "42",
            &[("gameName", "Tiny"), ("path", &dir), ("exeFile", "tiny.exe")],
        )];
        let out = scan_gog_from_entries(&entries);
        assert_eq!(
            out.games[0].executable_path.as_deref(),
            Some(tmp.path().join("tiny.exe").to_string_lossy().as_ref())
        );
    }
}
