use crate::cleaner::clean_game_title;
use crate::metadata::{remove_existing_cover_variants, sanitize_filename_id};
use crate::models::{
    now_timestamp_ms, DetectedGame, GameManualUpdate, GamePlatform, ScanReport,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// Persistent local cache of discovered games, downloaded covers, metadata, and manual edits.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct LibraryStore {
    pub games: Vec<DetectedGame>,
}

impl LibraryStore {
    pub fn load_from_file(path: &Path) -> Self {
        let Ok(raw) = fs::read_to_string(path) else {
            return Self::default();
        };
        serde_json::from_str::<Self>(&raw).unwrap_or_default()
    }

    pub fn save_to_file(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                format!(
                    "Impossibile creare la cartella dati {}: {e}",
                    parent.display()
                )
            })?;
        }
        let content = serde_json::to_string_pretty(self)
            .map_err(|e| format!("Errore serializzazione libreria: {e}"))?;
        fs::write(path, content)
            .map_err(|e| format!("Impossibile salvare la cache libreria {}: {e}", path.display()))
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
                scanned.is_favorite = cached.is_favorite;
                scanned.metadata_synced = cached.metadata_synced && scanned.cover_path.is_some();
                scanned.custom_override = cached.custom_override;
                scanned.updated_at = cached.updated_at;

                if cached.custom_override {
                    if !cached.title.trim().is_empty() {
                        scanned.title = cached.title.clone();
                        scanned.clean_title = cached.clean_title.clone();
                    }
                    if cached.executable_path.is_some() {
                        scanned.executable_path = cached.executable_path.clone();
                    }
                }
            }
        }

        report.games.sort_by(|a, b| {
            a.title
                .to_lowercase()
                .cmp(&b.title.to_lowercase())
                .then_with(|| a.id.cmp(&b.id))
        });

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
        self.games.sort_by(|a, b| {
            a.title
                .to_lowercase()
                .cmp(&b.title.to_lowercase())
                .then_with(|| a.id.cmp(&b.id))
        });
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
        self.games.sort_by(|a, b| {
            a.title
                .to_lowercase()
                .cmp(&b.title.to_lowercase())
                .then_with(|| a.id.cmp(&b.id))
        });
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
        let steam_count = self
            .games
            .iter()
            .filter(|g| g.platform == GamePlatform::Steam)
            .count();
        let epic_count = self
            .games
            .iter()
            .filter(|g| g.platform == GamePlatform::Epic)
            .count();
        let battlenet_count = self
            .games
            .iter()
            .filter(|g| g.platform == GamePlatform::BattleNet)
            .count();
        let standalone_count = self
            .games
            .iter()
            .filter(|g| g.platform == GamePlatform::Standalone)
            .count();

        ScanReport {
            games: self.games.clone(),
            steam_count,
            epic_count,
            battlenet_count,
            standalone_count,
            warnings: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
            battlenet_count: 1,
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
}
