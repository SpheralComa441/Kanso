//! Community plugins: small JavaScript programs that add information to a game's sheet or
//! commands to the Plugins menu, without ever touching the interface or the user's data.
//!
//! - [`manifest`]: `plugin.json`, including the hosts a plugin may contact;
//! - [`registry`]: the shipped plugins and the user's plugins folder;
//! - [`runtime`]: the sandbox the code runs in;
//! - [`http`]: the checked door to the network;
//! - [`output`]: the plain data a plugin may return, which Kanso draws itself.
//!
//! [`PluginEngine`] ties them to the library: it runs a plugin for a game, caches the answer
//! in the database and hands the interface what to show.

pub mod http;
pub mod manifest;
pub mod output;
pub mod registry;
pub mod runtime;

use crate::config::{AppConfig, PluginState};
use crate::db::{LibraryDb, PluginCacheEntry};
use crate::i18n::{Lang, T};
use crate::models::DetectedGame;
use http::{HttpTransport, UreqTransport};
use manifest::SettingKind;
use output::{parse_game_data, parse_view, PluginGameData, PluginView};
use registry::{Plugin, PluginSource};
use runtime::{run_hook, HookCall, HookOutput, RunLimits};
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashSet};
use std::sync::{Arc, Mutex, MutexGuard};

/// How long a failed answer is remembered before the plugin is asked again.
const ERROR_RETRY_MS: u64 = 30 * 60 * 1000;
/// Log lines passed on with an error, for the plugin's author.
const ERROR_LOG_LINES: usize = 12;

/// Whether the user (or, for a shipped plugin never touched, its own default) has it on.
pub fn is_enabled(plugin: &Plugin, config: &AppConfig) -> bool {
    match config.plugins.get(&plugin.manifest.id) {
        Some(state) => state.enabled,
        None => plugin.source == PluginSource::Bundled && plugin.manifest.enabled_by_default,
    }
}

/// Whether the plugin's label should appear on the game cards.
pub fn shows_badge(plugin: &Plugin, config: &AppConfig) -> bool {
    plugin.manifest.badge
        && config
            .plugins
            .get(&plugin.manifest.id)
            .is_none_or(|state| state.show_badge)
}

