//! The Epic Games Launcher's local catalog cache: the Epic games of the account, installed
//! or not.
//!
//! The launcher keeps what its library shows in
//! `%PROGRAMDATA%\Epic\EpicGamesLauncher\Data\Catalog\catcache.bin`: base64 text of a JSON
//! array of catalog items, each with a title, artwork (`keyImages`), categories, the
//! namespace and the releases (`releaseInfo`, whose `appId` is the "app name" the launcher
//! starts and installs a game by). Reading it needs no sign-in.
//!
//! An item is listed as a game by the rules the Playnite launcher applies to the same catalog
//! items: an application, not Unreal Engine content, not an add-on of another item, not
//! extra content, not a game another store's client runs.

use super::epic::build_epic_launch_uri;
use crate::cleaner::clean_game_title;
use crate::models::GamePlatform;
use crate::owned::OwnedGame;
use base64::Engine as _;
use serde_json::Value;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Environment variable that points Kanso at a specific cache file, for development and
/// end-to-end tests.
pub const EPIC_CATALOG_OVERRIDE_ENV: &str = "KANSO_EPIC_CATALOG";

/// Nothing sensible is bigger: a refusal to read a file that is not the cache.
const MAX_CACHE_BYTES: u64 = 128 * 1024 * 1024;

/// Categories of things that are not games to play.
const NON_GAME_CATEGORIES: &[&str] = &["digitalextras", "plugins", "plugins/engine", "engines"];

/// The catalog cache of the Epic Games Launcher on this PC, if there is one.
pub fn detect_epic_catalog_cache() -> Option<PathBuf> {
    if let Some(overridden) = std::env::var_os(EPIC_CATALOG_OVERRIDE_ENV) {
        let candidate = PathBuf::from(overridden);
        if candidate.is_file() {
            return Some(candidate);
        }
    }

    #[cfg(target_os = "windows")]
    {
        let program_data =
            std::env::var("PROGRAMDATA").unwrap_or_else(|_| r"C:\ProgramData".to_string());
        let path = PathBuf::from(program_data)
            .join("Epic")
            .join("EpicGamesLauncher")
            .join("Data")
            .join("Catalog")
            .join("catcache.bin");
        path.is_file().then_some(path)
    }

    #[cfg(not(target_os = "windows"))]
    {
        None
    }
}

/// The games listed in the cache file at `path`. A missing, oversized or unreadable file
/// simply gives no games.
pub fn read_owned_epic_games(path: &Path) -> Vec<OwnedGame> {
    let small_enough = std::fs::metadata(path).is_ok_and(|meta| meta.len() <= MAX_CACHE_BYTES);
    if !small_enough {
        return Vec::new();
    }
    std::fs::read(path)
        .ok()
        .and_then(|bytes| parse_epic_catalog_cache(&bytes))
        .unwrap_or_default()
}

/// The games in the bytes of a cache file. `None`: the bytes are not a cache file.
pub fn parse_epic_catalog_cache(bytes: &[u8]) -> Option<Vec<OwnedGame>> {
    // Base64 text, possibly with a byte-order mark or line breaks around it
    let text: Vec<u8> = bytes
        .strip_prefix(b"\xef\xbb\xbf")
        .unwrap_or(bytes)
        .iter()
        .copied()
        .filter(|byte| !byte.is_ascii_whitespace())
        .collect();
    let json = base64::engine::general_purpose::STANDARD.decode(&text).ok()?;
    let items: Vec<Value> = serde_json::from_slice(&json).ok()?;

    let mut seen: HashSet<String> = HashSet::new();
    let mut games = Vec::new();
    for item in &items {
        if let Some(owned) = game_of(item, None) {
            if seen.insert(owned.game.id.clone()) {
                games.push(owned);
            }
        }
    }
    Some(games)
}

