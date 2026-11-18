//! A library of games brought by a plugin: what `library(context)` may return, what of it is
//! let through, and how it joins the user's library.
//!
//! A plugin cannot see the PC, so it does not find games on disk: it lists the games of a
//! service (a store's account, a cloud service, a page of links) and, for each one, the one
//! address that starts it or shows where to get it. That address is the only thing a plugin's
//! game can make Kanso open, so it is checked here, against closed lists:
//!
//! - a web page, plain `https` (the same rule as the links a plugin shows);
//! - an address of one of the game clients in [`CLIENTS`], and only of those the plugin's
//!   manifest names under `library.opens`, which the user sees before switching the plugin on.
//!
//! Nothing else is ever opened: no file, no program, no other protocol of the system. The
//! check is made when the answer is read, again when it comes back from the cache, and once
//! more by whoever opens the address (the manifest may have changed in between).

use super::manifest::PluginManifest;
use super::output::is_safe_link;
use crate::cleaner::clean_game_title;
use crate::db::LibraryDb;
use crate::models::{DetectedGame, GamePlatform};
use crate::store::has_user_data;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use url::Url;

/// A game client a plugin's games may be handed to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Client {
    /// The name a manifest uses under `library.opens`.
    pub name: &'static str,
    /// The client's own name, as the interface shows it.
    pub label: &'static str,
    /// How the addresses the client answers to begin.
    pub prefixes: &'static [&'static str],
}

/// The clients a plugin may name. A closed list on purpose: every protocol registered on a PC
/// is a program that can be made to do something, and only the ones known to be game clients
/// are let through. The stores Kanso reads by itself (Steam, Epic, GOG, ...) are not here: a
/// plugin has no business opening them. A new client is one more line, after a look at what
/// its addresses can do.
pub const CLIENTS: [Client; 3] = [
    Client { name: "itch", label: "itch", prefixes: &["itch://"] },
    Client { name: "amazon", label: "Amazon Games", prefixes: &["amazon-games://"] },
    Client { name: "heroic", label: "Heroic Games Launcher", prefixes: &["heroic://"] },
];

/// Most games one plugin can list.
pub const MAX_LIBRARY_GAMES: usize = 3000;
const ID_CHARS: usize = 64;
const TITLE_CHARS: usize = 200;
const DESCRIPTION_CHARS: usize = 600;
const GENRE_CHARS: usize = 40;
const MAX_GENRES: usize = 8;
const ADDRESS_CHARS: usize = 600;
/// More hours than anyone has played anything: a figure above it is a mistake.
const MAX_PLAYTIME_MINUTES: f64 = 10.0 * 365.0 * 24.0 * 60.0;

/// The labels of the clients named in `opens`, for the interface.
pub fn client_labels(opens: &[String]) -> Vec<String> {
    CLIENTS
        .iter()
        .filter(|client| opens.iter().any(|name| name == client.name))
        .map(|client| client.label.to_string())
        .collect()
}

/// Whether a plugin whose manifest names `opens` may have Kanso open `address`: a plain
/// `https` page, or an address of one of those clients. Only the characters of an address
/// are accepted, so that nothing in it can be read as anything else by what receives it.
pub fn is_allowed_address(address: &str, opens: &[String]) -> bool {
    let plain = !address.is_empty()
        && address.len() <= ADDRESS_CHARS
        && address
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-._~:/?#[]@!$&()*+,;=%".contains(&b));
    if !plain {
        return false;
    }
    if address.starts_with("https://") {
        return is_safe_link(address);
    }
    CLIENTS
        .iter()
        .filter(|client| opens.iter().any(|name| name == client.name))
        .flat_map(|client| client.prefixes.iter())
        .any(|prefix| address.len() > prefix.len() && address.starts_with(prefix))
}

/// Whether `cover` is a picture the plugin may point at: a plain `https` address on one of
/// the hosts its manifest declares, the only ones the user agreed it contacts.
fn is_allowed_cover(cover: &str, hosts: &[String]) -> bool {
    is_safe_link(cover)
        && Url::parse(cover)
            .ok()
            .and_then(|url| url.host_str().map(str::to_ascii_lowercase))
            .is_some_and(|host| hosts.contains(&host))
}

