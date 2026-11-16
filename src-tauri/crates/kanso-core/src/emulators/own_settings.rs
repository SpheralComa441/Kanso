//! Where an emulator keeps its games, as its own settings say.
//!
//! Most emulators have a "games folders" list in a settings file, next to the executable
//! (portable installs) or in the user's profile. Kanso reads that list and nothing else: it
//! never writes to an emulator's files. File names, sections and keys were checked against
//! each emulator's source code; see `docs/EMULATORS.md` for the table.

use std::fs;
use std::path::{Path, PathBuf};

/// The folders of the user's profile that emulators keep their settings in.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Places {
    /// `%APPDATA%` (Roaming).
    pub appdata: Option<PathBuf>,
    /// `%LOCALAPPDATA%`.
    pub local_appdata: Option<PathBuf>,
    /// The user's Documents folder; more than one where OneDrive may have moved it.
    pub documents: Vec<PathBuf>,
    /// `Program Files`, `Program Files (x86)`, per-user programs: where installers put things.
    pub program_dirs: Vec<PathBuf>,
}

impl Places {
    /// Those of the user running Kanso, from the environment.
    pub fn of_this_pc() -> Self {
        let var = |name: &str| std::env::var_os(name).map(PathBuf::from).filter(|p| !p.as_os_str().is_empty());
        let profile = var("USERPROFILE");
        let local_appdata = var("LOCALAPPDATA");
        let mut documents = Vec::new();
        if let Some(profile) = &profile {
            documents.push(profile.join("Documents"));
            documents.push(profile.join("OneDrive").join("Documents"));
        }
        let mut program_dirs: Vec<PathBuf> =
            ["ProgramFiles", "ProgramFiles(x86)", "ProgramW6432"].iter().filter_map(|name| var(name)).collect();
        if let Some(local) = &local_appdata {
            program_dirs.push(local.join("Programs"));
        }
        if let Some(drive) = var("SystemDrive") {
            // "C:" alone is the current folder of that drive: the root needs its separator
            program_dirs.push(PathBuf::from(format!("{}\\", drive.to_string_lossy().trim_end_matches('\\'))));
        }
        program_dirs.dedup();
        Self {
            appdata: var("APPDATA"),
            local_appdata,
            documents,
            program_dirs,
        }
    }

    fn first_documents(&self, folder: &str) -> Option<PathBuf> {
        self.documents.iter().map(|documents| documents.join(folder)).find(|path| path.is_dir())
    }
}

/// One `key = value` line of a settings file, with the section it is in.
struct Entry {
    section: String,
    key: String,
    value: String,
}

/// Reads an INI-like file (INI, `.cfg`, the flat part of a TOML file): sections in square
/// brackets, `key = value` lines, repeated keys kept. Values lose their quotes.
fn read_entries(path: &Path) -> Vec<Entry> {
    let Ok(bytes) = fs::read(path) else {
        return Vec::new();
    };
    // Some emulators write a byte-order mark; none of these files is large
    let text = String::from_utf8_lossy(&bytes);
    let text = text.trim_start_matches('\u{feff}');
    let mut section = String::new();
    let mut entries = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            continue;
        }
        if let Some(name) = line.strip_prefix('[').and_then(|rest| rest.strip_suffix(']')) {
            section = name.trim().to_string();
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim();
        let value = ['"', '\'']
            .iter()
            .find_map(|quote| value.strip_prefix(*quote)?.strip_suffix(*quote))
            .unwrap_or(value);
        entries.push(Entry {
            section: section.clone(),
            key: key.trim().to_string(),
            value: value.to_string(),
        });
    }
    entries
}

fn values_of<'a>(entries: &'a [Entry], section: &str, key: impl Fn(&str) -> bool + 'a) -> impl Iterator<Item = &'a str> + 'a {
    let section = section.to_string();
    entries
        .iter()
        .filter(move |entry| entry.section.eq_ignore_ascii_case(&section) && key(&entry.key))
        .map(|entry| entry.value.as_str())
}

/// The first of `candidates` that is a file.
fn first_file(candidates: impl IntoIterator<Item = PathBuf>) -> Option<PathBuf> {
    candidates.into_iter().find(|path| path.is_file())
}

