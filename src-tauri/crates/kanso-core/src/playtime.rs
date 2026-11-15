//! Play time measured by Kanso itself, for the stores whose clients do not expose it.
//!
//! While Kanso is open (the tray counts), it periodically lists the running processes and
//! recognises library games by the folder their executables run from. Time is credited poll by
//! poll, so a PC left asleep with a game open does not add hours. Steam games are left to
//! Steam, whose own counter Kanso reads; what that counter gains between two scans is logged
//! as a session too, so the statistics have a history for every store.

use crate::models::{DetectedGame, GamePlatform, PlaySession};
use std::collections::{BTreeSet, HashMap};

/// Seconds between two looks at the running processes.
pub const POLL_INTERVAL_SECS: u64 = 15;
/// Most time a single poll may credit: a longer gap means the PC was asleep or Kanso stalled.
const MAX_CREDIT_SECS: u64 = POLL_INTERVAL_SECS * 3;
/// Polls a game may go unseen before its session is closed (a restart, a launcher handing over
/// to the real executable).
const MISSED_POLLS_TO_END: u8 = 2;

/// [`PlaySession::source`] of a session measured by Kanso.
pub const SOURCE_KANSO: &str = "kanso";
/// [`PlaySession::source`] of time taken from Steam's own counter.
pub const SOURCE_STEAM: &str = "steam";

/// Form of a path used for comparisons: forward slashes, no trailing slash, and lower case on
/// Windows, where paths are case-insensitive.
pub fn path_key(path: &str) -> String {
    let unified = path.trim().replace('\\', "/");
    let trimmed = unified.trim_end_matches('/');
    if cfg!(windows) {
        trimmed.to_lowercase()
    } else {
        trimmed.to_string()
    }
}

/// `true` for a filesystem root (`/`, `C:`), which can never identify a single game.
fn is_root_key(key: &str) -> bool {
    key.is_empty() || (key.len() == 2 && key.ends_with(':'))
}

#[derive(Debug, Clone)]
struct Candidate {
    id: String,
    /// Folder whose executables all belong to the game; `None` when the folder is shared.
    dir: Option<String>,
    exe: Option<String>,
}

/// Recognises which library games a list of running executables belongs to.
#[derive(Debug, Clone, Default)]
pub struct ProcessMatcher {
    candidates: Vec<Candidate>,
}

impl ProcessMatcher {
    /// Every installed game except Steam's is a candidate. A game is recognised by any executable inside
    /// its install folder, or only by its own executable when that folder is a filesystem root
    /// or also contains other games.
    pub fn new(games: &[DetectedGame]) -> Self {
        let mut candidates: Vec<Candidate> = games
            .iter()
            .filter(|game| game.platform != GamePlatform::Steam && game.is_installed)
            .map(|game| Candidate {
                id: game.id.clone(),
                dir: Some(path_key(&game.install_dir)).filter(|dir| !is_root_key(dir)),
                exe: game
                    .executable_path
                    .as_deref()
                    .map(path_key)
                    .filter(|exe| !exe.is_empty()),
            })
            .collect();

        let dirs: Vec<String> = candidates.iter().filter_map(|c| c.dir.clone()).collect();
        for candidate in &mut candidates {
            let shared = candidate.dir.as_deref().is_some_and(|dir| {
                let prefix = format!("{dir}/");
                dirs.iter().any(|other| other.starts_with(&prefix))
                    || dirs.iter().filter(|other| other.as_str() == dir).count() > 1
            });
            if shared {
                candidate.dir = None;
            }
        }
        candidates.retain(|c| c.dir.is_some() || c.exe.is_some());
        Self { candidates }
    }

    pub fn is_empty(&self) -> bool {
        self.candidates.is_empty()
    }

    /// Ids of the games that own at least one of `executables` (full paths).
    pub fn running_games<I, S>(&self, executables: I) -> BTreeSet<String>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut running = BTreeSet::new();
        if self.candidates.is_empty() {
            return running;
        }
        for executable in executables {
            let exe = path_key(executable.as_ref());
            let owner = self
                .candidates
                .iter()
                .filter(|c| {
                    c.exe.as_deref() == Some(exe.as_str())
                        || c.dir.as_deref().is_some_and(|dir| {
                            exe.len() > dir.len()
                                && exe.starts_with(dir)
                                && exe.as_bytes()[dir.len()] == b'/'
                        })
                })
                // The most specific folder wins
                .max_by_key(|c| c.dir.as_deref().map_or(0, str::len));
            if let Some(owner) = owner {
                running.insert(owner.id.clone());
            }
        }
        running
    }
}

