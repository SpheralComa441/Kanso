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
//!
//! The clients of the other stores keep fuller records: Epic, GOG Galaxy and Ubisoft Connect
//! name each game of the account and its artwork (see `scanner::epic_catalog`,
//! `scanner::gog_galaxy` and `scanner::ubisoft_cache`). Those games join the library through
//! [`merge_owned_games`]; their genres and tags come from the Steam store page of the same
//! title, found a few games at a time by [`match_absent_games_step`].

use crate::cleaner::clean_game_title;
use crate::db::LibraryDb;
use crate::epic_account::{EpicApi, EpicAsset, EpicError, EpicSession};
use crate::i18n::Lang;
use crate::metadata::{title_key, MetadataClient, SteamAppDetails};
use crate::models::{DetectedGame, GamePlatform};
use crate::scanner::steam::SteamPlayStats;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::{Mutex, MutexGuard};

/// Most apps looked up in one pass (eighty requests); the rest waits for the next sync.
const MAX_LOOKUPS_PER_PASS: usize = 4000;

fn lock(db: &Mutex<LibraryDb>) -> MutexGuard<'_, LibraryDb> {
    db.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// A game a store's client lists for the account, ready to join the library as not installed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnedGame {
    pub game: DetectedGame,
    /// Other library ids the same game may already be listed under (a client that knows a
    /// game by two numbers).
    pub other_ids: Vec<String>,
}

impl OwnedGame {
    /// `id` is the library id the scanner of the same store gives the game once installed,
    /// so that installing it later turns this very entry into an installed one.
    pub fn new(
        platform: GamePlatform,
        id: String,
        title: &str,
        clean_title: String,
        app_id: String,
        launch_uri: Option<String>,
        cover_url: Option<String>,
    ) -> Self {
        let mut game = DetectedGame::new_detected(
            id,
            title.to_string(),
            clean_title,
            platform,
            Some(app_id),
            String::new(),
            None,
            launch_uri,
        );
        game.is_installed = false;
        game.app_type = Some("game".to_string());
        game.cover_path = cover_url;
        Self { game, other_ids: Vec::new() }
    }
}

