pub mod cleaner;
pub mod config;
pub mod metadata;
pub mod models;
pub mod scanner;
pub mod store;
pub mod vdf;

pub use cleaner::{clean_game_title, is_placeholder_title};
pub use config::AppConfig;
pub use metadata::{MetadataClient, SgdbResolution, SteamStoreMetadata};
pub use models::{
    now_timestamp_ms, DetectedGame, GameManualUpdate, GamePlatform, ScanReport, SgdbGameMatch,
    SourceScanOutput,
};
pub use scanner::scan_all_installed_games;
pub use store::LibraryStore;