/// `user/` next to the executable when it exists (a portable install), else the emulator's
/// folder in `%APPDATA%`: how the Switch and 3DS emulators of the yuzu and Citra families
/// decide where their settings are.
fn qt_config(exe_dir: &Path, places: &Places, appdata_names: &[&str]) -> Option<PathBuf> {
    let portable = exe_dir.join("user");
    let mut roots = Vec::new();
    if portable.is_dir() {
        roots.push(portable);
    } else if let Some(appdata) = &places.appdata {
        roots.extend(appdata_names.iter().map(|name| appdata.join(name)));
    }
    first_file(roots.into_iter().map(|root| root.join("config").join("qt-config.ini")))
}

/// Game folders from a Qt settings file: `[UI]`, `Paths\gamedirs\<n>\path`. The first entries
/// are the emulated console's own storage (`SDMC`, `UserNAND`, `SysNAND`), not folders.
fn qt_game_dirs(config: &Path) -> Vec<String> {
    let entries = read_entries(config);
    values_of(&entries, "UI", |key| {
        let key = key.to_ascii_lowercase();
        key.starts_with("paths\\gamedirs\\") && key.ends_with("\\path")
    })
    .filter(|value| !["SDMC", "UserNAND", "SysNAND", "INSTALLED", "SYSTEM"].contains(value))
    // Qt writes a backslash as two
    .map(|value| value.replace("\\\\", "\\"))
    .collect()
}

/// The text between `<tag>` and `</tag>`, for each occurrence inside `text`.
fn xml_elements<'a>(text: &'a str, tag: &str) -> Vec<&'a str> {
    let (open, close) = (format!("<{tag}>"), format!("</{tag}>"));
    let mut found = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find(&open) {
        let after = &rest[start + open.len()..];
        let Some(end) = after.find(&close) else {
            break;
        };
        found.push(&after[..end]);
        rest = &after[end + close.len()..];
    }
    found
}

