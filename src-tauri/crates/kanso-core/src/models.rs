use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

/// Supported game source platforms.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GamePlatform {
    #[serde(rename = "steam", alias = "Steam")]
    Steam,
    #[serde(rename = "epic", alias = "Epic")]
    Epic,
    #[serde(rename = "gog", alias = "GOG")]
    Gog,
    #[serde(rename = "ea", alias = "EA")]
    Ea,
    #[serde(rename = "ubisoft", alias = "Ubisoft")]
    Ubisoft,
    #[serde(rename = "xbox", alias = "Xbox")]
    Xbox,
    #[serde(
        rename = "Battle.net",
        alias = "battlenet",
        alias = "battle.net",
        alias = "BattleNet"
    )]
    BattleNet,
    #[serde(rename = "standalone", alias = "Standalone", alias = "locale")]
    Standalone,
    /// A game file (a ROM, a disc image) that one of the user's emulators runs. The emulator
    /// is named by `platform_app_id` (see [`crate::emulators`]).
    #[serde(rename = "emulator", alias = "Emulator")]
    Emulator,
}

impl GamePlatform {
    pub fn as_str(&self) -> &'static str {
        match self {
            GamePlatform::Steam => "steam",
            GamePlatform::Epic => "epic",
            GamePlatform::Gog => "gog",
            GamePlatform::Ea => "ea",
            GamePlatform::Ubisoft => "ubisoft",
            GamePlatform::Xbox => "xbox",
            GamePlatform::BattleNet => "Battle.net",
            GamePlatform::Standalone => "standalone",
            GamePlatform::Emulator => "emulator",
        }
    }

    /// Inverse of [`Self::as_str`], used when reading the platform column back from the database.
    pub fn parse(raw: &str) -> Option<Self> {
        [
            GamePlatform::Steam,
            GamePlatform::Epic,
            GamePlatform::Gog,
            GamePlatform::Ea,
            GamePlatform::Ubisoft,
            GamePlatform::Xbox,
            GamePlatform::BattleNet,
            GamePlatform::Standalone,
            GamePlatform::Emulator,
        ]
        .into_iter()
        .find(|platform| platform.as_str().eq_ignore_ascii_case(raw.trim()))
    }
}

/// Returns current Unix epoch timestamp in milliseconds.
pub fn now_timestamp_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Where the user stands with a game. Set by hand, never guessed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GameStatus {
    /// Owned but not started yet.
    Backlog,
    Playing,
    Finished,
    Abandoned,
}

impl GameStatus {
    pub const ALL: [GameStatus; 4] = [
        GameStatus::Backlog,
        GameStatus::Playing,
        GameStatus::Finished,
        GameStatus::Abandoned,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            GameStatus::Backlog => "backlog",
            GameStatus::Playing => "playing",
            GameStatus::Finished => "finished",
            GameStatus::Abandoned => "abandoned",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|status| status.as_str().eq_ignore_ascii_case(raw.trim()))
    }
}

/// A stretch of play time, the unit of the statistics page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaySession {
    pub game_id: String,
    /// When the session began (ms).
    pub started_at: u64,
    pub seconds: u64,
    /// `kanso` when Kanso measured it, `steam` when it is the difference between two readings
    /// of Steam's own counter.
    pub source: String,
}

fn default_true() -> bool {
    true
}

