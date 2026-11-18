use crate::cleaner::{clean_game_title, is_placeholder_title};
use crate::i18n::T;
use crate::achievements::read_achievements_from_config_dir;
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
    parse_app_manifest_for_account(content, library_root, None)
}

/// What every Steam account id is counted from: SteamID64 = this + the account's number.
const STEAM_ID64_BASE: u64 = 76_561_197_960_265_728;

/// The SteamID64 of the account whose folder `userdata/<number>/config` is.
fn steam_id64_of(config_dir: &Path) -> Option<u64> {
    let number: u64 = config_dir.parent()?.file_name()?.to_str()?.parse().ok()?;
    (number > 0).then(|| STEAM_ID64_BASE + number)
}

/// Like [`parse_app_manifest_acf`], knowing which Steam account uses this PC: a game whose
/// manifest names another account as its owner (`LastOwner`) is played through family
/// sharing. Also reads the build installed (`buildid`).
pub fn parse_app_manifest_for_account(
    content: &str,
    library_root: &Path,
    account_id64: Option<u64>,
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
    // Steam knows a build by its number: there is no version a person would write
    game.version = app_state
        .get_str("buildid")
        .map(str::trim)
        .filter(|build| !build.is_empty() && *build != "0" && build.bytes().all(|b| b.is_ascii_digit()))
        .map(|build| format!("build {build}"));
    // An owner that is not the account in use: borrowed. No owner written, or no account
    // known, says nothing
    let owner = app_state.get_str("LastOwner").and_then(|raw| raw.trim().parse::<u64>().ok());
    game.is_family_shared = matches!((owner, account_id64), (Some(owner), Some(account)) if owner != 0 && owner != account);

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
    let account_id64 = steam_account_config_dir(steam_root).as_deref().and_then(steam_id64_of);

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
                Ok(acf_content) => match parse_app_manifest_for_account(&acf_content, &lib_root, account_id64) {
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

    // Whose each game is, from the licences the client holds for the account: more to the
    // point than who last ran the installed copy. Where the licences say nothing (no cache,
    // an app they do not cover) what the manifest said stands
    if let Some(ownership) = super::steam_licenses::read_steam_ownership(steam_root) {
        for game in &mut output.games {
            let shared = game
                .platform_app_id
                .as_deref()
                .and_then(|appid| appid.trim().parse::<u64>().ok())
                .and_then(|appid| ownership.is_family_shared(appid));
            if let Some(shared) = shared {
                game.is_family_shared = shared;
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

    // Achievement counts, from the same account's library cache
    if let Some(config_dir) = steam_account_config_dir(steam_root) {
        for game in &mut output.games {
            let summary = game
                .platform_app_id
                .as_deref()
                .and_then(|appid| read_achievements_from_config_dir(&config_dir, appid));
            if let Some(achievements) = summary {
                game.achievements_unlocked = Some(achievements.unlocked);
                game.achievements_total = Some(achievements.total);
            }
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

/// `userdata/<account>/config` of the Steam account used most recently on this PC (the one
/// whose `localconfig.vdf` changed last), where the client keeps what it knows about the
/// account's games.
pub fn steam_account_config_dir(steam_root: &Path) -> Option<PathBuf> {
    fs::read_dir(steam_root.join("userdata"))
        .ok()?
        .flatten()
        .map(|account| account.path().join("config"))
        .filter_map(|config_dir| {
            let modified = fs::metadata(config_dir.join("localconfig.vdf"))
                .ok()?
                .modified()
                .ok()?;
            Some((modified, config_dir))
        })
        .max_by_key(|(modified, _)| *modified)
        .map(|(_, config_dir)| config_dir)
}

/// Play statistics by appid for the Steam account used most recently on this PC.
///
/// Steam keeps them in `userdata/<account>/config/localconfig.vdf`, so they are available
/// without signing in anywhere, and they include time played before Kanso was installed or
/// outside of it. Empty when the file is missing or unreadable.
pub fn read_steam_play_stats(steam_root: &Path) -> HashMap<String, SteamPlayStats> {
    steam_account_config_dir(steam_root)
        .and_then(|config_dir| fs::read(config_dir.join("localconfig.vdf")).ok())
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
    fn a_manifest_says_the_build_installed_and_who_owns_the_game() {
        let manifest = |extra: &str| {
            format!("\"AppState\"\n{{\n\t\"appid\"\t\"620\"\n\t\"name\"\t\"Portal 2\"\n\t\"installdir\"\t\"Portal 2\"\n{extra}}}\n")
        };
        let me = STEAM_ID64_BASE + 123_456;
        let parse = |extra: &str, account: Option<u64>| {
            parse_app_manifest_for_account(&manifest(extra), Path::new("/steam"), account).unwrap().unwrap()
        };

        let mine = parse(&format!("\t\"buildid\"\t\"15738292\"\n\t\"LastOwner\"\t\"{me}\"\n"), Some(me));
        assert_eq!(mine.version.as_deref(), Some("build 15738292"));
        assert!(!mine.is_family_shared, "my own game");
        let borrowed = parse(&format!("\t\"LastOwner\"\t\"{}\"\n", me + 77), Some(me));
        assert!(borrowed.is_family_shared, "another account owns it: family sharing");
        assert_eq!(borrowed.version, None, "no build written, no version");
        // Nothing to go by: no owner written, owner zero, or no account known on this PC
        assert!(!parse("", Some(me)).is_family_shared);
        assert!(!parse("\t\"LastOwner\"\t\"0\"\n\t\"buildid\"\t\"0\"\n", Some(me)).is_family_shared);
        assert!(!parse(&format!("\t\"LastOwner\"\t\"{}\"\n", me + 77), None).is_family_shared);
        assert_eq!(parse("\t\"buildid\"\t\"12 or so\"\n", Some(me)).version, None);

        // The account is the one whose folder Steam keeps under userdata
        assert_eq!(steam_id64_of(Path::new("/steam/userdata/123456/config")), Some(me));
        assert_eq!(steam_id64_of(Path::new("/steam/userdata/anonymous/config")), None);
        assert_eq!(steam_id64_of(Path::new("/steam/userdata/0/config")), None);
    }

    #[test]
    fn the_scan_takes_whose_a_game_is_from_the_clients_licences() {
        use crate::scanner::steam_licenses::tests::license_cache;
        use crate::scanner::steam_packages::tests::PackageFile;

        let tmp = tempdir().unwrap();
        let root = tmp.path();
        let steamapps = root.join("steamapps");
        let me = 222u32;
        let my_id64 = STEAM_ID64_BASE + u64::from(me);
        let sister_id64 = STEAM_ID64_BASE + 333;
        let manifest = |appid: &str, owner: u64| {
            format!("\"AppState\"\n{{\n\t\"appid\"\t\"{appid}\"\n\t\"name\"\t\"Game {appid}\"\n\t\"installdir\"\t\"Game{appid}\"\n\t\"LastOwner\"\t\"{owner}\"\n}}\n")
        };
        for (appid, owner) in [("10", sister_id64), ("20", my_id64), ("30", sister_id64), ("40", my_id64)] {
            fs::create_dir_all(steamapps.join("common").join(format!("Game{appid}"))).unwrap();
            fs::write(steamapps.join(format!("appmanifest_{appid}.acf")), manifest(appid, owner)).unwrap();
        }
        let config = root.join("userdata").join(me.to_string()).join("config");
        fs::create_dir_all(&config).unwrap();
        fs::write(config.join("localconfig.vdf"), LOCAL_CONFIG).unwrap();
        let shared = |scan: &SourceScanOutput, appid: &str| {
            scan.games.iter().find(|g| g.id == format!("steam:{appid}")).unwrap().is_family_shared
        };

        // Without the licences: who the manifest names as the owner of the installed copy
        let by_manifest = scan_steam_from_root(root);
        assert_eq!(["10", "20", "30", "40"].map(|appid| shared(&by_manifest, appid)), [true, false, true, false]);

        // With them: app 10 is mine although my sister installed it, app 20 is hers although
        // I ran it last; app 30 and 40 are in no licence, and the manifest stands
        fs::create_dir_all(root.join("appcache")).unwrap();
        let packages = PackageFile::new(true).package(1, 1, &[], &[10]).package(2, 1, &[], &[20]);
        fs::write(root.join("appcache").join("packageinfo.vdf"), packages.finish()).unwrap();
        fs::write(config.join("licensecache"), license_cache(me, &[(1, me), (2, 333)])).unwrap();
        let by_licences = scan_steam_from_root(root);
        assert_eq!(["10", "20", "30", "40"].map(|appid| shared(&by_licences, appid)), [false, true, true, false]);

        // A licence cache that does not decode is as good as none
        fs::write(config.join("licensecache"), b"not a licence cache at all").unwrap();
        let again = scan_steam_from_root(root);
        assert_eq!(["10", "20", "30", "40"].map(|appid| shared(&again, appid)), [true, false, true, false]);
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
        // Opened for writing: Windows refuses to change the times of a read-only handle
        let old_file = fs::OpenOptions::new()
            .write(true)
            .open(old_account.join("localconfig.vdf"))
            .unwrap();
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
        assert!(scanned.games.iter().all(|g| g.achievements_total.is_none()), "no cache files yet");

        // Achievement counts come from the same account's library cache, never the old one's
        let cache = |unlocked: u32| {
            format!(r#"[["achievements",{{"version":2,"data":{{"vecHighlight":[],"vecUnachieved":[],"nTotal":128,"nAchieved":{unlocked}}}}}]]"#)
        };
        fs::create_dir_all(old_account.join("librarycache")).unwrap();
        fs::write(old_account.join("librarycache").join("1321440.json"), cache(99)).unwrap();
        fs::create_dir_all(current_account.join("librarycache")).unwrap();
        fs::write(current_account.join("librarycache").join("1321440.json"), cache(31)).unwrap();
        fs::write(current_account.join("librarycache").join("1145360.json"), "not json").unwrap();

        let scanned = scan_steam_from_root(root);
        let cassette = scanned.games.iter().find(|g| g.id == "steam:1321440").unwrap();
        assert_eq!((cassette.achievements_unlocked, cassette.achievements_total), (Some(31), Some(128)));
        let hades = scanned.games.iter().find(|g| g.id == "steam:1145360").unwrap();
        assert_eq!(hades.achievements_total, None, "an unreadable cache file is ignored");
        assert_eq!(steam_account_config_dir(root), Some(current_account));
    }
}
