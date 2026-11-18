//! `plugin.json`: what a plugin is, which hosts it may contact and what it adds to Kanso.

use super::library::CLIENTS;
use crate::config::is_valid_plugin_id;
use crate::i18n::Lang;
use serde_json::{Map, Value};
use std::collections::BTreeMap;

/// Version of the plugin format and API this build understands (`kansoPlugin` in the manifest).
pub const PLUGIN_API_VERSION: u64 = 1;
pub const MANIFEST_FILE: &str = "plugin.json";
pub const ENTRY_FILE: &str = "main.js";

/// Limits on a manifest; generous for honest plugins, tight enough to keep the settings page
/// and the permission prompt readable.
const MAX_HOSTS: usize = 8;
const MAX_COMMANDS: usize = 8;
const MAX_SETTINGS: usize = 12;
const MAX_OPTIONS: usize = 24;
/// How long an answer is reused when the manifest does not say (one week).
const DEFAULT_CACHE_HOURS: u64 = 168;
const MAX_CACHE_HOURS: u64 = 24 * 90;

/// A text given either once or per language (`{"en": "...", "it": "..."}`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LocalizedText(BTreeMap<String, String>);

impl LocalizedText {
    /// The text in `lang`, else in English, else in whatever language it was written.
    pub fn get(&self, lang: Lang) -> &str {
        self.0
            .get(lang.code())
            .or_else(|| self.0.get("en"))
            .or_else(|| self.0.get(""))
            .or_else(|| self.0.values().next())
            .map(String::as_str)
            .unwrap_or_default()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    fn parse(value: Option<&Value>, max_chars: usize) -> Self {
        let clean = |raw: &str| -> Option<String> {
            let text: String = raw.trim().chars().take(max_chars).collect();
            (!text.is_empty()).then_some(text)
        };
        let texts = match value {
            Some(Value::String(raw)) => clean(raw).map(|text| (String::new(), text)).into_iter().collect(),
            Some(Value::Object(by_lang)) => by_lang
                .iter()
                .filter(|(lang, _)| lang.len() <= 8)
                .filter_map(|(lang, text)| Some((lang.to_ascii_lowercase(), clean(text.as_str()?)?)))
                .collect(),
            _ => BTreeMap::new(),
        };
        Self(texts)
    }
}

/// An action the plugin adds to the Plugins menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginCommand {
    pub id: String,
    pub title: LocalizedText,
    pub description: LocalizedText,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingKind {
    Text,
    /// Like text, but masked in the interface (API keys).
    Secret,
    Toggle,
    Select,
}

impl SettingKind {
    pub fn as_str(self) -> &'static str {
        match self {
            SettingKind::Text => "text",
            SettingKind::Secret => "secret",
            SettingKind::Toggle => "toggle",
            SettingKind::Select => "select",
        }
    }
}

/// A value the user can set for the plugin in Settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginSetting {
    pub key: String,
    pub kind: SettingKind,
    pub label: LocalizedText,
    pub help: LocalizedText,
    pub default: String,
    /// Choices of a [`SettingKind::Select`]: value and label.
    pub options: Vec<(String, LocalizedText)>,
}

/// What a plugin that brings a library of games declares about it (`library` in the manifest).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PluginLibrary {
    /// What the library is called among the platforms; empty when the plugin's own name is.
    pub name: LocalizedText,
    /// The game clients its games may be handed to, by their names in
    /// [`super::library::CLIENTS`]. Web pages can always be opened.
    pub opens: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginManifest {
    pub id: String,
    pub name: LocalizedText,
    pub version: String,
    pub author: Option<String>,
    pub description: LocalizedText,
    pub homepage: Option<String>,
    /// The only hosts the plugin may contact, shown to the user before installing.
    pub hosts: Vec<String>,
    /// Implements `gameDetails(game)`: sections for the game's sheet.
    pub game_details: bool,
    /// `gameDetails` may also return a short label for the game cards.
    pub badge: bool,
    /// `gameDetails` may be run quietly for the whole library, so the labels are there
    /// before each sheet is opened.
    pub background: bool,
    /// Implements `home(context)`: a part of the Home page.
    pub home: bool,
    /// Implements `library(context)`: games that join the library under a platform of their
    /// own. `None` when the plugin brings no library.
    pub library: Option<PluginLibrary>,
    /// Hours an answer of `gameDetails` is reused (and of `home` and `library`, where a day
    /// is the most).
    pub cache_hours: u64,
    pub commands: Vec<PluginCommand>,
    pub settings: Vec<PluginSetting>,
    /// Honoured only for the plugins shipped with Kanso.
    pub enabled_by_default: bool,
}

