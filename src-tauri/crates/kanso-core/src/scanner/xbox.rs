//! Xbox app / PC Game Pass games: `<drive>:\XboxGames\<Game>\Content\MicrosoftGame.config`.

use super::exe_pick::slugify;
use crate::cleaner::clean_game_title;
use crate::models::{DetectedGame, GamePlatform, SourceScanOutput};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

#[cfg(target_os = "windows")]
const DEFAULT_XBOX_GAMES_FOLDER: &str = "XboxGames";
/// Bootstrapper the Xbox app itself uses for desktop shortcuts; present in most recent titles.
const GAME_LAUNCH_HELPER: &str = "gamelaunchhelper.exe";

/// What Kanso needs from a `MicrosoftGame.config`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct XboxGameConfig {
    /// `<Identity Name="...">`, stable per title.
    pub identity_name: String,
    /// `<ShellVisuals DefaultDisplayName="...">`; empty when it is a resource reference.
    pub display_name: String,
    /// `<Executable Name="...">` entries, in file order.
    pub executables: Vec<String>,
}

pub fn scan_xbox_installed() -> SourceScanOutput {
    scan_xbox_from_roots(&detect_xbox_game_roots())
}

/// Xbox games folders on every fixed drive. Each drive that hosts Xbox installs has a
/// `.GamingRoot` marker naming the folder (default `XboxGames`).
#[cfg(target_os = "windows")]
pub fn detect_xbox_game_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    for drive in fixed_drive_roots() {
        let folder = fs::read(drive.join(".GamingRoot"))
            .ok()
            .and_then(|bytes| parse_gaming_root(&bytes))
            .unwrap_or_else(|| DEFAULT_XBOX_GAMES_FOLDER.to_string());
        let root = drive.join(folder);
        if root.is_dir() {
            roots.push(root);
        }
    }
    roots
}

/// Root paths (`C:\`, `D:\`, ...) of local fixed disks only. Removable drives without media,
/// optical drives and network shares are skipped: probing them can block for seconds or make
/// Windows ask to insert a disk.
#[cfg(target_os = "windows")]
fn fixed_drive_roots() -> Vec<PathBuf> {
    use windows_sys::Win32::Storage::FileSystem::{GetDriveTypeW, GetLogicalDrives};

    /// `DRIVE_FIXED` from `fileapi.h`.
    const DRIVE_FIXED: u32 = 3;

    // SAFETY: GetLogicalDrives takes no arguments and only returns a bitmask.
    let present_drives = unsafe { GetLogicalDrives() };

    (0..26u8)
        .filter(|bit| present_drives & (1u32 << bit) != 0)
        .filter_map(|bit| {
            let root = format!("{}:\\", (b'A' + bit) as char);
            let wide: Vec<u16> = root.encode_utf16().chain(std::iter::once(0)).collect();
            // SAFETY: `wide` is a valid NUL-terminated UTF-16 string that outlives the call.
            let drive_type = unsafe { GetDriveTypeW(wide.as_ptr()) };
            (drive_type == DRIVE_FIXED).then(|| PathBuf::from(root))
        })
        .collect()
}

#[cfg(not(target_os = "windows"))]
pub fn detect_xbox_game_roots() -> Vec<PathBuf> {
    Vec::new()
}

/// Decodes a `.GamingRoot` marker: `RGBX`, a 32-bit count, then the games folder (relative to
/// the drive root) as NUL-terminated UTF-16LE.
pub fn parse_gaming_root(bytes: &[u8]) -> Option<String> {
    let payload = bytes.strip_prefix(b"RGBX")?.get(4..)?;
    let units: Vec<u16> = payload
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .take_while(|unit| *unit != 0)
        .collect();
    let folder = String::from_utf16(&units).ok()?;
    let folder = folder.trim().trim_matches(['\\', '/']);
    (!folder.is_empty()).then(|| folder.to_string())
}

