//! Looking for emulators on the PC.
//!
//! Emulators are rarely installed: most are an archive unpacked wherever the user keeps such
//! things, under whatever name they gave the folder (`D:\pcsx2 PS2 Emulator`). So Kanso does
//! not expect them anywhere in particular. It gathers candidates from several places and
//! keeps the executables whose file name is a known emulator's:
//!
//! 1. where programs are installed (the folders installers use, and Windows' own list of
//!    installed apps);
//! 2. the programs Windows remembers the user having run (two lists in the user's registry);
//! 3. a search of the fixed disks and of the user's Desktop, Documents and Downloads, a few
//!    levels deep and bounded in the number of folders it opens;
//! 4. what earlier searches found, remembered in a small file.
//!
//! The search of the disks (3) is the slow one: it runs the first time and when the user
//! asks for a scan, not at every start. Everything here only reads names of files and
//! folders; nothing is opened, changed or sent anywhere.

use super::identify;
use super::own_settings::Places;
use crate::scanner::registry::RegistryEntry;
use std::collections::{HashSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};

/// Where in the user's registry Windows keeps the programs that have been run: the value
/// names are the paths of their executables.
pub const RUN_PROGRAM_KEYS: [&str; 2] = [
    r"Software\Classes\Local Settings\Software\Microsoft\Windows\Shell\MuiCache",
    r"Software\Microsoft\Windows NT\CurrentVersion\AppCompatFlags\Compatibility Assistant\Store",
];

/// Folders opened at most by one search of the disks: a bound on how long it can take.
const MAX_FOLDERS_OPENED: usize = 12_000;
/// How far below a disk's root (or one of the user's folders) an emulator is looked for.
const DISK_DEPTH: usize = 4;
/// How far below a folder programs are installed in.
const PROGRAMS_DEPTH: usize = 2;

/// Folders that hold no emulator and are large: the system's own, the stores' game
/// libraries, and the caches of development tools.
const SKIPPED_FOLDERS: [&str; 24] = [
    "windows",
    "program files",
    "program files (x86)",
    "programdata",
    "users",
    "documents and settings",
    "recovery",
    "perflogs",
    "msocache",
    "system volume information",
    "windowsapps",
    "wpsystem",
    "wudownloadcache",
    "xboxgames",
    "steamapps",
    "appdata",
    "node_modules",
    "config.msi",
    "onedrivetemp",
    "windows.old",
    "intel",
    "amd",
    "nvidia",
    "drivers",
];

fn is_skipped(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.starts_with('$') || lower.starts_with('.') || SKIPPED_FOLDERS.contains(&lower.as_str())
}

/// The executables of known emulators below `roots`, each root searched to its own depth
/// (`1` = only the files directly in it), level by level, so that what lies near the top of
/// every root is seen before the search goes deep anywhere. It stops after opening
/// `max_folders` folders. A folder that holds an emulator is not searched further: what is
/// below it is that emulator's own.
pub fn search_folders(roots: &[(PathBuf, usize)], max_folders: usize) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = Vec::new();
    let mut queue: VecDeque<(PathBuf, usize)> = roots.iter().cloned().collect();
    let mut opened = 0usize;
    let mut seen: HashSet<PathBuf> = HashSet::new();

    while let Some((folder, depth)) = queue.pop_front() {
        if depth == 0 || opened >= max_folders || !seen.insert(folder.clone()) {
            continue;
        }
        let Ok(entries) = fs::read_dir(&folder) else {
            continue;
        };
        opened += 1;

        let mut subfolders: Vec<PathBuf> = Vec::new();
        let mut emulators: Vec<PathBuf> = Vec::new();
        for entry in entries.flatten() {
            // Links and junctions are not followed: they lead back up, or to another disk
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            let path = entry.path();
            if kind.is_file() {
                if identify(&path).is_some() {
                    emulators.push(path);
                }
            } else if kind.is_dir() && !is_skipped(&entry.file_name().to_string_lossy()) {
                subfolders.push(path);
            }
        }
        if emulators.is_empty() {
            subfolders.sort();
            queue.extend(subfolders.into_iter().map(|subfolder| (subfolder, depth - 1)));
        } else {
            emulators.sort();
            found.extend(emulators);
        }
    }
    found
}

