//! Ubisoft Connect: `Uninstall\Uplay Install <id>` keys (name + folder) and
//! `HKLM\SOFTWARE\WOW6432Node\Ubisoft\Launcher\Installs\<id>` (folder only).

use super::exe_pick::pick_main_executable;
use super::registry::{read_registry_subkeys, read_uninstall_entries, RegistryEntry, RegistryHive};
use crate::cleaner::clean_game_title;
use crate::models::{DetectedGame, GamePlatform, SourceScanOutput};
use std::collections::HashSet;
use std::path::Path;

const UBISOFT_INSTALLS_KEYS: &[&str] = &[
    r"SOFTWARE\WOW6432Node\Ubisoft\Launcher\Installs",
    r"SOFTWARE\Ubisoft\Launcher\Installs",
];

const UPLAY_UNINSTALL_PREFIX: &str = "uplay install ";

pub fn scan_ubisoft_installed() -> SourceScanOutput {
    let installs: Vec<RegistryEntry> = UBISOFT_INSTALLS_KEYS
        .iter()
        .flat_map(|path| read_registry_subkeys(RegistryHive::LocalMachine, path))
        .collect();
    scan_ubisoft_from_entries(&read_uninstall_entries(), &installs)
}

/// Ubisoft Connect games are started through `uplay://launch/<id>/0`, with the main executable
/// kept as a fallback. `Uninstall` keys give the proper display name; games that only appear
/// under `Launcher\Installs` are listed with their folder name.
pub fn scan_ubisoft_from_entries(
    uninstall_entries: &[RegistryEntry],
    launcher_installs: &[RegistryEntry],
) -> SourceScanOutput {
    let mut output = SourceScanOutput::default();
    let mut seen_ids = HashSet::new();

    let named = uninstall_entries.iter().filter_map(|entry| {
        let key = entry.subkey_name.trim();
        if key.len() <= UPLAY_UNINSTALL_PREFIX.len()
            || !key
                .get(..UPLAY_UNINSTALL_PREFIX.len())?
                .eq_ignore_ascii_case(UPLAY_UNINSTALL_PREFIX)
        {
            return None;
        }
        let id = key[UPLAY_UNINSTALL_PREFIX.len()..].trim();
        Some((id, entry.get("DisplayName"), entry.get("InstallLocation")))
    });

    let unnamed = launcher_installs
        .iter()
        .map(|entry| (entry.subkey_name.trim(), "", entry.get("InstallDir")));

    for (id, display_name, install_dir) in named.chain(unnamed) {
        if id.is_empty() || install_dir.is_empty() || !Path::new(install_dir).is_dir() {
            continue;
        }
        if !seen_ids.insert(id.to_string()) {
            continue;
        }

        let folder_name = Path::new(install_dir.trim_end_matches(['\\', '/']))
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        let title = if display_name.is_empty() {
            folder_name
        } else {
            display_name
        };
        if title.is_empty() {
            continue;
        }

        let exe = pick_main_executable(Path::new(install_dir), title)
            .map(|p| p.to_string_lossy().to_string());

        output.games.push(DetectedGame::new_detected(
            format!("ubisoft:{id}"),
            title.to_string(),
            clean_game_title(title),
            GamePlatform::Ubisoft,
            Some(id.to_string()),
            install_dir.to_string(),
            exe,
            Some(format!("uplay://launch/{id}/0")),
        ));
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn scans_uplay_uninstall_keys_and_launcher_only_installs() {
        let tmp = tempdir().unwrap();
        let anno_dir = tmp.path().join("Anno 1800");
        let valhalla_dir = tmp.path().join("Assassin's Creed Valhalla");
        fs::create_dir_all(&anno_dir).unwrap();
        fs::create_dir_all(&valhalla_dir).unwrap();
        fs::write(anno_dir.join("Anno1800.exe"), b"exe").unwrap();
        let anno = anno_dir.to_string_lossy().to_string();
        let valhalla = format!("{}/", valhalla_dir.to_string_lossy());

        let uninstall = vec![
            RegistryEntry::new(
                "Uplay Install 4553",
                &[("DisplayName", "Anno 1800"), ("InstallLocation", &anno)],
            ),
            // Unrelated program
            RegistryEntry::new(
                "{90160000-0011}",
                &[("DisplayName", "Office"), ("InstallLocation", &anno)],
            ),
            // Uninstalled: folder is gone
            RegistryEntry::new(
                "Uplay Install 635",
                &[("DisplayName", "Far Cry"), ("InstallLocation", r"Z:\gone")],
            ),
        ];
        let installs = vec![
            // Already found through its Uninstall key
            RegistryEntry::new("4553", &[("InstallDir", &anno)]),
            // Only known to the launcher: named after its folder
            RegistryEntry::new("13504", &[("InstallDir", &valhalla)]),
        ];

        let out = scan_ubisoft_from_entries(&uninstall, &installs);
        assert_eq!(out.games.len(), 2);

        let anno_game = out.games.iter().find(|g| g.id == "ubisoft:4553").unwrap();
        assert_eq!(anno_game.title, "Anno 1800");
        assert_eq!(anno_game.platform, GamePlatform::Ubisoft);
        assert_eq!(anno_game.launch_uri.as_deref(), Some("uplay://launch/4553/0"));
        assert_eq!(
            anno_game.executable_path.as_deref(),
            Some(anno_dir.join("Anno1800.exe").to_string_lossy().as_ref())
        );

        let valhalla_game = out.games.iter().find(|g| g.id == "ubisoft:13504").unwrap();
        assert_eq!(valhalla_game.title, "Assassin's Creed Valhalla");
        assert_eq!(valhalla_game.executable_path, None);
    }
}
