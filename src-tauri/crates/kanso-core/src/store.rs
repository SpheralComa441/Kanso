use crate::cleaner::{clean_game_title, is_placeholder_title};
use crate::db::{sort_games_by_title, LibraryDb};
use crate::i18n::T;
use crate::metadata::{remove_existing_cover_variants, sanitize_filename_id};
use crate::metadata::title_key;
use crate::models::{
    now_timestamp_ms, DetectedGame, GameManualUpdate, GamePlatform, GameStatus, ScanReport,
};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

/// In-memory view of the library with the rules for merging scans, manual edits and
/// automatic metadata. Persistence lives in [`LibraryDb`].
#[derive(Debug, Clone, Default)]
pub struct LibraryStore {
    pub games: Vec<DetectedGame>,
}

/// Something the user did with the game, which outlives its installation.
/// Most genres a user adds to one game, and the longest a genre's name gets.
const MAX_CUSTOM_GENRES: usize = 24;
const MAX_GENRE_CHARS: usize = 40;

fn has_user_data(game: &DetectedGame) -> bool {
    game.is_favorite
        || game.status.is_some()
        || !game.custom_genres.is_empty()
        || game.custom_override
        || game.play_count > 0
        || game.playtime_minutes.is_some_and(|minutes| minutes > 0)
}

impl LibraryStore {
    /// Reads the whole library from the database.
    pub fn load(db: &LibraryDb) -> Result<Self, String> {
        Ok(Self {
            games: db.load_games()?,
        })
    }

    /// Atomically makes the database match this store (used after a scan, which can both add
    /// and remove games). Single-game changes go through [`LibraryDb::upsert_game`] instead.
    pub fn save_all(&self, db: &mut LibraryDb) -> Result<(), String> {
        db.replace_all_games(&self.games)
    }

    /// Merges freshly scanned games with cached metadata, covers, favorites, and user overrides.
    pub fn merge_with_scan(&mut self, mut report: ScanReport) -> ScanReport {
        let existing_by_id: HashMap<String, DetectedGame> = self
            .games
            .iter()
            .cloned()
            .map(|g| (g.id.clone(), g))
            .collect();

        for scanned in &mut report.games {
            if let Some(cached) = existing_by_id.get(&scanned.id) {
                scanned.steamgriddb_id = cached.steamgriddb_id;
                scanned.cover_path = cached
                    .cover_path
                    .clone()
                    .filter(|p| Path::new(p).is_file());
                scanned.short_description = cached.short_description.clone();
                scanned.genres = cached.genres.clone();
                scanned.app_type = cached.app_type.clone();
                scanned.is_favorite = cached.is_favorite;
                scanned.is_hidden = cached.is_hidden;
                scanned.metadata_synced = cached.metadata_synced;
                scanned.metadata_attempted_at = cached.metadata_attempted_at;
                // The scan brings what the game's client recorded (Steam); a launch from
                // Kanso may be more recent than that
                scanned.last_played_at = scanned.last_played_at.max(cached.last_played_at);
                scanned.playtime_minutes = scanned.playtime_minutes.or(cached.playtime_minutes);
                scanned.status = cached.status;
                scanned.custom_genres = cached.custom_genres.clone();
                // Counts come from the Steam client's cache, which may be missing for a while
                if scanned.achievements_total.is_none() {
                    scanned.achievements_unlocked = cached.achievements_unlocked;
                    scanned.achievements_total = cached.achievements_total;
                }
                scanned.steam_store_appid = cached.steam_store_appid.clone();
                scanned.play_count = cached.play_count;
                scanned.metadata_lang = cached.metadata_lang.clone();
                scanned.tags = cached.tags.clone();
                scanned.tags_lang = cached.tags_lang.clone();
                scanned.custom_override = cached.custom_override;
                scanned.updated_at = cached.updated_at;

                // A cached `appid_<id>` (written by versions that trusted Steam's placeholder
                // name) is never worth keeping: the freshly scanned title always beats it.
                let cached_title_usable = !is_placeholder_title(&cached.title);

                if cached.custom_override {
                    if cached_title_usable {
                        scanned.title = cached.title.clone();
                        scanned.clean_title = cached.clean_title.clone();
                        scanned.title_provisional = cached.title_provisional;
                    }
                    if cached.executable_path.is_some() {
                        scanned.executable_path = cached.executable_path.clone();
                    }
                } else if scanned.title_provisional
                    && cached_title_usable
                    && !cached.title_provisional
                {
                    // The official name was already resolved online by a previous sync:
                    // keep it instead of falling back to the install-folder guess again.
                    scanned.title = cached.title.clone();
                    scanned.clean_title = cached.clean_title.clone();
                    scanned.title_provisional = false;
                }

                // An emulated game is known by the name the scan reads from its files. When
                // that name changes (Kanso learnt to read the game's title file, the file was
                // renamed) the cover and description found under the old name belonged to
                // another game: they are looked up again. A game the user edited is theirs.
                if scanned.platform == GamePlatform::Emulator && !cached.custom_override && cached.title != scanned.title {
                    scanned.steamgriddb_id = None;
                    scanned.cover_path = None;
                    scanned.short_description = None;
                    scanned.genres = Vec::new();
                    scanned.tags = Vec::new();
                    scanned.steam_store_appid = None;
                    scanned.metadata_synced = false;
                    scanned.metadata_attempted_at = 0;
                }
            }
        }

        // Games that were in the library and are no longer on disk stay, marked as not
        // installed: their status, hours and place in the statistics are the user's, and a
        // store game can be installed again. A loose executable from a custom folder with
        // nothing attached to it is simply gone.
        let on_disk: HashSet<&str> = report.games.iter().map(|g| g.id.as_str()).collect();
        let gone: Vec<DetectedGame> = self
            .games
            .iter()
            .filter(|game| !on_disk.contains(game.id.as_str()))
            .filter(|game| {
                // A loose executable, or a game file of an emulator, that is no longer there
                // and that the user never touched is simply gone
                !matches!(game.platform, GamePlatform::Standalone | GamePlatform::Emulator)
                    || has_user_data(game)
            })
            .cloned()
            .map(|mut game| {
                // A plugin's game was never looked for on disk: whether it can be started
                // is what its plugin says, and the scan leaves it as it is
                if game.platform != GamePlatform::Plugin {
                    game.is_installed = false;
                    // Nothing on disk says a version any more
                    game.version = None;
                }
                game
            })
            .collect();

        // A game found on disk may already be in the library as not installed under another
        // id: a store account or a client's records can know it by a different one than the
        // scanner does. The entry on disk takes over what was the user's and the other goes,
        // or the game would be listed twice.
        let mut taken_over: HashSet<String> = HashSet::new();
        for scanned in &mut report.games {
            if matches!(scanned.platform, GamePlatform::Standalone | GamePlatform::Emulator)
                || existing_by_id.contains_key(&scanned.id)
            {
                continue;
            }
            let title = title_key(&scanned.clean_title);
            let twin = gone.iter().find(|absent| {
                absent.platform == scanned.platform
                    && !title.is_empty()
                    && title_key(&absent.clean_title) == title
                    && !taken_over.contains(&absent.id)
            });
            if let Some(twin) = twin {
                scanned.status = twin.status;
                scanned.custom_genres = twin.custom_genres.clone();
                scanned.is_favorite = twin.is_favorite;
                scanned.is_hidden = twin.is_hidden;
                scanned.playtime_minutes = scanned.playtime_minutes.max(twin.playtime_minutes);
                scanned.last_played_at = scanned.last_played_at.max(twin.last_played_at);
                scanned.play_count = twin.play_count;
                scanned.short_description = twin.short_description.clone();
                scanned.genres = twin.genres.clone();
                scanned.metadata_lang = twin.metadata_lang.clone();
                scanned.tags = twin.tags.clone();
                scanned.tags_lang = twin.tags_lang.clone();
                scanned.steam_store_appid = twin.steam_store_appid.clone();
                scanned.steamgriddb_id = twin.steamgriddb_id;
                scanned.app_type = twin.app_type.clone();
                taken_over.insert(twin.id.clone());
            }
        }
        let gone = gone.into_iter().filter(|absent| !taken_over.contains(&absent.id));
        report.games.extend(gone);

        sort_games_by_title(&mut report.games);

        self.games = report.games.clone();
        report
    }

