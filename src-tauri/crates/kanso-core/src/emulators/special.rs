//! The emulators whose games are not simply "the files in some folders".
//!
//! - **RetroArch** runs many systems, each through a *core*. Its games are in its playlists
//!   (`playlists/*.lpl`, JSON), which also say which core runs each one.
//! - **RPCS3** keeps a list of games by serial (`config/games.yml`), each a folder holding the
//!   game's `EBOOT.BIN` and a `PARAM.SFO` with its title.
//! - **shadPS4** runs games that are folders: each holds an `eboot.bin`, the same file name
//!   for every game, and a `sce_sys/param.sfo` with its title.

use super::roms::{game_title, rom_title, RomFile, MAX_GAMES_PER_EMULATOR};
use serde_json::Value;
use std::collections::{HashSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};

/// A path as RetroArch writes it: `:\cores\x.dll` is relative to RetroArch's folder.
fn retroarch_path(raw: &str, exe_dir: &Path) -> PathBuf {
    match raw.strip_prefix(':') {
        Some(relative) => exe_dir.join(relative.trim_start_matches(['\\', '/'])),
        None => PathBuf::from(raw),
    }
}

/// The folder RetroArch keeps its playlists in: `playlists` next to the executable, unless
/// `retroarch.cfg` names another one.
fn playlists_dir(exe_dir: &Path) -> PathBuf {
    let configured = fs::read_to_string(exe_dir.join("retroarch.cfg")).ok().and_then(|text| {
        text.lines().find_map(|line| {
            let (key, value) = line.split_once('=')?;
            (key.trim() == "playlist_directory").then(|| value.trim().trim_matches('"').to_string())
        })
    });
    match configured.as_deref() {
        Some(value) if !value.is_empty() && value != "default" => retroarch_path(value, exe_dir),
        _ => exe_dir.join("playlists"),
    }
}

/// A game in a RetroArch playlist, with the core that runs it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaylistGame {
    /// The content path as RetroArch wrote it: a file, or `archive.zip#file-inside`.
    pub content: String,
    pub title: String,
    pub core: PathBuf,
}

/// What RetroArch's playlists hold: the games that can be started (their file exists and a
/// core is known for them), and how many were left out for lack of a core. RetroArch writes
/// `DETECT` for the core until the game has been started once from RetroArch itself.
pub fn retroarch_games(exe_dir: &Path) -> (Vec<PlaylistGame>, usize) {
    let mut games = Vec::new();
    let mut without_core = 0;
    let mut seen: HashSet<String> = HashSet::new();

    let mut playlists: Vec<PathBuf> = fs::read_dir(playlists_dir(exe_dir))
        .map(|entries| {
            entries
                .flatten()
                .map(|entry| entry.path())
                .filter(|path| path.extension().is_some_and(|e| e.eq_ignore_ascii_case("lpl")))
                .collect()
        })
        .unwrap_or_default();
    playlists.sort();

    let known = |core: Option<&str>| -> Option<String> {
        core.map(str::trim)
            .filter(|core| !core.is_empty() && !core.eq_ignore_ascii_case("DETECT"))
            .map(str::to_string)
    };

    for playlist in playlists {
        let Some(json) = fs::read_to_string(&playlist).ok().and_then(|text| serde_json::from_str::<Value>(&text).ok())
        else {
            continue;
        };
        let default_core = known(json.get("default_core_path").and_then(Value::as_str));
        for item in json.get("items").and_then(Value::as_array).map(Vec::as_slice).unwrap_or_default() {
            let Some(content) = item.get("path").and_then(Value::as_str).map(str::trim).filter(|p| !p.is_empty()) else {
                continue;
            };
            // "archive.zip#game.sfc": the file to look for is the archive
            let file = retroarch_path(content.split('#').next().unwrap_or(content), exe_dir);
            if !file.is_file() || !seen.insert(content.to_lowercase()) {
                continue;
            }
            let Some(core) = known(item.get("core_path").and_then(Value::as_str)).or_else(|| default_core.clone()) else {
                without_core += 1;
                continue;
            };
            let core = retroarch_path(&core, exe_dir);
            if !core.is_file() {
                without_core += 1;
                continue;
            }
            let label = item.get("label").and_then(Value::as_str).map(str::trim).unwrap_or_default();
            let stem = file.file_stem().and_then(|s| s.to_str()).unwrap_or_default();
            games.push(PlaylistGame {
                content: content.to_string(),
                title: rom_title(if label.is_empty() { stem } else { label }),
                core,
            });
            if games.len() >= MAX_GAMES_PER_EMULATOR {
                return (games, without_core);
            }
        }
    }
    (games, without_core)
}

