//! Steam achievements, read from the Steam client's own library cache: no sign-in, no API key.
//!
//! For every game whose page was opened in the Steam library, the client keeps
//! `userdata/<account>/config/librarycache/<appid>.json`: a list of `[section, {version, data}]`
//! pairs. The `achievements` section holds the exact totals and the achievements the library
//! page shows: the ones unlocked and a selection of the ones still locked (hidden locked
//! achievements are never listed). The file is as fresh as Steam's last visit to that page.

use serde::Serialize;
use serde_json::Value;
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use url::Url;

/// A cache file larger than this is not Steam's (the real ones are a few hundred kB at most).
const MAX_CACHE_FILE_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Achievement {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    /// Icon on Steam's CDN.
    pub icon_url: Option<String>,
    pub unlocked: bool,
    /// When it was unlocked (ms).
    pub unlocked_at: Option<u64>,
    /// Share of all players who have it, 0–100.
    pub global_percent: Option<f64>,
    /// How far along a counted achievement is, 0–1.
    pub progress: Option<f64>,
}

/// What Steam knows about a game's achievements for the local account.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameAchievements {
    pub total: u32,
    pub unlocked: u32,
    /// Unlocked achievements first (most recent first), then the locked ones Steam lists.
    /// May be shorter than `total`.
    pub items: Vec<Achievement>,
}

/// Parses a Steam `librarycache/<appid>.json`. `None` when the file is not in the expected
/// shape or the game has no achievements.
pub fn parse_library_cache_achievements(content: &str) -> Option<GameAchievements> {
    let root: Value = serde_json::from_str(content).ok()?;
    let section = root.as_array()?.iter().find_map(|entry| {
        let pair = entry.as_array()?;
        (pair.first()?.as_str()? == "achievements").then(|| pair.get(1))?
    })?;
    // Some sections carry their data as a JSON document inside a string
    let embedded;
    let data = match section.get("data")? {
        Value::String(raw) => {
            embedded = serde_json::from_str::<Value>(raw).ok()?;
            &embedded
        }
        other => other,
    };

    let count = |key: &str| {
        data.get(key)
            .and_then(Value::as_u64)
            .and_then(|n| u32::try_from(n).ok())
    };
    let total = count("nTotal").filter(|total| *total > 0)?;
    let unlocked = count("nAchieved").unwrap_or(0).min(total);

    let mut seen = HashSet::new();
    let mut items: Vec<Achievement> = ["vecHighlight", "vecAchievedHidden", "vecUnachieved"]
        .into_iter()
        .filter_map(|key| data.get(key)?.as_array())
        .flatten()
        .filter_map(parse_achievement)
        .filter(|achievement| seen.insert(achievement.id.clone()))
        .collect();
    // Unlocked first, newest first; the locked ones keep Steam's order (most common first)
    items.sort_by(|a, b| {
        b.unlocked
            .cmp(&a.unlocked)
            .then_with(|| b.unlocked_at.cmp(&a.unlocked_at))
    });

    Some(GameAchievements {
        total,
        unlocked,
        items,
    })
}

fn parse_achievement(raw: &Value) -> Option<Achievement> {
    let text = |key: &str| {
        raw.get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string)
    };
    let number = |key: &str| raw.get(key).and_then(Value::as_f64).filter(|n| n.is_finite());

    let id = text("strID")?;
    let unlocked = raw.get("bAchieved").and_then(Value::as_bool).unwrap_or(false);
    let unlocked_at = raw
        .get("rtUnlocked")
        .and_then(Value::as_u64)
        .filter(|secs| unlocked && *secs > 0)
        .map(|secs| secs.saturating_mul(1000));
    let progress = match (number("flCurrentProgress"), number("flMinProgress"), number("flMaxProgress")) {
        (Some(current), min, Some(max)) if !unlocked => {
            let min = min.unwrap_or(0.0);
            (max > min).then(|| ((current - min) / (max - min)).clamp(0.0, 1.0))
        }
        _ => None,
    };

    Some(Achievement {
        name: text("strName").unwrap_or_else(|| id.clone()),
        id,
        description: text("strDescription"),
        icon_url: text("strImage").filter(|url| is_steam_cdn_url(url)),
        unlocked,
        unlocked_at,
        global_percent: number("flAchieved").map(|percent| percent.clamp(0.0, 100.0)),
        progress,
    })
}

/// Only `https` images on Valve's own hosts are passed on to the interface.
fn is_steam_cdn_url(raw: &str) -> bool {
    let Ok(url) = Url::parse(raw) else {
        return false;
    };
    let host = url.host_str().unwrap_or_default().to_ascii_lowercase();
    url.scheme() == "https"
        && ["steamstatic.com", "steampowered.com", "steamusercontent.com"]
            .iter()
            .any(|domain| host == *domain || host.ends_with(&format!(".{domain}")))
}

