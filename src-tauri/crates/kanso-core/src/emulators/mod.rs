//! Emulators and the game files they run.
//!
//! Kanso knows a number of emulators by the name of their executable: for those it knows
//! which files are games, how to start one, and where the emulator's own settings list its
//! games folders. An emulator it does not know can be added in Settings with its executable,
//! the extensions of its games and a folder.
//!
//! What Kanso does with an emulator is start it with a game file as its argument. It reads
//! the emulator's settings and never writes them; it downloads no games, BIOS files or cores,
//! and contacts no site on behalf of this module.
//!
//! Emulated games are library entries like any other (`platform: emulator`,
//! `platform_app_id` naming the emulator), so covers, favourites, status and the sheet work
//! as for store games; the interface keeps them in their own section.

pub mod find;
pub mod own_settings;
pub mod roms;
pub mod special;

use crate::cleaner::clean_game_title;
use crate::config::EmulatorSetting;
use crate::i18n::T;
use crate::models::{DetectedGame, GamePlatform, SourceScanOutput};
use own_settings::Places;
use roms::{find_rom_files, RomFile};
use serde::Serialize;
use std::path::{Path, PathBuf};

/// How an emulator's games are found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Library {
    /// Files with known extensions, in folders.
    Files,
    /// RetroArch: the entries of its playlists, each with the core that runs it.
    RetroArchPlaylists,
    /// RPCS3: the folders its list of games names.
    Rpcs3Games,
    /// PlayStation 4: the folders that hold an `eboot.bin`, in the games folders.
    Ps4Folders,
}

/// An emulator Kanso knows.
pub struct EmulatorDef {
    pub id: &'static str,
    pub name: &'static str,
    /// What it emulates, as shown on its tile.
    pub systems: &'static str,
    /// File names of its executable, lower case; one ending in `*` matches by prefix. When
    /// several are on this PC, the one named first here is the one used.
    pub exe_names: &'static [&'static str],
    /// Folder names installers and guides put it in, looked for where programs are installed.
    pub install_dirs: &'static [&'static str],
    /// Extensions of the game files it opens, lower case, without the dot.
    pub extensions: &'static [&'static str],
    /// The arguments that start a game: `{rom}` is the game file, `{core}` RetroArch's core.
    pub args: &'static [&'static str],
    pub library: Library,
}

const SWITCH: &[&str] = &["xci", "nsp", "nca", "nro", "nso"];
const N3DS: &[&str] = &["3ds", "3dsx", "cci", "zcci", "cxi", "cia"];