/// The known emulators among the programs Windows remembers the user having run. A value
/// name is the path of an executable, sometimes followed by what the value describes
/// (`D:\Emu\Dolphin.exe.FriendlyAppName`).
pub fn among_run_programs(value_names: &[String]) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = Vec::new();
    for name in value_names {
        let Some(end) = name.to_ascii_lowercase().rfind(".exe").map(|at| at + 4) else {
            continue;
        };
        let path = PathBuf::from(&name[..end]);
        if identify(&path).is_some() && path.is_file() && !found.contains(&path) {
            found.push(path);
        }
    }
    found
}

/// The executables of known emulators where programs are installed: the folders Windows'
/// list of installed apps names, and the folders installers and guides use.
pub fn among_installed_programs(places: &Places, uninstall_entries: &[RegistryEntry]) -> Vec<PathBuf> {
    let mut roots: Vec<(PathBuf, usize)> = Vec::new();
    for def in super::KNOWN {
        for base in places.program_dirs.iter().chain(&places.local_appdata).chain(&places.appdata) {
            roots.extend(def.install_dirs.iter().map(|name| (base.join(name), 1)));
        }
        for entry in uninstall_entries {
            let name = entry.get("DisplayName").to_ascii_lowercase();
            let location = entry.get("InstallLocation").trim_matches('"');
            if !location.is_empty() && name.starts_with(&def.name.to_ascii_lowercase()) {
                roots.push((PathBuf::from(location), 1));
            }
        }
    }
    search_folders(&roots, usize::MAX)
}

/// What a search can draw on, and how far it should go.
#[derive(Debug, Clone, Default)]
pub struct Search {
    /// The file earlier discoveries are remembered in; `None` to remember nothing.
    pub remembered_file: Option<PathBuf>,
    /// Search the disks too. Without it the disks are searched only when nothing has ever
    /// been remembered (the first scan).
    pub thorough: bool,
    /// Roots of the fixed disks (`C:\`, `D:\`).
    pub drives: Vec<PathBuf>,
    /// Programs Windows remembers having run (value names of [`RUN_PROGRAM_KEYS`]).
    pub run_programs: Vec<String>,
    /// Windows' list of installed apps.
    pub uninstall_entries: Vec<RegistryEntry>,
}

fn read_remembered(file: &Path) -> Option<Vec<PathBuf>> {
    let text = fs::read_to_string(file).ok()?;
    let paths: Vec<String> = serde_json::from_str(&text).ok()?;
    Some(paths.into_iter().map(PathBuf::from).collect())
}

