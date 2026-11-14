use serde::{Deserialize, Serialize};

/// Supported game source platforms.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GamePlatform {
    Steam,
    Epic,
    Standalone,
}

impl GamePlatform {
    pub fn as_str(&self) -> &'static str {
        match self {
            GamePlatform::Steam => "steam",
            GamePlatform::Epic => "epic",
            GamePlatform::Standalone => "standalone",
        }
    }
}

/// Unified representation of an installed game discovered on the system.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectedGame {
    /// Deterministic identifier (e.g. `steam:1091500`, `epic:AlanWake2`, `standalone:a1b2c3d4`).
    pub id: String,
    /// Original display name as reported by the launcher manifest or folder/executable.
    pub title: String,
    /// Normalized title stripped of edition suffixes and trademark symbols for metadata APIs.
    pub clean_title: String,
    /// Platform source (`steam`, `epic`, or `standalone`).
    pub platform: GamePlatform,
    /// Platform-specific identifier (`appid` for Steam, `AppName` for Epic).
    pub platform_app_id: Option<String>,
    /// Root installation directory on disk.
    pub install_dir: String,
    /// Direct path to the game executable when known.
    pub executable_path: Option<String>,
    /// Protocol URI used to launch the game through its parent client, if available.
    pub launch_uri: Option<String>,
}

/// Aggregated result of a full library scan across Steam, Epic Games, and custom folders.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanReport {
    pub games: Vec<DetectedGame>,
    pub steam_count: usize,
    pub epic_count: usize,
    pub standalone_count: usize,
    pub warnings: Vec<String>,
}

/// Output from a single platform scanner.
#[derive(Debug, Clone, Default)]
pub struct SourceScanOutput {
    pub games: Vec<DetectedGame>,
    pub warnings: Vec<String>,
}