/// Unified representation of an installed game discovered on the system,
/// enriched with cached cover art, metadata, and optional user overrides.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectedGame {
    /// Deterministic identifier (e.g. `steam:1091500`, `epic:...`, `battlenet:...`, `standalone:...`).
    pub id: String,
    /// Display title (either detected or customized by the user).
    pub title: String,
    /// Normalized title stripped of edition suffixes and trademark symbols for metadata APIs.
    pub clean_title: String,
    /// Platform source (`steam`, `epic`, `gog`, `ea`, `ubisoft`, `xbox`, `Battle.net`,
    /// `standalone`, or `emulator`).
    pub platform: GamePlatform,
    /// Platform-specific identifier (`appid` for Steam, `AppName` for Epic, registry key for Battle.net).
    pub platform_app_id: Option<String>,
    /// Root installation directory on disk.
    pub install_dir: String,
    /// Direct path to the game executable when known or overridden by the user.
    pub executable_path: Option<String>,
    /// Protocol URI used to launch the game through its parent client, if available.
    pub launch_uri: Option<String>,

    /// Matched or manually chosen SteamGridDB game ID.
    #[serde(default)]
    pub steamgriddb_id: Option<u64>,
    /// Local path inside `AppData/covers/` to the vertical cover image.
    #[serde(default)]
    pub cover_path: Option<String>,
    /// Short game description (fetched in Italian from Steam Store API or edited manually).
    #[serde(default)]
    pub short_description: Option<String>,
    /// Genre tags (e.g. `["Azione", "GDR"]`).
    #[serde(default)]
    pub genres: Vec<String>,
    /// Whether the game is marked as a favorite by the user.
    #[serde(default)]
    pub is_favorite: bool,
    /// Whether automatic metadata & cover lookup has already been attempted.
    #[serde(default)]
    pub metadata_synced: bool,
    /// Whether the user manually customized fields or selected a specific SteamGridDB entry.
    #[serde(default)]
    pub custom_override: bool,
    /// `true` while `title` is only a local best guess (the Steam manifest carried an
    /// `appid_<id>` placeholder instead of the game's name). Metadata sync replaces it with
    /// the official name and clears the flag.
    #[serde(default)]
    pub title_provisional: bool,
    /// Store classification of a Steam app (`game`, `music`, `dlc`, `application`, ...), or
    /// `unlisted` when the store has no page for it. `None` until first looked up.
    #[serde(default)]
    pub app_type: Option<String>,
    /// Hidden from every library view except "Nascosti" (soundtracks, tools, unwanted entries).
    #[serde(default)]
    pub is_hidden: bool,
    /// When automatic metadata lookup was last attempted (ms); `0` = never. Drives retry back-off.
    #[serde(default)]
    pub metadata_attempted_at: u64,
    /// When the game was last played (ms): the latest between a launch from Kanso and what
    /// the game's own client recorded (Steam).
    #[serde(default)]
    pub last_played_at: Option<u64>,
    /// How many times the game was launched from Kanso.
    #[serde(default)]
    pub play_count: u32,
    /// Language (`it`, `en`, ...) the cached description and genres were fetched in.
    #[serde(default)]
    pub metadata_lang: Option<String>,
    /// Steam community tags ("Souls-like", "Co-op online", ...), most relevant first.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Language the tags were fetched in; `None` until a lookup got an answer (even an empty one).
    #[serde(default)]
    pub tags_lang: Option<String>,
    /// Total playtime in minutes: what the game's own client recorded (Steam) or what Kanso
    /// measured while the game was running (every other store); `None` when unknown.
    #[serde(default)]
    pub playtime_minutes: Option<u32>,
    /// Backlog, playing, finished or abandoned, as chosen by the user.
    #[serde(default)]
    pub status: Option<GameStatus>,
    /// Achievements unlocked / available according to the game's client (Steam); `None` when
    /// unknown or the game has none.
    #[serde(default)]
    pub achievements_unlocked: Option<u32>,
    #[serde(default)]
    pub achievements_total: Option<u32>,
    /// Steam store page matched to a game from another store: `None` until looked up, empty
    /// when there is none. Read it through [`DetectedGame::steam_store_id`].
    #[serde(default)]
    pub steam_store_appid: Option<String>,
    /// `false` for a game that is in the library but not on disk: uninstalled since Kanso
    /// first saw it, or known to be owned without ever having been installed here.
    #[serde(default = "default_true")]
    pub is_installed: bool,
    /// Genres the user gave the game, besides the ones its store page lists. They are the
    /// user's: a sync that fetches the store's genres again leaves them alone.
    #[serde(default)]
    pub custom_genres: Vec<String>,
    /// The version installed, as the game's client or its files say it: a build number, a
    /// version string. `None` when nothing on disk says.
    #[serde(default)]
    pub version: Option<String>,
    /// A Steam game on this PC that belongs to someone else's account: played through
    /// family sharing.
    #[serde(default)]
    pub is_family_shared: bool,
    /// Last metadata/cover update timestamp in ms (also used for UI image cache busting).
    #[serde(default)]
    pub updated_at: u64,
}

impl DetectedGame {
    /// Constructs a freshly scanned game entry prior to metadata enrichment.
    #[allow(clippy::too_many_arguments)]
    pub fn new_detected(
        id: String,
        title: String,
        clean_title: String,
        platform: GamePlatform,
        platform_app_id: Option<String>,
        install_dir: String,
        executable_path: Option<String>,
        launch_uri: Option<String>,
    ) -> Self {
        Self {
            id,
            title,
            clean_title,
            platform,
            platform_app_id,
            install_dir,
            executable_path,
            launch_uri,
            steamgriddb_id: None,
            cover_path: None,
            short_description: None,
            genres: Vec::new(),
            is_favorite: false,
            metadata_synced: false,
            custom_override: false,
            title_provisional: false,
            app_type: None,
            is_hidden: false,
            metadata_attempted_at: 0,
            last_played_at: None,
            play_count: 0,
            metadata_lang: None,
            tags: Vec::new(),
            tags_lang: None,
            playtime_minutes: None,
            status: None,
            achievements_unlocked: None,
            achievements_total: None,
            steam_store_appid: None,
            is_installed: true,
            custom_genres: Vec::new(),
            version: None,
            is_family_shared: false,
            updated_at: now_timestamp_ms(),
        }
    }

