use crate::cleaner::clean_game_title;
use crate::models::{DetectedGame, GamePlatform, SourceScanOutput};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

/// Substrings excluded (case-insensitively) when searching for the main Battle.net game `.exe`.
pub const EXCLUDED_BATTLENET_EXE_SUBSTRINGS: &[&str] = &[
    "unins",
    "crash",
    "crashmailer",
    "blizzarderror",
];

/// Exact lowercase names/subkeys belonging to the Battle.net desktop client itself rather than a game.
const BATTLENET_CLIENT_NAMES: &[&str] = &[
    "battle.net",
    "battle.net app",
    "blizzard battle.net",
    "blizzard battle.net app",
    "battle.net desktop app",
    "blizzard update agent",
    "battle.net update agent",
];

/// OS-independent representation of a Windows `Uninstall` registry subkey.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RawUninstallEntry {
    pub subkey_name: String,
    pub display_name: String,
    pub install_location: String,
    pub uninstall_string: String,
    pub publisher: String,
}

/// Reads Windows `HKLM\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall`
/// and scans installed Battle.net / Blizzard / Activision titles.
pub fn scan_battlenet_installed() -> SourceScanOutput {
    let entries = read_windows_uninstall_entries();
    scan_battlenet_from_entries(&entries)
}

#[cfg(target_os = "windows")]
fn read_windows_uninstall_entries() -> Vec<RawUninstallEntry> {
    use winreg::enums::HKEY_LOCAL_MACHINE;
    use winreg::RegKey;

    let mut results = Vec::new();
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let uninstall_path = r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall";

    let Ok(uninstall_key) = hklm.open_subkey(uninstall_path) else {
        return results;
    };

    for subkey_name in uninstall_key.enum_keys().flatten() {
        let Ok(subkey) = uninstall_key.open_subkey(&subkey_name) else {
            continue;
        };

        let display_name = subkey.get_value::<String, _>("DisplayName").unwrap_or_default();
        let install_location = subkey
            .get_value::<String, _>("InstallLocation")
            .unwrap_or_default();
        let uninstall_string = subkey
            .get_value::<String, _>("UninstallString")
            .unwrap_or_default();
        let publisher = subkey.get_value::<String, _>("Publisher").unwrap_or_default();

        results.push(RawUninstallEntry {
            subkey_name,
            display_name,
            install_location,
            uninstall_string,
            publisher,
        });
    }

    results
}

#[cfg(not(target_os = "windows"))]
fn read_windows_uninstall_entries() -> Vec<RawUninstallEntry> {
    Vec::new()
}

/// Returns `true` if the `Uninstall` registry entry belongs to a Battle.net / Blizzard / Activision
/// game, excluding the Battle.net launcher client itself.
pub fn is_battlenet_game_entry(entry: &RawUninstallEntry) -> bool {
    let display_trimmed = entry.display_name.trim();
    let install_trimmed = entry.install_location.trim();
    if display_trimmed.is_empty() || install_trimmed.is_empty() {
        return false;
    }

    let subkey_lower = entry.subkey_name.trim().to_ascii_lowercase();
    let display_lower = display_trimmed.to_ascii_lowercase();

    // Exclude the Battle.net client itself
    if subkey_lower == "battle.net"
        || BATTLENET_CLIENT_NAMES
            .iter()
            .any(|client_name| display_lower == *client_name)
    {
        return false;
    }

    let uninstall_lower = entry.uninstall_string.to_ascii_lowercase();
    let publisher_lower = entry.publisher.to_ascii_lowercase();

    uninstall_lower.contains("battle.net")
        || publisher_lower.contains("blizzard")
        || publisher_lower.contains("activision")
}

/// Evaluates raw Windows `Uninstall` registry entries and resolves each Battle.net game's
/// `DisplayName`, `InstallLocation`, and main executable in the root of `InstallLocation`.
pub fn scan_battlenet_from_entries(entries: &[RawUninstallEntry]) -> SourceScanOutput {
    let mut output = SourceScanOutput::default();
    let mut seen_install_dirs = HashSet::new();
    let mut seen_ids = HashSet::new();

    for entry in entries {
        if !is_battlenet_game_entry(entry) {
            continue;
        }

        let display_name = entry.display_name.trim();
        let install_location = entry.install_location.trim();
        let install_path = PathBuf::from(install_location);

        if !install_path.is_dir() {
            continue;
        }

        let dir_key = normalize_dir_key(install_location);
        if !seen_install_dirs.insert(dir_key) {
            continue;
        }

        let exe_path = find_battlenet_main_executable(&install_path, display_name)
            .map(|p| p.to_string_lossy().to_string());

        let raw_key = if !entry.subkey_name.trim().is_empty() {
            entry.subkey_name.trim()
        } else {
            display_name
        };
        let id = format!("battlenet:{}", slugify_key(raw_key));
        if !seen_ids.insert(id.clone()) {
            continue;
        }

        let mut game = DetectedGame::new_detected(
            id,
            display_name.to_string(),
            clean_game_title(display_name),
            GamePlatform::BattleNet,
            (!entry.subkey_name.trim().is_empty()).then(|| entry.subkey_name.trim().to_string()),
            install_location.to_string(),
            exe_path,
            None,
        );
        game.version = std::fs::read_to_string(Path::new(install_location).join(".build.info"))
            .ok()
            .and_then(|info| version_in_build_info(&info));
        output.games.push(game);
    }

    output
}

