//! The official plugins, run in the real sandbox against canned answers of their services.
//!
//! The Steam answers are copies of live responses. HowLongToBeat follows the flow of the
//! maintained open-source clients; PCGamingWiki and IsThereAnyDeal follow their public API
//! documentation. None of the three could be reached from the build environment.

use super::http::{HttpRequest, HttpResponse};
use super::output::{Block, GamesStyle, PluginGameData, PluginView};
use super::registry::{bundled_plugins, Plugin};
use super::runtime::tests::{ok_json, FakeTransport};
use super::*;
use crate::models::{GamePlatform, GameStatus};
use serde_json::json;

fn plugin(id: &str) -> Plugin {
    bundled_plugins()
        .into_iter()
        .find(|plugin| plugin.manifest.id == id)
        .unwrap_or_else(|| panic!("no shipped plugin {id}"))
}

fn config_with(id: &str, settings: &[(&str, &str)]) -> AppConfig {
    let mut config = AppConfig::default();
    config.plugins.insert(
        id.to_string(),
        PluginState {
            enabled: true,
            show_badge: true,
            settings: settings.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
        },
    );
    config
}

fn steam_game(appid: &str, title: &str) -> DetectedGame {
    DetectedGame::new_detected(
        format!("steam:{appid}"),
        title.to_string(),
        title.to_string(),
        GamePlatform::Steam,
        Some(appid.to_string()),
        format!("/steam/common/{title}"),
        None,
        Some(format!("steam://rungameid/{appid}")),
    )
}

fn other_game(id: &str, title: &str) -> DetectedGame {
    DetectedGame::new_detected(
        id.to_string(),
        title.to_string(),
        title.to_string(),
        GamePlatform::Gog,
        None,
        format!("/gog/{title}"),
        None,
        None,
    )
}

struct Bench {
    db: Mutex<LibraryDb>,
    engine: PluginEngine,
    transport: Arc<FakeTransport>,
}

impl Bench {
    fn new(answer: impl Fn(&HttpRequest) -> Result<HttpResponse, String> + Send + Sync + 'static) -> Self {
        let transport = FakeTransport::new(answer);
        Self {
            db: Mutex::new(LibraryDb::open_in_memory().unwrap()),
            engine: PluginEngine::with_transport(transport.clone(), "0.1.0"),
            transport,
        }
    }

    fn details(&self, plugin: &Plugin, config: &AppConfig, lang: Lang, game: &DetectedGame) -> PluginGameData {
        let answer = self.engine.game_data(&self.db, plugin, config, lang, game, true, 1_000);
        assert_eq!(answer.error, None, "logs: {:?}", answer.logs);
        answer.data.expect("data")
    }

    /// What the plugin adds to Home, asked afresh (past the cache).
    fn home(&self, plugin: &Plugin, config: &AppConfig, lang: Lang, games: &[DetectedGame], now: u64) -> PluginView {
        let answer = self.engine.home_data(&self.db, plugin, config, lang, games, true, now);
        assert_eq!(answer.error, None, "logs: {:?}", answer.logs);
        answer.data.expect("a view")
    }

    fn command(&self, plugin: &Plugin, config: &AppConfig, games: &[DetectedGame], now: u64) -> PluginView {
        self.engine
            .run_command(&self.db, plugin, config, Lang::It, "pick", games, now)
            .expect("command runs")
    }
}

fn status(code: u16, body: &str) -> Result<HttpResponse, String> {
    Ok(HttpResponse {
        status: code,
        headers: Vec::new(),
        body: body.to_string(),
    })
}

fn stat<'a>(data: &'a PluginGameData, label: &str) -> Option<&'a str> {
    data.sections
        .iter()
        .flat_map(|section| &section.blocks)
        .find_map(|block| match block {
            Block::Stats { items } => items.iter().find(|item| item.label == label).map(|item| item.value.as_str()),
            _ => None,
        })
}

fn links(data: &PluginGameData) -> Vec<&str> {
    data.sections
        .iter()
        .flat_map(|section| &section.blocks)
        .flat_map(|block| match block {
            Block::Links { items } => items.iter().map(|item| item.url.as_str()).collect::<Vec<_>>(),
            _ => Vec::new(),
        })
        .collect()
}

#[test]
fn play_tonight_suggests_one_to_continue_one_from_the_backlog_and_one_to_rediscover() {
    let bench = Bench::new(|request| Err(format!("this plugin has no network: {}", request.url)));
    let plugin = plugin("play-tonight");
    let config = AppConfig::default();
    assert!(is_enabled(&plugin, &config), "works offline, so it is on from the start");
    let day = 24 * 3_600_000_u64;
    let now = 400 * day;

    let mut playing = other_game("gog:playing", "In corso");
    playing.status = Some(GameStatus::Playing);
    playing.playtime_minutes = Some(600);
    playing.last_played_at = Some(now - 2 * day);
    let never = other_game("gog:never", "Mai aperto");
    let mut old = other_game("gog:old", "Dimenticato");
    old.playtime_minutes = Some(90);
    old.last_played_at = Some(now - 200 * day);
    let mut finished = other_game("gog:finished", "Finito");
    finished.status = Some(GameStatus::Finished);
    finished.playtime_minutes = Some(3000);
    finished.last_played_at = Some(now - 300 * day);
    let mut abandoned = other_game("gog:abandoned", "Abbandonato");
    abandoned.status = Some(GameStatus::Abandoned);
    let mut hidden = other_game("gog:hidden", "Nascosto");
    hidden.is_hidden = true;
    // In progress, but not on disk: nothing to start tonight
    let mut absent = other_game("gog:absent", "Disinstallato");
    absent.status = Some(GameStatus::Playing);
    absent.is_installed = false;
    let games = [playing, never, old, finished, abandoned, hidden, absent];

    for _ in 0..5 {
        let view = bench.command(&plugin, &config, &games, now);
        assert_eq!(view.title.as_deref(), Some("Stasera potresti giocare a…"));
        assert_eq!(view.games_style, GamesStyle::Mystery, "three cards to turn over");
        let hints: Vec<&str> = view.games.iter().filter_map(|pick| pick.hint.as_deref()).collect();
        assert_eq!(hints, ["Continua", "Dal backlog", "Riscopri"], "what each card says face down");
        let picks: Vec<(&str, &str)> = view
            .games
            .iter()
            .map(|pick| (pick.id.as_str(), pick.note.as_deref().unwrap_or_default()))
            .collect();
        assert_eq!(
            picks,
            [
                ("gog:playing", "Continua · 10 h di gioco"),
                ("gog:never", "Dal backlog · mai giocato"),
                ("gog:old", "Riscopri · non lo apri da 6 mesi"),
            ]
        );
    }

    // A game played only for minutes says so instead of rounding up to an hour
    let mut brief = other_game("gog:brief", "Appena provato");
    brief.playtime_minutes = Some(2);
    brief.last_played_at = Some(now - day);
    let view = bench.command(&plugin, &config, &[brief], now);
    assert_eq!(view.games[0].note.as_deref(), Some("Continua · 2 min di gioco"));

    // Finished games come back into play only when asked for
    let with_finished = config_with("play-tonight", &[("includeFinished", "true")]);
    let only_done = &games[3..];
    assert!(bench.command(&plugin, &config, only_done, now).games.is_empty());
    let view = bench.command(&plugin, &with_finished, only_done, now);
    assert_eq!(view.games.len(), 1);
    assert_eq!(view.games[0].id, "gog:finished");

    // An empty library gets an explanation instead of an empty dialog
    let view = bench.command(&plugin, &config, &[], now);
    assert!(view.games.is_empty());
    assert!(matches!(&view.sections[0].blocks[0], Block::Notice { .. }));
    assert!(bench.transport.urls().is_empty());
}