/// A game as a plugin's library lists it, after validation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryGame {
    /// The game's id inside its plugin: letters, digits, dots, dashes and underscores.
    pub id: String,
    pub title: String,
    /// The address that starts the game, or shows where to get it. `None` when the plugin
    /// gave none, or one it may not open.
    #[serde(default)]
    pub launch: Option<String>,
    /// Whether the game can be started right now, as the plugin says.
    pub installed: bool,
    #[serde(default)]
    pub genres: Vec<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub playtime_minutes: Option<u32>,
    #[serde(default)]
    pub last_played_at: Option<u64>,
    /// A cover on one of the plugin's declared hosts.
    #[serde(default)]
    pub cover: Option<String>,
    /// The game's page on the Steam store, when the plugin knows it: what the plugins that
    /// read Steam go by.
    #[serde(default)]
    pub steam_app_id: Option<String>,
}

/// What `library` returned, as kept in the plugin's cache.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginLibraryData {
    pub games: Vec<LibraryGame>,
    /// Addresses (to start a game, or of a cover) the plugin gave and may not use: its games
    /// are listed without them, and the count tells its author.
    #[serde(default)]
    pub refused: u32,
}

fn line(raw: &str, max_chars: usize) -> Option<String> {
    let cleaned: String = raw.trim().chars().filter(|c| !c.is_control()).take(max_chars).collect();
    let cleaned = cleaned.trim().to_string();
    (!cleaned.is_empty()).then_some(cleaned)
}

fn text_of(value: &Value, key: &str, max_chars: usize) -> Option<String> {
    line(value.get(key)?.as_str()?, max_chars)
}

/// An id as a plugin gives it: a short plain word, or a number.
fn id_of(value: &Value) -> Option<String> {
    let raw = match value.get("id")? {
        Value::String(text) => text.trim().to_string(),
        Value::Number(number) => number.to_string(),
        _ => return None,
    };
    let plain = (1..=ID_CHARS).contains(&raw.len())
        && raw.bytes().all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b));
    plain.then_some(raw)
}

/// Reads what `library` returned: `{ games: [...] }`, or the list alone. `null`/`undefined`
/// mean "no games". A game without an id or a title is not a game; an id listed twice counts
/// once; an address the plugin may not use is dropped and counted, and its game stays.
pub fn parse_library(value: &Value, manifest: &PluginManifest) -> PluginLibraryData {
    let opens = manifest.library.as_ref().map(|library| library.opens.as_slice()).unwrap_or_default();
    let items = match value {
        Value::Array(items) => items.as_slice(),
        other => other.get("games").and_then(Value::as_array).map(Vec::as_slice).unwrap_or_default(),
    };

    let mut data = PluginLibraryData::default();
    let mut seen: HashSet<String> = HashSet::new();
    for item in items {
        if data.games.len() >= MAX_LIBRARY_GAMES {
            break;
        }
        let (Some(id), Some(title)) = (id_of(item), text_of(item, "title", TITLE_CHARS)) else {
            continue;
        };
        if !seen.insert(id.clone()) {
            continue;
        }
        let mut address = |key: &str, allowed: &dyn Fn(&str) -> bool| -> Option<String> {
            let raw = item.get(key)?.as_str()?.trim();
            if raw.is_empty() {
                return None;
            }
            if allowed(raw) {
                Some(raw.to_string())
            } else {
                data.refused += 1;
                None
            }
        };
        let launch = address("launch", &|raw| is_allowed_address(raw, opens));
        let cover = address("cover", &|raw| is_allowed_cover(raw, &manifest.hosts));

        let mut genres: Vec<String> = Vec::new();
        for genre in item.get("genres").and_then(Value::as_array).into_iter().flatten() {
            if let Some(genre) = genre.as_str().and_then(|genre| line(genre, GENRE_CHARS)) {
                if genres.len() < MAX_GENRES && !genres.contains(&genre) {
                    genres.push(genre);
                }
            }
        }
        data.games.push(LibraryGame {
            id,
            title,
            launch,
            // A game is taken as ready to start unless its plugin says it is not
            installed: item.get("installed").and_then(Value::as_bool).unwrap_or(true),
            genres,
            description: text_of(item, "description", DESCRIPTION_CHARS),
            playtime_minutes: item
                .get("playtimeMinutes")
                .and_then(Value::as_f64)
                .filter(|minutes| minutes.is_finite() && (0.0..=MAX_PLAYTIME_MINUTES).contains(minutes))
                .map(|minutes| minutes.round() as u32),
            last_played_at: item
                .get("lastPlayedAt")
                .and_then(Value::as_f64)
                .filter(|at| at.is_finite() && *at > 0.0)
                .map(|at| at as u64),
            cover,
            steam_app_id: match item.get("steamAppId") {
                Some(Value::String(text)) => Some(text.trim().to_string()),
                Some(Value::Number(number)) => Some(number.to_string()),
                _ => None,
            }
            .filter(|id| (1..=12).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_digit())),
        });
    }
    data
}

