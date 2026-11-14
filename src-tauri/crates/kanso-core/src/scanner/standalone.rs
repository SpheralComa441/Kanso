use crate::cleaner::clean_game_title;
use crate::models::{DetectedGame, GamePlatform, SourceScanOutput};
use std::collections::{HashMap, HashSet};
use std::path::{Component, Path, PathBuf};
use walkdir::WalkDir;

/// Case-insensitive substrings excluded when scanning standalone `.exe` files.
pub const EXCLUDED_EXE_SUBSTRINGS: &[&str] = &[
    "unins",
    "crash",
    "redist",
    "dxsetup",
    "vc_redist",
    "launcher",
];

/// Generic binary subfolder names that should not be used as a game's display title.
const GENERIC_BIN_DIRS: &[&str] = &[
    "bin",
    "binaries",
    "win64",
    "win32",
    "x64",
    "x86",
    "release",
    "game",
    "retail",
];

/// Returns `true` if `path` has an `.exe` extension and its filename does NOT
/// contain any of the excluded substrings (`unins`, `crash`, `redist`, `dxsetup`,
/// `vc_redist`, `launcher`).
pub fn is_valid_game_executable(path: &Path) -> bool {
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
    !EXCLUDED_EXE_SUBSTRINGS
        .iter()
        .any(|pattern| lower.contains(pattern))
}

/// Scans user-provided custom folders for standalone `.exe` games, excluding installers,
/// crash handlers, redistributables, and secondary launchers.
pub fn scan_custom_folders(
    custom_folders: &[String],
    known_install_dirs: &[String],
) -> SourceScanOutput {
    let mut output = SourceScanOutput::default();
    let known_normalized: HashSet<String> = known_install_dirs
        .iter()
        .map(|p| normalize_path_key(p))
        .collect();

    let mut seen_exe_keys = HashSet::new();

    for raw_folder in custom_folders {
        let trimmed = raw_folder.trim();
        if trimmed.is_empty() {
            continue;
        }
        let root = PathBuf::from(trimmed);
        if !root.exists() {
            output.warnings.push(format!(
                "La cartella personalizzata '{}' non esiste o non è raggiungibile.",
                root.display()
            ));
            continue;
        }
        if !root.is_dir() {
            output.warnings.push(format!(
                "Il percorso '{}' non è una cartella valida.",
                root.display()
            ));
            continue;
        }

        let mut discovered_exes: Vec<PathBuf> = Vec::new();

        for entry in WalkDir::new(&root)
            .max_depth(5)
            .follow_links(false)
            .into_iter()
            .filter_map(Result::ok)
        {
            if !entry.file_type().is_file() {
                continue;
            }
            let path = entry.path();
            if !is_valid_game_executable(path) {
                continue;
            }

            // Skip executables located inside an already discovered Steam/Epic install dir
            let path_key = normalize_path_key(&path.to_string_lossy());
            if known_normalized
                .iter()
                .any(|known| !known.is_empty() && path_key.starts_with(known))
            {
                continue;
            }

            discovered_exes.push(path.to_path_buf());
        }

        discovered_exes.sort();

        // Count how many valid executables exist per top-level child directory of `root`
        let mut group_counts: HashMap<Option<String>, usize> = HashMap::new();
        for exe in &discovered_exes {
            let top_group = first_subdir_under(&root, exe);
            *group_counts.entry(top_group).or_insert(0) += 1;
        }

        let total_in_root = discovered_exes.len();

        for exe in discovered_exes {
            let exe_str = exe.to_string_lossy().to_string();
            let exe_key = normalize_path_key(&exe_str);
            if !seen_exe_keys.insert(exe_key.clone()) {
                continue;
            }

            let top_group = first_subdir_under(&root, &exe);
            let group_count = group_counts.get(&top_group).copied().unwrap_or(0);

            let (title, install_dir) = derive_standalone_title_and_dir(
                &root,
                &exe,
                top_group.as_deref(),
                group_count,
                total_in_root,
            );

            let id = format!("standalone:{:016x}", fnv1a_64(exe_key.as_bytes()));
            let clean = clean_game_title(&title);

            output.games.push(DetectedGame::new_detected(
                id,
                title,
                clean,
                GamePlatform::Standalone,
                None,
                install_dir.to_string_lossy().to_string(),
                Some(exe_str),
                None,
            ));
        }
    }

    output
}

fn first_subdir_under(root: &Path, file: &Path) -> Option<String> {
    let rel = file.strip_prefix(root).ok()?;
    let mut comps = rel.components();
    let first = comps.next()?;
    // Ensure there is at least one more component (the filename itself), meaning `first` is a directory
    comps.next()?;
    if let Component::Normal(os) = first {
        os.to_str().map(|s| s.to_string())
    } else {
        None
    }
}