/// Every executable of a known emulator the search turns up, each once, in the order of the
/// sources: where programs are installed, programs that were run, the disks (when searched),
/// then what was remembered. What is found is remembered for the next time.
pub fn find(places: &Places, search: &Search) -> Vec<PathBuf> {
    let remembered = search.remembered_file.as_deref().and_then(read_remembered);
    let mut found = among_installed_programs(places, &search.uninstall_entries);
    found.extend(among_run_programs(&search.run_programs));

    if search.thorough || (search.remembered_file.is_some() && remembered.is_none()) {
        // The user's own folders first: they are small, and the bound is shared
        let mut roots: Vec<(PathBuf, usize)> =
            places.profile_dirs.iter().map(|dir| (dir.clone(), DISK_DEPTH)).collect();
        for drive in &search.drives {
            roots.push((drive.clone(), DISK_DEPTH));
            // Skipped by name inside a disk: searched here, but only as deep as programs go
            roots.push((drive.join("Program Files"), PROGRAMS_DEPTH));
            roots.push((drive.join("Program Files (x86)"), PROGRAMS_DEPTH));
        }
        found.extend(search_folders(&roots, MAX_FOLDERS_OPENED));
    }
    found.extend(remembered.unwrap_or_default().into_iter().filter(|exe| exe.is_file() && identify(exe).is_some()));

    let mut seen: HashSet<String> = HashSet::new();
    found.retain(|exe| seen.insert(exe.to_string_lossy().to_lowercase()));

    if let Some(file) = &search.remembered_file {
        let paths: Vec<String> = found.iter().map(|exe| exe.to_string_lossy().into_owned()).collect();
        if let Ok(json) = serde_json::to_string_pretty(&paths) {
            // Remembering is a convenience: a file that cannot be written loses nothing else
            let _ = fs::write(file, json);
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::super::own_settings::tests::Scratch;
    use super::*;

    fn relative(found: &[PathBuf], root: &Path) -> Vec<String> {
        found.iter().map(|exe| exe.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/")).collect()
    }

    /// A disk laid out as people do: emulators unpacked under names of their own choosing.
    fn disk(s: &Scratch) -> PathBuf {
        s.file("D/pcsx2 PS2 Emulator/pcsx2-qt.exe", "x");
        s.file("D/pcsx2 PS2 Emulator/resources/updater.exe", "x");
        s.file("D/Duckstation PS1 Emulator/duckstation-qt-x64-ReleaseLTCG.exe", "x");
        s.file("D/Eden Nightly/eden.exe", "x");
        s.file("D/Eden Switch emulator/eden.exe", "x");
        s.file("D/mGBA-0.10.5-win64/mGBA.exe", "x");
        s.file("D/Emulatori/Nintendo/Dolphin-x64/Dolphin.exe", "x");
        s.file("D/Giochi PS2/Gran Turismo 4.iso", "x");
        s.file("D/Giochi Switch/Some Game/eden.txt", "x");
        // Too deep to be looked for, and inside folders that are not searched
        s.file("D/a/b/c/d/e/retroarch.exe", "x");
        s.file("D/SteamLibrary/steamapps/common/RetroArch/retroarch.exe", "x");
        s.file("D/$RECYCLE.BIN/x/Dolphin.exe", "x");
        s.file("D/.cache/Dolphin.exe", "x");
        s.file("D/WindowsApps/x/xemu.exe", "x");
        // Installed on this disk rather than on the system's
        s.file("D/Program Files/PPSSPP/PPSSPPWindows64.exe", "x");
        s.file("D/Program Files/Other/deep/down/here/Cemu.exe", "x");
        s.root.join("D")
    }

    #[test]
    fn finds_emulators_under_any_folder_name_a_few_levels_down() {
        let s = Scratch::new("walk");
        let drive = disk(&s);
        let found = search_folders(&[(drive.clone(), DISK_DEPTH), (drive.join("Program Files"), PROGRAMS_DEPTH)], MAX_FOLDERS_OPENED);
        assert_eq!(
            relative(&found, &drive),
            [
                // Nearest the root first
                "Duckstation PS1 Emulator/duckstation-qt-x64-ReleaseLTCG.exe",
                "Eden Nightly/eden.exe",
                "Eden Switch emulator/eden.exe",
                "mGBA-0.10.5-win64/mGBA.exe",
                "pcsx2 PS2 Emulator/pcsx2-qt.exe",
                "Program Files/PPSSPP/PPSSPPWindows64.exe",
                "Emulatori/Nintendo/Dolphin-x64/Dolphin.exe",
            ]
        );

        // A bound on the folders opened stops the search, deep folders last
        let few = search_folders(&[(drive.clone(), DISK_DEPTH)], 8);
        assert!(!few.is_empty() && few.len() < 6, "{few:?}");
        assert!(few.iter().all(|exe| exe.components().count() == drive.components().count() + 2), "what is near the top: {few:?}");
        // A root that is not there, a depth of nothing
        assert!(search_folders(&[(s.root.join("missing"), 4), (drive, 0)], 100).is_empty());
    }

    #[test]
    fn recognises_emulators_among_the_programs_that_were_run() {
        let s = Scratch::new("run");
        let dolphin = s.file("Emu/Dolphin/Dolphin.exe", "x");
        let duck = s.file("Emu/Duck/duckstation-qt-x64-ReleaseLTCG.exe", "x");
        s.file("Emu/notepad.exe", "x");
        let name = |path: &Path, suffix: &str| format!("{}{suffix}", path.to_string_lossy());
        let names = vec![
            name(&dolphin, ".FriendlyAppName"),
            name(&dolphin, ".ApplicationCompany"),
            name(&duck, ""),
            name(&s.root.join("Emu/notepad.exe"), ".FriendlyAppName"),
            // Run once, deleted since
            name(&s.root.join("Emu/Gone/pcsx2-qt.exe"), ".FriendlyAppName"),
            "LangID".to_string(),
        ];
        assert_eq!(among_run_programs(&names), [dolphin, duck]);
    }

    #[test]
    fn the_disks_are_searched_the_first_time_and_when_asked() {
        let s = Scratch::new("find-all");
        let drive = disk(&s);
        let mut places = s.places();
        places.profile_dirs = vec![s.dir("Home/Desktop")];
        let desktop = s.file("Home/Desktop/Emulatori/xemu/xemu.exe", "x");
        let installed = s.file("Program Files/RetroArch-Win64/retroarch.exe", "x");
        let ran = s.file("Elsewhere/Flycast/flycast.exe", "x");
        let memory = s.root.join("emulators.json");
        let search = |thorough: bool| Search {
            remembered_file: Some(memory.clone()),
            thorough,
            drives: vec![drive.clone()],
            run_programs: vec![format!("{}.FriendlyAppName", ran.to_string_lossy())],
            uninstall_entries: Vec::new(),
        };

        // Nothing remembered yet: the disks are searched without being asked
        let first = find(&places, &search(false));
        assert_eq!(first.len(), 10, "{first:?}");
        assert_eq!(&first[..2], [installed.clone(), ran.clone()], "installed and run first, then what the search turns up");
        assert!(first.contains(&desktop), "the user's own folders are searched too");
        assert!(memory.is_file());

        // A new emulator appears; an ordinary scan does not search the disks again...
        let new = s.file("D/Cemu 2/Cemu.exe", "x");
        fs::remove_file(drive.join("Eden Nightly/eden.exe")).unwrap();
        let second = find(&places, &search(false));
        assert!(!second.contains(&new) && second.len() == 9, "what was remembered, less what is gone: {second:?}");
        // ...a scan the user asked for does
        let third = find(&places, &search(true));
        assert!(third.contains(&new) && third.len() == 10);
        assert!(find(&places, &search(false)).contains(&new), "and it is remembered from then on");

        // Without a file to remember in, nothing is searched unless asked, and nothing is written
        let plain = Search { remembered_file: None, ..search(false) };
        assert_eq!(find(&places, &plain), [installed.clone(), ran.clone()]);
        // A file that is not a list is as good as none: the disks are searched again
        fs::write(&memory, "{ not a list").unwrap();
        assert_eq!(find(&places, &search(false)).len(), 10);
    }
    #[test]
    fn finds_emulators_where_programs_are_installed() {
        let s = Scratch::new("find");
        let duck = s.file("AppData/Local/DuckStation/duckstation-qt-x64-ReleaseLTCG.exe", "x");
        s.file("AppData/Local/DuckStation/updater.exe", "x");
        let retroarch = s.file("Program Files/RetroArch-Win64/retroarch.exe", "x");
        let ppsspp = s.file("Tools/PSP/PPSSPPWindows64.exe", "x");
        s.file("Tools/Other/Dolphin.exe", "x");
        let uninstall = [
            RegistryEntry::new("PPSSPP_is1", &[("DisplayName", "PPSSPP 1.17"), ("InstallLocation", &s.root.join("Tools/PSP").to_string_lossy())]),
            // Another program whose folder happens to hold an emulator is not looked into
            RegistryEntry::new("Other", &[("DisplayName", "Some Tool"), ("InstallLocation", &s.root.join("Tools/Other").to_string_lossy())]),
            RegistryEntry::new("Gone", &[("DisplayName", "Dolphin"), ("InstallLocation", "Z:/gone")]),
        ];
        let mut found = among_installed_programs(&s.places(), &uninstall);
        found.sort();
        let mut expected = vec![duck, retroarch, ppsspp];
        expected.sort();
        assert_eq!(found, expected);
    }
}
