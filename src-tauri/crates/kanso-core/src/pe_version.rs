//! The file version of a Windows program or library, read from the file itself.
//!
//! Windows programs carry a version in a resource (`RT_VERSION`): four numbers, the ones the
//! file's Properties show. This module finds them by walking the file's own tables, with no
//! Windows API, so it reads the same on any system and can be tested anywhere. It only
//! reads: a few small pieces of the file, wherever the tables say they are.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// The four numbers of a file version: major, minor, build, private.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FileVersion(pub [u16; 4]);

impl FileVersion {
    /// `310.9.16.0`, as the numbers are.
    pub fn dotted(&self) -> String {
        let [major, minor, build, private] = self.0;
        format!("{major}.{minor}.{build}.{private}")
    }

    /// As a person writes it: trailing zeros beyond the second number are left out
    /// (`310.9.16`, `1.0`).
    pub fn short(&self) -> String {
        let mut parts: Vec<u16> = self.0.to_vec();
        while parts.len() > 2 && parts.last() == Some(&0) {
            parts.pop();
        }
        parts.iter().map(u16::to_string).collect::<Vec<_>>().join(".")
    }

    /// Whether it says anything: a file whose version was never set carries four zeros.
    pub fn is_set(&self) -> bool {
        self.0 != [0, 0, 0, 0]
    }
}

/// Largest piece of a file read in one go: tables are a few kilobytes.
const MAX_PIECE: usize = 64 * 1024;
/// The resource type of version information.
const RT_VERSION: u32 = 16;
const FIXED_INFO_SIGNATURE: u32 = 0xFEEF_04BD;

fn read_at(file: &mut File, offset: u64, length: usize) -> Option<Vec<u8>> {
    if length > MAX_PIECE {
        return None;
    }
    let mut buffer = vec![0u8; length];
    file.seek(SeekFrom::Start(offset)).ok()?;
    file.read_exact(&mut buffer).ok()?;
    Some(buffer)
}

fn u16_at(bytes: &[u8], offset: usize) -> Option<u16> {
    bytes.get(offset..offset + 2).map(|b| u16::from_le_bytes([b[0], b[1]]))
}

