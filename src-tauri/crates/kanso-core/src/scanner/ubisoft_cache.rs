//! Ubisoft Connect's local product cache: the games of the account, installed or not.
//!
//! The client keeps the configuration of every product in the account's library in one file,
//! `%LOCALAPPDATA%\Ubisoft Game Launcher\cache\configuration\configurations` (older versions
//! kept it under the client's own folder). It is written once the client has been started and
//! signed in, and reading it needs no sign-in.
//!
//! The file is a protocol-buffers message: a list of entries, each with the product's id, its
//! install id and its configuration as YAML text. The YAML names the game (often through a
//! key of its `localizations` table), its artwork and how it starts. Layout and filtering
//! rules follow what the Playnite launcher's Ubisoft library does with the same file.

use crate::cleaner::clean_game_title;
use crate::models::GamePlatform;
use crate::owned::OwnedGame;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use yaml_rust2::{Yaml, YamlLoader};

/// Environment variable that points Kanso at a specific cache file, for development and
/// end-to-end tests (it also works where there is no Ubisoft Connect at all).
pub const UBISOFT_CACHE_OVERRIDE_ENV: &str = "KANSO_UBISOFT_CACHE";

/// Where the artwork named in a configuration is served from.
const ASSET_URL_BASE: &str = "https://ubistatic3-a.akamaihd.net/orbit/uplay_launcher_3_0/assets/";

/// Nothing sensible is bigger: a refusal to read a file that is not the cache.
const MAX_CACHE_BYTES: u64 = 96 * 1024 * 1024;
const MAX_CONFIGURATION_BYTES: usize = 2 * 1024 * 1024;

/// The cache file of the Ubisoft Connect client on this PC, if there is one.
pub fn detect_ubisoft_cache() -> Option<PathBuf> {
    if let Some(overridden) = std::env::var_os(UBISOFT_CACHE_OVERRIDE_ENV) {
        let candidate = PathBuf::from(overridden);
        if candidate.is_file() {
            return Some(candidate);
        }
    }

    #[cfg(target_os = "windows")]
    {
        let relative = Path::new("cache").join("configuration").join("configurations");
        let per_user = std::env::var_os("LOCALAPPDATA")
            .map(|dir| PathBuf::from(dir).join("Ubisoft Game Launcher").join(&relative));
        let with_the_client = std::env::var_os("ProgramFiles(x86)").map(|dir| {
            PathBuf::from(dir)
                .join("Ubisoft")
                .join("Ubisoft Game Launcher")
                .join(&relative)
        });
        [per_user, with_the_client]
            .into_iter()
            .flatten()
            .find(|path| path.is_file())
    }

    #[cfg(not(target_os = "windows"))]
    {
        None
    }
}

/// The games listed in the cache file at `path`. A missing, oversized or unreadable file
/// simply gives no games.
pub fn read_owned_ubisoft_games(path: &Path) -> Vec<OwnedGame> {
    let small_enough = std::fs::metadata(path).is_ok_and(|meta| meta.len() <= MAX_CACHE_BYTES);
    if !small_enough {
        return Vec::new();
    }
    std::fs::read(path)
        .ok()
        .and_then(|bytes| parse_ubisoft_cache(&bytes))
        .unwrap_or_default()
}

/// One entry of the cache, before its configuration is looked at.
#[derive(Debug, Clone, PartialEq, Eq)]
struct CacheEntry {
    product_id: u64,
    install_id: u64,
    configuration: String,
}

/// What a configuration says about its product.
#[derive(Debug, Default)]
struct Product {
    name: String,
    thumb_image: String,
    /// It can be started: what tells a game from its add-ons and extra content.
    has_start_game: bool,
    /// The copy of a game bought on another store (Steam): that store lists it.
    third_party: bool,
    /// Downloadable content of another product.
    is_dlc: bool,
    /// Ids of the products that are add-ons of this one.
    addons: Vec<u64>,
}