    pub fn get_game_mut(&mut self, game_id: &str) -> Option<&mut DetectedGame> {
        self.games.iter_mut().find(|g| g.id == game_id)
    }

    pub fn upsert_game(&mut self, updated: DetectedGame) {
        if let Some(slot) = self.games.iter_mut().find(|g| g.id == updated.id) {
            *slot = updated;
        } else {
            self.games.push(updated);
        }
        sort_games_by_title(&mut self.games);
    }

    /// Writes back a game whose metadata was refreshed outside the store lock (slow HTTP work),
    /// keeping whatever the user changed in the meantime. Returns the game as stored, or `None`
    /// when the write is dropped: the game left the library, or the user saved a manual
    /// override while an automatic sync was still in flight.
    ///
    /// `auto_hide` is set when this very lookup classified the app as a non-game (soundtrack,
    /// DLC, ...): that is the only way a sync may change a game's visibility.
    pub fn commit_enriched_game(
        &mut self,
        mut enriched: DetectedGame,
        auto_hide: bool,
    ) -> Option<DetectedGame> {
        let existing = self.games.iter().find(|g| g.id == enriched.id)?;
        if existing.custom_override && !enriched.custom_override {
            return None;
        }
        // User-owned state may have changed while the HTTP requests were in flight
        enriched.is_favorite = existing.is_favorite;
        enriched.last_played_at = existing.last_played_at;
        enriched.playtime_minutes = existing.playtime_minutes;
        enriched.status = existing.status;
        enriched.custom_genres = existing.custom_genres.clone();
        // What the scan read from disk, not something a lookup online knows
        enriched.version = existing.version.clone();
        enriched.is_family_shared = existing.is_family_shared;
        enriched.achievements_unlocked = existing.achievements_unlocked;
        enriched.achievements_total = existing.achievements_total;
        enriched.play_count = existing.play_count;
        enriched.is_hidden = existing.is_hidden || auto_hide;
        self.upsert_game(enriched.clone());
        Some(enriched)
    }

    /// Marks or unmarks a game as favorite. Unlike [`Self::apply_manual_update`] this is not a
    /// metadata override: automatic sync keeps managing the game.
    pub fn set_favorite(&mut self, game_id: &str, is_favorite: bool) -> Result<DetectedGame, String> {
        let Some(game) = self.get_game_mut(game_id) else {
            return Err(format!("{}: {game_id}", T::GameNotFound.s()));
        };
        game.is_favorite = is_favorite;
        Ok(game.clone())
    }

