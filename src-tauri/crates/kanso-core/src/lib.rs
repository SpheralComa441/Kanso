pub mod cleaner;
pub mod config;
pub mod db;
pub mod metadata;
pub mod models;
pub mod scanner;
pub mod store;
pub mod vdf;

pub use cleaner::{clean_game_title, is_placeholder_title};
pub use config::AppConfig;
pub use db::LibraryDb;
pub use metadata::{
    is_metadata_sync_due, needs_metadata, EnrichOutcome, MetadataClient, SgdbResolution,
    SteamStoreLookup, SteamStoreMetadata, METADATA_RETRY_INTERVAL_MS,
};
pub use models::{
    now_timestamp_ms, DetectedGame, GameManualUpdate, GamePlatform, ScanReport, SgdbGameMatch,
    SourceScanOutput,
};
pub use scanner::scan_all_installed_games;
pub use store::LibraryStore;