/// Adds to the library, as not installed, the games the store clients list and the library
/// does not have yet. A game is taken for one the library already has when its id (or one of
/// its other ids) is there, or when the same store already has a game of the same title: a
/// scanner may know an installed game by another id than the client's records do.
///
/// A game listed before and still not installed gets its hours and last session refreshed
/// when the client knows of more (GOG Galaxy counts them). Everything else about a game
/// already in the library is left as it is. Returns the games added or changed, as stored.
pub fn merge_owned_games(
    db: &Mutex<LibraryDb>,
    owned: Vec<OwnedGame>,
    now_ms: u64,
) -> Vec<DetectedGame> {
    if owned.is_empty() {
        return Vec::new();
    }
    let mut db = lock(db);
    let library = db.load_games().unwrap_or_default();
    let by_id: HashMap<&str, &DetectedGame> =
        library.iter().map(|game| (game.id.as_str(), game)).collect();
    let mut known_ids: HashSet<String> = library.iter().map(|game| game.id.clone()).collect();
    let mut known_titles: HashSet<(&'static str, String)> = library
        .iter()
        .map(|game| (game.platform.as_str(), title_key(&game.clean_title)))
        .collect();

    let mut changed: Vec<DetectedGame> = Vec::new();
    for OwnedGame { mut game, other_ids } in owned {
        if let Some(existing) = by_id.get(game.id.as_str()) {
            let playtime = game.playtime_minutes.max(existing.playtime_minutes);
            let last_played = game.last_played_at.max(existing.last_played_at);
            if !existing.is_installed
                && (playtime, last_played) != (existing.playtime_minutes, existing.last_played_at)
            {
                let mut refreshed = (*existing).clone();
                refreshed.playtime_minutes = playtime;
                refreshed.last_played_at = last_played;
                changed.push(refreshed);
            }
            continue;
        }
        let title = (game.platform.as_str(), title_key(&game.clean_title));
        if title.1.is_empty()
            || known_ids.contains(&game.id)
            || other_ids.iter().any(|id| known_ids.contains(id))
            || known_titles.contains(&title)
        {
            continue;
        }
        known_ids.insert(game.id.clone());
        known_titles.insert(title);
        game.updated_at = now_ms;
        changed.push(game);
    }

    if db.upsert_games(&changed).is_err() {
        return Vec::new();
    }
    changed
}

/// Items of an Epic account described by the catalog in one [`sync_epic_library_step`].
pub const EPIC_STEP: usize = 15;

/// What one step over a signed-in account's library did.
#[derive(Debug, Default)]
pub struct AccountStep {
    /// Games added to the library, as stored.
    pub added: Vec<DetectedGame>,
    /// Items of the account still to be looked at.
    pub remaining: usize,
}

/// Brings into the library, as not installed, up to [`EPIC_STEP`] of the games a signed-in
/// Epic account owns and the library does not have yet. `assets` is the account's library as
/// [`EpicApi::library`] lists it (names of apps); the catalog is asked about each item the
/// library does not know, one request per item, `pause` apart, and the rules of
/// [`crate::scanner::epic_catalog`] decide whether it is a game. What turns out not to be one
/// (add-ons, extra content, games another store's client runs) is remembered and not asked
/// about again. Callers repeat the step until nothing remains.
///
/// An item the catalog cannot be asked about right now ends the step with the error; what
/// was learned before it is kept. The database is never locked while Epic is being asked.
pub fn sync_epic_library_step(
    db: &Mutex<LibraryDb>,
    api: &EpicApi,
    session: &EpicSession,
    assets: &[EpicAsset],
    pause: std::time::Duration,
    now_ms: u64,
) -> Result<AccountStep, EpicError> {
    const PLATFORM: &str = "epic";
    let unknown: Vec<&EpicAsset> = {
        let db = lock(db);
        let in_library: HashSet<String> =
            db.load_games().unwrap_or_default().into_iter().map(|game| game.id).collect();
        let not_games = db.known_store_non_games(PLATFORM).unwrap_or_default();
        assets
            .iter()
            .filter(|asset| {
                !in_library.contains(&format!("epic:{}", asset.app_name))
                    && !not_games.contains(&asset.app_name)
            })
            .collect()
    };

    let mut games: Vec<OwnedGame> = Vec::new();
    let mut not_games: Vec<String> = Vec::new();
    let mut failure: Option<EpicError> = None;
    for (index, asset) in unknown.iter().take(EPIC_STEP).enumerate() {
        if index > 0 && !pause.is_zero() {
            std::thread::sleep(pause);
        }
        match api.catalog_item(session, asset) {
            Ok(item) => match item
                .and_then(|item| crate::scanner::epic_catalog::game_of(&item, Some(&asset.app_name)))
            {
                Some(game) => games.push(game),
                None => not_games.push(asset.app_name.clone()),
            },
            Err(error) => {
                failure = Some(error);
                break;
            }
        }
    }
    let looked_at = games.len() + not_games.len();

    // A game the library already has under another id (same store, same title) is not added:
    // note it too, or it would be asked about at every sync
    let candidates: Vec<String> = games.iter().map(|owned| owned.game.id.clone()).collect();
    let added = merge_owned_games(db, games, now_ms);
    let added_ids: HashSet<&str> = added.iter().map(|game| game.id.as_str()).collect();
    not_games.extend(
        candidates
            .iter()
            .filter(|id| !added_ids.contains(id.as_str()))
            .filter_map(|id| id.strip_prefix("epic:").map(str::to_string)),
    );
    let _ = lock(db).remember_store_non_games(PLATFORM, &not_games, now_ms);

    match failure {
        Some(error) => Err(error),
        None => Ok(AccountStep { added, remaining: unknown.len() - looked_at }),
    }
}

/// Games looked up on Steam in one [`match_absent_games_step`].
pub const STORE_PAGE_STEP: usize = 20;

/// What one step of matching games to their Steam store pages did.
#[derive(Debug, Default)]
pub struct MatchStep {
    /// Games whose genres, tags or description changed, as stored.
    pub changed: Vec<DetectedGame>,
    /// Games still waiting to be looked up.
    pub remaining: usize,
    /// Steam could not be asked: stop, and try again at the next sync.
    pub unreachable: bool,
}

/// A game of another store that is not installed and whose Steam page was never looked for.
fn lacks_store_page(game: &DetectedGame) -> bool {
    !game.is_installed
        && !game.is_hidden
        && game.platform != GamePlatform::Steam
        && game.steam_store_appid.is_none()
}

/// Finds the Steam store page of up to [`STORE_PAGE_STEP`] games of other stores that are
/// not installed, then gives the matched ones their genres, tags and description (see
/// [`sync_absent_game_details`]). Steam's catalog covers most of what the other stores sell,
/// and its genres and tags are the ones the rest of the library is filed under.
///
/// A page is accepted only when its title is the game's title (see
/// [`MetadataClient::find_steam_app_by_title`]): a game without one is noted as such and not
/// asked about again. One search per game, `pause` apart, so a large library is matched
/// over a few minutes without hammering the store; callers repeat the step until nothing
/// remains. The database is never locked while Steam is being asked.
pub fn match_absent_games_step(
    db: &Mutex<LibraryDb>,
    client: &MetadataClient,
    pause: std::time::Duration,
    now_ms: u64,
) -> MatchStep {
    let mut pending: Vec<(String, String)> = lock(db)
        .load_games()
        .unwrap_or_default()
        .iter()
        .filter(|game| lacks_store_page(game))
        .map(|game| (game.id.clone(), game.clean_title.clone()))
        .collect();
    let waiting = pending.len();
    pending.truncate(STORE_PAGE_STEP);

    let mut step = MatchStep::default();
    let mut found: Vec<(String, String)> = Vec::new();
    for (index, (game_id, title)) in pending.into_iter().enumerate() {
        if index > 0 && !pause.is_zero() {
            std::thread::sleep(pause);
        }
        match client.find_steam_app_by_title(&title) {
            Ok(appid) => found.push((game_id, appid.unwrap_or_default())),
            Err(_) => {
                step.unreachable = true;
                break;
            }
        }
    }
    step.remaining = waiting - found.len();

    {
        let mut db = lock(db);
        let settled: Vec<DetectedGame> = found
            .into_iter()
            .filter_map(|(game_id, appid)| {
                // As it is now: it may have been installed or hidden meanwhile
                let mut game = db.get_game(&game_id).ok().flatten()?;
                lacks_store_page(&game).then(|| {
                    game.steam_store_appid = Some(appid);
                    game
                })
            })
            .collect();
        if db.upsert_games(&settled).is_err() {
            step.unreachable = true;
            return step;
        }
    }

    step.changed = sync_absent_game_details(db, client, now_ms);
    step
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

    fn owned(platform: GamePlatform, id: &str, title: &str) -> OwnedGame {
        let app_id = id.split(':').nth(1).unwrap_or_default().to_string();
        OwnedGame::new(
            platform,
            id.to_string(),
            title,
            clean_game_title(title),
            app_id,
            None,
            Some(format!("https://cdn.example/{id}.jpg")),
        )
    }

    #[test]
    fn games_listed_by_the_store_clients_join_the_library_once() {
        let db = Mutex::new(LibraryDb::open_in_memory().unwrap());
        {
            let db = lock(&db);
            // Installed, known to the scanner by the client's install id
            let mut anno = installed("0", "Anno 1800");
            anno.id = "ubisoft:4554".into();
            anno.platform = GamePlatform::Ubisoft;
            db.upsert_game(&anno).unwrap();
            // Installed, known to the scanner by another id altogether, with a trademark sign
            let mut rocket = installed("0", "Rocket League®");
            rocket.id = "epic:rocket-league".into();
            rocket.platform = GamePlatform::Epic;
            rocket.clean_title = "Rocket League".into();
            db.upsert_game(&rocket).unwrap();
            // The same title on another store is another copy
            db.upsert_game(&installed("1091500", "Cyberpunk 2077")).unwrap();
        }

        let mut anno = owned(GamePlatform::Ubisoft, "ubisoft:4553", "Anno 1800 Complete Edition");
        anno.other_ids.push("ubisoft:4554".into());
        let mut witcher = owned(GamePlatform::Gog, "gog:1207664643", "The Witcher 3: Wild Hunt");
        witcher.game.playtime_minutes = Some(6000);
        witcher.game.last_played_at = Some(1_700_000_000_000);
        let listed = vec![
            anno,
            owned(GamePlatform::Epic, "epic:Sugar", "ROCKET LEAGUE"),
            owned(GamePlatform::Gog, "gog:1423049311", "Cyberpunk 2077"),
            witcher.clone(),
            owned(GamePlatform::Gog, "gog:1495134320", "The Witcher 3: Wild Hunt - Game of the Year Edition"),
            owned(GamePlatform::Epic, "epic:nameless", "™"),
        ];

        let added = merge_owned_games(&db, listed.clone(), 500);
        let mut ids: Vec<&str> = added.iter().map(|game| game.id.as_str()).collect();
        ids.sort();
        assert_eq!(
            ids,
            ["gog:1207664643", "gog:1423049311"],
            "not the installed ones, not the second edition of a game just added, nothing nameless"
        );
        assert_eq!(lock(&db).game_count().unwrap(), 5);

        let stored = lock(&db).get_game("gog:1207664643").unwrap().expect("stored");
        assert!(!stored.is_installed);
        assert_eq!(stored.app_type.as_deref(), Some("game"));
        assert_eq!(stored.cover_path.as_deref(), Some("https://cdn.example/gog:1207664643.jpg"));
        assert_eq!(stored.playtime_minutes, Some(6000));
        assert_eq!(stored.updated_at, 500);
        assert_eq!(stored.steam_store_appid, None, "its Steam page is looked for later");

        // Again: nothing new, nothing rewritten
        assert!(merge_owned_games(&db, listed.clone(), 600).is_empty());
        assert!(merge_owned_games(&db, Vec::new(), 600).is_empty());

        // Played some more through the client: the hours follow, the user's own data stays
        {
            let db = lock(&db);
            let mut game = db.get_game("gog:1207664643").unwrap().unwrap();
            game.is_favorite = true;
            game.status = Some(crate::models::GameStatus::Playing);
            db.upsert_game(&game).unwrap();
        }
        witcher.game.playtime_minutes = Some(6120);
        let changed = merge_owned_games(&db, vec![witcher.clone()], 700);
        assert_eq!(changed.len(), 1);
        let stored = lock(&db).get_game("gog:1207664643").unwrap().unwrap();
        assert_eq!(stored.playtime_minutes, Some(6120));
        assert!(stored.is_favorite);
        assert_eq!(stored.status, Some(crate::models::GameStatus::Playing));
        assert_eq!(stored.updated_at, 500);

        // Once installed, the scanner's hours are the ones that count
        {
            let db = lock(&db);
            let mut game = db.get_game("gog:1207664643").unwrap().unwrap();
            game.is_installed = true;
            db.upsert_game(&game).unwrap();
        }
        witcher.game.playtime_minutes = Some(9000);
        assert!(merge_owned_games(&db, vec![witcher], 800).is_empty());
    }

    #[test]
    fn games_of_other_stores_are_filed_under_the_genres_of_their_steam_page() {
        let (base_url, recorded) = spawn_mock_api_server();
        let client = MetadataClient::with_base_urls(&base_url, &base_url, &base_url);
        let db = Mutex::new(LibraryDb::open_in_memory().unwrap());
        merge_owned_games(
            &db,
            vec![
                owned(GamePlatform::Ubisoft, "ubisoft:5059", "Assassin's Creed® Odyssey"),
                owned(GamePlatform::Ubisoft, "ubisoft:4553", "Anno 1800"),
                owned(GamePlatform::BattleNet, "Battle.net:fenris", "Diablo IV"),
                owned(GamePlatform::Gog, "gog:1", "Unknown Gem"),
            ],
            1,
        );
        // Not part of this: a Steam game, an installed game, a hidden one, one already settled
        {
            let db = lock(&db);
            db.upsert_game(&absent("620", "Portal 2")).unwrap();
            let mut installed_gog = installed("0", "Diablo IV");
            installed_gog.id = "gog:2".into();
            installed_gog.platform = GamePlatform::Gog;
            db.upsert_game(&installed_gog).unwrap();
            let mut hidden = owned(GamePlatform::Gog, "gog:3", "Diablo IV").game;
            hidden.is_hidden = true;
            db.upsert_game(&hidden).unwrap();
            let mut settled = owned(GamePlatform::Gog, "gog:4", "Diablo IV").game;
            settled.steam_store_appid = Some(String::new());
            db.upsert_game(&settled).unwrap();
        }
        let searches = |recorded: &std::sync::Arc<Mutex<Vec<String>>>| {
            recorded.lock().unwrap().iter().filter(|path| path.starts_with("/actions/SearchApps/")).count()
        };

        let step = match_absent_games_step(&db, &client, std::time::Duration::ZERO, 50);
        assert!(!step.unreachable);
        assert_eq!(step.remaining, 0);
        assert_eq!(searches(&recorded), 4, "one search per game without a page");

        let page = |id: &str| lock(&db).get_game(id).unwrap().unwrap().steam_store_appid;
        assert_eq!(page("ubisoft:5059").as_deref(), Some("812140"));
        assert_eq!(page("Battle.net:fenris").as_deref(), Some("2344520"));
        assert_eq!(page("ubisoft:4553").as_deref(), Some(""), "near misses are no page");
        assert_eq!(page("gog:1").as_deref(), Some(""));
        assert_eq!(page("gog:2"), None);
        assert_eq!(page("gog:3"), None);

        // The matched game the store answers about got its genres, tags and description;
        // the Steam game that was waiting too, in the same request
        let mut changed: Vec<&str> = step.changed.iter().map(|game| game.id.as_str()).collect();
        changed.sort();
        assert_eq!(changed, ["Battle.net:fenris", "steam:620"]);
        let diablo = lock(&db).get_game("Battle.net:fenris").unwrap().unwrap();
        assert_eq!(diablo.genres, ["GDR"]);
        assert_eq!(diablo.tags, ["GDR", "Stile Souls"]);
        assert_eq!(diablo.short_description.as_deref(), Some("GDR d'azione oscuro."));
        assert_eq!(diablo.cover_path.as_deref(), Some("https://cdn.example/Battle.net:fenris.jpg"));

        // Nothing left: no more searches
        let step = match_absent_games_step(&db, &client, std::time::Duration::ZERO, 60);
        assert_eq!((step.remaining, step.changed.len(), step.unreachable), (0, 0, false));
        assert_eq!(searches(&recorded), 4);
    }

    #[test]
    fn matching_goes_a_few_games_at_a_time_and_stops_while_steam_is_unreachable() {
        let (base_url, _recorded) = spawn_mock_api_server();
        let client = MetadataClient::with_base_urls(&base_url, &base_url, &base_url);
        let db = Mutex::new(LibraryDb::open_in_memory().unwrap());
        let many: Vec<OwnedGame> = (0..STORE_PAGE_STEP + 7)
            .map(|n| owned(GamePlatform::Epic, &format!("epic:game{n}"), &format!("Unknown Game {n}")))
            .collect();
        assert_eq!(merge_owned_games(&db, many, 1).len(), STORE_PAGE_STEP + 7);

        // Offline: nothing is settled, so nothing is lost
        let dead = "http://127.0.0.1:9";
        let offline = MetadataClient::with_base_urls(dead, dead, dead);
        let step = match_absent_games_step(&db, &offline, std::time::Duration::ZERO, 2);
        assert!(step.unreachable);
        assert_eq!(step.remaining, STORE_PAGE_STEP + 7);

        let step = match_absent_games_step(&db, &client, std::time::Duration::ZERO, 3);
        assert_eq!((step.remaining, step.unreachable), (7, false));
        let step = match_absent_games_step(&db, &client, std::time::Duration::ZERO, 4);
        assert_eq!((step.remaining, step.unreachable), (0, false));
    }

    /// Run by hand: the games the three client readers find (their `KANSO_*` overrides point
    /// at real or sample files) matched against the real Steam store.
    /// `KANSO_STEAM_COMMUNITY` is steamcommunity.com (the search), `KANSO_STEAM_STORE` the
    /// store site (tag names), `KANSO_STEAM_API` the store service.
    #[test]
    fn real_other_stores_when_clients_and_steam_are_provided() {
        let (Ok(api), Ok(store), Ok(community)) = (
            std::env::var("KANSO_STEAM_API"),
            std::env::var("KANSO_STEAM_STORE"),
            std::env::var("KANSO_STEAM_COMMUNITY"),
        ) else {
            return;
        };
        let listed = crate::scanner::read_owned_games_from_clients();
        if listed.is_empty() {
            return;
        }
        let mut client = MetadataClient::with_base_urls(&store, &store, &community);
        client.set_steam_api_base_url(&api);
        let db = Mutex::new(LibraryDb::open_in_memory().unwrap());
        let added = merge_owned_games(&db, listed, 1).len();

        let started = std::time::Instant::now();
        let mut completed = 0;
        loop {
            let step = match_absent_games_step(&db, &client, std::time::Duration::from_millis(200), 2);
            completed += step.changed.len();
            assert!(!step.unreachable, "Steam could not be asked");
            if step.remaining == 0 {
                break;
            }
        }
        let games = lock(&db).load_games().unwrap();
        let matched = games.iter().filter(|game| game.steam_store_id().is_some()).count();
        println!(
            "{added} games from the clients -> {matched} with a Steam page, {completed} completed, in {:.1} s",
            started.elapsed().as_secs_f32()
        );
        for game in &games {
            println!(
                "  {:9} {:52} page {:8} {:?} | {:?}",
                game.platform.as_str(),
                game.title,
                game.steam_store_id().unwrap_or("-"),
                game.genres,
                &game.tags[..game.tags.len().min(3)]
            );
        }
        assert!(matched > 0);
    }

    #[test]
    fn the_games_of_a_signed_in_epic_account_join_the_library() {
        use crate::epic_account::tests::{mock_api, spawn_mock_epic};

        let (base_url, seen) = spawn_mock_epic();
        let api = mock_api(&base_url);
        let session = api.exchange_code("goodcode1234").unwrap();
        let assets = api.library(&session).unwrap();
        let db = Mutex::new(LibraryDb::open_in_memory().unwrap());
        // Already in the library: installed, found by the scanner under the same app name
        let mut rocket = installed("0", "Rocket League");
        rocket.id = "epic:Sugar".into();
        rocket.platform = GamePlatform::Epic;
        lock(&db).upsert_game(&rocket).unwrap();
        let catalog_requests = |seen: &crate::epic_account::tests::Seen| {
            seen.lock().unwrap().iter().filter(|(request, _, _)| request.contains("/catalog/")).count()
        };

        let step = sync_epic_library_step(&db, &api, &session, &assets, std::time::Duration::ZERO, 10)
            .expect("Epic answered");
        assert_eq!(step.remaining, 0);
        let mut added: Vec<(&str, &str)> =
            step.added.iter().map(|game| (game.id.as_str(), game.title.as_str())).collect();
        added.sort();
        assert_eq!(
            added,
            [("epic:Calluna", "Control"), ("epic:dc9d2e595d0e4650b35d659f90d41059", "Alan Wake 2")],
            "the games; not the add-on, not what the catalog no longer has, not what is installed"
        );
        assert_eq!(catalog_requests(&seen), 4, "one request per item the library did not know");

        let alan = lock(&db).get_game("epic:dc9d2e595d0e4650b35d659f90d41059").unwrap().unwrap();
        assert!(!alan.is_installed);
        assert_eq!(alan.platform, GamePlatform::Epic);
        assert_eq!(alan.cover_path.as_deref(), Some("https://cdn1.epicgames.com/alan/tall.jpg?w=600&h=900&resize=1"));
        assert_eq!(
            alan.install_uri().as_deref(),
            Some("com.epicgames.launcher://apps/alanwake2%3Acat-alan%3Adc9d2e595d0e4650b35d659f90d41059?action=install")
        );
        // The library names the app even when the catalog item lists no release
        let control = lock(&db).get_game("epic:Calluna").unwrap().unwrap();
        assert_eq!(control.platform_app_id.as_deref(), Some("Calluna"));
        assert!(lock(&db).get_game("epic:Sugar").unwrap().unwrap().is_installed, "left as it was");

        // Next time everything is known: nothing is asked
        let step = sync_epic_library_step(&db, &api, &session, &assets, std::time::Duration::ZERO, 20).unwrap();
        assert_eq!((step.added.len(), step.remaining), (0, 0));
        assert_eq!(catalog_requests(&seen), 4);
        assert_eq!(
            lock(&db).known_store_non_games("epic").unwrap(),
            HashSet::from(["nightsprings".to_string(), "Gone".to_string()])
        );
    }

    #[test]
    fn an_epic_library_is_taken_a_few_items_at_a_time_and_a_refused_session_stops_it() {
        use crate::epic_account::tests::{mock_api, spawn_mock_epic};

        let (base_url, _seen) = spawn_mock_epic();
        let api = mock_api(&base_url);
        let session = api.exchange_code("goodcode1234").unwrap();
        let db = Mutex::new(LibraryDb::open_in_memory().unwrap());
        // More items than a step takes, none of which the catalog knows
        let many: Vec<EpicAsset> = (0..EPIC_STEP + 4)
            .map(|n| EpicAsset { app_name: format!("App{n}"), namespace: "gone".into(), catalog_item_id: format!("cat{n}") })
            .collect();

        let step = sync_epic_library_step(&db, &api, &session, &many, std::time::Duration::ZERO, 1).unwrap();
        assert_eq!((step.added.len(), step.remaining), (0, 4));
        let step = sync_epic_library_step(&db, &api, &session, &many, std::time::Duration::ZERO, 2).unwrap();
        assert_eq!((step.added.len(), step.remaining), (0, 0));

        // Signing in again elsewhere made this session stale: the step says so, adds nothing
        let _newer = api.exchange_code("goodcode1234").unwrap();
        let assets = [EpicAsset { app_name: "Calluna".into(), namespace: "control".into(), catalog_item_id: "cat-control".into() }];
        let refused = sync_epic_library_step(&db, &api, &session, &assets, std::time::Duration::ZERO, 3);
        assert!(matches!(refused, Err(EpicError::Refused(_))), "{refused:?}");
        assert_eq!(lock(&db).game_count().unwrap(), 0);
        assert!(!lock(&db).known_store_non_games("epic").unwrap().contains("Calluna"));
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
