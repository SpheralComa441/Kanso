use crate::cleaner::{clean_game_title, is_placeholder_title};
use crate::i18n::T;
use crate::models::{DetectedGame, GamePlatform, SourceScanOutput};
use crate::vdf::{parse_vdf, VdfValue};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

/// Known non-game Steam AppIDs excluded from library discovery
/// (`228980` = Steamworks Common Redistributables).
const EXCLUDED_STEAM_APP_IDS: &[&str] = &["228980"];

/// Environment variable that points Kanso at a specific Steam folder, bypassing the registry.
/// Meant for development and end-to-end tests (it also works on non-Windows hosts, where
/// there is no registry to read) and for unusual portable Steam setups.
pub const STEAM_ROOT_OVERRIDE_ENV: &str = "KANSO_STEAM_ROOT";

/// Reads the Steam installation path from the Windows Registry:
/// `HKCU\Software\Valve\Steam` -> `SteamPath`, with fallbacks to `HKLM` and standard paths.
/// A valid directory in [`STEAM_ROOT_OVERRIDE_ENV`] takes precedence over all of them.
pub fn detect_steam_root() -> Option<PathBuf> {
    if let Some(overridden) = std::env::var_os(STEAM_ROOT_OVERRIDE_ENV) {
        let candidate = PathBuf::from(overridden);
        if candidate.is_dir() {
            return Some(candidate);
        }
    }

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
        return Err(format!(
            "{}: libraryfolders.vdf (libraryfolders)",
            T::InvalidManifest.s()
        ));
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

/// Builds a readable stand-in title from a Steam `installdir` (`Slay_the_Spire` -> `Slay the Spire`),
/// used only while the manifest's own `name` is an `appid_<id>` placeholder.
fn provisional_title_from_installdir(installdir: &str) -> String {
    let leaf = installdir
        .rsplit(['\\', '/'])
        .find(|part| !part.trim().is_empty())
        .unwrap_or(installdir);
    let spaced = if leaf.contains(' ') {
        leaf.to_string()
    } else {
        leaf.replace('_', " ")
    };
    spaced.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Parses a single Steam `appmanifest_*.acf` file and extracts `appid`, `name`, and `installdir`.
/// Returns `Ok(None)` if the manifest belongs to an excluded redistributable tool.
///
/// When `name` is missing or is Steam's `appid_<id>` placeholder, the game is still returned:
/// its title falls back to the install folder name and `title_provisional` is set so that
/// metadata sync can replace it with the official name.
pub fn parse_app_manifest_acf(
    content: &str,
    library_root: &Path,
) -> Result<Option<DetectedGame>, String> {
    let root = parse_vdf(content)?;
    let Some(app_state) = root.get_section("AppState") else {
        return Err(format!("{} (.acf): AppState", T::InvalidManifest.s()));
    };

    let appid = app_state
        .get_str("appid")
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| format!("{} (.acf): appid", T::InvalidManifest.s()))?;

    if EXCLUDED_STEAM_APP_IDS.contains(&appid) {
        return Ok(None);
    }

    let raw_name = app_state.get_str("name").map(str::trim).unwrap_or_default();

    if raw_name.eq_ignore_ascii_case("Steamworks Common Redistributables") {
        return Ok(None);
    }

    let installdir = app_state
        .get_str("installdir")
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| format!("{} (.acf, appid {appid}): installdir", T::InvalidManifest.s()))?;

    let full_install_dir = library_root
        .join("steamapps")
        .join("common")
        .join(installdir);

    let name_is_placeholder = is_placeholder_title(raw_name) || raw_name == appid;
    let title = if name_is_placeholder {
        provisional_title_from_installdir(installdir)
    } else {
        raw_name.to_string()
    };

    let mut game = DetectedGame::new_detected(
        format!("steam:{appid}"),
        title.clone(),
        clean_game_title(&title),
        GamePlatform::Steam,
        Some(appid.to_string()),
        full_install_dir.to_string_lossy().to_string(),
        None,
        Some(format!("steam://rungameid/{appid}")),
    );
    game.title_provisional = name_is_placeholder;

    Ok(Some(game))
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
                    "{} {}: {err}",
                    T::CannotRead.s(),
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
                    "{} {}: {err}",
                    T::CannotRead.s(),
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
                    .push(format!("{} {}: {err}", T::CannotRead.s(), path.display())),
            }
        }
    }

    // Hours played and last session, straight from the Steam client's own records
    let play_stats = read_steam_play_stats(steam_root);
    for game in &mut output.games {
        let Some(stats) = game
            .platform_app_id
            .as_deref()
            .and_then(|appid| play_stats.get(appid))
        else {
            continue;
        };
        game.playtime_minutes = Some(stats.playtime_minutes);
        if stats.last_played_secs > 0 {
            game.last_played_at = Some(stats.last_played_secs.saturating_mul(1000));
        }
    }

    output
}

