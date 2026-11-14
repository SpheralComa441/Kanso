//! EA app (and legacy Origin) games, found among the Windows `Uninstall` keys.

use super::exe_pick::{pick_main_executable, slugify};
use super::registry::{read_uninstall_entries, RegistryEntry};
use crate::cleaner::clean_game_title;
use crate::models::{DetectedGame, GamePlatform, SourceScanOutput};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

/// Entries published by Electronic Arts that are the client or its services, not games.
const EA_CLIENT_NAMES: &[&str] = &[
    "ea app",
    "ea desktop",
    "ea anticheat",
    "ea background service",
    "origin",
    "ea play",
];

pub fn scan_ea_installed() -> SourceScanOutput {
    scan_ea_from_entries(&read_uninstall_entries())
}

/// An `Uninstall` entry is an EA game when Electronic Arts is its publisher and it was laid
/// down by the EA installer: either the game folder contains the installer's `__Installer`
/// metadata folder, or its uninstaller lives under `Common Files\EAInstaller`.
pub fn is_ea_game_entry(entry: &RegistryEntry) -> bool {
    let display_name = entry.get("DisplayName").to_ascii_lowercase();
    let install_dir = entry.get("InstallLocation");
    if display_name.is_empty() || install_dir.is_empty() {
        return false;
    }
    if !entry
        .get("Publisher")
        .to_ascii_lowercase()
        .contains("electronic arts")
    {
        return false;
    }
    if EA_CLIENT_NAMES.contains(&display_name.as_str()) {
        return false;
    }

    entry
        .get("UninstallString")
        .to_ascii_lowercase()
        .contains("eainstaller")
        || Path::new(install_dir).join("__Installer").is_dir()
}

pub fn scan_ea_from_entries(uninstall_entries: &[RegistryEntry]) -> SourceScanOutput {
    let mut output = SourceScanOutput::default();
    let mut seen_ids = HashSet::new();

    for entry in uninstall_entries {
        if !is_ea_game_entry(entry) {
            continue;
        }
        let display_name = entry.get("DisplayName");
        let install_dir = entry.get("InstallLocation");
        let install_path = Path::new(install_dir);
        if !install_path.is_dir() {
            continue;
        }

        let id = format!("ea:{}", slugify(display_name));
        if !seen_ids.insert(id.clone()) {
            continue;
        }

        let exe = launcher_from_installer_data(install_path)
            .or_else(|| pick_main_executable(install_path, display_name))
            .map(|p| p.to_string_lossy().to_string());

        output.games.push(DetectedGame::new_detected(
            id,
            display_name.to_string(),
            clean_game_title(display_name),
            GamePlatform::Ea,
            None,
            install_dir.to_string(),
            exe,
            None,
        ));
    }

    output
}

/// Reads the game's real launcher from `__Installer\installerdata.xml`, where the EA installer
/// records it as `<filePath>[HKEY_LOCAL_MACHINE\...\Install Dir]Game.exe</filePath>`.
/// Trial launchers are ignored. Returns `None` when the file is missing or names no existing exe.
fn launcher_from_installer_data(install_dir: &Path) -> Option<PathBuf> {
    let xml = fs::read_to_string(install_dir.join("__Installer").join("installerdata.xml")).ok()?;
    parse_installer_data_launchers(&xml)
        .into_iter()
        .map(|relative| install_dir.join(relative))
        .find(|candidate| candidate.is_file())
}

/// Relative paths of the non-trial `<launcher>` executables listed in `installerdata.xml`.
pub fn parse_installer_data_launchers(xml: &str) -> Vec<String> {
    let mut launchers = Vec::new();
    for block in xml.split("<launcher").skip(1) {
        let block = block.split("</launcher>").next().unwrap_or(block);
        if text_between(block, "<trial>", "</trial>").is_some_and(|t| t.trim() == "1") {
            continue;
        }
        let Some(file_path) = text_between(block, "<filePath>", "</filePath>") else {
            continue;
        };
        // Drop the `[registry key]` placeholder that stands for the install folder
        let relative = file_path
            .rsplit(']')
            .next()
            .unwrap_or(file_path)
            .trim()
            .trim_start_matches(['\\', '/'])
            .replace('\\', std::path::MAIN_SEPARATOR_STR);
        if !relative.is_empty() && relative.to_ascii_lowercase().ends_with(".exe") {
            launchers.push(relative);
        }
    }
    launchers
}