/// Checks an answer again against the manifest as it is now: one read back from the cache
/// was validated against the manifest of when it was written.
pub fn recheck(data: &mut PluginLibraryData, manifest: &PluginManifest) {
    let opens = manifest.library.as_ref().map(|library| library.opens.as_slice()).unwrap_or_default();
    for game in &mut data.games {
        if game.launch.as_deref().is_some_and(|launch| !is_allowed_address(launch, opens)) {
            game.launch = None;
        }
        if game.cover.as_deref().is_some_and(|cover| !is_allowed_cover(cover, &manifest.hosts)) {
            game.cover = None;
        }
    }
}

/// The id a plugin's game has in the library.
pub fn library_id(plugin_id: &str, game_id: &str) -> String {
    format!("{}:{plugin_id}:{game_id}", GamePlatform::Plugin.as_str())
}

/// Whether `game` is one the library of plugin `plugin_id` brought.
pub fn is_game_of(game: &DetectedGame, plugin_id: &str) -> bool {
    game.platform == GamePlatform::Plugin && game.platform_app_id.as_deref() == Some(plugin_id)
}

/// What bringing a plugin's library up to date changed.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LibraryChange {
    /// Games added or changed, as stored.
    pub stored: Vec<DetectedGame>,
    /// Ids of the games that left the library.
    pub removed: Vec<String>,
}

fn is_remote(path: &str) -> bool {
    path.starts_with("https://") || path.starts_with("http://")
}