/// The core RetroArch's playlists name for `content`, looked up again when the game starts.
pub fn retroarch_core_for(exe_dir: &Path, content: &str) -> Option<PathBuf> {
    retroarch_games(exe_dir)
        .0
        .into_iter()
        .find(|game| game.content.eq_ignore_ascii_case(content))
        .map(|game| game.core)
}

/// A text value of a PlayStation title file (`PARAM.SFO`, `param.sfo`): a small table of
/// named values, the same on PlayStation 3 and 4. `TITLE` is the name of the game.
///
/// Layout: magic `\0PSF`, version, offset of the key table, offset of the data table, number
/// of entries; then one 16-byte entry each (key offset, format, length, max length, data
/// offset), all little-endian.
pub fn sfo_value(bytes: &[u8], wanted: &str) -> Option<String> {
    let u32_at = |offset: usize| -> Option<usize> {
        bytes.get(offset..offset + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize)
    };
    if bytes.get(..4)? != b"\0PSF" {
        return None;
    }
    let (keys, data, count) = (u32_at(8)?, u32_at(12)?, u32_at(16)?);
    for index in 0..count.min(256) {
        let entry = 20 + index * 16;
        let key_offset = u16::from_le_bytes([*bytes.get(entry)?, *bytes.get(entry + 1)?]) as usize;
        let key_start = keys.checked_add(key_offset)?;
        let key_end = key_start + bytes.get(key_start..)?.iter().position(|b| *b == 0)?;
        if &bytes[key_start..key_end] != wanted.as_bytes() {
            continue;
        }
        let length = u32_at(entry + 4)?;
        let start = data.checked_add(u32_at(entry + 12)?)?;
        let raw = bytes.get(start..start.checked_add(length)?)?;
        let raw = raw.split(|b| *b == 0).next().unwrap_or_default();
        let text = String::from_utf8_lossy(raw).replace(['\r', '\n'], " ");
        let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
        return (!text.is_empty()).then_some(text);
    }
    None
}

/// The `TITLE` of a PlayStation title file.
pub fn sfo_title(bytes: &[u8]) -> Option<String> {
    sfo_value(bytes, "TITLE")
}

/// The games RPCS3 lists: for each folder in `games.yml`, the `EBOOT.BIN` that starts it and
/// the title its `PARAM.SFO` gives (the folder's name when there is none).
pub fn rpcs3_games(exe_dir: &Path) -> Vec<RomFile> {
    // A `portable` folder next to the executable holds everything, when it exists
    let root = Some(exe_dir.join("portable")).filter(|dir| dir.is_dir()).unwrap_or_else(|| exe_dir.to_path_buf());
    let Some(text) = [root.join("config").join("games.yml"), root.join("games.yml")]
        .iter()
        .find_map(|path| fs::read_to_string(path).ok())
    else {
        return Vec::new();
    };

    let mut games = Vec::new();
    let mut seen: HashSet<PathBuf> = HashSet::new();
    for line in text.lines() {
        // "BLUS30443: D:/PS3/Demons Souls [BLUS30443]/": a flat map of serial to folder
        let Some((serial, folder)) = line.split_once(": ") else {
            continue;
        };
        let folder = folder.trim().trim_matches(['"', '\'']);
        if serial.trim().is_empty() || serial.starts_with('#') || folder.is_empty() {
            continue;
        }
        let folder = PathBuf::from(folder);
        let Some((eboot, sfo)) = [
            (folder.join("PS3_GAME").join("USRDIR").join("EBOOT.BIN"), folder.join("PS3_GAME").join("PARAM.SFO")),
            (folder.join("USRDIR").join("EBOOT.BIN"), folder.join("PARAM.SFO")),
        ]
        .into_iter()
        .find(|(eboot, _)| eboot.is_file()) else {
            continue;
        };
        if !seen.insert(eboot.clone()) {
            continue;
        }
        let title = fs::read(&sfo).ok().and_then(|bytes| sfo_title(&bytes)).unwrap_or_else(|| {
            let name = folder.file_name().and_then(|n| n.to_str()).unwrap_or_default();
            rom_title(name)
        });
        games.push(RomFile { path: eboot, title });
        if games.len() >= MAX_GAMES_PER_EMULATOR {
            break;
        }
    }
    games
}

/// How deep below a games folder a PlayStation 4 game is looked for: as deep as shadPS4
/// itself goes.
const PS4_MAX_DEPTH: usize = 5;
/// Most folders opened while looking for PlayStation 4 games in one games folder.
const PS4_MAX_FOLDERS: usize = 4000;