#[test]
fn daily_pick_proposes_one_game_a_day_on_home_from_the_backlog_first() {
    let offline = || Bench::new(|request| Err(format!("this plugin has no network: {}", request.url)));
    let plugin = plugin("daily-pick");
    assert!(plugin.manifest.home, "it adds a part to Home");
    assert!(!plugin.manifest.game_details && plugin.manifest.commands.is_empty(), "and nothing else");
    assert!(plugin.manifest.hosts.is_empty(), "it works offline");
    assert!(
        !is_enabled(&plugin, &AppConfig::default()),
        "nobody's Home changes until they switch it on"
    );
    let config = config_with("daily-pick", &[]);
    let hour = 3_600_000_u64;
    let day = 24 * hour;
    // 12:00 UTC. What must fall on the same day stays within two minutes of it: in no time
    // zone does midnight come between 12:00 and 12:02 UTC
    let noon = 20_000 * day + 12 * hour;

    let never = |id: &str, title: &str| other_game(id, title);
    let mut old = other_game("gog:old", "Dimenticato");
    old.playtime_minutes = Some(90);
    old.last_played_at = Some(noon - 200 * day);
    let mut playing = other_game("gog:playing", "In corso");
    playing.status = Some(GameStatus::Playing);
    playing.playtime_minutes = Some(600);
    playing.last_played_at = Some(noon - 2 * day);
    let mut finished = other_game("gog:finished", "Finito");
    finished.status = Some(GameStatus::Finished);
    let mut abandoned = other_game("gog:abandoned", "Abbandonato");
    abandoned.status = Some(GameStatus::Abandoned);
    let mut hidden = other_game("gog:hidden", "Nascosto");
    hidden.is_hidden = true;
    // Never played, but not on disk: what can be started right now comes first
    let mut absent = other_game("gog:absent", "Non installato");
    absent.is_installed = false;
    let waiting = ["gog:never-a", "gog:never-b", "gog:never-c"];
    let games = vec![
        playing.clone(),
        never("gog:never-a", "Mai aperto A"),
        old.clone(),
        finished.clone(),
        never("gog:never-b", "Mai aperto B"),
        abandoned.clone(),
        hidden.clone(),
        absent.clone(),
        never("gog:never-c", "Mai aperto C"),
    ];

    // One game, as a whole cover, with a line that says why
    let bench = offline();
    let view = bench.home(&plugin, &config, Lang::It, &games, noon);
    assert_eq!(view.title.as_deref(), Some("Il gioco di oggi"));
    assert_eq!(view.games_style, GamesStyle::Cards);
    assert!(view.sections.is_empty());
    assert_eq!(view.games.len(), 1, "one game a day: {:?}", view.games);
    let today = view.games[0].id.clone();
    assert!(waiting.contains(&today.as_str()), "from what was never played and is on disk: {today}");
    assert_eq!(view.games[0].note.as_deref(), Some("Dal backlog · mai giocato"));

    // Asked again the same day it is the same game: with the library in another order,
    // from a plugin that remembers nothing, and after the game was started
    let mut reversed = games.clone();
    reversed.reverse();
    assert_eq!(bench.home(&plugin, &config, Lang::It, &reversed, noon + 60_000).games[0].id, today);
    assert_eq!(offline().home(&plugin, &config, Lang::It, &reversed, noon + 60_000).games[0].id, today);
    let mut started = games.clone();
    for game in started.iter_mut().filter(|game| game.id == today) {
        game.playtime_minutes = Some(45);
        game.play_count = 1;
        game.last_played_at = Some(noon + 60_000);
    }
    let view = bench.home(&plugin, &config, Lang::It, &started, noon + 2 * 60_000);
    assert_eq!(view.games[0].id, today, "starting it does not make another take its place");
    assert_eq!(view.games[0].note.as_deref(), Some("Dalla tua libreria · 45 min di gioco"));

    // The days after: still from the games that wait, and never the same two days running
    let mut previous = today;
    for n in 1..=8 {
        let view = bench.home(&plugin, &config, Lang::It, &games, noon + n * day);
        let pick = view.games[0].id.clone();
        assert!(waiting.contains(&pick.as_str()), "day {n}: {pick}");
        assert_ne!(pick, previous, "day {n} repeats the day before");
        previous = pick;
    }
    // Over many days every game that waits has its turn, none far more often than another
    let turns = offline();
    let mut times = std::collections::BTreeMap::new();
    for n in 0..90 {
        let view = turns.home(&plugin, &config, Lang::It, &games, noon + n * day);
        *times.entry(view.games[0].id.clone()).or_insert(0_u32) += 1;
    }
    assert_eq!(times.len(), waiting.len(), "{times:?}");
    assert!(times.values().all(|count| (20..=40).contains(count)), "no game is passed over: {times:?}");

    // One game alone in its group is not proposed every day while the library has others:
    // the next group has its day, then the first one again
    let lone = offline();
    let few = [playing.clone(), never("gog:never-a", "Mai aperto A"), old.clone()];
    let mut days = Vec::new();
    for n in 0..6 {
        days.push(lone.home(&plugin, &config, Lang::It, &few, noon + n * day).games[0].id.clone());
    }
    assert!(days.windows(2).all(|pair| pair[0] != pair[1]), "{days:?}");
    assert_eq!(days.iter().filter(|id| *id == "gog:never-a").count(), 3, "every other day: {days:?}");
    assert!(days.iter().all(|id| id == "gog:never-a" || id == "gog:old"), "the long untouched one in between: {days:?}");
    // With a single game to propose, that one is the pick every day
    let single = offline();
    for n in 0..3 {
        assert_eq!(single.home(&plugin, &config, Lang::It, &few[1..2], noon + n * day).games[0].id, "gog:never-a");
    }

    // With nothing waiting on disk: what was left untouched for months, then anything that
    // is installed, then what is not; finished, abandoned and hidden games never
    let pick = |games: &[DetectedGame], lang: Lang| {
        let view = offline().home(&plugin, &config, lang, games, noon);
        view.games.first().map(|pick| (pick.id.clone(), pick.note.clone().unwrap_or_default()))
    };
    let leftovers = [playing.clone(), old, finished.clone(), abandoned.clone(), hidden.clone(), absent.clone()];
    assert_eq!(
        pick(&leftovers, Lang::It),
        Some(("gog:old".to_string(), "Riscopri · non lo apri da 6 mesi".to_string()))
    );
    assert_eq!(
        pick(&leftovers[..1], Lang::It),
        Some(("gog:playing".to_string(), "Dalla tua libreria · 10 h di gioco".to_string()))
    );
    assert_eq!(pick(&[playing, absent.clone()], Lang::It).map(|(id, _)| id).as_deref(), Some("gog:playing"));
    assert_eq!(
        pick(&leftovers[2..], Lang::It),
        Some(("gog:absent".to_string(), "Dal backlog · mai giocato".to_string()))
    );
    // Marked "to play" and already tried: from the backlog, without saying it was never played
    let mut marked = other_game("gog:marked", "Da giocare");
    marked.status = Some(GameStatus::Backlog);
    marked.playtime_minutes = Some(20);
    assert_eq!(pick(&[marked], Lang::It), Some(("gog:marked".to_string(), "Dal backlog".to_string())));
    // Marked as being played, with no hours known to Kanso: not from the backlog, and not
    // ahead of a game that really waits
    let mut elsewhere = other_game("gog:elsewhere", "Giocato altrove");
    elsewhere.status = Some(GameStatus::Playing);
    assert_eq!(
        pick(std::slice::from_ref(&elsewhere), Lang::It),
        Some(("gog:elsewhere".to_string(), "Dalla tua libreria".to_string()))
    );
    assert_eq!(
        pick(&[elsewhere, never("gog:never-z", "Mai aperto Z")], Lang::It).map(|(id, _)| id).as_deref(),
        Some("gog:never-z")
    );

    // Nothing to propose: no part on Home, rather than an empty one
    let nothing = offline();
    assert!(nothing.home(&plugin, &config, Lang::It, &[finished, abandoned, hidden], noon).is_empty());
    assert!(nothing.home(&plugin, &config, Lang::It, &[], noon).is_empty());

    // In the interface's language
    let one = [absent];
    for (lang, title, note) in [
        (Lang::En, "Today's pick", "From your backlog · never played"),
        (Lang::Es, "El juego de hoy", "De tus pendientes · nunca jugado"),
        (Lang::Ru, "Игра дня", "Из отложенных · ещё не запускалась"),
    ] {
        let view = offline().home(&plugin, &config, lang, &one, noon);
        assert_eq!(view.title.as_deref(), Some(title));
        assert_eq!(view.games[0].note.as_deref(), Some(note));
    }
    // The units of the time played are words of the line as well
    for (minutes, lang, note) in [
        (45, Lang::Ru, "Из вашей библиотеки · 45 мин в игре"),
        (600, Lang::Ru, "Из вашей библиотеки · 10 ч в игре"),
        (45, Lang::En, "From your library · 45 min played"),
        (600, Lang::Es, "De tu biblioteca · 10 h de juego"),
    ] {
        let mut recent = other_game("gog:recent", "Di ieri");
        recent.playtime_minutes = Some(minutes);
        recent.last_played_at = Some(noon - day);
        let view = offline().home(&plugin, &config, lang, &[recent], noon);
        assert_eq!(view.games[0].note.as_deref(), Some(note));
    }

    // Kept for an hour, so that the pick changes soon after midnight
    assert_eq!(plugin.manifest.cache_hours, 1);
    let cached = offline();
    let ask = |now: u64| cached.engine.home_data(&cached.db, &plugin, &config, Lang::It, &games, false, now);
    assert_eq!(ask(noon).fetched_at, noon);
    assert_eq!(ask(noon + 59 * 60_000).fetched_at, noon, "within the hour: from the cache");
    assert_eq!(ask(noon + hour).fetched_at, noon + hour, "then asked again");

    assert!(bench.transport.urls().is_empty());
}

