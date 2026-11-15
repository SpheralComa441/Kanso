//! GOG Galaxy's local database: the GOG games of the account, installed or not.
//!
//! Galaxy 2.0 keeps the user's library in an SQLite file,
//! `%PROGRAMDATA%\GOG.com\Galaxy\storage\galaxy-2.0.db`. Kanso opens it read-only and reads
//! a handful of tables, the ones the widely used "GOG Galaxy Export Script" reads:
//! - `ProductPurchaseDates`: one row per game bought or added (`gameReleaseKey`);
//! - `GamePieces`: for each release key, pieces of description as JSON (`title`,
//!   `originalTitle`, `originalImages`, `dlcs`, ...), named through `GamePieceTypes`;
//! - `UserReleaseProperties` (`isHidden`), `GameTimes` (`minutesInGame`) and
//!   `LastPlayedDates` (`lastPlayedDate`).
//!
//! A release key is `<platform>_<id>`: only `gog_` keys are read here. Nothing else of the
//! database is touched, and no table of it is ever written.

use crate::cleaner::clean_game_title;
use crate::models::GamePlatform;
use crate::owned::OwnedGame;
use rusqlite::{Connection, OpenFlags};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Environment variable that points Kanso at a specific database file, for development and
/// end-to-end tests.
pub const GOG_GALAXY_DB_OVERRIDE_ENV: &str = "KANSO_GOG_GALAXY_DB";

const GOG_KEY_PREFIX: &str = "gog_";
/// SQL pattern for [`GOG_KEY_PREFIX`] (`_` is a wildcard in LIKE and has to be escaped).
const GOG_KEY_PATTERN: &str = r"gog\_%";

/// The database of the GOG Galaxy client on this PC, if there is one.
pub fn detect_gog_galaxy_db() -> Option<PathBuf> {
    if let Some(overridden) = std::env::var_os(GOG_GALAXY_DB_OVERRIDE_ENV) {
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
            .join("GOG.com")
            .join("Galaxy")
            .join("storage")
            .join("galaxy-2.0.db");
        path.is_file().then_some(path)
    }

    #[cfg(not(target_os = "windows"))]
    {
        None
    }
}

/// The GOG games in the Galaxy database at `path`. A database that cannot be opened, is busy
/// or does not look as expected simply gives no games: it is read again at the next sync.
pub fn read_owned_gog_games(path: &Path) -> Vec<OwnedGame> {
    read_database(path).unwrap_or_default()
}

fn read_database(path: &Path) -> rusqlite::Result<Vec<OwnedGame>> {
    let db = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    // Galaxy may be writing: wait a moment rather than fail, never longer
    db.busy_timeout(Duration::from_millis(1500))?;

    let piece_types: HashMap<i64, String> = db
        .prepare("SELECT id, type FROM GamePieceTypes")?
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;

    let owned_keys: Vec<String> = db
        .prepare(
            "SELECT DISTINCT gameReleaseKey FROM ProductPurchaseDates
             WHERE gameReleaseKey LIKE ?1 ESCAPE '\\' ORDER BY gameReleaseKey",
        )?
        .query_map([GOG_KEY_PATTERN], |row| row.get(0))?
        .collect::<rusqlite::Result<_>>()?;

    // The pieces of every GOG release: (release key, piece type) -> JSON value
    let mut pieces: HashMap<(String, String), Value> = HashMap::new();
    let mut statement = db.prepare(
        "SELECT releaseKey, gamePieceTypeId, value FROM GamePieces
         WHERE releaseKey LIKE ?1 ESCAPE '\\'",
    )?;
    let mut rows = statement.query([GOG_KEY_PATTERN])?;
    while let Some(row) = rows.next()? {
        let key: String = row.get(0)?;
        let Some(kind) = piece_types.get(&row.get::<_, i64>(1)?) else {
            continue;
        };
        let text: Option<String> = row.get(2)?;
        if let Some(value) = text.and_then(|text| serde_json::from_str::<Value>(&text).ok()) {
            pieces.insert((key, kind.clone()), value);
        }
    }
    let piece = |key: &str, kind: &str| pieces.get(&(key.to_string(), kind.to_string()));

    // Downloadable content is listed by the game it belongs to
    let dlc_keys: HashSet<&str> = pieces
        .iter()
        .filter(|((_, kind), _)| kind == "dlcs")
        .filter_map(|(_, value)| value.get("dlcs")?.as_array())
        .flatten()
        .filter_map(Value::as_str)
        .collect();

    // The three tables below are extras: a Galaxy without them still gives its games
    let hidden: HashSet<String> = column_pairs(
        &db,
        "SELECT releaseKey, isHidden FROM UserReleaseProperties WHERE isHidden = 1",
    )
    .into_iter()
    .map(|(key, _)| key)
    .collect();
    let mut minutes: HashMap<String, u64> = HashMap::new();
    for (key, value) in column_pairs(&db, "SELECT releaseKey, minutesInGame FROM GameTimes") {
        *minutes.entry(key).or_default() += value.as_u64().unwrap_or(0);
    }
    let last_played: HashMap<String, u64> = column_pairs(
        &db,
        "SELECT gameReleaseKey, lastPlayedDate FROM LastPlayedDates",
    )
    .into_iter()
    .filter_map(|(key, value)| Some((key, parse_timestamp_ms(value.as_str()?)?)))
    .collect();

    let mut games = Vec::new();
    for key in &owned_keys {
        let Some(product_id) = key
            .strip_prefix(GOG_KEY_PREFIX)
            .filter(|id| !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit()))
        else {
            continue;
        };
        if dlc_keys.contains(key.as_str()) || hidden.contains(key) {
            continue;
        }
        // "title" is what the user may have renamed the game to, "originalTitle" GOG's own
        let Some(title) = ["title", "originalTitle"].into_iter().find_map(|kind| {
            let title = piece(key, kind)?.get("title")?.as_str()?.trim();
            (!title.is_empty()).then(|| title.to_string())
        }) else {
            continue;
        };
        let cover = ["images", "originalImages"].into_iter().find_map(|kind| {
            let url = piece(key, kind)?.get("verticalCover")?.as_str()?.trim();
            url.starts_with("https://").then(|| url.to_string())
        });

        let mut owned = OwnedGame::new(
            GamePlatform::Gog,
            format!("gog:{product_id}"),
            &title,
            clean_game_title(&title),
            product_id.to_string(),
            None,
            cover,
        );
        owned.game.playtime_minutes = minutes
            .get(key)
            .filter(|total| **total > 0)
            .map(|total| u32::try_from(*total).unwrap_or(u32::MAX));
        owned.game.last_played_at = last_played.get(key).copied();
        games.push(owned);
    }
    Ok(games)
}

