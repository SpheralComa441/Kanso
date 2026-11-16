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

/// The parts of the side panel the user can leave out: the Home page, the Store page, the
/// list of tags. Anything else in `config.json` is dropped.
pub const HIDEABLE_SECTIONS: [&str; 3] = ["home", "store", "tags"];

/// The parts of the Home page the user can leave out.
pub const HOME_SECTIONS: [&str; 5] = ["resume", "upNext", "mostPlayed", "recommendations", "deals"];
/// Most games lined up to play next.
pub const MAX_UP_NEXT: usize = 5;

/// A place the user named for the weather on the Home page. Kept coarse on purpose: the
/// coordinates are rounded to two decimals (about a kilometre), which is all a forecast
/// needs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WeatherPlace {
    pub name: String,
    pub latitude: f64,
    pub longitude: f64,
}

// Coordinates that are not numbers never get past `sanitized`: equality holds
impl Eq for WeatherPlace {}

/// How the user wants the Home page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct HomeConfig {
    /// Parts left out, from [`HOME_SECTIONS`].
    pub hidden: Vec<String>,
    /// The moving background of the season and of the holidays.
    pub effects: bool,
    /// Let the background follow the weather at `place`. Off until the user turns it on and
    /// names a place: it is the one thing on the Home page that asks a site (Open-Meteo)
    /// about somewhere.
    pub weather: bool,
    pub place: Option<WeatherPlace>,
    /// The games lined up to play next, in order: ids of library games, at most
    /// [`MAX_UP_NEXT`].
    pub up_next: Vec<String>,
}

impl Default for HomeConfig {
    fn default() -> Self {
        Self { hidden: Vec::new(), effects: true, weather: false, place: None, up_next: Vec::new() }
    }
}

impl HomeConfig {
    fn sanitized(self) -> Self {
        let place = self.place.and_then(|place| {
            let name: String = place.name.trim().chars().filter(|c| !c.is_control()).take(80).collect();
            let fits = place.latitude.is_finite()
                && place.longitude.is_finite()
                && (-90.0..=90.0).contains(&place.latitude)
                && (-180.0..=180.0).contains(&place.longitude);
            (!name.is_empty() && fits).then(|| WeatherPlace {
                name,
                latitude: (place.latitude * 100.0).round() / 100.0,
                longitude: (place.longitude * 100.0).round() / 100.0,
            })
        });
        let mut seen = HashSet::new();
        Self {
            hidden: HOME_SECTIONS
                .iter()
                .filter(|choice| self.hidden.iter().any(|item| item.trim() == **choice))
                .map(|choice| choice.to_string())
                .collect(),
            effects: self.effects,
            // Without a place there is no weather to ask about
            weather: self.weather && place.is_some(),
            place,
            up_next: self
                .up_next
                .iter()
                .map(|id| id.trim().to_string())
                .filter(|id| !id.is_empty() && id.len() <= 200 && !id.chars().any(char::is_control) && seen.insert(id.clone()))
                .take(MAX_UP_NEXT)
                .collect(),
        }
    }
}

/// Most emulators remembered, folders per emulator and extensions per emulator.
const MAX_EMULATORS: usize = 48;
const MAX_EMULATOR_FOLDERS: usize = 16;
const MAX_EMULATOR_EXTENSIONS: usize = 32;
/// Longest command line for starting a game in an emulator.
const MAX_EMULATOR_ARGS_CHARS: usize = 400;

/// An emulator the user told Kanso about, or what they added to one Kanso found by itself
/// (see [`crate::emulators`]).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct EmulatorSetting {
    /// Id of an emulator Kanso knows (`dolphin`), or `custom-…` for one it does not.
    pub id: String,
    /// Name of an emulator Kanso does not know. A known one keeps its own.
    pub name: String,
    /// Its executable. Empty for a known emulator that Kanso found by itself.
    pub exe_path: String,
    /// Folders with its games, besides those the emulator's own settings list.
    pub folders: Vec<String>,
    /// File extensions of its games, without the dot. A known emulator has its own list.
    pub extensions: Vec<String>,
    /// Arguments that start a game, `{rom}` standing for the game file. Empty: the usual
    /// ones of that emulator.
    pub args: String,
}

