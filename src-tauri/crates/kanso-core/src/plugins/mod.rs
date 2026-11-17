//! Community plugins: small JavaScript programs that add information to a game's sheet, a part
//! to the Home page or commands to the Plugins menu, without ever touching the interface or
//! the user's data.
//!
//! - [`manifest`]: `plugin.json`, including the hosts a plugin may contact;
//! - [`registry`]: the shipped plugins and the user's plugins folder;
//! - [`runtime`]: the sandbox the code runs in;
//! - [`http`]: the checked door to the network;
//! - [`output`]: the plain data a plugin may return, which Kanso draws itself.
//!
//! [`PluginEngine`] ties them to the library: it runs a plugin for a game or for the Home
//! page, caches the answer in the database and hands the interface what to show.

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
use output::{parse_game_data, parse_home_view, parse_view, PluginGameData, PluginView};
use registry::{Plugin, PluginSource};
use runtime::{run_hook, HookCall, HookOutput, RunLimits};
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashSet};
use std::sync::{Arc, Mutex, MutexGuard};

/// How long a failed answer is remembered before the plugin is asked again.
const ERROR_RETRY_MS: u64 = 30 * 60 * 1000;
/// Log lines passed on with an error, for the plugin's author.
const ERROR_LOG_LINES: usize = 12;
/// The longest an answer of `home` is reused, whatever the manifest says: the Home page is
/// about today.
pub const HOME_MAX_CACHE_HOURS: u64 = 24;
/// Where a plugin's answer for the Home page is kept, in the same cache as its answers per
/// game. Every game id starts with its platform and a colon, so this is never one of them.
const HOME_CACHE_KEY: &str = "@home";

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
    /// Adds a part to the Home page.
    pub home: bool,
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
        home: manifest.home,
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

