//! SQLite persistence for the game library: the single source of truth for discovered games,
//! cached metadata, cover paths and everything the user customised.

use crate::i18n::T;
use crate::models::{now_timestamp_ms, DetectedGame, GamePlatform};
use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::Deserialize;
use std::fs;
use std::path::Path;

/// File name of the library database inside the app's local data folder.
pub const LIBRARY_DB_FILE: &str = "library.db";
/// Library file written by Kanso before the SQLite store existed.
pub const LEGACY_LIBRARY_JSON_FILE: &str = "library.json";

/// Bumped whenever [`MIGRATIONS`] gains an entry; stored in SQLite's `PRAGMA user_version`.
const MIGRATIONS: &[&str] = &["
    CREATE TABLE games (
        id                    TEXT PRIMARY KEY NOT NULL,
        title                 TEXT NOT NULL,
        clean_title           TEXT NOT NULL,
        platform              TEXT NOT NULL,
        platform_app_id       TEXT,
        install_dir           TEXT NOT NULL,
        executable_path       TEXT,
        launch_uri            TEXT,
        steamgriddb_id        INTEGER,
        cover_path            TEXT,
        short_description     TEXT,
        genres                TEXT NOT NULL DEFAULT '[]',
        app_type              TEXT,
        is_favorite           INTEGER NOT NULL DEFAULT 0,
        is_hidden             INTEGER NOT NULL DEFAULT 0,
        metadata_synced       INTEGER NOT NULL DEFAULT 0,
        custom_override       INTEGER NOT NULL DEFAULT 0,
        title_provisional     INTEGER NOT NULL DEFAULT 0,
        metadata_attempted_at INTEGER NOT NULL DEFAULT 0,
        last_played_at        INTEGER,
        play_count            INTEGER NOT NULL DEFAULT 0,
        updated_at            INTEGER NOT NULL DEFAULT 0
    );
",
    // v2: metadata language (everything cached so far was fetched in Italian), and one more
    // lookup for every game now that Steam's own cover art is a fallback source.
    "
    ALTER TABLE games ADD COLUMN metadata_lang TEXT;
    UPDATE games SET metadata_lang = 'it'
        WHERE short_description IS NOT NULL OR genres <> '[]';
    UPDATE games SET metadata_attempted_at = 0;
",
];

const GAME_COLUMNS: &str = "id, title, clean_title, platform, platform_app_id, install_dir, \
    executable_path, launch_uri, steamgriddb_id, cover_path, short_description, genres, app_type, \
    is_favorite, is_hidden, metadata_synced, custom_override, title_provisional, \
    metadata_attempted_at, last_played_at, play_count, updated_at, metadata_lang";

const UPSERT_GAME_SQL: &str = "
    INSERT INTO games (
        id, title, clean_title, platform, platform_app_id, install_dir, executable_path,
        launch_uri, steamgriddb_id, cover_path, short_description, genres, app_type, is_favorite,
        is_hidden, metadata_synced, custom_override, title_provisional, metadata_attempted_at,
        last_played_at, play_count, updated_at, metadata_lang
    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18,
              ?19, ?20, ?21, ?22, ?23)
    ON CONFLICT(id) DO UPDATE SET
        title = excluded.title,
        clean_title = excluded.clean_title,
        platform = excluded.platform,
        platform_app_id = excluded.platform_app_id,
        install_dir = excluded.install_dir,
        executable_path = excluded.executable_path,
        launch_uri = excluded.launch_uri,
        steamgriddb_id = excluded.steamgriddb_id,
        cover_path = excluded.cover_path,
        short_description = excluded.short_description,
        genres = excluded.genres,
        app_type = excluded.app_type,
        is_favorite = excluded.is_favorite,
        is_hidden = excluded.is_hidden,
        metadata_synced = excluded.metadata_synced,
        custom_override = excluded.custom_override,
        title_provisional = excluded.title_provisional,
        metadata_attempted_at = excluded.metadata_attempted_at,
        last_played_at = excluded.last_played_at,
        play_count = excluded.play_count,
        updated_at = excluded.updated_at,
        metadata_lang = excluded.metadata_lang";

/// Shape of the `library.json` file written by Kanso before the SQLite store existed.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct LegacyJsonLibrary {
    games: Vec<DetectedGame>,
}

/// Open connection to the library database (`library.db` in the app's local data folder).
#[derive(Debug)]
pub struct LibraryDb {
    conn: Connection,
}

