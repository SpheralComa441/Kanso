//! Games the user has but that are not on disk.
//!
//! Three kinds end up here. Games Kanso once found installed simply stay in the library when
//! they are uninstalled (see [`crate::store::LibraryStore::merge_with_scan`]). And two local
//! records of the Steam client say what else the account has, without signing in anywhere:
//! `appcache/packageinfo.vdf` lists the apps of every package it owns, `localconfig.vdf` the
//! apps it has played. Both give appids only, so the name, the kind (game, DLC, soundtrack...)
//! and the cover of each app are asked to the public store service, fifty at a time. Apps that
//! turn out not to be games are remembered and never asked about again.
//!
//! A library can hold hundreds of such games, so they are left out of the game-by-game
//! metadata sync. What the lists are built from (genres, tags, a description) reaches them
//! through [`sync_absent_game_details`], fifty games per request.

use crate::cleaner::clean_game_title;
use crate::db::LibraryDb;
use crate::i18n::Lang;
use crate::metadata::{MetadataClient, SteamAppDetails};
use crate::models::{DetectedGame, GamePlatform};
use crate::scanner::steam::SteamPlayStats;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::{Mutex, MutexGuard};

/// Most apps looked up in one pass (eighty requests); the rest waits for the next sync.
const MAX_LOOKUPS_PER_PASS: usize = 4000;

fn lock(db: &Mutex<LibraryDb>) -> MutexGuard<'_, LibraryDb> {
    db.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Brings the Steam games that are not installed in line with the Steam client's records:
/// - a game already listed as not installed gets its hours and last session refreshed (it may
///   have been played on another PC);
/// - every app the account owns (`owned_apps`) or has played (`play_stats`) that is not in
///   the library is looked up, and added as a not-installed game if the store says it is one.
///
/// A game added here carries its name, its hours and the address of its cover on Steam's CDN;
/// the rest of its details is fetched when its sheet is first opened. Returns the games added
/// or changed, as stored. Network trouble adds nothing and is retried at the next sync; the
/// database is never locked while Steam is being asked.
pub fn sync_steam_library(
    db: &Mutex<LibraryDb>,
    client: &MetadataClient,
    play_stats: &HashMap<String, SteamPlayStats>,
    owned_apps: &HashSet<u64>,
    now_ms: u64,
) -> Vec<DetectedGame> {
    let mut changed = Vec::new();

    let (library_ids, known_non_games) = {
        let db = lock(db);
        let games = db.load_games().unwrap_or_default();
        for mut game in games.iter().filter(|g| is_absent_steam_game(g)).cloned() {
            let Some(stats) = game.platform_app_id.as_deref().and_then(|id| play_stats.get(id))
            else {
                continue;
            };
            let last_played = (stats.last_played_secs > 0)
                .then(|| stats.last_played_secs.saturating_mul(1000))
                .max(game.last_played_at);
            let playtime = Some(stats.playtime_minutes).max(game.playtime_minutes);
            if (playtime, last_played) != (game.playtime_minutes, game.last_played_at) {
                game.playtime_minutes = playtime;
                game.last_played_at = last_played;
                if db.upsert_game(&game).is_ok() {
                    changed.push(game);
                }
            }
        }
        let ids: HashSet<String> = games.into_iter().map(|game| game.id).collect();
        (ids, db.known_steam_non_games(now_ms).unwrap_or_default())
    };

    let played: HashMap<u64, &SteamPlayStats> = play_stats
        .iter()
        .filter(|(_, stats)| stats.playtime_minutes > 0)
        .filter_map(|(appid, stats)| Some((appid.trim().parse::<u64>().ok()?, stats)))
        .collect();
    let unknown: BTreeSet<u64> = owned_apps
        .iter()
        .chain(played.keys())
        .copied()
        .filter(|appid| {
            !library_ids.contains(&format!("steam:{appid}")) && !known_non_games.contains(appid)
        })
        .collect();
    // Should a pass not cover everything: what was played last first, then the newest apps
    let mut candidates: Vec<u64> = unknown.into_iter().collect();
    candidates.sort_by_key(|appid| {
        let last_played = played.get(appid).map_or(0, |stats| stats.last_played_secs);
        (std::cmp::Reverse(last_played), std::cmp::Reverse(*appid))
    });
    candidates.truncate(MAX_LOOKUPS_PER_PASS);
    if candidates.is_empty() {
        return changed;
    }

    let Some(summaries) = client.fetch_steam_app_summaries(&candidates) else {
        return changed;
    };

    let mut non_games: Vec<(u64, &str)> = Vec::new();
    let mut found: Vec<DetectedGame> = Vec::new();
    for summary in summaries {
        let appid = summary.appid;
        match (summary.name, summary.is_game) {
            (Some(name), true) => {
                let mut game = DetectedGame::new_detected(
                    format!("steam:{appid}"),
                    name.clone(),
                    clean_game_title(&name),
                    GamePlatform::Steam,
                    Some(appid.to_string()),
                    String::new(),
                    None,
                    Some(format!("steam://rungameid/{appid}")),
                );
                game.is_installed = false;
                game.app_type = Some("game".to_string());
                game.cover_path = summary.cover_url;
                if let Some(stats) = played.get(&appid) {
                    game.playtime_minutes = Some(stats.playtime_minutes);
                    game.last_played_at = (stats.last_played_secs > 0)
                        .then(|| stats.last_played_secs.saturating_mul(1000));
                }
                game.updated_at = now_ms;
                found.push(game);
            }
            (Some(_), false) => non_games.push((appid, "other")),
            (None, _) => non_games.push((appid, "unlisted")),
        }
    }

    let mut db = lock(db);
    let _ = db.remember_steam_non_games(&non_games, now_ms);
    for game in found {
        // Installed (or added) while Steam was being asked: what is there wins
        if matches!(db.get_game(&game.id), Ok(None)) && db.upsert_game(&game).is_ok() {
            changed.push(game);
        }
    }
    changed
}

