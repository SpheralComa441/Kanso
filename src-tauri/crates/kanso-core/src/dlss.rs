//! NVIDIA DLSS in a game: which version it carries, and bringing it up to date.
//!
//! A game that supports DLSS ships NVIDIA's libraries in its folder: `nvngx_dlss.dll` (Super
//! Resolution), `nvngx_dlssg.dll` (Frame Generation) and `nvngx_dlssd.dll` (Ray
//! Reconstruction). Kanso looks for those three names in the game's folder and reads the
//! version each file declares. That part only reads, and contacts nobody.
//!
//! On the user's request a library can be replaced with a newer one: taken from another of
//! the user's games, from a file the user chose, or, when the user turned that on, downloaded
//! from NVIDIA's own public repository ([`NvidiaDlss`]). Kanso ships no library and hosts
//! none: the files are NVIDIA's, under NVIDIA's licence. The original is kept beside the new
//! file (`<name>.kanso-original`) and put back by [`restore`]; it is never overwritten by a
//! later replacement, so what comes back is always what the game shipped with.

use crate::pe_version::{file_version, FileVersion};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// How deep below the game's folder the libraries are looked for: engines keep them a few
/// folders down (`Engine/Plugins/Runtime/Nvidia/DLSS/Binaries/ThirdParty/Win64`).
const MAX_DEPTH: usize = 9;
/// Most folder entries looked at for one game: a folder that is not a game's (a whole disk
/// added by hand) is given up on rather than walked for minutes.
const MAX_ENTRIES: usize = 60_000;
/// What the original library is kept as, beside the one that replaced it.
pub const BACKUP_SUFFIX: &str = ".kanso-original";
/// What a library is written as before it takes the place of the old one.
const INCOMING_SUFFIX: &str = ".kanso-new";
/// The largest library accepted from the network: NVIDIA's are under 100 MB.
const MAX_DOWNLOAD_BYTES: u64 = 300 * 1024 * 1024;

const NVIDIA_REFS_URL: &str = "https://github.com/NVIDIA/DLSS.git/info/refs?service=git-upload-pack";
const NVIDIA_RAW_BASE_URL: &str = "https://raw.githubusercontent.com/NVIDIA/DLSS";
/// Where NVIDIA publishes the libraries, for the credit shown next to them.
pub const NVIDIA_DLSS_PAGE: &str = "https://github.com/NVIDIA/DLSS";

/// One of the three parts of DLSS, each a library of its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DlssPart {
    SuperResolution,
    FrameGeneration,
    RayReconstruction,
}

impl DlssPart {
    pub const ALL: [DlssPart; 3] = [DlssPart::SuperResolution, DlssPart::FrameGeneration, DlssPart::RayReconstruction];

    /// The part a file name is, whatever its case.
    pub fn of_file(name: &str) -> Option<Self> {
        match name.to_ascii_lowercase().as_str() {
            "nvngx_dlss.dll" => Some(Self::SuperResolution),
            "nvngx_dlssg.dll" => Some(Self::FrameGeneration),
            "nvngx_dlssd.dll" => Some(Self::RayReconstruction),
            _ => None,
        }
    }

    /// The name of the part's library.
    pub fn file_name(self) -> &'static str {
        match self {
            Self::SuperResolution => "nvngx_dlss.dll",
            Self::FrameGeneration => "nvngx_dlssg.dll",
            Self::RayReconstruction => "nvngx_dlssd.dll",
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
    /// Kanso replaced a copy at this version and keeps the original beside it.
    pub replaced: bool,
}

/// One library file of a game.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DlssFile {
    pub part: DlssPart,
    pub version: FileVersion,
    pub path: PathBuf,
}

/// What a game's folder holds of DLSS: the libraries, and the originals Kanso kept.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DlssFiles {
    pub libraries: Vec<DlssFile>,
    /// Each original kept: the part, and the file (`<library>.kanso-original`).
    pub originals: Vec<(DlssPart, PathBuf)>,
}