impl PluginManifest {
    /// Parses and validates a `plugin.json`. The error says what to fix, for plugin authors.
    pub fn parse(raw: &str) -> Result<Self, String> {
        let root: Value = serde_json::from_str(raw).map_err(|e| format!("plugin.json: {e}"))?;
        let root = root
            .as_object()
            .ok_or("plugin.json: expected a JSON object")?;

        match root.get("kansoPlugin").and_then(Value::as_u64) {
            Some(PLUGIN_API_VERSION) => {}
            Some(other) => {
                return Err(format!(
                    "plugin.json: \"kansoPlugin\" is {other}, this version of Kanso supports {PLUGIN_API_VERSION}"
                ))
            }
            None => return Err("plugin.json: \"kansoPlugin\": 1 is missing".to_string()),
        }

        let id = text_field(root, "id", 40).unwrap_or_default();
        if !is_valid_plugin_id(&id) {
            return Err(
                "plugin.json: \"id\" must be 2-40 lower-case letters, digits or dashes".to_string(),
            );
        }
        let name = LocalizedText::parse(root.get("name"), 60);
        if name.is_empty() {
            return Err("plugin.json: \"name\" is missing".to_string());
        }
        let version = text_field(root, "version", 20)
            .filter(|v| v.bytes().all(|b| b.is_ascii_alphanumeric() || b".+-".contains(&b)))
            .ok_or("plugin.json: \"version\" is missing or not a plain version like 1.0.0")?;

        let hosts = match root.get("hosts") {
            None | Some(Value::Null) => Vec::new(),
            Some(Value::Array(items)) => {
                if items.len() > MAX_HOSTS {
                    return Err(format!("plugin.json: at most {MAX_HOSTS} hosts"));
                }
                let mut hosts = Vec::new();
                for item in items {
                    let host = item.as_str().unwrap_or_default().trim().to_ascii_lowercase();
                    if !is_valid_host(&host) {
                        return Err(format!(
                            "plugin.json: \"{host}\" is not a plain host name (no scheme, path, port, wildcard or IP address)"
                        ));
                    }
                    if !hosts.contains(&host) {
                        hosts.push(host);
                    }
                }
                hosts
            }
            Some(_) => return Err("plugin.json: \"hosts\" must be a list of host names".to_string()),
        };

        let provides = root.get("provides").and_then(Value::as_object);
        let provided = |key: &str| {
            provides
                .and_then(|p| p.get(key))
                .and_then(Value::as_bool)
                .unwrap_or(false)
        };
        let game_details = provided("gameDetails");
        let home = provided("home");

        let mut commands = Vec::new();
        for item in list_field(root, "commands", MAX_COMMANDS)? {
            let command = item.as_object().ok_or("plugin.json: a command must be an object")?;
            let id = text_field(command, "id", 32).unwrap_or_default();
            if !is_identifier(&id) {
                return Err(format!("plugin.json: command id \"{id}\" must be a plain identifier"));
            }
            let title = LocalizedText::parse(command.get("title"), 60);
            if title.is_empty() {
                return Err(format!("plugin.json: command \"{id}\" has no title"));
            }
            if commands.iter().any(|c: &PluginCommand| c.id == id) {
                return Err(format!("plugin.json: command \"{id}\" is declared twice"));
            }
            commands.push(PluginCommand {
                id,
                title,
                description: LocalizedText::parse(command.get("description"), 200),
            });
        }

        let mut settings = Vec::new();
        for item in list_field(root, "settings", MAX_SETTINGS)? {
            let setting = item.as_object().ok_or("plugin.json: a setting must be an object")?;
            let key = text_field(setting, "key", 32).unwrap_or_default();
            if !is_identifier(&key) {
                return Err(format!("plugin.json: setting key \"{key}\" must be a plain identifier"));
            }
            if settings.iter().any(|s: &PluginSetting| s.key == key) {
                return Err(format!("plugin.json: setting \"{key}\" is declared twice"));
            }
            let kind = match setting.get("type").and_then(Value::as_str).unwrap_or("text") {
                "text" => SettingKind::Text,
                "secret" => SettingKind::Secret,
                "toggle" => SettingKind::Toggle,
                "select" => SettingKind::Select,
                other => return Err(format!("plugin.json: setting \"{key}\" has unknown type \"{other}\"")),
            };
            let mut options = Vec::new();
            if kind == SettingKind::Select {
                for option in list_field(setting, "options", MAX_OPTIONS)? {
                    let (value, label) = match option {
                        Value::String(value) => (value.trim().to_string(), LocalizedText::default()),
                        Value::Object(option) => (
                            text_field(option, "value", 80).unwrap_or_default(),
                            LocalizedText::parse(option.get("label"), 80),
                        ),
                        _ => continue,
                    };
                    if !value.is_empty() {
                        options.push((value, label));
                    }
                }
                if options.is_empty() {
                    return Err(format!("plugin.json: setting \"{key}\" needs \"options\""));
                }
            }
            let default = match setting.get("default") {
                Some(Value::String(text)) => text.trim().chars().take(200).collect(),
                Some(Value::Bool(flag)) => flag.to_string(),
                Some(Value::Number(number)) => number.to_string(),
                _ => String::new(),
            };
            let label = LocalizedText::parse(setting.get("label"), 80);
            settings.push(PluginSetting {
                label: if label.is_empty() {
                    LocalizedText::parse(Some(&Value::String(key.clone())), 80)
                } else {
                    label
                },
                key,
                kind,
                help: LocalizedText::parse(setting.get("help"), 300),
                default,
                options,
            });
        }

        let library = if provided("library") {
            let declared = match root.get("library") {
                None | Some(Value::Null) => None,
                Some(Value::Object(declared)) => Some(declared),
                Some(_) => return Err("plugin.json: \"library\" must be an object".to_string()),
            };
            let mut opens = Vec::new();
            if let Some(declared) = declared {
                for item in list_field(declared, "opens", CLIENTS.len())? {
                    let name = item.as_str().unwrap_or_default().trim().to_ascii_lowercase();
                    if !CLIENTS.iter().any(|client| client.name == name) {
                        let known: Vec<&str> = CLIENTS.iter().map(|client| client.name).collect();
                        return Err(format!(
                            "plugin.json: \"{name}\" under \"library.opens\" is not a game client Kanso knows ({}); web pages need no entry",
                            known.join(", ")
                        ));
                    }
                    if !opens.contains(&name) {
                        opens.push(name);
                    }
                }
            }
            Some(PluginLibrary {
                name: LocalizedText::parse(declared.and_then(|declared| declared.get("name")), 40),
                opens,
            })
        } else {
            None
        };

        if !game_details && !home && library.is_none() && commands.is_empty() {
            return Err(
                "plugin.json: the plugin adds nothing: set \"gameDetails\", \"home\" or \"library\" to true under \"provides\", or declare \"commands\""
                    .to_string(),
            );
        }

        Ok(Self {
            id,
            name,
            version,
            author: text_field(root, "author", 80),
            description: LocalizedText::parse(root.get("description"), 400),
            homepage: text_field(root, "homepage", 300).filter(|url| url.starts_with("https://")),
            hosts,
            game_details,
            badge: game_details && provided("badge"),
            background: game_details && provided("background"),
            home,
            library,
            cache_hours: root
                .get("cacheHours")
                .and_then(Value::as_u64)
                .unwrap_or(DEFAULT_CACHE_HOURS)
                .clamp(1, MAX_CACHE_HOURS),
            commands,
            settings,
            enabled_by_default: root
                .get("enabledByDefault")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        })
    }
}

