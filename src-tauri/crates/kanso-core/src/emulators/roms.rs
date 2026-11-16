//! The game files in an emulator's folders: which files count, and what to call them.

use super::special::sfo_value;
use crate::cleaner::clean_game_title;
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// How deep below a games folder files are looked for (system/game/disc at most).
const MAX_DEPTH: usize = 4;
/// Most game files read from one emulator: a folder that is not a games folder (a whole
/// drive picked by mistake) must not flood the library.
pub const MAX_GAMES_PER_EMULATOR: usize = 5000;

/// A game file found in a folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RomFile {
    pub path: PathBuf,
    pub title: String,
}

/// The name of a game from the name of its file, as collections name them.
///
/// `Legend of Zelda, The - Ocarina of Time (USA) (Rev 2) [!].z64` becomes
/// `The Legend of Zelda: Ocarina of Time`: region, revision and dump marks go, an article
/// moved to the end comes back to the front, and the dash collections use where the title has
/// a colon becomes one.
pub fn rom_title(file_stem: &str) -> String {
    // Marks in round and square brackets: (USA), (En,Fr), (Disc 1), [!], [b1]...
    let mut bare = String::with_capacity(file_stem.len());
    let mut depth = 0usize;
    for c in file_stem.chars() {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' => depth = depth.saturating_sub(1),
            _ if depth == 0 => bare.push(if c == '_' { ' ' } else { c }),
            _ => {}
        }
    }
    let bare = bare.split_whitespace().collect::<Vec<_>>().join(" ");
    let bare = bare.trim_matches(|c: char| c == '-' || c == '.' || c.is_whitespace());

    // "Legend of Zelda, The - Ocarina of Time": the article belongs before the name
    let (name, rest) = match bare.split_once(" - ") {
        Some((name, rest)) => (name, Some(rest)),
        None => (bare, None),
    };
    let name = [", The", ", A", ", An", ", Les", ", Le", ", La", ", Il", ", El", ", Los", ", Las", ", Der", ", Die", ", Das"]
        .iter()
        .find_map(|article| {
            let cut = name.len().checked_sub(article.len())?;
            (name.is_char_boundary(cut) && name[cut..].eq_ignore_ascii_case(article))
                .then(|| format!("{} {}", &article[2..], &name[..cut]))
        })
        .unwrap_or_else(|| name.to_string());

    let title = match rest {
        Some(rest) => format!("{name}: {}", rest.replace(" - ", ": ")),
        None => name,
    };
    if title.trim().is_empty() {
        file_stem.trim().to_string()
    } else {
        title
    }
}

/// File names that say nothing about the game: the boot file every game of a console has.
const BOOT_FILES: [&str; 4] = ["eboot", "boot", "default", "main"];
/// Folders that belong to the layout of a game rather than to its name.
const LAYOUT_FOLDERS: [&str; 4] = ["usrdir", "ps3_game", "image0", "sce_sys"];
/// A title file is a few kilobytes: anything larger is not read.
const MAX_SFO_BYTES: u64 = 1024 * 1024;

/// The `TITLE` of a PlayStation title file, when `path` is a small file of that kind.
pub fn sfo_title_of(path: &Path) -> Option<String> {
    let size = fs::metadata(path).ok().filter(|meta| meta.is_file())?.len();
    if size > MAX_SFO_BYTES {
        return None;
    }
    sfo_value(&fs::read(path).ok()?, "TITLE")
}

/// What to call the game a file is.
///
/// Usually its file name says it (see [`rom_title`]). A console's boot file does not: every
/// PlayStation game is an `eboot.bin`, every extracted Xbox 360 game a `default.xex`. Such a
/// game is named by the title file next to it (`sce_sys/param.sfo`, `PARAM.SFO`) or, without
/// one, by the folder it is in.
pub fn game_title(file: &Path) -> String {
    let stem = file.file_stem().and_then(|s| s.to_str()).unwrap_or_default();
    if !BOOT_FILES.iter().any(|name| stem.eq_ignore_ascii_case(name)) {
        return rom_title(stem);
    }
    let Some(dir) = file.parent() else {
        return rom_title(stem);
    };
    let title_files = [
        Some(dir.join("sce_sys").join("param.sfo")),
        Some(dir.join("PARAM.SFO")),
        dir.parent().map(|above| above.join("PARAM.SFO")),
    ];
    if let Some(title) = title_files.iter().flatten().find_map(|path| sfo_title_of(path)) {
        return title;
    }
    dir.ancestors()
        .filter_map(|folder| folder.file_name().and_then(|name| name.to_str()))
        .find(|name| !LAYOUT_FOLDERS.iter().any(|layout| name.eq_ignore_ascii_case(layout)))
        .map(rom_title)
        .unwrap_or_else(|| rom_title(stem))
}

