use crate::cleaner::clean_game_title;
use crate::models::{DetectedGame, GamePlatform, SourceScanOutput};
use crate::vdf::{parse_vdf, VdfValue};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

/// Known non-game Steam AppIDs excluded from library discovery
/// (`228980` = Steamworks Common Redistributables).
const EXCLUDED_STEAM_APP_IDS: &[&str] = &["228980"];

/// Reads the Steam installation path from the Windows Registry:
/// `HKCU\Software\Valve\Steam` -> `SteamPath`, with fallbacks to `HKLM` and standard paths.
pub fn detect_steam_root() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
        use winreg::RegKey;

        if let Ok(hkcu) = RegKey::predef(HKEY_CURRENT_USER).open_subkey(r"Software\Valve\Steam") {
            if let Ok(steam_path) = hkcu.get_value::<String, _>("SteamPath") {
                let candidate = normalize_windows_path(&steam_path);
                if candidate.exists() {
                    return Some(candidate);
                }
            }
        }

        for subkey in [
            r"SOFTWARE\WOW6432Node\Valve\Steam",
            r"SOFTWARE\Valve\Steam",
        ] {
            if let Ok(hklm) = RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey(subkey) {
                if let Ok(install_path) = hklm.get_value::<String, _>("InstallPath") {
                    let candidate = normalize_windows_path(&install_path);
                    if candidate.exists() {
                        return Some(candidate);
                    }
                }
            }
        }

        let fallback = PathBuf::from(r"C:\Program Files (x86)\Steam");
        if fallback.exists() {
            return Some(fallback);
        }

        None
    }

    #[cfg(not(target_os = "windows"))]
    {
        None
    }
}

#[cfg(target_os = "windows")]
fn normalize_windows_path(raw: &str) -> PathBuf {
    PathBuf::from(raw.trim().replace('/', r"\"))
}

/// Parses Steam's `libraryfolders.vdf` content and returns all configured library root paths.
/// Supports both modern nested sections (`"0" { "path" "..." }`) and legacy key-value entries.
pub fn parse_library_folders_vdf(content: &str) -> Result<Vec<PathBuf>, String> {
    let root = parse_vdf(content)?;
    let Some(folders_sec) = root.get_section("libraryfolders") else {
        return Err("Sezione 'libraryfolders' mancante in libraryfolders.vdf".to_string());
    };

    let mut paths = Vec::new();
    for (key, value) in &folders_sec.entries {
        match value {
            VdfValue::Section(sec) => {
                if let Some(p) = sec.get_str("path") {
                    let trimmed = p.trim();
                    if !trimmed.is_empty() {
                        paths.push(PathBuf::from(trimmed));
                    }
                }
            }
            VdfValue::Str(s) => {
                // Legacy format uses numeric keys ("1", "2", ...) mapped directly to folder paths
                if key.chars().all(|c| c.is_ascii_digit()) && !s.trim().is_empty() {
                    paths.push(PathBuf::from(s.trim()));
                }
            }
        }
    }

    Ok(paths)
}

/// Parses a single Steam `appmanifest_*.acf` file and extracts `appid`, `name`, and `installdir`.
/// Returns `Ok(None)` if the manifest belongs to an excluded redistributable tool.
pub fn parse_app_manifest_acf(
    content: &str,
    library_root: &Path,
) -> Result<Option<DetectedGame>, String> {
    let root = parse_vdf(content)?;
    let Some(app_state) = root.get_section("AppState") else {
        return Err("Sezione 'AppState' mancante nel file .acf".to_string());
    };

    let appid = app_state
        .get_str("appid")
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "Campo 'appid' mancante nel file .acf".to_string())?;

    if EXCLUDED_STEAM_APP_IDS.contains(&appid) {
        return Ok(None);
    }

    let name = app_state
        .get_str("name")
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| format!("Campo 'name' mancante per appid {appid}"))?;

    if name.eq_ignore_ascii_case("Steamworks Common Redistributables") {
        return Ok(None);
    }

    let installdir = app_state
        .get_str("installdir")
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| format!("Campo 'installdir' mancante per appid {appid}"))?;

    let full_install_dir = library_root
        .join("steamapps")
        .join("common")
        .join(installdir);

    Ok(Some(DetectedGame {
        id: format!("steam:{appid}"),
        title: name.to_string(),
        clean_title: clean_game_title(name),
        platform: GamePlatform::Steam,
        platform_app_id: Some(appid.to_string()),
        install_dir: full_install_dir.to_string_lossy().to_string(),
        executable_path: None,
        launch_uri: Some(format!("steam://rungameid/{appid}")),
    }))
}

