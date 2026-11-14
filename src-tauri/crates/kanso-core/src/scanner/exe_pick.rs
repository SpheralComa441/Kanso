//! Heuristic used by the registry-based scanners to find a game's main executable when the
//! store does not record it.

use std::fs;
use std::path::{Path, PathBuf};

/// Helper binaries that are never the game itself (case-insensitive substrings).
const NON_GAME_EXE_SUBSTRINGS: &[&str] = &[
    "unins",
    "crash",
    "redist",
    "setup",
    "install",
    "cleanup",
    "touchup",
    "activation",
    "anticheat",
    "easyanticheat",
    "battleye",
    "dxsetup",
    "prereq",
    "report",
    "updater",
    "diagnostic",
    "benchmark",
    "helper",
];

/// Returns `true` for an `.exe` that is plausibly a game binary rather than an installer,
/// crash reporter or other helper.
pub fn is_candidate_game_exe(path: &Path) -> bool {
    let is_exe = path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("exe"));
    if !is_exe {
        return false;
    }
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    let lower = name.to_ascii_lowercase();
    !NON_GAME_EXE_SUBSTRINGS
        .iter()
        .any(|pattern| lower.contains(pattern))
}

/// Picks the most likely main executable of the game installed in `install_dir`: the candidate
/// whose file name shares the most words with `title`, the biggest file breaking ties (the game
/// binary dwarfs its helpers). Looks in the folder itself, then one level down (`bin\`, `x64\`...).
pub fn pick_main_executable(install_dir: &Path, title: &str) -> Option<PathBuf> {
    let title_tokens: Vec<String> = title
        .split(|c: char| !c.is_alphanumeric())
        .filter(|token| token.len() >= 2)
        .map(|token| token.to_lowercase())
        .collect();

    if let Some(found) = best_exe_in_dir(install_dir, &title_tokens) {
        return Some(found);
    }

    let mut subdirs: Vec<PathBuf> = fs::read_dir(install_dir)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect();
    subdirs.sort();

    let mut best: Option<(ExeScore, PathBuf)> = None;
    for subdir in subdirs {
        for (score, path) in scored_exes_in_dir(&subdir, &title_tokens) {
            if best.as_ref().is_none_or(|(top, _)| score > *top) {
                best = Some((score, path));
            }
        }
    }
    best.map(|(_, path)| path)
}

/// (words shared with the title, file size)
type ExeScore = (usize, u64);

fn scored_exes_in_dir(dir: &Path, title_tokens: &[String]) -> Vec<(ExeScore, PathBuf)> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut scored: Vec<(ExeScore, PathBuf)> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_file() && is_candidate_game_exe(path))
        .map(|path| {
            let name = path
                .file_stem()
                .and_then(|n| n.to_str())
                .unwrap_or_default()
                .to_lowercase();
            let matches = title_tokens
                .iter()
                .filter(|token| name.contains(token.as_str()))
                .count();
            let size = fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            ((matches, size), path)
        })
        .collect();
    // Deterministic order for equal scores
    scored.sort_by(|a, b| a.1.cmp(&b.1));
    scored
}

fn best_exe_in_dir(dir: &Path, title_tokens: &[String]) -> Option<PathBuf> {
    let mut best: Option<(ExeScore, PathBuf)> = None;
    for (score, path) in scored_exes_in_dir(dir, title_tokens) {
        if best.as_ref().is_none_or(|(top, _)| score > *top) {
            best = Some((score, path));
        }
    }
    best.map(|(_, path)| path)
}

/// Lower-case, forward-slash, no-trailing-slash form of a path for comparisons.
pub fn normalize_dir_key(path: &str) -> String {
    path.trim()
        .replace('\\', "/")
        .trim_end_matches('/')
        .to_lowercase()
}

/// Lower-case ASCII slug for building stable ids out of display names.
pub fn slugify(name: &str) -> String {
    let mut slug = String::with_capacity(name.len());
    let mut last_was_dash = true;
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            last_was_dash = false;
        } else if !last_was_dash {
            slug.push('-');
            last_was_dash = true;
        }
    }
    slug.trim_end_matches('-').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn prefers_title_match_then_size_and_skips_helpers() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        fs::write(root.join("UnityCrashHandler64.exe"), vec![0u8; 900]).unwrap();
        fs::write(root.join("unins000.exe"), vec![0u8; 800]).unwrap();
        fs::write(root.join("Tool.exe"), vec![0u8; 500]).unwrap();
        fs::write(root.join("AnnoGame.exe"), vec![0u8; 100]).unwrap();
        fs::write(root.join("readme.txt"), b"hi").unwrap();

        // Title match wins over the bigger unrelated exe
        assert_eq!(
            pick_main_executable(root, "Anno 1800"),
            Some(root.join("AnnoGame.exe"))
        );
        // No word in common: the biggest candidate wins
        assert_eq!(
            pick_main_executable(root, "Qualcosa"),
            Some(root.join("Tool.exe"))
        );
    }

    #[test]
    fn falls_back_to_first_level_subfolders() {
        let dir = tempdir().unwrap();
        let bin = dir.path().join("bin");
        fs::create_dir_all(&bin).unwrap();
        fs::write(bin.join("game.exe"), vec![0u8; 10]).unwrap();
        fs::write(dir.path().join("vc_redist.x64.exe"), vec![0u8; 999]).unwrap();

        assert_eq!(
            pick_main_executable(dir.path(), "Whatever"),
            Some(bin.join("game.exe"))
        );
        assert_eq!(pick_main_executable(&dir.path().join("missing"), "x"), None);
    }

    #[test]
    fn slug_and_dir_key_are_stable() {
        assert_eq!(slugify("EA SPORTS FC™ 24"), "ea-sports-fc-24");
        assert_eq!(slugify("  --Dead Space!!  "), "dead-space");
        assert_eq!(normalize_dir_key(r"D:\Games\Apex\"), "d:/games/apex");
    }
}