/// What commands and `home` are called with: the visible library, and the time.
fn library_context(visible: &[&DetectedGame], now_ms: u64) -> Value {
    json!({
        "games": visible.iter().map(|game| game_payload(game)).collect::<Vec<_>>(),
        "now": now_ms,
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

/// What a plugin adds to the Home page.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HomeDataAnswer {
    pub plugin_id: String,
    pub plugin_name: String,
    /// `None` when the plugin failed and nothing older is worth showing. An empty view is
    /// the plugin saying it has nothing for today.
    pub data: Option<PluginView>,
    /// Why the last attempt failed, with the plugin's last log lines for its author.
    pub error: Option<String>,
    pub logs: Vec<String>,
    /// When `data` was obtained (ms).
    pub fetched_at: u64,
}

/// One call of a hook whose answer is kept in the plugin's cache.
struct CachedCall<'a> {
    /// What the answer is filed under: a game id, or [`HOME_CACHE_KEY`].
    key: &'a str,
    hook: &'a str,
    /// The longest the answer is reused; the answer itself may ask for less.
    max_hours: u64,
    /// Asks again even if the cached answer is still fresh.
    force: bool,
    now_ms: u64,
}

/// What a cached call gave: the answer, or the last good one together with the error.
struct CachedAnswer<D> {
    data: Option<D>,
    error: Option<String>,
    logs: Vec<String>,
    fetched_at: u64,
}

/// A plugin's label for a game card.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BadgeEntry {
    pub plugin_id: String,
    pub plugin_name: String,
    pub text: String,
    /// What the figure is, as the plugin says; `None` when it does not.
    pub label: Option<String>,
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

    /// Calls a hook through the plugin's cache: the stored answer while it is fresh, otherwise
    /// a new one (always with `force`). An answer is stale as soon as the plugin's code,
    /// version, settings or the interface language change. When the call fails, the last good
    /// answer is handed back together with the error; with nothing to fall back on, the error
    /// itself is remembered for a while so that the plugin is not asked at every look.
    ///
    /// `args` is built only if the plugin really runs. The database is locked only to read
    /// and write the cache, never while the plugin runs.
    #[allow(clippy::too_many_arguments)]
    fn cached<D: Serialize + DeserializeOwned>(
        &self,
        db: &Mutex<LibraryDb>,
        plugin: &Plugin,
        config: &AppConfig,
        lang: Lang,
        call: &CachedCall<'_>,
        args: impl FnOnce() -> Value,
        parse: impl Fn(&Value) -> D,
    ) -> CachedAnswer<D> {
        let settings = effective_settings(plugin, config);
        let signature = Self::signature(plugin, &settings, lang);
        let plugin_id = plugin.manifest.id.as_str();
        let now_ms = call.now_ms;

        let (cached, storage) = {
            let db = lock(db);
            (
                db.get_plugin_cache(plugin_id, call.key).ok().flatten(),
                db.load_plugin_storage(plugin_id).unwrap_or_default(),
            )
        };
        let cached = cached.filter(|entry| entry.signature == signature);
        let cached_data = |entry: &PluginCacheEntry| {
            entry
                .ok
                .then(|| serde_json::from_str::<D>(&entry.data).ok())
                .flatten()
        };
        if let Some(entry) = cached.as_ref().filter(|entry| !call.force && entry.expires_at > now_ms) {
            return match cached_data(entry) {
                Some(data) => CachedAnswer { data: Some(data), error: None, logs: Vec::new(), fetched_at: entry.fetched_at },
                None => CachedAnswer {
                    data: None,
                    error: Some(entry.data.clone()),
                    logs: Vec::new(),
                    fetched_at: entry.fetched_at,
                },
            };
        }

        match self.call(plugin, &settings, &storage, lang, call.hook, &args()) {
            Ok(output) => {
                let data = parse(&output.value);
                // An answer may ask to be kept for less than the usual time (data not out
                // yet, a service that could not be reached), never for longer
                let cache_hours = output
                    .value
                    .get("cacheHours")
                    .and_then(Value::as_f64)
                    .filter(|hours| hours.is_finite() && *hours > 0.0)
                    .map_or(call.max_hours, |hours| (hours.ceil() as u64).clamp(1, call.max_hours));
                let entry = PluginCacheEntry {
                    data: serde_json::to_string(&data).unwrap_or_else(|_| "{}".to_string()),
                    ok: true,
                    signature,
                    fetched_at: now_ms,
                    expires_at: now_ms + cache_hours * 3_600_000,
                };
                let mut db = lock(db);
                let _ = db.put_plugin_cache(plugin_id, call.key, &entry);
                if let Some(storage) = &output.storage {
                    let _ = db.save_plugin_storage(plugin_id, storage);
                }
                CachedAnswer { data: Some(data), error: None, logs: Vec::new(), fetched_at: now_ms }
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
                match previous {
                    Some((data, fetched_at)) => {
                        CachedAnswer { data: Some(data), error: Some(message), logs, fetched_at }
                    }
                    None => {
                        let entry = PluginCacheEntry {
                            data: message.clone(),
                            ok: false,
                            signature,
                            fetched_at: now_ms,
                            expires_at: now_ms + ERROR_RETRY_MS,
                        };
                        let _ = lock(db).put_plugin_cache(plugin_id, call.key, &entry);
                        CachedAnswer { data: None, error: Some(message), logs, fetched_at: now_ms }
                    }
                }
            }
        }
    }

    /// What `plugin` shows for `game`: the cached answer while it is fresh (the manifest's
    /// `cacheHours`), otherwise a new one (always with `force`).
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
        let call = CachedCall {
            key: &game.id,
            hook: "gameDetails",
            max_hours: plugin.manifest.cache_hours,
            force,
            now_ms,
        };
        let answer = self.cached(db, plugin, config, lang, &call, || game_payload(game), parse_game_data);
        GameDataAnswer {
            plugin_id: plugin.manifest.id.clone(),
            plugin_name: plugin.manifest.name.get(lang).to_string(),
            data: answer.data,
            error: answer.error,
            logs: answer.logs,
            fetched_at: answer.fetched_at,
        }
    }

    /// What `plugin` adds to the Home page, from the visible library. Cached like
    /// [`Self::game_data`], but for a day at most ([`HOME_MAX_CACHE_HOURS`]) whatever the
    /// manifest says. The games it points at are checked against the library at every look:
    /// one that was hidden or removed since the answer was cached is not shown.
    #[allow(clippy::too_many_arguments)]
    pub fn home_data(
        &self,
        db: &Mutex<LibraryDb>,
        plugin: &Plugin,
        config: &AppConfig,
        lang: Lang,
        games: &[DetectedGame],
        force: bool,
        now_ms: u64,
    ) -> HomeDataAnswer {
        let visible: Vec<&DetectedGame> = games.iter().filter(|game| !game.is_hidden).collect();
        let call = CachedCall {
            key: HOME_CACHE_KEY,
            hook: "home",
            max_hours: plugin.manifest.cache_hours.min(HOME_MAX_CACHE_HOURS),
            force,
            now_ms,
        };
        let answer = self.cached(
            db,
            plugin,
            config,
            lang,
            &call,
            || library_context(&visible, now_ms),
            parse_home_view,
        );
        // Only games that are really in the library can be pointed at
        let known: HashSet<&str> = visible.iter().map(|game| game.id.as_str()).collect();
        HomeDataAnswer {
            plugin_id: plugin.manifest.id.clone(),
            plugin_name: plugin.manifest.name.get(lang).to_string(),
            data: answer.data.map(|mut view| {
                view.games.retain(|pick| known.contains(pick.id.as_str()));
                view
            }),
            error: answer.error,
            logs: answer.logs,
            fetched_at: answer.fetched_at,
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
                // The answer for the Home page shares the cache and is not about a game
                if !entry.ok || entry.signature != signature || game_id == HOME_CACHE_KEY {
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
                        label: badge.label,
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
        let args = library_context(&visible, now_ms);

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

    /// A plugin that adds a part to Home and says how often it really ran.
    const HOME: &str = r#"
        kanso.plugin({
          home(context) {
            const runs = kanso.storage.get("runs", 0) + 1;
            kanso.storage.set("runs", runs);
            return {
              title: "Run " + runs + " at " + context.now + " (" + kanso.settings.region + ")",
              sections: [1, 2, 3, 4].map((n) => ({ title: "Part " + n, blocks: [{ type: "text", text: "Words." }] })),
              gamesStyle: "cards",
              games: context.games.map((game) => ({ id: game.id, note: game.title })).concat([{ id: "ghost:1" }]),
            };
          },
        });
    "#;

    fn home_plugin(code: &str, cache_hours: u64) -> Plugin {
        Plugin {
            manifest: manifest::PluginManifest::parse(&format!(
                r#"{{
                  "kansoPlugin": 1, "id": "demo", "name": {{"en": "Demo", "it": "Dimostrazione"}},
                  "version": "1.0.0", "hosts": ["api.example.org"], "cacheHours": {cache_hours},
                  "provides": {{"home": true}},
                  "settings": [{{"key": "region", "type": "select", "default": "eu", "options": ["eu", "us"]}}]
                }}"#
            ))
            .unwrap(),
            code: code.to_string(),
            source: PluginSource::User,
        }
    }

    fn runs(db: &Mutex<LibraryDb>) -> Option<String> {
        lock(db).load_plugin_storage("demo").unwrap().get("runs").cloned()
    }

    #[test]
    fn a_part_of_home_is_built_from_the_visible_library_and_cut_to_size() {
        let db = Mutex::new(LibraryDb::open_in_memory().unwrap());
        let engine = PluginEngine::with_transport(FakeTransport::offline(), "0.1.0");
        let plugin = home_plugin(HOME, 2);
        let config = enabled(&[]);

        let mut hidden = game("epic:hidden", "Hidden");
        hidden.is_hidden = true;
        let mut games: Vec<DetectedGame> = (0..20).map(|n| game(&format!("epic:{n:02}"), &format!("Game {n}"))).collect();
        games.insert(3, hidden);

        let answer = engine.home_data(&db, &plugin, &config, Lang::It, &games, false, 5_000);
        assert_eq!((answer.plugin_id.as_str(), answer.plugin_name.as_str()), ("demo", "Dimostrazione"));
        assert_eq!(answer.error, None, "{:?}", answer.logs);
        assert_eq!(answer.fetched_at, 5_000);
        let view = answer.data.expect("a view");
        assert_eq!(view.title.as_deref(), Some("Run 1 at 5000 (eu)"), "the time and the settings reach the hook");
        assert_eq!(view.sections.len(), 2, "two sections at most on Home");
        assert_eq!(view.games_style, output::GamesStyle::Cards);
        assert_eq!(view.games.len(), 12, "twelve games at most on Home");
        assert!(view.games.iter().all(|pick| pick.id != "epic:hidden"), "hidden games are not handed over");
        assert_eq!(view.games[3].id, "epic:03");
        assert_eq!(view.games[3].note.as_deref(), Some("Game 3"));

        // A game hidden (or gone) after the answer was cached is not pointed at any more,
        // without running the plugin again
        games.retain(|game| game.id != "epic:00");
        games[0].is_hidden = true; // epic:01
        let later = engine.home_data(&db, &plugin, &config, Lang::It, &games, false, 6_000);
        let view = later.data.expect("a view");
        assert_eq!(view.title.as_deref(), Some("Run 1 at 5000 (eu)"), "from the cache");
        assert_eq!(view.games.len(), 10);
        assert_eq!(view.games[0].id, "epic:02");
        assert_eq!(runs(&db).as_deref(), Some("1"));

        // What is kept for Home is not about a game: the cards get no label from it, and
        // the answers per game do not see it
        assert!(engine.badges(&lock(&db), std::slice::from_ref(&plugin), &config, Lang::It).is_empty());
        assert_eq!(lock(&db).list_plugin_cache("demo").unwrap().len(), 1);
        assert!(lock(&db).get_plugin_cache("demo", HOME_CACHE_KEY).unwrap().is_some());

        // Nothing to show today: an empty view, cached like any other answer
        let quiet = home_plugin("kanso.plugin({ home() { kanso.storage.set('asked', (kanso.storage.get('asked') || 0) + 1); return null; } });", 2);
        let nothing = engine.home_data(&db, &quiet, &config, Lang::It, &games, false, 7_000);
        assert_eq!(nothing.error, None);
        assert!(nothing.data.expect("a view").is_empty());
        engine.home_data(&db, &quiet, &config, Lang::It, &games, false, 8_000);
        assert_eq!(lock(&db).load_plugin_storage("demo").unwrap().get("asked").map(String::as_str), Some("1"));
    }

    #[test]
    fn a_part_of_home_is_cached_for_a_day_at_most_and_asked_again_like_a_game_sheet() {
        let db = Mutex::new(LibraryDb::open_in_memory().unwrap());
        let engine = PluginEngine::with_transport(FakeTransport::offline(), "0.1.0");
        let games = [game("epic:a", "A")];
        let config = enabled(&[]);
        let hour = 3_600_000;
        let title = |answer: HomeDataAnswer| answer.data.and_then(|view| view.title).unwrap_or_default();

        // The manifest says a week: on Home that is a day
        let plugin = home_plugin(HOME, 168);
        let first = engine.home_data(&db, &plugin, &config, Lang::En, &games, false, 0);
        assert_eq!(title(first), "Run 1 at 0 (eu)");
        assert_eq!(lock(&db).get_plugin_cache("demo", HOME_CACHE_KEY).unwrap().unwrap().expires_at, HOME_MAX_CACHE_HOURS * hour);
        let same_day = engine.home_data(&db, &plugin, &config, Lang::En, &games, false, 23 * hour);
        assert_eq!(same_day.fetched_at, 0);
        assert_eq!(title(same_day), "Run 1 at 0 (eu)", "reused within the day");
        let next_day = engine.home_data(&db, &plugin, &config, Lang::En, &games, false, 24 * hour);
        assert_eq!(title(next_day), format!("Run 2 at {} (eu)", 24 * hour), "asked again after a day");

        // Asked again at once: when the user refreshes, and whenever the settings, the
        // language, the plugin's code or its version change
        let now = 25 * hour;
        let ask = |plugin: &Plugin, config: &AppConfig, lang: Lang, force: bool| {
            title(engine.home_data(&db, plugin, config, lang, &games, force, now))
        };
        assert!(ask(&plugin, &config, Lang::En, false).starts_with("Run 2 "), "still fresh");
        assert!(ask(&plugin, &config, Lang::En, true).starts_with("Run 3 "), "forced from the interface");
        let us = enabled(&[("region", "us")]);
        assert_eq!(ask(&plugin, &us, Lang::En, false), format!("Run 4 at {now} (us)"));
        assert!(ask(&plugin, &us, Lang::It, false).starts_with("Run 5 "), "another language");
        let edited = home_plugin(&format!("{HOME}\n// edited"), 168);
        assert!(ask(&edited, &us, Lang::It, false).starts_with("Run 6 "), "another main.js");
        let mut updated = edited.clone();
        updated.manifest.version = "1.0.1".to_string();
        assert!(ask(&updated, &us, Lang::It, false).starts_with("Run 7 "), "another version");
        assert!(ask(&updated, &us, Lang::It, false).starts_with("Run 7 "), "and then it is cached again");

        // A shorter time in the manifest stands; an answer can shorten its own, never go
        // past the manifest's or past the day
        let expiry = |cache_hours: u64, reply: &str| {
            let db = Mutex::new(LibraryDb::open_in_memory().unwrap());
            let plugin = home_plugin(&format!("kanso.plugin({{ home: () => ({reply}) }});"), cache_hours);
            engine.home_data(&db, &plugin, &config, Lang::En, &games, false, 0);
            let expires_at = lock(&db).get_plugin_cache("demo", HOME_CACHE_KEY).unwrap().unwrap().expires_at;
            expires_at
        };
        assert_eq!(expiry(2, "{ sections: [] }"), 2 * hour);
        assert_eq!(expiry(2160, "{ sections: [] }"), 24 * hour);
        assert_eq!(expiry(2160, "null"), 24 * hour, "nothing to show is cached like an answer");
        assert_eq!(expiry(168, "{ cacheHours: 3 }"), 3 * hour);
        assert_eq!(expiry(168, "{ cacheHours: 0.5 }"), hour, "at least one hour");
        assert_eq!(expiry(168, "{ cacheHours: 500 }"), 24 * hour, "never past the day");
        assert_eq!(expiry(2, "{ cacheHours: 12 }"), 2 * hour, "never past the manifest's");
        assert_eq!(expiry(168, "{ cacheHours: -1 }"), 24 * hour);
        assert_eq!(expiry(168, "{ cacheHours: 'soon' }"), 24 * hour);
    }

    #[test]
    fn a_failing_part_of_home_keeps_the_last_good_answer_and_reports_the_details() {
        let db = Mutex::new(LibraryDb::open_in_memory().unwrap());
        let online = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let transport = {
            let online = Arc::clone(&online);
            FakeTransport::new(move |_| {
                if online.load(std::sync::atomic::Ordering::SeqCst) {
                    ok_json(json!({"id": "epic:hades"}))
                } else {
                    Err("dns error".to_string())
                }
            })
        };
        let engine = PluginEngine::with_transport(transport.clone(), "0.1.0");
        let plugin = home_plugin(
            r#"kanso.plugin({
                 home() {
                   kanso.log("asking the service");
                   const pick = kanso.http.getJson("https://api.example.org/today");
                   return { title: "Today", games: [{ id: pick.id }] };
                 },
               });"#,
            24,
        );
        let config = enabled(&[]);
        let games = [game("epic:hades", "Hades")];
        let minute = 60_000;

        // Offline with nothing cached: the error, with what the plugin logged and where it
        // happened, and no view
        let failed = engine.home_data(&db, &plugin, &config, Lang::En, &games, false, 0);
        assert_eq!(failed.data, None);
        assert!(failed.error.as_deref().unwrap().contains("dns error"), "{:?}", failed.error);
        assert!(failed.logs.iter().any(|line| line == "asking the service"), "{:?}", failed.logs);
        assert!(failed.logs.iter().any(|line| line.contains("main.js")), "{:?}", failed.logs);

        // Not asked again at every look at Home, only after a while or when the user says so
        let soon = engine.home_data(&db, &plugin, &config, Lang::En, &games, false, 10 * minute);
        assert!(soon.data.is_none() && soon.error.is_some());
        assert_eq!(transport.urls().len(), 1, "not asked again within the retry window");
        engine.home_data(&db, &plugin, &config, Lang::En, &games, false, 31 * minute);
        assert_eq!(transport.urls().len(), 2, "asked again once the window passed");

        online.store(true, std::sync::atomic::Ordering::SeqCst);
        let good = engine.home_data(&db, &plugin, &config, Lang::En, &games, true, 40 * minute);
        assert_eq!(good.error, None);
        let view = good.data.clone().expect("a view");
        assert_eq!(view.games[0].id, "epic:hades");

        // Later the service is down again: the old answer stays, marked with the error and
        // with the time it is from, whether it expired or the user asked again
        online.store(false, std::sync::atomic::Ordering::SeqCst);
        let day = 24 * 60 * minute;
        for (force, now) in [(true, 50 * minute), (false, 2 * day)] {
            let stale = engine.home_data(&db, &plugin, &config, Lang::En, &games, force, now);
            assert_eq!(stale.data, good.data);
            assert_eq!(stale.fetched_at, 40 * minute);
            assert!(stale.error.as_deref().unwrap().contains("dns error"));
            assert!(!stale.logs.is_empty());
        }

        // A plugin that says it adds a part of Home and has no such hook fails the same way
        let hookless = home_plugin("kanso.plugin({ gameDetails: () => null });", 24);
        let other_db = Mutex::new(LibraryDb::open_in_memory().unwrap());
        let missing = engine.home_data(&other_db, &hookless, &config, Lang::En, &games, false, 0);
        assert!(missing.data.is_none());
        assert!(missing.error.as_deref().unwrap().contains("does not implement home"), "{:?}", missing.error);
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
        assert!(info.game_details && !info.home, "what it adds is what its manifest says");
        assert!(describe(&home_plugin(HOME, 2), &untouched, Lang::It).home);
        assert!(installed_state(None).enabled);
    }
}
