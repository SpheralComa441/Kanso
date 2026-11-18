//! Whose licences the Steam account on this PC holds: its own, and the ones lent to it by the
//! other members of its Steam family.
//!
//! The Steam client keeps the list it last received in `userdata/<account>/config/licensecache`.
//! Each licence names a package and the account it belongs to, which is what tells a game of
//! the family from one of the account's own, installed or not. The file is read where it is:
//! nothing is asked of Steam, nothing leaves the PC.
//!
//! The format is not documented by Valve. The file is the client's `CMsgClientLicenseList`
//! message (a protocol-buffers list of licences) followed by the CRC-32 of the message, and
//! the whole is XORed with a stream of printable characters drawn from the random number
//! generator of Valve's own libraries, seeded with the account's number. That much is known
//! from open-source tools that read it, and was checked here against a real file: the
//! checksum matched, and the games its owner knew to be the family's were the family's.
//! Anything that does not decode to a message with the right checksum is taken as "not
//! known", never guessed at.

use super::steam::steam_account_config_dir;
use super::steam_packages::{parse_package_info, SteamPackage};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

/// A licence cache holds some tens of bytes per licence: far more than this is not one.
const MAX_FILE_BYTES: u64 = 16 * 1024 * 1024;
const MAX_PACKAGE_FILE_BYTES: u64 = 64 * 1024 * 1024;

/// One licence of the list, reduced to what tells whose it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SteamLicense {
    pub package_id: u32,
    /// The account the licence belongs to (its number, as in `userdata/<number>`).
    pub owner_id: u32,
}

/// The generator behind the stream: "ran1" of Numerical Recipes (a Park-Miller generator with
/// a shuffle table), as Valve's libraries implement it.
struct UniformStream {
    idum: i32,
    iy: i32,
    iv: [i32; UniformStream::TABLE],
}

impl UniformStream {
    const TABLE: usize = 32;
    const IA: i32 = 16_807;
    const IM: i32 = 2_147_483_647;
    const IQ: i32 = 127_773;
    const IR: i32 = 2_836;
    const NDIV: i32 = 1 + (Self::IM - 1) / Self::TABLE as i32;

    fn new(seed: u32) -> Self {
        // The seed is taken as a signed number and made negative, which asks for the table
        // to be filled at the first draw
        let seed = seed as i32;
        Self {
            idum: if seed < 0 { seed } else { -seed },
            iy: 0,
            iv: [0; Self::TABLE],
        }
    }

    fn step(&mut self) {
        let k = self.idum / Self::IQ;
        self.idum = Self::IA * (self.idum - k * Self::IQ) - Self::IR * k;
        if self.idum < 0 {
            self.idum += Self::IM;
        }
    }

    fn next(&mut self) -> i32 {
        if self.idum <= 0 || self.iy == 0 {
            let positive = self.idum.wrapping_neg();
            self.idum = if positive < 1 { 1 } else { positive };
            for slot in (0..Self::TABLE + 8).rev() {
                self.step();
                if slot < Self::TABLE {
                    self.iv[slot] = self.idum;
                }
            }
            self.iy = self.iv[0];
        }
        self.step();
        // Always within the table: `iy` is below the modulus
        let slot = (self.iy / Self::NDIV).rem_euclid(Self::TABLE as i32) as usize;
        self.iy = self.iv[slot];
        self.iv[slot] = self.idum;
        self.iy
    }

    /// A printable character, 32 to 126, each as likely as the others.
    fn next_char(&mut self) -> u8 {
        const RANGE: i32 = 95;
        // Draws above the last whole multiple of the range are thrown away
        const MAX_ACCEPTABLE: i32 = i32::MAX - ((1u32 << 31) % RANGE as u32) as i32;
        loop {
            let drawn = self.next();
            if drawn <= MAX_ACCEPTABLE {
                return 32 + (drawn % RANGE) as u8;
            }
        }
    }
}

/// CRC-32 as zip and PNG compute it.
fn crc32(data: &[u8]) -> u32 {
    let mut crc = u32::MAX;
    for byte in data {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = if crc & 1 == 1 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
        }
    }
    !crc
}

/// A reader of the few protocol-buffers constructs the list uses.
struct Reader<'a> {
    data: &'a [u8],
    at: usize,
}

enum Field<'a> {
    Number(u64),
    Bytes(&'a [u8]),
}

