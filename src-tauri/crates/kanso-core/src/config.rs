use crate::i18n::{Lang, T};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
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

/// What the user may choose for the animations; the first is the default.
const ANIMATION_CHOICES: [&str; 5] = ["theme", "off", "subtle", "smooth", "lively"];

/// What the cover of a game can tell under the pointer, besides its title, in the order it
/// is shown. Anything else in `config.json` is dropped.
pub const CARD_INFO_CHOICES: [&str; 7] =
    ["genres", "tags", "playtime", "lastPlayed", "status", "achievements", "badges"];
/// What it tells until the user chooses.
const CARD_INFO_DEFAULT: [&str; 3] = ["genres", "playtime", "badges"];

/// Most plugins whose state is remembered.
const MAX_PLUGIN_STATES: usize = 64;
/// Longest value of a plugin setting (an API key, a country code, ...).
const MAX_PLUGIN_SETTING_CHARS: usize = 2000;

/// What the user decided about one plugin.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PluginState {
    pub enabled: bool,
    /// Whether the plugin's short label is shown on the game cards.
    pub show_badge: bool,
    /// Values of the settings the plugin declares (API keys included: like the SteamGridDB
    /// key they never leave this PC except towards the plugin's own service).
    pub settings: BTreeMap<String, String>,
}

impl Default for PluginState {
    fn default() -> Self {
        Self {
            enabled: false,
            show_badge: true,
            settings: BTreeMap::new(),
        }
    }
}

/// `true` for a well-formed plugin id: 2–40 lower-case letters, digits and dashes, starting
/// with a letter or digit. Ids double as folder names, so nothing else is accepted.
pub fn is_valid_plugin_id(id: &str) -> bool {
    (2..=40).contains(&id.len())
        && id.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        && !id.starts_with('-')
}

/// User configuration persisted locally inside the OS AppData directory (`config.json`).
/// API keys are never stored in repository `.env` files.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
    /// Whether Kanso measures play time for the games whose store does not expose it.
    pub track_playtime: bool,
    /// Per-plugin choices, by plugin id. A plugin without an entry uses its own default.
    pub plugins: BTreeMap<String, PluginState>,
    /// How much the interface moves: `theme` (as the active theme says), `off`, `subtle`,
    /// `smooth` or `lively`.
    pub animations: String,
    /// What a cover tells about its game under the pointer, from [`CARD_INFO_CHOICES`]. An
    /// empty list is a choice too: the title alone.
    pub card_info: Vec<String>,
    /// The user says they have Xbox Game Pass for PC, linked to their Battle.net account:
    /// the games it makes installable there are listed (see
    /// [`crate::battlenet_account::game_pass_games`]). Kanso cannot find this out by itself.
    pub has_game_pass: bool,
    /// The user asked for the games of the Game Pass catalog for PC to be listed as not
    /// installed (see [`crate::gamepass`]): a few hundred games, from Microsoft's public
    /// catalog.
    pub game_pass_catalog: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            steamgriddb_api_key: String::new(),
            custom_folders: Vec::new(),
            minimize_to_tray_on_launch: false,
            language: None,
            theme_id: None,
            accessibility: AccessibilityConfig::default(),
            track_playtime: true,
            plugins: BTreeMap::new(),
            animations: ANIMATION_CHOICES[0].to_string(),
            card_info: CARD_INFO_DEFAULT.iter().map(|item| item.to_string()).collect(),
            has_game_pass: false,
            game_pass_catalog: false,
        }
    }
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
            track_playtime: self.track_playtime,
            // Anything else is not a choice: back to what the theme says
            animations: Some(self.animations.trim().to_ascii_lowercase())
                .filter(|choice| ANIMATION_CHOICES.contains(&choice.as_str()))
                .unwrap_or_else(|| ANIMATION_CHOICES[0].to_string()),
            // Known names only, once each, in the order Kanso shows them
            card_info: CARD_INFO_CHOICES
                .iter()
                .filter(|choice| self.card_info.iter().any(|item| item.trim() == **choice))
                .map(|choice| choice.to_string())
                .collect(),
            has_game_pass: self.has_game_pass,
            game_pass_catalog: self.game_pass_catalog,
            plugins: self
                .plugins
                .into_iter()
                .filter(|(id, _)| is_valid_plugin_id(id))
                .take(MAX_PLUGIN_STATES)
                .map(|(id, state)| {
                    let settings = state
                        .settings
                        .into_iter()
                        .filter(|(key, _)| !key.is_empty() && key.len() <= 40)
                        .map(|(key, value)| {
                            let value: String =
                                value.trim().chars().take(MAX_PLUGIN_SETTING_CHARS).collect();
                            (key, value)
                        })
                        .collect();
                    (id, PluginState { settings, ..state })
                })
                .collect(),
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
            track_playtime: false,
            has_game_pass: true,
            game_pass_catalog: true,
            animations: " Lively ".to_string(),
            card_info: vec![
                "badges".to_string(),
                " genres ".to_string(),
                "badges".to_string(),
                "<b>".to_string(),
            ],
            plugins: BTreeMap::from([
                (
                    "price-history".to_string(),
                    PluginState {
                        enabled: true,
                        show_badge: false,
                        settings: BTreeMap::from([
                            ("apiKey".to_string(), "  itad-key  ".to_string()),
                            (String::new(), "dropped".to_string()),
                        ]),
                    },
                ),
                ("../escape".to_string(), PluginState::default()),
            ]),
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

        assert!(!saved.track_playtime);
        assert_eq!(saved.card_info, ["genres", "badges"], "known names, once, in Kanso's order");
        assert_eq!(saved.plugins.len(), 1, "ids that are not plain names are dropped");
        let plugin = &saved.plugins["price-history"];
        assert!(plugin.enabled && !plugin.show_badge);
        assert_eq!(
            plugin.settings,
            BTreeMap::from([("apiKey".to_string(), "itad-key".to_string())])
        );

        let reloaded = AppConfig::load_from_file(&cfg_path).expect("reload config");
        assert_eq!(reloaded, saved);

        // A config written before these settings existed, and an unknown font name
        fs::write(&cfg_path, r#"{"steamgriddbApiKey":"k","language":"it"}"#).unwrap();
        let old = AppConfig::load_from_file(&cfg_path).expect("old config");
        assert_eq!(old.accessibility, AccessibilityConfig::default());
        assert!(old.track_playtime, "measuring play time is on unless switched off");
        assert!(old.plugins.is_empty());
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
    #[test]
    fn the_cover_tells_the_usual_three_things_until_the_user_chooses() {
        // A config.json written before the setting existed
        let old: AppConfig = serde_json::from_str("{}").expect("old config");
        assert_eq!(old.sanitized().card_info, ["genres", "playtime", "badges"]);
        // Nothing at all is a choice, not a missing value
        let bare: AppConfig = serde_json::from_str(r#"{"cardInfo": []}"#).expect("bare config");
        assert!(bare.sanitized().card_info.is_empty());
        let all: AppConfig = serde_json::from_str(
            r#"{"cardInfo": ["badges", "achievements", "status", "lastPlayed", "playtime", "tags", "genres", "url(x)"]}"#,
        )
        .expect("full config");
        assert_eq!(all.sanitized().card_info, CARD_INFO_CHOICES);
    }

}