/// Scans a Steam installation root, discovering all secondary libraries in `libraryfolders.vdf`
/// and parsing every `appmanifest_*.acf` found under `<library>/steamapps/`.
pub fn scan_steam_from_root(steam_root: &Path) -> SourceScanOutput {
    let mut output = SourceScanOutput::default();
    let mut library_roots: Vec<PathBuf> = vec![steam_root.to_path_buf()];

    let vdf_candidates = [
        steam_root.join("steamapps").join("libraryfolders.vdf"),
        steam_root.join("config").join("libraryfolders.vdf"),
    ];

    for vdf_path in vdf_candidates {
        if vdf_path.is_file() {
            match fs::read_to_string(&vdf_path) {
                Ok(content) => match parse_library_folders_vdf(&content) {
                    Ok(extra_paths) => {
                        library_roots.extend(extra_paths);
                        break;
                    }
                    Err(err) => output
                        .warnings
                        .push(format!("Steam VDF ({}): {err}", vdf_path.display())),
                },
                Err(err) => output.warnings.push(format!(
                    "Impossibile leggere {}: {err}",
                    vdf_path.display()
                )),
            }
        }
    }

    let mut seen_libs = HashSet::new();
    let mut seen_app_ids = HashSet::new();

    for lib_root in library_roots {
        let canonical_key = lib_root
            .to_string_lossy()
            .replace('\\', "/")
            .trim_end_matches('/')
            .to_lowercase();
        if !seen_libs.insert(canonical_key) {
            continue;
        }

        let steamapps_dir = lib_root.join("steamapps");
        if !steamapps_dir.is_dir() {
            continue;
        }

        let entries = match fs::read_dir(&steamapps_dir) {
            Ok(entries) => entries,
            Err(err) => {
                output.warnings.push(format!(
                    "Impossibile leggere la libreria Steam {}: {err}",
                    steamapps_dir.display()
                ));
                continue;
            }
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let Some(file_name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            let lower_name = file_name.to_ascii_lowercase();
            if !lower_name.starts_with("appmanifest_") || !lower_name.ends_with(".acf") {
                continue;
            }

            match fs::read_to_string(&path) {
                Ok(acf_content) => match parse_app_manifest_acf(&acf_content, &lib_root) {
                    Ok(Some(game)) => {
                        if seen_app_ids.insert(game.id.clone()) {
                            output.games.push(game);
                        }
                    }
                    Ok(None) => {}
                    Err(err) => output
                        .warnings
                        .push(format!("Manifest Steam ({}): {err}", path.display())),
                },
                Err(err) => output
                    .warnings
                    .push(format!("Impossibile leggere {}: {err}", path.display())),
            }
        }
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn parses_mock_steam_acf_manifest_and_cleans_title() {
        let mock_acf = r#"
"AppState"
{
	"appid"		"292030"
	"Universe"		"1"
	"name"		"The Witcher® 3: Wild Hunt - Game of the Year Edition"
	"StateFlags"		"4"
	"installdir"		"The Witcher 3"
	"LastUpdated"		"1700000000"
}
"#;
        let lib_root = Path::new("/mock/SteamLibrary");
        let parsed = parse_app_manifest_acf(mock_acf, lib_root)
            .expect("valid acf")
            .expect("should not be filtered out");

        assert_eq!(parsed.id, "steam:292030");
        assert_eq!(
            parsed.title,
            "The Witcher® 3: Wild Hunt - Game of the Year Edition"
        );
        assert_eq!(parsed.clean_title, "The Witcher 3: Wild Hunt");
        assert_eq!(parsed.platform, GamePlatform::Steam);
        assert_eq!(parsed.platform_app_id.as_deref(), Some("292030"));
        assert_eq!(
            Path::new(&parsed.install_dir),
            lib_root
                .join("steamapps")
                .join("common")
                .join("The Witcher 3")
        );
        assert_eq!(
            parsed.launch_uri.as_deref(),
            Some("steam://rungameid/292030")
        );
    }

    #[test]
    fn filters_out_steamworks_common_redistributables() {
        let redist_acf = r#"
"AppState"
{
	"appid"		"228980"
	"name"		"Steamworks Common Redistributables"
	"installdir"		"Steamworks Shared"
}
"#;
        let parsed = parse_app_manifest_acf(redist_acf, Path::new("/mock/Steam"))
            .expect("valid acf syntax");
        assert!(parsed.is_none());
    }

    #[test]
    fn scans_multiple_steam_libraries_from_vdf_and_acf_files() {
        let tmp = tempdir().expect("tempdir");
        let primary_steam = tmp.path().join("SteamPrimary");
        let secondary_lib = tmp.path().join("SteamSecondary");

        let primary_steamapps = primary_steam.join("steamapps");
        let secondary_steamapps = secondary_lib.join("steamapps");
        fs::create_dir_all(&primary_steamapps).unwrap();
        fs::create_dir_all(&secondary_steamapps).unwrap();

        let escaped_primary = primary_steam.to_string_lossy().replace('\\', "\\\\");
        let escaped_secondary = secondary_lib.to_string_lossy().replace('\\', "\\\\");

        let vdf = format!(
            r#"
"libraryfolders"
{{
    "0"
    {{
        "path" "{escaped_primary}"
    }}
    "1"
    {{
        "path" "{escaped_secondary}"
    }}
}}
"#
        );
        fs::write(primary_steamapps.join("libraryfolders.vdf"), vdf).unwrap();

        fs::write(
            primary_steamapps.join("appmanifest_1091500.acf"),
            r#"
"AppState"
{
    "appid" "1091500"
    "name" "Cyberpunk 2077"
    "installdir" "Cyberpunk 2077"
}
"#,
        )
        .unwrap();

        fs::write(
            secondary_steamapps.join("appmanifest_1245620.acf"),
            r#"
"AppState"
{
    "appid" "1245620"
    "name" "ELDEN RING™"
    "installdir" "ELDEN RING"
}
"#,
        )
        .unwrap();

        let scan = scan_steam_from_root(&primary_steam);
        assert!(scan.warnings.is_empty(), "warnings: {:?}", scan.warnings);
        assert_eq!(scan.games.len(), 2);

        let ids: HashSet<_> = scan.games.iter().map(|g| g.id.as_str()).collect();
        assert!(ids.contains("steam:1091500"));
        assert!(ids.contains("steam:1245620"));
    }
}
