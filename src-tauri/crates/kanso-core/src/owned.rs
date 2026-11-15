//! Games the user has but that are not on disk.
//!
//! Two kinds end up here. Games Kanso once found installed simply stay in the library when
//! they are uninstalled (see [`crate::store::LibraryStore::merge_with_scan`]). And Steam games
//! the account has played but that are not installed are added from the Steam client's own
//! records: `localconfig.vdf` lists every app with play time, by appid only, so the names and
//! the kind of each app (game, DLC, soundtrack...) are asked to the public store service, many
//! at a time. Apps that turn out not to be games are remembered and never asked about again.

use crate::cleaner::clean_game_title;
use crate::db::LibraryDb;
use crate::metadata::MetadataClient;
use crate::models::{DetectedGame, GamePlatform};
use crate::scanner::steam::SteamPlayStats;
use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, MutexGuard};

/// Most apps looked up in one pass; the rest waits for the next sync.
const MAX_LOOKUPS_PER_PASS: usize = 400;

fn lock(db: &Mutex<LibraryDb>) -> MutexGuard<'_, LibraryDb> {
    db.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Brings the Steam games that are not installed in line with the Steam client's records:
/// - a game already listed as not installed gets its hours and last session refreshed (it may
///   have been played on another PC);
/// - every other app with play time that is not in the library is looked up, and added as a
///   not-installed game if the store says it is one.
///
/// Returns the games added or changed, as stored. Network trouble adds nothing and is retried
/// at the next sync; the database is never locked while Steam is being asked.
pub fn sync_steam_played_games(
    db: &Mutex<LibraryDb>,
    client: &MetadataClient,
    play_stats: &HashMap<String, SteamPlayStats>,
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
        (ids, db.known_steam_non_games().unwrap_or_default())
    };

    // Most recently played first: with a long list, those are the ones worth seeing first
    let mut candidates: Vec<(u64, &SteamPlayStats)> = play_stats
        .iter()
        .filter(|(_, stats)| stats.playtime_minutes > 0)
        .filter_map(|(appid, stats)| Some((appid.trim().parse::<u64>().ok()?, stats)))
        .filter(|(appid, _)| {
            !library_ids.contains(&format!("steam:{appid}")) && !known_non_games.contains(appid)
        })
        .collect();
    candidates.sort_by_key(|(appid, stats)| (std::cmp::Reverse(stats.last_played_secs), *appid));
    candidates.truncate(MAX_LOOKUPS_PER_PASS);
    if candidates.is_empty() {
        return changed;
    }

    let appids: Vec<u64> = candidates.iter().map(|(appid, _)| *appid).collect();
    let Some(summaries) = client.fetch_steam_app_summaries(&appids) else {
        return changed;
    };

    let mut non_games: Vec<(u64, &str)> = Vec::new();
    let mut found: Vec<DetectedGame> = Vec::new();
    for summary in &summaries {
        let stats = candidates
            .iter()
            .find(|(appid, _)| *appid == summary.appid)
            .map(|(_, stats)| *stats);
        match (&summary.name, summary.is_game, stats) {
            (Some(name), true, Some(stats)) => {
                let appid = summary.appid;
                let mut game = DetectedGame::new_detected(
                    format!("steam:{appid}"),
                    name.clone(),
                    clean_game_title(name),
                    GamePlatform::Steam,
                    Some(appid.to_string()),
                    String::new(),
                    None,
                    Some(format!("steam://rungameid/{appid}")),
                );
                game.is_installed = false;
                game.app_type = Some("game".to_string());
                game.playtime_minutes = Some(stats.playtime_minutes);
                game.last_played_at =
                    (stats.last_played_secs > 0).then(|| stats.last_played_secs.saturating_mul(1000));
                game.updated_at = now_ms;
                found.push(game);
            }
            (Some(_), _, _) => non_games.push((summary.appid, "other")),
            (None, _, _) => non_games.push((summary.appid, "unlisted")),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metadata::tests::spawn_mock_api_server;

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

    #[test]
    fn played_steam_games_that_are_not_installed_join_the_library() {
        let (base_url, recorded) = spawn_mock_api_server();
        let client = MetadataClient::with_base_urls(&base_url, &base_url, &base_url);
        let db = Mutex::new(LibraryDb::open_in_memory().unwrap());
        lock(&db).upsert_game(&installed("1145360", "Hades")).unwrap();

        let play_stats = HashMap::from([
            ("1145360".to_string(), stats(2112, 1_700_000_000)), // installed
            ("620".to_string(), stats(540, 1_650_000_000)),      // a game, not installed
            ("220".to_string(), stats(30, 0)),                   // another one, never dated
            ("1880832".to_string(), stats(15, 1_600_000_000)),   // a DLC
            ("228980".to_string(), stats(999, 1_600_000_000)),   // a tool the store does not list
            ("400".to_string(), stats(0, 0)),                    // owned config entry, never played
            ("not-a-number".to_string(), stats(5, 5)),
        ]);

        let added = sync_steam_played_games(&db, &client, &play_stats, 42);
        let mut titles: Vec<&str> = added.iter().map(|g| g.title.as_str()).collect();
        titles.sort();
        assert_eq!(titles, ["Half-Life® 2", "Portal 2"], "games only, with their official names");

        let portal = lock(&db).get_game("steam:620").unwrap().expect("stored");
        assert!(!portal.is_installed);
        assert_eq!(portal.playtime_minutes, Some(540));
        assert_eq!(portal.last_played_at, Some(1_650_000_000_000));
        assert_eq!(portal.platform_app_id.as_deref(), Some("620"));
        assert_eq!(portal.install_uri().as_deref(), Some("steam://install/620"));
        assert_eq!(portal.app_type.as_deref(), Some("game"));
        assert_eq!(portal.install_dir, "");
        let half_life = lock(&db).get_game("steam:220").unwrap().expect("stored");
        assert_eq!(half_life.clean_title, "Half-Life 2");
        assert_eq!(half_life.last_played_at, None);
        assert!(lock(&db).get_game("steam:1145360").unwrap().unwrap().is_installed);
        assert_eq!(lock(&db).game_count().unwrap(), 3);

        // One request for the four unknown apps; the never-played one is not even asked about
        let lookups = |recorded: &std::sync::Arc<Mutex<Vec<String>>>| {
            recorded
                .lock()
                .unwrap()
                .iter()
                .filter(|path| path.starts_with("/IStoreBrowseService/GetItems/v1/"))
                .cloned()
                .collect::<Vec<_>>()
        };
        let asked = lookups(&recorded);
        assert_eq!(asked.len(), 1, "{asked:?}");
        assert!(!asked[0].contains("%3A400%7D") && !asked[0].contains("1145360"), "{}", asked[0]);

        // Next time nothing is new: the DLC and the tool are remembered, nothing is asked
        assert_eq!(
            lock(&db).known_steam_non_games().unwrap(),
            HashSet::from([1_880_832, 228_980])
        );
        assert!(sync_steam_played_games(&db, &client, &play_stats, 43).is_empty());
        assert_eq!(lookups(&recorded).len(), 1);

        // Played some more on another PC: the hours of the game that is not installed follow
        let later = HashMap::from([("620".to_string(), stats(600, 1_660_000_000))]);
        let changed = sync_steam_played_games(&db, &client, &later, 44);
        assert_eq!(changed.len(), 1);
        assert_eq!(changed[0].playtime_minutes, Some(600));
        assert_eq!(changed[0].last_played_at, Some(1_660_000_000_000));
        assert_eq!(lookups(&recorded).len(), 1);
    }

    #[test]
    fn nothing_is_added_while_steam_cannot_be_reached() {
        let dead = "http://127.0.0.1:9";
        let client = MetadataClient::with_base_urls(dead, dead, dead);
        let db = Mutex::new(LibraryDb::open_in_memory().unwrap());
        let play_stats = HashMap::from([("620".to_string(), stats(540, 1_650_000_000))]);

        assert!(sync_steam_played_games(&db, &client, &play_stats, 1).is_empty());
        assert_eq!(lock(&db).game_count().unwrap(), 0);
        assert!(lock(&db).known_steam_non_games().unwrap().is_empty(), "asked again next time");
    }
}