/// The emulators Kanso knows. Executable names, arguments and extensions are the ones their
/// own sources, Playnite and EmulationStation-DE use (see `docs/EMULATORS.md`).
pub const KNOWN: &[EmulatorDef] = &[
    EmulatorDef {
        id: "retroarch",
        name: "RetroArch",
        systems: "Multi-system",
        exe_names: &["retroarch.exe"],
        install_dirs: &["RetroArch-Win64", "RetroArch"],
        extensions: &[],
        args: &["-L", "{core}", "{rom}"],
        library: Library::RetroArchPlaylists,
    },
    EmulatorDef {
        id: "dolphin",
        name: "Dolphin",
        systems: "GameCube · Wii",
        exe_names: &["dolphin.exe"],
        install_dirs: &["Dolphin", "Dolphin-x64"],
        extensions: &["iso", "gcm", "gcz", "ciso", "wbfs", "rvz", "wia", "wad", "tgc", "m3u"],
        args: &["-b", "-e", "{rom}"],
        library: Library::Files,
    },
    EmulatorDef {
        id: "pcsx2",
        name: "PCSX2",
        systems: "PlayStation 2",
        exe_names: &["pcsx2-qt*"],
        install_dirs: &["PCSX2"],
        extensions: &["iso", "chd", "cso", "zso", "gz", "bin", "img", "mdf", "m3u"],
        args: &["-batch", "--", "{rom}"],
        library: Library::Files,
    },
    EmulatorDef {
        id: "duckstation",
        name: "DuckStation",
        systems: "PlayStation",
        exe_names: &["duckstation-qt*", "duckstation-nogui*"],
        install_dirs: &["DuckStation"],
        extensions: &["cue", "chd", "pbp", "iso", "img", "bin", "ecm", "mds", "ccd", "m3u"],
        args: &["-batch", "--", "{rom}"],
        library: Library::Files,
    },
    EmulatorDef {
        id: "rpcs3",
        name: "RPCS3",
        systems: "PlayStation 3",
        exe_names: &["rpcs3.exe"],
        install_dirs: &["RPCS3"],
        extensions: &[],
        args: &["--no-gui", "{rom}"],
        library: Library::Rpcs3Games,
    },
    EmulatorDef {
        id: "shadps4",
        name: "shadPS4",
        systems: "PlayStation 4",
        // Its launcher, which has a window, before the emulator proper, which has none
        exe_names: &["shadps4qtlauncher.exe", "shadps4.exe"],
        install_dirs: &["shadPS4", "shadPS4QtLauncher"],
        extensions: &[],
        args: &["-g", "{rom}"],
        library: Library::Ps4Folders,
    },
    EmulatorDef {
        id: "ppsspp",
        name: "PPSSPP",
        systems: "PSP",
        exe_names: &["ppssppwindows64.exe", "ppssppwindows.exe"],
        install_dirs: &["PPSSPP"],
        extensions: &["iso", "cso", "pbp", "chd"],
        args: &["{rom}"],
        library: Library::Files,
    },
    EmulatorDef {
        id: "cemu",
        name: "Cemu",
        systems: "Wii U",
        exe_names: &["cemu.exe"],
        install_dirs: &["Cemu"],
        extensions: &["wua", "wud", "wux", "rpx"],
        args: &["-g", "{rom}"],
        library: Library::Files,
    },
    EmulatorDef {
        id: "ryujinx",
        name: "Ryujinx",
        systems: "Nintendo Switch",
        exe_names: &["ryujinx.exe", "ryujinx.ava.exe"],
        install_dirs: &["Ryujinx"],
        extensions: SWITCH,
        args: &["{rom}"],
        library: Library::Files,
    },
    EmulatorDef {
        id: "yuzu",
        name: "yuzu",
        systems: "Nintendo Switch",
        exe_names: &["yuzu.exe"],
        install_dirs: &["yuzu"],
        extensions: SWITCH,
        args: &["-g", "{rom}"],
        library: Library::Files,
    },
    EmulatorDef {
        id: "suyu",
        name: "suyu",
        systems: "Nintendo Switch",
        exe_names: &["suyu.exe"],
        install_dirs: &["suyu"],
        extensions: SWITCH,
        args: &["-g", "{rom}"],
        library: Library::Files,
    },
    EmulatorDef {
        id: "eden",
        name: "Eden",
        systems: "Nintendo Switch",
        exe_names: &["eden.exe"],
        install_dirs: &["Eden"],
        extensions: SWITCH,
        args: &["-g", "{rom}"],
        library: Library::Files,
    },
    EmulatorDef {
        id: "citron",
        name: "Citron",
        systems: "Nintendo Switch",
        exe_names: &["citron.exe"],
        install_dirs: &["Citron"],
        extensions: SWITCH,
        args: &["-g", "{rom}"],
        library: Library::Files,
    },
    EmulatorDef {
        id: "sudachi",
        name: "Sudachi",
        systems: "Nintendo Switch",
        exe_names: &["sudachi.exe"],
        install_dirs: &["Sudachi"],
        extensions: SWITCH,
        args: &["-g", "{rom}"],
        library: Library::Files,
    },
    EmulatorDef {
        id: "azahar",
        name: "Azahar",
        systems: "Nintendo 3DS",
        exe_names: &["azahar.exe"],
        install_dirs: &["Azahar"],
        extensions: N3DS,
        args: &["{rom}"],
        library: Library::Files,
    },
    EmulatorDef {
        id: "lime3ds",
        name: "Lime3DS",
        systems: "Nintendo 3DS",
        exe_names: &["lime3ds.exe", "lime3ds-gui.exe", "lime-qt.exe"],
        install_dirs: &["Lime3DS"],
        extensions: N3DS,
        args: &["{rom}"],
        library: Library::Files,
    },
    EmulatorDef {
        id: "citra",
        name: "Citra",
        systems: "Nintendo 3DS",
        exe_names: &["citra-qt.exe"],
        install_dirs: &["Citra", "Citra\\nightly-mingw", "Citra\\canary-mingw"],
        extensions: N3DS,
        args: &["{rom}"],
        library: Library::Files,
    },
    EmulatorDef {
        id: "melonds",
        name: "melonDS",
        systems: "Nintendo DS",
        exe_names: &["melonds.exe"],
        install_dirs: &["melonDS"],
        extensions: &["nds", "dsi", "ids", "srl"],
        args: &["{rom}"],
        library: Library::Files,
    },
    EmulatorDef {
        id: "mgba",
        name: "mGBA",
        systems: "Game Boy · Game Boy Advance",
        exe_names: &["mgba.exe"],
        install_dirs: &["mGBA"],
        extensions: &["gba", "agb", "gbc", "cgb", "gb", "sgb", "mb"],
        args: &["{rom}"],
        library: Library::Files,
    },
    EmulatorDef {
        id: "xemu",
        name: "xemu",
        systems: "Xbox",
        exe_names: &["xemu.exe"],
        install_dirs: &["xemu"],
        extensions: &["iso", "xiso"],
        args: &["-dvd_path", "{rom}"],
        library: Library::Files,
    },
    EmulatorDef {
        id: "xenia",
        name: "Xenia",
        systems: "Xbox 360",
        exe_names: &["xenia.exe", "xenia_canary.exe", "xenia_canary_netplay.exe", "xenia_edge.exe"],
        install_dirs: &["Xenia"],
        extensions: &["iso", "xex", "zar"],
        args: &["{rom}"],
        library: Library::Files,
    },
    EmulatorDef {
        id: "snes9x",
        name: "Snes9x",
        systems: "Super Nintendo",
        exe_names: &["snes9x*"],
        install_dirs: &["Snes9x"],
        extensions: &["sfc", "smc", "fig", "swc", "bs", "st"],
        args: &["{rom}"],
        library: Library::Files,
    },
    EmulatorDef {
        id: "mesen",
        name: "Mesen",
        systems: "NES · SNES · Game Boy · PC Engine · Master System",
        exe_names: &["mesen.exe"],
        install_dirs: &["Mesen"],
        extensions: &[
            "nes", "fds", "unf", "unif", "sfc", "smc", "gb", "gbc", "gba", "pce", "sgx", "sms", "gg", "sg", "col", "ws", "wsc",
        ],
        args: &["{rom}"],
        library: Library::Files,
    },
    EmulatorDef {
        id: "flycast",
        name: "Flycast",
        systems: "Dreamcast · NAOMI",
        exe_names: &["flycast.exe"],
        install_dirs: &["Flycast"],
        extensions: &["gdi", "cdi", "chd", "cue", "m3u"],
        args: &["{rom}"],
        library: Library::Files,
    },
];