fn is_absent_steam_game(game: &DetectedGame) -> bool {
    game.platform == GamePlatform::Steam && !game.is_installed
}

/// A game that is not installed, has a Steam store page and was never asked about in `lang`.
fn lacks_details(game: &DetectedGame, lang: Lang) -> bool {
    !game.is_installed
        && !game.is_hidden
        && game.steam_store_id().is_some()
        && game.tags_lang.as_deref() != Some(lang.code())
}

/// Gives the games that are not installed their genres, community tags and description, in
/// the client's language, so that they can be browsed by genre and by tag like the others.
/// Steam is asked about fifty games per request: a library of five hundred costs ten requests,
/// once, and again only when the interface language changes. Games of other stores take part
/// through the Steam store page matched to them.
///
/// Texts the user wrote by hand are never replaced. Returns the games whose details changed,
/// as stored. Network trouble changes nothing and is retried at the next sync; the database
/// is never locked while Steam is being asked.
pub fn sync_absent_game_details(
    db: &Mutex<LibraryDb>,
    client: &MetadataClient,
    now_ms: u64,
) -> Vec<DetectedGame> {
    let lang = client.language();

    let mut pending: Vec<(String, u64)> = lock(db)
        .load_games()
        .unwrap_or_default()
        .iter()
        .filter(|game| lacks_details(game, lang))
        .filter_map(|game| Some((game.id.clone(), game.steam_store_id()?.parse().ok()?)))
        .collect();
    pending.truncate(MAX_LOOKUPS_PER_PASS);
    if pending.is_empty() {
        return Vec::new();
    }

    // Two entries can share a store page (the same game owned on two stores)
    let appids: Vec<u64> = pending
        .iter()
        .map(|(_, appid)| *appid)
        .collect::<BTreeSet<u64>>()
        .into_iter()
        .collect();
    let Some(details) = client.fetch_steam_app_details(&appids) else {
        return Vec::new();
    };
    let details: HashMap<u64, SteamAppDetails> =
        details.into_iter().map(|details| (details.appid, details)).collect();

    let mut db = lock(db);
    let mut settled: Vec<DetectedGame> = Vec::new();
    let mut changed: Vec<DetectedGame> = Vec::new();
    for (game_id, appid) in pending {
        // Against the game as it is now: it may have been installed, hidden or edited by
        // hand while Steam was being asked
        let (Ok(Some(mut game)), Some(details)) = (db.get_game(&game_id), details.get(&appid))
        else {
            continue;
        };
        if !lacks_details(&game, lang) {
            continue;
        }
        if apply_details(&mut game, details, lang) {
            game.updated_at = now_ms;
            changed.push(game.clone());
        }
        settled.push(game);
    }
    if db.upsert_games(&settled).is_err() {
        return Vec::new();
    }
    changed
}