/// Finds the primary game executable in the root of `install_dir`:
/// - Excludes any `.exe` containing `unins`, `crash`, `CrashMailer`, or `BlizzardError` (case-insensitive),
///   as well as `Battle.net.exe`.
/// - Prefers `*Launcher.exe` (e.g. `Diablo IV Launcher.exe`, `Overwatch Launcher.exe`),
///   otherwise picks the main `.exe` matching the game title in `install_dir`.
pub fn find_battlenet_main_executable(install_dir: &Path, display_name: &str) -> Option<PathBuf> {
    if let Some(root_exe) = pick_best_exe_in_dir(install_dir, display_name) {
        return Some(root_exe);
    }

    // Fallback: check immediate subdirectories (such as `_retail_`) if root has no `.exe`
    let entries = fs::read_dir(install_dir).ok()?;
    let mut subdirs: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    subdirs.sort();

    for subdir in subdirs {
        if let Some(sub_exe) = pick_best_exe_in_dir(&subdir, display_name) {
            return Some(sub_exe);
        }
    }

    None
}

fn pick_best_exe_in_dir(dir: &Path, display_name: &str) -> Option<PathBuf> {
    let entries = fs::read_dir(dir).ok()?;
    let mut candidates: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file() && is_valid_battlenet_exe(p))
        .collect();

    if candidates.is_empty() {
        return None;
    }

    let title_tokens: Vec<String> = display_name
        .split(|c: char| !c.is_alphanumeric())
        .filter(|tok| tok.len() >= 2)
        .map(|tok| tok.to_ascii_lowercase())
        .collect();

    // Sort so the highest-priority executable comes first:
    // 1. `*Launcher.exe` preferred (`is_launcher == true`)
    // 2. Higher token overlap with `DisplayName`
    // 3. Deterministic alphabetical tie-breaker
    candidates.sort_by(|a, b| {
        let a_name = a
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        let b_name = b
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();

        let a_is_launcher = a_name.ends_with("launcher.exe");
        let b_is_launcher = b_name.ends_with("launcher.exe");

        let a_score = token_match_score(&a_name, &title_tokens);
        let b_score = token_match_score(&b_name, &title_tokens);

        b_is_launcher
            .cmp(&a_is_launcher)
            .then_with(|| b_score.cmp(&a_score))
            .then_with(|| a_name.cmp(&b_name))
    });

    candidates.into_iter().next()
}

fn is_valid_battlenet_exe(path: &Path) -> bool {
    let is_exe = path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("exe"));
    if !is_exe {
        return false;
    }

    let Some(file_name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    let lower = file_name.to_ascii_lowercase();
    if lower == "battle.net.exe" {
        return false;
    }

    !EXCLUDED_BATTLENET_EXE_SUBSTRINGS
        .iter()
        .any(|excluded| lower.contains(excluded))
}

fn token_match_score(exe_lower: &str, title_tokens: &[String]) -> usize {
    title_tokens
        .iter()
        .filter(|tok| exe_lower.contains(tok.as_str()))
        .count()
}

fn normalize_dir_key(path: &str) -> String {
    path.replace('\\', "/")
        .trim_end_matches('/')
        .to_ascii_lowercase()
}