/// The library entry for a catalog item, if the item is a game for Windows.
///
/// `app_name` is the name the launcher starts and installs the game by, when the caller
/// already knows it (the account's library names it for each item it lists for Windows);
/// otherwise it is read off the item's own Windows release.
pub fn game_of(item: &Value, app_name: Option<&str>) -> Option<OwnedGame> {
    let text = |key: &str| item.get(key).and_then(Value::as_str).map(str::trim).unwrap_or_default();
    let categories: Vec<&str> = item
        .get("categories")
        .and_then(Value::as_array)
        .map(|list| list.iter().filter_map(|category| category.get("path")?.as_str()).collect())
        .unwrap_or_default();
    let has = |path: &str| categories.iter().any(|category| category.eq_ignore_ascii_case(path));

    let title = text("title");
    let namespace = text("namespace");
    let catalog_id = text("id");
    if title.is_empty() || namespace.eq_ignore_ascii_case("ue") || !has("applications") {
        return None;
    }
    if NON_GAME_CATEGORIES.iter().any(|path| has(path)) {
        return None;
    }
    // An add-on names the game it belongs to; a few can be started on their own
    let is_addon = item.get("mainGameItem").is_some_and(|main| !main.is_null());
    if is_addon && !has("addons/launchable") {
        return None;
    }
    // Bought on Epic but run by another store's client, which lists it as its own
    let attribute = |name: &str| {
        item.get("customAttributes")
            .and_then(|attributes| attributes.get(name))
            .and_then(|attribute| attribute.get("value"))
            .and_then(Value::as_str)
            .map(|value| value.trim().to_ascii_lowercase())
            .unwrap_or_default()
    };
    if attribute("ThirdPartyManagedApp") == "the ea app" || attribute("partnerLinkType") == "ubisoft" {
        return None;
    }

    // The release for Windows carries the name the launcher knows the game by
    let app_name = match app_name.map(str::trim) {
        Some(known) => Some(known).filter(|app| is_plain_id(app))?,
        None => item
            .get("releaseInfo")
            .and_then(Value::as_array)?
            .iter()
            .filter(|release| {
                release
                    .get("platform")
                    .and_then(Value::as_array)
                    .is_none_or(|platforms| {
                        platforms.is_empty()
                            || platforms
                                .iter()
                                .filter_map(Value::as_str)
                                .any(|platform| platform.eq_ignore_ascii_case("Windows"))
                    })
            })
            .filter_map(|release| release.get("appId")?.as_str())
            .map(str::trim)
            .find(|app| is_plain_id(app))?,
    };

    let ids_are_plain = is_plain_id(namespace) && is_plain_id(catalog_id);
    let launch_uri = build_epic_launch_uri(
        if ids_are_plain { namespace } else { "" },
        if ids_are_plain { catalog_id } else { "" },
        app_name,
    );

    Some(OwnedGame::new(
        GamePlatform::Epic,
        format!("epic:{app_name}"),
        title,
        clean_game_title(title),
        app_name.to_string(),
        launch_uri,
        cover_url(item),
    ))
}

/// Letters, digits, dashes, underscores and dots: what the launcher's ids are made of, and
/// all that may go into an address handed to the launcher.
fn is_plain_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
}