#[test]
fn steam_reviews_show_the_user_score_and_metacritic() {
    let bench = Bench::new(|request| {
        if request.url.starts_with("https://api.steampowered.com/IStoreBrowseService/GetItems/v1/") {
            if request.url.contains("990080") {
                // Live answer for Hogwarts Legacy, Italian
                ok_json(json!({"response": {"store_items": [{"item_type": 0, "id": 990080, "success": 1, "visible": true,
                    "name": "Hogwarts Legacy", "appid": 990080,
                    "reviews": {"summary_filtered": {"review_count": 278694, "percent_positive": 89, "review_score": 8,
                                                     "review_score_label": "Molto positive"},
                                "summary_language_specific": {"review_count": 1915, "percent_positive": 91,
                                                              "review_score": 8, "review_score_label": "Molto positive"}}}]}}))
            } else {
                // Live answer for an app the store does not know
                ok_json(json!({"response": {"store_items": [{"item_type": 0, "id": 0, "success": 15, "visible": false, "appid": 0}]}}))
            }
        } else if request.url == "https://store.steampowered.com/api/appdetails?appids=990080&filters=metacritic" {
            ok_json(json!({"990080": {"success": true, "data": {"metacritic": {"score": 83,
                "url": "https://www.metacritic.com/game/pc/hogwarts-legacy?ftag=MCD-06-10aaa1f"}}}}))
        } else {
            ok_json(json!({"1": {"success": false}}))
        }
    });
    let plugin = plugin("steam-reviews");
    let config = AppConfig::default();
    assert!(is_enabled(&plugin, &config), "Steam is already contacted by Kanso itself");

    let data = bench.details(&plugin, &config, Lang::It, &steam_game("990080", "Hogwarts Legacy"));
    let badge = data.badge.as_ref().expect("badge");
    assert_eq!(badge.text, "89%");
    assert_eq!(badge.tooltip.as_deref(), Some("Recensioni positive su Steam: Molto positive"));
    assert_eq!(badge.label.as_deref(), Some("Gradimento Steam"), "what the figure is");
    assert_eq!(data.sections[0].title.as_deref(), Some("Recensioni"));
    assert_eq!(
        data.sections[0].blocks[0],
        Block::Meter {
            label: "Utenti di Steam · Molto positive".into(),
            value: 89.0,
            max: 100.0,
            value_label: Some("89% positive · 278.694 recensioni".into()),
        }
    );
    assert_eq!(stat(&data, "Metacritic"), Some("83 / 100"));
    assert_eq!(
        links(&data),
        [
            "https://store.steampowered.com/app/990080/#app_reviews_hash",
            "https://www.metacritic.com/game/pc/hogwarts-legacy?ftag=MCD-06-10aaa1f"
        ]
    );
    let asked = &bench.transport.urls()[0];
    assert!(asked.contains("%22language%22%3A%22italian%22") && asked.contains("include_reviews"), "{asked}");

    // A game from another store uses the Steam page matched to it; one without a page asks nothing
    let mut on_gog = other_game("gog:hl", "Hogwarts Legacy");
    on_gog.steam_store_appid = Some("990080".to_string());
    assert_eq!(bench.details(&plugin, &config, Lang::En, &on_gog).badge.unwrap().text, "89%");
    let before = bench.transport.urls().len();
    assert!(bench.details(&plugin, &config, Lang::It, &other_game("gog:x", "Homebrew")).is_empty());
    assert_eq!(bench.transport.urls().len(), before);
    assert!(bench.details(&plugin, &config, Lang::It, &steam_game("1", "Unknown")).is_empty());
}