impl EmulatorDef {
    /// Which of its executable names `file_name` is: the lower, the more it is preferred.
    fn rank(&self, file_name: &str) -> Option<usize> {
        let name = file_name.to_ascii_lowercase();
        self.exe_names.iter().position(|pattern| match pattern.strip_suffix('*') {
            Some(prefix) => name.starts_with(prefix) && name.ends_with(".exe"),
            None => name == *pattern,
        })
    }

    fn runs(&self, file_name: &str) -> bool {
        self.rank(file_name).is_some()
    }

    /// The arguments that start a game with the executable `exe`.
    ///
    /// shadPS4's launcher takes other arguments than the emulator it starts: `-d` chooses
    /// the emulator version selected in the launcher, and a path with no option before it
    /// would be read as the emulator to use, not as the game.
    fn args_for(&self, exe: &Path) -> Vec<String> {
        let name = exe.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_ascii_lowercase();
        let args: &[&str] = match self.id {
            "shadps4" if name.starts_with("shadps4qtlauncher") => &["-d", "-g", "{rom}"],
            _ => self.args,
        };
        args.iter().map(|arg| arg.to_string()).collect()
    }
}

/// The emulator Kanso knows by this id.
pub fn known(id: &str) -> Option<&'static EmulatorDef> {
    KNOWN.iter().find(|def| def.id == id)
}

/// The emulator an executable is, by its file name; `None` for any other program.
pub fn identify(exe_path: &Path) -> Option<&'static EmulatorDef> {
    identify_name(exe_path.file_name()?.to_str()?)
}

/// The emulator whose executable has this file name.
pub fn identify_name(file_name: &str) -> Option<&'static EmulatorDef> {
    KNOWN.iter().find(|def| def.runs(file_name))
}

/// A folder an emulator's games are read from, and who said so.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GamesFolder {
    pub path: String,
    /// Listed by the emulator's own settings rather than added by the user in Kanso.
    pub from_emulator: bool,
}