/// The file an original is kept as, beside its library.
pub fn original_path(library: &Path) -> PathBuf {
    let mut name = library.file_name().unwrap_or_default().to_os_string();
    name.push(BACKUP_SUFFIX);
    library.with_file_name(name)
}

/// Looks through a game's folder for the three libraries and the originals kept beside them.
/// Empty for a game without DLSS, for a folder that is not there, or when the folder is too
/// large to be a game's.
pub fn find_dlss_files(install_dir: &Path) -> DlssFiles {
    let mut found = DlssFiles::default();
    if install_dir.as_os_str().is_empty() {
        return found;
    }
    let mut budget = MAX_ENTRIES;
    let mut pending = vec![(install_dir.to_path_buf(), 0usize)];
    while let Some((dir, depth)) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            if budget == 0 {
                return DlssFiles::default();
            }
            budget -= 1;
            // What the entry itself is: a link to a folder elsewhere is not followed
            let Ok(kind) = entry.file_type() else { continue };
            if kind.is_dir() {
                if depth < MAX_DEPTH {
                    pending.push((entry.path(), depth + 1));
                }
            } else if kind.is_file() {
                let name = entry.file_name();
                let Some(name) = name.to_str() else { continue };
                if let Some(part) = DlssPart::of_file(name) {
                    if let Some(version) = file_version(&entry.path()).filter(FileVersion::is_set) {
                        found.libraries.push(DlssFile { part, version, path: entry.path() });
                    }
                } else if let Some(part) = name
                    .to_ascii_lowercase()
                    .strip_suffix(BACKUP_SUFFIX)
                    .and_then(DlssPart::of_file)
                {
                    found.originals.push((part, entry.path()));
                }
            }
        }
    }
    // The same order on every run, whatever order the disk lists its entries in
    found.libraries.sort_by(|a, b| a.path.cmp(&b.path));
    found.originals.sort();
    found
}

/// The DLSS libraries in a game's folder: each part, at each version found, newest first.
pub fn find_dlss(install_dir: &Path) -> Vec<DlssLibrary> {
    summarize(&find_dlss_files(install_dir))
}

/// What [`find_dlss_files`] found, as the game's sheet shows it.
pub fn summarize(files: &DlssFiles) -> Vec<DlssLibrary> {
    let mut found: BTreeMap<(DlssPart, FileVersion), (u32, bool)> = BTreeMap::new();
    for file in &files.libraries {
        let entry = found.entry((file.part, file.version)).or_insert((0, false));
        entry.0 += 1;
        entry.1 |= files.originals.iter().any(|(_, original)| *original == original_path(&file.path));
    }
    let mut libraries: Vec<DlssLibrary> = found
        .into_iter()
        .map(|((part, version), (copies, replaced))| DlssLibrary { part, version: version.short(), copies, replaced })
        .collect();
    // Part by part, the newest version first
    libraries.reverse();
    libraries.sort_by_key(|library| library.part);
    libraries
}

/// The newest copy of a part among a game's files, if it has the part.
pub fn newest_of(files: &DlssFiles, part: DlssPart) -> Option<&DlssFile> {
    files.libraries.iter().filter(|file| file.part == part).max_by_key(|file| file.version)
}

/// Why a library was not replaced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DlssError {
    /// The file offered is not a library with a version.
    NotALibrary,
    /// The game has no copy of the part older than the one offered.
    NothingOlder,
    /// The game's file could not be written: the game is running, or the folder is protected.
    Busy(String),
}