/// Makes the library hold the games of plugin `plugin_id` as `listed` has them now; an empty
/// list is what a plugin that is switched off, or gone, lists.
///
/// - A game the library does not have joins it.
/// - A game it has keeps everything the user gave it (status, favourite, genres, a title or
///   a cover chosen by hand) and what Kanso found for it; the address, whether it can be
///   started, and hours and last session when the plugin knows of more, follow the plugin.
/// - A game no longer listed leaves, unless the user gave it something: then it stays as
///   not installed, with no address to open, like a game of a store that was uninstalled.
///
/// `now_ms` stamps what changes; a "last played" beyond it is ignored.
pub fn reconcile(
    db: &Mutex<LibraryDb>,
    plugin_id: &str,
    listed: &[LibraryGame],
    now_ms: u64,
) -> LibraryChange {
    let mut db = db.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let library = db.load_games().unwrap_or_default();
    let existing: HashMap<&str, &DetectedGame> = library
        .iter()
        .filter(|game| is_game_of(game, plugin_id))
        .map(|game| (game.id.as_str(), game))
        .collect();

    let mut change = LibraryChange::default();
    let mut still_listed: HashSet<String> = HashSet::new();
    for game in listed {
        let id = library_id(plugin_id, &game.id);
        if !still_listed.insert(id.clone()) {
            continue;
        }
        // A session that has not happened yet is not one (a figure in seconds, a clock set
        // wrong): it is left out, not moved, or it would move again at every look
        let last_played = game.last_played_at.filter(|at| *at <= now_ms);
        let steam_page = game.steam_app_id.clone();
        let next = match existing.get(id.as_str()) {
            Some(current) => {
                let mut next = (*current).clone();
                if !current.custom_override {
                    next.title = game.title.clone();
                    next.clean_title = clean_game_title(&game.title);
                }
                next.launch_uri = game.launch.clone();
                next.is_installed = game.installed;
                next.playtime_minutes = current.playtime_minutes.max(game.playtime_minutes);
                next.last_played_at = current.last_played_at.max(last_played);
                if next.genres.is_empty() {
                    next.genres = game.genres.clone();
                }
                if next.short_description.is_none() {
                    next.short_description = game.description.clone();
                }
                if next.steam_store_appid.is_none() {
                    next.steam_store_appid = steam_page;
                }
                // The plugin's cover fills an empty place, or replaces its own earlier one:
                // a cover on disk (found by Kanso, or chosen by the user) is never touched
                if let Some(cover) = &game.cover {
                    if current.cover_path.as_deref().is_none_or(is_remote) {
                        next.cover_path = Some(cover.clone());
                    }
                }
                if next == **current {
                    continue;
                }
                next
            }
            None => {
                let mut next = DetectedGame::new_detected(
                    id,
                    game.title.clone(),
                    clean_game_title(&game.title),
                    GamePlatform::Plugin,
                    Some(plugin_id.to_string()),
                    String::new(),
                    None,
                    game.launch.clone(),
                );
                next.is_installed = game.installed;
                next.app_type = Some("game".to_string());
                next.genres = game.genres.clone();
                next.short_description = game.description.clone();
                next.playtime_minutes = game.playtime_minutes;
                next.last_played_at = last_played;
                next.cover_path = game.cover.clone();
                next.steam_store_appid = steam_page;
                next
            }
        };
        change.stored.push(DetectedGame { updated_at: now_ms, ..next });
    }

    for (id, game) in &existing {
        if still_listed.contains(*id) {
            continue;
        }
        if !has_user_data(game) {
            change.removed.push((*id).to_string());
        } else if game.is_installed || game.launch_uri.is_some() {
            let mut kept = (*game).clone();
            kept.is_installed = false;
            kept.launch_uri = None;
            kept.updated_at = now_ms;
            change.stored.push(kept);
        }
    }
    change.removed.sort();

    if db.upsert_games(&change.stored).is_err() || db.delete_games(&change.removed).is_err() {
        return LibraryChange::default();
    }
    change
}