fn slugify_key(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn scans_mock_battlenet_uninstall_entries_and_excludes_client_and_helpers() {
        let tmp = tempdir().expect("tempdir");

        // 1. Battle.net client folder (must be excluded)
        let bnet_client_dir = tmp.path().join("Battle.net");
        fs::create_dir_all(&bnet_client_dir).unwrap();
        fs::write(bnet_client_dir.join("Battle.net Launcher.exe"), b"mock").unwrap();

        // 2. Blizzard game WITH *Launcher.exe alongside main exe & error/crash helpers
        let diablo_dir = tmp.path().join("Diablo IV");
        fs::create_dir_all(&diablo_dir).unwrap();
        fs::write(diablo_dir.join("Diablo IV.exe"), b"mock").unwrap();
        fs::write(diablo_dir.join("Diablo IV Launcher.exe"), b"mock").unwrap();
        fs::write(diablo_dir.join("BlizzardError.exe"), b"mock").unwrap();
        fs::write(diablo_dir.join("CrashMailer_64.exe"), b"mock").unwrap();
        fs::write(diablo_dir.join("unins000.exe"), b"mock").unwrap();

        // 3. Activision game WITHOUT *Launcher.exe, matched via Publisher = Activision
        let cod_dir = tmp.path().join("Call of Duty");
        fs::create_dir_all(&cod_dir).unwrap();
        fs::write(cod_dir.join("cod.exe"), b"mock").unwrap();
        fs::write(cod_dir.join("crash_handler.exe"), b"mock").unwrap();
        fs::write(cod_dir.join("CrashMailer.exe"), b"mock").unwrap();
        fs::write(cod_dir.join("BlizzardError.exe"), b"mock").unwrap();
        fs::write(cod_dir.join("unins001.exe"), b"mock").unwrap();

        // 4. Unrelated third-party Windows app (must be excluded)
        let seven_zip_dir = tmp.path().join("7-Zip");
        fs::create_dir_all(&seven_zip_dir).unwrap();
        fs::write(seven_zip_dir.join("7zFM.exe"), b"mock").unwrap();

        let mock_entries = vec![
            RawUninstallEntry {
                subkey_name: "Battle.net".to_string(),
                display_name: "Battle.net".to_string(),
                install_location: bnet_client_dir.to_string_lossy().to_string(),
                uninstall_string: r"C:\ProgramData\Battle.net\Agent\Blizzard Uninstaller.exe"
                    .to_string(),
                publisher: "Blizzard Entertainment".to_string(),
            },
            RawUninstallEntry {
                subkey_name: "Fenris".to_string(),
                display_name: "Diablo® IV".to_string(),
                install_location: diablo_dir.to_string_lossy().to_string(),
                uninstall_string:
                    r"C:\ProgramData\Battle.net\Agent\Blizzard Uninstaller.exe --lang=itIT"
                        .to_string(),
                publisher: "Blizzard Entertainment".to_string(),
            },
            RawUninstallEntry {
                subkey_name: "ODIN".to_string(),
                display_name: "Call of Duty®".to_string(),
                install_location: cod_dir.to_string_lossy().to_string(),
                uninstall_string: r"C:\Games\COD\unins001.exe".to_string(),
                publisher: "Activision Publishing, Inc.".to_string(),
            },
            RawUninstallEntry {
                subkey_name: "7-Zip".to_string(),
                display_name: "7-Zip 24.08 (x64)".to_string(),
                install_location: seven_zip_dir.to_string_lossy().to_string(),
                uninstall_string: r"C:\Program Files\7-Zip\Uninstall.exe".to_string(),
                publisher: "Igor Pavlov".to_string(),
            },
        ];

        let out = scan_battlenet_from_entries(&mock_entries);
        assert!(out.warnings.is_empty());
        assert_eq!(out.games.len(), 2);

        let diablo = out
            .games
            .iter()
            .find(|g| g.clean_title == "Diablo IV")
            .expect("Diablo IV detected");
        assert_eq!(diablo.platform, GamePlatform::BattleNet);
        assert_eq!(diablo.platform.as_str(), "Battle.net");
        assert_eq!(
            serde_json::to_string(&diablo.platform).unwrap(),
            "\"Battle.net\""
        );
        assert_eq!(
            Path::new(diablo.executable_path.as_deref().unwrap()),
            diablo_dir.join("Diablo IV Launcher.exe")
        );

        let cod = out
            .games
            .iter()
            .find(|g| g.clean_title == "Call of Duty")
            .expect("Call of Duty detected");
        assert_eq!(cod.platform, GamePlatform::BattleNet);
        assert_eq!(
            Path::new(cod.executable_path.as_deref().unwrap()),
            cod_dir.join("cod.exe")
        );
    }
}

/// The version of a Blizzard game, from the `.build.info` its client keeps in the game's
/// folder: a table with columns separated by `|`, one of them named `Version!STRING:0`.
fn version_in_build_info(info: &str) -> Option<String> {
    let mut lines = info.lines().filter(|line| !line.trim().is_empty());
    let column = lines.next()?.split('|').position(|name| name.trim().starts_with("Version!"))?;
    // The row of the build in use comes first
    super::plain_version(lines.next()?.split('|').nth(column)?)
}

#[cfg(test)]
mod version_tests {
    use super::version_in_build_info;

    #[test]
    fn reads_the_version_from_blizzard_s_build_info() {
        let info = "Branch!STRING:0|Active!DEC:1|Build Key!HEX:16|Version!STRING:0|Product!STRING:0\nus|1|abcdef|16.3.41150|hero\neu|0|abcdef|16.2.40000|hero\n";
        assert_eq!(version_in_build_info(info).as_deref(), Some("16.3.41150"));
        assert_eq!(version_in_build_info("Branch!STRING:0|Active!DEC:1\nus|1\n"), None, "no such column");
        assert_eq!(version_in_build_info("Branch!STRING:0|Version!STRING:0\n"), None, "no row");
        assert_eq!(version_in_build_info("Branch!STRING:0|Version!STRING:0\nus\n"), None, "a short row");
        assert_eq!(version_in_build_info(""), None);
    }
}