/// Parses the attributes Kanso needs out of a `MicrosoftGame.config` XML document.
pub fn parse_microsoft_game_config(xml: &str) -> XboxGameConfig {
    let display_name = xml_attribute(xml, "ShellVisuals", "DefaultDisplayName")
        .filter(|name| !name.to_ascii_lowercase().starts_with("ms-resource:"))
        .unwrap_or_default();

    let mut executables = Vec::new();
    let mut rest = xml;
    while let Some(pos) = rest.find("<Executable ") {
        let tag_end = rest[pos..].find('>').map_or(rest.len(), |end| pos + end);
        if let Some(name) = attribute_in_tag(&rest[pos..tag_end], "Name") {
            if !name.is_empty() {
                executables.push(name);
            }
        }
        rest = &rest[tag_end..];
    }

    XboxGameConfig {
        identity_name: xml_attribute(xml, "Identity", "Name").unwrap_or_default(),
        display_name,
        executables,
    }
}

fn xml_attribute(xml: &str, tag: &str, attribute: &str) -> Option<String> {
    let start = xml.find(&format!("<{tag} "))?;
    let end = xml[start..].find('>').map_or(xml.len(), |e| start + e);
    attribute_in_tag(&xml[start..end], attribute)
}

fn attribute_in_tag(tag: &str, attribute: &str) -> Option<String> {
    let needle = format!(" {attribute}=\"");
    let start = tag.find(&needle)? + needle.len();
    let end = tag[start..].find('"')? + start;
    Some(
        tag[start..end]
            .replace("&amp;", "&")
            .replace("&apos;", "'")
            .replace("&quot;", "\"")
            .trim()
            .to_string(),
    )
}

/// Reads a config that may be UTF-8 (with or without BOM) or UTF-16LE.
fn read_config_text(path: &Path) -> Option<String> {
    let bytes = fs::read(path).ok()?;
    if let Some(utf16) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        let units: Vec<u16> = utf16
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect();
        return String::from_utf16(&units).ok();
    }
    let text = String::from_utf8_lossy(&bytes);
    Some(text.trim_start_matches('\u{feff}').to_string())
}