/// Achievements of the Steam app `appid` from an account's `config` folder
/// (see [`crate::scanner::steam::steam_account_config_dir`]).
pub fn read_achievements_from_config_dir(config_dir: &Path, appid: &str) -> Option<GameAchievements> {
    let appid = appid.trim();
    if appid.is_empty() || !appid.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let path = config_dir.join("librarycache").join(format!("{appid}.json"));
    if fs::metadata(&path).ok()?.len() > MAX_CACHE_FILE_BYTES {
        return None;
    }
    let bytes = fs::read(path).ok()?;
    parse_library_cache_achievements(&String::from_utf8_lossy(&bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A file written by a real Steam client (Hogwarts Legacy, nothing unlocked yet).
    const REAL_CACHE: &str = include_str!("../tests/fixtures/steam_librarycache_990080.json");

    #[test]
    fn reads_totals_and_locked_achievements_from_a_real_steam_cache_file() {
        let achievements = parse_library_cache_achievements(REAL_CACHE).expect("achievements");
        assert_eq!(achievements.total, 45);
        assert_eq!(achievements.unlocked, 0);
        // Steam lists a selection of the locked ones, not all 45
        assert_eq!(achievements.items.len(), 12);

        let first = &achievements.items[0];
        assert_eq!(first.id, "PFA_30");
        assert_eq!(first.name, "A Talent for Spending");
        assert_eq!(first.description.as_deref(), Some("Spend 5 Talent Points"));
        assert!(!first.unlocked);
        assert_eq!(first.unlocked_at, None);
        assert!((first.global_percent.unwrap() - 61.2).abs() < 0.01);
        assert_eq!(first.progress, None, "0/0 is not a counted achievement");
        assert!(first
            .icon_url
            .as_deref()
            .is_some_and(|url| url.starts_with("https://shared.steamstatic.com/")));
    }

    #[test]
    fn unlocked_achievements_come_first_and_progress_is_normalised() {
        let cache = r#"[
          ["descriptions", {"version": 1, "data": {"strSnippet": "x"}}],
          ["achievements", {"version": 2, "data": {
            "vecHighlight": [
              {"strID": "OLD", "strName": "Old one", "bAchieved": true, "rtUnlocked": 1700000000, "flAchieved": 80.5},
              {"strID": "NEW", "strName": "New one", "bAchieved": true, "rtUnlocked": 1750000000, "flAchieved": 3}
            ],
            "vecAchievedHidden": [
              {"strID": "SECRET", "strName": "Secret", "strDescription": "", "bAchieved": true, "rtUnlocked": 1720000000},
              {"strID": "NEW", "strName": "Duplicate of NEW", "bAchieved": true, "rtUnlocked": 1}
            ],
            "vecUnachieved": [
              {"strID": "COUNT", "strName": "Collector", "strImage": "http://evil.example/a.jpg",
               "bAchieved": false, "rtUnlocked": 0, "flAchieved": 250,
               "flMinProgress": 0, "flCurrentProgress": 30, "flMaxProgress": 120},
              {"strName": "No id"}
            ],
            "nTotal": 10, "nAchieved": 3}}]
        ]"#;
        let achievements = parse_library_cache_achievements(cache).expect("achievements");
        assert_eq!((achievements.unlocked, achievements.total), (3, 10));
        let ids: Vec<&str> = achievements.items.iter().map(|a| a.id.as_str()).collect();
        assert_eq!(ids, ["NEW", "SECRET", "OLD", "COUNT"]);
        assert_eq!(achievements.items[0].unlocked_at, Some(1_750_000_000_000));
        assert_eq!(achievements.items[1].description, None);

        let counted = &achievements.items[3];
        assert_eq!(counted.progress, Some(0.25));
        assert_eq!(counted.global_percent, Some(100.0), "out-of-range values are clamped");
        assert_eq!(counted.icon_url, None, "only https images on Steam's hosts are kept");
    }

    #[test]
    fn games_without_achievements_and_unexpected_files_yield_nothing() {
        let none = r#"[["achievements", {"version": 2, "data": {"vecUnachieved": [], "nTotal": 0, "nAchieved": 0}}]]"#;
        assert_eq!(parse_library_cache_achievements(none), None);
        assert_eq!(parse_library_cache_achievements(r#"[["usernews", {"data": []}]]"#), None);
        assert_eq!(parse_library_cache_achievements(r#"{"achievements": {}}"#), None);
        assert_eq!(parse_library_cache_achievements("not json"), None);

        // Data embedded as a string, as other sections of the same file do
        let embedded = r#"[["achievements", {"version": 3, "data": "{\"nTotal\": 4, \"nAchieved\": 9}"}]]"#;
        let parsed = parse_library_cache_achievements(embedded).expect("achievements");
        assert_eq!((parsed.unlocked, parsed.total), (4, 4), "never more unlocked than exist");
    }

    #[test]
    fn reads_the_cache_file_of_an_app_and_rejects_odd_app_ids() {
        let dir = tempfile::tempdir().unwrap();
        let cache_dir = dir.path().join("librarycache");
        fs::create_dir_all(&cache_dir).unwrap();
        fs::write(cache_dir.join("990080.json"), REAL_CACHE).unwrap();

        let found = read_achievements_from_config_dir(dir.path(), "990080").expect("achievements");
        assert_eq!(found.total, 45);
        assert_eq!(read_achievements_from_config_dir(dir.path(), "123"), None);
        assert_eq!(read_achievements_from_config_dir(dir.path(), "../990080"), None);
        assert_eq!(read_achievements_from_config_dir(dir.path(), ""), None);
    }
}