    /// Hides a game from the library (or shows it again). Like a favorite, not an override.
    pub fn set_hidden(&mut self, game_id: &str, is_hidden: bool) -> Result<DetectedGame, String> {
        let Some(game) = self.get_game_mut(game_id) else {
            return Err(format!("{}: {game_id}", T::GameNotFound.s()));
        };
        game.is_hidden = is_hidden;
        Ok(game.clone())
    }

    /// Sets or clears where the user stands with a game.
    pub fn set_status(
        &mut self,
        game_id: &str,
        status: Option<GameStatus>,
    ) -> Result<DetectedGame, String> {
        let Some(game) = self.get_game_mut(game_id) else {
            return Err(format!("{}: {game_id}", T::GameNotFound.s()));
        };
        game.status = status;
        Ok(game.clone())
    }

    /// Sets the genres the user gives a game besides its store's own: plain names, each once
    /// (whatever the capitals), and none that the game already has from its store.
    pub fn set_custom_genres(&mut self, game_id: &str, genres: &[String]) -> Result<DetectedGame, String> {
        let Some(game) = self.get_game_mut(game_id) else {
            return Err(format!("{}: {game_id}", T::GameNotFound.s()));
        };
        let mut seen: HashSet<String> = game.genres.iter().map(|genre| genre.trim().to_lowercase()).collect();
        game.custom_genres = genres
            .iter()
            .map(|genre| {
                genre
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ")
                    .chars()
                    .filter(|c| !c.is_control())
                    .take(MAX_GENRE_CHARS)
                    .collect::<String>()
            })
            .filter(|genre| !genre.is_empty() && seen.insert(genre.to_lowercase()))
            .take(MAX_CUSTOM_GENRES)
            .collect();
        Ok(game.clone())
    }

    /// Records a successful launch for the "recently played" view.
    pub fn record_launch(&mut self, game_id: &str, now_ms: u64) -> Result<DetectedGame, String> {
        let Some(game) = self.get_game_mut(game_id) else {
            return Err(format!("{}: {game_id}", T::GameNotFound.s()));
        };
        game.last_played_at = Some(now_ms);
        game.play_count = game.play_count.saturating_add(1);
        Ok(game.clone())
    }

    /// Applies manual edits (title, executable path, genres, description, favorite) to a single game.
    pub fn apply_manual_update(&mut self, payload: GameManualUpdate) -> Result<DetectedGame, String> {
        let Some(game) = self.get_game_mut(&payload.id) else {
            return Err(format!("{}: {}", T::GameNotFound.s(), payload.id));
        };

        let trimmed_title = payload.title.trim();
        if !trimmed_title.is_empty() {
            game.title = trimmed_title.to_string();
            game.clean_title = clean_game_title(trimmed_title);
            // The user has seen and confirmed (or rewritten) the title
            game.title_provisional = false;
        }

        game.executable_path = payload
            .executable_path
            .map(|p| p.trim().to_string())
            .filter(|p| !p.is_empty());

        game.short_description = payload
            .short_description
            .map(|d| d.trim().to_string())
            .filter(|d| !d.is_empty());

        game.genres = payload
            .genres
            .into_iter()
            .map(|g| g.trim().to_string())
            .filter(|g| !g.is_empty())
            .collect();

        if let Some(fav) = payload.is_favorite {
            game.is_favorite = fav;
        }

        game.custom_override = true;
        game.updated_at = now_timestamp_ms();

        let result = game.clone();
        sort_games_by_title(&mut self.games);
        Ok(result)
    }

    /// Copies a user-selected local image file into `covers/` and assigns it to `game_id`.
    pub fn set_local_cover_file(
        &mut self,
        game_id: &str,
        source_image_path: &Path,
        covers_dir: &Path,
    ) -> Result<DetectedGame, String> {
        if !source_image_path.is_file() {
            return Err(format!(
                "{}: {}",
                T::ImageFileNotFound.s(),
                source_image_path.display()
            ));
        }

        let ext = source_image_path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
            .filter(|e| matches!(e.as_str(), "png" | "jpg" | "jpeg" | "webp"))
            .unwrap_or_else(|| "png".to_string());

        fs::create_dir_all(covers_dir).map_err(|e| {
            format!("{} {}: {e}", T::CannotCreateFolder.s(), covers_dir.display())
        })?;

        let safe_stem = sanitize_filename_id(game_id);
        remove_existing_cover_variants(covers_dir, &safe_stem);
        let dest = covers_dir.join(format!("{safe_stem}.{ext}"));

        fs::copy(source_image_path, &dest).map_err(|e| {
            format!("{} ({}): {e}", T::CoverSaveFailed.s(), dest.display())
        })?;

        let Some(game) = self.get_game_mut(game_id) else {
            return Err(format!("{}: {game_id}", T::GameNotFound.s()));
        };

        game.cover_path = Some(dest.to_string_lossy().to_string());
        game.custom_override = true;
        game.metadata_synced = true;
        game.updated_at = now_timestamp_ms();

        Ok(game.clone())
    }