/// Scans Xbox games folders (`C:\XboxGames`, ...): every subfolder holding
/// `Content\MicrosoftGame.config` is a game. It is started through `gamelaunchhelper.exe` when
/// the title ships it, otherwise through the first executable the config declares.
pub fn scan_xbox_from_roots(roots: &[PathBuf]) -> SourceScanOutput {
    let mut output = SourceScanOutput::default();
    let mut seen_ids = HashSet::new();

    for root in roots {
        let Ok(entries) = fs::read_dir(root) else {
            continue;
        };
        let mut game_dirs: Vec<PathBuf> = entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.is_dir())
            .collect();
        game_dirs.sort();

        for game_dir in game_dirs {
            let content_dir = game_dir.join("Content");
            let Some(xml) = read_config_text(&content_dir.join("MicrosoftGame.config")) else {
                continue;
            };
            let config = parse_microsoft_game_config(&xml);

            let folder_name = game_dir
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default();
            let title = if config.display_name.is_empty() {
                folder_name
            } else {
                config.display_name.as_str()
            };
            if title.is_empty() {
                continue;
            }

            let id_key = if config.identity_name.is_empty() {
                slugify(folder_name)
            } else {
                config.identity_name.clone()
            };
            let id = format!("xbox:{id_key}");
            if !seen_ids.insert(id.clone()) {
                continue;
            }

            let helper = content_dir.join(GAME_LAUNCH_HELPER);
            let exe = if helper.is_file() {
                Some(helper)
            } else {
                config
                    .executables
                    .iter()
                    .map(|name| content_dir.join(name.replace('\\', std::path::MAIN_SEPARATOR_STR)))
                    .find(|candidate| candidate.is_file())
            };

            output.games.push(DetectedGame::new_detected(
                id,
                title.to_string(),
                clean_game_title(title),
                GamePlatform::Xbox,
                (!config.identity_name.is_empty()).then(|| config.identity_name.clone()),
                game_dir.to_string_lossy().to_string(),
                exe.map(|p| p.to_string_lossy().to_string()),
                None,
            ));
        }
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    const FORZA_CONFIG: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<Game configVersion="1">
  <Identity Name="Microsoft.624F8B84B80" Publisher="CN=Microsoft Corporation" Version="3.0.0.0" />
  <ExecutableList>
    <Executable Name="ForzaHorizon5.exe" Id="Game" TargetDeviceFamily="PC" />
  </ExecutableList>
  <ShellVisuals DefaultDisplayName="Forza Horizon 5" PublisherDisplayName="Xbox Game Studios" />
</Game>"#;

    const RESOURCE_NAME_CONFIG: &str = r#"<Game configVersion="1">
  <Identity Name="Studio.SeaOfSomething" Publisher="CN=Studio" />
  <ExecutableList>
    <Executable Name="Missing.exe" Id="A" />
    <Executable Name="Binaries\Win64\Sea.exe" Id="Game" />
  </ExecutableList>
  <ShellVisuals DefaultDisplayName="ms-resource:ApplicationDisplayName" />
</Game>"#;

    #[test]
    fn parses_microsoft_game_config() {
        let config = parse_microsoft_game_config(FORZA_CONFIG);
        assert_eq!(config.identity_name, "Microsoft.624F8B84B80");
        assert_eq!(config.display_name, "Forza Horizon 5");
        assert_eq!(config.executables, vec!["ForzaHorizon5.exe"]);

        let resource = parse_microsoft_game_config(RESOURCE_NAME_CONFIG);
        assert_eq!(resource.display_name, "", "resource references are not names");
        assert_eq!(resource.executables.len(), 2);
    }

    #[test]
    fn parses_gaming_root_marker() {
        let mut bytes = b"RGBX".to_vec();
        bytes.extend_from_slice(&1u32.to_le_bytes());
        for unit in "XboxGames".encode_utf16() {
            bytes.extend_from_slice(&unit.to_le_bytes());
        }
        bytes.extend_from_slice(&[0, 0]);
        assert_eq!(parse_gaming_root(&bytes).as_deref(), Some("XboxGames"));
        assert_eq!(parse_gaming_root(b"nope"), None);
        assert_eq!(parse_gaming_root(b"RGBX\x01\x00\x00\x00\x00\x00"), None);
    }

    #[test]
    fn scans_xbox_games_folder() {
        let tmp = tempdir().unwrap();
        let root = tmp.path().join("XboxGames");

        // Recent title: ships gamelaunchhelper.exe
        let forza = root.join("Forza Horizon 5").join("Content");
        fs::create_dir_all(&forza).unwrap();
        fs::write(forza.join("MicrosoftGame.config"), FORZA_CONFIG).unwrap();
        fs::write(forza.join("ForzaHorizon5.exe"), b"game").unwrap();
        fs::write(forza.join("gamelaunchhelper.exe"), b"helper").unwrap();

        // Older title: resource display name, exe in a subfolder, UTF-16 config
        let sea = root.join("Sea of Something").join("Content");
        fs::create_dir_all(sea.join("Binaries").join("Win64")).unwrap();
        let mut utf16 = vec![0xFF, 0xFE];
        for unit in RESOURCE_NAME_CONFIG.encode_utf16() {
            utf16.extend_from_slice(&unit.to_le_bytes());
        }
        fs::write(sea.join("MicrosoftGame.config"), utf16).unwrap();
        fs::write(sea.join("Binaries").join("Win64").join("Sea.exe"), b"game").unwrap();

        // Not a game: no config
        fs::create_dir_all(root.join("GameSave")).unwrap();

        let out = scan_xbox_from_roots(&[root.clone(), tmp.path().join("missing")]);
        assert_eq!(out.games.len(), 2);

        let forza_game = &out.games[0];
        assert_eq!(forza_game.id, "xbox:Microsoft.624F8B84B80");
        assert_eq!(forza_game.title, "Forza Horizon 5");
        assert_eq!(forza_game.platform, GamePlatform::Xbox);
        assert_eq!(
            forza_game.executable_path.as_deref(),
            Some(forza.join("gamelaunchhelper.exe").to_string_lossy().as_ref())
        );

        let sea_game = &out.games[1];
        assert_eq!(sea_game.title, "Sea of Something", "falls back to the folder name");
        assert_eq!(
            sea_game.executable_path.as_deref(),
            Some(
                sea.join("Binaries")
                    .join("Win64")
                    .join("Sea.exe")
                    .to_string_lossy()
                    .as_ref()
            )
        );
    }
}