impl LibraryDb {
    /// Opens (creating it and its parent folder if needed) the database at `path` and brings
    /// its schema up to date.
    pub fn open(path: &Path) -> Result<Self, String> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                format!("{} {}: {e}", T::CannotCreateFolder.s(), parent.display())
            })?;
        }
        let conn = Connection::open(path).map_err(|e| {
            format!("{} {}: {e}", T::CannotOpenLibraryDb.s(), path.display())
        })?;
        Self::from_connection(conn)
    }

    /// Opens the library stored in `data_dir` for normal app use and never fails:
    /// - a database that cannot be read is set aside (renamed, never deleted) and replaced by a
    ///   fresh one, since the games themselves are rediscovered by the next scan;
    /// - if no file can be created at all, the session runs on a volatile in-memory library;
    /// - a pre-SQLite `library.json` is imported once.
    ///
    /// Returns the database plus user-facing warnings about anything that went wrong.
    pub fn open_or_recover(data_dir: &Path) -> (Self, Vec<String>) {
        let db_path = data_dir.join(LIBRARY_DB_FILE);
        let mut warnings = Vec::new();

        let mut db = match Self::open(&db_path) {
            Ok(db) => db,
            Err(open_err) => {
                let backup = data_dir.join(format!(
                    "{LIBRARY_DB_FILE}.unreadable-{}",
                    now_timestamp_ms()
                ));
                let set_aside = db_path.is_file() && fs::rename(&db_path, &backup).is_ok();

                match Self::open(&db_path) {
                    Ok(db) if set_aside => {
                        warnings.push(format!(
                            "{} {}. {}",
                            T::LibraryDbSetAside.s(),
                            backup.display(),
                            T::LibraryDbRecreated.s()
                        ));
                        db
                    }
                    Ok(db) => db,
                    Err(_) => {
                        warnings.push(format!("{open_err} {}", T::SessionNotSaved.s()));
                        Self::open_in_memory()
                            .expect("SQLite must be able to create an in-memory database")
                    }
                }
            }
        };

        if let Err(import_err) = db.import_legacy_json(&data_dir.join(LEGACY_LIBRARY_JSON_FILE)) {
            warnings.push(format!("{}: {import_err}", T::LegacyImportFailed.s()));
        }

        (db, warnings)
    }

    /// Volatile database, for tests and as a last resort when the file cannot be opened.
    pub fn open_in_memory() -> Result<Self, String> {
        let conn = Connection::open_in_memory()
            .map_err(|e| format!("{}: {e}", T::CannotOpenLibraryDb.s()))?;
        Self::from_connection(conn)
    }

    fn from_connection(conn: Connection) -> Result<Self, String> {
        let mut db = Self { conn };
        db.migrate()?;
        Ok(db)
    }

    fn migrate(&mut self) -> Result<(), String> {
        let db_err = |e: rusqlite::Error| format!("{}: {e}", T::LibraryDbError.s());

        let current: i64 = self
            .conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .map_err(db_err)?;
        let current = usize::try_from(current).unwrap_or(0);
        if current > MIGRATIONS.len() {
            return Err(T::LibraryDbNewer.s().to_string());
        }

        for (index, migration) in MIGRATIONS.iter().enumerate().skip(current) {
            let tx = self.conn.transaction().map_err(db_err)?;
            tx.execute_batch(migration).map_err(db_err)?;
            tx.pragma_update(None, "user_version", (index + 1) as i64)
                .map_err(db_err)?;
            tx.commit().map_err(db_err)?;
        }
        Ok(())
    }

    /// All games, sorted by title (case-insensitive) then id.
    pub fn load_games(&self) -> Result<Vec<DetectedGame>, String> {
        let db_err = |e: rusqlite::Error| format!("{}: {e}", T::LibraryDbError.s());
        let mut stmt = self
            .conn
            .prepare(&format!("SELECT {GAME_COLUMNS} FROM games"))
            .map_err(db_err)?;
        let mut games = stmt
            .query_map([], game_from_row)
            .map_err(db_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_err)?;
        sort_games_by_title(&mut games);
        Ok(games)
    }

    pub fn get_game(&self, game_id: &str) -> Result<Option<DetectedGame>, String> {
        self.conn
            .query_row(
                &format!("SELECT {GAME_COLUMNS} FROM games WHERE id = ?1"),
                [game_id],
                game_from_row,
            )
            .optional()
            .map_err(|e| format!("{} ({game_id}): {e}", T::LibraryDbError.s()))
    }

    pub fn game_count(&self) -> Result<usize, String> {
        self.conn
            .query_row("SELECT COUNT(*) FROM games", [], |row| row.get::<_, i64>(0))
            .map(|n| usize::try_from(n).unwrap_or(0))
            .map_err(|e| format!("{}: {e}", T::LibraryDbError.s()))
    }

    /// Inserts or fully updates a single game.
    pub fn upsert_game(&self, game: &DetectedGame) -> Result<(), String> {
        execute_upsert(&self.conn, game)
            .map(|_| ())
            .map_err(|e| format!("{} ({}): {e}", T::LibraryDbError.s(), game.id))
    }

    /// Makes the table contain exactly `games`, atomically: either every change is stored or,
    /// on any failure, the previous library is left untouched.
    pub fn replace_all_games(&mut self, games: &[DetectedGame]) -> Result<(), String> {
        let db_err = |e: rusqlite::Error| format!("{}: {e}", T::LibraryDbError.s());
        let tx = self.conn.transaction().map_err(db_err)?;

        tx.execute_batch("CREATE TEMP TABLE IF NOT EXISTS keep_ids (id TEXT PRIMARY KEY); DELETE FROM keep_ids;")
            .map_err(db_err)?;
        for game in games {
            execute_upsert(&tx, game).map_err(db_err)?;
            tx.execute("INSERT OR IGNORE INTO keep_ids (id) VALUES (?1)", [&game.id])
                .map_err(db_err)?;
        }
        tx.execute(
            "DELETE FROM games WHERE id NOT IN (SELECT id FROM keep_ids)",
            [],
        )
        .map_err(db_err)?;
        tx.execute_batch("DELETE FROM keep_ids;").map_err(db_err)?;

        tx.commit().map_err(db_err)
    }

    /// One-time import of the pre-SQLite `library.json`. Runs only while the database is still
    /// empty; afterwards the file is renamed (`.imported`, or `.unreadable` when it could not be
    /// parsed) so it is kept as a backup but never read again. Returns how many games were imported.
    pub fn import_legacy_json(&mut self, json_path: &Path) -> Result<usize, String> {
        if !json_path.is_file() || self.game_count()? > 0 {
            return Ok(0);
        }

        let parsed = fs::read_to_string(json_path)
            .ok()
            .and_then(|raw| serde_json::from_str::<LegacyJsonLibrary>(&raw).ok());

        let Some(legacy) = parsed else {
            let _ = fs::rename(json_path, json_path.with_extension("json.unreadable"));
            return Ok(0);
        };

        self.replace_all_games(&legacy.games)?;
        let _ = fs::rename(json_path, json_path.with_extension("json.imported"));
        Ok(legacy.games.len())
    }
}