/// Writes what the store said into `game` and notes that it was asked in `lang`. Returns
/// whether anything the user can see changed.
fn apply_details(game: &mut DetectedGame, details: &SteamAppDetails, lang: Lang) -> bool {
    let before = (game.tags.clone(), game.genres.clone(), game.short_description.clone());
    let code = lang.code();

    if details.listed {
        game.tags = details.tags.clone();
        if !game.custom_override {
            // Texts fetched in another language are replaced, texts in this one are kept
            let outdated = game.metadata_lang.as_deref() != Some(code);
            if details.short_description.is_some() && (outdated || game.short_description.is_none()) {
                game.short_description = details.short_description.clone();
            }
            if !details.genres.is_empty() && (outdated || game.genres.is_empty()) {
                game.genres = details.genres.clone();
            }
            if game.short_description.is_some() || !game.genres.is_empty() {
                game.metadata_lang = Some(code.to_string());
            }
        }
    }
    // An answer settles the matter, even "the store does not list it": no asking forever
    game.tags_lang = Some(code.to_string());
    before != (game.tags.clone(), game.genres.clone(), game.short_description.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metadata::tests::spawn_mock_api_server;
    use crate::metadata::needs_metadata;

    fn stats(minutes: u32, last_played_secs: u64) -> SteamPlayStats {
        SteamPlayStats {
            playtime_minutes: minutes,
            last_played_secs,
        }
    }

    fn installed(appid: &str, title: &str) -> DetectedGame {
        DetectedGame::new_detected(
            format!("steam:{appid}"),
            title.to_string(),
            title.to_string(),
            GamePlatform::Steam,
            Some(appid.to_string()),
            format!("/steam/common/{title}"),
            None,
            Some(format!("steam://rungameid/{appid}")),
        )
    }

    fn lookups(recorded: &std::sync::Arc<Mutex<Vec<String>>>) -> Vec<String> {
        recorded
            .lock()
            .unwrap()
            .iter()
            .filter(|path| path.starts_with("/IStoreBrowseService/GetItems/v1/"))
            .cloned()
            .collect()
    }

    #[test]
    fn owned_and_played_steam_games_that_are_not_installed_join_the_library() {
        let (base_url, recorded) = spawn_mock_api_server();
        let client = MetadataClient::with_base_urls(&base_url, &base_url, &base_url);
        let db = Mutex::new(LibraryDb::open_in_memory().unwrap());
        lock(&db).upsert_game(&installed("1145360", "Hades")).unwrap();

        // What the account owns: three games (one installed), a DLC, a tool
        let owned = HashSet::from([1_145_360, 620, 105_600, 1_880_832, 228_980]);
        let play_stats = HashMap::from([
            ("1145360".to_string(), stats(2112, 1_700_000_000)), // installed
            ("620".to_string(), stats(540, 1_650_000_000)),      // owned, played, not installed
            ("220".to_string(), stats(30, 0)),                   // played, in no owned package
            ("400".to_string(), stats(0, 0)),                    // a settings entry, never played
            ("not-a-number".to_string(), stats(5, 5)),
        ]);

        let added = sync_steam_library(&db, &client, &play_stats, &owned, 42);
        let mut titles: Vec<&str> = added.iter().map(|g| g.title.as_str()).collect();
        titles.sort();
        assert_eq!(titles, ["Half-Life® 2", "Portal 2", "Terraria"], "games only, officially named");

        let portal = lock(&db).get_game("steam:620").unwrap().expect("stored");
        assert!(!portal.is_installed);
        assert_eq!(portal.playtime_minutes, Some(540));
        assert_eq!(portal.last_played_at, Some(1_650_000_000_000));
        assert_eq!(portal.platform_app_id.as_deref(), Some("620"));
        assert_eq!(portal.install_uri().as_deref(), Some("steam://install/620"));
        assert_eq!(portal.install_dir, "");
        // The cover is an address on Steam's CDN, not a download
        assert_eq!(
            portal.cover_path,
            Some(format!("{base_url}/store_item_assets/steam/apps/620/library_600x900.jpg?t=1790187113"))
        );
        // Nothing more is fetched for it until someone opens its sheet
        assert!(!needs_metadata(&portal, true, Lang::It));

        let half_life = lock(&db).get_game("steam:220").unwrap().expect("stored");
        assert_eq!(half_life.clean_title, "Half-Life 2");
        assert_eq!(half_life.last_played_at, None);
        assert!(half_life.cover_path.as_deref().is_some_and(|url| url.ends_with("/steam/apps/220/header.jpg?t=1")));
        let terraria = lock(&db).get_game("steam:105600").unwrap().expect("stored");
        assert_eq!((terraria.playtime_minutes, terraria.cover_path.clone()), (None, None), "owned, never played");
        assert!(lock(&db).get_game("steam:1145360").unwrap().unwrap().is_installed);
        assert_eq!(lock(&db).game_count().unwrap(), 4);

        // One request for the five unknown apps; installed and never-played ones are not asked
        let asked = lookups(&recorded);
        assert_eq!(asked.len(), 1, "{asked:?}");
        assert!(!asked[0].contains("%3A400%7D") && !asked[0].contains("1145360"), "{}", asked[0]);
        assert!(asked[0].contains("include_assets"));

        // Next time nothing is new: the DLC and the tool are remembered, nothing is asked
        assert_eq!(
            lock(&db).known_steam_non_games(43).unwrap(),
            HashSet::from([1_880_832, 228_980])
        );
        assert!(sync_steam_library(&db, &client, &play_stats, &owned, 43).is_empty());
        assert_eq!(lookups(&recorded).len(), 1);

        // A month later the app the store did not list is asked about once more (a listing
        // can come back); the DLC stays settled
        let month_later = 43 + crate::db::UNLISTED_RECHECK_MS + 1;
        assert_eq!(
            lock(&db).known_steam_non_games(month_later).unwrap(),
            HashSet::from([1_880_832])
        );

        // Played some more on another PC: the hours of the game that is not installed follow
        let later = HashMap::from([("620".to_string(), stats(600, 1_660_000_000))]);
        let changed = sync_steam_library(&db, &client, &later, &owned, 44);
        assert_eq!(changed.len(), 1);
        assert_eq!(changed[0].playtime_minutes, Some(600));
        assert_eq!(changed[0].last_played_at, Some(1_660_000_000_000));
        assert_eq!(lookups(&recorded).len(), 1);
    }

    /// Run by hand against a real package cache and the real store service (or a local
    /// forwarder to it):
    /// `KANSO_PACKAGEINFO=/path/packageinfo.vdf KANSO_STEAM_API=https://api.steampowered.com
    ///  cargo test -p kanso-core real_steam_library -- --nocapture`
    #[test]
    fn real_steam_library_when_a_cache_and_the_store_are_provided() {
        let (Ok(path), Ok(api)) = (std::env::var("KANSO_PACKAGEINFO"), std::env::var("KANSO_STEAM_API"))
        else {
            return;
        };
        let packages = crate::scanner::steam_packages::parse_package_info(&std::fs::read(path).unwrap())
            .expect("a real file parses");
        let owned: HashSet<u64> = packages
            .iter()
            .filter(|package| package.is_owned())
            .flat_map(|package| package.app_ids.iter().map(|appid| u64::from(*appid)))
            .collect();
        let client = MetadataClient::with_base_urls(&api, &api, &api);
        let db = Mutex::new(LibraryDb::open_in_memory().unwrap());

        let started = std::time::Instant::now();
        let added = sync_steam_library(&db, &client, &HashMap::new(), &owned, 1);
        let with_cover = added.iter().filter(|game| game.cover_path.is_some()).count();
        println!(
            "{} apps in owned packages -> {} games added ({} with a cover address), {} remembered as not games, in {:.1} s",
            owned.len(),
            added.len(),
            with_cover,
            lock(&db).known_steam_non_games(1).unwrap().len(),
            started.elapsed().as_secs_f32()
        );
        for game in added.iter().take(5) {
            println!("  {} | {} | {:?}", game.id, game.title, game.cover_path);
        }
        assert!(!added.is_empty());
        assert!(sync_steam_library(&db, &client, &HashMap::new(), &owned, 2).is_empty(), "nothing left to ask");
    }

    fn absent(appid: &str, title: &str) -> DetectedGame {
        let mut game = installed(appid, title);
        game.is_installed = false;
        game.install_dir = String::new();
        game
    }

    fn details_requests(recorded: &std::sync::Arc<Mutex<Vec<String>>>) -> usize {
        lookups(recorded).iter().filter(|path| path.contains("include_basic_info")).count()
    }

    #[test]
    fn games_that_are_not_installed_get_genres_tags_and_description_in_one_request() {
        let (base_url, recorded) = spawn_mock_api_server();
        let client = MetadataClient::with_base_urls(&base_url, &base_url, &base_url);
        let db = Mutex::new(LibraryDb::open_in_memory().unwrap());
        {
            let db = lock(&db);
            db.upsert_game(&absent("620", "Portal 2")).unwrap();
            db.upsert_game(&absent("570", "Dota 2")).unwrap();
            // Gone from the store since it was bought
            db.upsert_game(&absent("228980", "Delisted")).unwrap();
            // On disk: the game-by-game sync looks after it
            db.upsert_game(&installed("1145360", "Hades")).unwrap();
            // Not installed and hidden: nobody browses it
            let mut hidden = absent("105600", "Terraria");
            hidden.is_hidden = true;
            db.upsert_game(&hidden).unwrap();
            // From another store, uninstalled, with the Steam page matched to it earlier and
            // texts fetched in another language
            let mut diablo = DetectedGame::new_detected(
                "Battle.net:fenris".into(),
                "Diablo IV".into(),
                "Diablo IV".into(),
                GamePlatform::BattleNet,
                Some("fenris".into()),
                String::new(),
                None,
                None,
            );
            diablo.is_installed = false;
            diablo.steam_store_appid = Some("2344520".into());
            diablo.short_description = Some("Dark action RPG.".into());
            diablo.genres = vec!["Action".into(), "RPG".into()];
            diablo.metadata_lang = Some("en".into());
            diablo.tags = vec!["Souls-like".into()];
            diablo.tags_lang = Some("en".into());
            diablo.is_favorite = true;
            db.upsert_game(&diablo).unwrap();
            // Not installed, no Steam page known: nothing to ask
            let mut unknown = diablo.clone();
            unknown.id = "ea:unknown".into();
            unknown.steam_store_appid = Some(String::new());
            db.upsert_game(&unknown).unwrap();
        }

        let changed = sync_absent_game_details(&db, &client, 77);
        let mut ids: Vec<&str> = changed.iter().map(|game| game.id.as_str()).collect();
        ids.sort();
        assert_eq!(ids, ["Battle.net:fenris", "steam:570", "steam:620"]);
        assert_eq!(details_requests(&recorded), 1, "one request for all of them");

        let portal = lock(&db).get_game("steam:620").unwrap().unwrap();
        assert_eq!(portal.genres, ["Azione", "GDR"]);
        assert_eq!(portal.tags, ["Giochi di società", "GDR"]);
        assert_eq!(portal.short_description.as_deref(), Some("Rompicapo cooperativi.\n\nCon portali & laser."));
        assert_eq!(portal.metadata_lang.as_deref(), Some("it"));
        assert_eq!(portal.tags_lang.as_deref(), Some("it"));
        assert_eq!(portal.updated_at, 77);
        assert!(!portal.is_installed);

        let dota = lock(&db).get_game("steam:570").unwrap().unwrap();
        assert_eq!(dota.genres, ["Strategia", "Free-to-Play"]);
        assert_eq!(dota.short_description, None);

        // The texts in the old language are replaced, what the user set is kept
        let diablo = lock(&db).get_game("Battle.net:fenris").unwrap().unwrap();
        assert_eq!(diablo.genres, ["GDR"]);
        assert_eq!(diablo.tags, ["GDR", "Stile Souls"]);
        assert_eq!(diablo.short_description.as_deref(), Some("GDR d'azione oscuro."));
        assert!(diablo.is_favorite);

        // Settled without anything to show, so that it is not asked about at every sync
        let delisted = lock(&db).get_game("steam:228980").unwrap().unwrap();
        assert_eq!(delisted.tags_lang.as_deref(), Some("it"));
        assert!(delisted.genres.is_empty() && delisted.tags.is_empty());

        // Left alone
        let db_guard = lock(&db);
        assert_eq!(db_guard.get_game("steam:1145360").unwrap().unwrap().tags_lang, None);
        assert_eq!(db_guard.get_game("steam:105600").unwrap().unwrap().tags_lang, None);
        assert_eq!(db_guard.get_game("ea:unknown").unwrap().unwrap().genres, ["Action", "RPG"]);
        drop(db_guard);

        // Nothing left to ask
        assert!(sync_absent_game_details(&db, &client, 78).is_empty());
        assert_eq!(details_requests(&recorded), 1);

        // The interface switches to English: one more request, everything follows
        let english = client.clone().with_language(Lang::En);
        let changed = sync_absent_game_details(&db, &english, 79);
        assert_eq!(changed.len(), 3);
        assert_eq!(details_requests(&recorded), 2);
        let portal = lock(&db).get_game("steam:620").unwrap().unwrap();
        assert_eq!(portal.genres, ["Action", "RPG"]);
        assert_eq!(portal.tags, ["Party Game", "RPG"]);
        assert_eq!(portal.short_description.as_deref(), Some("Co-op puzzles.\n\nWith portals & lasers."));
    }

    #[test]
    fn details_never_replace_what_the_user_wrote_and_wait_while_steam_is_unreachable() {
        let (base_url, _recorded) = spawn_mock_api_server();
        let client = MetadataClient::with_base_urls(&base_url, &base_url, &base_url);
        let db = Mutex::new(LibraryDb::open_in_memory().unwrap());
        let mut edited = absent("620", "Portal 2");
        edited.custom_override = true;
        edited.short_description = Some("Il mio preferito.".into());
        edited.genres = vec!["Rompicapo".into()];
        lock(&db).upsert_game(&edited).unwrap();
        lock(&db).upsert_game(&absent("570", "Dota 2")).unwrap();

        // No answer: nothing is settled, everything is asked again next time
        let dead = "http://127.0.0.1:9";
        let offline = MetadataClient::with_base_urls(dead, dead, dead);
        assert!(sync_absent_game_details(&db, &offline, 1).is_empty());
        assert_eq!(lock(&db).get_game("steam:570").unwrap().unwrap().tags_lang, None);

        let changed = sync_absent_game_details(&db, &client, 2);
        assert_eq!(changed.len(), 2);
        let portal = lock(&db).get_game("steam:620").unwrap().unwrap();
        assert_eq!(portal.short_description.as_deref(), Some("Il mio preferito."));
        assert_eq!(portal.genres, ["Rompicapo"]);
        assert_eq!(portal.tags, ["Giochi di società", "GDR"], "tags are Steam's own: they follow");
    }

    /// Run by hand like the test above: the details of a real library, against the real
    /// service. `KANSO_STEAM_STORE` is the store site, for the names of the tags.
    #[test]
    fn real_details_when_a_cache_and_the_store_are_provided() {
        let (Ok(path), Ok(api), Ok(store)) = (
            std::env::var("KANSO_PACKAGEINFO"),
            std::env::var("KANSO_STEAM_API"),
            std::env::var("KANSO_STEAM_STORE"),
        ) else {
            return;
        };
        let packages = crate::scanner::steam_packages::parse_package_info(&std::fs::read(path).unwrap())
            .expect("a real file parses");
        let owned: HashSet<u64> = packages
            .iter()
            .filter(|package| package.is_owned())
            .flat_map(|package| package.app_ids.iter().map(|appid| u64::from(*appid)))
            .collect();
        // The store site answers the tag names, the API host everything else
        let mut client = MetadataClient::with_base_urls(&store, &store, &store);
        client.set_steam_api_base_url(&api);
        let db = Mutex::new(LibraryDb::open_in_memory().unwrap());
        let added = sync_steam_library(&db, &client, &HashMap::new(), &owned, 1).len();

        let started = std::time::Instant::now();
        let changed = sync_absent_game_details(&db, &client, 2);
        let with_genres = changed.iter().filter(|game| !game.genres.is_empty()).count();
        let with_tags = changed.iter().filter(|game| !game.tags.is_empty()).count();
        let with_text = changed.iter().filter(|game| game.short_description.is_some()).count();
        println!(
            "{added} games -> {} completed in {:.1} s: {with_genres} with genres, {with_tags} with tags, {with_text} with a description",
            changed.len(),
            started.elapsed().as_secs_f32()
        );
        let mut by_genre: HashMap<&str, usize> = HashMap::new();
        for genre in changed.iter().flat_map(|game| game.genres.iter()) {
            *by_genre.entry(genre.as_str()).or_default() += 1;
        }
        let mut by_genre: Vec<(&str, usize)> = by_genre.into_iter().collect();
        by_genre.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
        println!("  genres: {by_genre:?}");
        for game in changed.iter().take(4) {
            println!("  {} | {:?} | {:?}", game.title, game.genres, &game.tags[..game.tags.len().min(5)]);
        }
        assert!(with_genres * 10 >= added * 8, "most games have a genre among their tags");
        // For trying the interface on real data: the library as the interface receives it
        // (`KANSO_DUMP`, a JSON file) or added to an existing library (`KANSO_DUMP_DB`)
        let games = lock(&db).load_games().unwrap();
        if let Ok(dump) = std::env::var("KANSO_DUMP") {
            std::fs::write(dump, serde_json::to_string(&games).unwrap()).unwrap();
        }
        if let Ok(library) = std::env::var("KANSO_DUMP_DB") {
            let mut library = LibraryDb::open(std::path::Path::new(&library)).unwrap();
            library.upsert_games(&games).unwrap();
        }
        assert!(sync_absent_game_details(&db, &client, 3).is_empty(), "nothing left to ask");
    }

    #[test]
    fn nothing_is_added_while_steam_cannot_be_reached() {
        let dead = "http://127.0.0.1:9";
        let client = MetadataClient::with_base_urls(dead, dead, dead);
        let db = Mutex::new(LibraryDb::open_in_memory().unwrap());
        let play_stats = HashMap::from([("620".to_string(), stats(540, 1_650_000_000))]);
        let owned = HashSet::from([220, 620]);

        assert!(sync_steam_library(&db, &client, &play_stats, &owned, 1).is_empty());
        assert_eq!(lock(&db).game_count().unwrap(), 0);
        assert!(lock(&db).known_steam_non_games(1).unwrap().is_empty(), "asked again next time");
    }
}
