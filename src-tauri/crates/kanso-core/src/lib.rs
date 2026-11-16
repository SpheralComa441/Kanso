pub mod accounts;
pub mod achievements;
pub mod battlenet_account;
pub mod cleaner;
pub mod config;
pub mod db;
pub mod deals;
pub mod emulators;
pub mod ea_account;
pub mod epic_account;
pub mod gamepass;
pub mod gog_account;
pub mod i18n;
pub mod metadata;
pub mod models;
pub mod owned;
pub mod playtime;
pub mod plugins;
pub mod scanner;
pub mod secrets;
pub mod signin;
pub mod store;
#[cfg(test)]
mod test_http;
pub mod theme;
pub mod ubisoft_account;
pub mod xbox_account;
pub mod vdf;

pub use achievements::{Achievement, GameAchievements};
pub use cleaner::{clean_game_title, is_placeholder_title};
pub use config::{AppConfig, PluginState};
pub use db::LibraryDb;
pub use deals::{Deal, DealStore, DealsClient, DealsReport};
pub use i18n::{current_language, set_language, Lang, T};
pub use metadata::{
    is_absent_cover_due, is_metadata_sync_due, needs_metadata, EnrichOutcome, MetadataClient, SgdbResolution,
    SteamAppSummary, SteamStoreLookup, SteamStoreMetadata, METADATA_RETRY_INTERVAL_MS,
};
pub use models::{
    now_timestamp_ms, DetectedGame, GameManualUpdate, GamePlatform, GameStatus, PlaySession,
    ScanReport, SgdbGameMatch, SourceScanOutput,
};
pub use scanner::{find_emulators, scan_all_installed_games, scan_library};
pub use store::LibraryStore;
pub use theme::Theme;