fn derive_standalone_title_and_dir(
    root: &Path,
    exe: &Path,
    top_group: Option<&str>,
    group_count: usize,
    total_in_root: usize,
) -> (String, PathBuf) {
    let parent_dir = exe.parent().unwrap_or(root).to_path_buf();

    if let Some(sub) = top_group {
        let is_generic = GENERIC_BIN_DIRS
            .iter()
            .any(|g| g.eq_ignore_ascii_case(sub));

        if !is_generic && group_count == 1 {
            // E.g. D:\Games\Hollow Knight\hollow_knight.exe -> "Hollow Knight"
            return (prettify_name(sub), root.join(sub));
        }

        if is_generic && total_in_root == 1 {
            // User added the game folder directly, e.g. D:\Games\Witcher 3\bin\x64\witcher3.exe
            if let Some(root_name) = root.file_name().and_then(|n| n.to_str()) {
                return (prettify_name(root_name), root.to_path_buf());
            }
        }
    } else if total_in_root == 1 {
        // Single exe directly inside `root`: if stem matches a generic shipping name, fallback to root name
        if let Some(stem) = exe.file_stem().and_then(|s| s.to_str()) {
            let pretty = prettify_exe_stem(stem);
            if !pretty.is_empty() {
                return (pretty, parent_dir);
            }
        }
    }

    let fallback_title = exe
        .file_stem()
        .and_then(|s| s.to_str())
        .map(prettify_exe_stem)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "Gioco Locale".to_string());

    (fallback_title, parent_dir)
}

fn prettify_exe_stem(stem: &str) -> String {
    let mut working = stem.to_string();
    for unreal_suffix in ["-Win64-Shipping", "-Win32-Shipping", "_Win64_Shipping"] {
        if working.len() > unreal_suffix.len()
            && working
                .to_ascii_lowercase()
                .ends_with(&unreal_suffix.to_ascii_lowercase())
        {
            working.truncate(working.len() - unreal_suffix.len());
            break;
        }
    }
    prettify_name(&working)
}

fn prettify_name(name: &str) -> String {
    let replaced = if !name.contains(' ') && name.contains('_') {
        name.replace('_', " ")
    } else {
        name.to_string()
    };
    replaced.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn normalize_path_key(path: &str) -> String {
    path.replace('\\', "/")
        .trim_end_matches('/')
        .to_lowercase()
}

fn fnv1a_64(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for &b in bytes {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn filters_all_excluded_executable_patterns() {
        assert!(is_valid_game_executable(Path::new("HollowKnight.exe")));
        assert!(is_valid_game_executable(Path::new("Celeste.EXE")));

        // Excluded substrings (case-insensitive)
        assert!(!is_valid_game_executable(Path::new("unins000.exe")));
        assert!(!is_valid_game_executable(Path::new(
            "UnityCrashHandler64.exe"
        )));
        assert!(!is_valid_game_executable(Path::new("UEPrereqSetup_redist.exe")));
        assert!(!is_valid_game_executable(Path::new("DXSETUP.exe")));
        assert!(!is_valid_game_executable(Path::new("VC_redist.x64.exe")));
        assert!(!is_valid_game_executable(Path::new("GameLauncher.exe")));

        // Non-executables
        assert!(!is_valid_game_executable(Path::new("readme.txt")));
        assert!(!is_valid_game_executable(Path::new("steam_api64.dll")));
    }

    #[test]
    fn scans_custom_folder_and_excludes_helper_binaries() {
        let tmp = tempdir().expect("tempdir");
        let games_dir = tmp.path().join("StandaloneGames");

        let hk_dir = games_dir.join("Hollow Knight");
        fs::create_dir_all(&hk_dir).unwrap();
        fs::write(hk_dir.join("hollow_knight.exe"), b"mock").unwrap();
        fs::write(hk_dir.join("UnityCrashHandler64.exe"), b"mock").unwrap();
        fs::write(hk_dir.join("unins000.exe"), b"mock").unwrap();

        let celeste_dir = games_dir.join("Celeste");
        fs::create_dir_all(celeste_dir.join("_CommonRedist")).unwrap();
        fs::write(celeste_dir.join("Celeste.exe"), b"mock").unwrap();
        fs::write(
            celeste_dir.join("_CommonRedist").join("vc_redist.x64.exe"),
            b"mock",
        )
        .unwrap();
        fs::write(
            celeste_dir.join("_CommonRedist").join("DXSETUP.exe"),
            b"mock",
        )
        .unwrap();
        fs::write(celeste_dir.join("Launcher.exe"), b"mock").unwrap();

        let out = scan_custom_folders(&[games_dir.to_string_lossy().to_string()], &[]);
        assert!(out.warnings.is_empty(), "warnings: {:?}", out.warnings);
        assert_eq!(out.games.len(), 2);

        let titles: Vec<&str> = out.games.iter().map(|g| g.title.as_str()).collect();
        assert!(titles.contains(&"Hollow Knight"));
        assert!(titles.contains(&"Celeste"));
        assert!(out
            .games
            .iter()
            .all(|g| g.platform == GamePlatform::Standalone));
    }
}