/// The folders the emulator `id`, installed in `exe_dir`, lists as holding its games.
/// Only folders that exist are returned, each once; a path relative to the emulator is
/// resolved against its folder. An emulator without such a list gives none.
pub fn game_folders(id: &str, exe_dir: &Path, places: &Places) -> Vec<PathBuf> {
    let raw: Vec<String> = match id {
        "dolphin" => {
            // portable.txt next to the executable, an old Documents folder, or %APPDATA%
            let user_dir = if exe_dir.join("portable.txt").is_file() {
                Some(exe_dir.join("User"))
            } else {
                places
                    .first_documents("Dolphin Emulator")
                    .or_else(|| places.appdata.as_ref().map(|appdata| appdata.join("Dolphin Emulator")))
            };
            let entries = user_dir
                .map(|dir| read_entries(&dir.join("Config").join("Dolphin.ini")))
                .unwrap_or_default();
            values_of(&entries, "General", |key| {
                key.to_ascii_lowercase()
                    .strip_prefix("isopath")
                    .is_some_and(|rest| !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_digit()))
            })
            .map(str::to_string)
            .collect()
        }
        "pcsx2" | "duckstation" => {
            let config = if id == "pcsx2" {
                let portable = exe_dir.join("portable.ini").is_file() || exe_dir.join("portable.txt").is_file();
                let root = if portable { Some(exe_dir.to_path_buf()) } else { places.first_documents("PCSX2") };
                root.map(|root| root.join("inis").join("PCSX2.ini"))
            } else {
                let portable = exe_dir.join("portable.txt").is_file() || exe_dir.join("settings.ini").is_file();
                let root = if portable {
                    Some(exe_dir.to_path_buf())
                } else {
                    places
                        .first_documents("DuckStation")
                        .or_else(|| places.local_appdata.as_ref().map(|local| local.join("DuckStation")))
                };
                root.map(|root| root.join("settings.ini"))
            };
            let entries = config.map(|path| read_entries(&path)).unwrap_or_default();
            values_of(&entries, "GameList", |key| {
                key.eq_ignore_ascii_case("Paths") || key.eq_ignore_ascii_case("RecursivePaths")
            })
            .map(str::to_string)
            .collect()
        }
        "ppsspp" => {
            // Without installed.txt the memory stick is next to the executable; with it, it is
            // where that file says, or in Documents
            let marker = exe_dir.join("installed.txt");
            let memstick = if marker.is_file() {
                let written = fs::read_to_string(&marker).unwrap_or_default();
                let written = written.trim().trim_start_matches('\u{feff}');
                if written.is_empty() {
                    places.first_documents("PPSSPP")
                } else {
                    Some(PathBuf::from(written))
                }
            } else {
                Some(exe_dir.join("memstick"))
            };
            let entries = memstick
                .map(|dir| read_entries(&dir.join("PSP").join("SYSTEM").join("ppsspp.ini")))
                .unwrap_or_default();
            let pinned = values_of(&entries, "PinnedPaths", |key| key.to_ascii_lowercase().starts_with("path"));
            let current = values_of(&entries, "General", |key| key.eq_ignore_ascii_case("CurrentDirectory"));
            pinned.chain(current).map(str::to_string).collect()
        }
        "cemu" => {
            let portable = exe_dir.join("portable");
            let config = if portable.is_dir() {
                Some(portable.join("settings.xml"))
            } else {
                first_file(
                    [Some(exe_dir.join("settings.xml")), places.appdata.as_ref().map(|a| a.join("Cemu").join("settings.xml"))]
                        .into_iter()
                        .flatten(),
                )
            };
            let text = config.and_then(|path| fs::read_to_string(path).ok()).unwrap_or_default();
            xml_elements(&text, "GamePaths")
                .into_iter()
                .flat_map(|block| xml_elements(block, "Entry"))
                .map(|entry| {
                    entry
                        .trim()
                        .replace("&amp;", "&")
                        .replace("&apos;", "'")
                        .replace("&quot;", "\"")
                        .replace("&lt;", "<")
                        .replace("&gt;", ">")
                })
                .collect()
        }
        "ryujinx" => {
            let portable = exe_dir.join("portable");
            let config = first_file(
                [
                    portable.is_dir().then(|| portable.join("Config.json")),
                    Some(exe_dir.join("Config.json")),
                    places.appdata.as_ref().map(|appdata| appdata.join("Ryujinx").join("Config.json")),
                ]
                .into_iter()
                .flatten(),
            );
            config
                .and_then(|path| fs::read_to_string(path).ok())
                .and_then(|text| serde_json::from_str::<serde_json::Value>(text.trim_start_matches('\u{feff}')).ok())
                .and_then(|json| json.get("game_dirs").and_then(|dirs| dirs.as_array().cloned()))
                .unwrap_or_default()
                .iter()
                .filter_map(|dir| dir.as_str().map(str::to_string))
                .collect()
        }
        "yuzu" | "suyu" | "eden" | "citron" | "sudachi" => qt_config(exe_dir, places, &[id])
            .map(|config| qt_game_dirs(&config))
            .unwrap_or_default(),
        "azahar" | "lime3ds" | "citra" => {
            let names: &[&str] = match id {
                "azahar" => &["Azahar", "Lime3DS", "Citra"],
                "lime3ds" => &["Lime3DS", "Citra"],
                _ => &["Citra"],
            };
            qt_config(exe_dir, places, names).map(|config| qt_game_dirs(&config)).unwrap_or_default()
        }
        "xemu" => {
            let config = first_file(
                [Some(exe_dir.join("xemu.toml")), places.appdata.as_ref().map(|a| a.join("xemu").join("xemu").join("xemu.toml"))]
                    .into_iter()
                    .flatten(),
            );
            let entries = config.map(|path| read_entries(&path)).unwrap_or_default();
            values_of(&entries, "general", |key| key == "games_dir").map(str::to_string).collect()
        }
        "snes9x" => {
            let entries = read_entries(&exe_dir.join("snes9x.conf"));
            values_of(&entries, "Settings\\Win\\Files", |key| key.eq_ignore_ascii_case("Dir:Roms"))
                .map(str::to_string)
                .collect()
        }
        "flycast" => {
            let entries = read_entries(&exe_dir.join("emu.cfg"));
            values_of(&entries, "config", |key| key.eq_ignore_ascii_case("Dreamcast.ContentPath"))
                .flat_map(|list| list.split(';'))
                .map(str::to_string)
                .collect()
        }
        _ => Vec::new(),
    };

    let mut folders: Vec<PathBuf> = Vec::new();
    for text in raw {
        let text = text.trim();
        if text.is_empty() {
            continue;
        }
        let path = PathBuf::from(text);
        let path = if path.is_relative() { exe_dir.join(path) } else { path };
        // A whole drive is where someone last browsed, not a games folder
        if path.parent().is_none() || !path.is_dir() {
            continue;
        }
        if !folders.contains(&path) {
            folders.push(path);
        }
    }
    folders
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A scratch folder for one test, with the profile folders emulators look into.
    pub(crate) struct Scratch {
        pub root: PathBuf,
    }

    impl Scratch {
        pub(crate) fn new(name: &str) -> Self {
            let root = std::env::temp_dir().join(format!("kanso-emu-{name}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&root);
            fs::create_dir_all(&root).unwrap();
            Self { root }
        }

        pub(crate) fn places(&self) -> Places {
            Places {
                appdata: Some(self.root.join("AppData").join("Roaming")),
                local_appdata: Some(self.root.join("AppData").join("Local")),
                documents: vec![self.root.join("Documents")],
                program_dirs: vec![self.root.join("Program Files")],
            }
        }

        pub(crate) fn dir(&self, relative: &str) -> PathBuf {
            let path = self.root.join(relative);
            fs::create_dir_all(&path).unwrap();
            path
        }

        pub(crate) fn file(&self, relative: &str, contents: &str) -> PathBuf {
            let path = self.root.join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, contents).unwrap();
            path
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    fn names(folders: &[PathBuf], root: &Path) -> Vec<String> {
        folders
            .iter()
            .map(|folder| folder.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/"))
            .collect()
    }

    #[test]
    fn reads_the_games_folders_each_emulator_lists() {
        let s = Scratch::new("settings");
        let places = s.places();
        for folder in ["Roms/GC", "Roms/Wii", "Roms/PS2", "Roms/PS2b", "Roms/PSX", "Roms/PSP", "Roms/WiiU", "Roms/Switch", "Roms/3DS", "Roms/Xbox", "Roms/DC", "Roms/Naomi"] {
            s.dir(folder);
        }
        let at = |name: &str| s.root.join(name).to_string_lossy().to_string();

        // Dolphin, settings in %APPDATA%: numbered keys, one of them a folder that is gone
        let dolphin = s.dir("Emu/Dolphin");
        s.file(
            "AppData/Roaming/Dolphin Emulator/Config/Dolphin.ini",
            &format!("[General]\nISOPaths = 3\nISOPath0 = {}\nISOPath1 = {}\nISOPath2 = {}\nRecursiveISOPaths = True\n[Interface]\nISOPath9 = {}\n", at("Roms/GC"), at("Roms/Wii"), at("Roms/Gone"), at("Roms/PS2")),
        );
        assert_eq!(names(&game_folders("dolphin", &dolphin, &places), &s.root), ["Roms/GC", "Roms/Wii"]);
        // A portable Dolphin reads its own User folder, not the profile's
        s.file("Emu/Dolphin/portable.txt", "");
        s.file("Emu/Dolphin/User/Config/Dolphin.ini", &format!("[General]\nISOPath0 = {}\n", at("Roms/Wii")));
        assert_eq!(names(&game_folders("dolphin", &dolphin, &places), &s.root), ["Roms/Wii"]);

        // PCSX2: repeated keys, in Documents; portable when portable.ini is there
        let pcsx2 = s.dir("Emu/PCSX2");
        s.file("Documents/PCSX2/inis/PCSX2.ini", &format!("[UI]\nTheme = dark\n[GameList]\nRecursivePaths = {}\nPaths = {}\nExcludedPaths = {}\n", at("Roms/PS2"), at("Roms/PS2b"), at("Roms/PSX")));
        assert_eq!(names(&game_folders("pcsx2", &pcsx2, &places), &s.root), ["Roms/PS2", "Roms/PS2b"]);

        // DuckStation: next to the executable when settings.ini is there, else %LOCALAPPDATA%
        let duck = s.dir("Emu/Duck");
        s.file("AppData/Local/DuckStation/settings.ini", &format!("[GameList]\nRecursivePaths = {}\n", at("Roms/PSX")));
        assert_eq!(names(&game_folders("duckstation", &duck, &places), &s.root), ["Roms/PSX"]);
        s.file("Emu/Duck/settings.ini", &format!("[GameList]\nRecursivePaths = {}\n", at("Roms/PS2")));
        assert_eq!(names(&game_folders("duckstation", &duck, &places), &s.root), ["Roms/PS2"]);

        // PPSSPP: pinned folders and the last one browsed, on its memory stick
        let ppsspp = s.dir("Emu/PPSSPP");
        s.file("Emu/PPSSPP/memstick/PSP/SYSTEM/ppsspp.ini", &format!("\u{feff}[General]\nCurrentDirectory = {}\n[PinnedPaths]\nPath0 = {}\n", at("Roms/PSP"), at("Roms/PSP")));
        assert_eq!(names(&game_folders("ppsspp", &ppsspp, &places), &s.root), ["Roms/PSP"]);

        // Cemu: XML, with an escaped ampersand
        let cemu = s.dir("Emu/Cemu");
        s.dir("Roms/Wii U & more");
        s.file("AppData/Roaming/Cemu/settings.xml", &format!("<?xml version=\"1.0\"?>\n<content>\n  <GamePaths>\n    <Entry>{}</Entry>\n    <Entry>{}</Entry>\n  </GamePaths>\n  <GameCache><Entry>x</Entry></GameCache>\n</content>", at("Roms/WiiU"), at("Roms/Wii U &amp; more")));
        assert_eq!(names(&game_folders("cemu", &cemu, &places), &s.root), ["Roms/WiiU", "Roms/Wii U & more"]);

        // Ryujinx: JSON
        let ryujinx = s.dir("Emu/Ryujinx");
        s.file("AppData/Roaming/Ryujinx/Config.json", &serde_json::json!({"version": 60, "game_dirs": [at("Roms/Switch"), 7], "autoload_dirs": []}).to_string());
        assert_eq!(names(&game_folders("ryujinx", &ryujinx, &places), &s.root), ["Roms/Switch"]);

        // The yuzu family: Qt settings, the console's own storage skipped, backslashes doubled
        let eden = s.dir("Emu/Eden");
        let doubled = at("Roms/Switch").replace('\\', "\\\\");
        s.file("AppData/Roaming/eden/config/qt-config.ini", &format!("[UI]\nPaths\\gamedirs\\size=4\nPaths\\gamedirs\\1\\path=SDMC\nPaths\\gamedirs\\2\\path=UserNAND\nPaths\\gamedirs\\3\\path=SysNAND\nPaths\\gamedirs\\4\\path={doubled}\nPaths\\gamedirs\\4\\deep_scan=true\n"));
        assert_eq!(names(&game_folders("eden", &eden, &places), &s.root), ["Roms/Switch"]);
        assert!(game_folders("yuzu", &eden, &places).is_empty(), "each fork has its own folder");

        // Azahar also finds what Citra left behind; a portable install wins over the profile
        let azahar = s.dir("Emu/Azahar");
        s.file("AppData/Roaming/Citra/config/qt-config.ini", &format!("[UI]\nPaths\\gamedirs\\1\\path=INSTALLED\nPaths\\gamedirs\\2\\path={}\n", at("Roms/3DS")));
        assert_eq!(names(&game_folders("azahar", &azahar, &places), &s.root), ["Roms/3DS"]);
        s.file("Emu/Azahar/user/config/qt-config.ini", "[UI]\n");
        assert!(game_folders("azahar", &azahar, &places).is_empty());

        // xemu: TOML, single-quoted
        let xemu = s.dir("Emu/xemu");
        s.file("Emu/xemu/xemu.toml", &format!("[general]\ngames_dir = '{}'\n[sys.files]\ndvd_path = 'x.iso'\n", at("Roms/Xbox")));
        assert_eq!(names(&game_folders("xemu", &xemu, &places), &s.root), ["Roms/Xbox"]);

        // Snes9x: a folder relative to the emulator
        let snes9x = s.dir("Emu/Snes9x");
        s.dir("Emu/Snes9x/Roms");
        s.file("Emu/Snes9x/snes9x.conf", "[Settings\\Win\\Files]\nDir:Roms = .\\Roms\nRom:RecentGame1 = x.sfc\n".replace(".\\Roms", "Roms").as_str());
        assert_eq!(names(&game_folders("snes9x", &snes9x, &places), &s.root), ["Emu/Snes9x/Roms"]);

        // Flycast: a list separated by semicolons
        let flycast = s.dir("Emu/Flycast");
        s.file("Emu/Flycast/emu.cfg", &format!("[config]\nDreamcast.ContentPath = {};{}\n", at("Roms/DC"), at("Roms/Naomi")));
        assert_eq!(names(&game_folders("flycast", &flycast, &places), &s.root), ["Roms/DC", "Roms/Naomi"]);

        // Emulators without a list, and ones Kanso does not know, give nothing
        assert!(game_folders("melonds", &s.root, &places).is_empty());
        assert!(game_folders("custom-thing", &s.root, &places).is_empty());
    }

    #[test]
    fn a_whole_drive_is_not_a_games_folder() {
        let s = Scratch::new("root");
        let root = if cfg!(windows) { "C:\\" } else { "/" };
        s.file("Emu/Flycast/emu.cfg", &format!("[config]\nDreamcast.ContentPath = {root}\n"));
        assert!(game_folders("flycast", &s.root.join("Emu/Flycast"), &s.places()).is_empty());
    }
}