/// What changed for a game between two polls.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrackerEvent {
    /// The game was seen running for the first time.
    Started { game_id: String },
    /// The session crossed one or more whole minutes: add them to the game's total.
    Progress {
        game_id: String,
        started_at: u64,
        session_seconds: u64,
        add_minutes: u32,
    },
    /// The game is no longer running. `add_minutes` rounds the session to the nearest minute.
    Ended {
        game_id: String,
        started_at: u64,
        session_seconds: u64,
        add_minutes: u32,
    },
}

#[derive(Debug, Clone)]
struct OpenSession {
    started_at: u64,
    seconds: u64,
    credited_minutes: u64,
    missed_polls: u8,
}

/// Turns "these games are running now" observations into play time.
#[derive(Debug, Clone, Default)]
pub struct SessionTracker {
    sessions: HashMap<String, OpenSession>,
    last_poll_ms: Option<u64>,
}

impl SessionTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Games with a session in progress, sorted.
    pub fn running(&self) -> Vec<String> {
        let mut ids: Vec<String> = self.sessions.keys().cloned().collect();
        ids.sort();
        ids
    }

    /// Records that exactly `running` are running at `now_ms` and returns what follows from it.
    pub fn observe(&mut self, now_ms: u64, running: &BTreeSet<String>) -> Vec<TrackerEvent> {
        let elapsed = self
            .last_poll_ms
            .map(|last| now_ms.saturating_sub(last) / 1000)
            .unwrap_or(0)
            .min(MAX_CREDIT_SECS);
        self.last_poll_ms = Some(now_ms);
        let mut events = Vec::new();

        for game_id in running {
            match self.sessions.get_mut(game_id) {
                Some(session) => {
                    session.seconds += elapsed;
                    session.missed_polls = 0;
                    let minutes = session.seconds / 60;
                    if minutes > session.credited_minutes {
                        events.push(TrackerEvent::Progress {
                            game_id: game_id.clone(),
                            started_at: session.started_at,
                            session_seconds: session.seconds,
                            add_minutes: minutes_delta(minutes, session.credited_minutes),
                        });
                        session.credited_minutes = minutes;
                    }
                }
                None => {
                    self.sessions.insert(
                        game_id.clone(),
                        OpenSession {
                            started_at: now_ms,
                            seconds: 0,
                            credited_minutes: 0,
                            missed_polls: 0,
                        },
                    );
                    events.push(TrackerEvent::Started {
                        game_id: game_id.clone(),
                    });
                }
            }
        }

        let mut ended: Vec<String> = Vec::new();
        for (game_id, session) in &mut self.sessions {
            if running.contains(game_id) {
                continue;
            }
            session.missed_polls += 1;
            if session.missed_polls >= MISSED_POLLS_TO_END {
                ended.push(game_id.clone());
            }
        }
        ended.sort();
        for game_id in ended {
            if let Some(session) = self.sessions.remove(&game_id) {
                events.push(end_event(game_id, &session));
            }
        }
        events
    }

    /// Closes every open session (Kanso is quitting, or tracking was switched off).
    pub fn finish_all(&mut self) -> Vec<TrackerEvent> {
        let mut ids: Vec<String> = self.sessions.keys().cloned().collect();
        ids.sort();
        self.last_poll_ms = None;
        ids.into_iter()
            .filter_map(|id| self.sessions.remove(&id).map(|session| end_event(id, &session)))
            .collect()
    }
}

fn minutes_delta(minutes: u64, credited: u64) -> u32 {
    u32::try_from(minutes.saturating_sub(credited)).unwrap_or(u32::MAX)
}

fn end_event(game_id: String, session: &OpenSession) -> TrackerEvent {
    let rounded = (session.seconds + 30) / 60;
    TrackerEvent::Ended {
        game_id,
        started_at: session.started_at,
        session_seconds: session.seconds,
        add_minutes: minutes_delta(rounded, session.credited_minutes),
    }
}

/// Sessions implied by Steam's own counters: for every Steam game whose playtime grew between
/// the library as stored (`before`) and a fresh scan (`after`), the difference becomes a
/// session ending when Steam says the game was last played. Games seen for the first time
/// contribute nothing: when their hours were played is unknown.
pub fn steam_sessions_between(
    before: &[DetectedGame],
    after: &[DetectedGame],
    now_ms: u64,
) -> Vec<PlaySession> {
    let known: HashMap<&str, u32> = before
        .iter()
        .filter(|game| game.platform == GamePlatform::Steam)
        .filter_map(|game| Some((game.id.as_str(), game.playtime_minutes?)))
        .collect();

    after
        .iter()
        .filter(|game| game.platform == GamePlatform::Steam)
        .filter_map(|game| {
            let previous = *known.get(game.id.as_str())?;
            let gained = game.playtime_minutes?.checked_sub(previous).filter(|m| *m > 0)?;
            let seconds = u64::from(gained) * 60;
            let ended_at = game.last_played_at.filter(|at| *at <= now_ms).unwrap_or(now_ms);
            Some(PlaySession {
                game_id: game.id.clone(),
                started_at: ended_at.saturating_sub(seconds * 1000),
                seconds,
                source: SOURCE_STEAM.to_string(),
            })
        })
        .collect()
}