/// The vertical box art of an item, else the landscape one, as an address on Epic's CDN
/// asking for a cover-sized image.
fn cover_url(item: &Value) -> Option<String> {
    let images = item.get("keyImages")?.as_array()?;
    ["DieselGameBoxTall", "DieselGameBox"].into_iter().find_map(|wanted| {
        let url = images
            .iter()
            .find(|image| image.get("type").and_then(Value::as_str) == Some(wanted))?
            .get("url")?
            .as_str()?
            .trim();
        if !url.starts_with("https://") {
            return None;
        }
        Some(if url.contains('?') {
            url.to_string()
        } else {
            format!("{url}?w=600&h=900&resize=1")
        })
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A cache as the launcher writes it: base64 of the JSON array of catalog items.
    pub(crate) fn sample_catalog_cache() -> Vec<u8> {
        let items = serde_json::json!([
            {
                "id": "4fe75bbc5a674f4f9b356b5c90567da5",
                "title": "Alan Wake 2",
                "namespace": "c4763f236d08423eb47b4c3008779c84",
                "categories": [{"path": "games"}, {"path": "applications"}],
                "keyImages": [
                    {"type": "DieselGameBox", "url": "https://cdn1.epicgames.com/offer/wide.jpg"},
                    {"type": "DieselGameBoxTall", "url": "https://cdn1.epicgames.com/offer/tall.jpg"},
                    {"type": "Thumbnail", "url": "https://cdn1.epicgames.com/offer/thumb.jpg"}
                ],
                "releaseInfo": [{"id": "r1", "appId": "dc9d2e595d0e4650b35d659f90d41059", "platform": ["Windows"]}],
                "developer": "Remedy Entertainment"
            },
            {
                "id": "b1b2b3", "title": "Alan Wake 2: Night Springs", "namespace": "c4763f236d08423eb47b4c3008779c84",
                "categories": [{"path": "addons"}, {"path": "applications"}],
                "mainGameItem": {"id": "4fe75bbc5a674f4f9b356b5c90567da5"},
                "releaseInfo": [{"appId": "nightsprings", "platform": ["Windows"]}]
            },
            {
                "id": "c1", "title": "Rocket League®", "namespace": "9773aa1aa54f4f7b80e44bef04986cea",
                "categories": [{"path": "games"}, {"path": "applications"}],
                "keyImages": [{"type": "DieselGameBox", "url": "https://cdn1.epicgames.com/rl/wide.jpg?h=480"}],
                "releaseInfo": [
                    {"appId": "SugarMac", "platform": ["Mac"]},
                    {"appId": "Sugar", "platform": ["Windows", "Win32"]}
                ]
            },
            {
                "id": "ue1", "title": "Unreal Engine", "namespace": "ue",
                "categories": [{"path": "engines"}, {"path": "applications"}],
                "releaseInfo": [{"appId": "UE_5.4", "platform": ["Windows"]}]
            },
            {
                "id": "m1", "title": "Quixel Megascans Pack", "namespace": "marketplace",
                "categories": [{"path": "assets"}],
                "releaseInfo": [{"appId": "megascans", "platform": ["Windows"]}]
            },
            {
                "id": "x1", "title": "Soundtrack", "namespace": "ns1",
                "categories": [{"path": "digitalextras"}, {"path": "applications"}],
                "releaseInfo": [{"appId": "ost", "platform": ["Windows"]}]
            },
            {
                "id": "e1", "title": "Star Wars Jedi: Survivor", "namespace": "ns2",
                "categories": [{"path": "games"}, {"path": "applications"}],
                "customAttributes": {"ThirdPartyManagedApp": {"type": "STRING", "value": "The EA App"}},
                "releaseInfo": [{"appId": "jedi", "platform": ["Windows"]}]
            },
            {
                "id": "u1", "title": "Assassin's Creed Mirage", "namespace": "ns3",
                "categories": [{"path": "games"}, {"path": "applications"}],
                "customAttributes": {"partnerLinkType": {"type": "STRING", "value": "ubisoft"}},
                "releaseInfo": [{"appId": "mirage", "platform": ["Windows"]}]
            },
            {
                "id": "a1", "title": "Mobile Only", "namespace": "ns4",
                "categories": [{"path": "games"}, {"path": "applications"}],
                "releaseInfo": [{"appId": "android-only", "platform": ["Android"]}]
            },
            {
                "id": "s1", "title": "Standalone Expansion", "namespace": "ns5",
                "categories": [{"path": "addons"}, {"path": "addons/launchable"}, {"path": "applications"}],
                "mainGameItem": {"id": "zzz"},
                "releaseInfo": [{"appId": "Expansion_1", "platform": []}]
            },
            {
                "id": "odd id!", "title": "Odd Ids", "namespace": "ns6",
                "categories": [{"path": "applications"}],
                "keyImages": [{"type": "DieselGameBoxTall", "url": "http://insecure.example/tall.jpg"}],
                "releaseInfo": [{"appId": "bad name with spaces"}, {"appId": "OddGame"}]
            },
            {"id": "n1", "title": "", "namespace": "ns7", "categories": [{"path": "applications"}], "releaseInfo": [{"appId": "nameless"}]},
            {"id": "c1-again", "title": "Rocket League (duplicate)", "namespace": "ns8", "categories": [{"path": "applications"}], "releaseInfo": [{"appId": "Sugar"}]},
            "not an item"
        ]);
        base64::engine::general_purpose::STANDARD
            .encode(items.to_string())
            .into_bytes()
    }

    #[test]
    fn lists_the_games_of_the_catalog_cache_and_nothing_else() {
        let games = parse_epic_catalog_cache(&sample_catalog_cache()).expect("a cache");
        let titles: Vec<&str> = games.iter().map(|owned| owned.game.title.as_str()).collect();
        assert_eq!(titles, ["Alan Wake 2", "Rocket League®", "Standalone Expansion", "Odd Ids"]);

        let alan = &games[0].game;
        assert_eq!(alan.id, "epic:dc9d2e595d0e4650b35d659f90d41059");
        assert_eq!(alan.platform, GamePlatform::Epic);
        assert_eq!(alan.platform_app_id.as_deref(), Some("dc9d2e595d0e4650b35d659f90d41059"));
        assert!(!alan.is_installed);
        assert_eq!(
            alan.launch_uri.as_deref(),
            Some("com.epicgames.launcher://apps/c4763f236d08423eb47b4c3008779c84%3A4fe75bbc5a674f4f9b356b5c90567da5%3Adc9d2e595d0e4650b35d659f90d41059?action=launch&silent=true")
        );
        assert_eq!(
            alan.install_uri().as_deref(),
            Some("com.epicgames.launcher://apps/c4763f236d08423eb47b4c3008779c84%3A4fe75bbc5a674f4f9b356b5c90567da5%3Adc9d2e595d0e4650b35d659f90d41059?action=install")
        );
        // The vertical box art, sized for a cover
        assert_eq!(
            alan.cover_path.as_deref(),
            Some("https://cdn1.epicgames.com/offer/tall.jpg?w=600&h=900&resize=1")
        );

        // The Windows release names the game; landscape art when there is no vertical one,
        // left alone when its address already asks for a size
        let rocket = &games[1].game;
        assert_eq!(rocket.id, "epic:Sugar");
        assert_eq!(rocket.clean_title, "Rocket League");
        assert_eq!(rocket.cover_path.as_deref(), Some("https://cdn1.epicgames.com/rl/wide.jpg?h=480"));

        // Ids that are not plain never reach an address for the launcher
        let odd = &games[3].game;
        assert_eq!(odd.id, "epic:OddGame");
        assert_eq!(
            odd.launch_uri.as_deref(),
            Some("com.epicgames.launcher://apps/OddGame?action=launch&silent=true")
        );
        assert_eq!(odd.cover_path, None, "only secure addresses become covers");
    }

    #[test]
    fn refuses_what_is_not_a_catalog_cache() {
        assert!(parse_epic_catalog_cache(b"").is_none());
        assert!(parse_epic_catalog_cache(b"{\"plain\": \"json\"}").is_none());
        assert!(parse_epic_catalog_cache(b"bm90IGpzb24=").is_none(), "base64 of something else");
        let object = base64::engine::general_purpose::STANDARD.encode("{\"an\":\"object\"}");
        assert!(parse_epic_catalog_cache(object.as_bytes()).is_none());

        // A byte-order mark and line breaks around the text are fine
        let mut wrapped = b"\xef\xbb\xbf".to_vec();
        for chunk in sample_catalog_cache().chunks(76) {
            wrapped.extend_from_slice(chunk);
            wrapped.extend_from_slice(b"\r\n");
        }
        assert_eq!(parse_epic_catalog_cache(&wrapped).map(|games| games.len()), Some(4));
    }

    #[test]
    fn reads_the_cache_file_from_disk_and_shrugs_at_a_missing_one() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("catcache.bin");
        assert!(read_owned_epic_games(&path).is_empty());
        std::fs::write(&path, sample_catalog_cache()).unwrap();
        assert_eq!(read_owned_epic_games(&path).len(), 4);
    }

    /// Run by hand against a real cache file:
    /// `KANSO_EPIC_CATALOG=/path/catcache.bin cargo test -p kanso-core real_epic_catalog -- --nocapture`
    #[test]
    fn real_epic_catalog_when_one_is_provided() {
        let Some(path) = std::env::var_os(EPIC_CATALOG_OVERRIDE_ENV) else {
            return;
        };
        let bytes = std::fs::read(path).unwrap();
        let games = parse_epic_catalog_cache(&bytes).expect("a real file parses");
        println!("{} games", games.len());
        for owned in &games {
            println!("  {} | {} | {:?}", owned.game.id, owned.game.title, owned.game.cover_path);
        }
        assert!(!games.is_empty());
    }
}
