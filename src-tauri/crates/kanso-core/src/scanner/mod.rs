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
use crate::emulators::{self, own_settings::Places, Emulator};
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
/// 9. Emulators and the game files they run (see [`crate::emulators`])
///
/// A game is listed once: when a later source reports an install folder an earlier one already
/// claimed (e.g. an EA or Ubisoft title bought and installed through Steam), the later entry
/// is dropped.
pub fn scan_all_installed_games(config: &AppConfig) -> ScanReport {
    scan_library(config, None, false).0
}

/// What the search for emulators draws on from this PC (see [`emulators::find`]):
/// `remembered_file` is where discoveries are kept between scans, `thorough` asks for the
/// disks to be searched as well.
pub fn emulator_search(remembered_file: Option<&std::path::Path>, thorough: bool) -> emulators::find::Search {
    emulators::find::Search {
        remembered_file: remembered_file.map(std::path::Path::to_path_buf),
        thorough,
        drives: xbox::fixed_drive_roots(),
        run_programs: emulators::find::RUN_PROGRAM_KEYS
            .iter()
            .flat_map(|key| registry::read_registry_value_names(registry::RegistryHive::CurrentUser, key))
            .collect(),
        uninstall_entries: registry::read_uninstall_entries(),
    }
}

/// [`scan_all_installed_games`], together with the emulators found on the way: the interface
/// lists them, and starting an emulated game needs to know its emulator.
pub fn scan_library(
    config: &AppConfig,
    emulators_file: Option<&std::path::Path>,
    thorough: bool,
) -> (ScanReport, Vec<Emulator>) {
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
    let mut standalone_out = standalone::scan_custom_folders(&config.custom_folders, &known_dirs);
    // An emulator kept in a games folder is an emulator, not a game
    let (emulator_exes, loose_games) = split_off_emulators(std::mem::take(&mut standalone_out.games));
    // A loose program says its own version; a store's game is given one by its store
    let mut loose_games = loose_games;
    for game in &mut loose_games {
        game.version = game
            .executable_path
            .as_deref()
            .and_then(|exe| crate::pe_version::file_version(std::path::Path::new(exe)))
            .map(|version| version.short());
    }
    report.games.extend(loose_games);
    report.warnings.extend(standalone_out.warnings);

    let places = Places::of_this_pc();
    let mut found = emulator_exes;
    found.extend(emulators::find::find(&places, &emulator_search(emulators_file, thorough)));
    let emulators = emulators::resolve(&config.emulators, &found, &places);
    let emulated = emulators::scan_games(&emulators);
    report.games.extend(emulated.games);
    report.warnings.extend(emulated.warnings);

    sort_games_by_title(&mut report.games);
    (report, emulators)
}

/// The emulators of this PC without scanning for games: those added in Settings, those
/// installed or run, and those earlier scans found. The disks are not searched.
pub fn find_emulators(config: &AppConfig, emulators_file: Option<&std::path::Path>) -> Vec<Emulator> {
    let places = Places::of_this_pc();
    let mut search = emulator_search(emulators_file, false);
    // Listing is not searching: with nothing remembered yet the disks are left to the scan
    if !emulators_file.is_some_and(std::path::Path::is_file) {
        search.remembered_file = None;
    }
    let found = emulators::find::find(&places, &search);
    emulators::resolve(&config.emulators, &found, &places)
}

/// Separates the executables of known emulators from the loose games of the custom folders.
fn split_off_emulators(games: Vec<DetectedGame>) -> (Vec<std::path::PathBuf>, Vec<DetectedGame>) {
    let mut exes = Vec::new();
    let mut kept = Vec::new();
    for game in games {
        match game.executable_path.as_deref().map(std::path::PathBuf::from) {
            Some(exe) if emulators::identify(&exe).is_some() => exes.push(exe),
            _ => kept.push(game),
        }
    }
    (exes, kept)
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
    fn an_emulator_in_a_games_folder_is_not_listed_as_a_game() {
        let mut dolphin = game("standalone:a", GamePlatform::Standalone, "D:/Games/Dolphin");
        dolphin.executable_path = Some("D:/Games/Dolphin/Dolphin.exe".to_string());
        let mut celeste = game("standalone:b", GamePlatform::Standalone, "D:/Games/Celeste");
        celeste.executable_path = Some("D:/Games/Celeste/Celeste.exe".to_string());
        let nameless = game("standalone:c", GamePlatform::Standalone, "D:/Games/Odd");

        let (exes, games) = split_off_emulators(vec![dolphin, celeste, nameless]);
        assert_eq!(exes, [std::path::PathBuf::from("D:/Games/Dolphin/Dolphin.exe")]);
        let ids: Vec<&str> = games.iter().map(|g| g.id.as_str()).collect();
        assert_eq!(ids, ["standalone:b", "standalone:c"]);
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