/// An emulator on this PC, with everything needed to list and start its games.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Emulator {
    pub id: String,
    pub name: String,
    pub systems: String,
    pub exe_path: PathBuf,
    pub folders: Vec<GamesFolder>,
    pub extensions: Vec<String>,
    /// The arguments that start a game, `{rom}` and `{core}` still to be filled in.
    pub args: Vec<String>,
    pub library: Library,
    pub added_by_user: bool,
    /// The version its executable carries, when it carries one.
    pub version: Option<String>,
}

/// What the interface shows of an emulator.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EmulatorInfo {
    pub id: String,
    pub name: String,
    pub systems: String,
    pub exe_path: String,
    pub folders: Vec<GamesFolder>,
    pub added_by_user: bool,
    pub version: Option<String>,
}

impl Emulator {
    pub fn info(&self) -> EmulatorInfo {
        EmulatorInfo {
            id: self.id.clone(),
            name: self.name.clone(),
            systems: self.systems.clone(),
            exe_path: self.exe_path.to_string_lossy().into_owned(),
            folders: self.folders.clone(),
            added_by_user: self.added_by_user,
            version: self.version.clone(),
        }
    }

    fn exe_dir(&self) -> PathBuf {
        self.exe_path.parent().map(Path::to_path_buf).unwrap_or_default()
    }
}

/// What Kanso recognises an executable as, for the Settings dialog.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EmulatorIdentity {
    pub id: String,
    pub name: String,
    pub systems: String,
    pub extensions: Vec<String>,
    pub args: String,
}

impl EmulatorIdentity {
    pub fn of(def: &EmulatorDef) -> Self {
        Self {
            id: def.id.to_string(),
            name: def.name.to_string(),
            systems: def.systems.to_string(),
            extensions: def.extensions.iter().map(|e| e.to_string()).collect(),
            args: def.args.join(" "),
        }
    }
}

/// Splits a command line the user typed into arguments: spaces separate, double quotes keep
/// a part together. There is no shell behind it: nothing is expanded or interpreted.
pub fn split_args(line: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut started = false;
    for c in line.chars() {
        match c {
            '"' => {
                quoted = !quoted;
                started = true;
            }
            c if c.is_whitespace() && !quoted => {
                if started {
                    args.push(std::mem::take(&mut current));
                    started = false;
                }
            }
            c => {
                current.push(c);
                started = true;
            }
        }
    }
    if started {
        args.push(current);
    }
    args
}

/// The emulators of this PC: those the user added in Settings first, then the known ones
/// among `found_exes` (each emulator once: the executable it prefers, the first copy found). A setting without an
/// executable adds its folders and arguments to the emulator Kanso found by itself.
pub fn resolve(settings: &[EmulatorSetting], found_exes: &[PathBuf], places: &Places) -> Vec<Emulator> {
    let mut emulators: Vec<Emulator> = Vec::new();

    let build = |id: &str, exe: PathBuf, setting: Option<&EmulatorSetting>, added_by_user: bool| -> Emulator {
        let def = known(id);
        let exe_dir = exe.parent().map(Path::to_path_buf).unwrap_or_default();
        let mut folders: Vec<GamesFolder> = own_settings::game_folders(id, &exe_dir, places)
            .into_iter()
            .map(|path| GamesFolder { path: path.to_string_lossy().into_owned(), from_emulator: true })
            .collect();
        for folder in setting.map(|s| s.folders.as_slice()).unwrap_or_default() {
            let same = |known: &GamesFolder| {
                known.path.replace('\\', "/").trim_end_matches('/').eq_ignore_ascii_case(folder.replace('\\', "/").trim_end_matches('/'))
            };
            if !folders.iter().any(same) {
                folders.push(GamesFolder { path: folder.clone(), from_emulator: false });
            }
        }
        let own_args = setting.map(|s| split_args(&s.args)).filter(|args| !args.is_empty());
        Emulator {
            id: id.to_string(),
            name: def.map(|d| d.name.to_string()).unwrap_or_else(|| {
                let typed = setting.map(|s| s.name.trim()).unwrap_or_default();
                if typed.is_empty() {
                    exe.file_stem().and_then(|s| s.to_str()).unwrap_or(id).to_string()
                } else {
                    typed.to_string()
                }
            }),
            systems: def.map(|d| d.systems.to_string()).unwrap_or_default(),
            extensions: match def {
                Some(def) => def.extensions.iter().map(|e| e.to_string()).collect(),
                None => setting.map(|s| s.extensions.clone()).unwrap_or_default(),
            },
            args: own_args.unwrap_or_else(|| match def {
                Some(def) => def.args_for(&exe),
                None => vec!["{rom}".to_string()],
            }),
            library: def.map(|d| d.library).unwrap_or(Library::Files),
            version: crate::pe_version::file_version(&exe).map(|version| version.short()),
            exe_path: exe,
            folders,
            added_by_user,
        }
    };

    for setting in settings.iter().filter(|setting| !setting.exe_path.trim().is_empty()) {
        if !emulators.iter().any(|known| known.id == setting.id) {
            emulators.push(build(&setting.id, PathBuf::from(setting.exe_path.trim()), Some(setting), true));
        }
    }
    // Each emulator found once: the executable its definition prefers, then the first found
    let mut chosen: Vec<(&EmulatorDef, usize, &PathBuf)> = Vec::new();
    for exe in found_exes {
        let Some((def, rank)) = identify(exe).and_then(|def| Some((def, def.rank(exe.file_name()?.to_str()?)?))) else {
            continue;
        };
        if emulators.iter().any(|known| known.id == def.id) || !exe.is_file() {
            continue;
        }
        match chosen.iter_mut().find(|(known, _, _)| known.id == def.id) {
            Some(entry) if rank < entry.1 => *entry = (def, rank, exe),
            Some(_) => {}
            None => chosen.push((def, rank, exe)),
        }
    }
    for (def, _, exe) in chosen {
        let setting = settings.iter().find(|setting| setting.id == def.id);
        emulators.push(build(def.id, exe.clone(), setting, false));
    }
    emulators.sort_by_key(|emulator| emulator.name.to_lowercase());
    emulators
}

