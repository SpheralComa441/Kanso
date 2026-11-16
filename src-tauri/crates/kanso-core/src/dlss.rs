//! Which version of NVIDIA DLSS a game carries, read from the game's own files.
//!
//! A game that supports DLSS ships NVIDIA's libraries in its folder: `nvngx_dlss.dll` (Super
//! Resolution), `nvngx_dlssg.dll` (Frame Generation) and `nvngx_dlssd.dll` (Ray
//! Reconstruction). Kanso looks for those three names in the game's folder and reads the
//! version each file declares. It only reads: nothing is downloaded, replaced or written, and
//! nothing is sent anywhere.

use crate::pe_version::file_version;
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::Path;

/// How deep below the game's folder the libraries are looked for: engines keep them a few
/// folders down (`Engine/Plugins/Runtime/Nvidia/DLSS/Binaries/ThirdParty/Win64`).
const MAX_DEPTH: usize = 9;
/// Most folder entries looked at for one game: a folder that is not a game's (a whole disk
/// added by hand) is given up on rather than walked for minutes.
const MAX_ENTRIES: usize = 60_000;

/// One of the three parts of DLSS, each a library of its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DlssPart {
    SuperResolution,
    FrameGeneration,
    RayReconstruction,
}

impl DlssPart {
    /// The part a file name is, whatever its case.
    pub fn of_file(name: &str) -> Option<Self> {
        match name.to_ascii_lowercase().as_str() {
            "nvngx_dlss.dll" => Some(Self::SuperResolution),
            "nvngx_dlssg.dll" => Some(Self::FrameGeneration),
            "nvngx_dlssd.dll" => Some(Self::RayReconstruction),
            _ => None,
        }
    }
}

/// A part of DLSS found in a game, at one version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DlssLibrary {
    pub part: DlssPart,
    /// As the file declares it: `3.7.10`, `310.2.1`.
    pub version: String,
    /// How many copies at this version the game carries (some keep one per executable).
    pub copies: u32,
}

/// The DLSS libraries in a game's folder: each part, at each version found, newest first.
/// Empty for a game without DLSS, for a folder that is not there, or when the folder is too
/// large to be a game's.
pub fn find_dlss(install_dir: &Path) -> Vec<DlssLibrary> {
    let mut found: BTreeMap<(DlssPart, [u16; 4]), u32> = BTreeMap::new();
    let mut budget = MAX_ENTRIES;
    let mut pending = vec![(install_dir.to_path_buf(), 0usize)];
    while let Some((dir, depth)) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            if budget == 0 {
                return Vec::new();
            }
            budget -= 1;
            // What the entry itself is: a link to a folder elsewhere is not followed
            let Ok(kind) = entry.file_type() else { continue };
            if kind.is_dir() {
                if depth < MAX_DEPTH {
                    pending.push((entry.path(), depth + 1));
                }
            } else if kind.is_file() {
                let Some(part) = entry.file_name().to_str().and_then(DlssPart::of_file) else { continue };
                if let Some(version) = file_version(&entry.path()).filter(|version| version.is_set()) {
                    *found.entry((part, version.0)).or_insert(0) += 1;
                }
            }
        }
    }
    let mut libraries: Vec<DlssLibrary> = found
        .into_iter()
        .map(|((part, version), copies)| DlssLibrary {
            part,
            version: crate::pe_version::FileVersion(version).short(),
            copies,
        })
        .collect();
    // Part by part, the newest version first
    libraries.reverse();
    libraries.sort_by_key(|library| library.part);
    libraries
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pe_version::tests::program_with_version;

    fn write_versioned_file(path: &Path, version: [u16; 4]) {
        std::fs::write(path, program_with_version(version, true)).unwrap();
    }

    #[test]
    fn names_the_three_parts_and_nothing_else() {
        assert_eq!(DlssPart::of_file("nvngx_dlss.dll"), Some(DlssPart::SuperResolution));
        assert_eq!(DlssPart::of_file("NVNGX_DLSSG.DLL"), Some(DlssPart::FrameGeneration));
        assert_eq!(DlssPart::of_file("nvngx_dlssd.dll"), Some(DlssPart::RayReconstruction));
        assert_eq!(DlssPart::of_file("nvngx_dlss.dll.bak"), None);
        assert_eq!(DlssPart::of_file("libxess.dll"), None);
    }

    #[test]
    fn finds_the_libraries_of_a_game_with_their_versions() {
        let game = tempfile::tempdir().unwrap();
        let deep = game.path().join("Engine/Plugins/Runtime/Nvidia/DLSS/Binaries/ThirdParty/Win64");
        std::fs::create_dir_all(&deep).unwrap();
        std::fs::create_dir_all(game.path().join("bin/x64")).unwrap();
        write_versioned_file(&deep.join("nvngx_dlss.dll"), [3, 7, 10, 0]);
        write_versioned_file(&game.path().join("bin/x64/nvngx_dlss.dll"), [3, 7, 10, 0]);
        write_versioned_file(&game.path().join("bin/x64/NVNGX_DLSSG.dll"), [310, 2, 1, 0]);
        write_versioned_file(&game.path().join("nvngx_dlss.dll"), [2, 4, 0, 0]);
        write_versioned_file(&game.path().join("game.exe"), [1, 0, 0, 0]);
        // Not a library at all: no version, not listed
        std::fs::write(game.path().join("bin/nvngx_dlssd.dll"), b"not a program").unwrap();

        assert_eq!(
            find_dlss(game.path()),
            [
                DlssLibrary { part: DlssPart::SuperResolution, version: "3.7.10".into(), copies: 2 },
                DlssLibrary { part: DlssPart::SuperResolution, version: "2.4".into(), copies: 1 },
                DlssLibrary { part: DlssPart::FrameGeneration, version: "310.2.1".into(), copies: 1 },
            ]
        );
        // Nothing was touched
        assert_eq!(std::fs::read(game.path().join("bin/nvngx_dlssd.dll")).unwrap(), b"not a program");
    }

    #[test]
    fn a_game_without_dlss_or_without_a_folder_has_none() {
        let game = tempfile::tempdir().unwrap();
        std::fs::write(game.path().join("game.exe"), b"x").unwrap();
        assert!(find_dlss(game.path()).is_empty());
        assert!(find_dlss(&game.path().join("gone")).is_empty());
        assert!(find_dlss(Path::new("")).is_empty());
    }

    #[test]
    fn libraries_buried_too_deep_are_not_looked_for() {
        let game = tempfile::tempdir().unwrap();
        let mut dir = game.path().to_path_buf();
        for level in 0..=MAX_DEPTH {
            dir = dir.join(format!("d{level}"));
        }
        std::fs::create_dir_all(&dir).unwrap();
        write_versioned_file(&dir.join("nvngx_dlss.dll"), [3, 5, 0, 0]);
        assert!(find_dlss(game.path()).is_empty());
    }
}