#[test]
fn steam_news_lists_the_latest_announcements_with_links_to_steam() {
    let bench = Bench::new(|request| {
        assert!(request.url.starts_with("https://api.steampowered.com/ISteamNews/GetNewsForApp/v2/?appid=990080"));
        // Live answer, trimmed
        ok_json(json!({"appnews": {"appid": 990080, "newsitems": [
            {"gid": "1848664643342127", "title": "Hogwarts Legacy PC Patch – November 5th, 2026",
             "url": "https://steamstore-a.akamaihd.net/news/externalpost/steam_community_announcements/1848664643342127",
             "is_external_url": true, "author": "mylivemtl",
             "contents": "Hogwarts Legacy Build Version: 2153883 (Steam, EGS) [b]DEVELOPER NOTES:[/b] This patch contains some PC specific bugs fixes {STEAM_CLAN_IMAGE}/123/abc.png https://example.org/x and more...",
             "feedlabel": "Community Announcements", "date": 1793896124, "feedname": "steam_community_announcements",
             "feed_type": 1, "appid": 990080, "tags": ["patchnotes"]},
            {"gid": "1845383656382343", "title": "WB Games' Autumn Sale is Here!",
             "url": "https://steamstore-a.akamaihd.net/news/externalpost/steam_community_announcements/1845383656382343",
             "is_external_url": true, "author": "x", "contents": "Get great deals during the Autumn Sale!",
             "feedlabel": "Community Announcements", "date": 1790874046, "feedname": "steam_community_announcements",
             "feed_type": 1, "appid": 990080}
        ], "count": 412}}))
    });
    let plugin = plugin("steam-news");
    let data = bench.details(&plugin, &config_with("steam-news", &[("count", "3")]), Lang::It, &steam_game("990080", "Hogwarts Legacy"));

    assert_eq!(data.badge, None);
    assert_eq!(data.sections[0].title.as_deref(), Some("Novità"));
    let Block::List { items } = &data.sections[0].blocks[0] else { panic!("list") };
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].title, "Hogwarts Legacy PC Patch – November 5th, 2026");
    let subtitle = items[0].subtitle.as_deref().unwrap();
    assert!(subtitle.starts_with("5 nov 2026 · Note della patch — Hogwarts Legacy Build Version"), "{subtitle}");
    assert!(!subtitle.contains('[') && !subtitle.contains("STEAM_CLAN") && !subtitle.contains("http"), "{subtitle}");
    assert_eq!(items[0].url.as_deref(), Some("https://store.steampowered.com/news/app/990080/view/1848664643342127"));
    assert!(items[1].subtitle.as_deref().unwrap().starts_with("1 ott 2026 — Get great deals"));
    assert_eq!(links(&data), ["https://store.steampowered.com/news/app/990080"]);
    assert!(bench.transport.urls()[0].contains("&count=3&"));
}