/// Case-insensitive alphabetical order used everywhere a library is listed.
pub fn sort_games_by_title(games: &mut [DetectedGame]) {
    games.sort_by(|a, b| {
        a.title
            .to_lowercase()
            .cmp(&b.title.to_lowercase())
            .then_with(|| a.id.cmp(&b.id))
    });
}

fn execute_upsert(conn: &Connection, game: &DetectedGame) -> rusqlite::Result<usize> {
    let genres_json = serde_json::to_string(&game.genres).unwrap_or_else(|_| "[]".to_string());
    conn.execute(
        UPSERT_GAME_SQL,
        params![
            game.id,
            game.title,
            game.clean_title,
            game.platform.as_str(),
            game.platform_app_id,
            game.install_dir,
            game.executable_path,
            game.launch_uri,
            game.steamgriddb_id.map(to_db_int),
            game.cover_path,
            game.short_description,
            genres_json,
            game.app_type,
            game.is_favorite,
            game.is_hidden,
            game.metadata_synced,
            game.custom_override,
            game.title_provisional,
            to_db_int(game.metadata_attempted_at),
            game.last_played_at.map(to_db_int),
            i64::from(game.play_count),
            to_db_int(game.updated_at),
            game.metadata_lang,
        ],
    )
}

fn game_from_row(row: &Row<'_>) -> rusqlite::Result<DetectedGame> {
    let platform_raw: String = row.get("platform")?;
    let genres_json: String = row.get("genres")?;

    Ok(DetectedGame {
        id: row.get("id")?,
        title: row.get("title")?,
        clean_title: row.get("clean_title")?,
        // An unknown platform can only come from a newer Kanso: keep the game, launchable as local
        platform: GamePlatform::parse(&platform_raw).unwrap_or(GamePlatform::Standalone),
        platform_app_id: row.get("platform_app_id")?,
        install_dir: row.get("install_dir")?,
        executable_path: row.get("executable_path")?,
        launch_uri: row.get("launch_uri")?,
        steamgriddb_id: row
            .get::<_, Option<i64>>("steamgriddb_id")?
            .map(from_db_int),
        cover_path: row.get("cover_path")?,
        short_description: row.get("short_description")?,
        genres: serde_json::from_str(&genres_json).unwrap_or_default(),
        app_type: row.get("app_type")?,
        is_favorite: row.get("is_favorite")?,
        is_hidden: row.get("is_hidden")?,
        metadata_synced: row.get("metadata_synced")?,
        custom_override: row.get("custom_override")?,
        title_provisional: row.get("title_provisional")?,
        metadata_attempted_at: from_db_int(row.get("metadata_attempted_at")?),
        last_played_at: row.get::<_, Option<i64>>("last_played_at")?.map(from_db_int),
        play_count: u32::try_from(row.get::<_, i64>("play_count")?).unwrap_or(0),
        updated_at: from_db_int(row.get("updated_at")?),
        metadata_lang: row.get("metadata_lang")?,
    })
}

