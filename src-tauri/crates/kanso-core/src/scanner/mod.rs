pub mod epic;
pub mod standalone;
pub mod steam;

use crate::config::AppConfig;
use crate::models::{DetectedGame, ScanReport};

/// Runs the complete game auto-discovery pipeline on the host machine:
/// 1. Steam (Registry + `libraryfolders.vdf` + `appmanifest_*.acf`)
/// 2. Epic Games (`C:\ProgramData\Epic\EpicGamesLauncher\Data\Manifests\*.item`)
/// 3. User-configured Standalone / Custom folders (`*.exe` with exclusion rules)
pub fn scan_all_installed_games(config: &AppConfig) -> ScanReport {
    let mut games: Vec<DetectedGame> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();

    // 1. Steam discovery
    let mut steam_count = 0;
    if let Some(steam_root) = steam::detect_steam_root() {
        let steam_out = steam::scan_steam_from_root(&steam_root);
        steam_count = steam_out.games.len();
        games.extend(steam_out.games);
        warnings.extend(steam_out.warnings);
    }

    // 2. Epic Games discovery
    let mut epic_count = 0;
    if let Some(epic_dir) = epic::default_epic_manifests_dir() {
        let epic_out = epic::scan_epic_manifests_dir(&epic_dir);
        epic_count = epic_out.games.len();
        games.extend(epic_out.games);
        warnings.extend(epic_out.warnings);
    }

    // Collect already known Steam/Epic install directories so custom folders don't duplicate them
    let known_dirs: Vec<String> = games.iter().map(|g| g.install_dir.clone()).collect();

    // 3. Custom / Standalone folders discovery
    let standalone_out = standalone::scan_custom_folders(&config.custom_folders, &known_dirs);
    let standalone_count = standalone_out.games.len();
    games.extend(standalone_out.games);
    warnings.extend(standalone_out.warnings);

    // Sort alphabetically (case-insensitive) by title for a clean, deterministic library view
    games.sort_by(|a, b| {
        a.title
            .to_lowercase()
            .cmp(&b.title.to_lowercase())
            .then_with(|| a.id.cmp(&b.id))
    });

    ScanReport {
        games,
        steam_count,
        epic_count,
        standalone_count,
        warnings,
    }
}