    /// Builds a `ScanReport` snapshot from the currently stored games.
    pub fn to_report(&self) -> ScanReport {
        ScanReport {
            games: self.games.clone(),
            warnings: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::GamePlatform;
    use tempfile::tempdir;

    #[test]
    fn a_game_installed_under_another_id_takes_the_place_of_its_not_installed_entry() {
        // Listed by a store account under the store's own id, with what the user set on it
        let mut from_account = DetectedGame::new_detected(
            "ubisoft:f40e304d-8e8a-4a2b-b4a4-000000000001".into(),
            "Assassin's Creed\u{ae} Odyssey".into(),
            "Assassin's Creed Odyssey".into(),
            GamePlatform::Ubisoft,
            None,
            String::new(),
            None,
            None,
        );
        from_account.is_installed = false;
        from_account.is_favorite = true;
        from_account.status = Some(crate::models::GameStatus::Backlog);
        from_account.genres = vec!["GDR".into()];
        from_account.tags = vec!["Mondo aperto".into()];
        from_account.steam_store_appid = Some("812140".into());
        // Same title on another store: another copy, left alone
        let mut on_epic = from_account.clone();
        on_epic.id = "epic:odyssey".into();
        on_epic.platform = GamePlatform::Epic;
        let mut store = LibraryStore { games: vec![from_account, on_epic] };

        // The scanner finds it installed, under the launcher's number
        let scanned = DetectedGame::new_detected(
            "ubisoft:5059".into(),
            "Assassin's Creed Odyssey".into(),
            "Assassin's Creed Odyssey".into(),
            GamePlatform::Ubisoft,
            Some("5059".into()),
            "C:/Games/ACOdyssey".into(),
            None,
            Some("uplay://launch/5059/0".into()),
        );
        let report = store.merge_with_scan(ScanReport { games: vec![scanned], warnings: vec![] });

        let mut ids: Vec<&str> = report.games.iter().map(|game| game.id.as_str()).collect();
        ids.sort();
        assert_eq!(ids, ["epic:odyssey", "ubisoft:5059"], "one entry per copy, not two for Ubisoft's");
        let installed = report.games.iter().find(|game| game.id == "ubisoft:5059").unwrap();
        assert!(installed.is_installed);
        assert!(installed.is_favorite);
        assert_eq!(installed.status, Some(crate::models::GameStatus::Backlog));
        assert_eq!(installed.genres, ["GDR"]);
        assert_eq!(installed.steam_store_appid.as_deref(), Some("812140"));
        assert_eq!(installed.launch_uri.as_deref(), Some("uplay://launch/5059/0"));
        let on_epic = report.games.iter().find(|game| game.id == "epic:odyssey").unwrap();
        assert!(!on_epic.is_installed);
    }

    #[test]
    fn preserves_manual_overrides_across_rescans() {
        let tmp = tempdir().unwrap();
        let cover_file = tmp.path().join("custom_cover.png");
        fs::write(&cover_file, b"pngbytes").unwrap();

        let mut store = LibraryStore::default();
        let mut initial = DetectedGame::new_detected(
            "battlenet:fenris".to_string(),
            "Fenris".to_string(),
            "Fenris".to_string(),
            GamePlatform::BattleNet,
            Some("Fenris".to_string()),
            r"C:\Games\Diablo IV".to_string(),
            Some(r"C:\Games\Diablo IV\Diablo IV Launcher.exe".to_string()),
            None,
        );
        initial.cover_path = Some(cover_file.to_string_lossy().to_string());
        store.upsert_game(initial);

        store
            .apply_manual_update(GameManualUpdate {
                id: "battlenet:fenris".to_string(),
                title: "Diablo IV".to_string(),
                executable_path: Some(r"C:\Games\Diablo IV\Diablo IV.exe".to_string()),
                short_description: Some("Ritorno a Sanctuarium.".to_string()),
                genres: vec!["Azione".to_string(), "GDR".to_string()],
                is_favorite: Some(true),
            })
            .unwrap();

        // Simulate subsequent raw scan returning original unedited title/exe
        let raw_rescan = ScanReport {
            games: vec![DetectedGame::new_detected(
                "battlenet:fenris".to_string(),
                "Fenris".to_string(),
                "Fenris".to_string(),
                GamePlatform::BattleNet,
                Some("Fenris".to_string()),
                r"C:\Games\Diablo IV".to_string(),
                Some(r"C:\Games\Diablo IV\Diablo IV Launcher.exe".to_string()),
                None,
            )],
            ..Default::default()
        };

        let merged = store.merge_with_scan(raw_rescan);
        let g = &merged.games[0];
        assert_eq!(g.title, "Diablo IV");
        assert_eq!(
            g.executable_path.as_deref(),
            Some(r"C:\Games\Diablo IV\Diablo IV.exe")
        );
        assert_eq!(g.genres, vec!["Azione", "GDR"]);
        assert!(g.is_favorite);
    }

    fn scanned_steam(appid: &str, title: &str, provisional: bool) -> DetectedGame {
        let mut game = DetectedGame::new_detected(
            format!("steam:{appid}"),
            title.to_string(),
            title.to_string(),
            GamePlatform::Steam,
            Some(appid.to_string()),
            format!(r"C:\Steam\steamapps\common\{title}"),
            None,
            Some(format!("steam://rungameid/{appid}")),
        );
        game.title_provisional = provisional;
        game
    }

    fn rescan(store: &mut LibraryStore, game: DetectedGame) -> DetectedGame {
        let merged = store.merge_with_scan(ScanReport {
            games: vec![game],
            ..Default::default()
        });
        merged.games.into_iter().next().unwrap()
    }

    #[test]
    fn legacy_cached_placeholder_title_is_replaced_and_queued_for_resolution() {
        let tmp = tempdir().unwrap();
        let cover_file = tmp.path().join("steam_1321440.png");
        fs::write(&cover_file, b"pngbytes").unwrap();

        // library.json written before the fix: placeholder title, fully "synced"
        let mut legacy = scanned_steam("1321440", "appid_1321440", false);
        legacy.cover_path = Some(cover_file.to_string_lossy().to_string());
        legacy.genres = vec!["Indie".to_string(), "GDR".to_string()];
        legacy.metadata_synced = true;
        legacy.is_favorite = true;

        for custom_override in [false, true] {
            let mut cached = legacy.clone();
            cached.custom_override = custom_override;
            let mut store = LibraryStore {
                games: vec![cached],
            };

            let merged = rescan(&mut store, scanned_steam("1321440", "Cassette Beasts", true));

            assert_eq!(merged.title, "Cassette Beasts", "override={custom_override}");
            assert_eq!(merged.clean_title, "Cassette Beasts");
            assert!(
                merged.title_provisional,
                "must stay flagged so metadata sync fetches the official name"
            );
            // Everything else in the cache survives
            assert!(merged.cover_path.is_some());
            assert_eq!(merged.genres, vec!["Indie", "GDR"]);
            assert!(merged.is_favorite);
        }
    }

    #[test]
    fn resolved_official_title_survives_rescans_of_a_placeholder_manifest() {
        // Metadata sync already replaced the guess with the official store name
        let mut store = LibraryStore {
            games: vec![scanned_steam("1321440", "Cassette Beasts", false)],
        };

        // The manifest on disk still says `appid_1321440`, so the scanner guesses again
        let merged = rescan(&mut store, scanned_steam("1321440", "CassetteBeasts", true));

        assert_eq!(merged.title, "Cassette Beasts");
        assert!(!merged.title_provisional);

        // A manually edited title is confirmed by the user and never re-resolved
        store
            .apply_manual_update(GameManualUpdate {
                id: "steam:1321440".to_string(),
                title: "Cassette Beasts!".to_string(),
                executable_path: None,
                short_description: None,
                genres: vec![],
                is_favorite: None,
            })
            .unwrap();
        let merged = rescan(&mut store, scanned_steam("1321440", "CassetteBeasts", true));
        assert_eq!(merged.title, "Cassette Beasts!");
        assert!(!merged.title_provisional);
    }

    #[test]
    fn favorite_toggle_is_not_an_override_and_survives_concurrent_sync_write_back() {
        let mut store = LibraryStore {
            games: vec![scanned_steam("1091500", "Cyberpunk 2077", false)],
        };

        // Metadata sync snapshots the game, then spends a while on HTTP...
        let mut in_flight = store.games[0].clone();

        // ...during which the user stars the game
        let starred = store.set_favorite("steam:1091500", true).unwrap();
        assert!(starred.is_favorite);
        assert!(!starred.custom_override, "a favorite must not freeze automatic sync");

        // The stale snapshot is written back with fresh metadata
        in_flight.genres = vec!["GDR".to_string()];
        in_flight.metadata_synced = true;
        let committed = store.commit_enriched_game(in_flight, false).expect("committed");

        assert!(committed.is_favorite, "the star set mid-sync must be kept");
        assert_eq!(store.games[0].genres, vec!["GDR"]);
        assert!(store.games[0].is_favorite);

        assert!(store.set_favorite("steam:404", true).is_err());
    }

    #[test]
    fn sync_write_back_never_clobbers_a_manual_override_or_resurrects_removed_games() {
        let mut store = LibraryStore {
            games: vec![scanned_steam("1091500", "Cyberpunk 2077", false)],
        };
        let in_flight = store.games[0].clone();

        store
            .apply_manual_update(GameManualUpdate {
                id: "steam:1091500".to_string(),
                title: "CP2077".to_string(),
                executable_path: None,
                short_description: None,
                genres: vec!["Preferiti miei".to_string()],
                is_favorite: None,
            })
            .unwrap();

        assert!(store.commit_enriched_game(in_flight.clone(), false).is_none());
        assert_eq!(store.games[0].title, "CP2077");

        store.games.clear();
        assert!(store.commit_enriched_game(in_flight, false).is_none());
        assert!(store.games.is_empty());
    }

    #[test]
    fn hidden_and_play_history_survive_rescans_and_sync_write_back() {
        let mut store = LibraryStore {
            games: vec![scanned_steam("1091500", "Cyberpunk 2077", false)],
        };
        let in_flight = store.games[0].clone();

        // While a sync is in flight the user launches the game twice and hides it
        store.record_launch("steam:1091500", 1_000).unwrap();
        let played = store.record_launch("steam:1091500", 2_000).unwrap();
        assert_eq!(played.last_played_at, Some(2_000));
        assert_eq!(played.play_count, 2);
        let hidden = store.set_hidden("steam:1091500", true).unwrap();
        assert!(hidden.is_hidden && !hidden.custom_override);

        let committed = store.commit_enriched_game(in_flight, false).unwrap();
        assert!(committed.is_hidden);
        assert_eq!(committed.play_count, 2);
        assert_eq!(committed.last_played_at, Some(2_000));

        // A rescan finds the same manifest again: nothing the user did is lost
        let merged = rescan(&mut store, scanned_steam("1091500", "Cyberpunk 2077", false));
        assert!(merged.is_hidden);
        assert_eq!(merged.play_count, 2);
        assert_eq!(merged.last_played_at, Some(2_000));

        // Steam's own record of the game arrives with a scan: its playtime is taken, and the
        // later of the two "last played" times wins
        let mut from_steam = scanned_steam("1091500", "Cyberpunk 2077", false);
        from_steam.playtime_minutes = Some(754);
        from_steam.last_played_at = Some(1_500);
        let merged = rescan(&mut store, from_steam);
        assert_eq!(merged.playtime_minutes, Some(754));
        assert_eq!(merged.last_played_at, Some(2_000), "the launch from Kanso is more recent");

        let mut from_steam = scanned_steam("1091500", "Cyberpunk 2077", false);
        from_steam.playtime_minutes = Some(800);
        from_steam.last_played_at = Some(9_000);
        let merged = rescan(&mut store, from_steam);
        assert_eq!(merged.playtime_minutes, Some(800));
        assert_eq!(merged.last_played_at, Some(9_000), "played from Steam since then");

        // Tags found by a sync survive rescans; a scan without Steam's records (the file
        // could not be read this time) does not forget the playtime
        let mut synced = store.games[0].clone();
        synced.tags = vec!["Mondo aperto".to_string(), "Cyberpunk".to_string()];
        synced.tags_lang = Some("it".to_string());
        synced.playtime_minutes = None;
        let committed = store.commit_enriched_game(synced, false).unwrap();
        assert_eq!(committed.playtime_minutes, Some(800), "playtime belongs to the scan");
        let merged = rescan(&mut store, scanned_steam("1091500", "Cyberpunk 2077", false));
        assert_eq!(merged.tags, vec!["Mondo aperto", "Cyberpunk"]);
        assert_eq!(merged.tags_lang.as_deref(), Some("it"));
        assert_eq!(merged.playtime_minutes, Some(800));
        assert_eq!(merged.last_played_at, Some(9_000));

        // The status is the user's: scans and syncs carry it along. Achievement counts come
        // with a scan and are kept when a later scan cannot read them.
        store.set_status("steam:1091500", Some(GameStatus::Playing)).unwrap();
        let mut with_achievements = scanned_steam("1091500", "Cyberpunk 2077", false);
        with_achievements.achievements_unlocked = Some(12);
        with_achievements.achievements_total = Some(57);
        let merged = rescan(&mut store, with_achievements);
        assert_eq!(merged.status, Some(GameStatus::Playing));
        assert_eq!((merged.achievements_unlocked, merged.achievements_total), (Some(12), Some(57)));
        let mut stale_sync = store.games[0].clone();
        stale_sync.status = None;
        stale_sync.achievements_total = None;
        stale_sync.steam_store_appid = Some("1091500".to_string());
        let committed = store.commit_enriched_game(stale_sync, false).unwrap();
        assert_eq!(committed.status, Some(GameStatus::Playing));
        assert_eq!(committed.achievements_total, Some(57));
        let merged = rescan(&mut store, scanned_steam("1091500", "Cyberpunk 2077", false));
        assert_eq!((merged.achievements_unlocked, merged.achievements_total), (Some(12), Some(57)));
        assert_eq!(merged.steam_store_appid.as_deref(), Some("1091500"), "what a sync found is kept");
        assert_eq!(store.set_status("steam:1091500", None).unwrap().status, None);
        assert!(store.set_status("missing", None).is_err());

        // The user shows it again while another sync is in flight: stays visible...
        let in_flight = store.games[0].clone();
        store.set_hidden("steam:1091500", false).unwrap();
        assert!(!store.commit_enriched_game(in_flight.clone(), false).unwrap().is_hidden);
        // ...unless that very lookup just classified it as a non-game
        assert!(store.commit_enriched_game(in_flight, true).unwrap().is_hidden);
    }

    #[test]
    fn store_round_trips_through_the_database() {
        let mut db = LibraryDb::open_in_memory().unwrap();
        let mut store = LibraryStore {
            games: vec![
                scanned_steam("2", "Beta", false),
                scanned_steam("1", "alpha", true),
            ],
        };
        store.set_favorite("steam:2", true).unwrap();
        store.save_all(&mut db).unwrap();

        let reloaded = LibraryStore::load(&db).unwrap();
        let titles: Vec<&str> = reloaded.games.iter().map(|g| g.title.as_str()).collect();
        assert_eq!(titles, vec!["alpha", "Beta"]);
        assert!(reloaded.games[0].title_provisional);
        assert!(reloaded.games[1].is_favorite);

        // A store game a rescan no longer finds stays, marked as not installed
        let mut store = reloaded;
        let report = store.merge_with_scan(ScanReport {
            games: vec![scanned_steam("2", "Beta", false)],
            ..Default::default()
        });
        assert_eq!(report.games.len(), 2, "the interface is told about both");
        store.save_all(&mut db).unwrap();
        assert_eq!(db.game_count().unwrap(), 2);
        assert!(db.get_game("steam:2").unwrap().unwrap().is_favorite);
        assert!(db.get_game("steam:2").unwrap().unwrap().is_installed);
        assert!(!db.get_game("steam:1").unwrap().unwrap().is_installed);
    }

    #[test]
    fn the_user_s_own_genres_are_kept_apart_from_the_store_s() {
        let mut hades = scanned_steam("1145360", "Hades", false);
        hades.genres = vec!["Azione".to_string(), "GDR".to_string()];
        let mut store = LibraryStore { games: vec![hades] };

        let typed = ["  Da   rigiocare ", "azione", "DA RIGIOCARE", "", "Roguelike\u{7}", &"x".repeat(60)].map(str::to_string);
        let saved = store.set_custom_genres("steam:1145360", &typed).unwrap();
        assert_eq!(
            saved.custom_genres,
            ["Da rigiocare".to_string(), "Roguelike".to_string(), "x".repeat(40)],
            "tidied, each once whatever the capitals, none the store already gives"
        );
        assert_eq!(saved.genres, ["Azione", "GDR"], "the store's own are not touched");
        assert!(store.set_custom_genres("steam:missing", &[]).is_err());

        // A rescan, and a sync that fetched the store's genres again, leave them alone
        let rescanned = store.merge_with_scan(ScanReport { games: vec![scanned_steam("1145360", "Hades", false)], ..Default::default() });
        assert_eq!(rescanned.games[0].custom_genres.len(), 3);
        let mut synced = rescanned.games[0].clone();
        synced.genres = vec!["Action".to_string()];
        synced.custom_genres = Vec::new();
        synced.version = Some("stale".to_string());
        let committed = store.commit_enriched_game(synced, false).unwrap();
        assert_eq!((committed.genres.as_slice(), committed.custom_genres.len()), (&["Action".to_string()][..], 3));
        assert_eq!(committed.version, None, "the version is what the scan read, not what a sync carries");

        // They are the user's: a game with only that to its name is not forgotten when it goes
        let gone = store.merge_with_scan(ScanReport::default());
        assert_eq!(gone.games.len(), 1);
        // Clearing them is setting none
        assert!(store.set_custom_genres("steam:1145360", &[]).unwrap().custom_genres.is_empty());
    }

    #[test]
    fn an_emulated_game_the_scan_renames_gets_its_cover_looked_up_again() {
        let emulated = |title: &str| {
            DetectedGame::new_detected(
                "emu:custom-ps4:00000000000000aa".to_string(),
                title.to_string(),
                title.to_string(),
                GamePlatform::Emulator,
                Some("custom-ps4".to_string()),
                "/games/CUSA00900".to_string(),
                Some("/games/CUSA00900/eboot.bin".to_string()),
                None,
            )
        };
        let cover = std::env::temp_dir().join(format!("kanso-renamed-cover-{}.png", std::process::id()));
        fs::write(&cover, b"png").unwrap();
        // What a scan that only knew the file's name left behind: someone else's cover
        let mut wrong = emulated("eboot");
        wrong.cover_path = Some(cover.to_string_lossy().into_owned());
        wrong.steamgriddb_id = Some(42);
        wrong.short_description = Some("A football game.".to_string());
        wrong.genres = vec!["Sports".to_string()];
        wrong.metadata_synced = true;
        wrong.metadata_attempted_at = 1_700_000_000_000;
        wrong.is_favorite = true;
        wrong.playtime_minutes = Some(90);
        let mut store = LibraryStore { games: vec![wrong.clone()] };

        // The same name again: everything stays
        let same = store.merge_with_scan(ScanReport { games: vec![emulated("eboot")], ..Default::default() });
        assert_eq!(same.games[0].cover_path, wrong.cover_path);
        assert!(same.games[0].metadata_synced);

        // The scan now reads the game's own title: cover and description go, the rest stays
        let renamed = store.merge_with_scan(ScanReport { games: vec![emulated("Bloodborne")], ..Default::default() });
        let game = &renamed.games[0];
        assert_eq!(game.title, "Bloodborne");
        assert_eq!((game.cover_path.as_deref(), game.steamgriddb_id, game.short_description.as_deref()), (None, None, None));
        assert!(game.genres.is_empty() && !game.metadata_synced && game.metadata_attempted_at == 0);
        assert!(game.is_favorite, "what the user did with the game is still theirs");
        assert_eq!(game.playtime_minutes, Some(90));

        // A game the user renamed and gave a cover to is left as they made it
        let mut edited = wrong.clone();
        edited.title = "My Bloodborne".to_string();
        edited.custom_override = true;
        let mut store = LibraryStore { games: vec![edited] };
        let kept = store.merge_with_scan(ScanReport { games: vec![emulated("Bloodborne")], ..Default::default() });
        assert_eq!(kept.games[0].title, "My Bloodborne");
        assert_eq!(kept.games[0].cover_path, wrong.cover_path);
        let _ = fs::remove_file(&cover);
    }

    #[test]
    fn uninstalled_games_keep_their_place_and_come_back_when_installed_again() {
        let standalone = |id: &str, title: &str| {
            DetectedGame::new_detected(
                id.to_string(),
                title.to_string(),
                title.to_string(),
                GamePlatform::Standalone,
                None,
                format!("/games/{title}"),
                Some(format!("/games/{title}/game.exe")),
                None,
            )
        };
        let mut finished = scanned_steam("620", "Portal 2", false);
        finished.status = Some(GameStatus::Finished);
        finished.playtime_minutes = Some(540);
        finished.cover_path = Some("/covers/steam_620.png".to_string());
        let mut loved = standalone("standalone:loved", "Loved Indie");
        loved.is_favorite = true;
        let mut store = LibraryStore {
            games: vec![finished, loved, standalone("standalone:loose", "Loose Exe")],
        };

        // Everything is uninstalled (or its folder is no longer scanned)
        let report = store.merge_with_scan(ScanReport::default());
        let ids: Vec<&str> = report.games.iter().map(|g| g.id.as_str()).collect();
        assert_eq!(
            ids,
            ["standalone:loved", "steam:620"],
            "a loose executable with nothing attached to it is simply gone"
        );
        assert!(report.games.iter().all(|g| !g.is_installed));
        let portal = &store.games[1];
        assert_eq!(portal.status, Some(GameStatus::Finished));
        assert_eq!(portal.playtime_minutes, Some(540));
        assert_eq!(portal.install_uri().as_deref(), Some("steam://install/620"));
        assert_eq!(store.games[0].install_uri(), None, "nothing can install a local game");

        // A later scan that still does not find them changes nothing
        let again = store.merge_with_scan(ScanReport::default());
        assert_eq!(again.games.len(), 2);

        // Installed again: same entry, on disk, with everything it had
        let report = store.merge_with_scan(ScanReport {
            games: vec![scanned_steam("620", "Portal 2", false)],
            ..Default::default()
        });
        let portal = report.games.iter().find(|g| g.id == "steam:620").unwrap();
        assert!(portal.is_installed);
        assert_eq!(portal.status, Some(GameStatus::Finished));
        assert_eq!(portal.playtime_minutes, Some(540));
    }

    #[test]
    fn uninstalling_is_asked_of_the_game_s_client_or_done_there() {
        let game = |platform: GamePlatform, app_id: Option<&str>| {
            DetectedGame::new_detected(
                "x:1".to_string(),
                "X".to_string(),
                "X".to_string(),
                platform,
                app_id.map(str::to_string),
                "/games/x".to_string(),
                Some("/games/x/x.exe".to_string()),
                None,
            )
        };
        let uri = |platform: GamePlatform, app_id: Option<&str>| game(platform, app_id).uninstall_uri();

        // The two clients that take the request
        assert_eq!(uri(GamePlatform::Steam, Some("620")).as_deref(), Some("steam://uninstall/620"));
        assert_eq!(uri(GamePlatform::Ubisoft, Some("635")).as_deref(), Some("uplay://uninstall/635"));
        // The others: the game or the library in the client, or the apps of Windows
        assert_eq!(uri(GamePlatform::Gog, Some("1207664643")).as_deref(), Some("goggalaxy://openGameView/1207664643"));
        assert_eq!(uri(GamePlatform::Epic, Some("Fortnite")).as_deref(), Some("com.epicgames.launcher://store/library"));
        assert_eq!(uri(GamePlatform::Ea, None).as_deref(), Some("origin2://library/open"));
        assert_eq!(uri(GamePlatform::BattleNet, Some("Diablo IV")).as_deref(), Some("battlenet://"));
        assert_eq!(uri(GamePlatform::Xbox, Some("Microsoft.624F8B84B80")).as_deref(), Some("ms-settings:appsfeatures"));
        // Nothing is built from an id that is not what the client knows the game by
        assert_eq!(uri(GamePlatform::Steam, Some("620; rm -rf")), None);
        assert_eq!(uri(GamePlatform::Steam, None), None);
        assert_eq!(uri(GamePlatform::Gog, Some("../x")), None);
        // A loose program or a game file has no client: Kanso deletes nothing itself
        assert_eq!(uri(GamePlatform::Standalone, None), None);
        assert_eq!(uri(GamePlatform::Emulator, Some("dolphin")), None);
        // What is not installed cannot be uninstalled
        let mut absent = game(GamePlatform::Steam, Some("620"));
        absent.is_installed = false;
        assert_eq!(absent.uninstall_uri(), None);
    }

    #[test]
    fn install_addresses_exist_only_for_stores_that_can_be_asked() {
        let game = |platform: GamePlatform, app_id: Option<&str>, launch_uri: Option<&str>| {
            DetectedGame::new_detected(
                "x:1".to_string(),
                "X".to_string(),
                "X".to_string(),
                platform,
                app_id.map(ToString::to_string),
                String::new(),
                None,
                launch_uri.map(ToString::to_string),
            )
            .install_uri()
        };
        assert_eq!(game(GamePlatform::Steam, Some("620"), None).as_deref(), Some("steam://install/620"));
        assert_eq!(game(GamePlatform::Steam, Some("620 & calc"), None), None, "only a plain appid");
        assert_eq!(
            game(
                GamePlatform::Epic,
                Some("Fortnite"),
                Some("com.epicgames.launcher://apps/ns%3Aitem%3AFortnite?action=launch&silent=true")
            )
            .as_deref(),
            Some("com.epicgames.launcher://apps/ns%3Aitem%3AFortnite?action=install")
        );
        assert_eq!(game(GamePlatform::Epic, Some("Fortnite"), Some("https://example.org/x?action=launch")), None);
        assert_eq!(
            game(GamePlatform::Gog, Some("1207658924"), None).as_deref(),
            Some("goggalaxy://openGameView/1207658924")
        );
        assert_eq!(
            game(GamePlatform::Ubisoft, Some("4553"), Some("uplay://launch/4553/0")).as_deref(),
            Some("uplay://launch/4553/0")
        );
        assert_eq!(game(GamePlatform::Xbox, Some("Microsoft.Game"), None), None);
        assert_eq!(game(GamePlatform::Standalone, None, None), None);
    }
}