/// The games in the bytes of a cache file. `None`: the bytes are not a cache file.
pub fn parse_ubisoft_cache(bytes: &[u8]) -> Option<Vec<OwnedGame>> {
    let entries = read_entries(bytes)?;
    let products: Vec<(&CacheEntry, Product)> = entries
        .iter()
        .filter_map(|entry| Some((entry, read_product(&entry.configuration)?)))
        .collect();

    // An add-on is known by the game that names it, wherever in the file that game is
    let addons: HashSet<u64> = products
        .iter()
        .flat_map(|(_, product)| product.addons.iter().copied())
        .collect();

    let mut seen: HashSet<u64> = HashSet::new();
    let mut games = Vec::new();
    for (entry, product) in products {
        let title = product.name.trim();
        if product.third_party
            || product.is_dlc
            || !product.has_start_game
            || addons.contains(&entry.product_id)
            || entry.product_id == 0
            || title.is_empty()
            || !seen.insert(entry.product_id)
        {
            continue;
        }

        let id = entry.product_id;
        let mut owned = OwnedGame::new(
            GamePlatform::Ubisoft,
            format!("ubisoft:{id}"),
            title,
            clean_game_title(title),
            id.to_string(),
            Some(format!("uplay://launch/{id}/0")),
            cover_url(&product.thumb_image),
        );
        // Some games are registered on the PC under their install id instead
        if entry.install_id != 0 && entry.install_id != id {
            owned.other_ids.push(format!("ubisoft:{}", entry.install_id));
        }
        games.push(owned);
    }
    Some(games)
}

/// Address of an artwork file named in a configuration; file names only, nothing that could
/// lead elsewhere.
fn cover_url(file: &str) -> Option<String> {
    let file = file.trim();
    let plain = !file.is_empty()
        && !file.starts_with('.')
        && file
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'));
    plain.then(|| format!("{ASSET_URL_BASE}{file}"))
}

/// The entries of the cache: field 1 of the outer message, repeated. Each entry carries the
/// product id (field 1), the install id (field 2) and the configuration text (field 3).
fn read_entries(bytes: &[u8]) -> Option<Vec<CacheEntry>> {
    let mut entries = Vec::new();
    let mut outer = ProtoReader::new(bytes);
    while let Some(field) = outer.next_field() {
        // A file cut short (the client was writing it) still gives the entries before the cut
        let Some((number, value)) = field else {
            break;
        };
        let (1, ProtoValue::Bytes(message)) = (number, value) else {
            continue;
        };

        let mut entry = CacheEntry {
            product_id: 0,
            install_id: 0,
            configuration: String::new(),
        };
        let mut inner = ProtoReader::new(message);
        let mut well_formed = true;
        while let Some(field) = inner.next_field() {
            match field {
                Some((1, ProtoValue::Number(id))) => entry.product_id = id,
                Some((2, ProtoValue::Number(id))) => entry.install_id = id,
                Some((3, ProtoValue::Bytes(text))) if text.len() <= MAX_CONFIGURATION_BYTES => {
                    entry.configuration = String::from_utf8_lossy(text).into_owned();
                }
                Some(_) => {}
                None => {
                    well_formed = false;
                    break;
                }
            }
        }
        if well_formed && !entry.configuration.is_empty() {
            entries.push(entry);
        }
    }
    // A file that is something else entirely gives nothing: say so rather than "no games"
    (!entries.is_empty() || bytes.is_empty()).then_some(entries)
}

/// The parts of a configuration Kanso uses. `None`: the text is not a configuration.
fn read_product(configuration: &str) -> Option<Product> {
    let documents = YamlLoader::load_from_str(configuration).ok()?;
    let document = documents.first()?;
    let root = &document["root"];
    root.as_hash()?;

    // A value may be the text itself or the key of a text in the localization table
    let texts = &document["localizations"]["default"];
    let text_of = |value: &Yaml| -> String {
        let raw = match value {
            Yaml::String(text) => text.clone(),
            Yaml::Integer(number) => number.to_string(),
            _ => return String::new(),
        };
        match &texts[raw.as_str()] {
            Yaml::String(localized) => localized.clone(),
            _ => raw,
        }
    };
    let present = |value: &Yaml| !matches!(value, Yaml::BadValue | Yaml::Null);
    let truthy = |value: &Yaml| match value {
        Yaml::Boolean(flag) => *flag,
        Yaml::String(text) => matches!(text.to_ascii_lowercase().as_str(), "yes" | "true"),
        _ => false,
    };

    Some(Product {
        name: text_of(&root["name"]),
        thumb_image: text_of(&root["thumb_image"]),
        has_start_game: present(&root["start_game"]),
        third_party: present(&root["third_party_platform"]),
        is_dlc: truthy(&root["is_ulc"]),
        addons: root["addons"]
            .as_vec()
            .map(|addons| {
                addons
                    .iter()
                    .filter_map(|addon| u64::try_from(addon["id"].as_i64()?).ok())
                    .collect()
            })
            .unwrap_or_default(),
    })
}