fn text_field(object: &Map<String, Value>, key: &str, max_chars: usize) -> Option<String> {
    let text: String = object.get(key)?.as_str()?.trim().chars().take(max_chars).collect();
    (!text.is_empty()).then_some(text)
}

fn list_field<'a>(
    object: &'a Map<String, Value>,
    key: &str,
    max_items: usize,
) -> Result<&'a [Value], String> {
    match object.get(key) {
        None | Some(Value::Null) => Ok(&[]),
        Some(Value::Array(items)) if items.len() <= max_items => Ok(items),
        Some(Value::Array(_)) => Err(format!("plugin.json: at most {max_items} entries in \"{key}\"")),
        Some(_) => Err(format!("plugin.json: \"{key}\" must be a list")),
    }
}

/// A JavaScript-style identifier of at most 32 characters.
fn is_identifier(raw: &str) -> bool {
    let mut bytes = raw.bytes();
    (1..=32).contains(&raw.len())
        && bytes.next().is_some_and(|b| b.is_ascii_alphabetic())
        && bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

/// A public DNS name: dot-separated labels of letters, digits and dashes, ending in a
/// non-numeric label. Rules out IP addresses, `localhost`, ports, paths and wildcards.
pub fn is_valid_host(host: &str) -> bool {
    let labels: Vec<&str> = host.split('.').collect();
    host.len() <= 253
        && labels.len() >= 2
        && labels.iter().all(|label| {
            (1..=63).contains(&label.len())
                && label.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
                && !label.starts_with('-')
                && !label.ends_with('-')
        })
        && labels
            .last()
            .is_some_and(|tld| tld.len() >= 2 && tld.bytes().all(|b| b.is_ascii_lowercase()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_complete_manifest() {
        let manifest = PluginManifest::parse(
            r#"{
              "kansoPlugin": 1,
              "id": "price-history",
              "name": {"en": "Price history", "it": "Storico prezzi"},
              "version": "1.2.0",
              "author": "Someone",
              "description": "Lowest price ever.",
              "homepage": "https://example.org/plugin",
              "hosts": ["API.Example.org", "api.example.org", "example.org"],
              "provides": {"gameDetails": true, "badge": true, "background": true},
              "cacheHours": 100000,
              "enabledByDefault": true,
              "commands": [{"id": "refresh", "title": {"en": "Refresh"}, "description": "Again."}],
              "settings": [
                {"key": "apiKey", "type": "secret", "label": {"it": "Chiave API"}, "help": "Get one."},
                {"key": "country", "type": "select", "default": "IT",
                 "options": ["IT", {"value": "US", "label": {"en": "United States"}}]},
                {"key": "bundles", "type": "toggle", "default": true}
              ]
            }"#,
        )
        .expect("valid manifest");

        assert_eq!(manifest.id, "price-history");
        assert_eq!(manifest.name.get(Lang::It), "Storico prezzi");
        assert_eq!(manifest.name.get(Lang::Ru), "Price history", "falls back to English");
        assert_eq!(manifest.description.get(Lang::Es), "Lowest price ever.");
        assert_eq!(manifest.hosts, ["api.example.org", "example.org"]);
        assert!(manifest.game_details && manifest.badge && manifest.background);
        assert!(!manifest.home, "a part of Home only when the manifest says so");
        assert_eq!(manifest.cache_hours, 24 * 90, "clamped");
        assert!(manifest.enabled_by_default);
        assert_eq!(manifest.commands[0].id, "refresh");
        assert_eq!(manifest.settings.len(), 3);
        assert_eq!(manifest.settings[0].kind, SettingKind::Secret);
        assert_eq!(manifest.settings[0].label.get(Lang::En), "Chiave API");
        assert_eq!(manifest.settings[1].default, "IT");
        assert_eq!(manifest.settings[1].options.len(), 2);
        assert_eq!(manifest.settings[1].options[1].1.get(Lang::It), "United States");
        assert_eq!(manifest.settings[2].default, "true");
        assert_eq!(manifest.settings[2].label.get(Lang::It), "bundles", "the key stands in for a missing label");
    }

    #[test]
    fn rejects_manifests_that_are_wrong_or_ask_for_too_much() {
        let with = |patch: &str| {
            let base = r#""kansoPlugin": 1, "id": "demo", "name": "Demo", "version": "1.0", "provides": {"gameDetails": true}"#;
            PluginManifest::parse(&format!("{{{base}{patch}}}"))
        };
        assert!(with("").is_ok());

        let error = |patch: &str| with(patch).expect_err(patch);
        assert!(error(r#", "hosts": ["https://example.org"]"#).contains("plain host name"));
        assert!(error(r#", "hosts": ["127.0.0.1"]"#).contains("plain host name"));
        assert!(error(r#", "hosts": ["localhost"]"#).contains("plain host name"));
        assert!(error(r#", "hosts": ["*.example.org"]"#).contains("plain host name"));
        assert!(error(r#", "hosts": ["example.org:8080"]"#).contains("plain host name"));
        assert!(error(r#", "hosts": ["example.org/path"]"#).contains("plain host name"));
        assert!(error(r#", "hosts": "example.org""#).contains("must be a list"));
        assert!(error(r#", "commands": [{"id": "do it", "title": "x"}]"#).contains("identifier"));
        assert!(error(r#", "settings": [{"key": "a", "type": "file"}]"#).contains("unknown type"));
        assert!(error(r#", "settings": [{"key": "a", "type": "select"}]"#).contains("options"));

        let bad = |raw: &str| PluginManifest::parse(raw).expect_err(raw);
        assert!(bad("[]").contains("JSON object"));
        assert!(bad(r#"{"id": "demo"}"#).contains("kansoPlugin"));
        assert!(bad(r#"{"kansoPlugin": 2, "id": "demo"}"#).contains("supports 1"));
        assert!(bad(r#"{"kansoPlugin": 1, "id": "../demo", "name": "x", "version": "1"}"#).contains("\"id\""));
        assert!(bad(r#"{"kansoPlugin": 1, "id": "demo", "version": "1"}"#).contains("\"name\""));
        assert!(bad(r#"{"kansoPlugin": 1, "id": "demo", "name": "x", "version": "1 0"}"#).contains("version"));
        assert!(bad(r#"{"kansoPlugin": 1, "id": "demo", "name": "x", "version": "1"}"#).contains("adds nothing"));
    }

    #[test]
    fn a_part_of_home_is_enough_for_a_plugin_and_must_be_said_plainly() {
        let with = |provides: &str| {
            PluginManifest::parse(&format!(
                r#"{{"kansoPlugin": 1, "id": "demo", "name": "Demo", "version": "1.0", "provides": {provides}}}"#
            ))
        };

        // Nothing but a part of Home: a plugin in its own right, and nothing else is assumed
        let home_only = with(r#"{"home": true}"#).expect("valid manifest");
        assert!(home_only.home);
        assert!(!home_only.game_details && !home_only.badge && !home_only.background);
        assert!(home_only.commands.is_empty());

        // Alongside what a plugin could already add
        let both = with(r#"{"home": true, "gameDetails": true, "badge": true}"#).expect("valid manifest");
        assert!(both.home && both.game_details && both.badge);

        // Only `true` switches it on: anything else is "no", and a plugin with nothing else adds nothing
        for provides in [r#"{"home": false}"#, r#"{"home": "yes"}"#, r#"{"home": 1}"#, r#"{"home": null}"#, "{}"] {
            assert!(with(provides).expect_err(provides).contains("adds nothing"), "{provides}");
        }
        let error = with("{}").unwrap_err();
        assert!(error.contains("\"home\"") && error.contains("\"gameDetails\"") && error.contains("\"commands\""), "{error}");
    }

    #[test]
    fn the_manifest_in_the_plugin_guide_is_valid() {
        // The guide's first block of JSON is the manifest authors start from
        // On a Windows checkout the guide's lines may end in CR LF
        let guide = include_str!("../../../../../docs/PLUGINS.md").replace("\r\n", "\n");
        let start = guide.find("```json\n").expect("the guide has a manifest") + "```json\n".len();
        let example = &guide[start..start + guide[start..].find("\n```").expect("the example ends")];
        let manifest = PluginManifest::parse(example).expect("the manifest in docs/PLUGINS.md must stay valid");

        assert_eq!(manifest.id, "my-plugin");
        assert!(manifest.game_details && manifest.badge && manifest.background && manifest.home, "everything it says it provides");
        assert_eq!(manifest.cache_hours, 168);
        assert_eq!(manifest.hosts, ["api.example.org"]);
        assert_eq!(manifest.commands.len(), 1);
        assert_eq!(manifest.settings.len(), 4, "one setting of each type");
    }
}