/// Whether a folder holds an update or mods of a game rather than a game: shadPS4 keeps
/// those next to the game, under its folder's name and a suffix (`CUSA00900-UPDATE`,
/// `CUSA00900-patch`, `CUSA00900-mods`).
fn is_ps4_addition(folder_name: &str) -> bool {
    let name = folder_name.to_ascii_lowercase();
    ["-update", "-patch", "-mods"].iter().any(|suffix| name.ends_with(suffix))
}

/// The PlayStation 4 games in `folders`: every folder that holds an `eboot.bin`, the file
/// shadPS4 starts. The title is the one in the game's `sce_sys/param.sfo`, or the folder's
/// name without one. A game's folder is not searched further, and updates or mods kept in a
/// folder of their own are not games.
pub fn ps4_games(folders: &[PathBuf]) -> Vec<RomFile> {
    let mut games: Vec<RomFile> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for root in folders {
        let mut queue: VecDeque<(PathBuf, usize)> = VecDeque::from([(root.clone(), 0)]);
        let mut opened = 0;
        while let Some((folder, depth)) = queue.pop_front() {
            opened += 1;
            if opened > PS4_MAX_FOLDERS || games.len() >= MAX_GAMES_PER_EMULATOR {
                break;
            }
            let Ok(entries) = fs::read_dir(&folder) else {
                continue;
            };
            let mut eboot = None;
            let mut below = Vec::new();
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                let Ok(kind) = entry.file_type() else {
                    continue;
                };
                if kind.is_file() && name.eq_ignore_ascii_case("eboot.bin") {
                    eboot = Some(entry.path());
                } else if kind.is_dir() && !name.starts_with('.') && !is_ps4_addition(&name) {
                    below.push(entry.path());
                }
            }
            match eboot {
                Some(eboot) => {
                    if seen.insert(eboot.to_string_lossy().to_lowercase()) {
                        games.push(RomFile { title: game_title(&eboot), path: eboot });
                    }
                }
                None if depth < PS4_MAX_DEPTH => {
                    below.sort();
                    queue.extend(below.into_iter().map(|path| (path, depth + 1)));
                }
                None => {}
            }
        }
    }
    games.sort_by(|a, b| a.path.cmp(&b.path));
    games.truncate(MAX_GAMES_PER_EMULATOR);
    games
}

#[cfg(test)]
pub(crate) mod tests {
    use super::super::own_settings::tests::Scratch;
    use super::*;

    /// A `PARAM.SFO` holding the given values, as a console would write it.
    pub(crate) fn sfo(values: &[(&str, &str)]) -> Vec<u8> {
        let mut keys = Vec::new();
        let mut data = Vec::new();
        let mut index = Vec::new();
        for (key, value) in values {
            let length = value.len() as u32 + 1;
            index.extend_from_slice(&(keys.len() as u16).to_le_bytes());
            index.extend_from_slice(&0x0204u16.to_le_bytes());
            index.extend_from_slice(&length.to_le_bytes());
            index.extend_from_slice(&(length + 3).to_le_bytes());
            index.extend_from_slice(&(data.len() as u32).to_le_bytes());
            keys.extend_from_slice(key.as_bytes());
            keys.push(0);
            data.extend_from_slice(value.as_bytes());
            data.extend_from_slice(&[0, 0, 0, 0]);
        }
        let key_table = 20 + index.len() as u32;
        let mut file = b"\0PSF".to_vec();
        file.extend_from_slice(&0x0101u32.to_le_bytes());
        file.extend_from_slice(&key_table.to_le_bytes());
        file.extend_from_slice(&(key_table + keys.len() as u32).to_le_bytes());
        file.extend_from_slice(&(values.len() as u32).to_le_bytes());
        file.extend(index);
        file.extend(keys);
        file.extend(data);
        file
    }

    #[test]
    fn reads_the_title_of_a_playstation_game() {
        let file = sfo(&[("CATEGORY", "DG"), ("TITLE", "Demon's Souls"), ("TITLE_ID", "BLUS30443")]);
        assert_eq!(sfo_title(&file).as_deref(), Some("Demon's Souls"));
        assert_eq!(sfo_title(&sfo(&[("TITLE", "Ni no Kuni\nWrath of the White Witch")])).as_deref(), Some("Ni no Kuni Wrath of the White Witch"));
        // No title, not such a file, or a file cut short: nothing, and no panic
        assert_eq!(sfo_title(&sfo(&[("CATEGORY", "DG")])), None);
        assert_eq!(sfo_title(b"PNG and then some"), None);
        for cut in 0..file.len() {
            let _ = sfo_title(&file[..cut]);
        }
    }