/// The rows of a two-column query as (text, JSON value); nothing when the table or one of
/// its columns is not there.
fn column_pairs(db: &Connection, sql: &str) -> Vec<(String, Value)> {
    let read = || -> rusqlite::Result<Vec<(String, Value)>> {
        db.prepare(sql)?
            .query_map([], |row| {
                let value = match row.get_ref(1)? {
                    rusqlite::types::ValueRef::Integer(number) => Value::from(number),
                    rusqlite::types::ValueRef::Real(number) => Value::from(number),
                    rusqlite::types::ValueRef::Text(text) => {
                        Value::from(String::from_utf8_lossy(text).into_owned())
                    }
                    _ => Value::Null,
                };
                Ok((row.get::<_, String>(0)?, value))
            })?
            .collect()
    };
    read().unwrap_or_default()
}

/// Milliseconds since the Unix epoch of a `YYYY-MM-DD HH:MM:SS` text (UTC), which is how
/// Galaxy writes the moment a game was last played. Anything else is not guessed at.
fn parse_timestamp_ms(text: &str) -> Option<u64> {
    let bytes = text.trim().as_bytes();
    if bytes.len() < 19 || bytes[4] != b'-' || bytes[7] != b'-' || bytes[13] != b':' || bytes[16] != b':' {
        return None;
    }
    let number = |from: usize, to: usize| -> Option<u64> {
        std::str::from_utf8(&bytes[from..to]).ok()?.parse().ok()
    };
    let (year, month, day) = (number(0, 4)?, number(5, 7)?, number(8, 10)?);
    let (hour, minute, second) = (number(11, 13)?, number(14, 16)?, number(17, 19)?);
    if year < 1970 || !(1..=12).contains(&month) || !(1..=31).contains(&day) || hour > 23 || minute > 59 || second > 60 {
        return None;
    }
    // Days from 1970-01-01 to the given date (proleptic Gregorian calendar)
    let (shifted_year, shifted_month) = if month <= 2 { (year - 1, month + 9) } else { (year, month - 3) };
    let era = shifted_year / 400;
    let year_of_era = shifted_year % 400;
    let day_of_year = (153 * shifted_month + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    let days = (era * 146_097 + day_of_era).checked_sub(719_468)?;
    Some((days * 86_400 + hour * 3_600 + minute * 60 + second) * 1_000)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A database with the tables and the kinds of rows Galaxy writes.
    pub(crate) fn sample_galaxy_db(path: &Path) {
        let db = Connection::open(path).unwrap();
        db.execute_batch(
            r#"
            CREATE TABLE GamePieceTypes (id INTEGER PRIMARY KEY, type TEXT NOT NULL);
            CREATE TABLE GamePieces (id INTEGER PRIMARY KEY, releaseKey TEXT NOT NULL,
                gamePieceTypeId INTEGER NOT NULL, userId INTEGER NOT NULL, value TEXT);
            CREATE TABLE ProductPurchaseDates (gameReleaseKey TEXT NOT NULL, userId INTEGER NOT NULL,
                purchaseDate TEXT);
            CREATE TABLE UserReleaseProperties (releaseKey TEXT NOT NULL, userId INTEGER NOT NULL,
                isHidden INTEGER NOT NULL DEFAULT 0);
            CREATE TABLE GameTimes (releaseKey TEXT NOT NULL, userId INTEGER NOT NULL, minutesInGame INTEGER);
            CREATE TABLE LastPlayedDates (gameReleaseKey TEXT NOT NULL, userId INTEGER NOT NULL, lastPlayedDate TEXT);

            INSERT INTO GamePieceTypes (id, type) VALUES
                (11, 'originalTitle'), (12, 'title'), (13, 'originalImages'), (14, 'images'),
                (15, 'dlcs'), (16, 'meta'), (17, 'allGameReleases');

            INSERT INTO ProductPurchaseDates (gameReleaseKey, userId, purchaseDate) VALUES
                ('gog_1207664643', 1, '2020-01-01 10:00:00'),
                ('gog_1207664643', 1, '2020-01-01 10:00:00'),
                ('gog_1495134320', 1, '2021-05-05 10:00:00'),
                ('gog_1207666883', 1, '2021-05-05 10:00:00'),
                ('gog_2093619782', 1, '2022-02-02 10:00:00'),
                ('gog_1444444444', 1, '2022-02-02 10:00:00'),
                ('gog_notanumber', 1, '2022-02-02 10:00:00'),
                ('gog_1555555555', 1, '2022-02-02 10:00:00'),
                ('steam_620', 1, '2019-01-01 10:00:00'),
                ('epic_Fortnite', 1, '2019-01-01 10:00:00');

            INSERT INTO GamePieces (releaseKey, gamePieceTypeId, userId, value) VALUES
                ('gog_1207664643', 11, 1, '{"title":"The Witcher 3: Wild Hunt - Game of the Year Edition"}'),
                ('gog_1207664643', 12, 1, '{"title":null}'),
                ('gog_1207664643', 13, 1, '{"background":"https://images.gog.com/bg.webp","squareIcon":"https://images.gog.com/icon.webp","verticalCover":"https://images.gog.com/witcher3_glx_vertical_cover.webp?namespace=gamesdb"}'),
                ('gog_1207664643', 15, 1, '{"dlcs":["gog_1207666883"]}'),
                ('gog_1207664643', 16, 1, '{"genres":["Role-playing (RPG)","Adventure"],"themes":["Fantasy"],"releaseDate":1431993600}'),
                ('gog_1495134320', 11, 1, '{"title":"The Witcher 3: Wild Hunt"}'),
                ('gog_1495134320', 12, 1, '{"title":"Witcher 3 (my copy)"}'),
                ('gog_1495134320', 13, 1, '{"verticalCover":"https://images.gog.com/original.webp"}'),
                ('gog_1495134320', 14, 1, '{"verticalCover":"https://images.gog.com/chosen-by-the-user.webp"}'),
                ('gog_1495134320', 15, 1, '{"dlcs":null}'),
                ('gog_1207666883', 11, 1, '{"title":"The Witcher 3: Wild Hunt - Blood and Wine"}'),
                ('gog_2093619782', 11, 1, '{"title":"Cyberpunk 2077"}'),
                ('gog_2093619782', 13, 1, '{"verticalCover":"http://not-secure.example/cover.jpg"}'),
                ('gog_1444444444', 11, 1, '{"title":"Hidden By The User"}'),
                ('gog_1555555555', 11, 1, 'not json at all'),
                ('steam_620', 11, 1, '{"title":"Portal 2"}');

            INSERT INTO UserReleaseProperties (releaseKey, userId, isHidden) VALUES
                ('gog_1444444444', 1, 1), ('gog_2093619782', 1, 0);
            INSERT INTO GameTimes (releaseKey, userId, minutesInGame) VALUES
                ('gog_1207664643', 1, 6000), ('gog_1207664643', 2, 30), ('gog_2093619782', 1, 0);
            INSERT INTO LastPlayedDates (gameReleaseKey, userId, lastPlayedDate) VALUES
                ('gog_1207664643', 1, '2023-11-14 22:13:20'), ('gog_2093619782', 1, NULL),
                ('gog_1495134320', 1, 'yesterday');
            "#,
        )
        .unwrap();
    }

    #[test]
    fn lists_the_gog_games_of_a_galaxy_database() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("galaxy-2.0.db");
        sample_galaxy_db(&path);

        let games = read_owned_gog_games(&path);
        let titles: Vec<&str> = games.iter().map(|owned| owned.game.title.as_str()).collect();
        assert_eq!(
            titles,
            [
                "The Witcher 3: Wild Hunt - Game of the Year Edition",
                "Witcher 3 (my copy)",
                "Cyberpunk 2077",
            ],
            "no DLC, nothing hidden, nothing nameless, no other store's games"
        );

        let witcher = &games[0].game;
        assert_eq!(witcher.id, "gog:1207664643");
        assert_eq!(witcher.clean_title, "The Witcher 3: Wild Hunt");
        assert_eq!(witcher.platform, GamePlatform::Gog);
        assert_eq!(witcher.platform_app_id.as_deref(), Some("1207664643"));
        assert!(!witcher.is_installed);
        assert_eq!(witcher.install_uri().as_deref(), Some("goggalaxy://openGameView/1207664643"));
        assert_eq!(
            witcher.cover_path.as_deref(),
            Some("https://images.gog.com/witcher3_glx_vertical_cover.webp?namespace=gamesdb")
        );
        assert_eq!(witcher.playtime_minutes, Some(6030), "every account's minutes on this PC");
        assert_eq!(witcher.last_played_at, Some(1_700_000_000_000));

        // The user's own name and artwork for the game win over GOG's
        assert_eq!(games[1].game.cover_path.as_deref(), Some("https://images.gog.com/chosen-by-the-user.webp"));
        assert_eq!(games[1].game.last_played_at, None, "an unknown date format is not guessed at");

        // Only secure addresses become covers; zero minutes is "never played"
        assert_eq!(games[2].game.cover_path, None);
        assert_eq!(games[2].game.playtime_minutes, None);
    }

    #[test]
    fn a_database_that_is_missing_or_something_else_gives_no_games() {
        let dir = tempfile::tempdir().unwrap();
        assert!(read_owned_gog_games(&dir.path().join("nothing-here.db")).is_empty());

        let not_a_database = dir.path().join("text.db");
        std::fs::write(&not_a_database, "hello").unwrap();
        assert!(read_owned_gog_games(&not_a_database).is_empty());

        // Another program's database: none of Galaxy's tables
        let other = dir.path().join("other.db");
        Connection::open(&other).unwrap().execute_batch("CREATE TABLE notes (text TEXT);").unwrap();
        assert!(read_owned_gog_games(&other).is_empty());

        // A Galaxy without the optional tables still gives its games
        let bare = dir.path().join("bare.db");
        Connection::open(&bare)
            .unwrap()
            .execute_batch(
                r#"CREATE TABLE GamePieceTypes (id INTEGER PRIMARY KEY, type TEXT);
                   CREATE TABLE GamePieces (releaseKey TEXT, gamePieceTypeId INTEGER, value TEXT);
                   CREATE TABLE ProductPurchaseDates (gameReleaseKey TEXT);
                   INSERT INTO GamePieceTypes VALUES (1, 'originalTitle');
                   INSERT INTO ProductPurchaseDates VALUES ('gog_42');
                   INSERT INTO GamePieces VALUES ('gog_42', 1, '{"title":"Beneath a Steel Sky"}');"#,
            )
            .unwrap();
        let games = read_owned_gog_games(&bare);
        assert_eq!(games.len(), 1);
        assert_eq!(games[0].game.id, "gog:42");
    }

    #[test]
    fn the_database_is_only_ever_read() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("galaxy-2.0.db");
        sample_galaxy_db(&path);
        let before = std::fs::read(&path).unwrap();
        read_owned_gog_games(&path);
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }

    #[test]
    fn reads_galaxys_timestamps() {
        assert_eq!(parse_timestamp_ms("1970-01-01 00:00:00"), Some(0));
        assert_eq!(parse_timestamp_ms("2023-11-14 22:13:20"), Some(1_700_000_000_000));
        assert_eq!(parse_timestamp_ms("2024-02-29 12:00:00.123"), Some(1_709_208_000_000));
        assert_eq!(parse_timestamp_ms("2024-13-01 00:00:00"), None);
        assert_eq!(parse_timestamp_ms("14/11/2023"), None);
        assert_eq!(parse_timestamp_ms(""), None);
    }

    /// Run by hand against a real database:
    /// `KANSO_GOG_GALAXY_DB=/path/galaxy-2.0.db cargo test -p kanso-core real_galaxy_db -- --nocapture`
    #[test]
    fn real_galaxy_db_when_one_is_provided() {
        let Some(path) = std::env::var_os(GOG_GALAXY_DB_OVERRIDE_ENV) else {
            return;
        };
        let games = read_database(Path::new(&path)).expect("a real database reads");
        println!("{} GOG games", games.len());
        for owned in games.iter().take(12) {
            let game = &owned.game;
            println!("  {} | {} | {:?} | {:?} min", game.id, game.title, game.cover_path, game.playtime_minutes);
        }
        assert!(!games.is_empty());
    }
}