fn fnv1a_64(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325, |hash, byte| (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3))
}

/// The library entry of a game file run by `emulator`. The id depends on the emulator and on
/// where the file is, so that what the user did with the game survives rescans.
fn emulated_game(emulator: &Emulator, file: &str, title: String) -> DetectedGame {
    let key = file.replace('\\', "/").to_lowercase();
    let folder = Path::new(file.split('#').next().unwrap_or(file))
        .parent()
        .map(|dir| dir.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut game = DetectedGame::new_detected(
        format!("emu:{}:{:016x}", emulator.id, fnv1a_64(key.as_bytes())),
        title.clone(),
        clean_game_title(&title),
        GamePlatform::Emulator,
        Some(emulator.id.clone()),
        folder,
        Some(file.to_string()),
        None,
    );
    // A game file has no version of its own: what matters is the emulator that runs it
    game.version = emulator.version.clone();
    game
}

/// The games of each emulator, as library entries.
pub fn scan_games(emulators: &[Emulator]) -> SourceScanOutput {
    let mut output = SourceScanOutput::default();
    for emulator in emulators {
        match emulator.library {
            Library::Files => {
                let folders: Vec<PathBuf> = emulator.folders.iter().map(|folder| PathBuf::from(&folder.path)).collect();
                for RomFile { path, title } in find_rom_files(&folders, &emulator.extensions) {
                    output.games.push(emulated_game(emulator, &path.to_string_lossy(), title));
                }
            }
            Library::RetroArchPlaylists => {
                let (games, without_core) = special::retroarch_games(&emulator.exe_dir());
                for game in games {
                    output.games.push(emulated_game(emulator, &game.content, game.title));
                }
                if without_core > 0 {
                    output.warnings.push(format!("RetroArch: {without_core} {}", T::RetroArchNoCore.s()));
                }
            }
            Library::Rpcs3Games => {
                for RomFile { path, title } in special::rpcs3_games(&emulator.exe_dir()) {
                    output.games.push(emulated_game(emulator, &path.to_string_lossy(), title));
                }
            }
            Library::Ps4Folders => {
                let folders: Vec<PathBuf> = emulator.folders.iter().map(|folder| PathBuf::from(&folder.path)).collect();
                for RomFile { path, title } in special::ps4_games(&folders) {
                    output.games.push(emulated_game(emulator, &path.to_string_lossy(), title));
                }
            }
        }
    }
    output
}

/// A program to start, with its arguments and the folder to start it in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchCommand {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub working_dir: PathBuf,
}

