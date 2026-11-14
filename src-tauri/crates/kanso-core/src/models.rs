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
    /// Platform source (`steam`, `epic`, `gog`, `ea`, `ubisoft`, `xbox`, `Battle.net`, or `standalone`).
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
    /// When the game was last launched from Kanso (ms).
    #[serde(default)]
    pub last_played_at: Option<u64>,
    /// How many times the game was launched from Kanso.
    #[serde(default)]
    pub play_count: u32,
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
            updated_at: now_timestamp_ms(),
        }
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