/// Lists the executables of the running processes. Kept alive between polls so that paths
/// already resolved are not asked for again.
pub struct ProcessLister {
    system: Option<sysinfo::System>,
}

impl Default for ProcessLister {
    fn default() -> Self {
        Self::new()
    }
}

impl ProcessLister {
    pub fn new() -> Self {
        Self {
            system: sysinfo::System::new().ok(),
        }
    }

    /// Full paths of the executables of every process that can be inspected; processes of
    /// other users or of the system that refuse are simply left out.
    pub fn executables(&mut self) -> Vec<String> {
        let Some(system) = self.system.as_mut() else {
            return Vec::new();
        };
        system.refresh_processes_specifics(
            sysinfo::ProcessesToUpdate::All,
            true,
            sysinfo::ProcessRefreshKind::nothing().with_exe(sysinfo::UpdateKind::OnlyIfNotSet),
        );
        system
            .processes()
            .values()
            .filter_map(|process| process.exe())
            .map(|path| path.to_string_lossy().to_string())
            .filter(|path| !path.is_empty())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn game(id: &str, platform: GamePlatform, dir: &str, exe: Option<&str>) -> DetectedGame {
        DetectedGame::new_detected(
            id.to_string(),
            id.to_string(),
            id.to_string(),
            platform,
            None,
            dir.to_string(),
            exe.map(ToString::to_string),
            None,
        )
    }

    fn running(ids: &[&str]) -> BTreeSet<String> {
        ids.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn games_are_recognised_by_any_executable_inside_their_folder() {
        let games = [
            game("epic:alan", GamePlatform::Epic, "/games/Alan Wake 2", None),
            game("gog:witcher", GamePlatform::Gog, "/games/The Witcher 3/", Some("/games/The Witcher 3/bin/x64/witcher3.exe")),
            game("steam:1", GamePlatform::Steam, "/steam/steamapps/common/Portal", None),
        ];
        let matcher = ProcessMatcher::new(&games);

        let found = matcher.running_games([
            "/games/Alan Wake 2/AlanWake2.exe",
            "/games/The Witcher 3/bin/x64/witcher3.exe",
            "/steam/steamapps/common/Portal/portal.exe",
            "/games/Alan Wake 2 Demo/demo.exe",
            "/usr/bin/bash",
        ]);
        assert_eq!(found, running(&["epic:alan", "gog:witcher"]), "Steam is left to Steam");

        // A launcher handing over to the real executable in a subfolder is still the same game
        assert_eq!(
            matcher.running_games(["/games/Alan Wake 2/Binaries/Win64/Game-Shipping.exe"]),
            running(&["epic:alan"])
        );
        assert!(matcher.running_games(Vec::<String>::new()).is_empty());
    }

    #[test]
    fn a_folder_shared_by_several_games_only_matches_exact_executables() {
        let games = [
            // An executable dropped straight into the folder that also holds other games
            game("standalone:loose", GamePlatform::Standalone, "/games", Some("/games/loose.exe")),
            game("standalone:inner", GamePlatform::Standalone, "/games/Inner", Some("/games/Inner/inner.exe")),
            // A filesystem root can never identify a game
            game("standalone:root", GamePlatform::Standalone, "/", Some("/rootgame")),
            game("standalone:nothing", GamePlatform::Standalone, "/", None),
        ];
        let matcher = ProcessMatcher::new(&games);

        assert_eq!(matcher.running_games(["/games/Inner/helper.exe"]), running(&["standalone:inner"]));
        assert_eq!(matcher.running_games(["/games/loose.exe"]), running(&["standalone:loose"]));
        assert!(matcher.running_games(["/games/other.exe", "/usr/bin/env"]).is_empty());
        assert_eq!(matcher.running_games(["/rootgame"]), running(&["standalone:root"]));
    }

    #[cfg(windows)]
    #[test]
    fn windows_paths_match_whatever_their_case_and_slashes() {
        let games = [game("epic:a", GamePlatform::Epic, r"D:\Games\Alan Wake 2\", None)];
        let matcher = ProcessMatcher::new(&games);
        assert_eq!(
            matcher.running_games([r"d:\games\ALAN WAKE 2\AlanWake2.exe"]),
            running(&["epic:a"])
        );
        assert!(ProcessMatcher::new(&[game("epic:r", GamePlatform::Epic, r"D:\", None)]).is_empty());
    }

    #[test]
    fn time_is_credited_poll_by_poll_in_whole_minutes() {
        let mut tracker = SessionTracker::new();
        let ids = running(&["epic:a"]);
        let mut now = 1_000_000;

        assert_eq!(
            tracker.observe(now, &ids),
            [TrackerEvent::Started { game_id: "epic:a".into() }]
        );
        assert_eq!(tracker.running(), ["epic:a"]);

        // Three more polls: 45 s, nothing to credit yet
        for _ in 0..3 {
            now += 15_000;
            assert!(tracker.observe(now, &ids).is_empty());
        }
        now += 15_000;
        assert_eq!(
            tracker.observe(now, &ids),
            [TrackerEvent::Progress {
                game_id: "epic:a".into(),
                started_at: 1_000_000,
                session_seconds: 60,
                add_minutes: 1,
            }]
        );

        // Two polls without the game close the session; the 40 s played since round up
        now += 15_000;
        tracker.observe(now, &ids);
        now += 15_000;
        tracker.observe(now, &ids);
        now += 10_000;
        tracker.observe(now, &ids);
        now += 15_000;
        assert!(tracker.observe(now, &running(&[])).is_empty(), "one missed poll is tolerated");
        now += 15_000;
        assert_eq!(
            tracker.observe(now, &running(&[])),
            [TrackerEvent::Ended {
                game_id: "epic:a".into(),
                started_at: 1_000_000,
                session_seconds: 100,
                add_minutes: 1,
            }]
        );
        assert!(tracker.running().is_empty());
    }

    #[test]
    fn a_sleeping_pc_does_not_add_hours_and_a_brief_absence_keeps_the_session() {
        let mut tracker = SessionTracker::new();
        let ids = running(&["gog:b"]);
        tracker.observe(0, &ids);

        // Eight hours pass between two polls (the PC slept): only one poll's worth counts
        assert!(tracker.observe(8 * 3_600_000, &ids).is_empty(), "45 s credited, no minute yet");

        // Missing for one poll (the launcher handing over), then back: same session
        let t = 8 * 3_600_000;
        assert!(tracker.observe(t + 15_000, &running(&[])).is_empty());
        assert_eq!(
            tracker.observe(t + 30_000, &ids),
            [TrackerEvent::Progress {
                game_id: "gog:b".into(),
                started_at: 0,
                session_seconds: 60,
                add_minutes: 1,
            }]
        );

        // Quitting Kanso closes what is open; a session of a few seconds adds nothing
        tracker.observe(t + 45_000, &running(&["gog:b", "ea:c"]));
        let closed = tracker.finish_all();
        assert_eq!(closed.len(), 2);
        assert!(closed.contains(&TrackerEvent::Ended {
            game_id: "ea:c".into(),
            started_at: t + 45_000,
            session_seconds: 0,
            add_minutes: 0,
        }));
        assert!(closed.contains(&TrackerEvent::Ended {
            game_id: "gog:b".into(),
            started_at: 0,
            session_seconds: 75,
            add_minutes: 0,
        }));
    }

    #[test]
    fn steam_counter_growth_between_scans_becomes_a_session() {
        let mut stored = game("steam:10", GamePlatform::Steam, "/s/10", None);
        stored.playtime_minutes = Some(600);
        let mut unchanged = game("steam:20", GamePlatform::Steam, "/s/20", None);
        unchanged.playtime_minutes = Some(30);
        let mut other_store = game("epic:x", GamePlatform::Epic, "/e/x", None);
        other_store.playtime_minutes = Some(10);

        let mut played = stored.clone();
        played.playtime_minutes = Some(690);
        played.last_played_at = Some(5_000_000_000);
        let mut brand_new = game("steam:30", GamePlatform::Steam, "/s/30", None);
        brand_new.playtime_minutes = Some(4_000);
        let mut other_store_after = other_store.clone();
        other_store_after.playtime_minutes = Some(50);

        let sessions = steam_sessions_between(
            &[stored, unchanged.clone(), other_store],
            &[played, unchanged, brand_new, other_store_after],
            6_000_000_000,
        );
        assert_eq!(
            sessions,
            [PlaySession {
                game_id: "steam:10".into(),
                started_at: 5_000_000_000 - 90 * 60_000,
                seconds: 90 * 60,
                source: SOURCE_STEAM.into(),
            }]
        );
    }

    #[test]
    fn the_process_list_contains_this_very_test_binary() {
        let me = std::env::current_exe().expect("current exe");
        let me = path_key(&me.to_string_lossy());
        let listed = ProcessLister::new().executables();
        assert!(
            listed.iter().any(|path| path_key(path) == me),
            "{me} not among {} processes",
            listed.len()
        );
    }
}
