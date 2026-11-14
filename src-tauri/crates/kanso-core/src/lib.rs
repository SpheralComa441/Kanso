pub mod cleaner;
pub mod config;
pub mod models;
pub mod scanner;
pub mod vdf;

pub use cleaner::clean_game_title;
pub use config::AppConfig;
pub use models::{DetectedGame, GamePlatform, ScanReport, SourceScanOutput};
pub use scanner::scan_all_installed_games;
