use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::Path;

/// User configuration persisted locally inside the OS AppData directory (`config.json`).
/// API keys are never stored in repository `.env` files.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AppConfig {
    /// Optional SteamGridDB API key configured by the user in Settings.
    pub steamgriddb_api_key: String,
    /// User-added directories scanned for standalone `.exe` games.
    pub custom_folders: Vec<String>,
    /// Whether Kanso should minimize to the system tray when launching a game.
    pub minimize_to_tray_on_launch: bool,
}

impl AppConfig {
    /// Returns `true` when a non-empty SteamGridDB API key has been configured.
    pub fn has_api_key(&self) -> bool {
        !self.steamgriddb_api_key.trim().is_empty()
    }

    /// Normalizes fields by trimming whitespace and deduplicating custom folder paths.
    pub fn sanitized(self) -> Self {
        let mut seen = HashSet::new();
        let mut folders = Vec::new();
        for folder in self.custom_folders {
            let trimmed = folder.trim().to_string();
            if trimmed.is_empty() {
                continue;
            }
            let dedup_key = normalize_folder_key(&trimmed);
            if seen.insert(dedup_key) {
                folders.push(trimmed);
            }
        }

        Self {
            steamgriddb_api_key: self.steamgriddb_api_key.trim().to_string(),
            custom_folders: folders,
            minimize_to_tray_on_launch: self.minimize_to_tray_on_launch,
        }
    }

    /// Loads `AppConfig` from disk, returning `AppConfig::default()` if the file does not exist yet.
    pub fn load_from_file(path: &Path) -> Result<Self, String> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let raw = fs::read_to_string(path)
            .map_err(|e| format!("Impossibile leggere il file di configurazione {}: {e}", path.display()))?;
        let parsed: Self = serde_json::from_str(&raw)
            .map_err(|e| format!("Formato config.json non valido in {}: {e}", path.display()))?;
        Ok(parsed.sanitized())
    }

    /// Saves the sanitized `AppConfig` to disk, creating parent directories if needed.
    pub fn save_to_file(&self, path: &Path) -> Result<Self, String> {
        let clean = self.clone().sanitized();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                format!(
                    "Impossibile creare la cartella di configurazione {}: {e}",
                    parent.display()
                )
            })?;
        }
        let serialized = serde_json::to_string_pretty(&clean)
            .map_err(|e| format!("Errore di serializzazione config.json: {e}"))?;
        fs::write(path, serialized)
            .map_err(|e| format!("Impossibile salvare config.json in {}: {e}", path.display()))?;
        Ok(clean)
    }
}

fn normalize_folder_key(path: &str) -> String {
    path.replace('\\', "/")
        .trim_end_matches('/')
        .to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn saves_and_loads_sanitized_config() {
        let dir = tempdir().expect("tempdir");
        let cfg_path = dir.path().join("appdata").join("config.json");

        // Missing file returns default config without error
        let initial = AppConfig::load_from_file(&cfg_path).expect("default config");
        assert!(!initial.has_api_key());
        assert!(initial.custom_folders.is_empty());

        let input = AppConfig {
            steamgriddb_api_key: "  sgdb_secret_key_123  ".to_string(),
            custom_folders: vec![
                r"D:\Games\".to_string(),
                r"d:/games".to_string(),
                "  ".to_string(),
                r"E:\IndieGames".to_string(),
            ],
            minimize_to_tray_on_launch: true,
        };

        let saved = input.save_to_file(&cfg_path).expect("save config");
        assert_eq!(saved.steamgriddb_api_key, "sgdb_secret_key_123");
        assert_eq!(saved.custom_folders, vec![r"D:\Games\", r"E:\IndieGames"]);
        assert!(saved.has_api_key());

        let reloaded = AppConfig::load_from_file(&cfg_path).expect("reload config");
        assert_eq!(reloaded, saved);
    }
}