/// What the Steam client recorded about one app for an account.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SteamPlayStats {
    pub playtime_minutes: u32,
    /// Unix time of the last session, `0` when never played.
    pub last_played_secs: u64,
}

/// Parses a Steam `localconfig.vdf`: under `UserLocalConfigStore/Software/Valve/Steam/apps`
/// every app the account owns has `Playtime` (minutes) and `LastPlayed` (Unix seconds).
pub fn parse_local_config_play_stats(content: &str) -> HashMap<String, SteamPlayStats> {
    let Ok(root) = parse_vdf(content) else {
        return HashMap::new();
    };
    let apps = ["UserLocalConfigStore", "Software", "Valve", "Steam", "apps"]
        .into_iter()
        .try_fold(&root, |section, key| section.get_section(key));
    let Some(apps) = apps else {
        return HashMap::new();
    };

    apps.entries
        .iter()
        .filter_map(|(appid, value)| {
            let VdfValue::Section(app) = value else {
                return None;
            };
            let number = |key: &str| app.get_str(key).and_then(|raw| raw.trim().parse::<u64>().ok());
            let playtime = number("Playtime");
            let last_played = number("LastPlayed");
            if playtime.is_none() && last_played.is_none() {
                return None;
            }
            Some((
                appid.trim().to_string(),
                SteamPlayStats {
                    playtime_minutes: u32::try_from(playtime.unwrap_or(0)).unwrap_or(u32::MAX),
                    last_played_secs: last_played.unwrap_or(0),
                },
            ))
        })
        .collect()
}