/// The ids of the plugins that have games in the library.
pub fn plugins_with_games(games: &[DetectedGame]) -> HashSet<String> {
    games
        .iter()
        .filter(|game| game.platform == GamePlatform::Plugin)
        .filter_map(|game| game.platform_app_id.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::GameStatus;
    use serde_json::json;
    use tempfile::tempdir;

    fn manifest(library: &str, hosts: &str) -> PluginManifest {
        PluginManifest::parse(&format!(
            r#"{{"kansoPlugin": 1, "id": "shelf", "name": "Shelf", "version": "1.0.0",
                "hosts": {hosts}, "provides": {{"library": true}}, "library": {library}}}"#
        ))
        .expect("a valid manifest")
    }

    fn opens(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| name.to_string()).collect()
    }

    #[test]
    fn only_web_pages_and_the_declared_clients_can_be_opened() {
        let itch = opens(&["itch"]);
        for allowed in ["https://example.org/play/1", "https://play.example.org/?game=1&mode=cloud#top", "itch://games/123", "itch://install?game_id=7"] {
            assert!(is_allowed_address(allowed, &itch), "{allowed}");
        }
        // A client the manifest does not name
        assert!(!is_allowed_address("amazon-games://play/abc", &itch));
        assert!(is_allowed_address("amazon-games://play/abc", &opens(&["itch", "amazon"])));
        assert!(is_allowed_address("heroic://launch/gog/123", &opens(&["heroic"])));
        // With nothing declared, web pages only
        assert!(is_allowed_address("https://example.org/", &[]));
        assert!(!is_allowed_address("itch://games/123", &[]));

        for refused in [
            "",
            "itch://",
            "http://example.org/",
            "https://user:secret@example.org/",
            "https://localhost/",
            "file:///C:/Windows/System32/calc.exe",
            "C:\\Windows\\System32\\calc.exe",
            "calc.exe",
            "javascript:alert(1)",
            "data:text/html,hi",
            "ms-msdt:/id%20PCWDiagnostic",
            "search-ms:query=x",
            "steam://rungameid/10",
            "shell:startup",
            "\\\\server\\share\\run.exe",
            "itch://games/1 & calc.exe",
            "itch://games/1\"&calc",
            "itch://games/1\n",
            "ITCH://games/1",
            " itch://games/1",
            "itch:/games/1",
            "itchy://games/1",
            "xitch://games/1",
            "https://example.org/a b",
            "https://example.org/<script>",
            "https://example.org/|calc",
            "https://example.org/^x",
            "https://example.org/`x`",
            "https://example.org/caffè",
        ] {
            assert!(!is_allowed_address(refused, &opens(&["itch", "amazon", "heroic"])), "{refused:?}");
        }
        assert!(!is_allowed_address(&format!("https://example.org/{}", "a".repeat(600)), &[]));
    }

    #[test]
    fn a_manifest_names_its_clients_from_the_closed_list() {
        let shelf = manifest(r#"{"name": {"en": "The Shelf", "it": "Lo scaffale"}, "opens": ["itch", "Amazon", "itch"]}"#, "[]");
        let library = shelf.library.expect("a library");
        assert_eq!(library.opens, ["itch", "amazon"]);
        assert_eq!(library.name.get(crate::i18n::Lang::It), "Lo scaffale");
        assert_eq!(client_labels(&library.opens), ["itch", "Amazon Games"]);

        // Declared under "provides" alone: web pages only, and the plugin's own name
        let plain = PluginManifest::parse(
            r#"{"kansoPlugin": 1, "id": "links", "name": "Links", "version": "1", "provides": {"library": true}}"#,
        )
        .unwrap();
        assert_eq!(plain.library, Some(Default::default()));
        // Not declared under "provides": no library, whatever else is written
        let other = PluginManifest::parse(
            r#"{"kansoPlugin": 1, "id": "links", "name": "Links", "version": "1", "provides": {"home": true}, "library": {"opens": ["itch"]}}"#,
        )
        .unwrap();
        assert_eq!(other.library, None);

        let refuse = |library: &str| {
            PluginManifest::parse(&format!(
                r#"{{"kansoPlugin": 1, "id": "x-y", "name": "X", "version": "1", "provides": {{"library": true}}, "library": {library}}}"#
            ))
            .expect_err(library)
        };
        for unknown in [r#"{"opens": ["steam"]}"#, r#"{"opens": ["file"]}"#, r#"{"opens": ["ms-msdt"]}"#, r#"{"opens": [7]}"#, r#"{"opens": ["*"]}"#] {
            assert!(refuse(unknown).contains("library.opens"), "{unknown}");
        }
        assert!(refuse(r#"{"opens": "itch"}"#).contains("must be a list"));
        assert!(refuse(r#"["itch"]"#).contains("must be an object"));
    }

    #[test]
    fn reads_the_games_a_plugin_lists_and_drops_what_it_may_not_use() {
        let shelf = manifest(r#"{"opens": ["itch"]}"#, r#"["img.example.org"]"#);
        let data = parse_library(
            &json!({"games": [
                {"id": 42, "title": "  Night Shift  ", "launch": "itch://games/42", "installed": false,
                 "genres": ["Puzzle", "Puzzle", "", 7, "Narrative"], "description": "A short one.",
                 "playtimeMinutes": 90.4, "lastPlayedAt": 1700000000000u64,
                 "cover": "https://img.example.org/42.png", "steamAppId": 1234},
                {"id": "web-1", "title": "Cloud Game", "launch": "https://play.example.org/1"},
                {"id": "bad-launch", "title": "Calculator", "launch": "file:///C:/Windows/System32/calc.exe",
                 "cover": "https://elsewhere.example.org/x.png", "steamAppId": "12x"},
                {"id": "other-client", "title": "Not Mine", "launch": "amazon-games://play/1"},
                {"id": "web-1", "title": "Listed twice"},
                {"id": "no title"},
                {"id": "spaces in id", "title": "Bad id"},
                {"id": "../up", "title": "Bad id"},
                {"id": "", "title": "Bad id"},
                {"title": "No id"},
                {"id": "odd", "title": "Odd Figures", "playtimeMinutes": -5, "lastPlayedAt": "yesterday", "installed": "yes", "launch": 12},
                "not an object",
            ]}),
            &shelf,
        );

        let ids: Vec<&str> = data.games.iter().map(|game| game.id.as_str()).collect();
        assert_eq!(ids, ["42", "web-1", "bad-launch", "other-client", "odd"]);
        assert_eq!(data.refused, 3, "the file address, the cover elsewhere and the other client's address");
        assert_eq!(
            data.games[0],
            LibraryGame {
                id: "42".into(),
                title: "Night Shift".into(),
                launch: Some("itch://games/42".into()),
                installed: false,
                genres: vec!["Puzzle".into(), "Narrative".into()],
                description: Some("A short one.".into()),
                playtime_minutes: Some(90),
                last_played_at: Some(1_700_000_000_000),
                cover: Some("https://img.example.org/42.png".into()),
                steam_app_id: Some("1234".into()),
            }
        );
        assert_eq!(data.games[1].launch.as_deref(), Some("https://play.example.org/1"));
        assert!(data.games[1].installed, "ready to start unless the plugin says otherwise");
        assert_eq!((data.games[2].launch.as_deref(), data.games[2].cover.as_deref(), data.games[2].steam_app_id.as_deref()), (None, None, None));
        assert_eq!(data.games[3].launch, None);
        let odd = &data.games[4];
        assert_eq!((odd.playtime_minutes, odd.last_played_at, odd.installed, odd.launch.as_deref()), (None, None, true, None));

        // The list alone, nothing at all, and too many games
        assert_eq!(parse_library(&json!([{"id": "a", "title": "A"}]), &shelf).games.len(), 1);
        assert!(parse_library(&Value::Null, &shelf).games.is_empty());
        assert!(parse_library(&json!({"games": "all of them"}), &shelf).games.is_empty());
        let many: Vec<Value> = (0..MAX_LIBRARY_GAMES + 50).map(|n| json!({"id": n, "title": format!("Game {n}")})).collect();
        assert_eq!(parse_library(&json!(many), &shelf).games.len(), MAX_LIBRARY_GAMES);
    }

    #[test]
    fn an_answer_from_the_cache_is_checked_against_the_manifest_of_now() {
        let before = manifest(r#"{"opens": ["itch"]}"#, r#"["img.example.org"]"#);
        let mut data = parse_library(
            &json!([
                {"id": "a", "title": "A", "launch": "itch://games/1", "cover": "https://img.example.org/a.png"},
                {"id": "b", "title": "B", "launch": "https://example.org/b"},
            ]),
            &before,
        );
        // The manifest was edited since: the client and the host are no longer declared
        let now = manifest("{}", "[]");
        recheck(&mut data, &now);
        assert_eq!((data.games[0].launch.as_deref(), data.games[0].cover.as_deref()), (None, None));
        assert_eq!(data.games[1].launch.as_deref(), Some("https://example.org/b"), "a web page needs no declaration");
    }

    fn listed(id: &str, title: &str) -> LibraryGame {
        LibraryGame {
            id: id.to_string(),
            title: title.to_string(),
            launch: Some(format!("https://play.example.org/{id}")),
            installed: true,
            genres: Vec::new(),
            description: None,
            playtime_minutes: None,
            last_played_at: None,
            cover: None,
            steam_app_id: None,
        }
    }

    #[test]
    fn a_plugins_games_join_the_library_follow_it_and_leave_with_it() {
        let dir = tempdir().unwrap();
        let db = Mutex::new(LibraryDb::open(&dir.path().join("library.db")).unwrap());
        let games = |db: &Mutex<LibraryDb>| db.lock().unwrap().load_games().unwrap();
        let by_id = |db: &Mutex<LibraryDb>, id: &str| db.lock().unwrap().get_game(id).unwrap();

        // A game of a store, and one of another plugin: never touched
        {
            let steam = DetectedGame::new_detected("steam:10".into(), "Counter".into(), "Counter".into(), GamePlatform::Steam, Some("10".into()), "D:/Steam/Counter".into(), None, Some("steam://rungameid/10".into()));
            let mut other = DetectedGame::new_detected(library_id("other", "a"), "Other A".into(), "Other A".into(), GamePlatform::Plugin, Some("other".into()), String::new(), None, None);
            other.is_installed = true;
            db.lock().unwrap().upsert_games(&[steam, other]).unwrap();
        }

        let mut night = listed("night", "Night Shift™");
        night.genres = vec!["Puzzle".into()];
        night.description = Some("A short one.".into());
        night.playtime_minutes = Some(30);
        night.last_played_at = Some(900);
        night.cover = Some("https://img.example.org/night.png".into());
        night.steam_app_id = Some("77".into());
        let mut cloud = listed("cloud", "Cloud Game");
        cloud.installed = false;
        cloud.last_played_at = Some(9_999);
        let change = reconcile(&db, "shelf", &[night.clone(), cloud.clone(), listed("night", "Listed twice")], 1_000);
        assert_eq!(change.stored.len(), 2);
        assert!(change.removed.is_empty());

        let stored = by_id(&db, "plugin:shelf:night").expect("joined");
        assert_eq!(stored.platform, GamePlatform::Plugin);
        assert_eq!(stored.platform_app_id.as_deref(), Some("shelf"));
        assert_eq!((stored.title.as_str(), stored.clean_title.as_str()), ("Night Shift™", "Night Shift"));
        assert_eq!(stored.launch_uri.as_deref(), Some("https://play.example.org/night"));
        assert!(stored.is_installed && stored.install_dir.is_empty() && stored.executable_path.is_none());
        assert_eq!(stored.genres, ["Puzzle"]);
        assert_eq!(stored.short_description.as_deref(), Some("A short one."));
        assert_eq!((stored.playtime_minutes, stored.last_played_at), (Some(30), Some(900)));
        assert_eq!(stored.cover_path.as_deref(), Some("https://img.example.org/night.png"));
        assert_eq!(stored.steam_store_id(), Some("77"));
        let stored = by_id(&db, "plugin:shelf:cloud").unwrap();
        assert!(!stored.is_installed);
        assert_eq!(stored.last_played_at, None, "a session that has not happened yet is not one");

        // Nothing changed: nothing is written
        assert_eq!(reconcile(&db, "shelf", &[night.clone(), cloud.clone()], 2_000), LibraryChange::default());

        // The user makes the game theirs; Kanso finds a cover; then the plugin lists it anew
        {
            let db = db.lock().unwrap();
            let mut mine = db.get_game("plugin:shelf:night").unwrap().unwrap();
            mine.title = "Night Shift (my name)".into();
            mine.custom_override = true;
            mine.status = Some(GameStatus::Playing);
            mine.is_favorite = true;
            mine.genres = vec!["Mine".into()];
            mine.short_description = Some("Mine.".into());
            mine.playtime_minutes = Some(120);
            mine.cover_path = Some("C:/covers/night.png".into());
            mine.steam_store_appid = Some("99".into());
            db.upsert_game(&mine).unwrap();
        }
        night.title = "Night Shift: Director's Cut".into();
        night.launch = Some("https://play.example.org/night-2".into());
        night.installed = false;
        night.playtime_minutes = Some(45);
        night.genres = vec!["Horror".into()];
        night.cover = Some("https://img.example.org/night-2.png".into());
        cloud.title = "Cloud Game® Two".into();
        cloud.cover = Some("https://img.example.org/cloud.png".into());
        let change = reconcile(&db, "shelf", &[night.clone(), cloud.clone()], 3_000);
        assert_eq!(change.stored.len(), 2);
        let stored = by_id(&db, "plugin:shelf:night").unwrap();
        assert_eq!(stored.title, "Night Shift (my name)", "a title given by hand stays");
        assert_eq!(stored.launch_uri.as_deref(), Some("https://play.example.org/night-2"));
        assert!(!stored.is_installed);
        assert_eq!(stored.playtime_minutes, Some(120), "the larger figure");
        assert_eq!((stored.status, stored.is_favorite), (Some(GameStatus::Playing), true));
        assert_eq!(stored.genres, ["Mine"]);
        assert_eq!(stored.short_description.as_deref(), Some("Mine."));
        assert_eq!(stored.cover_path.as_deref(), Some("C:/covers/night.png"), "a cover on disk is never replaced");
        assert_eq!(stored.steam_store_appid.as_deref(), Some("99"));
        let stored = by_id(&db, "plugin:shelf:cloud").unwrap();
        assert_eq!((stored.title.as_str(), stored.clean_title.as_str()), ("Cloud Game® Two", "Cloud Game Two"));
        assert_eq!(stored.cover_path.as_deref(), Some("https://img.example.org/cloud.png"));
        // Its own earlier cover is replaced by its new one
        cloud.cover = Some("https://img.example.org/cloud-2.png".into());
        reconcile(&db, "shelf", &[night.clone(), cloud.clone()], 3_500);
        assert_eq!(by_id(&db, "plugin:shelf:cloud").unwrap().cover_path.as_deref(), Some("https://img.example.org/cloud-2.png"));

        // The plugin lists neither any more (or was switched off): the one the user never
        // touched leaves, the other stays as not installed, with nothing to open
        let change = reconcile(&db, "shelf", &[], 4_000);
        assert_eq!(change.removed, ["plugin:shelf:cloud"]);
        assert_eq!(change.stored.len(), 1);
        assert!(by_id(&db, "plugin:shelf:cloud").is_none());
        let kept = by_id(&db, "plugin:shelf:night").unwrap();
        assert!(!kept.is_installed);
        assert_eq!(kept.launch_uri, None);
        assert_eq!(kept.status, Some(GameStatus::Playing));
        assert_eq!(reconcile(&db, "shelf", &[], 5_000), LibraryChange::default(), "and nothing more to do");

        // Listed again, it is back as it was
        night.installed = true;
        reconcile(&db, "shelf", &[night], 6_000);
        let back = by_id(&db, "plugin:shelf:night").unwrap();
        assert!(back.is_installed && back.launch_uri.is_some() && back.is_favorite);

        // The rest of the library was never touched
        let ids: Vec<String> = games(&db).into_iter().map(|game| game.id).collect();
        assert!(ids.contains(&"steam:10".to_string()) && ids.contains(&"plugin:other:a".to_string()), "{ids:?}");
        assert_eq!(plugins_with_games(&games(&db)), HashSet::from(["shelf".to_string(), "other".to_string()]));
    }

    #[test]
    fn a_scan_of_the_disks_leaves_a_plugins_games_as_they_are() {
        let mut store = crate::store::LibraryStore::default();
        let mut web = DetectedGame::new_detected(library_id("shelf", "web"), "Web Game".into(), "Web Game".into(), GamePlatform::Plugin, Some("shelf".into()), String::new(), None, Some("https://play.example.org/web".into()));
        web.version = None;
        let steam = DetectedGame::new_detected("steam:10".into(), "Counter".into(), "Counter".into(), GamePlatform::Steam, Some("10".into()), "D:/Steam/Counter".into(), None, None);
        store.games = vec![web.clone(), steam];

        // Nothing is on disk any more
        let report = store.merge_with_scan(crate::models::ScanReport::default());
        let after = |id: &str| report.games.iter().find(|game| game.id == id).cloned().expect(id);
        assert!(!after("steam:10").is_installed, "a store's game that is gone is not installed");
        assert_eq!(after("plugin:shelf:web"), web, "the plugin's game is exactly as it was");
    }
}