    #[test]
    fn ps4_games_are_folders_named_by_their_title_file() {
        let s = Scratch::new("ps4");
        let games_dir = s.dir("Giochi PS4");
        // A game by its serial, with its title file
        s.file("Giochi PS4/CUSA00900/eboot.bin", "x");
        fs::write(s.dir("Giochi PS4/CUSA00900/sce_sys").join("param.sfo"), sfo(&[("CATEGORY", "gd"), ("TITLE", "Bloodborne"), ("TITLE_ID", "CUSA00900")])).unwrap();
        // Data files of the game are not games
        s.file("Giochi PS4/CUSA00900/data/movie.bin", "x");
        s.file("Giochi PS4/CUSA00900/sce_module/libc.prx", "x");
        // Its update, kept next to it: part of the same game
        s.file("Giochi PS4/CUSA00900-UPDATE/eboot.bin", "x");
        s.file("Giochi PS4/CUSA00900-patch/eboot.bin", "x");
        s.file("Giochi PS4/CUSA00900-mods/eboot.bin", "x");
        // A game with no title file is called as its folder
        s.file("Giochi PS4/Gravity Rush 2 [CUSA04943]/EBOOT.BIN", "x");
        // Games grouped in a folder; a folder too deep; a hidden one; one with no game
        s.file("Giochi PS4/Sparatutto/CUSA07408/eboot.bin", "x");
        fs::write(s.dir("Giochi PS4/Sparatutto/CUSA07408/sce_sys").join("param.sfo"), sfo(&[("TITLE", "God of War")])).unwrap();
        s.file("Giochi PS4/a/b/c/d/e/CUSA11111/eboot.bin", "x");
        s.file("Giochi PS4/.trash/CUSA22222/eboot.bin", "x");
        s.file("Giochi PS4/Salvataggi/readme.txt", "x");

        let listed = |folders: &[PathBuf]| -> Vec<(String, String)> {
            ps4_games(folders)
                .into_iter()
                .map(|game| (game.title, game.path.strip_prefix(&s.root).unwrap().to_string_lossy().replace('\\', "/")))
                .collect()
        };
        let expected = [
            ("Bloodborne", "Giochi PS4/CUSA00900/eboot.bin"),
            ("Gravity Rush 2", "Giochi PS4/Gravity Rush 2 [CUSA04943]/EBOOT.BIN"),
            ("God of War", "Giochi PS4/Sparatutto/CUSA07408/eboot.bin"),
        ]
        .map(|(title, path)| (title.to_string(), path.to_string()));
        assert_eq!(listed(std::slice::from_ref(&games_dir)), expected);
        // The same folder twice, a folder inside it and one that is gone change nothing
        assert_eq!(listed(&[games_dir.clone(), games_dir.join("Sparatutto"), s.root.join("gone"), games_dir.clone()]), expected);
        // A folder that is itself a game
        assert_eq!(listed(&[games_dir.join("CUSA00900")]), [expected[0].clone()]);
    }

    #[test]
    fn rpcs3_games_are_the_folders_its_list_names() {
        let s = Scratch::new("rpcs3");
        let exe_dir = s.dir("RPCS3");
        let disc = s.dir("PS3/Demons Souls [BLUS30443]");
        s.file("PS3/Demons Souls [BLUS30443]/PS3_GAME/USRDIR/EBOOT.BIN", "x");
        fs::write(s.dir("PS3/Demons Souls [BLUS30443]/PS3_GAME").join("PARAM.SFO"), sfo(&[("TITLE", "Demon's Souls")])).unwrap();
        // A game installed on the emulated hard disk, with no title file
        let installed = s.dir("RPCS3/dev_hdd0/game/NPUB30910");
        s.file("RPCS3/dev_hdd0/game/NPUB30910/USRDIR/EBOOT.BIN", "x");
        // Listed, but its folder is gone
        let gone = s.root.join("PS3/Gone");
        s.file(
            "RPCS3/config/games.yml",
            &format!("BLUS30443: {}/\nNPUB30910: \"{}\"\nBLES00000: {}\nnot a pair\n", disc.display(), installed.display(), gone.display()),
        );

        let games: Vec<(String, String)> = rpcs3_games(&exe_dir)
            .into_iter()
            .map(|game| (game.title, game.path.strip_prefix(&s.root).unwrap().to_string_lossy().replace('\\', "/")))
            .collect();
        assert_eq!(
            games,
            [
                ("Demon's Souls".to_string(), "PS3/Demons Souls [BLUS30443]/PS3_GAME/USRDIR/EBOOT.BIN".to_string()),
                ("NPUB30910".to_string(), "RPCS3/dev_hdd0/game/NPUB30910/USRDIR/EBOOT.BIN".to_string()),
            ]
        );
        assert!(rpcs3_games(&s.root.join("nowhere")).is_empty());
    }