/// Play statistics by appid for the Steam account used most recently on this PC.
///
/// Steam keeps them in `userdata/<account>/config/localconfig.vdf`, so they are available
/// without signing in anywhere, and they include time played before Kanso was installed or
/// outside of it. Empty when the file is missing or unreadable.
pub fn read_steam_play_stats(steam_root: &Path) -> HashMap<String, SteamPlayStats> {
    let Ok(accounts) = fs::read_dir(steam_root.join("userdata")) else {
        return HashMap::new();
    };
    let newest = accounts
        .flatten()
        .map(|account| account.path().join("config").join("localconfig.vdf"))
        .filter_map(|path| {
            let modified = fs::metadata(&path).ok()?.modified().ok()?;
            Some((modified, path))
        })
        .max_by_key(|(modified, _)| *modified);

    newest
        .and_then(|(_, path)| fs::read(path).ok())
        // Friend names and the like are not always valid UTF-8
        .map(|bytes| parse_local_config_play_stats(&String::from_utf8_lossy(&bytes)))
        .unwrap_or_default()
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
    fn placeholder_appid_name_falls_back_to_installdir_and_is_flagged_provisional() {
        // Real-world manifest written by Steam before it cached the app info
        let placeholder_acf = r#"
"AppState"
{
	"appid"		"1321440"
	"Universe"		"1"
	"name"		"appid_1321440"
	"StateFlags"		"4"
	"installdir"		"Cassette Beasts"
}
"#;
        let parsed = parse_app_manifest_acf(placeholder_acf, Path::new("/mock/SteamLibrary"))
            .expect("valid acf")
            .expect("game must still be listed");

        assert_eq!(parsed.id, "steam:1321440");
        assert_eq!(parsed.title, "Cassette Beasts");
        assert_eq!(parsed.clean_title, "Cassette Beasts");
        assert!(parsed.title_provisional);
        assert_eq!(parsed.platform_app_id.as_deref(), Some("1321440"));

        // A manifest with no `name` at all is handled the same way instead of being dropped
        let nameless_acf = r#"
"AppState"
{
	"appid"		"646570"
	"installdir"		"Slay_the_Spire"
}
"#;
        let nameless = parse_app_manifest_acf(nameless_acf, Path::new("/mock/SteamLibrary"))
            .expect("valid acf")
            .expect("game must still be listed");
        assert_eq!(nameless.title, "Slay the Spire");
        assert!(nameless.title_provisional);
    }

    #[test]
    fn regular_manifest_names_are_not_provisional() {
        let acf = r#"
"AppState"
{
	"appid"		"1091500"
	"name"		"Cyberpunk 2077"
	"installdir"		"Cyberpunk 2077"
}
"#;
        let parsed = parse_app_manifest_acf(acf, Path::new("/mock/Steam"))
            .expect("valid acf")
            .expect("listed");
        assert_eq!(parsed.title, "Cyberpunk 2077");
        assert!(!parsed.title_provisional);
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

    const LOCAL_CONFIG: &str = r#"
"UserLocalConfigStore"
{
	"friends"
	{
		"PersonaName"		"Quote \" and brace } in a name"
	}
	"Software"
	{
		"Valve"
		{
			"Steam"
			{
				"apps"
				{
					"1321440"
					{
						"LastPlayed"		"1731000000"
						"Playtime"		"754"
						"cloud"
						{
							"last_sync_state"		"synchronized"
						}
					}
					"1145360"
					{
						"LastPlayed"		"1700000000"
						"Playtime2wks"		"12"
					}
					"413150"
					{
						"cloud"
						{
							"quota_bytes"		"1000"
						}
					}
				}
			}
		}
	}
}
"#;

    #[test]
    fn reads_playtime_and_last_session_from_the_steam_local_config() {
        let stats = parse_local_config_play_stats(LOCAL_CONFIG);
        assert_eq!(
            stats.get("1321440"),
            Some(&SteamPlayStats { playtime_minutes: 754, last_played_secs: 1_731_000_000 })
        );
        // Played, but Steam wrote no total: counted as zero minutes rather than dropped
        assert_eq!(
            stats.get("1145360"),
            Some(&SteamPlayStats { playtime_minutes: 0, last_played_secs: 1_700_000_000 })
        );
        assert!(!stats.contains_key("413150"), "an app with no play data says nothing");

        // The key casing differs between Steam versions
        let lower = LOCAL_CONFIG.replace("\"Software\"", "\"software\"").replace("\"apps\"", "\"Apps\"");
        assert_eq!(parse_local_config_play_stats(&lower).len(), 2);

        assert!(parse_local_config_play_stats("not a vdf {").is_empty());
        assert!(parse_local_config_play_stats("\"Other\" { }").is_empty());
    }

    #[test]
    fn scan_attaches_the_play_stats_of_the_most_recent_steam_account() {
        let tmp = tempdir().unwrap();
        let root = tmp.path();
        let steamapps = root.join("steamapps");
        fs::create_dir_all(steamapps.join("common").join("CassetteBeasts")).unwrap();
        fs::create_dir_all(steamapps.join("common").join("Hades")).unwrap();
        let manifest = |appid: &str, name: &str, dir: &str| {
            format!("\"AppState\"\n{{\n\t\"appid\"\t\"{appid}\"\n\t\"name\"\t\"{name}\"\n\t\"installdir\"\t\"{dir}\"\n}}\n")
        };
        fs::write(steamapps.join("appmanifest_1321440.acf"), manifest("1321440", "Cassette Beasts", "CassetteBeasts")).unwrap();
        fs::write(steamapps.join("appmanifest_1145360.acf"), manifest("1145360", "Hades", "Hades")).unwrap();

        // No userdata at all: the scan still works, without play data
        let bare = scan_steam_from_root(root);
        assert_eq!(bare.games.len(), 2);
        assert!(bare.games.iter().all(|g| g.playtime_minutes.is_none() && g.last_played_at.is_none()));

        // Two accounts: an old one, and the one used last
        let old_account = root.join("userdata").join("111").join("config");
        let current_account = root.join("userdata").join("222").join("config");
        fs::create_dir_all(&old_account).unwrap();
        fs::create_dir_all(&current_account).unwrap();
        fs::write(old_account.join("localconfig.vdf"), LOCAL_CONFIG.replace("\"754\"", "\"9999\"")).unwrap();
        let old_file = fs::File::open(old_account.join("localconfig.vdf")).unwrap();
        old_file
            .set_modified(std::time::SystemTime::now() - std::time::Duration::from_secs(86_400))
            .unwrap();
        fs::write(current_account.join("localconfig.vdf"), LOCAL_CONFIG).unwrap();

        let scanned = scan_steam_from_root(root);
        let cassette = scanned.games.iter().find(|g| g.id == "steam:1321440").unwrap();
        assert_eq!(cassette.playtime_minutes, Some(754));
        assert_eq!(cassette.last_played_at, Some(1_731_000_000_000), "stored in milliseconds");
        let hades = scanned.games.iter().find(|g| g.id == "steam:1145360").unwrap();
        assert_eq!(hades.playtime_minutes, Some(0));
        assert_eq!(hades.last_played_at, Some(1_700_000_000_000));
    }
}