/// The command that starts `game_file` in `emulator`. Each argument is passed to the program
/// as it is, with no shell in between: the game's path cannot be read as anything else.
pub fn launch_command(emulator: &Emulator, game_file: &str) -> Result<LaunchCommand, String> {
    let core = match emulator.library {
        Library::RetroArchPlaylists if emulator.args.iter().any(|arg| arg.contains("{core}")) => Some(
            special::retroarch_core_for(&emulator.exe_dir(), game_file)
                .ok_or_else(|| T::RetroArchCoreMissing.s().to_string())?
                .to_string_lossy()
                .into_owned(),
        ),
        _ => None,
    };
    let mut args: Vec<String> = emulator
        .args
        .iter()
        .map(|arg| arg.replace("{rom}", game_file).replace("{core}", core.as_deref().unwrap_or_default()))
        .collect();
    // Arguments typed without the placeholder: the game still has to be named
    if !emulator.args.iter().any(|arg| arg.contains("{rom}")) {
        args.push(game_file.to_string());
    }
    Ok(LaunchCommand {
        program: emulator.exe_path.clone(),
        args,
        working_dir: emulator.exe_dir(),
    })
}

#[cfg(test)]
mod tests {
    use super::own_settings::tests::Scratch;
    use super::*;

    #[test]
    fn executables_are_recognised_by_name() {
        for (file, id) in [
            ("Dolphin.exe", Some("dolphin")),
            ("pcsx2-qt.exe", Some("pcsx2")),
            ("pcsx2-qtx64-avx2.exe", Some("pcsx2")),
            // The old PCSX2 takes other arguments: not the one Kanso knows
            ("pcsx2.exe", None),
            ("duckstation-qt-x64-ReleaseLTCG.exe", Some("duckstation")),
            ("duckstation-qt-x64-ReleaseLTCG.pdb", None),
            ("snes9x-x64.exe", Some("snes9x")),
            ("retroarch.exe", Some("retroarch")),
            ("retroarch_debug.exe", None),
            ("Ryujinx.exe", Some("ryujinx")),
            ("xenia_canary.exe", Some("xenia")),
            ("Mesen.exe", Some("mesen")),
            ("Mesen-S.exe", None),
            ("shadPS4.exe", Some("shadps4")),
            ("shadPS4QtLauncher.exe", Some("shadps4")),
            ("notepad.exe", None),
        ] {
            assert_eq!(identify(Path::new("D:/Emu").join(file).as_path()).map(|def| def.id), id, "{file}");
        }
        // Every emulator has a distinct id, a name and a way to start a game
        let mut ids: Vec<&str> = KNOWN.iter().map(|def| def.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), KNOWN.len());
        for def in KNOWN {
            assert!(crate::config::is_valid_plugin_id(def.id), "{}", def.id);
            assert!(def.args.iter().any(|arg| arg.contains("{rom}")), "{}", def.id);
            assert_eq!(def.library == Library::Files, !def.extensions.is_empty(), "{}", def.id);
        }
    }

    #[test]
    fn a_typed_command_line_is_split_without_a_shell() {
        assert_eq!(split_args(r#"-b -e "{rom}""#), ["-b", "-e", "{rom}"]);
        assert_eq!(split_args(r#"  --config "C:\My Files\a.ini"   --exec={rom} "" "#), ["--config", r"C:\My Files\a.ini", "--exec={rom}", ""]);
        assert_eq!(split_args("a;b && c | d $(e) %PATH%"), ["a;b", "&&", "c", "|", "d", "$(e)", "%PATH%"], "nothing is special");
        assert!(split_args("   ").is_empty());
    }

    #[test]
    fn the_user_s_emulators_come_first_and_each_is_listed_once() {
        let s = Scratch::new("resolve");
        let places = s.places();
        let roms = s.dir("Roms/GC");
        let more = s.dir("Roms/More");
        let found_dolphin = s.file("Found/Dolphin/Dolphin.exe", "x");
        let found_duck = s.file("Found/Duck/duckstation-qt-x64-ReleaseLTCG.exe", "x");
        let my_dolphin = s.file("Mine/Dolphin/Dolphin.exe", "x");
        s.file("Mine/Dolphin/portable.txt", "");
        s.file("Mine/Dolphin/User/Config/Dolphin.ini", &format!("[General]\nISOPath0 = {}\n", roms.display()));

        let settings = [
            EmulatorSetting {
                id: "dolphin".into(),
                exe_path: my_dolphin.to_string_lossy().into_owned(),
                // One the emulator already lists, one more
                folders: vec![format!("{}/", roms.display()), more.to_string_lossy().into_owned()],
                args: "--exec={rom} --batch".into(),
                ..EmulatorSetting::default()
            },
            // Folders added to an emulator Kanso found
            EmulatorSetting { id: "duckstation".into(), folders: vec![more.to_string_lossy().into_owned()], ..EmulatorSetting::default() },
            // One Kanso does not know
            EmulatorSetting {
                id: "custom-openbor".into(),
                name: "OpenBOR".into(),
                exe_path: s.root.join("Mine/OpenBOR/OpenBOR.exe").to_string_lossy().into_owned(),
                extensions: vec!["pak".into()],
                ..EmulatorSetting::default()
            },
            // Nothing found and no executable given: there is no such emulator here
            EmulatorSetting { id: "cemu".into(), folders: vec![more.to_string_lossy().into_owned()], ..EmulatorSetting::default() },
        ];
        let emulators = resolve(&settings, &[found_dolphin, found_duck.clone(), s.root.join("Found/notepad.exe")], &places);

        let ids: Vec<&str> = emulators.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(ids, ["dolphin", "duckstation", "custom-openbor"], "by name: Dolphin, DuckStation, OpenBOR");
        let dolphin = &emulators[0];
        assert_eq!(dolphin.exe_path, my_dolphin, "the user's copy, not the one found");
        assert!(dolphin.added_by_user);
        assert_eq!(dolphin.args, ["--exec={rom}", "--batch"]);
        assert_eq!(
            dolphin.folders,
            [
                GamesFolder { path: roms.to_string_lossy().into_owned(), from_emulator: true },
                GamesFolder { path: more.to_string_lossy().into_owned(), from_emulator: false },
            ]
        );
        let duck = &emulators[1];
        assert_eq!((duck.exe_path.as_path(), duck.added_by_user), (found_duck.as_path(), false));
        assert_eq!(duck.args, ["-batch", "--", "{rom}"]);
        assert_eq!(duck.folders.len(), 1);
        let custom = &emulators[2];
        assert_eq!((custom.name.as_str(), custom.systems.as_str()), ("OpenBOR", ""));
        assert_eq!((custom.extensions.as_slice(), custom.args.as_slice()), (&["pak".to_string()][..], &["{rom}".to_string()][..]));
        assert_eq!(dolphin.info().exe_path, my_dolphin.to_string_lossy());
    }

    #[test]
    fn shadps4_lists_game_folders_and_starts_them_as_each_executable_expects() {
        let s = Scratch::new("shadps4");
        let places = s.places();
        let core = s.file("Emu/shadPS4 core/shadPS4.exe", "x");
        let launcher = s.file("Emu/shadPS4 launcher/shadPS4QtLauncher.exe", "x");
        let eboot = s.file("Giochi PS4/CUSA00900/eboot.bin", "x");
        std::fs::write(s.dir("Giochi PS4/CUSA00900/sce_sys").join("param.sfo"), special::tests::sfo(&[("TITLE", "Bloodborne")])).unwrap();
        s.file("Giochi PS4/CUSA00900/data/level.bin", "x");
        let settings = [EmulatorSetting { id: "shadps4".into(), folders: vec![s.root.join("Giochi PS4").to_string_lossy().into_owned()], ..EmulatorSetting::default() }];

        // Only the emulator proper: the game is named with -g, as every version takes it
        let emulators = resolve(&settings, std::slice::from_ref(&core), &places);
        assert_eq!(emulators[0].exe_path, core);
        let scanned = scan_games(&emulators);
        let games: Vec<(&str, Option<&Path>)> = scanned.games.iter().map(|game| (game.title.as_str(), game.executable_path.as_deref().map(Path::new))).collect();
        assert_eq!(games, [("Bloodborne", Some(eboot.as_path()))], "one game per folder, named by its title file");
        let command = launch_command(&emulators[0], &eboot.to_string_lossy()).unwrap();
        assert_eq!(command.args, ["-g", eboot.to_string_lossy().as_ref()]);

        // Both on this PC, whichever is found first: the launcher, which has a window; it
        // needs to be told which emulator to use before the game
        for found in [[core.clone(), launcher.clone()], [launcher.clone(), core.clone()]] {
            let emulators = resolve(&settings, &found, &places);
            assert_eq!(emulators.len(), 1);
            assert_eq!(emulators[0].exe_path, launcher);
            let command = launch_command(&emulators[0], &eboot.to_string_lossy()).unwrap();
            assert_eq!(command.args, ["-d", "-g", eboot.to_string_lossy().as_ref()]);
            assert_eq!(command.working_dir, s.root.join("Emu/shadPS4 launcher"), "where it looks for its portable settings");
        }
        // The copy the user chose is the one used, and arguments they typed are kept
        let own = [EmulatorSetting { id: "shadps4".into(), exe_path: core.to_string_lossy().into_owned(), args: "-f true -g {rom}".into(), ..EmulatorSetting::default() }];
        let emulators = resolve(&own, &[launcher], &places);
        assert_eq!((emulators.len(), emulators[0].exe_path.as_path()), (1, core.as_path()));
        assert_eq!(emulators[0].args, ["-f", "true", "-g", "{rom}"]);
    }

    #[test]
    fn games_become_library_entries_and_start_with_their_emulator() {
        let s = Scratch::new("scan");
        let places = s.places();
        let dolphin_exe = s.file("Emu/Dolphin/Dolphin.exe", "x");
        let rom = s.file("Roms/GC/Legend of Zelda, The - The Wind Waker (Europe).rvz", "x");
        s.file("Roms/GC/readme.txt", "x");
        let retroarch_exe = s.file("Emu/RetroArch/retroarch.exe", "x");
        let core = s.file("Emu/RetroArch/cores/snes9x_libretro.dll", "x");
        let snes = s.file("Roms/SNES/Super Metroid (Japan, USA).sfc", "x");
        let orphan = s.file("Roms/SNES/Orphan.sfc", "x");
        let quote = |path: &Path| serde_json::to_string(&path.to_string_lossy()).unwrap();
        s.file(
            "Emu/RetroArch/playlists/snes.lpl",
            &format!(r#"{{"items":[{{"path":{},"label":"Super Metroid (Japan, USA)","core_path":{}}},{{"path":{},"core_path":"DETECT"}}]}}"#, quote(&snes), quote(&core), quote(&orphan)),
        );

        let settings = [EmulatorSetting { id: "dolphin".into(), folders: vec![s.root.join("Roms/GC").to_string_lossy().into_owned()], ..EmulatorSetting::default() }];
        let emulators = resolve(&settings, &[dolphin_exe.clone(), retroarch_exe.clone()], &places);
        let scanned = scan_games(&emulators);

        let listed: Vec<(&str, &str, Option<&str>)> = scanned
            .games
            .iter()
            .map(|game| (game.title.as_str(), game.platform.as_str(), game.platform_app_id.as_deref()))
            .collect();
        assert_eq!(
            listed,
            [("The Legend of Zelda: The Wind Waker", "emulator", Some("dolphin")), ("Super Metroid", "emulator", Some("retroarch"))]
        );
        let zelda = &scanned.games[0];
        assert!(zelda.id.starts_with("emu:dolphin:") && zelda.id.len() == "emu:dolphin:".len() + 16, "{}", zelda.id);
        // Compared as paths: on Windows the same file can be written with either separator
        assert_eq!(zelda.executable_path.as_deref().map(Path::new), Some(rom.as_path()));
        assert_eq!(Path::new(&zelda.install_dir), s.root.join("Roms/GC"));
        assert!(zelda.is_installed && zelda.launch_uri.is_none());
        assert_eq!(scan_games(&emulators).games[0].id, zelda.id, "the same file is the same game at every scan");
        assert_eq!(scanned.warnings.len(), 1, "one game of RetroArch has no core yet: {:?}", scanned.warnings);
        assert!(scanned.warnings[0].starts_with("RetroArch: 1 "));

        // Starting: the emulator, its usual arguments, the game file as one argument
        let dolphin = emulators.iter().find(|e| e.id == "dolphin").unwrap();
        let command = launch_command(dolphin, &rom.to_string_lossy()).unwrap();
        assert_eq!(command.program, dolphin_exe);
        assert_eq!(command.args, ["-b", "-e", rom.to_string_lossy().as_ref()]);
        assert_eq!(command.working_dir, s.root.join("Emu/Dolphin"));
        // RetroArch needs the core its playlist names
        let retroarch = emulators.iter().find(|e| e.id == "retroarch").unwrap();
        let command = launch_command(retroarch, &snes.to_string_lossy()).unwrap();
        assert_eq!(command.args, ["-L", core.to_string_lossy().as_ref(), snes.to_string_lossy().as_ref()]);
        assert!(launch_command(retroarch, &orphan.to_string_lossy()).is_err(), "no core, no start");
        // Arguments typed without the placeholder still name the game; a name with spaces,
        // quotes or ampersands stays one argument
        let mut typed = dolphin.clone();
        typed.args = split_args("--batch");
        let odd = r#"D:\Roms\A & B "special" (USA).iso"#;
        assert_eq!(launch_command(&typed, odd).unwrap().args, ["--batch", odd]);
    }
}