#[test]
fn howlongtobeat_finds_the_search_endpoint_gets_a_token_and_reads_the_times() {
    let bench = Bench::new(|request| {
        let url = request.url.as_str();
        let header = |name: &str| {
            request
                .headers
                .iter()
                .find(|(key, _)| key.eq_ignore_ascii_case(name))
                .map(|(_, value)| value.as_str())
        };
        assert!(header("User-Agent").is_some_and(|ua| ua.contains("Mozilla")), "{url}");
        assert_eq!(header("Referer"), Some("https://howlongtobeat.com/"), "{url}");

        if url == "https://howlongtobeat.com/" {
            status(200, r#"<html><head>
                <script src="https://cdn.example.net/analytics.js"></script>
                <script src="/_next/static/chunks/webpack-1a2b.js" defer=""></script>
                <script defer="" src="/_next/static/chunks/pages/_app-9f8e.js"></script>
                </head></html>"#)
        } else if url.ends_with("webpack-1a2b.js") {
            status(200, "self.webpackChunk=[];fetch('/static/other.json');")
        } else if url.ends_with("_app-9f8e.js") {
            status(200, r#"let r=await fetch("/api/seek/d4b2e330",{method:"POST",headers:{"Content-Type":"application/json"},body:JSON.stringify(e)});"#)
        } else if url.starts_with("https://howlongtobeat.com/api/seek/d4b2e330/init?t=") {
            ok_json(json!({"token": "tok-123", "hpKey": "fx_field", "hpVal": "fx_value"}))
        } else if url == "https://howlongtobeat.com/api/seek/d4b2e330" {
            assert_eq!(request.method, "POST");
            assert_eq!(header("x-auth-token"), Some("tok-123"));
            assert_eq!(header("x-hp-key"), Some("fx_field"));
            assert_eq!(header("x-hp-val"), Some("fx_value"));
            assert_eq!(header("Origin"), Some("https://howlongtobeat.com"));
            let body: Value = serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
            assert_eq!(body["searchType"], "games");
            assert_eq!(body["fx_field"], "fx_value", "the site's extra field travels in the body too");
            let terms: Vec<&str> = body["searchTerms"].as_array().unwrap().iter().filter_map(Value::as_str).collect();
            if terms == ["Hogwarts", "Legacy"] {
                ok_json(json!({"count": 2, "data": [
                    {"game_id": 99999, "game_name": "Hogwarts Legacy: Dark Arts Pack", "game_alias": "", "comp_main": 3600},
                    {"game_id": 80368, "game_name": "Hogwarts Legacy", "game_alias": "", "game_type": "game",
                     "comp_main": 95400, "comp_plus": 180000, "comp_100": 250200, "comp_all": 160200, "review_score": 83}
                ]}))
            } else {
                ok_json(json!({"count": 0, "data": []}))
            }
        } else {
            status(404, "not found")
        }
    });
    let plugin = plugin("howlongtobeat");
    let config = config_with("howlongtobeat", &[]);
    assert!(!is_enabled(&plugin, &AppConfig::default()), "a third-party site: off until the user switches it on");

    let data = bench.details(&plugin, &config, Lang::It, &steam_game("990080", "Hogwarts Legacy"));
    assert_eq!(stat(&data, "Storia principale"), Some("27 h"), "26.5 h: whole hours from 20 h up");
    assert_eq!(stat(&data, "Storia + extra"), Some("50 h"));
    assert_eq!(stat(&data, "Completista"), Some("70 h"));
    assert_eq!(stat(&data, "Tutti gli stili"), Some("45 h"));
    let badge = data.badge.as_ref().expect("badge");
    assert_eq!((badge.text.as_str(), badge.tooltip.as_deref()), ("27 h", Some("HowLongToBeat · Storia principale")));
    assert_eq!(badge.label.as_deref(), Some("Per finirlo"), "what the figure is");
    assert_eq!(links(&data), ["https://howlongtobeat.com/game/80368"]);

    // Homepage, the two scripts, the token, the search: five requests for the first game
    assert_eq!(bench.transport.urls().len(), 5, "{:?}", bench.transport.urls());
    // The endpoint and the token are remembered: the next game costs one request, and a game
    // the site does not know simply shows nothing
    assert!(bench.details(&plugin, &config, Lang::It, &other_game("gog:x", "Totally Unknown")).is_empty());
    assert_eq!(bench.transport.urls().len(), 6, "{:?}", bench.transport.urls());
}

#[test]
fn howlongtobeat_looks_the_endpoint_up_again_when_the_remembered_one_stops_answering() {
    let bench = Bench::new(|request| {
        let url = request.url.as_str();
        if url == "https://howlongtobeat.com/" {
            status(200, r#"<script src="/_next/static/chunks/app.js"></script>"#)
        } else if url.ends_with("/app.js") {
            status(200, r#"fetch("/api/find/new1",{method:"POST",body:x})"#)
        } else if url.contains("/api/find/new1/init") {
            ok_json(json!({"token": "fresh"}))
        } else if url == "https://howlongtobeat.com/api/find/new1" {
            ok_json(json!({"data": [{"game_id": 1, "game_name": "Hades", "comp_main": 1800, "comp_plus": 45000, "comp_100": 0}]}))
        } else {
            status(404, "gone")
        }
    });
    let plugin = plugin("howlongtobeat");
    // What an earlier run remembered: an endpoint that has since moved
    lock(&bench.db)
        .save_plugin_storage(
            "howlongtobeat",
            &BTreeMap::from([
                ("searchPath".to_string(), json!({"path": "api/old/zzz", "at": 4_000_000_000_000_u64}).to_string()),
                ("token".to_string(), json!({"path": "api/old/zzz", "at": 4_000_000_000_000_u64, "token": "stale", "key": null, "value": null}).to_string()),
            ]),
        )
        .unwrap();

    let data = bench.details(&plugin, &config_with("howlongtobeat", &[]), Lang::En, &other_game("gog:hades", "Hades"));
    assert_eq!(stat(&data, "Main story"), Some("30 min"));
    assert_eq!(stat(&data, "Main + extras"), Some("12.5 h"), "half hours below 20 h, with the language's decimal mark");
    assert_eq!(stat(&data, "Completionist"), None, "zero means the site has no data");
    let urls = bench.transport.urls();
    assert_eq!(urls[0], "https://howlongtobeat.com/api/old/zzz");
    assert_eq!(urls.last().map(String::as_str), Some("https://howlongtobeat.com/api/find/new1"));

    let stored = lock(&bench.db).load_plugin_storage("howlongtobeat").unwrap();
    assert!(stored["searchPath"].contains("api/find/new1"), "{stored:?}");
}

/// Source of a wiki page in the shape PCGamingWiki's articles have.
const PCGW_PAGE: &str = "{{Infobox game\n|cover = Hogwarts Legacy cover.jpg\n|developers =\n{{Infobox game/row/developer|Avalanche Software}}\n}}\n\n==Video==\n{{Video\n|wsgf link                  = \n|ultrawidescreen wsgf award = \n|multimonitor               = true\n|ultrawidescreen            = true\n|ultrawidescreen notes      = Cutscenes are {{term|pillarboxed}}.\n|4k ultra hd                = true\n|120 fps                    = true\n|hdr                        = hackable\n|hdr notes                  = Use [[Special K]].\n|ray tracing                = unknown\n|color blind                = false\n}}\n\n==Input==\n{{Input\n|key remap                 = true\n|controller support        = true\n|full controller           = false\n|controller remap          = limited\n}}\n";

fn wiki_page(title: &str, text: &str) -> Result<HttpResponse, String> {
    ok_json(json!({"batchcomplete": true, "query": {"pages": [{"pageid": 148771, "ns": 0, "title": title,
        "revisions": [{"slots": {"main": {"contentmodel": "wikitext", "contentformat": "text/x-wiki", "content": text}}}]}]}}))
}

fn redirect(location: &str) -> Result<HttpResponse, String> {
    Ok(HttpResponse {
        status: 302,
        headers: vec![("location".to_string(), location.to_string())],
        body: String::new(),
    })
}

#[test]
fn pcgamingwiki_finds_the_page_and_reads_the_facts_from_its_source() {
    let bench = Bench::new(|request| {
        let url = request.url.as_str();
        if url.contains("cargoquery") {
            // What the wiki answers to anonymous Cargo queries
            ok_json(json!({"error": {"code": "permissiondenied", "info": "You don't have permission to run arbitrary Cargo queries."}}))
        } else if url == "https://www.pcgamingwiki.com/api/appid.php?appid=990080" {
            redirect("https://www.pcgamingwiki.com/wiki/Hogwarts_Legacy")
        } else if url.contains("prop=revisions") && url.ends_with("&titles=Hogwarts%20Legacy") {
            wiki_page("Hogwarts Legacy", PCGW_PAGE)
        } else if url.contains("prop=revisions") && url.ends_with("&titles=Disco%20Elysium") {
            // Older answer shape: pages by id, the text under "*"
            ok_json(json!({"query": {"pages": {"77": {"pageid": 77, "title": "Disco Elysium",
                "revisions": [{"*": "{{Video|ultrawidescreen = false}}\n{{Input\n|controller support = true\n}}"}]}}}}))
        } else if url.contains("prop=revisions") {
            ok_json(json!({"batchcomplete": true, "query": {"pages": [{"ns": 0, "title": "Whatever", "missing": true}]}}))
        } else {
            status(404, "not found")
        }
    });
    let plugin = plugin("pcgamingwiki");
    let config = config_with("pcgamingwiki", &[]);

    let data = bench.details(&plugin, &config, Lang::It, &steam_game("990080", "Hogwarts Legacy"));
    assert_eq!(data.sections[0].title.as_deref(), Some("PCGamingWiki"));
    assert_eq!(stat(&data, "Ultrawide"), Some("Sì"));
    assert_eq!(stat(&data, "4K"), Some("Sì"));
    assert_eq!(stat(&data, "120+ FPS"), Some("Sì"));
    assert_eq!(stat(&data, "HDR"), Some("Con una modifica"));
    assert_eq!(stat(&data, "Ray tracing"), None, "unknown values are left out");
    assert_eq!(stat(&data, "Controller"), Some("Sì"));
    assert_eq!(stat(&data, "Controller: supporto completo"), Some("No"));
    assert_eq!(links(&data), ["https://www.pcgamingwiki.com/wiki/Hogwarts_Legacy"]);
    let urls = bench.transport.urls();
    assert_eq!(urls.len(), 2, "the redirect, then the page: {urls:?}");
    assert!(urls.iter().all(|url| !url.contains("cargoquery")), "{urls:?}");

    // No Steam page: found by title
    let data = bench.details(&plugin, &config, Lang::En, &other_game("gog:disco", "Disco Elysium"));
    assert_eq!(stat(&data, "Ultrawide"), Some("No"));
    assert_eq!(stat(&data, "Controller"), Some("Yes"));
    assert_eq!(links(&data), ["https://www.pcgamingwiki.com/wiki/Disco_Elysium"]);

    // A game the wiki does not have shows nothing
    assert!(bench.details(&plugin, &config, Lang::En, &other_game("gog:none", "Nothing Like This")).is_empty());
}

#[test]
fn pcgamingwiki_still_links_to_the_wiki_when_its_api_refuses() {
    let hour = 3_600_000;
    let expires = |bench: &Bench, id: &str| {
        lock(&bench.db).get_plugin_cache("pcgamingwiki", id).unwrap().unwrap().expires_at - 1_000
    };
    let plugin = plugin("pcgamingwiki");
    let config = config_with("pcgamingwiki", &[]);

    // The redirect works, reading the page does not (what the wiki answers to a refused action)
    let bench = Bench::new(|request| {
        if request.url.contains("/api/appid.php") {
            redirect("/wiki/Hogwarts_Legacy")
        } else {
            ok_json(json!({"error": {"code": "readapidenied", "info": "You need read permission to use this module."}}))
        }
    });
    let game = steam_game("990080", "Hogwarts Legacy");
    let data = bench.details(&plugin, &config, Lang::It, &game);
    assert_eq!(links(&data), ["https://www.pcgamingwiki.com/wiki/Hogwarts_Legacy"]);
    assert_eq!(stat(&data, "Ultrawide"), None);
    assert_eq!(expires(&bench, &game.id), 12 * hour, "asked again soon, not in a month");

    // Nothing answers at all: the link goes through the wiki's own redirect, or its search
    let bench = Bench::new(|_| Err("connection refused".to_string()));
    let data = bench.details(&plugin, &config, Lang::It, &game);
    assert_eq!(links(&data), ["https://www.pcgamingwiki.com/api/appid.php?appid=990080"]);
    let other = other_game("gog:disco", "Disco Elysium");
    let data = bench.details(&plugin, &config, Lang::It, &other);
    assert_eq!(links(&data), ["https://www.pcgamingwiki.com/w/index.php?search=Disco%20Elysium"]);
    assert_eq!(expires(&bench, &other.id), 12 * hour);

    // An appid the wiki does not know (no redirect to a page) falls back to the title
    let bench = Bench::new(|request| {
        if request.url.contains("/api/appid.php") {
            redirect("https://www.pcgamingwiki.com/wiki/Special:Search?search=1")
        } else if request.url.ends_with("&titles=Tiny%20Indie") {
            wiki_page("Tiny Indie", "{{Video\n|hdr = true\n}}")
        } else {
            status(500, "")
        }
    });
    let data = bench.details(&plugin, &config, Lang::En, &steam_game("123", "Tiny Indie"));
    assert_eq!(stat(&data, "HDR"), Some("Yes"));
    assert_eq!(links(&data), ["https://www.pcgamingwiki.com/wiki/Tiny_Indie"]);
}

#[test]
fn price_history_needs_the_users_key_and_shows_current_and_lowest_price() {
    let bench = Bench::new(|request| {
        let url = request.url.as_str();
        assert!(url.starts_with("https://api.isthereanydeal.com/"), "{url}");
        if !url.contains("key=good-key") {
            return status(403, r#"{"status_code":403,"reason_phrase":"Invalid or expired api key"}"#);
        }
        if url.starts_with("https://api.isthereanydeal.com/games/lookup/v1?appid=990080") {
            ok_json(json!({"found": true, "game": {"id": "018d937f-21e1-728e-86d7-9acb3c59f2bb",
                "slug": "hogwarts-legacy", "title": "Hogwarts Legacy", "type": "game", "mature": false}}))
        } else if url.starts_with("https://api.isthereanydeal.com/games/overview/v2?country=IT") {
            assert_eq!(request.method, "POST");
            assert_eq!(request.body.as_deref(), Some(r#"["018d937f-21e1-728e-86d7-9acb3c59f2bb"]"#));
            ok_json(json!({"prices": [{"id": "018d937f-21e1-728e-86d7-9acb3c59f2bb",
                "current": {"shop": {"id": 61, "name": "Steam"}, "price": {"amount": 17.99, "amountInt": 1799, "currency": "EUR"},
                            "regular": {"amount": 59.99, "amountInt": 5999, "currency": "EUR"}, "cut": 70,
                            "timestamp": "2026-11-10T19:02:11+01:00", "url": "https://itad.link/018d937f/"},
                "lowest": {"shop": {"id": 35, "name": "GOG"}, "price": {"amount": 11.99, "amountInt": 1199, "currency": "EUR"},
                           "regular": {"amount": 59.99, "amountInt": 5999, "currency": "EUR"}, "cut": 80,
                           "timestamp": "2026-06-26T19:15:44+02:00"},
                "bundled": 0, "urls": {"game": "https://isthereanydeal.com/game/hogwarts-legacy/info/"}}], "bundles": []}))
        } else {
            ok_json(json!({"found": false}))
        }
    });
    let plugin = plugin("price-history");
    let game = steam_game("990080", "Hogwarts Legacy");

    // Without a key: an explanation and where to get one, and not a single request
    let data = bench.details(&plugin, &config_with("price-history", &[]), Lang::It, &game);
    assert!(matches!(&data.sections[0].blocks[0], Block::Notice { tone, .. } if tone == "warning"));
    assert_eq!(links(&data), ["https://isthereanydeal.com/apps/my/"]);
    assert!(bench.transport.urls().is_empty());

    let data = bench.details(&plugin, &config_with("price-history", &[("apiKey", " good-key ")]), Lang::It, &game);
    assert_eq!(data.sections[0].title.as_deref(), Some("Prezzi"));
    assert_eq!(stat(&data, "Miglior prezzo ora"), Some("17,99 EUR · −70% · Steam"));
    assert_eq!(stat(&data, "Minimo storico"), Some("11,99 EUR · −80% · GOG"));
    assert_eq!(
        links(&data),
        ["https://isthereanydeal.com/game/hogwarts-legacy/info/", "https://itad.link/018d937f/"]
    );

    // The game's id was remembered: only the prices are asked the second time
    let before = bench.transport.urls().len();
    bench.details(&plugin, &config_with("price-history", &[("apiKey", "good-key"), ("country", "us")]), Lang::En, &game);
    let urls = bench.transport.urls();
    assert_eq!(urls.len(), before + 1);
    assert!(urls.last().unwrap().contains("country=US"));

    // A refused key is explained instead of failing; an unknown game shows nothing
    let data = bench.details(&plugin, &config_with("price-history", &[("apiKey", "wrong")]), Lang::En, &other_game("gog:q", "Q"));
    assert!(matches!(&data.sections[0].blocks[0], Block::Notice { text, .. } if text.contains("refused")));
    let nothing = bench.details(&plugin, &config_with("price-history", &[("apiKey", "good-key")]), Lang::En, &other_game("gog:q", "Q"));
    assert!(nothing.is_empty());
}

/// Nexus Mods' public list of games, with the fields the real file has.
fn nexus_list() -> String {
    let game = |id: u32, name: &str, domain: &str, mods: u32| {
        json!({
            "id": id, "name": name, "name_lower": name.to_lowercase(),
            "forum_url": "https://forums.nexusmods.com/", "nexusmods_url": format!("https://www.nexusmods.com/{domain}"),
            "genre": "RPG", "file_count": 10, "downloads": 1000, "domain_name": domain,
            "approved_date": 1, "mods": mods, "collections": 2
        })
    };
    json!([
        game(110, "Skyrim", "skyrim", 73225),
        game(1704, "Skyrim Special Edition", "skyrimspecialedition", 144507),
        game(952, "The Witcher 3: Wild Hunt", "witcher3", 10935),
        game(1151, "Fallout 4", "fallout4", 78035),
        game(130, "Fallout New Vegas", "newvegas", 42545),
        game(3474, "Baldur's Gate 3", "baldursgate3", 20495),
        game(1627, "Dark Souls 3", "darksouls3", 1702),
        game(7001, "The Elder Scrolls VI: Sentinel", "sentinel", 0),
        game(7002, "Odd | Name", "../odd", 5),
        "not an entry"
    ])
    .to_string()
}

#[test]
fn nexus_mods_counts_the_mods_of_a_game_without_naming_it_to_anyone() {
    let bench = Bench::new(|request| {
        assert_eq!(request.url, "https://data.nexusmods.com/file/nexus-data/games.json", "the public list, and nothing else");
        assert_eq!(request.method, "GET");
        let header = |name: &str| request.headers.iter().find(|(key, _)| key.eq_ignore_ascii_case(name)).map(|(_, value)| value.as_str());
        assert_eq!(header("Application-Name"), Some("Kanso"), "it says who is asking, as Nexus Mods asks");
        assert_eq!(header("Application-Version"), Some("0.1.0"));
        status(200, &nexus_list())
    });
    let plugin = plugin("nexus-mods");
    assert_eq!(plugin.manifest.hosts, ["data.nexusmods.com", "api.nexusmods.com"]);
    assert!(!is_enabled(&plugin, &AppConfig::default()), "it contacts a new site: the user turns it on");
    let config = config_with("nexus-mods", &[]);

    // The same name, punctuation aside
    let fallout = other_game("gog:fnv", "Fallout: New Vegas");
    let data = bench.details(&plugin, &config, Lang::It, &fallout);
    assert_eq!(stat(&data, "Mod disponibili"), Some("42.545"));
    assert_eq!(links(&data), ["https://www.nexusmods.com/games/newvegas/mods"], "one link: the game's mods page");
    assert_eq!(data.badge, None, "nothing on the cards: only in the sheet");
    assert_eq!(bench.transport.urls().len(), 1);

    // The list is kept: the next games ask nobody
    // A store's long title, Nexus Mods' everyday name: the edition must not be lost on the way
    let mut skyrim = steam_game("489830", "The Elder Scrolls V: Skyrim Special Edition");
    skyrim.clean_title = "The Elder Scrolls V: Skyrim".to_string();
    let data = bench.details(&plugin, &config, Lang::En, &skyrim);
    assert_eq!(stat(&data, "Mods available"), Some("144,507"), "Special Edition, not the 2011 game");
    assert_eq!(links(&data), ["https://www.nexusmods.com/games/skyrimspecialedition/mods"]);
    let hint = data.sections.iter().flat_map(|s| &s.blocks).find_map(|block| match block {
        Block::Stats { items } => items[0].hint.clone(),
        _ => None,
    });
    assert_eq!(hint.as_deref(), Some("On Nexus Mods as “Skyrim Special Edition”"), "a match under another name says so");
    // An edition suffix the store adds
    let mut witcher = other_game("gog:w3", "The Witcher 3: Wild Hunt - Game of the Year Edition");
    witcher.clean_title = "The Witcher 3: Wild Hunt".to_string();
    assert_eq!(links(&bench.details(&plugin, &config, Lang::It, &witcher)), ["https://www.nexusmods.com/games/witcher3/mods"]);
    // A number written the Roman way in one place and with digits in the other
    let souls = steam_game("374320", "DARK SOULS III");
    assert_eq!(links(&bench.details(&plugin, &config, Lang::It, &souls)), ["https://www.nexusmods.com/games/darksouls3/mods"]);
    assert_eq!(bench.transport.urls().len(), 1, "still the one download");

    // No page for the game, no mods for it, or a malformed entry: nothing is shown
    for title in ["Celeste", "The Elder Scrolls VI: Sentinel", "Odd Name", "42"] {
        let answer = bench.engine.game_data(&bench.db, &plugin, &config, Lang::It, &other_game("gog:x", title), true, 2_000);
        assert_eq!(answer.error, None, "{title}");
        assert!(answer.data.is_none_or(|data| data.is_empty()), "{title}");
    }

    // A game file run by an emulator is left alone
    let mut emulated = other_game("emu:dolphin:1", "Fallout 4");
    emulated.platform = GamePlatform::Emulator;
    let answer = bench.engine.game_data(&bench.db, &plugin, &config, Lang::It, &emulated, true, 3_000);
    assert!(answer.error.is_none() && answer.data.is_none_or(|data| data.is_empty()));
    assert_eq!(bench.transport.urls().len(), 1);
}

/// What Nexus Mods' public API answers a search with, shaped as its schema says.
fn nexus_api_answer(request: &HttpRequest, games: &[(&str, &str, u32)]) -> String {
    let body: serde_json::Value = serde_json::from_str(request.body.as_deref().expect("a body")).expect("json");
    let query = body["query"].as_str().expect("a query");
    assert!(query.contains("games(filter: { name: [{ value: $n0, op: ") && query.contains("nodes { name domainName modCount }"), "{query}");
    let nodes: Vec<serde_json::Value> = games
        .iter()
        .map(|(name, domain, mods)| json!({"name": name, "domainName": domain, "modCount": mods}))
        .collect();
    // One alias per name searched for: the first finds the games, the others nothing
    let mut data = serde_json::Map::new();
    for index in 0..body["variables"].as_object().expect("variables").len() {
        data.insert(format!("s{index}"), json!({"nodes": if index == 0 { nodes.clone() } else { Vec::new() }}));
    }
    json!({"data": data}).to_string()
}

#[test]
fn nexus_mods_asks_about_the_one_game_when_the_list_is_missing() {
    let bench = Bench::new(|request| {
        if request.url == "https://data.nexusmods.com/file/nexus-data/games.json" {
            // As seen in the wild: the public file answering "not found" for an hour and more
            return status(404, "<!doctype html><title>Not Found</title>");
        }
        assert_eq!((request.method.as_str(), request.url.as_str()), ("POST", "https://api.nexusmods.com/v2/graphql"));
        let header = |name: &str| request.headers.iter().find(|(key, _)| key.eq_ignore_ascii_case(name)).map(|(_, value)| value.as_str());
        assert_eq!((header("Application-Name"), header("Content-Type")), (Some("Kanso"), Some("application/json")));
        assert!(header("Authorization").is_none() && header("apikey").is_none(), "no account, no key");
        let body = request.body.as_deref().unwrap_or_default();
        if body.contains("Witcher") {
            assert!(body.contains("*The Witcher 3: Wild Hunt*") || body.contains("\"The Witcher 3: Wild Hunt\""), "{body}");
            status(200, &nexus_api_answer(request, &[("The Witcher 2", "witcher2", 660), ("The Witcher 3: Wild Hunt", "witcher3", 10935)]))
        } else if body.contains("Celeste") {
            status(200, &nexus_api_answer(request, &[]))
        } else {
            status(200, &nexus_api_answer(request, &[("Skyrim", "skyrim", 73225), ("Skyrim Special Edition", "skyrimspecialedition", 144507)]))
        }
    });
    let plugin = plugin("nexus-mods");
    let config = config_with("nexus-mods", &[]);

    let witcher = other_game("gog:w3", "The Witcher 3: Wild Hunt");
    let data = bench.details(&plugin, &config, Lang::It, &witcher);
    assert_eq!(stat(&data, "Mod disponibili"), Some("10.935"));
    assert_eq!(links(&data), ["https://www.nexusmods.com/games/witcher3/mods"]);
    assert_eq!(
        bench.transport.urls(),
        ["https://data.nexusmods.com/file/nexus-data/games.json", "https://api.nexusmods.com/v2/graphql"],
        "the list first; the API only because there is none"
    );

    // The missing list is not asked for again at every game
    let mut skyrim = steam_game("489830", "The Elder Scrolls V: Skyrim Special Edition");
    skyrim.clean_title = "The Elder Scrolls V: Skyrim".to_string();
    let data = bench.details(&plugin, &config, Lang::It, &skyrim);
    assert_eq!(links(&data), ["https://www.nexusmods.com/games/skyrimspecialedition/mods"], "the edition, not the 2011 game");
    assert_eq!(bench.transport.urls().len(), 3, "{:?}", bench.transport.urls());

    // A game Nexus Mods does not know: both ways of searching are tried, then nothing is shown
    let answer = bench.engine.game_data(&bench.db, &plugin, &config, Lang::It, &other_game("gog:c", "Celeste"), true, 2_000);
    assert!(answer.error.is_none() && answer.data.is_none_or(|data| data.is_empty()));
    assert_eq!(bench.transport.urls().len(), 5);
}

#[test]
fn nexus_mods_uses_an_old_list_rather_than_name_a_game() {
    let fail = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let failing = Arc::clone(&fail);
    let bench = Bench::new(move |request| {
        assert_eq!(request.url, "https://data.nexusmods.com/file/nexus-data/games.json", "never the API while a list is kept");
        if failing.load(std::sync::atomic::Ordering::SeqCst) {
            status(404, "gone")
        } else {
            status(200, &nexus_list())
        }
    });
    let plugin = plugin("nexus-mods");
    let config = config_with("nexus-mods", &[]);
    assert_eq!(links(&bench.details(&plugin, &config, Lang::It, &other_game("gog:f4", "Fallout 4"))), ["https://www.nexusmods.com/games/fallout4/mods"]);

    // The file disappears; the list kept from before answers, whatever its age
    fail.store(true, std::sync::atomic::Ordering::SeqCst);
    let data = bench.details(&plugin, &config, Lang::It, &other_game("gog:bg3", "Baldur's Gate 3"));
    assert_eq!(stat(&data, "Mod disponibili"), Some("20.495"));
    assert_eq!(bench.transport.urls().len(), 1, "within the week the kept list is not even checked");
}

#[test]
fn nexus_mods_says_so_when_neither_the_list_nor_the_api_answers() {
    let bench = Bench::new(|_| status(503, "busy"));
    let plugin = plugin("nexus-mods");
    let config = config_with("nexus-mods", &[]);
    let answer = bench.engine.game_data(&bench.db, &plugin, &config, Lang::It, &other_game("gog:f4", "Fallout 4"), true, 1_000);
    assert!(answer.data.is_none(), "an error, not an empty section");
    let error = answer.error.expect("an error");
    assert!(error.contains("HTTP 503") && error.contains("list of games") && error.contains("API"), "it says what failed: {error}");
}