fn text_between<'a>(haystack: &'a str, open: &str, close: &str) -> Option<&'a str> {
    let start = haystack.find(open)? + open.len();
    let end = haystack[start..].find(close)? + start;
    Some(&haystack[start..end])
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    const INSTALLER_DATA: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<DiPManifest version="4.0">
  <runtime>
    <launcher uid="1">
      <name locale="en_US">Trial</name>
      <filePath>[HKEY_LOCAL_MACHINE\SOFTWARE\EA Sports\EA SPORTS FC 24\Install Dir]FC24_Trial.exe</filePath>
      <trial>1</trial>
    </launcher>
    <launcher uid="2">
      <name locale="en_US">EA SPORTS FC 24</name>
      <filePath>[HKEY_LOCAL_MACHINE\SOFTWARE\EA Sports\EA SPORTS FC 24\Install Dir]FC24.exe</filePath>
      <trial>0</trial>
    </launcher>
  </runtime>
</DiPManifest>"#;

    #[test]
    fn parses_non_trial_launchers_from_installer_data() {
        assert_eq!(parse_installer_data_launchers(INSTALLER_DATA), vec!["FC24.exe"]);
        assert!(parse_installer_data_launchers("<DiPManifest/>").is_empty());
    }

    #[test]
    fn scans_ea_games_and_excludes_client_and_other_publishers() {
        let tmp = tempdir().unwrap();

        // Game with installer metadata: launcher comes from installerdata.xml
        let fc_dir = tmp.path().join("EA SPORTS FC 24");
        fs::create_dir_all(fc_dir.join("__Installer")).unwrap();
        fs::write(fc_dir.join("__Installer").join("installerdata.xml"), INSTALLER_DATA).unwrap();
        fs::write(fc_dir.join("FC24.exe"), b"game").unwrap();
        fs::write(fc_dir.join("FC24_Trial.exe"), vec![0u8; 5000]).unwrap();
        let fc = fc_dir.to_string_lossy().to_string();

        // Game recognised by its EAInstaller uninstaller: exe picked heuristically
        let apex_dir = tmp.path().join("Apex");
        fs::create_dir_all(&apex_dir).unwrap();
        fs::write(apex_dir.join("r5apex.exe"), vec![0u8; 300]).unwrap();
        fs::write(apex_dir.join("EasyAntiCheat_launcher.exe"), vec![0u8; 900]).unwrap();
        let apex = apex_dir.to_string_lossy().to_string();

        let entries = vec![
            RegistryEntry::new(
                "{FC24-GUID}",
                &[
                    ("DisplayName", "EA SPORTS FC™ 24"),
                    ("Publisher", "Electronic Arts"),
                    ("InstallLocation", &fc),
                    ("UninstallString", r#""C:\Program Files\Common Files\EAInstaller\EA SPORTS FC 24\Cleanup.exe" uninstall_game -autologging"#),
                ],
            ),
            RegistryEntry::new(
                "{APEX-GUID}",
                &[
                    ("DisplayName", "Apex Legends"),
                    ("Publisher", "Electronic Arts, Inc."),
                    ("InstallLocation", &apex),
                    ("UninstallString", r#""C:\Program Files\Common Files\EAInstaller\Apex\Cleanup.exe" uninstall_game"#),
                ],
            ),
            // The client itself
            RegistryEntry::new(
                "{EA-APP}",
                &[
                    ("DisplayName", "EA app"),
                    ("Publisher", "Electronic Arts"),
                    ("InstallLocation", &apex),
                    ("UninstallString", r#""C:\ProgramData\Package Cache\{x}\EAappInstaller.exe" /uninstall"#),
                ],
            ),
            // Another publisher's program sitting in a folder with an exe
            RegistryEntry::new(
                "{OTHER}",
                &[
                    ("DisplayName", "Some Tool"),
                    ("Publisher", "Someone Else"),
                    ("InstallLocation", &apex),
                    ("UninstallString", "uninstall.exe"),
                ],
            ),
            // EA entry with neither installer marker
            RegistryEntry::new(
                "{EA-MISC}",
                &[
                    ("DisplayName", "EA Something"),
                    ("Publisher", "Electronic Arts"),
                    ("InstallLocation", &apex),
                    ("UninstallString", "msiexec /x {EA-MISC}"),
                ],
            ),
        ];

        let out = scan_ea_from_entries(&entries);
        let ids: Vec<&str> = out.games.iter().map(|g| g.id.as_str()).collect();
        assert_eq!(ids, vec!["ea:ea-sports-fc-24", "ea:apex-legends"]);

        let fc_game = &out.games[0];
        assert_eq!(fc_game.platform, GamePlatform::Ea);
        assert_eq!(fc_game.clean_title, "EA SPORTS FC 24");
        assert_eq!(
            fc_game.executable_path.as_deref(),
            Some(fc_dir.join("FC24.exe").to_string_lossy().as_ref()),
            "the non-trial launcher from installerdata.xml wins over the bigger trial exe"
        );
        assert_eq!(
            out.games[1].executable_path.as_deref(),
            Some(apex_dir.join("r5apex.exe").to_string_lossy().as_ref())
        );
    }
}
