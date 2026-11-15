//! The Steam apps an account owns, from the Steam client's own package cache.
//!
//! `appcache/packageinfo.vdf` holds one record per package (a purchase, a free licence, a
//! bundle) the accounts used on this PC have a licence for, each with the appids it grants.
//! It is a binary file: an 8-byte header, then per package an id, a SHA-1, a change number, an
//! access token (newer format only) and a binary key/value tree, ended by an id of all ones.
//! The appids say nothing about what each app is: games, DLC, tools and soundtracks are told
//! apart later by the store (see [`crate::owned`]).

use std::collections::HashSet;
use std::fs;
use std::path::Path;

/// Header of the format without access tokens (older clients).
const MAGIC_V27: u32 = 0x0656_5527;
/// Header of the current format.
const MAGIC_V28: u32 = 0x0656_5528;
const END_OF_PACKAGES: u32 = 0xFFFF_FFFF;
/// A real file is a few hundred kB; anything much larger is not Steam's.
const MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;
/// Deepest nesting accepted in a package's tree (real ones use two levels).
const MAX_DEPTH: usize = 12;

/// The default package every account has: tools, servers, a few free-to-play games.
const DEFAULT_PACKAGE: u32 = 0;
/// `licensetype` of a licence bought once; higher values are recurring subscriptions.
const MAX_ONE_OFF_LICENSE_TYPE: i32 = 2;

/// One package of the cache, reduced to what tells whether its apps belong to the library.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SteamPackage {
    pub id: u32,
    pub app_ids: Vec<u32>,
    pub license_type: i32,
    /// A free weekend: the licence ended with the weekend.
    pub free_weekend: bool,
    /// Flagged by Steam itself as never counting as owned.
    pub counts_as_unowned: bool,
}

impl SteamPackage {
    /// Whether the package makes its apps part of the account's library: not the default
    /// package everyone has, not a past free weekend, not a subscription.
    pub fn is_owned(&self) -> bool {
        self.id != DEFAULT_PACKAGE
            && !self.free_weekend
            && !self.counts_as_unowned
            && self.license_type <= MAX_ONE_OFF_LICENSE_TYPE
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Value {
    Map(Vec<(String, Value)>),
    Text(String),
    Int(i32),
    Other,
}

impl Value {
    fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Map(entries) => entries
                .iter()
                .find(|(name, _)| name.eq_ignore_ascii_case(key))
                .map(|(_, value)| value),
            _ => None,
        }
    }

    fn int(&self, key: &str) -> Option<i32> {
        match self.get(key)? {
            Value::Int(number) => Some(*number),
            Value::Text(text) => text.trim().parse().ok(),
            _ => None,
        }
    }

    /// `true` for a key present with anything but zero or "false".
    fn flag(&self, key: &str) -> bool {
        match self.get(key) {
            Some(Value::Int(number)) => *number != 0,
            Some(Value::Text(text)) => !matches!(text.trim(), "" | "0" | "false"),
            Some(_) => true,
            None => false,
        }
    }
}