/// Which disc of a multi-disc game a file is, from marks like `(Disc 2)` or `(CD 1 of 3)`.
fn disc_number(file_stem: &str) -> Option<u32> {
    let lower = file_stem.to_ascii_lowercase();
    ["(disc ", "(disk ", "(cd ", "(dvd ", "[disc ", "[cd "].iter().find_map(|mark| {
        let after = &lower[lower.find(mark)? + mark.len()..];
        let digits: String = after.chars().take_while(char::is_ascii_digit).collect();
        digits.parse().ok()
    })
}

fn extension_of(path: &Path) -> String {
    path.extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default()
}

/// The game files in `folders` (searched a few levels deep) with one of `extensions`, each
/// game once:
///
/// - a multi-disc game is its playlist (`.m3u`) if it has one, otherwise its first disc;
/// - a disc image described by a sheet (`.cue`, `.gdi`, `.ccd`, `.mds`) is the sheet, not the
///   data files next to it;
/// - hidden files and folders are left alone.
///
/// Sorted by path, so that the result does not depend on the order the disk lists files in.
pub fn find_rom_files(folders: &[PathBuf], extensions: &[String]) -> Vec<RomFile> {
    let wanted: HashSet<&str> = extensions.iter().map(String::as_str).collect();
    let mut files: Vec<PathBuf> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    for folder in folders {
        if !folder.is_dir() {
            continue;
        }
        let walker = WalkDir::new(folder)
            .max_depth(MAX_DEPTH)
            .follow_links(false)
            .into_iter()
            .filter_entry(|entry| {
                entry.depth() == 0
                    || !entry.file_name().to_string_lossy().starts_with('.')
            });
        for entry in walker.flatten() {
            if !entry.file_type().is_file() {
                continue;
            }
            let path = entry.into_path();
            if !wanted.contains(extension_of(&path).as_str()) {
                continue;
            }
            // The same folder listed twice, or one inside the other
            if seen.insert(path.to_string_lossy().to_lowercase()) {
                files.push(path);
            }
            if files.len() >= MAX_GAMES_PER_EMULATOR * 4 {
                break;
            }
        }
    }
    files.sort();

    // Folders that hold a sheet: the tracks and data files next to it are part of that disc
    const SHEETS: [&str; 5] = ["cue", "gdi", "ccd", "mds", "m3u"];
    const DATA: [&str; 4] = ["bin", "img", "sub", "mdf"];
    let folders_with_sheet: HashSet<PathBuf> = files
        .iter()
        .filter(|path| SHEETS.contains(&extension_of(path).as_str()))
        .filter_map(|path| path.parent().map(Path::to_path_buf))
        .collect();

    // One entry per game and folder: playlist first, then the lowest disc
    let mut by_game: BTreeMap<(PathBuf, String), (u32, PathBuf, String)> = BTreeMap::new();
    for path in files {
        let extension = extension_of(&path);
        let parent = path.parent().map(Path::to_path_buf).unwrap_or_default();
        if DATA.contains(&extension.as_str()) && folders_with_sheet.contains(&parent) {
            continue;
        }
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or_default();
        let title = game_title(&path);
        // A playlist stands for all its discs; a file with no disc mark is its own game
        let rank = match (extension.as_str(), disc_number(stem)) {
            ("m3u", _) => 0,
            (_, Some(disc)) => disc.max(1),
            (_, None) => 1,
        };
        let key = if extension == "m3u" || disc_number(stem).is_some() {
            (parent, clean_game_title(&title).to_lowercase())
        } else {
            // Two different files with the same cleaned name (two regions) stay two games
            (parent, format!("{}\u{0}{}", clean_game_title(&title).to_lowercase(), stem.to_lowercase()))
        };
        match by_game.get(&key) {
            Some((best, _, _)) if *best <= rank => {}
            _ => {
                by_game.insert(key, (rank, path, title));
            }
        }
    }

    let mut games: Vec<RomFile> = by_game
        .into_values()
        .map(|(_, path, title)| RomFile { path, title })
        .collect();
    // A disc-marked file and the playlist of the same game: the playlist already stands for it
    let playlists: HashSet<(PathBuf, String)> = games
        .iter()
        .filter(|game| extension_of(&game.path) == "m3u")
        .map(|game| {
            (
                game.path.parent().map(Path::to_path_buf).unwrap_or_default(),
                clean_game_title(&game.title).to_lowercase(),
            )
        })
        .collect();
    games.retain(|game| {
        extension_of(&game.path) == "m3u"
            || !playlists.contains(&(
                game.path.parent().map(Path::to_path_buf).unwrap_or_default(),
                clean_game_title(&game.title).to_lowercase(),
            ))
    });
    games.sort_by(|a, b| a.path.cmp(&b.path));
    games.truncate(MAX_GAMES_PER_EMULATOR);
    games
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn a_file_name_becomes_the_name_of_the_game() {
        for (file, title) in [
            ("Super Mario Galaxy (USA) (En,Fr,Es)", "Super Mario Galaxy"),
            ("Legend of Zelda, The - Ocarina of Time (USA) (Rev 2) [!]", "The Legend of Zelda: Ocarina of Time"),
            ("Final Fantasy VII (Europe) (Disc 1)", "Final Fantasy VII"),
            ("Metal_Gear_Solid_3_-_Snake_Eater", "Metal Gear Solid 3: Snake Eater"),
            ("Castlevania - Symphony of the Night [SLUS-00067]", "Castlevania: Symphony of the Night"),
            ("Pokémon - Edizione Smeraldo (Italy)", "Pokémon: Edizione Smeraldo"),
            ("Last of Us, The", "The Last of Us"),
            ("Ratchet & Clank - Up Your Arsenal - Demo", "Ratchet & Clank: Up Your Arsenal: Demo"),
            ("Tetris", "Tetris"),
            // Nothing but marks: the file name is still better than an empty title
            ("(Unknown) [b]", "(Unknown) [b]"),
        ] {
            assert_eq!(rom_title(file), title, "{file}");
        }
    }

    #[test]
    fn a_boot_file_is_named_by_its_title_file_or_its_folder() {
        use super::super::special::tests::sfo;
        let dir = std::env::temp_dir().join(format!("kanso-boot-files-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let write = |relative: &str, bytes: &[u8]| -> PathBuf {
            let path = dir.join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, bytes).unwrap();
            path
        };

        // PlayStation 4: the title file is in sce_sys, next to the boot file
        let ps4 = write("PS4/CUSA00900/eboot.bin", b"x");
        write("PS4/CUSA00900/sce_sys/param.sfo", &sfo(&[("TITLE", "Bloodborne")]));
        assert_eq!(game_title(&ps4), "Bloodborne");
        // PlayStation 3: next to the boot file's folder
        let ps3 = write("PS3/Demons Souls [BLUS30443]/PS3_GAME/USRDIR/EBOOT.BIN", b"x");
        write("PS3/Demons Souls [BLUS30443]/PS3_GAME/PARAM.SFO", &sfo(&[("TITLE", "Demon's Souls")]));
        assert_eq!(game_title(&ps3), "Demon's Souls");
        // No title file, or one that is not: the folder that names the game
        let bare = write("PS3/Journey (Europe)/PS3_GAME/USRDIR/EBOOT.BIN", b"x");
        assert_eq!(game_title(&bare), "Journey");
        let broken = write("PS4/Gravity Rush 2/eboot.bin", b"x");
        write("PS4/Gravity Rush 2/sce_sys/param.sfo", b"not a title file");
        assert_eq!(game_title(&broken), "Gravity Rush 2");
        let xbox = write("X360/Red Dead Redemption/default.xex", b"x");
        assert_eq!(game_title(&xbox), "Red Dead Redemption");
        // Any other file is called by its own name, whatever is next to it
        let named = write("PS4/CUSA00900/Bloodborne Trial (Europe).bin", b"x");
        assert_eq!(game_title(&named), "Bloodborne Trial");

        // In a list of files, each boot file is its own game with its own name
        let extensions = vec!["bin".to_string()];
        let titles: Vec<String> = find_rom_files(&[dir.join("PS3")], &extensions).into_iter().map(|rom| rom.title).collect();
        assert_eq!(titles, ["Demon's Souls", "Journey"]);

        let _ = fs::remove_dir_all(&dir);
    }

    fn touch(root: &Path, relative: &str) {
        let path = root.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, b"x").unwrap();
    }

    fn found(root: &Path, extensions: &[&str]) -> Vec<(String, String)> {
        let extensions: Vec<String> = extensions.iter().map(|e| e.to_string()).collect();
        find_rom_files(&[root.to_path_buf()], &extensions)
            .into_iter()
            .map(|rom| {
                let relative = rom.path.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/");
                (relative, rom.title)
            })
            .collect()
    }

    #[test]
    fn each_game_is_listed_once_whatever_files_it_is_made_of() {
        let dir = std::env::temp_dir().join(format!("kanso-roms-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);

        // Plain files, in the folder and below it; other files are not games
        touch(&dir, "Super Mario Sunshine (Europe).iso");
        touch(&dir, "gamecube/Metroid Prime (USA).rvz");
        touch(&dir, "notes.txt");
        touch(&dir, "saves/Metroid Prime.gci");
        // Hidden folders are left alone; folders deeper than a collection goes are not walked
        touch(&dir, ".trash/Deleted Game.iso");
        touch(&dir, "a/b/c/d/e/Too Deep.iso");
        // A disc image with its sheet: the sheet is the game, its tracks are not
        touch(&dir, "psx/Crash Bandicoot (Europe).cue");
        touch(&dir, "psx/Crash Bandicoot (Europe).bin");
        // A loose .bin with no sheet in its folder is a game of its own
        touch(&dir, "loose/Tekken 3 (USA).bin");
        // Three discs and no playlist: the first disc stands for the game
        touch(&dir, "ff8/Final Fantasy VIII (Italy) (Disc 2).chd");
        touch(&dir, "ff8/Final Fantasy VIII (Italy) (Disc 1).chd");
        touch(&dir, "ff8/Final Fantasy VIII (Italy) (Disc 3).chd");
        // Discs with a playlist: the playlist stands for the game
        touch(&dir, "mgs/Metal Gear Solid (USA) (Disc 1).chd");
        touch(&dir, "mgs/Metal Gear Solid (USA) (Disc 2).chd");
        touch(&dir, "mgs/Metal Gear Solid (USA).m3u");
        // Two regions of the same game are two files the user chose to keep
        touch(&dir, "regions/Gran Turismo (Europe).chd");
        touch(&dir, "regions/Gran Turismo (Japan).chd");

        let games = found(&dir, &["iso", "rvz", "cue", "bin", "chd", "m3u"]);
        assert_eq!(
            games,
            [
                ("Super Mario Sunshine (Europe).iso", "Super Mario Sunshine"),
                ("ff8/Final Fantasy VIII (Italy) (Disc 1).chd", "Final Fantasy VIII"),
                ("gamecube/Metroid Prime (USA).rvz", "Metroid Prime"),
                ("loose/Tekken 3 (USA).bin", "Tekken 3"),
                ("mgs/Metal Gear Solid (USA).m3u", "Metal Gear Solid"),
                ("psx/Crash Bandicoot (Europe).cue", "Crash Bandicoot"),
                ("regions/Gran Turismo (Europe).chd", "Gran Turismo"),
                ("regions/Gran Turismo (Japan).chd", "Gran Turismo"),
            ]
            .map(|(path, title)| (path.to_string(), title.to_string()))
        );

        // A folder that does not exist, and the same folder twice, change nothing
        let extensions = vec!["iso".to_string()];
        let twice = find_rom_files(&[dir.clone(), dir.join("missing"), dir.clone()], &extensions);
        assert_eq!(twice.len(), 1);

        let _ = fs::remove_dir_all(&dir);
    }
}