    #[test]
    fn retroarch_games_are_the_ones_in_its_playlists_that_have_a_core() {
        let s = Scratch::new("retroarch");
        let exe_dir = s.dir("RetroArch");
        let snes = s.file("RetroArch/cores/snes9x_libretro.dll", "x");
        let mario = s.file("Roms/SNES/Super Mario World (USA).sfc", "x");
        let zelda = s.file("Roms/SNES/zelda.zip", "x");
        let sonic = s.file("Roms/MD/Sonic The Hedgehog (USA, Europe).md", "x");
        let quote = |path: &Path| serde_json::to_string(&path.to_string_lossy()).unwrap();

        // A playlist whose items name their core, one way or the other
        s.file(
            "RetroArch/playlists/Nintendo - Super Nintendo Entertainment System.lpl",
            &format!(
                r#"{{"version":"1.5","default_core_path":"DETECT","items":[
                    {{"path":{mario},"label":"Super Mario World (USA)","core_path":{core},"core_name":"Snes9x","crc32":"B19ED489|crc","db_name":"x.lpl"}},
                    {{"path":{zelda_inner},"label":"","core_path":":\\cores\\snes9x_libretro.dll","core_name":"Snes9x"}},
                    {{"path":{mario},"label":"A duplicate","core_path":{core}}},
                    {{"path":{gone},"label":"Gone","core_path":{core}}},
                    {{"label":"No path"}}
                ]}}"#,
                mario = quote(&mario),
                core = quote(&snes),
                zelda_inner = serde_json::to_string(&format!("{}#Legend of Zelda, The - A Link to the Past (USA).sfc", zelda.to_string_lossy())).unwrap(),
                gone = quote(&s.root.join("Roms/SNES/gone.sfc")),
            )
            .replace(":\\\\cores\\\\", if cfg!(windows) { ":\\\\cores\\\\" } else { ":/cores/" }),
        );
        // A playlist never started from RetroArch: no core for its games yet
        s.file(
            "RetroArch/playlists/Sega - Mega Drive - Genesis.lpl",
            &format!(r#"{{"default_core_path":"DETECT","items":[{{"path":{},"label":"Sonic The Hedgehog (USA, Europe)","core_path":"DETECT","core_name":"DETECT"}}]}}"#, quote(&sonic)),
        );
        s.file("RetroArch/playlists/broken.lpl", "{ not json");
        // RetroArch's history lives next to the executable, not among the playlists
        s.file("RetroArch/content_history.lpl", "{}");

        let (games, without_core) = retroarch_games(&exe_dir);
        let listed: Vec<(&str, &Path)> = games.iter().map(|game| (game.title.as_str(), game.core.as_path())).collect();
        assert_eq!(listed, [("Super Mario World", snes.as_path()), ("zelda", snes.as_path())]);
        assert_eq!(without_core, 1, "Sonic has no core yet");
        assert!(games[1].content.contains('#'), "the archive is passed to RetroArch as it wrote it");
        assert_eq!(retroarch_core_for(&exe_dir, &games[0].content.to_uppercase()).as_deref(), Some(snes.as_path()));
        assert_eq!(retroarch_core_for(&exe_dir, "elsewhere.sfc"), None);

        // Another playlists folder, named in retroarch.cfg; and no playlists at all
        s.file("Elsewhere/lists/a.lpl", &format!(r#"{{"default_core_path":{},"items":[{{"path":{}}}]}}"#, quote(&snes), quote(&sonic)));
        s.file("RetroArch/retroarch.cfg", &format!("video_driver = \"vulkan\"\nplaylist_directory = {}\n", quote(&s.root.join("Elsewhere/lists")).replace("\\\\", "\\")));
        let (elsewhere, _) = retroarch_games(&exe_dir);
        assert_eq!(elsewhere.len(), 1);
        assert_eq!(elsewhere[0].title, "Sonic The Hedgehog", "no label: the file's name, cleaned");
        assert_eq!(retroarch_games(&s.root.join("nowhere")), (Vec::new(), 0));
    }
}