impl<'a> Reader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, at: 0 }
    }

    fn is_done(&self) -> bool {
        self.at >= self.data.len()
    }

    fn varint(&mut self) -> Option<u64> {
        let mut value = 0u64;
        for shift in (0..64).step_by(7) {
            let byte = *self.data.get(self.at)?;
            self.at += 1;
            value |= u64::from(byte & 0x7F) << shift;
            if byte < 0x80 {
                return Some(value);
            }
        }
        None
    }

    fn take(&mut self, len: usize) -> Option<&'a [u8]> {
        let end = self.at.checked_add(len)?;
        let bytes = self.data.get(self.at..end)?;
        self.at = end;
        Some(bytes)
    }

    /// The next field: its number and its value. `None` when the data is not a message.
    fn field(&mut self) -> Option<(u64, Field<'a>)> {
        let key = self.varint()?;
        let number = key >> 3;
        if number == 0 {
            return None;
        }
        let value = match key & 7 {
            0 => Field::Number(self.varint()?),
            1 => Field::Number(u64::from_le_bytes(self.take(8)?.try_into().ok()?)),
            2 => {
                let len = usize::try_from(self.varint()?).ok()?;
                Field::Bytes(self.take(len)?)
            }
            5 => Field::Number(u64::from(u32::from_le_bytes(self.take(4)?.try_into().ok()?))),
            _ => return None,
        };
        Some((number, value))
    }
}

/// Field numbers of `CMsgClientLicenseList` and of its `License`.
const LIST_LICENSES: u64 = 2;
const LICENSE_PACKAGE_ID: u64 = 1;
const LICENSE_OWNER_ID: u64 = 12;

fn parse_license(data: &[u8]) -> Option<SteamLicense> {
    let mut reader = Reader::new(data);
    let (mut package_id, mut owner_id) = (None, None);
    while !reader.is_done() {
        match reader.field()? {
            (LICENSE_PACKAGE_ID, Field::Number(value)) => package_id = u32::try_from(value).ok(),
            (LICENSE_OWNER_ID, Field::Number(value)) => owner_id = u32::try_from(value).ok(),
            _ => {}
        }
    }
    Some(SteamLicense {
        package_id: package_id?,
        // A licence that names no owner says nothing about whose it is
        owner_id: owner_id.unwrap_or(0),
    })
}

/// Reads a licence cache as the client of the account numbered `account_id` wrote it.
/// `None` when the data is not that: another account's file, a format that changed, a file
/// cut short. The checksum the client appends decides.
pub fn decode_license_cache(data: &[u8], account_id: u32) -> Option<Vec<SteamLicense>> {
    if data.len() <= 4 {
        return None;
    }
    let mut stream = UniformStream::new(account_id);
    let plain: Vec<u8> = data.iter().map(|byte| byte ^ stream.next_char()).collect();
    let (message, checksum) = plain.split_at(plain.len() - 4);
    if crc32(message) != u32::from_le_bytes(checksum.try_into().ok()?) {
        return None;
    }

    let mut licenses = Vec::new();
    let mut reader = Reader::new(message);
    while !reader.is_done() {
        if let (LIST_LICENSES, Field::Bytes(license)) = reader.field()? {
            licenses.push(parse_license(license)?);
        }
    }
    Some(licenses)
}

/// Which Steam apps the account holds a licence of its own for, and which it can only use
/// because another account of its family does.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SteamOwnership {
    own: HashSet<u64>,
    family: HashSet<u64>,
}

impl SteamOwnership {
    /// From the licences of the account numbered `account_id` and the packages they name.
    pub fn from_licenses(licenses: &[SteamLicense], packages: &[SteamPackage], account_id: u32) -> Self {
        let apps_of: HashMap<u32, &[u32]> =
            packages.iter().map(|package| (package.id, package.app_ids.as_slice())).collect();
        let mut ownership = Self::default();
        for license in licenses {
            let Some(apps) = apps_of.get(&license.package_id) else {
                continue;
            };
            let holder = if license.owner_id == account_id {
                &mut ownership.own
            } else if license.owner_id != 0 {
                &mut ownership.family
            } else {
                continue;
            };
            holder.extend(apps.iter().map(|app| u64::from(*app)));
        }
        ownership
    }

    /// `Some(true)` for an app only another account of the family holds a licence for,
    /// `Some(false)` for one the account holds itself (whoever else does), `None` for an
    /// app no licence is known for.
    pub fn is_family_shared(&self, appid: u64) -> Option<bool> {
        if self.own.contains(&appid) {
            Some(false)
        } else if self.family.contains(&appid) {
            Some(true)
        } else {
            None
        }
    }
}