fn u32_at(bytes: &[u8], offset: usize) -> Option<u32> {
    bytes.get(offset..offset + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

/// A section of the file: where it is in memory and where on disk.
struct Section {
    virtual_address: u32,
    virtual_size: u32,
    raw_offset: u32,
    raw_size: u32,
}

/// The place in the file of an address the tables give as "in memory".
fn file_offset(sections: &[Section], address: u32) -> Option<u64> {
    sections.iter().find_map(|section| {
        let size = section.virtual_size.max(section.raw_size);
        let within = address.checked_sub(section.virtual_address)?;
        (within < size && within < section.raw_size).then(|| u64::from(section.raw_offset) + u64::from(within))
    })
}

/// The first entry of a resource directory, or the one with the given id.
fn directory_entry(file: &mut File, directory: u64, wanted: Option<u32>) -> Option<u32> {
    let header = read_at(file, directory, 16)?;
    let entries = usize::from(u16_at(&header, 12)?) + usize::from(u16_at(&header, 14)?);
    let table = read_at(file, directory + 16, entries.min(4096) * 8)?;
    table.chunks_exact(8).find_map(|entry| {
        let id = u32_at(entry, 0)?;
        let target = u32_at(entry, 4)?;
        match wanted {
            // Named entries have the high bit set: an id never does
            Some(wanted) => (id == wanted).then_some(target),
            None => Some(target),
        }
    })
}

/// The file version of the program or library at `path`; `None` when the file is not one, has
/// no version resource or cannot be read.
pub fn file_version(path: &Path) -> Option<FileVersion> {
    let mut file = File::open(path).ok()?;
    let dos = read_at(&mut file, 0, 64)?;
    if dos.get(..2)? != b"MZ" {
        return None;
    }
    let pe = u64::from(u32_at(&dos, 0x3C)?);
    let coff = read_at(&mut file, pe, 24)?;
    if coff.get(..4)? != b"PE\0\0" {
        return None;
    }
    let section_count = usize::from(u16_at(&coff, 6)?).min(96);
    let optional_size = usize::from(u16_at(&coff, 20)?);
    let optional = read_at(&mut file, pe + 24, optional_size)?;
    // Where the data directories start depends on the kind of program (32 or 64 bit)
    let directories = match u16_at(&optional, 0)? {
        0x10B => 96,
        0x20B => 112,
        _ => return None,
    };
    // The third directory is the resources'
    let resources_address = u32_at(&optional, directories + 16)?;
    if resources_address == 0 {
        return None;
    }

    let table = read_at(&mut file, pe + 24 + optional_size as u64, section_count * 40)?;
    let sections: Vec<Section> = table
        .chunks_exact(40)
        .filter_map(|section| {
            Some(Section {
                virtual_size: u32_at(section, 8)?,
                virtual_address: u32_at(section, 12)?,
                raw_size: u32_at(section, 16)?,
                raw_offset: u32_at(section, 20)?,
            })
        })
        .collect();
    let root = file_offset(&sections, resources_address)?;

    // Three levels: the type (version), then the first name, then the first language
    const SUBDIRECTORY: u32 = 0x8000_0000;
    let by_name = directory_entry(&mut file, root, Some(RT_VERSION))?;
    let by_language = directory_entry(&mut file, root + u64::from(by_name & !SUBDIRECTORY), None)?;
    let leaf = if by_language & SUBDIRECTORY != 0 {
        directory_entry(&mut file, root + u64::from(by_language & !SUBDIRECTORY), None)?
    } else {
        by_language
    };
    if by_name & SUBDIRECTORY == 0 || leaf & SUBDIRECTORY != 0 {
        return None;
    }
    let data = read_at(&mut file, root + u64::from(leaf), 16)?;
    let info_offset = file_offset(&sections, u32_at(&data, 0)?)?;
    let info_size = (u32_at(&data, 4)? as usize).min(1024);
    let info = read_at(&mut file, info_offset, info_size)?;

    // The fixed part follows the block's name, on a four-byte boundary
    let fixed = (0..info.len().saturating_sub(16)).step_by(4).find(|at| u32_at(&info, *at) == Some(FIXED_INFO_SIGNATURE))?;
    let most = u32_at(&info, fixed + 8)?;
    let least = u32_at(&info, fixed + 12)?;
    let version = FileVersion([(most >> 16) as u16, most as u16, (least >> 16) as u16, least as u16]);
    version.is_set().then_some(version)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::fs;

    /// A file laid out as a Windows program with nothing in it but a version resource.
    pub(crate) fn program_with_version(version: [u16; 4], sixty_four_bit: bool) -> Vec<u8> {
        let optional_size: usize = if sixty_four_bit { 240 } else { 224 };
        let pe = 0x80usize;
        let sections_at = pe + 24 + optional_size;
        let raw_offset = 0x400u32;
        let virtual_address = 0x3000u32;

        // The resources: type 16 → one name → one language → the data
        let mut resources = Vec::new();
        let directory = |id: u32, target: u32| -> Vec<u8> {
            let mut bytes = vec![0u8; 12];
            bytes.extend_from_slice(&0u16.to_le_bytes());
            bytes.extend_from_slice(&1u16.to_le_bytes());
            bytes.extend_from_slice(&id.to_le_bytes());
            bytes.extend_from_slice(&target.to_le_bytes());
            bytes
        };
        resources.extend(directory(16, 0x8000_0000 | 24));
        resources.extend(directory(1, 0x8000_0000 | 48));
        resources.extend(directory(0x409, 72));
        // The block of version information: its header, its name, then the fixed part
        let mut info = Vec::new();
        info.extend_from_slice(&0u16.to_le_bytes());
        info.extend_from_slice(&52u16.to_le_bytes());
        info.extend_from_slice(&0u16.to_le_bytes());
        for unit in "VS_VERSION_INFO\0".encode_utf16() {
            info.extend_from_slice(&unit.to_le_bytes());
        }
        while info.len() % 4 != 0 {
            info.push(0);
        }
        info.extend_from_slice(&FIXED_INFO_SIGNATURE.to_le_bytes());
        info.extend_from_slice(&0x0001_0000u32.to_le_bytes());
        info.extend_from_slice(&((u32::from(version[0]) << 16) | u32::from(version[1])).to_le_bytes());
        info.extend_from_slice(&((u32::from(version[2]) << 16) | u32::from(version[3])).to_le_bytes());
        info.extend_from_slice(&[0u8; 36]);
        let data_at = 72 + 16;
        resources.extend_from_slice(&(virtual_address + data_at).to_le_bytes());
        resources.extend_from_slice(&(info.len() as u32).to_le_bytes());
        resources.extend_from_slice(&[0u8; 8]);
        resources.extend(info);

        let mut file = vec![0u8; raw_offset as usize];
        file[..2].copy_from_slice(b"MZ");
        file[0x3C..0x40].copy_from_slice(&(pe as u32).to_le_bytes());
        file[pe..pe + 4].copy_from_slice(b"PE\0\0");
        file[pe + 6..pe + 8].copy_from_slice(&1u16.to_le_bytes());
        file[pe + 20..pe + 22].copy_from_slice(&(optional_size as u16).to_le_bytes());
        let optional = pe + 24;
        file[optional..optional + 2].copy_from_slice(&(if sixty_four_bit { 0x20Bu16 } else { 0x10B }).to_le_bytes());
        let directories = optional + if sixty_four_bit { 112 } else { 96 };
        file[directories + 16..directories + 20].copy_from_slice(&virtual_address.to_le_bytes());
        file[directories + 20..directories + 24].copy_from_slice(&(resources.len() as u32).to_le_bytes());
        file[sections_at..sections_at + 5].copy_from_slice(b".rsrc");
        file[sections_at + 8..sections_at + 12].copy_from_slice(&(resources.len() as u32).to_le_bytes());
        file[sections_at + 12..sections_at + 16].copy_from_slice(&virtual_address.to_le_bytes());
        file[sections_at + 16..sections_at + 20].copy_from_slice(&(resources.len() as u32).to_le_bytes());
        file[sections_at + 20..sections_at + 24].copy_from_slice(&raw_offset.to_le_bytes());
        file.extend(resources);
        file
    }

    #[test]
    fn reads_the_version_a_program_carries() {
        let dir = std::env::temp_dir().join(format!("kanso-pe-version-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let write = |name: &str, bytes: &[u8]| {
            let path = dir.join(name);
            fs::write(&path, bytes).unwrap();
            path
        };

        let library = write("nvngx_dlss.dll", &program_with_version([310, 9, 16, 0], true));
        let version = file_version(&library).expect("a version");
        assert_eq!(version, FileVersion([310, 9, 16, 0]));
        assert_eq!((version.dotted().as_str(), version.short().as_str()), ("310.9.16.0", "310.9.16"));
        let old = write("game.exe", &program_with_version([1, 0, 0, 0], false));
        assert_eq!(file_version(&old).map(|v| v.short()).as_deref(), Some("1.0"), "32-bit programs too");
        assert!(FileVersion([3, 8, 1, 0]) < FileVersion([310, 1, 0, 0]), "versions compare number by number");

        // Never set, not a program, cut short, missing: nothing, and no panic
        assert_eq!(file_version(&write("unset.exe", &program_with_version([0, 0, 0, 0], true))), None);
        assert_eq!(file_version(&write("notes.txt", b"MZ is how a text can start too")), None);
        assert_eq!(file_version(&write("empty.dll", b"")), None);
        assert_eq!(file_version(&dir.join("missing.dll")), None);
        let whole = program_with_version([2, 5, 0, 7], true);
        for cut in (0..whole.len()).step_by(7) {
            let _ = file_version(&write("cut.dll", &whole[..cut]));
        }
        // Tables that point outside the file, or at each other
        let mut wild = whole.clone();
        let optional = 0x80 + 24;
        wild[optional + 112 + 16..optional + 112 + 20].copy_from_slice(&0xFFFF_FF00u32.to_le_bytes());
        assert_eq!(file_version(&write("wild.dll", &wild)), None);

        let _ = fs::remove_dir_all(&dir);
    }
}