/// Reads bytes off the front of the file, failing instead of running past its end.
struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, len: usize) -> Option<&'a [u8]> {
        let end = self.pos.checked_add(len)?;
        let bytes = self.data.get(self.pos..end)?;
        self.pos = end;
        Some(bytes)
    }

    fn u8(&mut self) -> Option<u8> {
        self.take(1).map(|bytes| bytes[0])
    }

    fn u32(&mut self) -> Option<u32> {
        self.take(4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    /// A NUL-terminated string.
    fn text(&mut self) -> Option<String> {
        let rest = self.data.get(self.pos..)?;
        let len = rest.iter().position(|byte| *byte == 0)?;
        let text = String::from_utf8_lossy(&rest[..len]).into_owned();
        self.pos += len + 1;
        Some(text)
    }

    /// A binary key/value map: typed, named entries up to an end marker.
    fn map(&mut self, depth: usize) -> Option<Value> {
        if depth > MAX_DEPTH {
            return None;
        }
        let mut entries = Vec::new();
        loop {
            let kind = self.u8()?;
            if kind == 0x08 {
                return Some(Value::Map(entries));
            }
            let name = self.text()?;
            let value = match kind {
                0x00 => self.map(depth + 1)?,
                0x01 => Value::Text(self.text()?),
                0x02 => Value::Int(self.u32()? as i32),
                // float, pointer, colour: four bytes that are of no interest here
                0x03 | 0x04 | 0x06 => self.take(4).map(|_| Value::Other)?,
                // 64-bit integers
                0x07 | 0x0A => self.take(8).map(|_| Value::Other)?,
                // Anything else cannot be skipped safely: the file is not what it should be
                _ => return None,
            };
            entries.push((name, value));
        }
    }
}

/// Parses a `packageinfo.vdf`. `None` when the file is not in a known format or is cut short:
/// a partial list would silently hide games.
pub fn parse_package_info(data: &[u8]) -> Option<Vec<SteamPackage>> {
    let mut reader = Reader { data, pos: 0 };
    let has_token = match reader.u32()? {
        MAGIC_V27 => false,
        MAGIC_V28 => true,
        _ => return None,
    };
    reader.u32()?; // universe

    let mut packages = Vec::new();
    loop {
        let id = reader.u32()?;
        if id == END_OF_PACKAGES {
            return Some(packages);
        }
        // SHA-1 of the record, change number, access token
        reader.take(20 + 4 + if has_token { 8 } else { 0 })?;
        let tree = reader.map(0)?;
        // The tree has a single entry, named after the package, that holds everything
        let Value::Map(roots) = &tree else {
            return None;
        };
        let body = roots.first().map(|(_, body)| body)?;

        let app_ids = match body.get("appids") {
            Some(Value::Map(apps)) => apps
                .iter()
                .filter_map(|(_, app)| match app {
                    Value::Int(appid) => u32::try_from(*appid).ok(),
                    _ => None,
                })
                .collect(),
            _ => Vec::new(),
        };
        let extended = body.get("extended");
        packages.push(SteamPackage {
            id,
            app_ids,
            license_type: body.int("licensetype").unwrap_or(1),
            free_weekend: extended.is_some_and(|flags| flags.flag("freeweekend")),
            counts_as_unowned: extended.is_some_and(|flags| flags.flag("alwayscountsasunowned")),
        });
    }
}

/// Appids of everything the accounts used on this PC own on Steam, games or not. Empty when
/// the cache is missing or unreadable.
pub fn read_owned_steam_apps(steam_root: &Path) -> HashSet<u64> {
    let path = steam_root.join("appcache").join("packageinfo.vdf");
    let small_enough = fs::metadata(&path).is_ok_and(|meta| meta.len() <= MAX_FILE_BYTES);
    if !small_enough {
        return HashSet::new();
    }
    fs::read(&path)
        .ok()
        .and_then(|bytes| parse_package_info(&bytes))
        .unwrap_or_default()
        .iter()
        .filter(|package| package.is_owned())
        .flat_map(|package| package.app_ids.iter().map(|appid| u64::from(*appid)))
        .collect()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Builds files in the layout of a real `packageinfo.vdf` (checked against one written by
    /// the Steam client: header, per-package preamble, binary key/value tree, terminator).
    pub(crate) struct PackageFile {
        bytes: Vec<u8>,
        with_token: bool,
    }

    impl PackageFile {
        pub(crate) fn new(with_token: bool) -> Self {
            let magic = if with_token { MAGIC_V28 } else { MAGIC_V27 };
            let mut bytes = magic.to_le_bytes().to_vec();
            bytes.extend_from_slice(&1u32.to_le_bytes());
            Self { bytes, with_token }
        }

        fn text(&mut self, kind: u8, name: &str) {
            self.bytes.push(kind);
            self.bytes.extend_from_slice(name.as_bytes());
            self.bytes.push(0);
        }

        fn int(&mut self, name: &str, value: i32) {
            self.text(0x02, name);
            self.bytes.extend_from_slice(&value.to_le_bytes());
        }

        /// One package record. `extended` entries are written as integers, or as strings
        /// when given as `("key", Err("text"))`.
        pub(crate) fn package(
            mut self,
            id: u32,
            license_type: i32,
            extended: &[(&str, Result<i32, &str>)],
            app_ids: &[u32],
        ) -> Self {
            self.bytes.extend_from_slice(&id.to_le_bytes());
            self.bytes.extend_from_slice(&[0xAB; 20]);
            self.bytes.extend_from_slice(&77u32.to_le_bytes());
            if self.with_token {
                self.bytes.extend_from_slice(&0u64.to_le_bytes());
            }
            self.text(0x00, &id.to_string());
            self.int("packageid", id as i32);
            self.int("billingtype", 10);
            self.int("licensetype", license_type);
            self.int("status", 0);
            self.text(0x00, "extended");
            for (key, value) in extended {
                match value {
                    Ok(number) => self.int(key, *number),
                    Err(text) => {
                        self.text(0x01, key);
                        self.bytes.extend_from_slice(text.as_bytes());
                        self.bytes.push(0);
                    }
                }
            }
            self.bytes.push(0x08);
            self.text(0x00, "appids");
            for (index, appid) in app_ids.iter().enumerate() {
                self.int(&index.to_string(), *appid as i32);
            }
            self.bytes.push(0x08);
            self.text(0x00, "depotids");
            self.int("0", 228_981);
            self.bytes.push(0x08);
            self.text(0x00, "appitems");
            self.bytes.push(0x08);
            self.bytes.extend_from_slice(&[0x08, 0x08]);
            self
        }

        pub(crate) fn finish(mut self) -> Vec<u8> {
            self.bytes.extend_from_slice(&END_OF_PACKAGES.to_le_bytes());
            self.bytes
        }
    }

    fn sample() -> Vec<u8> {
        PackageFile::new(true)
            // The default package: free things every account has
            .package(0, 1, &[("excludefromsharing", Ok(1))], &[5, 7, 440, 570])
            // A bought game with its DLC and soundtrack
            .package(
                469_100,
                1,
                &[("allowcrossregiontradingandgifting", Err("false"))],
                &[990_080, 1_880_832, 1_880_833],
            )
            // A game kept from a giveaway: it expired as an offer, not as a licence
            .package(
                301_533,
                1,
                &[("freepromotion", Ok(1)), ("starttime", Ok(1_585_245_600)), ("expirytime", Ok(1_585_587_600))],
                &[203_160],
            )
            // A free weekend, long over
            .package(54_029, 1, &[("freeweekend", Ok(1)), ("expirytime", Ok(1_532_289_600))], &[381_210])
            // A monthly subscription
            .package(474_129, 3, &[], &[1_289_670])
            .package(88_001, 1, &[("alwayscountsasunowned", Ok(1))], &[570])
            .finish()
    }

    #[test]
    fn reads_every_package_with_its_apps_and_flags() {
        let packages = parse_package_info(&sample()).expect("parses");
        assert_eq!(packages.len(), 6);
        assert_eq!(packages[1].id, 469_100);
        assert_eq!(packages[1].app_ids, [990_080, 1_880_832, 1_880_833]);
        assert!(packages[1].is_owned());
        assert!(packages[2].is_owned(), "a giveaway that was claimed is kept");
        assert!(!packages[0].is_owned(), "the default package is not the user's library");
        assert!(packages[3].free_weekend && !packages[3].is_owned());
        assert!(!packages[4].is_owned(), "a subscription is not a purchase");
        assert!(packages[5].counts_as_unowned && !packages[5].is_owned());

        // The older layout has no access token before each tree
        let old = PackageFile::new(false).package(12, 1, &[], &[620, 400]).finish();
        assert_eq!(parse_package_info(&old).unwrap()[0].app_ids, [620, 400]);
        assert_eq!(parse_package_info(&PackageFile::new(true).finish()), Some(Vec::new()));
    }

    #[test]
    fn refuses_files_that_are_not_a_whole_package_cache() {
        let good = sample();
        assert_eq!(parse_package_info(b""), None);
        assert_eq!(parse_package_info(b"\"UserLocalConfigStore\"\n{\n}\n"), None, "a text VDF");
        // Cut anywhere, it is refused rather than read as a shorter list
        for cut in [9, 40, good.len() / 2, good.len() - 4, good.len() - 1] {
            assert_eq!(parse_package_info(&good[..cut]), None, "cut at {cut}");
        }
        let mut wrong_magic = good.clone();
        wrong_magic[0] = 0x29;
        assert_eq!(parse_package_info(&wrong_magic), None, "a format newer than known");

        // An entry type that cannot be skipped
        let mut odd = PackageFile::new(true).package(12, 1, &[], &[620]).finish();
        let at = odd.windows(11).position(|w| w == b"\x02packageid\x00").unwrap();
        odd[at] = 0x5A;
        assert_eq!(parse_package_info(&odd), None);

        // Nesting without end must not exhaust the stack
        let mut deep = PackageFile::new(true).finish();
        deep.truncate(8);
        deep.extend_from_slice(&7u32.to_le_bytes());
        deep.extend_from_slice(&[0; 32]);
        for _ in 0..100_000 {
            deep.extend_from_slice(&[0x00, b'x', 0x00]);
        }
        assert_eq!(parse_package_info(&deep), None);
    }

    #[test]
    fn owned_apps_come_from_the_packages_that_count() {
        let dir = tempfile::tempdir().unwrap();
        assert!(read_owned_steam_apps(dir.path()).is_empty(), "no cache, no apps");

        let appcache = dir.path().join("appcache");
        fs::create_dir_all(&appcache).unwrap();
        fs::write(appcache.join("packageinfo.vdf"), sample()).unwrap();
        assert_eq!(
            read_owned_steam_apps(dir.path()),
            HashSet::from([990_080, 1_880_832, 1_880_833, 203_160])
        );

        fs::write(appcache.join("packageinfo.vdf"), b"garbage").unwrap();
        assert!(read_owned_steam_apps(dir.path()).is_empty());
    }

    /// Run by hand against a file from a real Steam installation:
    /// `KANSO_PACKAGEINFO=/path/to/packageinfo.vdf cargo test -p kanso-core real_package -- --nocapture`
    #[test]
    fn real_package_cache_when_one_is_provided() {
        let Ok(path) = std::env::var("KANSO_PACKAGEINFO") else {
            return;
        };
        let packages = parse_package_info(&fs::read(path).unwrap()).expect("a real file parses");
        let owned: HashSet<u32> = packages
            .iter()
            .filter(|package| package.is_owned())
            .flat_map(|package| package.app_ids.iter().copied())
            .collect();
        println!(
            "{} packages, {} owned, {} distinct apps in owned packages",
            packages.len(),
            packages.iter().filter(|p| p.is_owned()).count(),
            owned.len()
        );
        assert!(!packages.is_empty());
    }
}