impl EmulatorSetting {
    fn sanitized(self) -> Option<Self> {
        let id = self.id.trim().to_ascii_lowercase();
        if !is_valid_plugin_id(&id) {
            return None;
        }
        let plain = |text: &str, max: usize| -> String {
            text.trim().chars().filter(|c| !c.is_control()).take(max).collect()
        };
        let mut seen = HashSet::new();
        let folders: Vec<String> = self
            .folders
            .iter()
            .map(|folder| folder.trim().to_string())
            .filter(|folder| !folder.is_empty() && seen.insert(normalize_folder_key(folder)))
            .take(MAX_EMULATOR_FOLDERS)
            .collect();
        let mut seen = HashSet::new();
        let extensions: Vec<String> = self
            .extensions
            .iter()
            .map(|extension| extension.trim().trim_start_matches('.').to_ascii_lowercase())
            .filter(|extension| {
                (1..=10).contains(&extension.len())
                    && extension.bytes().all(|b| b.is_ascii_alphanumeric())
                    && seen.insert(extension.clone())
            })
            .take(MAX_EMULATOR_EXTENSIONS)
            .collect();
        let exe_path = plain(&self.exe_path, 1000);
        // An emulator added as one Kanso did not know, which it has learnt since: it becomes
        // the known one, with that emulator's own name, files and way of finding games
        let learnt = Some(&id)
            .filter(|id| id.starts_with("custom-"))
            .and_then(|_| exe_path.rsplit(['\\', '/']).next())
            .and_then(crate::emulators::identify_name);
        if let Some(def) = learnt {
            return Some(Self {
                id: def.id.to_string(),
                name: String::new(),
                exe_path,
                folders,
                extensions: Vec::new(),
                args: plain(&self.args, MAX_EMULATOR_ARGS_CHARS),
            });
        }
        Some(Self {
            id,
            name: plain(&self.name, 60),
            exe_path,
            folders,
            extensions,
            args: plain(&self.args, MAX_EMULATOR_ARGS_CHARS),
        })
    }
}

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
    /// Parts of the side panel the user hid, from [`HIDEABLE_SECTIONS`]. With the Home page
    /// hidden, Kanso opens on the library.
    pub hidden_sections: Vec<String>,
    /// How the user wants the Home page.
    pub home: HomeConfig,
    /// Emulators added by the user, and folders added to the ones Kanso found.
    pub emulators: Vec<EmulatorSetting>,
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
            hidden_sections: Vec::new(),
            home: HomeConfig::default(),
            emulators: Vec::new(),
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
            hidden_sections: HIDEABLE_SECTIONS
                .iter()
                .filter(|choice| self.hidden_sections.iter().any(|item| item.trim() == **choice))
                .map(|choice| choice.to_string())
                .collect(),
            home: self.home.sanitized(),
            emulators: {
                // One entry per emulator: a later one with the same id is a leftover
                let mut seen = HashSet::new();
                self.emulators
                    .into_iter()
                    .filter_map(EmulatorSetting::sanitized)
                    .filter(|emulator| seen.insert(emulator.id.clone()))
                    .take(MAX_EMULATORS)
                    .collect()
            },
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
            emulators: vec![
                EmulatorSetting {
                    id: " Dolphin ".to_string(),
                    exe_path: r" D:\Emu\Dolphin\Dolphin.exe ".to_string(),
                    folders: vec![r"D:\Roms\Wii".to_string(), r"d:/roms/wii/".to_string(), " ".to_string()],
                    extensions: vec![".ISO".to_string(), "rvz".to_string(), "iso".to_string(), "a b".to_string()],
                    args: " -b -e {rom}\n ".to_string(),
                    ..EmulatorSetting::default()
                },
                // The same emulator again, and an id that is not one
                EmulatorSetting { id: "dolphin".to_string(), ..EmulatorSetting::default() },
                EmulatorSetting { id: "../escape".to_string(), ..EmulatorSetting::default() },
                // Added by hand when Kanso did not know it, known since
                EmulatorSetting {
                    id: "custom-shadps4".to_string(),
                    name: "shadPS4".to_string(),
                    exe_path: r"D:\shadPS4\shadPS4.exe".to_string(),
                    folders: vec![r"D:\Giochi PS4".to_string()],
                    extensions: vec!["bin".to_string()],
                    ..EmulatorSetting::default()
                },
                // One Kanso still does not know stays as the user described it
                EmulatorSetting {
                    id: "custom-openbor".to_string(),
                    name: " OpenBOR ".to_string(),
                    exe_path: r"D:\OpenBOR\OpenBOR.exe".to_string(),
                    extensions: vec!["pak".to_string()],
                    ..EmulatorSetting::default()
                },
            ],
            hidden_sections: vec![" tags ".to_string(), "library".to_string(), "home".to_string(), "tags".to_string()],
            home: HomeConfig {
                hidden: vec!["deals".to_string(), "everything".to_string(), "resume".to_string()],
                effects: false,
                weather: true,
                place: Some(WeatherPlace { name: "  Torino\n ".to_string(), latitude: 45.070_312, longitude: 7.686_856 }),
                up_next: ["steam:1", " steam:2 ", "steam:1", "", "gog:3", "epic:4", "ea:5", "xbox:6"].map(str::to_string).to_vec(),
            },
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
        assert_eq!(saved.hidden_sections, ["home", "tags"], "only what can be hidden, once each");
        assert_eq!(saved.home.hidden, ["resume", "deals"], "known parts of Home, in Kanso's order");
        assert!(!saved.home.effects && saved.home.weather);
        assert_eq!(
            saved.home.place,
            Some(WeatherPlace { name: "Torino".to_string(), latitude: 45.07, longitude: 7.69 }),
            "a place is kept coarse: about a kilometre"
        );
        assert_eq!(saved.home.up_next, ["steam:1", "steam:2", "gog:3", "epic:4", "ea:5"], "five at most, each once");
        // Weather with no place, or a place that is not one, is no weather
        let nowhere = AppConfig { home: HomeConfig { weather: true, ..HomeConfig::default() }, ..AppConfig::default() }.sanitized();
        assert!(!nowhere.home.weather && nowhere.home.effects, "the seasonal background is on until switched off");
        let off_the_map = AppConfig {
            home: HomeConfig { weather: true, place: Some(WeatherPlace { name: "X".to_string(), latitude: 123.0, longitude: 0.0 }), ..HomeConfig::default() },
            ..AppConfig::default()
        }
        .sanitized();
        assert_eq!((off_the_map.home.weather, off_the_map.home.place), (false, None));
        assert_eq!(
            saved.emulators[..1],
            [EmulatorSetting {
                id: "dolphin".to_string(),
                name: String::new(),
                exe_path: r"D:\Emu\Dolphin\Dolphin.exe".to_string(),
                folders: vec![r"D:\Roms\Wii".to_string()],
                extensions: vec!["iso".to_string(), "rvz".to_string()],
                args: "-b -e {rom}".to_string(),
            }]
        );
        assert_eq!(
            saved.emulators[1..],
            [
                EmulatorSetting {
                    id: "shadps4".to_string(),
                    exe_path: r"D:\shadPS4\shadPS4.exe".to_string(),
                    folders: vec![r"D:\Giochi PS4".to_string()],
                    ..EmulatorSetting::default()
                },
                EmulatorSetting {
                    id: "custom-openbor".to_string(),
                    name: "OpenBOR".to_string(),
                    exe_path: r"D:\OpenBOR\OpenBOR.exe".to_string(),
                    extensions: vec!["pak".to_string()],
                    ..EmulatorSetting::default()
                },
            ]
        );
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
