pub mod battlenet;
pub mod ea;
pub mod epic;
pub mod epic_catalog;
pub mod exe_pick;
pub mod gog;
pub mod gog_galaxy;
pub mod registry;
pub mod standalone;
pub mod steam;
pub mod steam_packages;
pub mod ubisoft;
pub mod ubisoft_cache;
pub mod xbox;

use crate::config::AppConfig;
use crate::db::sort_games_by_title;
use crate::models::{DetectedGame, ScanReport, SourceScanOutput};
use exe_pick::normalize_dir_key;

/// Runs the complete game auto-discovery pipeline on the host machine:
/// 1. Steam (Registry + `libraryfolders.vdf` + `appmanifest_*.acf`)
/// 2. Epic Games (`C:\ProgramData\Epic\EpicGamesLauncher\Data\Manifests\*.item`)
/// 3. GOG (`HKLM\SOFTWARE\WOW6432Node\GOG.com\Games`)
/// 4. Ubisoft Connect (`Uninstall\Uplay Install <id>` + `Ubisoft\Launcher\Installs`)
/// 5. EA app (`Uninstall` entries laid down by the EA installer)
/// 6. Battle.net (`HKLM\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall`)
/// 7. Xbox app / PC Game Pass (`<drive>:\XboxGames\*\Content\MicrosoftGame.config`)
/// 8. User-configured Standalone / Custom folders (`*.exe` with exclusion rules)
///
/// A game is listed once: when a later source reports an install folder an earlier one already
/// claimed (e.g. an EA or Ubisoft title bought and installed through Steam), the later entry
/// is dropped.
pub fn scan_all_installed_games(config: &AppConfig) -> ScanReport {
    let mut sources: Vec<SourceScanOutput> = Vec::new();

    if let Some(steam_root) = steam::detect_steam_root() {
        sources.push(steam::scan_steam_from_root(&steam_root));
    }
    if let Some(epic_dir) = epic::default_epic_manifests_dir() {
        sources.push(epic::scan_epic_manifests_dir(&epic_dir));
    }
    sources.push(gog::scan_gog_installed());
    sources.push(ubisoft::scan_ubisoft_installed());
    sources.push(ea::scan_ea_installed());
    sources.push(battlenet::scan_battlenet_installed());
    sources.push(xbox::scan_xbox_installed());

    let mut report = merge_scan_sources(sources);

    // Custom folders come last so they never duplicate what a store already reported
    let known_dirs: Vec<String> = report.games.iter().map(|g| g.install_dir.clone()).collect();
    let standalone_out = standalone::scan_custom_folders(&config.custom_folders, &known_dirs);
    report.games.extend(standalone_out.games);
    report.warnings.extend(standalone_out.warnings);

    sort_games_by_title(&mut report.games);
    report
}

/// The games the store clients on this PC list for the account, installed or not, read from
/// the clients' own local records: no sign-in, no network. Steam is not here: its records
/// give ids only, and [`crate::owned::sync_steam_library`] asks the store for the rest.
pub fn read_owned_games_from_clients() -> Vec<crate::owned::OwnedGame> {
    let mut owned = Vec::new();
    if let Some(path) = epic_catalog::detect_epic_catalog_cache() {
        owned.extend(epic_catalog::read_owned_epic_games(&path));
    }
    if let Some(path) = gog_galaxy::detect_gog_galaxy_db() {
        owned.extend(gog_galaxy::read_owned_gog_games(&path));
    }
    owned.extend(ubisoft_cache::read_owned_ubisoft_games_of_this_pc());
    owned
}

/// Concatenates store scan results in priority order, dropping games whose install folder was
/// already claimed by an earlier source.
pub fn merge_scan_sources(sources: Vec<SourceScanOutput>) -> ScanReport {
    let mut games: Vec<DetectedGame> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();
    let mut claimed_dirs: Vec<String> = Vec::new();

    for source in sources {
        warnings.extend(source.warnings);
        // Folders claimed by this same source do not exclude each other
        let mut claimed_by_this_source = Vec::new();
        for game in source.games {
            let dir_key = normalize_dir_key(&game.install_dir);
            let duplicate = !dir_key.is_empty()
                && claimed_dirs
                    .iter()
                    .any(|claimed| is_same_or_nested_dir(&dir_key, claimed));
            if duplicate {
                continue;
            }
            if !dir_key.is_empty() {
                claimed_by_this_source.push(dir_key);
            }
            games.push(game);
        }
        claimed_dirs.extend(claimed_by_this_source);
    }

    ScanReport { games, warnings }
}

/// `true` when the two normalized folders are the same or one contains the other.
fn is_same_or_nested_dir(a: &str, b: &str) -> bool {
    let nested = |inner: &str, outer: &str| {
        inner
            .strip_prefix(outer)
            .is_some_and(|rest| rest.starts_with('/'))
    };
    a == b || nested(a, b) || nested(b, a)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::GamePlatform;

    fn game(id: &str, platform: GamePlatform, install_dir: &str) -> DetectedGame {
        DetectedGame::new_detected(
            id.to_string(),
            id.to_string(),
            id.to_string(),
            platform,
            None,
            install_dir.to_string(),
            None,
            None,
        )
    }

    fn source(games: Vec<DetectedGame>) -> SourceScanOutput {
        SourceScanOutput {
            games,
            warnings: Vec::new(),
        }
    }

    #[test]
    fn later_sources_do_not_duplicate_games_already_claimed_by_install_folder() {
        let steam = source(vec![
            game("steam:1", GamePlatform::Steam, r"D:\Steam\steamapps\common\Apex Legends"),
            game("steam:2", GamePlatform::Steam, r"D:\Steam\steamapps\common\Anno 1800"),
        ]);
        let ubisoft = source(vec![
            // Same game, bought on Steam: only path casing and trailing slash differ
            game("ubisoft:4553", GamePlatform::Ubisoft, r"d:\steam\steamapps\common\anno 1800\"),
            game("ubisoft:635", GamePlatform::Ubisoft, r"E:\Ubisoft\Far Cry 6"),
        ]);
        let ea = source(vec![
            game("ea:apex-legends", GamePlatform::Ea, r"D:\Steam\steamapps\common\Apex Legends"),
            // A different game whose folder merely shares a name prefix
            game("ea:apex-legends-2", GamePlatform::Ea, r"D:\Steam\steamapps\common\Apex Legends 2"),
        ]);
        let battlenet = SourceScanOutput {
            // Reported from inside a folder Ubisoft already claimed
            games: vec![game("battlenet:x", GamePlatform::BattleNet, r"E:\Ubisoft\Far Cry 6\bin")],
            warnings: vec!["avviso".to_string()],
        };

        let report = merge_scan_sources(vec![steam, ubisoft, ea, battlenet]);
        let ids: Vec<&str> = report.games.iter().map(|g| g.id.as_str()).collect();
        assert_eq!(ids, vec!["steam:1", "steam:2", "ubisoft:635", "ea:apex-legends-2"]);
        assert_eq!(report.warnings, vec!["avviso"]);
    }

    #[test]
    fn games_of_the_same_source_sharing_a_folder_are_all_kept() {
        // Two Steam manifests can legitimately point at the same folder (game + its tool)
        let steam = source(vec![
            game("steam:1", GamePlatform::Steam, r"D:\Steam\steamapps\common\Shared"),
            game("steam:2", GamePlatform::Steam, r"D:\Steam\steamapps\common\Shared"),
        ]);
        assert_eq!(merge_scan_sources(vec![steam]).games.len(), 2);
    }
}
