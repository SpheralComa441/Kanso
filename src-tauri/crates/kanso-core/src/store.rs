use crate::cleaner::{clean_game_title, is_placeholder_title};
use crate::db::{sort_games_by_title, LibraryDb};
use crate::metadata::{remove_existing_cover_variants, sanitize_filename_id};
use crate::models::{now_timestamp_ms, DetectedGame, GameManualUpdate, ScanReport};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// In-memory view of the library with the rules for merging scans, manual edits and
/// automatic metadata. Persistence lives in [`LibraryDb`].
#[derive(Debug, Clone, Default)]
pub struct LibraryStore {
    pub games: Vec<DetectedGame>,
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
                scanned.last_played_at = cached.last_played_at;
                scanned.play_count = cached.play_count;
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
            }
        }

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
        enriched.play_count = existing.play_count;
        enriched.is_hidden = existing.is_hidden || auto_hide;
        self.upsert_game(enriched.clone());
        Some(enriched)
    }

    /// Marks or unmarks a game as favorite. Unlike [`Self::apply_manual_update`] this is not a
    /// metadata override: automatic sync keeps managing the game.
    pub fn set_favorite(&mut self, game_id: &str, is_favorite: bool) -> Result<DetectedGame, String> {
        let Some(game) = self.get_game_mut(game_id) else {
            return Err(format!("Gioco '{game_id}' non trovato nella libreria."));
        };
        game.is_favorite = is_favorite;
        Ok(game.clone())
    }

    /// Hides a game from the library (or shows it again). Like a favorite, not an override.
    pub fn set_hidden(&mut self, game_id: &str, is_hidden: bool) -> Result<DetectedGame, String> {
        let Some(game) = self.get_game_mut(game_id) else {
            return Err(format!("Gioco '{game_id}' non trovato nella libreria."));
        };
        game.is_hidden = is_hidden;
        Ok(game.clone())
    }

    /// Records a successful launch for the "recently played" view.
    pub fn record_launch(&mut self, game_id: &str, now_ms: u64) -> Result<DetectedGame, String> {
        let Some(game) = self.get_game_mut(game_id) else {
            return Err(format!("Gioco '{game_id}' non trovato nella libreria."));
        };
        game.last_played_at = Some(now_ms);
        game.play_count = game.play_count.saturating_add(1);
        Ok(game.clone())
    }

    /// Applies manual edits (title, executable path, genres, description, favorite) to a single game.
    pub fn apply_manual_update(&mut self, payload: GameManualUpdate) -> Result<DetectedGame, String> {
        let Some(game) = self.get_game_mut(&payload.id) else {
            return Err(format!("Gioco '{}' non trovato nella libreria.", payload.id));
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
                "File immagine non trovato: {}",
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
            format!(
                "Impossibile creare la cartella copertine {}: {e}",
                covers_dir.display()
            )
        })?;

        let safe_stem = sanitize_filename_id(game_id);
        remove_existing_cover_variants(covers_dir, &safe_stem);
        let dest = covers_dir.join(format!("{safe_stem}.{ext}"));

        fs::copy(source_image_path, &dest).map_err(|e| {
            format!(
                "Impossibile copiare l'immagine di copertina in {}: {e}",
                dest.display()
            )
        })?;

        let Some(game) = self.get_game_mut(game_id) else {
            return Err(format!("Gioco '{game_id}' non trovato nella libreria."));
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

        // A rescan that no longer finds a game removes it from the database too
        let mut store = reloaded;
        store.merge_with_scan(ScanReport {
            games: vec![scanned_steam("2", "Beta", false)],
            ..Default::default()
        });
        store.save_all(&mut db).unwrap();
        assert_eq!(db.game_count().unwrap(), 1);
        assert!(db.get_game("steam:2").unwrap().unwrap().is_favorite);
    }
}