fn read_limited(path: &Path, max_bytes: u64) -> Option<Vec<u8>> {
    fs::metadata(path).ok().filter(|meta| meta.len() <= max_bytes)?;
    fs::read(path).ok()
}

/// What the Steam client on this PC knows about whose the account's games are, for the
/// account used most recently. `None` when it cannot be told: no licence cache, one that does
/// not decode, no description of the packages.
pub fn read_steam_ownership(steam_root: &Path) -> Option<SteamOwnership> {
    let config_dir = steam_account_config_dir(steam_root)?;
    let account_id: u32 = config_dir.parent()?.file_name()?.to_str()?.parse().ok()?;
    let cache = read_limited(&config_dir.join("licensecache"), MAX_FILE_BYTES)?;
    let licenses = decode_license_cache(&cache, account_id)?;
    let packages = read_limited(&steam_root.join("appcache").join("packageinfo.vdf"), MAX_PACKAGE_FILE_BYTES)
        .and_then(|bytes| parse_package_info(&bytes))?;
    Some(SteamOwnership::from_licenses(&licenses, &packages, account_id))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::scanner::steam_packages::tests::PackageFile;
    use tempfile::tempdir;

    fn varint(mut value: u64, out: &mut Vec<u8>) {
        loop {
            let byte = (value & 0x7F) as u8;
            value >>= 7;
            if value == 0 {
                out.push(byte);
                return;
            }
            out.push(byte | 0x80);
        }
    }

    /// A licence as the client writes one: every field of the message, in order.
    fn license(package_id: u32, owner_id: u32) -> Vec<u8> {
        let mut out = Vec::new();
        out.push(0x08);
        varint(u64::from(package_id), &mut out);
        // time_created, time_next_process: fixed 32-bit
        out.push(0x15);
        out.extend_from_slice(&1_700_000_000u32.to_le_bytes());
        out.push(0x1D);
        out.extend_from_slice(&0u32.to_le_bytes());
        // minute_limit, minutes_used, payment_method, flags
        for (tag, value) in [(0x20u8, 0u64), (0x28, 0), (0x30, 1025), (0x38, 16_896)] {
            out.push(tag);
            varint(value, &mut out);
        }
        // purchase_country_code: a string
        out.extend_from_slice(&[0x42, 2, b'I', b'T']);
        for (tag, value) in [(0x48u8, 1u64), (0x50, 0), (0x58, 23_456_789)] {
            out.push(tag);
            varint(value, &mut out);
        }
        out.push(0x60);
        varint(u64::from(owner_id), &mut out);
        // access_token: a 64-bit number written as a varint
        out.extend_from_slice(&[0x88, 0x01]);
        varint(0x1234_5678_9ABC_DEF0, &mut out);
        out
    }

    /// A licence cache as the client of `account_id` writes it.
    pub(crate) fn license_cache(account_id: u32, licenses: &[(u32, u32)]) -> Vec<u8> {
        let mut message = vec![0x08, 0x01];
        for (package_id, owner_id) in licenses {
            let body = license(*package_id, *owner_id);
            message.push(0x12);
            varint(body.len() as u64, &mut message);
            message.extend_from_slice(&body);
        }
        let checksum = crc32(&message);
        message.extend_from_slice(&checksum.to_le_bytes());
        let mut stream = UniformStream::new(account_id);
        message.iter().map(|byte| byte ^ stream.next_char()).collect()
    }

    #[test]
    fn the_stream_is_the_one_of_valves_generator() {
        // The first characters for a few seeds, as the reference implementation gives them
        let first = |seed: u32| {
            let mut stream = UniformStream::new(seed);
            (0..12).map(|_| stream.next_char()).collect::<Vec<u8>>()
        };
        assert_eq!(first(1), [63, 101, 116, 40, 94, 67, 95, 114, 45, 119, 97, 60]);
        assert_eq!(first(12_345), [84, 79, 100, 72, 46, 79, 37, 69, 46, 42, 100, 55]);
        assert_eq!(first(987_654_321), [37, 66, 81, 124, 39, 59, 59, 32, 87, 84, 76, 80]);
        assert_eq!(first(2_147_483_647), [32, 63, 101, 116, 40, 94, 67, 95, 114, 45, 119, 97]);
        // Only printable characters, whatever the seed
        let mut stream = UniformStream::new(424_242);
        assert!((0..5_000).all(|_| (32..=126).contains(&stream.next_char())));
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926, "the usual check value of CRC-32");
    }

    #[test]
    fn reads_the_licences_and_whose_they_are() {
        let me = 12_345_678;
        let cache = license_cache(me, &[(100, me), (200, 87_654_321), (300, 11_111_111), (4_000_000, me)]);
        // Looks like noise, not like a list of licences
        assert!(!cache.windows(2).any(|pair| pair == b"IT"));

        let licenses = decode_license_cache(&cache, me).expect("decodes for its account");
        assert_eq!(
            licenses,
            [
                SteamLicense { package_id: 100, owner_id: me },
                SteamLicense { package_id: 200, owner_id: 87_654_321 },
                SteamLicense { package_id: 300, owner_id: 11_111_111 },
                SteamLicense { package_id: 4_000_000, owner_id: me },
            ]
        );
        assert_eq!(decode_license_cache(&license_cache(me, &[]), me), Some(Vec::new()), "an account with no licences");

        // Another account's number, a damaged file, a file cut short, no file at all
        assert_eq!(decode_license_cache(&cache, me + 1), None);
        let mut damaged = cache.clone();
        damaged[20] ^= 0x01;
        assert_eq!(decode_license_cache(&damaged, me), None);
        assert_eq!(decode_license_cache(&cache[..cache.len() - 9], me), None);
        assert_eq!(decode_license_cache(&[], me), None);
        assert_eq!(decode_license_cache(&[1, 2, 3, 4], me), None);
    }

    #[test]
    fn tells_the_games_of_the_family_from_the_accounts_own() {
        let me = 12_345_678;
        let (sister, brother) = (87_654_321, 11_111_111);
        let licenses = [
            SteamLicense { package_id: 100, owner_id: me },
            SteamLicense { package_id: 200, owner_id: sister },
            SteamLicense { package_id: 300, owner_id: brother },
            // The same game bought by two of the family, and one bought by me as well
            SteamLicense { package_id: 301, owner_id: sister },
            SteamLicense { package_id: 400, owner_id: sister },
            SteamLicense { package_id: 401, owner_id: me },
            // No owner written, and a package the client does not describe
            SteamLicense { package_id: 500, owner_id: 0 },
            SteamLicense { package_id: 999, owner_id: sister },
        ];
        let package = |id: u32, app_ids: &[u32]| SteamPackage {
            id,
            app_ids: app_ids.to_vec(),
            license_type: 1,
            free_weekend: false,
            counts_as_unowned: false,
        };
        let packages = [
            package(100, &[10, 11]),
            package(200, &[20]),
            package(300, &[30, 31]),
            package(301, &[30]),
            package(400, &[40]),
            package(401, &[40, 41]),
            package(500, &[50]),
        ];
        let ownership = SteamOwnership::from_licenses(&licenses, &packages, me);

        for mine in [10, 11, 40, 41] {
            assert_eq!(ownership.is_family_shared(mine), Some(false), "{mine}");
        }
        for borrowed in [20, 30, 31] {
            assert_eq!(ownership.is_family_shared(borrowed), Some(true), "{borrowed}");
        }
        for unknown in [50, 60] {
            assert_eq!(ownership.is_family_shared(unknown), None, "{unknown}");
        }
    }

    #[test]
    fn reads_whose_the_games_are_from_the_clients_files() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        let me = 12_345_678u32;
        let config = root.join("userdata").join(me.to_string()).join("config");
        fs::create_dir_all(&config).unwrap();
        fs::create_dir_all(root.join("appcache")).unwrap();

        // Nothing to go by yet
        assert_eq!(read_steam_ownership(root), None);
        fs::write(config.join("localconfig.vdf"), "\"UserLocalConfigStore\"\n{\n}\n").unwrap();
        assert_eq!(read_steam_ownership(root), None, "no licence cache");

        fs::write(config.join("licensecache"), license_cache(me, &[(100, me), (200, 87_654_321)])).unwrap();
        assert_eq!(read_steam_ownership(root), None, "no description of the packages");

        let packages = PackageFile::new(true).package(100, 1, &[], &[10]).package(200, 1, &[], &[20, 21]);
        fs::write(root.join("appcache").join("packageinfo.vdf"), packages.finish()).unwrap();
        let ownership = read_steam_ownership(root).expect("both files are there");
        assert_eq!(ownership.is_family_shared(10), Some(false));
        assert_eq!(ownership.is_family_shared(20), Some(true));
        assert_eq!(ownership.is_family_shared(21), Some(true));
        assert_eq!(ownership.is_family_shared(99), None);

        // A cache written for another account says nothing about this one
        fs::write(config.join("licensecache"), license_cache(me + 1, &[(200, me + 1)])).unwrap();
        assert_eq!(read_steam_ownership(root), None);
    }
}