/// Replaces every copy of `part` in a game that is older than `source` with `source`, keeping
/// each original beside it. Copies of DLSS 1, which newer libraries do not stand in for, and
/// copies already as new are left alone. Returns how many copies were replaced.
///
/// Each copy is replaced whole or not at all: the new file is written beside the old one and
/// takes its place in one step. If that fails nothing of the game is changed.
pub fn replace_part(files: &DlssFiles, part: DlssPart, source: &Path) -> Result<u32, DlssError> {
    let new_version = file_version(source).filter(FileVersion::is_set).ok_or(DlssError::NotALibrary)?;
    let older: Vec<&DlssFile> = files
        .libraries
        .iter()
        .filter(|file| file.part == part && file.version < new_version && file.version.0[0] != 1 && file.path != source)
        .collect();
    if older.is_empty() {
        return Err(DlssError::NothingOlder);
    }
    let mut replaced = 0;
    for file in older {
        let original = original_path(&file.path);
        let incoming = {
            let mut name = file.path.file_name().unwrap_or_default().to_os_string();
            name.push(INCOMING_SUFFIX);
            file.path.with_file_name(name)
        };
        // The original is kept once: a second replacement must not put a replaced file in its place
        let kept_now = !original.exists();
        let outcome = (|| -> std::io::Result<()> {
            if kept_now {
                std::fs::copy(&file.path, &original)?;
            }
            std::fs::copy(source, &incoming)?;
            std::fs::rename(&incoming, &file.path)
        })();
        if let Err(error) = outcome {
            // Left as found: no half-written file, no original of a library that was not replaced
            let _ = std::fs::remove_file(&incoming);
            if kept_now {
                let _ = std::fs::remove_file(&original);
            }
            return Err(DlssError::Busy(error.to_string()));
        }
        replaced += 1;
    }
    Ok(replaced)
}

/// Puts back the originals Kanso kept, for one part or for all. Returns how many came back.
pub fn restore(files: &DlssFiles, part: Option<DlssPart>) -> Result<u32, DlssError> {
    let mut restored = 0;
    for (kept_part, original) in &files.originals {
        if part.is_some_and(|wanted| wanted != *kept_part) {
            continue;
        }
        // "<library>.kanso-original" goes back to "<library>"
        let Some(library) = original
            .file_name()
            .and_then(|name| name.to_str())
            .and_then(|name| name.get(..name.len() - BACKUP_SUFFIX.len()))
            .map(|name| original.with_file_name(name))
        else {
            continue;
        };
        std::fs::rename(original, &library).map_err(|error| DlssError::Busy(error.to_string()))?;
        restored += 1;
    }
    Ok(restored)
}

/// The newest version among the tags of NVIDIA's repository, from what its git server lists
/// (`refs/tags/v310.9.16`). Anything that is not a tag of three numbers is ignored.
pub fn parse_latest_tag(advertisement: &[u8]) -> Option<FileVersion> {
    String::from_utf8_lossy(advertisement)
        .split(|c: char| c.is_whitespace() || c == '\0')
        .filter_map(|word| word.strip_prefix("refs/tags/v"))
        .filter_map(|tag| {
            let numbers: Vec<u16> = tag.trim_end_matches("^{}").split('.').map(|part| part.parse().ok()).collect::<Option<_>>()?;
            <[u16; 3]>::try_from(numbers).ok().map(|[major, minor, build]| FileVersion([major, minor, build, 0]))
        })
        .max()
}

/// NVIDIA's public repository of the DLSS libraries, read without an account or a key. Two
/// kinds of request, neither saying anything of the user or of their games: the list of the
/// repository's tags, and a library file at a tag.
pub struct NvidiaDlss {
    refs_url: String,
    raw_base_url: String,
    agent: ureq::Agent,
}

impl Default for NvidiaDlss {
    fn default() -> Self {
        Self::new()
    }
}

impl NvidiaDlss {
    pub fn new() -> Self {
        Self::with_urls(NVIDIA_REFS_URL, NVIDIA_RAW_BASE_URL)
    }