/// A value of a protocol-buffers field, as far as this file needs to tell them apart.
enum ProtoValue<'a> {
    Number(u64),
    Bytes(&'a [u8]),
    /// A fixed-width number: nothing here uses one.
    Other,
}

/// Walks the fields of one protocol-buffers message, never reading past its end.
struct ProtoReader<'a> {
    data: &'a [u8],
    position: usize,
}

impl<'a> ProtoReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, position: 0 }
    }

    /// The next field as (number, value). `None`: the message is over. `Some(None)`: what
    /// follows is not a well-formed field.
    fn next_field(&mut self) -> Option<Option<(u64, ProtoValue<'a>)>> {
        if self.position >= self.data.len() {
            return None;
        }
        Some(self.read_field())
    }

    fn read_field(&mut self) -> Option<(u64, ProtoValue<'a>)> {
        let key = self.varint()?;
        let number = key >> 3;
        let value = match key & 7 {
            0 => ProtoValue::Number(self.varint()?),
            1 => {
                self.take(8)?;
                ProtoValue::Other
            }
            2 => {
                let length = usize::try_from(self.varint()?).ok()?;
                ProtoValue::Bytes(self.take(length)?)
            }
            5 => {
                self.take(4)?;
                ProtoValue::Other
            }
            _ => return None,
        };
        (number != 0).then_some((number, value))
    }

    fn varint(&mut self) -> Option<u64> {
        let mut value: u64 = 0;
        for shift in (0..64).step_by(7) {
            let byte = *self.data.get(self.position)?;
            self.position += 1;
            value |= u64::from(byte & 0x7f) << shift;
            if byte & 0x80 == 0 {
                return Some(value);
            }
        }
        None
    }

    fn take(&mut self, length: usize) -> Option<&'a [u8]> {
        let end = self.position.checked_add(length)?;
        let slice = self.data.get(self.position..end)?;
        self.position = end;
        Some(slice)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    fn varint(mut value: u64) -> Vec<u8> {
        let mut out = Vec::new();
        loop {
            let byte = (value & 0x7f) as u8;
            value >>= 7;
            if value == 0 {
                out.push(byte);
                return out;
            }
            out.push(byte | 0x80);
        }
    }

    fn entry(product_id: u64, install_id: u64, configuration: &str) -> Vec<u8> {
        let mut message = Vec::new();
        message.push(0x08);
        message.extend(varint(product_id));
        message.push(0x10);
        message.extend(varint(install_id));
        message.push(0x1a);
        message.extend(varint(configuration.len() as u64));
        message.extend(configuration.as_bytes());

        let mut field = vec![0x0a];
        field.extend(varint(message.len() as u64));
        field.extend(message);
        field
    }

    /// A cache as the client writes it: games, an add-on, the Steam copy of a game, extra
    /// content that cannot be started.
    pub(crate) fn sample_cache() -> Vec<u8> {
        let odyssey = r#"version: 2.0
root:
  name: l1
  background_image: l2
  thumb_image: l3
  sort_string: ''
  addons:
    - id: 5060
    - id: 5061
  start_game:
    online:
      executables:
        - shortcut_name: Assassin's Creed Odyssey
          path:
            relative: ACOdyssey.exe
          working_directory:
            register: HKEY_LOCAL_MACHINE\SOFTWARE\Ubisoft\Launcher\Installs\5059\InstallDir
localizations:
  default:
    l1: "Assassin's Creed® Odyssey"
    l2: 1a2b3c_background.jpg
    l3: 4d5e6f_thumb.jpg
"#;
        let season_pass = "version: 2.0\nroot:\n  name: Season Pass\n  is_ulc: yes\n  start_game: ~\n";
        let anno = "version: 2.0\nroot:\n  name: Anno 1800\n  thumb_image: anno_thumb.png\n  start_game:\n    offline:\n      executables: []\n";
        let steam_copy = "root:\n  name: Far Cry 5\n  third_party_platform:\n    name: steam\n  start_game:\n    online: {}\n";
        let soundtrack = "root:\n  name: Rayman Legends Soundtrack\n  thumb_image: ost.jpg\n";
        let untold_dlc = "root:\n  name: The Fate of Atlantis\n  start_game:\n    online: {}\n";
        let odd_art = "root:\n  name: Trials Rising\n  thumb_image: ../../elsewhere.jpg\n  start_game:\n    online: {}\n";

        let mut cache = Vec::new();
        // The add-on comes before the game that names it
        cache.extend(entry(5061, 5061, untold_dlc));
        cache.extend(entry(5059, 5059, odyssey));
        cache.extend(entry(5060, 5060, season_pass));
        cache.extend(entry(4553, 4554, anno));
        cache.extend(entry(1847, 1847, steam_copy));
        cache.extend(entry(410, 410, soundtrack));
        cache.extend(entry(3600, 3600, odd_art));
        cache.extend(entry(77, 77, "this: is: not: yaml: ["));
        // The same game listed twice
        cache.extend(entry(4553, 4554, anno));
        cache
    }

    #[test]
    fn lists_the_games_of_the_cache_and_nothing_else() {
        let games = parse_ubisoft_cache(&sample_cache()).expect("a cache");
        let titles: Vec<&str> = games.iter().map(|owned| owned.game.title.as_str()).collect();
        assert_eq!(titles, ["Assassin's Creed® Odyssey", "Anno 1800", "Trials Rising"]);

        let odyssey = &games[0].game;
        assert_eq!(odyssey.id, "ubisoft:5059");
        assert_eq!(odyssey.clean_title, "Assassin's Creed Odyssey");
        assert_eq!(odyssey.platform, GamePlatform::Ubisoft);
        assert_eq!(odyssey.platform_app_id.as_deref(), Some("5059"));
        assert!(!odyssey.is_installed);
        assert_eq!(odyssey.install_dir, "");
        assert_eq!(odyssey.launch_uri.as_deref(), Some("uplay://launch/5059/0"));
        assert_eq!(odyssey.install_uri().as_deref(), Some("uplay://launch/5059/0"));
        // Name and artwork were keys of the localization table
        assert_eq!(
            odyssey.cover_path.as_deref(),
            Some("https://ubistatic3-a.akamaihd.net/orbit/uplay_launcher_3_0/assets/4d5e6f_thumb.jpg")
        );
        assert!(games[0].other_ids.is_empty());

        // Plain values are used as they are; a different install id is remembered
        assert_eq!(
            games[1].game.cover_path.as_deref(),
            Some("https://ubistatic3-a.akamaihd.net/orbit/uplay_launcher_3_0/assets/anno_thumb.png")
        );
        assert_eq!(games[1].other_ids, ["ubisoft:4554"]);

        // An artwork name that is not a plain file name is not turned into an address
        assert_eq!(games[2].game.cover_path, None);
    }

    #[test]
    fn refuses_what_is_not_a_cache() {
        assert_eq!(parse_ubisoft_cache(b"").map(|games| games.len()), Some(0));
        assert!(parse_ubisoft_cache(b"just some text, not a protocol buffer").is_none());
        // Cut in the middle of an entry: what came whole before it is kept
        let mut cut = sample_cache();
        cut.truncate(cut.len() - 20);
        let games = parse_ubisoft_cache(&cut).expect("the entries before the cut");
        assert_eq!(games.len(), 3);
        // A length that runs past the end of the file
        assert!(parse_ubisoft_cache(&[0x0a, 0xff, 0xff, 0x03, 0x08, 0x01]).is_none());
    }

    #[test]
    fn reads_the_cache_file_from_disk_and_shrugs_at_a_missing_one() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("configurations");
        assert!(read_owned_ubisoft_games(&path).is_empty());
        std::fs::write(&path, sample_cache()).unwrap();
        assert_eq!(read_owned_ubisoft_games(&path).len(), 3);
    }

    /// Run by hand against a real cache file:
    /// `KANSO_UBISOFT_CACHE=/path/configurations cargo test -p kanso-core real_ubisoft_cache -- --nocapture`
    #[test]
    fn real_ubisoft_cache_when_one_is_provided() {
        let Some(path) = std::env::var_os(UBISOFT_CACHE_OVERRIDE_ENV) else {
            return;
        };
        let bytes = std::fs::read(path).unwrap();
        let entries = read_entries(&bytes).expect("a real file parses");
        let games = parse_ubisoft_cache(&bytes).unwrap();
        println!("{} entries -> {} games", entries.len(), games.len());
        for owned in &games {
            println!("  {} | {} | {:?} | {:?}", owned.game.id, owned.game.title, owned.game.cover_path, owned.other_ids);
        }
        assert!(!games.is_empty());
    }
}