/// SQLite integers are signed 64-bit; ids and millisecond timestamps always fit.
fn to_db_int(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

fn from_db_int(value: i64) -> u64 {
    u64::try_from(value).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn sample(id: &str, title: &str) -> DetectedGame {
        DetectedGame::new_detected(
            id.to_string(),
            title.to_string(),
            title.to_string(),
            GamePlatform::Steam,
            Some("1".to_string()),
            r"C:\Games\Sample".to_string(),
            None,
            Some("steam://rungameid/1".to_string()),
        )
    }

    #[test]
    fn round_trips_every_field_and_persists_across_reopen() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("appdata").join("library.db");

        let mut full = sample("gog:1207658924", "The Witcher® 3");
        full.clean_title = "The Witcher 3".to_string();
        full.platform = GamePlatform::Gog;
        full.executable_path = Some(r"D:\GOG\Witcher 3\bin\x64\witcher3.exe".to_string());
        full.launch_uri = None;
        full.steamgriddb_id = Some(5_276_290);
        full.cover_path = Some(r"C:\covers\gog_1207658924.png".to_string());
        full.short_description = Some("Caccia ai mostri: \"Geralt\" di Rivia.".to_string());
        full.genres = vec!["GDR".to_string(), "Azione".to_string()];
        full.app_type = Some("game".to_string());
        full.is_favorite = true;
        full.is_hidden = true;
        full.metadata_synced = true;
        full.custom_override = true;
        full.title_provisional = true;
        full.metadata_attempted_at = 1_731_500_000_000;
        full.last_played_at = Some(1_731_600_000_000);
        full.play_count = 7;
        full.metadata_lang = Some("ru".to_string());
        full.updated_at = 1_731_700_000_000;

        let bare = sample("steam:10", "alpha");

        {
            let db = LibraryDb::open(&db_path).expect("create db");
            db.upsert_game(&full).unwrap();
            db.upsert_game(&bare).unwrap();
        }

        let db = LibraryDb::open(&db_path).expect("reopen db");
        let games = db.load_games().unwrap();
        assert_eq!(games.len(), 2);
        // Sorted case-insensitively by title
        assert_eq!(games[0], bare);
        assert_eq!(games[1], full);
        assert_eq!(db.get_game("gog:1207658924").unwrap(), Some(full));
        assert_eq!(db.get_game("missing").unwrap(), None);
    }

    #[test]
    fn replace_all_updates_inserts_and_removes_in_one_go() {
        let mut db = LibraryDb::open_in_memory().unwrap();
        db.replace_all_games(&[sample("steam:1", "One"), sample("steam:2", "Two")])
            .unwrap();

        let mut renamed = sample("steam:2", "Two (renamed)");
        renamed.is_favorite = true;
        db.replace_all_games(&[renamed.clone(), sample("steam:3", "Three")])
            .unwrap();

        let ids: Vec<String> = db.load_games().unwrap().into_iter().map(|g| g.id).collect();
        assert_eq!(ids, vec!["steam:3", "steam:2"]);
        assert_eq!(db.get_game("steam:2").unwrap(), Some(renamed));

        db.replace_all_games(&[]).unwrap();
        assert_eq!(db.game_count().unwrap(), 0);
    }

    #[test]
    fn imports_legacy_library_json_once_and_keeps_it_as_backup() {
        let dir = tempdir().unwrap();
        let json_path = dir.path().join("library.json");
        // Written by a version that knew none of the newer fields
        fs::write(
            &json_path,
            r#"{"games":[{"id":"steam:1321440","title":"appid_1321440","cleanTitle":"appid_1321440",
                "platform":"steam","platformAppId":"1321440","installDir":"D:\\Steam\\steamapps\\common\\Cassette Beasts",
                "executablePath":null,"launchUri":"steam://rungameid/1321440","steamgriddbId":5330001,
                "coverPath":"C:\\covers\\steam_1321440.png","shortDescription":"Mostri.","genres":["Indie","GDR"],
                "isFavorite":false,"metadataSynced":true,"customOverride":false,"updatedAt":1700000000000},
               {"id":"battlenet:fenris","title":"Diablo IV","cleanTitle":"Diablo IV","platform":"Battle.net",
                "platformAppId":"Fenris","installDir":"C:\\Diablo IV","executablePath":"C:\\Diablo IV\\Diablo IV Launcher.exe",
                "launchUri":null,"customOverride":true}]}"#,
        )
        .unwrap();

        let mut db = LibraryDb::open_in_memory().unwrap();
        assert_eq!(db.import_legacy_json(&json_path).unwrap(), 2);

        let games = db.load_games().unwrap();
        assert_eq!(games.len(), 2);
        let cassette = db.get_game("steam:1321440").unwrap().unwrap();
        assert_eq!(cassette.genres, vec!["Indie", "GDR"]);
        assert_eq!(cassette.steamgriddb_id, Some(5_330_001));
        assert!(cassette.metadata_synced);
        let diablo = db.get_game("battlenet:fenris").unwrap().unwrap();
        assert_eq!(diablo.platform, GamePlatform::BattleNet);
        assert!(diablo.custom_override);

        assert!(!json_path.exists(), "the JSON must not be imported twice");
        assert!(dir.path().join("library.json.imported").is_file());
        assert_eq!(db.import_legacy_json(&json_path).unwrap(), 0);
    }

    #[test]
    fn legacy_import_never_overwrites_an_existing_library_or_fails_on_garbage() {
        let dir = tempdir().unwrap();
        let json_path = dir.path().join("library.json");

        // Unreadable JSON: set aside, library stays empty, no error
        fs::write(&json_path, "{ not json").unwrap();
        let mut db = LibraryDb::open_in_memory().unwrap();
        assert_eq!(db.import_legacy_json(&json_path).unwrap(), 0);
        assert!(dir.path().join("library.json.unreadable").is_file());

        // A populated database is never touched by a stray library.json
        db.upsert_game(&sample("steam:1", "Kept")).unwrap();
        fs::write(&json_path, r#"{"games":[]}"#).unwrap();
        assert_eq!(db.import_legacy_json(&json_path).unwrap(), 0);
        assert!(json_path.is_file());
        assert_eq!(db.game_count().unwrap(), 1);
    }

    #[test]
    fn refuses_a_database_from_a_newer_schema() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("library.db");
        {
            let conn = Connection::open(&db_path).unwrap();
            conn.pragma_update(None, "user_version", 999).unwrap();
        }
        assert!(LibraryDb::open(&db_path).is_err());
    }

    #[test]
    fn open_or_recover_sets_a_corrupt_database_aside_and_imports_the_legacy_json() {
        let dir = tempdir().unwrap();
        let data_dir = dir.path();
        fs::write(data_dir.join(LIBRARY_DB_FILE), b"this is not a sqlite database at all").unwrap();
        fs::write(
            data_dir.join(LEGACY_LIBRARY_JSON_FILE),
            r#"{"games":[{"id":"steam:1","title":"One","cleanTitle":"One","platform":"steam",
                "platformAppId":"1","installDir":"C:\\One","executablePath":null,"launchUri":null}]}"#,
        )
        .unwrap();

        let (db, warnings) = LibraryDb::open_or_recover(data_dir);

        assert_eq!(warnings.len(), 1, "warnings: {warnings:?}");
        assert!(warnings[0].contains("illeggibile"));
        let kept_aside = fs::read_dir(data_dir)
            .unwrap()
            .flatten()
            .any(|e| e.file_name().to_string_lossy().starts_with("library.db.unreadable-"));
        assert!(kept_aside, "the unreadable file must be kept, not deleted");

        // The fresh database works and picked up the old JSON library
        assert_eq!(db.game_count().unwrap(), 1);
        db.upsert_game(&sample("steam:2", "Two")).unwrap();
        drop(db);

        // A healthy database opens silently and is not re-imported
        let (db, warnings) = LibraryDb::open_or_recover(data_dir);
        assert!(warnings.is_empty(), "warnings: {warnings:?}");
        assert_eq!(db.game_count().unwrap(), 2);
    }
}