/// The values of the settings the plugin declares: the user's, or the declared defaults.
/// Anything the manifest does not declare is not passed on.
pub fn effective_settings(plugin: &Plugin, config: &AppConfig) -> BTreeMap<String, String> {
    let saved = config.plugins.get(&plugin.manifest.id);
    plugin
        .manifest
        .settings
        .iter()
        .map(|setting| {
            let value = saved
                .and_then(|state| state.settings.get(&setting.key))
                .filter(|value| {
                    setting.kind != SettingKind::Select
                        || setting.options.iter().any(|(option, _)| option == *value)
                })
                .cloned()
                .unwrap_or_else(|| setting.default.clone());
            (setting.key.clone(), value)
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginCommandInfo {
    pub id: String,
    pub title: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginOptionInfo {
    pub value: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginSettingInfo {
    pub key: String,
    /// `text`, `secret`, `toggle` or `select`.
    pub kind: String,
    pub label: String,
    pub help: String,
    pub default: String,
    pub options: Vec<PluginOptionInfo>,
}

/// A plugin as the interface lists it, texts already in the interface language.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginInfo {
    pub id: String,
    pub name: String,
    pub version: String,
    pub author: Option<String>,
    pub description: String,
    pub homepage: Option<String>,
    pub hosts: Vec<String>,
    /// Shipped with Kanso (cannot be removed, only switched off).
    pub bundled: bool,
    pub enabled: bool,
    pub show_badge: bool,
    pub game_details: bool,
    pub badge: bool,
    pub background: bool,
    pub commands: Vec<PluginCommandInfo>,
    pub settings: Vec<PluginSettingInfo>,
}

pub fn describe(plugin: &Plugin, config: &AppConfig, lang: Lang) -> PluginInfo {
    let manifest = &plugin.manifest;
    PluginInfo {
        id: manifest.id.clone(),
        name: manifest.name.get(lang).to_string(),
        version: manifest.version.clone(),
        author: manifest.author.clone(),
        description: manifest.description.get(lang).to_string(),
        homepage: manifest.homepage.clone(),
        hosts: manifest.hosts.clone(),
        bundled: plugin.source == PluginSource::Bundled,
        enabled: is_enabled(plugin, config),
        show_badge: shows_badge(plugin, config),
        game_details: manifest.game_details,
        badge: manifest.badge,
        background: manifest.background,
        commands: manifest
            .commands
            .iter()
            .map(|command| PluginCommandInfo {
                id: command.id.clone(),
                title: command.title.get(lang).to_string(),
                description: command.description.get(lang).to_string(),
            })
            .collect(),
        settings: manifest
            .settings
            .iter()
            .map(|setting| PluginSettingInfo {
                key: setting.key.clone(),
                kind: setting.kind.as_str().to_string(),
                label: setting.label.get(lang).to_string(),
                help: setting.help.get(lang).to_string(),
                default: setting.default.clone(),
                options: setting
                    .options
                    .iter()
                    .map(|(value, label)| PluginOptionInfo {
                        value: value.clone(),
                        label: if label.is_empty() { value.clone() } else { label.get(lang).to_string() },
                    })
                    .collect(),
            })
            .collect(),
    }
}

/// The state a plugin gets when it is installed by hand: switched on.
pub fn installed_state(previous: Option<&PluginState>) -> PluginState {
    PluginState {
        enabled: true,
        ..previous.cloned().unwrap_or_default()
    }
}

/// A game as plugins see it. Deliberately without folders, executables or anything else that
/// says something about the PC rather than about the game.
pub fn game_payload(game: &DetectedGame) -> Value {
    json!({
        "id": game.id,
        "title": game.title,
        "cleanTitle": game.clean_title,
        "platform": game.platform.as_str(),
        "steamAppId": game.steam_store_id(),
        "description": game.short_description,
        "genres": game.genres,
        "tags": game.tags,
        "isFavorite": game.is_favorite,
        "installed": game.is_installed,
        "status": game.status.map(|status| status.as_str()),
        "playtimeMinutes": game.playtime_minutes,
        "lastPlayedAt": game.last_played_at,
        "playCount": game.play_count,
        "achievements": game.achievements_total.map(|total| json!({
            "unlocked": game.achievements_unlocked.unwrap_or(0),
            "total": total,
        })),
    })
}

/// What a plugin has to say about a game.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameDataAnswer {
    pub plugin_id: String,
    pub plugin_name: String,
    /// `None` when the plugin failed and nothing older is worth showing.
    pub data: Option<PluginGameData>,
    /// Why the last attempt failed, with the plugin's last log lines for its author.
    pub error: Option<String>,
    pub logs: Vec<String>,
    /// When `data` was obtained (ms).
    pub fetched_at: u64,
}

/// A plugin's label for a game card.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BadgeEntry {
    pub plugin_id: String,
    pub plugin_name: String,
    pub text: String,
    pub tooltip: Option<String>,
}

/// Runs plugins against the library.
pub struct PluginEngine {
    transport: Arc<dyn HttpTransport>,
    app_version: String,
    limits: RunLimits,
}

fn lock(db: &Mutex<LibraryDb>) -> MutexGuard<'_, LibraryDb> {
    db.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

impl PluginEngine {
    /// An engine talking to the real network.
    pub fn new(app_version: &str) -> Self {
        Self::with_transport(Arc::new(UreqTransport::new(&format!("Kanso/{app_version}"))), app_version)
    }

    pub fn with_transport(transport: Arc<dyn HttpTransport>, app_version: &str) -> Self {
        Self {
            transport,
            app_version: app_version.to_string(),
            limits: RunLimits::default(),
        }
    }

    /// Identifies what an answer depends on besides the game: the plugin's code and version,
    /// its settings and the interface language. A cached answer with another signature is stale.
    fn signature(plugin: &Plugin, settings: &BTreeMap<String, String>, lang: Lang) -> String {
        // FNV-1a: stable across runs and platforms, unlike the standard hasher
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        let mut feed = |bytes: &[u8]| {
            for byte in bytes {
                hash ^= u64::from(*byte);
                hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
            }
        };
        feed(plugin.code.as_bytes());
        for (key, value) in settings {
            feed(key.as_bytes());
            feed(&[0]);
            feed(value.as_bytes());
            feed(&[0]);
        }
        format!("{}|{}|{hash:016x}", plugin.manifest.version, lang.code())
    }

    fn call(
        &self,
        plugin: &Plugin,
        settings: &BTreeMap<String, String>,
        storage: &BTreeMap<String, String>,
        lang: Lang,
        hook: &str,
        args: &Value,
    ) -> Result<HookOutput, runtime::HookError> {
        run_hook(
            &HookCall {
                code: &plugin.code,
                hosts: &plugin.manifest.hosts,
                settings,
                storage,
                locale: lang.code(),
                app_version: &self.app_version,
                hook,
                args,
            },
            Arc::clone(&self.transport),
            &self.limits,
        )
    }

    /// What `plugin` shows for `game`: the cached answer while it is fresh, otherwise a new
    /// one (always with `force`). The database is locked only to read and write the cache,
    /// never while the plugin runs.
    #[allow(clippy::too_many_arguments)]
    pub fn game_data(
        &self,
        db: &Mutex<LibraryDb>,
        plugin: &Plugin,
        config: &AppConfig,
        lang: Lang,
        game: &DetectedGame,
        force: bool,
        now_ms: u64,
    ) -> GameDataAnswer {
        let settings = effective_settings(plugin, config);
        let signature = Self::signature(plugin, &settings, lang);
        let plugin_id = plugin.manifest.id.as_str();
        let answer = |data: Option<PluginGameData>, error: Option<String>, logs: Vec<String>, fetched_at: u64| {
            GameDataAnswer {
                plugin_id: plugin_id.to_string(),
                plugin_name: plugin.manifest.name.get(lang).to_string(),
                data,
                error,
                logs,
                fetched_at,
            }
        };

        let (cached, storage) = {
            let db = lock(db);
            (
                db.get_plugin_cache(plugin_id, &game.id).ok().flatten(),
                db.load_plugin_storage(plugin_id).unwrap_or_default(),
            )
        };
        let cached = cached.filter(|entry| entry.signature == signature);
        let cached_data = |entry: &PluginCacheEntry| {
            entry
                .ok
                .then(|| serde_json::from_str::<PluginGameData>(&entry.data).ok())
                .flatten()
        };
        if let Some(entry) = cached.as_ref().filter(|entry| !force && entry.expires_at > now_ms) {
            return match cached_data(entry) {
                Some(data) => answer(Some(data), None, Vec::new(), entry.fetched_at),
                None => answer(None, Some(entry.data.clone()), Vec::new(), entry.fetched_at),
            };
        }

        match self.call(plugin, &settings, &storage, lang, "gameDetails", &game_payload(game)) {
            Ok(output) => {
                let data = parse_game_data(&output.value);
                // An answer may ask to be kept for less than the plugin's usual time (data
                // not out yet, a service that could not be reached), never for longer
                let cache_hours = output
                    .value
                    .get("cacheHours")
                    .and_then(Value::as_f64)
                    .filter(|hours| hours.is_finite() && *hours > 0.0)
                    .map_or(plugin.manifest.cache_hours, |hours| {
                        (hours.ceil() as u64).clamp(1, plugin.manifest.cache_hours)
                    });
                let entry = PluginCacheEntry {
                    data: serde_json::to_string(&data).unwrap_or_else(|_| "{}".to_string()),
                    ok: true,
                    signature,
                    fetched_at: now_ms,
                    expires_at: now_ms + cache_hours * 3_600_000,
                };
                let mut db = lock(db);
                let _ = db.put_plugin_cache(plugin_id, &game.id, &entry);
                if let Some(storage) = &output.storage {
                    let _ = db.save_plugin_storage(plugin_id, storage);
                }
                answer(Some(data), None, Vec::new(), now_ms)
            }
            Err(error) => {
                let message = format!("{}: {}", T::PluginFailed.s(), error.message);
                let logs: Vec<String> = error
                    .logs
                    .iter()
                    .rev()
                    .take(ERROR_LOG_LINES)
                    .rev()
                    .cloned()
                    .collect();
                // An older good answer is better than an error: keep it and show it
                let previous = cached.as_ref().and_then(|entry| Some((cached_data(entry)?, entry.fetched_at)));
                if previous.is_none() {
                    let entry = PluginCacheEntry {
                        data: message.clone(),
                        ok: false,
                        signature,
                        fetched_at: now_ms,
                        expires_at: now_ms + ERROR_RETRY_MS,
                    };
                    let _ = lock(db).put_plugin_cache(plugin_id, &game.id, &entry);
                }
                match previous {
                    Some((data, fetched_at)) => answer(Some(data), Some(message), logs, fetched_at),
                    None => answer(None, Some(message), logs, now_ms),
                }
            }
        }
    }

    /// `true` when [`Self::game_data`] would have to run the plugin for this game.
    pub fn needs_game_data(
        &self,
        db: &LibraryDb,
        plugin: &Plugin,
        config: &AppConfig,
        lang: Lang,
        game_id: &str,
        now_ms: u64,
    ) -> bool {
        let signature = Self::signature(plugin, &effective_settings(plugin, config), lang);
        db.get_plugin_cache(&plugin.manifest.id, game_id)
            .ok()
            .flatten()
            .is_none_or(|entry| entry.signature != signature || entry.expires_at <= now_ms)
    }

    /// The labels the enabled plugins have for the game cards, by game id, from what is
    /// cached (an expired answer still labels the card until it is replaced).
    pub fn badges(
        &self,
        db: &LibraryDb,
        plugins: &[Plugin],
        config: &AppConfig,
        lang: Lang,
    ) -> BTreeMap<String, Vec<BadgeEntry>> {
        let mut by_game: BTreeMap<String, Vec<BadgeEntry>> = BTreeMap::new();
        for plugin in plugins {
            if !is_enabled(plugin, config) || !shows_badge(plugin, config) {
                continue;
            }
            let signature = Self::signature(plugin, &effective_settings(plugin, config), lang);
            for (game_id, entry) in db.list_plugin_cache(&plugin.manifest.id).unwrap_or_default() {
                if !entry.ok || entry.signature != signature {
                    continue;
                }
                let badge = serde_json::from_str::<PluginGameData>(&entry.data)
                    .ok()
                    .and_then(|data| data.badge);
                if let Some(badge) = badge {
                    by_game.entry(game_id).or_default().push(BadgeEntry {
                        plugin_id: plugin.manifest.id.clone(),
                        plugin_name: plugin.manifest.name.get(lang).to_string(),
                        text: badge.text,
                        tooltip: badge.tooltip,
                    });
                }
            }
        }
        by_game
    }

    /// Runs a command of the Plugins menu over the visible library.
    #[allow(clippy::too_many_arguments)]
    pub fn run_command(
        &self,
        db: &Mutex<LibraryDb>,
        plugin: &Plugin,
        config: &AppConfig,
        lang: Lang,
        command_id: &str,
        games: &[DetectedGame],
        now_ms: u64,
    ) -> Result<PluginView, String> {
        if !plugin.manifest.commands.iter().any(|command| command.id == command_id) {
            return Err(format!("{}: {command_id}", T::PluginNotFound.s()));
        }
        let plugin_id = plugin.manifest.id.as_str();
        let settings = effective_settings(plugin, config);
        let storage = lock(db).load_plugin_storage(plugin_id).unwrap_or_default();
        let visible: Vec<&DetectedGame> = games.iter().filter(|game| !game.is_hidden).collect();
        let args = json!({
            "games": visible.iter().map(|game| game_payload(game)).collect::<Vec<_>>(),
            "now": now_ms,
        });

        let output = self
            .call(plugin, &settings, &storage, lang, &format!("commands.{command_id}"), &args)
            .map_err(|error| format!("{}: {}", T::PluginFailed.s(), error.message))?;
        if let Some(storage) = &output.storage {
            let _ = lock(db).save_plugin_storage(plugin_id, storage);
        }

        let mut view = parse_view(&output.value);
        // Only games that are really in the library can be pointed at
        let known: HashSet<&str> = visible.iter().map(|game| game.id.as_str()).collect();
        view.games.retain(|pick| known.contains(pick.id.as_str()));
        Ok(view)
    }
}

#[cfg(test)]
mod bundled_tests;

#[cfg(test)]
mod tests {
    use super::runtime::tests::{ok_json, FakeTransport};
    use super::*;
    use crate::models::{GamePlatform, GameStatus};

    fn test_plugin(code: &str) -> Plugin {
        Plugin {
            manifest: manifest::PluginManifest::parse(
                r#"{
                  "kansoPlugin": 1, "id": "demo", "name": {"en": "Demo", "it": "Dimostrazione"},
                  "version": "1.0.0", "hosts": ["api.example.org"], "cacheHours": 2,
                  "provides": {"gameDetails": true, "badge": true},
                  "commands": [{"id": "pick", "title": "Pick"}],
                  "settings": [
                    {"key": "apiKey", "type": "secret"},
                    {"key": "region", "type": "select", "default": "eu", "options": ["eu", "us"]}
                  ]
                }"#,
            )
            .unwrap(),
            code: code.to_string(),
            source: PluginSource::User,
        }
    }

    fn game(id: &str, title: &str) -> DetectedGame {
        DetectedGame::new_detected(
            id.to_string(),
            title.to_string(),
            title.to_string(),
            GamePlatform::Epic,
            Some("internal-name".to_string()),
            r"C:\Users\someone\Games\Secret Folder".to_string(),
            Some(r"C:\Users\someone\Games\Secret Folder\game.exe".to_string()),
            Some("com.epicgames.launcher://apps/x".to_string()),
        )
    }

    fn enabled(settings: &[(&str, &str)]) -> AppConfig {
        let mut config = AppConfig::default();
        config.plugins.insert(
            "demo".to_string(),
            PluginState {
                enabled: true,
                show_badge: true,
                settings: settings.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
            },
        );
        config
    }

    const DETAILS: &str = r#"
        kanso.plugin({
          gameDetails(game) {
            const info = kanso.http.getJson("https://api.example.org/hours?title=" + encodeURIComponent(game.title) + "&region=" + kanso.settings.region);
            kanso.storage.set("calls", (kanso.storage.get("calls") || 0) + 1);
            return {
              badge: { text: info.hours + " h", tooltip: "Main story" },
              sections: [{ title: "Demo", blocks: [{ type: "stats", items: [{ label: "Main", value: info.hours + " h" }] }] }],
            };
          },
          commands: {
            pick(context) {
              const unplayed = context.games.filter((game) => !game.playtimeMinutes);
              return { title: "Pick", games: [{ id: unplayed[0].id, note: "Never played" }, { id: "ghost:1" }] };
            },
          },
        });
    "#;

    #[test]
    fn plugins_see_the_game_but_nothing_about_the_pc() {
        let mut portal = game("epic:portal", "Portal 2");
        portal.steam_store_appid = Some("620".to_string());
        portal.status = Some(GameStatus::Playing);
        portal.playtime_minutes = Some(90);
        portal.achievements_total = Some(50);
        portal.achievements_unlocked = Some(7);
        portal.tags = vec!["Puzzle".to_string()];

        let payload = game_payload(&portal);
        assert_eq!(payload["steamAppId"], "620");
        assert_eq!(payload["platform"], "epic");
        assert_eq!(payload["status"], "playing");
        assert_eq!(payload["achievements"], json!({"unlocked": 7, "total": 50}));
        let text = payload.to_string();
        for private in ["Secret Folder", "someone", "game.exe", "epicgames.launcher", "internal-name"] {
            assert!(!text.contains(private), "{private} leaked: {text}");
        }

        assert_eq!(game_payload(&game("epic:x", "X"))["steamAppId"], Value::Null);
    }

    #[test]
    fn answers_are_cached_until_they_expire_or_the_settings_change() {
        let db = Mutex::new(LibraryDb::open_in_memory().unwrap());
        let transport = FakeTransport::new(|_| ok_json(json!({"hours": 12})));
        let engine = PluginEngine::with_transport(transport.clone(), "0.1.0");
        let plugin = test_plugin(DETAILS);
        let portal = game("epic:portal", "Portal 2");
        let config = enabled(&[]);
        let hour = 3_600_000;

        let first = engine.game_data(&db, &plugin, &config, Lang::It, &portal, false, 1_000);
        assert_eq!(first.plugin_name, "Dimostrazione");
        assert_eq!(first.error, None);
        let data = first.data.expect("data");
        assert_eq!(data.badge.as_ref().map(|b| b.text.as_str()), Some("12 h"));
        assert_eq!(data.sections.len(), 1);
        assert_eq!(transport.urls(), ["https://api.example.org/hours?title=Portal%202&region=eu"]);

        // Fresh: answered from the cache, the plugin is not run
        let again = engine.game_data(&db, &plugin, &config, Lang::It, &portal, false, 1_000 + hour);
        assert_eq!(again.data.as_ref(), Some(&data));
        assert_eq!(again.fetched_at, 1_000);
        assert_eq!(transport.urls().len(), 1);
        assert!(!engine.needs_game_data(&lock(&db), &plugin, &config, Lang::It, "epic:portal", 1_000 + hour));

        // Expired (cacheHours: 2), forced, another setting, another language: asked again
        assert!(engine.needs_game_data(&lock(&db), &plugin, &config, Lang::It, "epic:portal", 1_000 + 3 * hour));
        engine.game_data(&db, &plugin, &config, Lang::It, &portal, false, 1_000 + 3 * hour);
        engine.game_data(&db, &plugin, &config, Lang::It, &portal, true, 1_000 + 3 * hour);
        let us = enabled(&[("region", "us"), ("undeclared", "x")]);
        assert!(engine.needs_game_data(&lock(&db), &plugin, &us, Lang::It, "epic:portal", 1_000 + 3 * hour));
        engine.game_data(&db, &plugin, &us, Lang::It, &portal, false, 1_000 + 3 * hour);
        engine.game_data(&db, &plugin, &us, Lang::En, &portal, false, 1_000 + 3 * hour);
        let urls = transport.urls();
        assert_eq!(urls.len(), 5);
        assert!(urls[3].ends_with("region=us"));

        // What the plugin stored survived every run
        assert_eq!(
            lock(&db).load_plugin_storage("demo").unwrap().get("calls").map(String::as_str),
            Some("5")
        );

        // The cached label feeds the game cards, unless the user switched labels or the plugin off
        let plugins = [plugin.clone()];
        let badges = engine.badges(&lock(&db), &plugins, &us, Lang::En);
        assert_eq!(badges["epic:portal"][0].text, "12 h");
        assert_eq!(badges["epic:portal"][0].plugin_name, "Demo");
        let mut hidden_badge = us.clone();
        hidden_badge.plugins.get_mut("demo").unwrap().show_badge = false;
        assert!(engine.badges(&lock(&db), &plugins, &hidden_badge, Lang::En).is_empty());
        assert!(engine.badges(&lock(&db), &plugins, &AppConfig::default(), Lang::En).is_empty());
    }

    #[test]
    fn an_answer_may_ask_to_be_cached_for_less_but_never_for_longer() {
        let db = Mutex::new(LibraryDb::open_in_memory().unwrap());
        let engine = PluginEngine::with_transport(FakeTransport::offline(), "0.1.0");
        let config = enabled(&[]);
        let hour = 3_600_000;
        let expiry = |code: &str, id: &str| {
            let plugin = test_plugin(code);
            let game = game(id, "Any");
            engine.game_data(&db, &plugin, &config, Lang::En, &game, false, 0);
            lock(&db).get_plugin_cache("demo", id).unwrap().unwrap().expires_at
        };

        // The test plugin's manifest says two hours
        let reply = |hours: &str| {
            format!("kanso.plugin({{ gameDetails: () => ({{ cacheHours: {hours}, sections: [] }}) }});")
        };
        assert_eq!(expiry(&reply("undefined"), "g:default"), 2 * hour);
        assert_eq!(expiry(&reply("1"), "g:shorter"), hour);
        assert_eq!(expiry(&reply("0.2"), "g:fraction"), hour, "at least one hour");
        assert_eq!(expiry(&reply("500"), "g:longer"), 2 * hour, "capped at the manifest's");
        assert_eq!(expiry(&reply("-3"), "g:negative"), 2 * hour);
        assert_eq!(expiry(&reply("'soon'"), "g:text"), 2 * hour);
    }

    #[test]
    fn a_failure_keeps_the_last_good_answer_and_is_not_retried_at_once() {
        let db = Mutex::new(LibraryDb::open_in_memory().unwrap());
        let online = std::sync::atomic::AtomicBool::new(false);
        let online = Arc::new(online);
        let transport = {
            let online = Arc::clone(&online);
            FakeTransport::new(move |_| {
                if online.load(std::sync::atomic::Ordering::SeqCst) {
                    ok_json(json!({"hours": 8}))
                } else {
                    Err("dns error".to_string())
                }
            })
        };
        let engine = PluginEngine::with_transport(transport.clone(), "0.1.0");
        let plugin = test_plugin(DETAILS);
        let config = enabled(&[]);
        let hades = game("epic:hades", "Hades");

        // Offline with nothing cached: an error, remembered for a while
        let failed = engine.game_data(&db, &plugin, &config, Lang::En, &hades, false, 0);
        assert_eq!(failed.data, None);
        assert!(failed.error.as_deref().unwrap().contains("dns error"), "{:?}", failed.error);
        assert!(failed.logs.iter().any(|line| line.contains("main.js")), "{:?}", failed.logs);
        let soon = engine.game_data(&db, &plugin, &config, Lang::En, &hades, false, 60_000);
        assert!(soon.error.is_some());
        assert_eq!(transport.urls().len(), 1, "not asked again within the retry window");
        assert!(engine
            .badges(&lock(&db), std::slice::from_ref(&plugin), &config, Lang::En)
            .is_empty());

        // The user asks again once back online
        online.store(true, std::sync::atomic::Ordering::SeqCst);
        let good = engine.game_data(&db, &plugin, &config, Lang::En, &hades, true, 120_000);
        assert!(good.error.is_none() && good.data.is_some());

        // Later the service is down again: the old answer stays on screen, with the error
        online.store(false, std::sync::atomic::Ordering::SeqCst);
        let stale = engine.game_data(&db, &plugin, &config, Lang::En, &hades, true, 500_000);
        assert_eq!(stale.data, good.data);
        assert_eq!(stale.fetched_at, 120_000);
        assert!(stale.error.is_some());
    }

    #[test]
    fn commands_run_over_the_visible_library_and_point_only_at_real_games() {
        let db = Mutex::new(LibraryDb::open_in_memory().unwrap());
        let engine = PluginEngine::with_transport(FakeTransport::offline(), "0.1.0");
        let plugin = test_plugin(DETAILS);
        let config = enabled(&[]);

        let mut played = game("epic:a", "A");
        played.playtime_minutes = Some(30);
        let mut hidden = game("epic:hidden", "Hidden");
        hidden.is_hidden = true;
        let fresh = game("epic:b", "B");
        let games = [played, hidden, fresh];

        let view = engine
            .run_command(&db, &plugin, &config, Lang::En, "pick", &games, 1)
            .expect("runs");
        assert_eq!(view.title.as_deref(), Some("Pick"));
        assert_eq!(view.games.len(), 1, "the made-up id is dropped: {:?}", view.games);
        assert_eq!(view.games[0].id, "epic:b");
        assert_eq!(view.games[0].note.as_deref(), Some("Never played"));

        assert!(engine.run_command(&db, &plugin, &config, Lang::En, "undeclared", &games, 1).is_err());
    }

    #[test]
    fn enablement_and_settings_follow_the_user_then_the_plugin_defaults() {
        let mut plugin = test_plugin(DETAILS);
        let untouched = AppConfig::default();
        assert!(!is_enabled(&plugin, &untouched), "a user plugin is off until switched on");
        assert!(is_enabled(&plugin, &enabled(&[])));

        // A shipped plugin may be on by default; the user's choice always wins
        plugin.source = PluginSource::Bundled;
        plugin.manifest.enabled_by_default = true;
        assert!(is_enabled(&plugin, &untouched));
        let mut off = enabled(&[]);
        off.plugins.get_mut("demo").unwrap().enabled = false;
        assert!(!is_enabled(&plugin, &off));

        let settings = effective_settings(&plugin, &enabled(&[("apiKey", "k"), ("region", "mars"), ("other", "x")]));
        assert_eq!(
            settings,
            BTreeMap::from([("apiKey".to_string(), "k".to_string()), ("region".to_string(), "eu".to_string())]),
            "unknown choices fall back to the default, undeclared keys are dropped"
        );

        let info = describe(&plugin, &untouched, Lang::It);
        assert_eq!(info.name, "Dimostrazione");
        assert!(info.bundled && info.enabled && info.show_badge);
        assert_eq!(info.hosts, ["api.example.org"]);
        assert_eq!(info.settings[0].kind, "secret");
        assert_eq!(info.settings[1].options[1].label, "us");
        assert_eq!(info.commands[0].title, "Pick");
        assert!(installed_state(None).enabled);
    }
}