    pub fn with_urls(refs_url: &str, raw_base_url: &str) -> Self {
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_secs(10))
            .timeout_read(Duration::from_secs(60))
            .user_agent(crate::USER_AGENT)
            .build();
        Self { refs_url: refs_url.to_string(), raw_base_url: raw_base_url.trim_end_matches('/').to_string(), agent }
    }

    /// The newest version NVIDIA has published.
    pub fn latest(&self) -> Result<FileVersion, String> {
        let response = self.agent.get(&self.refs_url).call().map_err(|e| e.to_string())?;
        let mut listing = Vec::new();
        response.into_reader().take(1024 * 1024).read_to_end(&mut listing).map_err(|e| e.to_string())?;
        parse_latest_tag(&listing).ok_or_else(|| "no version listed".to_string())
    }

    /// The address of a part's library at a version.
    pub fn url_of(&self, version: FileVersion, part: DlssPart) -> String {
        let [major, minor, build, _] = version.0;
        format!("{}/v{major}.{minor}.{build}/lib/Windows_x86_64/rel/{}", self.raw_base_url, part.file_name())
    }

    /// Downloads a part's library at `version` to `destination`. The file is written beside
    /// its destination first and takes its name only once it is whole and declares the
    /// version asked for; anything else is thrown away.
    pub fn download(&self, version: FileVersion, part: DlssPart, destination: &Path) -> Result<(), String> {
        let response = self.agent.get(&self.url_of(version, part)).call().map_err(|e| e.to_string())?;
        let mut name = destination.file_name().unwrap_or_default().to_os_string();
        name.push(".part");
        let incoming = destination.with_file_name(name);
        let outcome = (|| -> Result<(), String> {
            if let Some(parent) = destination.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            let mut file = std::fs::File::create(&incoming).map_err(|e| e.to_string())?;
            let written = std::io::copy(&mut response.into_reader().take(MAX_DOWNLOAD_BYTES + 1), &mut file).map_err(|e| e.to_string())?;
            file.flush().map_err(|e| e.to_string())?;
            drop(file);
            if written > MAX_DOWNLOAD_BYTES {
                return Err("file too large".to_string());
            }
            if file_version(&incoming) != Some(version) {
                return Err("not the version asked for".to_string());
            }
            std::fs::rename(&incoming, destination).map_err(|e| e.to_string())
        })();
        if outcome.is_err() {
            let _ = std::fs::remove_file(&incoming);
        }
        outcome
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pe_version::tests::program_with_version;
    use std::io::{BufRead, BufReader};
    use std::net::TcpListener;

    fn write_versioned_file(path: &Path, version: [u16; 4]) {
        std::fs::write(path, program_with_version(version, true)).unwrap();
    }

    fn version_at(path: &Path) -> Option<[u16; 4]> {
        file_version(path).map(|version| version.0)
    }

    fn library(part: DlssPart, version: &str, copies: u32, replaced: bool) -> DlssLibrary {
        DlssLibrary { part, version: version.into(), copies, replaced }
    }

    #[test]
    fn names_the_three_parts_and_nothing_else() {
        assert_eq!(DlssPart::of_file("nvngx_dlss.dll"), Some(DlssPart::SuperResolution));
        assert_eq!(DlssPart::of_file("NVNGX_DLSSG.DLL"), Some(DlssPart::FrameGeneration));
        assert_eq!(DlssPart::of_file("nvngx_dlssd.dll"), Some(DlssPart::RayReconstruction));
        assert_eq!(DlssPart::of_file("nvngx_dlss.dll.bak"), None);
        assert_eq!(DlssPart::of_file("libxess.dll"), None);
        for part in DlssPart::ALL {
            assert_eq!(DlssPart::of_file(part.file_name()), Some(part));
        }
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
                library(DlssPart::SuperResolution, "3.7.10", 2, false),
                library(DlssPart::SuperResolution, "2.4", 1, false),
                library(DlssPart::FrameGeneration, "310.2.1", 1, false),
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

    #[test]
    fn replaces_older_copies_keeping_the_originals_and_puts_them_back() {
        let game = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(game.path().join("bin")).unwrap();
        let root_copy = game.path().join("nvngx_dlss.dll");
        let bin_copy = game.path().join("bin/nvngx_dlss.dll");
        let frame_generation = game.path().join("bin/nvngx_dlssg.dll");
        write_versioned_file(&root_copy, [3, 5, 0, 0]);
        write_versioned_file(&bin_copy, [310, 9, 16, 0]);
        write_versioned_file(&frame_generation, [3, 5, 0, 0]);
        let newer = other.path().join("nvngx_dlss.dll");
        write_versioned_file(&newer, [310, 2, 1, 0]);
        let original_bytes = std::fs::read(&root_copy).unwrap();

        // Only the copy older than the one offered, and only of that part
        assert_eq!(replace_part(&find_dlss_files(game.path()), DlssPart::SuperResolution, &newer), Ok(1));
        assert_eq!(version_at(&root_copy), Some([310, 2, 1, 0]));
        assert_eq!(version_at(&bin_copy), Some([310, 9, 16, 0]), "a copy already newer is left alone");
        assert_eq!(version_at(&frame_generation), Some([3, 5, 0, 0]), "another part is not touched");
        assert_eq!(std::fs::read(original_path(&root_copy)).unwrap(), original_bytes, "the original is kept beside it");
        assert!(!original_path(&bin_copy).exists());
        assert_eq!(
            find_dlss(game.path()),
            [
                library(DlssPart::SuperResolution, "310.9.16", 1, false),
                library(DlssPart::SuperResolution, "310.2.1", 1, true),
                library(DlssPart::FrameGeneration, "3.5", 1, false),
            ],
            "the sheet says which copy was replaced"
        );

        // Replaced again with something newer still: the original kept is still the game's own
        let newest = other.path().join("newest.dll");
        write_versioned_file(&newest, [310, 9, 16, 0]);
        assert_eq!(replace_part(&find_dlss_files(game.path()), DlssPart::SuperResolution, &newest), Ok(1));
        assert_eq!(version_at(&root_copy), Some([310, 9, 16, 0]));
        assert_eq!(std::fs::read(original_path(&root_copy)).unwrap(), original_bytes);
        // Nothing older left
        assert_eq!(replace_part(&find_dlss_files(game.path()), DlssPart::SuperResolution, &newest), Err(DlssError::NothingOlder));

        // Back to what the game shipped with
        assert_eq!(restore(&find_dlss_files(game.path()), Some(DlssPart::FrameGeneration)), Ok(0));
        assert_eq!(restore(&find_dlss_files(game.path()), None), Ok(1));
        assert_eq!(std::fs::read(&root_copy).unwrap(), original_bytes);
        assert!(!original_path(&root_copy).exists());
        assert!(find_dlss(game.path()).iter().all(|library| !library.replaced));
        // No stray file is left in the game's folder
        let names: Vec<String> = std::fs::read_dir(game.path()).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        assert!(names.iter().all(|name| !name.contains("kanso")), "{names:?}");
    }

    #[test]
    fn what_cannot_stand_in_is_refused_and_nothing_changes() {
        let game = tempfile::tempdir().unwrap();
        let first = game.path().join("nvngx_dlss.dll");
        write_versioned_file(&first, [1, 0, 13, 0]);
        let other = tempfile::tempdir().unwrap();
        let newer = other.path().join("nvngx_dlss.dll");
        write_versioned_file(&newer, [3, 7, 0, 0]);
        // DLSS 1 is a different thing: newer libraries do not replace it
        assert_eq!(replace_part(&find_dlss_files(game.path()), DlssPart::SuperResolution, &newer), Err(DlssError::NothingOlder));
        assert_eq!(version_at(&first), Some([1, 0, 13, 0]));

        write_versioned_file(&first, [2, 4, 0, 0]);
        let junk = other.path().join("junk.dll");
        std::fs::write(&junk, b"not a library").unwrap();
        assert_eq!(replace_part(&find_dlss_files(game.path()), DlssPart::SuperResolution, &junk), Err(DlssError::NotALibrary));
        assert_eq!(replace_part(&find_dlss_files(game.path()), DlssPart::SuperResolution, &other.path().join("gone.dll")), Err(DlssError::NotALibrary));
        // The game has no Frame Generation to bring up to date
        assert_eq!(replace_part(&find_dlss_files(game.path()), DlssPart::FrameGeneration, &newer), Err(DlssError::NothingOlder));
        assert_eq!(version_at(&first), Some([2, 4, 0, 0]));
        assert!(!original_path(&first).exists());
    }

    #[test]
    fn reads_the_newest_version_among_nvidias_tags() {
        // Cut from the real answer of the repository's git server
        let listing = b"001e# service=git-upload-pack\n000001599981c6350dc58c0ce21f024fd65e31e851c31689 HEAD\0multi_ack symref=HEAD:refs/heads/main agent=git/github\n\
            003d9981c6350dc58c0ce21f024fd65e31e851c31689 refs/heads/main\n003ee1dcfa33048f31ce41c104a040d9e9629809f916 refs/pull/1/head\n\
            003eb9426cbe3e1acba28794e20ad1601ba0000000 refs/tags/v3.7.20\n0040aaaa refs/tags/v310.9.1\n0043bbbb refs/tags/v310.9.1^{}\n\
            0041cccc refs/tags/v310.9.16\n0044dddd refs/tags/v310.9.16^{}\n0040eeee refs/tags/v310.7.0\n0040ffff refs/tags/vnext\n0040ffff refs/tags/v999.1\n0000";
        assert_eq!(parse_latest_tag(listing), Some(FileVersion([310, 9, 16, 0])));
        assert_eq!(parse_latest_tag(b"<html>not a listing</html>"), None);
        assert_eq!(
            NvidiaDlss::new().url_of(FileVersion([310, 9, 16, 0]), DlssPart::FrameGeneration),
            "https://raw.githubusercontent.com/NVIDIA/DLSS/v310.9.16/lib/Windows_x86_64/rel/nvngx_dlssg.dll"
        );
    }

    /// Serves one file's bytes for every request, and says which paths were asked for.
    fn serve(bytes: Vec<u8>) -> (String, std::sync::mpsc::Receiver<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let (asked, paths) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            for mut stream in listener.incoming().flatten() {
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut line = String::new();
                let _ = reader.read_line(&mut line);
                let _ = asked.send(line.split_whitespace().nth(1).unwrap_or_default().to_string());
                loop {
                    let mut header = String::new();
                    if reader.read_line(&mut header).is_err() || header.trim().is_empty() {
                        break;
                    }
                }
                let _ = stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", bytes.len()).as_bytes());
                let _ = stream.write_all(&bytes);
            }
        });
        (base, paths)
    }

    #[test]
    fn a_download_is_kept_only_if_it_is_the_version_asked_for() {
        let (base, asked) = serve(program_with_version([310, 9, 16, 0], true));
        let cache = tempfile::tempdir().unwrap();
        let nvidia = NvidiaDlss::with_urls(&format!("{base}/refs"), &base);
        let destination = cache.path().join("v310.9.16/nvngx_dlssg.dll");
        assert_eq!(nvidia.download(FileVersion([310, 9, 16, 0]), DlssPart::FrameGeneration, &destination), Ok(()));
        assert_eq!(asked.recv().unwrap(), "/v310.9.16/lib/Windows_x86_64/rel/nvngx_dlssg.dll", "only the file's address is asked for");
        assert_eq!(version_at(&destination), Some([310, 9, 16, 0]));

        // The server hands over something else: nothing is kept
        let wrong = cache.path().join("v310.9.17/nvngx_dlss.dll");
        assert!(nvidia.download(FileVersion([310, 9, 17, 0]), DlssPart::SuperResolution, &wrong).is_err());
        assert!(!wrong.exists());
        assert_eq!(std::fs::read_dir(wrong.parent().unwrap()).unwrap().count(), 0, "no half file is left behind");
        // Nobody answers
        assert!(NvidiaDlss::with_urls("http://127.0.0.1:9/refs", "http://127.0.0.1:9").latest().is_err());
    }
}