    /// Address that asks the game's own client to uninstall it, for the stores that have
    /// one. Like the others, built from what is stored for the game, never from the
    /// interface. The client asks for confirmation itself: Kanso deletes nothing.
    pub fn uninstall_uri(&self) -> Option<String> {
        if !self.is_installed {
            return None;
        }
        let app_id = self
            .platform_app_id
            .as_deref()
            .map(str::trim)
            .filter(|id| !id.is_empty())?;
        match self.platform {
            GamePlatform::Steam => app_id
                .bytes()
                .all(|b| b.is_ascii_digit())
                .then(|| format!("steam://uninstall/{app_id}")),
            _ => None,
        }
    }

    /// Address that asks the game's own client to install it, for the stores that have one.
    /// Built from what is stored for the game, never from the interface.
    pub fn install_uri(&self) -> Option<String> {
        let app_id = self
            .platform_app_id
            .as_deref()
            .map(str::trim)
            .filter(|id| !id.is_empty());
        match self.platform {
            GamePlatform::Steam => app_id
                .filter(|id| id.bytes().all(|b| b.is_ascii_digit()))
                .map(|id| format!("steam://install/{id}")),
            // Same address as launching, with another action
            GamePlatform::Epic => self
                .launch_uri
                .as_deref()
                .and_then(|uri| uri.split('?').next())
                .filter(|base| base.starts_with("com.epicgames.launcher://apps/"))
                .map(|base| format!("{base}?action=install")),
            GamePlatform::Gog => app_id
                .filter(|id| id.bytes().all(|b| b.is_ascii_digit()))
                .map(|id| format!("goggalaxy://openGameView/{id}")),
            // Ubisoft Connect opens on the game, where it offers the download
            GamePlatform::Ubisoft => self
                .launch_uri
                .clone()
                .filter(|uri| uri.starts_with("uplay://")),
            // The Battle.net app opens on the game, where it offers the download. Only a
            // game listed by the account knows the code the app calls it by
            GamePlatform::BattleNet => self
                .launch_uri
                .clone()
                .filter(|uri| uri.starts_with("battlenet://")),
            // The Xbox app, or the Microsoft Store, opens on the game's page, where it
            // offers the download: the address an account or the Game Pass catalog gave
            GamePlatform::Xbox => self
                .launch_uri
                .clone()
                .filter(|uri| uri.starts_with("msxbox://game/?productId=") || uri.starts_with("ms-windows-store://pdp/?PFN=")),
            GamePlatform::Ea | GamePlatform::Standalone | GamePlatform::Emulator => None,
        }
    }

    /// Appid of the game's Steam store page: its own for a Steam game, the matched one for a
    /// game from another store, `None` when there is none or it is not known yet.
    pub fn steam_store_id(&self) -> Option<&str> {
        let appid = if self.platform == GamePlatform::Steam {
            self.platform_app_id.as_deref()
        } else {
            self.steam_store_appid.as_deref()
        };
        appid
            .map(str::trim)
            .filter(|id| !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit()))
    }
}

/// Candidate game returned by SteamGridDB (`/games/steam/{appid}` or `/search/autocomplete/{term}`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SgdbGameMatch {
    pub id: u64,
    pub name: String,
    pub release_year: Option<i32>,
    pub verified: bool,
    pub types: Vec<String>,
}

/// Payload used when manually editing a single game's fields in the UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameManualUpdate {
    pub id: String,
    pub title: String,
    pub executable_path: Option<String>,
    pub short_description: Option<String>,
    pub genres: Vec<String>,
    pub is_favorite: Option<bool>,
}

/// Aggregated result of a full library scan across every supported store and the custom folders.
/// Per-platform counts are derived from `games` by whoever needs them.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanReport {
    pub games: Vec<DetectedGame>,
    pub warnings: Vec<String>,
}

/// Output from a single platform scanner.
#[derive(Debug, Clone, Default)]
pub struct SourceScanOutput {
    pub games: Vec<DetectedGame>,
    pub warnings: Vec<String>,
}
