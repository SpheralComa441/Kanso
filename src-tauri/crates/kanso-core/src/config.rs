use crate::i18n::{Lang, T};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::Path;

/// Fonts chosen for legibility that can replace a theme's fonts.
const READABLE_FONTS: [&str; 2] = ["atkinson", "opendyslexic"];

/// Preferences for people who need larger text, stronger contrast or less motion. They apply
/// on top of whatever theme is selected.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AccessibilityConfig {
    /// Size of text and controls, in percent of the theme's own size.
    pub text_scale: u32,
    /// Stronger text and border colors.
    pub high_contrast: bool,
    /// No transitions, zooms or other animations.
    pub reduce_motion: bool,
    /// `atkinson` (low vision) or `opendyslexic` instead of the theme's fonts.
    pub readable_font: Option<String>,
    /// Scrollbars always visible instead of only under the pointer.
    pub always_show_scrollbars: bool,
    /// A thick outline around the element that has keyboard focus.
    pub strong_focus: bool,
}

impl Default for AccessibilityConfig {
    fn default() -> Self {
        Self {
            text_scale: 100,
            high_contrast: false,
            reduce_motion: false,
            readable_font: None,
            always_show_scrollbars: false,
            strong_focus: false,
        }
    }
}

impl AccessibilityConfig {
    fn sanitized(self) -> Self {
        Self {
            text_scale: self.text_scale.clamp(80, 160),
            readable_font: self
                .readable_font
                .map(|font| font.trim().to_ascii_lowercase())
                .filter(|font| READABLE_FONTS.contains(&font.as_str())),
            ..self
        }
    }
}

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
    /// Interface language (`it`, `en`, `es`, `ru`). `None` until the user or the first-run
    /// detection picks one.
    pub language: Option<String>,
    /// Id of the selected theme; `None` means the built-in default.
    pub theme_id: Option<String>,
    pub accessibility: AccessibilityConfig,
}

impl AppConfig {
    /// Returns `true` when a non-empty SteamGridDB API key has been configured.
    pub fn has_api_key(&self) -> bool {
        !self.steamgriddb_api_key.trim().is_empty()
    }

    /// The configured interface language, if it is one Kanso supports.
    pub fn lang(&self) -> Option<Lang> {
        self.language.as_deref().and_then(Lang::from_code)
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
            // Unsupported codes are dropped rather than stored
            language: self
                .language
                .as_deref()
                .and_then(Lang::from_code)
                .map(|lang| lang.code().to_string()),
            theme_id: self
                .theme_id
                .map(|id| id.trim().to_string())
                .filter(|id| !id.is_empty()),
            accessibility: self.accessibility.sanitized(),
        }
    }

    /// Loads `AppConfig` from disk, returning `AppConfig::default()` if the file does not exist yet.
    pub fn load_from_file(path: &Path) -> Result<Self, String> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let raw = fs::read_to_string(path)
            .map_err(|e| format!("{} {}: {e}", T::CannotRead.s(), path.display()))?;
        let parsed: Self = serde_json::from_str(&raw)
            .map_err(|e| format!("{} {}: {e}", T::InvalidFormat.s(), path.display()))?;
        Ok(parsed.sanitized())
    }

    /// Saves the sanitized `AppConfig` to disk, creating parent directories if needed.
    pub fn save_to_file(&self, path: &Path) -> Result<Self, String> {
        let clean = self.clone().sanitized();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                format!("{} {}: {e}", T::CannotCreateFolder.s(), parent.display())
            })?;
        }
        let serialized = serde_json::to_string_pretty(&clean)
            .map_err(|e| format!("{} config.json: {e}", T::CannotSave.s()))?;
        // Write to a sibling file and rename over the target: a crash mid-write can no longer
        // leave a truncated config.json (and with it a lost API key) behind
        let tmp_path = path.with_extension("json.tmp");
        fs::write(&tmp_path, serialized)
            .and_then(|()| fs::rename(&tmp_path, path))
            .map_err(|e| format!("{} {}: {e}", T::CannotSave.s(), path.display()))?;
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
            language: Some("EN-us".to_string()),
            theme_id: Some("  nord ".to_string()),
            accessibility: AccessibilityConfig {
                text_scale: 400,
                high_contrast: true,
                readable_font: Some(" OpenDyslexic ".to_string()),
                ..AccessibilityConfig::default()
            },
        };

        let saved = input.save_to_file(&cfg_path).expect("save config");
        assert_eq!(saved.steamgriddb_api_key, "sgdb_secret_key_123");
        assert_eq!(saved.custom_folders, vec![r"D:\Games\", r"E:\IndieGames"]);
        assert!(saved.has_api_key());
        assert_eq!(saved.language.as_deref(), Some("en"));
        assert_eq!(saved.lang(), Some(Lang::En));
        assert_eq!(saved.theme_id.as_deref(), Some("nord"));
        assert_eq!(saved.accessibility.text_scale, 160, "clamped");
        assert!(saved.accessibility.high_contrast && !saved.accessibility.reduce_motion);
        assert_eq!(saved.accessibility.readable_font.as_deref(), Some("opendyslexic"));

        let reloaded = AppConfig::load_from_file(&cfg_path).expect("reload config");
        assert_eq!(reloaded, saved);

        // A config written before these settings existed, and an unknown font name
        fs::write(&cfg_path, r#"{"steamgriddbApiKey":"k","language":"it"}"#).unwrap();
        let old = AppConfig::load_from_file(&cfg_path).expect("old config");
        assert_eq!(old.accessibility, AccessibilityConfig::default());
        let odd = AppConfig {
            accessibility: AccessibilityConfig {
                text_scale: 10,
                readable_font: Some("comic sans".to_string()),
                ..AccessibilityConfig::default()
            },
            ..old
        }
        .sanitized();
        assert_eq!(odd.accessibility.text_scale, 80);
        assert_eq!(odd.accessibility.readable_font, None);
    }
}
