use crate::cleaner::{clean_game_title, is_placeholder_title};
use crate::i18n::{Lang, T};
use crate::models::{now_timestamp_ms, DetectedGame, GamePlatform, SgdbGameMatch};
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

const DEFAULT_SGDB_BASE_URL: &str = "https://www.steamgriddb.com/api/v2";
const DEFAULT_STEAM_STORE_BASE_URL: &str = "https://store.steampowered.com";
const DEFAULT_STEAM_COMMUNITY_BASE_URL: &str = "https://steamcommunity.com";
const DEFAULT_STEAM_API_BASE_URL: &str = "https://api.steampowered.com";
const DEFAULT_STEAM_ASSETS_BASE_URL: &str =
    "https://shared.akamai.steamstatic.com/store_item_assets";

/// Metadata extracted from the public Steam Store API (`&l=italian`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SteamStoreMetadata {
    pub steam_appid: String,
    /// Official store name of the app, used to replace `appid_<id>` placeholder titles.
    pub name: Option<String>,
    /// Store classification, lower-cased: `game`, `dlc`, `music`, `application`, ...
    pub app_type: Option<String>,
    pub short_description: Option<String>,
    pub genres: Vec<String>,
}

/// Answer of the Steam Store API about one app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SteamStoreLookup {
    Found(SteamStoreMetadata),
    /// The store answered and has no page for this app (delisted, tool, no search hit).
    NotListed,
    /// The store could not be reached or sent an unusable answer: worth retrying later.
    Unavailable,
}

/// What an automatic metadata pass did to a game.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EnrichOutcome {
    /// Something the user can see changed (title, cover, description, genres, visibility).
    pub changed: bool,
    /// This pass learned the app is not a game (soundtrack, DLC, ...) and hid it.
    pub auto_hidden: bool,
    /// Non-fatal SteamGridDB problem (bad API key, network error) worth surfacing.
    pub warning: Option<String>,
}

/// How many of a game's Steam community tags are kept, most relevant first.
const MAX_GAME_TAGS: usize = 15;

/// How long to wait before automatically retrying a game whose metadata is still incomplete.
pub const METADATA_RETRY_INTERVAL_MS: u64 = 24 * 60 * 60 * 1000;

/// Steam store app types that are not something to launch from a game library.
const NON_GAME_APP_TYPES: &[&str] = &[
    "music",
    "dlc",
    "video",
    "episode",
    "series",
    "advertising",
    "hardware",
];

/// `true` while automatic lookup still has something to find for `game`: a cover (Steam games
/// can always get Steam's own artwork, other stores need a SteamGridDB key), description and
/// genres, the official name behind a provisional title, the store type of a Steam app, its
/// community tags, or texts cached in a language other than `lang`. Hidden games are left alone.
pub fn needs_metadata(game: &DetectedGame, has_api_key: bool, lang: Lang) -> bool {
    // A game that is not on disk is completed when its sheet is opened: a library can hold
    // hundreds of them, and looking each one up unasked would be a download nobody wanted
    if game.is_hidden || !game.is_installed {
        return false;
    }
    let missing_cover = cover_is_missing(game) && (has_api_key || steam_appid_of(game).is_some());
    let missing_details = game.short_description.is_none() && game.genres.is_empty();
    let steam_type_unknown = game.app_type.is_none() && steam_appid_of(game).is_some();
    missing_cover
        || missing_details
        || game.title_provisional
        || steam_type_unknown
        || needs_tags(game, lang)
        || needs_store_page(game)
        || needs_translation(game, lang)
}

/// Whether an automatic metadata pass should run for `game` now. Incomplete games are retried
/// at most once per [`METADATA_RETRY_INTERVAL_MS`] unless `retry_now` (the user asked for a
/// scan, changed language, or just configured an API key).
pub fn is_metadata_sync_due(
    game: &DetectedGame,
    has_api_key: bool,
    lang: Lang,
    now_ms: u64,
    retry_now: bool,
) -> bool {
    needs_metadata(game, has_api_key, lang)
        && (retry_now
            || game.metadata_attempted_at == 0
            || now_ms.saturating_sub(game.metadata_attempted_at) >= METADATA_RETRY_INTERVAL_MS)
}

fn cover_is_missing(game: &DetectedGame) -> bool {
    game.cover_path
        .as_deref()
        .is_none_or(|p| !Path::new(p).is_file())
}

/// Automatically fetched description/genres exist but in another language. Texts the user
/// edited by hand are never replaced.
fn needs_translation(game: &DetectedGame, lang: Lang) -> bool {
    !game.custom_override
        && (game.short_description.is_some() || !game.genres.is_empty())
        && game.metadata_lang.as_deref() != Some(lang.code())
}

/// Tags were never fetched, or are in another language.
fn needs_tags(game: &DetectedGame, lang: Lang) -> bool {
    game.tags_lang.as_deref() != Some(lang.code())
}

/// A game from another store whose Steam store page was never looked up.
fn needs_store_page(game: &DetectedGame) -> bool {
    game.platform != GamePlatform::Steam && game.steam_store_appid.is_none()
}

fn steam_appid_of(game: &DetectedGame) -> Option<&str> {
    if game.platform != GamePlatform::Steam {
        return None;
    }
    game.platform_app_id
        .as_deref()
        .map(str::trim)
        .filter(|id| !id.is_empty())
}

/// Apps asked about in one request to the store service.
const STEAM_SUMMARY_BATCH: usize = 50;
/// `type` of a game in the store service's answers (4 is DLC, 11 a soundtrack, ...).
const STEAM_STORE_TYPE_GAME: u64 = 0;

/// What the store says about a Steam app: enough to decide whether to list it, and to show it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SteamAppSummary {
    pub appid: u64,
    /// `None` when the store does not list the app (delisted, a tool, region-locked).
    pub name: Option<String>,
    pub is_game: bool,
    /// Address of the vertical library cover on Steam's CDN (the landscape header when the
    /// game has no vertical art).
    pub cover_url: Option<String>,
}

/// Outcome of matching a detected game against SteamGridDB.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SgdbResolution {
    pub game: SgdbGameMatch,
    /// `true` when the match came from `GET /games/steam/{appid}` (authoritative for that
    /// Steam app), `false` when it is only the first hit of a text search.
    pub exact_steam_match: bool,
}

/// Official artwork of a Steam app, as full URLs.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SteamArtwork {
    /// Vertical 600x900 library capsule: what the Steam client shows in its library grid.
    pub library_capsule_url: Option<String>,
    /// Landscape 460x215 store header, available for practically every app.
    pub header_url: Option<String>,
}

/// HTTP client for SteamGridDB API v2 and the public Steam Store API.
#[derive(Debug, Clone)]
pub struct MetadataClient {
    sgdb_base_url: String,
    steam_store_base_url: String,
    steam_community_base_url: String,
    steam_api_base_url: String,
    steam_assets_base_url: String,
    /// Language descriptions, genres and tags are requested in.
    lang: Lang,
    /// Names of Steam's community tags in `lang`, fetched once on first use (`None` inside:
    /// the list could not be loaded).
    tag_names: Arc<OnceLock<Option<HashMap<u64, String>>>>,
    agent: ureq::Agent,
}

impl Default for MetadataClient {
    fn default() -> Self {
        Self::new()
    }
}

impl MetadataClient {
    pub fn new() -> Self {
        let mut client = Self::with_base_urls(
            DEFAULT_SGDB_BASE_URL,
            DEFAULT_STEAM_STORE_BASE_URL,
            DEFAULT_STEAM_COMMUNITY_BASE_URL,
        );
        client.steam_api_base_url = DEFAULT_STEAM_API_BASE_URL.to_string();
        client.steam_assets_base_url = DEFAULT_STEAM_ASSETS_BASE_URL.to_string();
        client
    }

    /// Client against custom endpoints (tests, mirrors). Steam's Web API and its artwork CDN
    /// are expected on the store base URL (`/IStoreBrowseService/...`, `/store_item_assets/...`).
    pub fn with_base_urls(
        sgdb_base_url: &str,
        steam_store_base_url: &str,
        steam_community_base_url: &str,
    ) -> Self {
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_secs(8))
            .timeout_read(Duration::from_secs(15))
            .user_agent("KansoLauncher/0.1.0")
            .build();

        let steam_store_base_url = steam_store_base_url.trim_end_matches('/').to_string();
        Self {
            sgdb_base_url: sgdb_base_url.trim_end_matches('/').to_string(),
            steam_api_base_url: steam_store_base_url.clone(),
            steam_assets_base_url: format!("{steam_store_base_url}/store_item_assets"),
            steam_store_base_url,
            steam_community_base_url: steam_community_base_url.trim_end_matches('/').to_string(),
            lang: Lang::It,
            tag_names: Arc::default(),
            agent,
        }
    }

    /// Requests descriptions and genres in `lang` from now on.
    pub fn with_language(mut self, lang: Lang) -> Self {
        self.lang = lang;
        self.tag_names = Arc::default();
        self
    }

    /// Queries the dedicated SteamGridDB endpoint for an exact Steam `appid`:
    /// `GET {sgdb_base_url}/games/steam/{appid}`.
    /// Returns `Ok(None)` if SteamGridDB responds with 404 or `success: false` so callers can fall back cleanly.
    pub fn lookup_sgdb_by_steam_appid(
        &self,
        api_key: &str,
        steam_appid: &str,
    ) -> Result<Option<SgdbGameMatch>, String> {
        let trimmed_key = api_key.trim();
        if trimmed_key.is_empty() {
            return Err(T::SgdbKeyMissing.s().to_string());
        }
        let clean_appid = steam_appid.trim();
        if clean_appid.is_empty() {
            return Ok(None);
        }

        let url = format!(
            "{}/games/steam/{}",
            self.sgdb_base_url,
            percent_encode_path_segment(clean_appid)
        );

        let response = match self
            .agent
            .get(&url)
            .set("Authorization", &format!("Bearer {trimmed_key}"))
            .set("Accept", "application/json")
            .call()
        {
            Ok(resp) => resp,
            Err(ureq::Error::Status(401 | 403, _)) => {
                return Err(T::SgdbKeyInvalid.s().to_string());
            }
            Err(ureq::Error::Status(404, _)) => return Ok(None),
            Err(ureq::Error::Status(code, _)) => {
                return Err(format!(
                    "{} {code} (Steam AppID {clean_appid})",
                    T::SgdbHttpError.s()
                ));
            }
            Err(e) => return Err(format!("{}: {e}", T::SgdbNetworkError.s())),
        };

        let json: Value = response
            .into_json()
            .map_err(|e| format!("{}: {e}", T::SgdbBadResponse.s()))?;

        if json.get("success").and_then(Value::as_bool) != Some(true) {
            return Ok(None);
        }

        let Some(data) = json.get("data") else {
            return Ok(None);
        };

        Ok(parse_sgdb_game_object(data))
    }

    /// Searches SteamGridDB by text query:
    /// `GET {sgdb_base_url}/search/autocomplete/{term}`.
    /// Used both for manual search ("Cerca su SteamGridDB") and as automatic lookup
    /// for non-Steam games (Epic, Battle.net, Standalone) or fallback when Steam `appid` isn't matched.
    pub fn search_steamgriddb(
        &self,
        api_key: &str,
        query: &str,
    ) -> Result<Vec<SgdbGameMatch>, String> {
        let trimmed_key = api_key.trim();
        if trimmed_key.is_empty() {
            return Err(T::SgdbKeyMissing.s().to_string());
        }
        let cleaned_query = query.trim();
        if cleaned_query.is_empty() {
            return Ok(Vec::new());
        }

        let url = format!(
            "{}/search/autocomplete/{}",
            self.sgdb_base_url,
            percent_encode_path_segment(cleaned_query)
        );

        let response = match self
            .agent
            .get(&url)
            .set("Authorization", &format!("Bearer {trimmed_key}"))
            .set("Accept", "application/json")
            .call()
        {
            Ok(resp) => resp,
            Err(ureq::Error::Status(401 | 403, _)) => {
                return Err(T::SgdbKeyInvalid.s().to_string());
            }
            Err(ureq::Error::Status(404, _)) => return Ok(Vec::new()),
            Err(ureq::Error::Status(code, _)) => {
                return Err(format!("{} {code}", T::SgdbHttpError.s()));
            }
            Err(e) => return Err(format!("{}: {e}", T::SgdbNetworkError.s())),
        };

        let json: Value = response
            .into_json()
            .map_err(|e| format!("{}: {e}", T::SgdbBadResponse.s()))?;

        if json.get("success").and_then(Value::as_bool) != Some(true) {
            return Ok(Vec::new());
        }

        let Some(items) = json.get("data").and_then(Value::as_array) else {
            return Ok(Vec::new());
        };

        Ok(items.iter().filter_map(parse_sgdb_game_object).collect())
    }

    /// Resolves the best SteamGridDB game match for a detected game:
    /// - **Steam games** with `appid`: queries `GET /games/steam/{appid}` FIRST for a 100% exact match,
    ///   falling back to `/search/autocomplete/{clean_title}` only if `/games/steam/{appid}` yields no match.
    /// - **Non-Steam games** (Epic, Battle.net, Standalone): queries `/search/autocomplete/{clean_title}`.
    pub fn resolve_sgdb_for_game(
        &self,
        api_key: &str,
        game: &DetectedGame,
    ) -> Result<Option<SgdbGameMatch>, String> {
        Ok(self
            .resolve_sgdb_match(api_key, game)?
            .map(|resolution| resolution.game))
    }

    /// Same lookup as [`Self::resolve_sgdb_for_game`], additionally reporting whether the
    /// result is the exact Steam `appid` match or just a text-search guess.
    pub fn resolve_sgdb_match(
        &self,
        api_key: &str,
        game: &DetectedGame,
    ) -> Result<Option<SgdbResolution>, String> {
        if game.platform == GamePlatform::Steam {
            if let Some(appid) = game
                .platform_app_id
                .as_deref()
                .map(str::trim)
                .filter(|id| !id.is_empty())
            {
                match self.lookup_sgdb_by_steam_appid(api_key, appid) {
                    Ok(Some(exact_match)) => {
                        return Ok(Some(SgdbResolution {
                            game: exact_match,
                            exact_steam_match: true,
                        }))
                    }
                    Ok(None) => {
                        // Exact Steam AppID not indexed on SteamGridDB -> fallback to text autocomplete
                    }
                    // A bad key will fail the text search too: report it instead of hiding it
                    Err(err)
                        if err == T::SgdbKeyInvalid.s() || err == T::SgdbKeyMissing.s() =>
                    {
                        return Err(err)
                    }
                    Err(_) => {
                        // Transient error on Steam endpoint -> try text autocomplete fallback
                    }
                }
            }
        }

        let search_term = if !game.clean_title.trim().is_empty() {
            &game.clean_title
        } else {
            &game.title
        };
        let results = self.search_steamgriddb(api_key, search_term)?;
        Ok(results.into_iter().next().map(|first_hit| SgdbResolution {
            game: first_hit,
            exact_steam_match: false,
        }))
    }

    /// Downloads the vertical `600x900` cover grid for `sgdb_game_id` and saves it locally inside `covers_dir`.
    pub fn download_cover_for_sgdb_id(
        &self,
        api_key: &str,
        sgdb_game_id: u64,
        local_game_id: &str,
        covers_dir: &Path,
    ) -> Result<Option<PathBuf>, String> {
        let trimmed_key = api_key.trim();
        if trimmed_key.is_empty() {
            return Err(T::SgdbKeyMissing.s().to_string());
        }

        // 1. Primary request: 600x900 vertical grids
        let url_600x900 = format!(
            "{}/grids/game/{}?dimensions=600x900",
            self.sgdb_base_url, sgdb_game_id
        );
        let mut chosen_image_url = self.fetch_best_grid_url(trimmed_key, &url_600x900, false)?;

        // 2. Fallback: any vertical grid if exact 600x900 is unavailable
        if chosen_image_url.is_none() {
            let url_any = format!("{}/grids/game/{}", self.sgdb_base_url, sgdb_game_id);
            chosen_image_url = self.fetch_best_grid_url(trimmed_key, &url_any, true)?;
        }

        let Some(img_url) = chosen_image_url else {
            return Ok(None);
        };

        self.download_cover_image(&img_url, local_game_id, covers_dir)
    }

    /// Downloads `image_url` and stores it as the cover of `local_game_id` inside `covers_dir`,
    /// replacing any previous cover. `Ok(None)` when the server has no such image.
    pub fn download_cover_image(
        &self,
        image_url: &str,
        local_game_id: &str,
        covers_dir: &Path,
    ) -> Result<Option<PathBuf>, String> {
        let resp = match self.agent.get(image_url).call() {
            Ok(resp) => resp,
            Err(ureq::Error::Status(404, _)) => return Ok(None),
            Err(e) => return Err(format!("{}: {e}", T::CoverDownloadFailed.s())),
        };

        let content_type = resp.content_type().to_string();
        let ext = detect_image_extension(image_url, &content_type);

        let mut bytes = Vec::new();
        resp.into_reader()
            .take(15 * 1024 * 1024)
            .read_to_end(&mut bytes)
            .map_err(|e| format!("{}: {e}", T::CoverDownloadFailed.s()))?;

        if bytes.is_empty() {
            return Ok(None);
        }

        fs::create_dir_all(covers_dir).map_err(|e| {
            format!("{} {}: {e}", T::CannotCreateFolder.s(), covers_dir.display())
        })?;

        let safe_stem = sanitize_filename_id(local_game_id);
        remove_existing_cover_variants(covers_dir, &safe_stem);

        let dest = covers_dir.join(format!("{safe_stem}.{ext}"));
        fs::write(&dest, &bytes)
            .map_err(|e| format!("{} ({}): {e}", T::CoverSaveFailed.s(), dest.display()))?;

        Ok(Some(dest))
    }

    /// Asks Steam's public store service for an app's official artwork.
    ///
    /// This is the only reliable source for recent apps: their images live under hashed
    /// paths (`steam/apps/<id>/<hash>/library_capsule.jpg`), so the classic
    /// `.../library_600x900.jpg` URL answers 404 for them.
    pub fn fetch_steam_artwork(&self, steam_appid: &str) -> Option<SteamArtwork> {
        let appid: u64 = steam_appid.trim().parse().ok()?;
        let item = self
            .fetch_steam_store_item(appid, serde_json::json!({ "include_assets": true }))
            .ok()??;
        let assets = item.get("assets")?;
        // e.g. "steam/apps/3883570/${FILENAME}?t=1792307731"
        let url_format = assets.get("asset_url_format")?.as_str()?;

        let asset_url = |key: &str| {
            assets
                .get(key)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|file| !file.is_empty())
                .map(|file| {
                    format!(
                        "{}/{}",
                        self.steam_assets_base_url,
                        url_format.replace("${FILENAME}", file)
                    )
                })
        };

        let artwork = SteamArtwork {
            library_capsule_url: asset_url("library_capsule"),
            header_url: asset_url("header"),
        };
        (artwork != SteamArtwork::default()).then_some(artwork)
    }

    /// One app as Steam's public store service describes it, with the parts `data_request`
    /// asks for. `Ok(None)`: the service answered and has no such (visible) app. `Err`: it
    /// could not be asked.
    /// Name and kind of many Steam apps at once, from the public store service (no key).
    /// Apps the store does not list come back without a name. `None` means Steam could not
    /// be reached: nothing is known, ask again later.
    pub fn fetch_steam_app_summaries(&self, appids: &[u64]) -> Option<Vec<SteamAppSummary>> {
        let mut summaries = Vec::with_capacity(appids.len());
        for batch in appids.chunks(STEAM_SUMMARY_BATCH) {
            let input = serde_json::json!({
                "ids": batch.iter().map(|appid| serde_json::json!({ "appid": appid })).collect::<Vec<_>>(),
                "context": { "language": self.lang.steam_language(), "country_code": "US" },
                "data_request": { "include_assets": true },
            });
            let url = format!(
                "{}/IStoreBrowseService/GetItems/v1/?input_json={}",
                self.steam_api_base_url,
                percent_encode_path_segment(&input.to_string())
            );
            let root: Value = self.agent.get(&url).call().ok()?.into_json().ok()?;
            let items = root
                .get("response")
                .and_then(|response| response.get("store_items"))
                .and_then(Value::as_array)?;

            for appid in batch {
                // An unlisted app is answered with a stub: `success` other than 1, `appid: 0`
                let listed = items.iter().find(|item| {
                    item.get("appid").and_then(Value::as_u64) == Some(*appid)
                        && item.get("success").and_then(Value::as_u64) == Some(1)
                });
                summaries.push(SteamAppSummary {
                    appid: *appid,
                    name: listed
                        .and_then(|item| item.get("name"))
                        .and_then(Value::as_str)
                        .map(|name| decode_basic_html_entities(name).trim().to_string())
                        .filter(|name| !name.is_empty()),
                    is_game: listed
                        .and_then(|item| item.get("type"))
                        .and_then(Value::as_u64)
                        == Some(STEAM_STORE_TYPE_GAME),
                    cover_url: listed.and_then(|item| self.steam_cover_url(item)),
                });
            }
        }
        Some(summaries)
    }

    /// The cover in a store item's `assets`: the vertical capsule, else the header.
    fn steam_cover_url(&self, item: &Value) -> Option<String> {
        let assets = item.get("assets")?;
        let url_format = assets.get("asset_url_format")?.as_str()?;
        ["library_capsule", "header"].into_iter().find_map(|key| {
            let file = assets.get(key)?.as_str()?.trim();
            (!file.is_empty()).then(|| {
                format!(
                    "{}/{}",
                    self.steam_assets_base_url,
                    url_format.replace("${FILENAME}", file)
                )
            })
        })
    }

    fn fetch_steam_store_item(&self, appid: u64, data_request: Value) -> Result<Option<Value>, ()> {
        let input = serde_json::json!({
            "ids": [{ "appid": appid }],
            "context": { "language": self.lang.steam_language(), "country_code": "US" },
            "data_request": data_request,
        });
        let url = format!(
            "{}/IStoreBrowseService/GetItems/v1/?input_json={}",
            self.steam_api_base_url,
            percent_encode_path_segment(&input.to_string())
        );
        let root: Value = self
            .agent
            .get(&url)
            .call()
            .map_err(|_| ())?
            .into_json()
            .map_err(|_| ())?;
        let items = root
            .get("response")
            .and_then(|response| response.get("store_items"))
            .and_then(Value::as_array);
        // An app the store does not show comes back as a stub with `appid: 0`
        Ok(items.and_then(|items| {
            items
                .iter()
                .find(|item| item.get("appid").and_then(Value::as_u64) == Some(appid))
                .cloned()
        }))
    }

    /// Names of Steam's community tags in the client's language, by tag id.
    fn steam_tag_names(&self) -> Option<&HashMap<u64, String>> {
        self.tag_names
            .get_or_init(|| {
                let url = format!(
                    "{}/tagdata/populartags/{}",
                    self.steam_store_base_url,
                    self.lang.steam_language()
                );
                let list: Vec<Value> = self.agent.get(&url).call().ok()?.into_json().ok()?;
                let names: HashMap<u64, String> = list
                    .iter()
                    .filter_map(|tag| {
                        let id = tag.get("tagid")?.as_u64()?;
                        let name = tag.get("name")?.as_str()?.trim();
                        (!name.is_empty()).then(|| (id, name.to_string()))
                    })
                    .collect();
                (!names.is_empty()).then_some(names)
            })
            .as_ref()
    }

    /// The community tags Steam users gave an app ("Souls-like", "Party Game", "Co-op", ...),
    /// most relevant first, in the client's language. These are what the store's own genres
    /// lack. An empty list: the app has none; `None`: Steam could not be asked.
    pub fn fetch_steam_tags(&self, steam_appid: &str) -> Option<Vec<String>> {
        let appid: u64 = steam_appid.trim().parse().ok()?;
        let names = self.steam_tag_names()?;
        let Some(item) = self
            .fetch_steam_store_item(
                appid,
                serde_json::json!({ "include_tag_count": MAX_GAME_TAGS }),
            )
            .ok()?
        else {
            return Some(Vec::new());
        };

        let mut weighted: Vec<(u64, u64)> = item
            .get("tags")
            .and_then(Value::as_array)
            .map(|tags| {
                tags.iter()
                    .filter_map(|tag| {
                        let id = tag.get("tagid")?.as_u64()?;
                        Some((id, tag.get("weight").and_then(Value::as_u64).unwrap_or(0)))
                    })
                    .collect()
            })
            .unwrap_or_default();
        weighted.sort_by_key(|&(_, weight)| std::cmp::Reverse(weight));

        let mut tags: Vec<String> = Vec::new();
        for (id, _) in weighted {
            if let Some(name) = names.get(&id) {
                if !tags.iter().any(|known| known.eq_ignore_ascii_case(name)) {
                    tags.push(name.clone());
                }
            }
            if tags.len() == MAX_GAME_TAGS {
                break;
            }
        }
        Some(tags)
    }

    fn fetch_best_grid_url(
        &self,
        api_key: &str,
        endpoint_url: &str,
        prefer_vertical: bool,
    ) -> Result<Option<String>, String> {
        let resp = match self
            .agent
            .get(endpoint_url)
            .set("Authorization", &format!("Bearer {api_key}"))
            .set("Accept", "application/json")
            .call()
        {
            Ok(r) => r,
            Err(ureq::Error::Status(404, _)) => return Ok(None),
            Err(ureq::Error::Status(401 | 403, _)) => {
                return Err(T::SgdbKeyInvalid.s().to_string())
            }
            Err(e) => return Err(format!("{}: {e}", T::SgdbNetworkError.s())),
        };

        let json: Value = resp
            .into_json()
            .map_err(|e| format!("{}: {e}", T::SgdbBadResponse.s()))?;

        let Some(items) = json.get("data").and_then(Value::as_array) else {
            return Ok(None);
        };

        // Filter out NSFW/humor entries first if clean alternatives exist
        let pick_from = |require_vertical: bool, allow_nsfw: bool| -> Option<String> {
            items.iter().find_map(|item| {
                let nsfw = item.get("nsfw").and_then(Value::as_bool).unwrap_or(false);
                if !allow_nsfw && nsfw {
                    return None;
                }
                if require_vertical {
                    let w = item.get("width").and_then(Value::as_u64).unwrap_or(0);
                    let h = item.get("height").and_then(Value::as_u64).unwrap_or(0);
                    if h <= w {
                        return None;
                    }
                }
                item.get("url")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|u| !u.is_empty())
                    .map(ToString::to_string)
            })
        };

        if prefer_vertical {
            if let Some(u) = pick_from(true, false).or_else(|| pick_from(true, true)) {
                return Ok(Some(u));
            }
        }

        Ok(pick_from(false, false).or_else(|| pick_from(false, true)))
    }

    /// Queries the public Steam Store API (no API key required) for the app's official `name`,
    /// type, and Italian `short_description` and `genres`. `None` when nothing was found,
    /// whatever the reason; see [`Self::lookup_steam_store`] to tell the reasons apart.
    pub fn fetch_steam_store_metadata(
        &self,
        known_steam_appid: Option<&str>,
        lookup_title: &str,
    ) -> Option<SteamStoreMetadata> {
        match self.lookup_steam_store(known_steam_appid, lookup_title) {
            SteamStoreLookup::Found(meta) => Some(meta),
            SteamStoreLookup::NotListed | SteamStoreLookup::Unavailable => None,
        }
    }

    /// Looks an app up on the Steam Store by its `appid` or, for non-Steam games, by the first
    /// search hit for `lookup_title`.
    pub fn lookup_steam_store(
        &self,
        known_steam_appid: Option<&str>,
        lookup_title: &str,
    ) -> SteamStoreLookup {
        let appid = match known_steam_appid
            .map(str::trim)
            .filter(|id| !id.is_empty())
        {
            Some(id) => id.to_string(),
            None => match self.search_steam_store_appid(lookup_title) {
                Ok(Some(found)) => found,
                Ok(None) => return SteamStoreLookup::NotListed,
                Err(()) => return SteamStoreLookup::Unavailable,
            },
        };

        let url = format!(
            "{}/api/appdetails?appids={}&l={}",
            self.steam_store_base_url,
            percent_encode_path_segment(&appid),
            self.lang.steam_language()
        );

        let Ok(resp) = self.agent.get(&url).call() else {
            return SteamStoreLookup::Unavailable;
        };
        let Ok(root) = resp.into_json::<Value>() else {
            return SteamStoreLookup::Unavailable;
        };
        let Some(entry) = root.get(&appid) else {
            return SteamStoreLookup::Unavailable;
        };
        if entry.get("success").and_then(Value::as_bool) != Some(true) {
            return SteamStoreLookup::NotListed;
        }
        let Some(data) = entry.get("data") else {
            return SteamStoreLookup::NotListed;
        };

        let text_field = |key: &str| {
            data.get(key)
                .and_then(Value::as_str)
                .map(decode_basic_html_entities)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
        };

        let genres = data
            .get("genres")
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(|g| g.get("description").and_then(Value::as_str))
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        SteamStoreLookup::Found(SteamStoreMetadata {
            steam_appid: appid,
            name: text_field("name"),
            app_type: text_field("type").map(|t| t.to_ascii_lowercase()),
            short_description: text_field("short_description"),
            genres,
        })
    }

    /// First Steam appid matching `raw_title`; `Ok(None)` when the search has no hit,
    /// `Err` when the search itself failed.
    /// The Steam store appid matching `title`, searched at most once per enrichment: `known`
    /// carries what an earlier step already learned.
    fn store_app_of(
        &self,
        known: &mut Option<Result<Option<String>, ()>>,
        title: &str,
    ) -> Result<Option<String>, ()> {
        known
            .get_or_insert_with(|| self.search_steam_store_appid(title))
            .clone()
    }

    fn search_steam_store_appid(&self, raw_title: &str) -> Result<Option<String>, ()> {
        let cleaned = clean_game_title(raw_title);
        if cleaned.is_empty() {
            return Ok(None);
        }
        let url = format!(
            "{}/actions/SearchApps/{}",
            self.steam_community_base_url,
            percent_encode_path_segment(&cleaned)
        );
        let resp = self.agent.get(&url).call().map_err(|_| ())?;
        let items: Vec<SteamSearchAppItem> = resp.into_json().map_err(|_| ())?;
        Ok(items.into_iter().next().and_then(|item| item.appid_string()))
    }

    /// Automatically enriches a game with its official Steam name (when the scanner could only
    /// produce a provisional title), Steam store type, description & genres in the client's
    /// language, and cover art: SteamGridDB when `api_key` is set, then Steam's own artwork.
    ///
    /// Always stamps `metadata_attempted_at`; `updated_at` moves only when something visible
    /// changed, so an unsuccessful retry leaves the game (and its cached cover URL) untouched.
    pub fn enrich_game_automatically(
        &self,
        game: &mut DetectedGame,
        api_key: Option<&str>,
        covers_dir: &Path,
    ) -> EnrichOutcome {
        let before = game.clone();
        let mut outcome = EnrichOutcome::default();
        let api_key = api_key.map(str::trim).filter(|k| !k.is_empty());
        let steam_appid = steam_appid_of(game).map(ToString::to_string);
        let lang_code = self.lang.code();

        // A provisional title can only be resolved reliably through the exact Steam appid
        let mut needs_title = game.title_provisional && steam_appid.is_some();
        let type_unknown = steam_appid.is_some() && game.app_type.is_none();
        let translate = needs_translation(game, self.lang);
        // What step 1 learns about the Steam store page of a non-Steam game: its appid, that
        // there is none (`Ok(None)`), or that the store could not be asked (`Err`)
        let mut store_app: Option<Result<Option<String>, ()>> = None;

        // 1. Official name, type, description & genres via the public Steam Store API. Runs
        //    before the cover lookup so a freshly resolved title also drives SteamGridDB's
        //    text search.
        if needs_title
            || type_unknown
            || translate
            || game.short_description.is_none()
            || game.genres.is_empty()
        {
            match self.lookup_steam_store(steam_appid.as_deref(), &game.clean_title) {
                SteamStoreLookup::Found(meta) => {
                    store_app = Some(Ok(Some(meta.steam_appid.clone())));
                    if needs_title
                        && meta.name.as_deref().is_some_and(|n| apply_resolved_title(game, n))
                    {
                        needs_title = false;
                    }
                    if translate {
                        // Replace the texts cached in the previous language
                        if meta.short_description.is_some() {
                            game.short_description = meta.short_description;
                        }
                        if !meta.genres.is_empty() {
                            game.genres = meta.genres;
                        }
                    } else {
                        if game.short_description.is_none() {
                            game.short_description = meta.short_description;
                        }
                        if game.genres.is_empty() && !meta.genres.is_empty() {
                            game.genres = meta.genres;
                        }
                    }
                    if !game.custom_override
                        && (game.short_description.is_some() || !game.genres.is_empty())
                    {
                        game.metadata_lang = Some(lang_code.to_string());
                    }
                    if type_unknown {
                        let app_type = meta.app_type.unwrap_or_else(|| "unknown".to_string());
                        // Soundtracks, DLC and the like are hidden once, when first classified;
                        // the user can always bring them back from "Nascosti"
                        if NON_GAME_APP_TYPES.contains(&app_type.as_str()) && !game.is_hidden {
                            game.is_hidden = true;
                            outcome.auto_hidden = true;
                        }
                        game.app_type = Some(app_type);
                    }
                }
                SteamStoreLookup::NotListed => {
                    store_app = Some(Ok(None));
                    if type_unknown {
                        game.app_type = Some("unlisted".to_string());
                    }
                    if translate {
                        // Nothing better exists: keep the old texts and stop asking
                        game.metadata_lang = Some(lang_code.to_string());
                    }
                }
                SteamStoreLookup::Unavailable => store_app = Some(Err(())),
            }
        }

        // 2. Cover art, when missing
        let mut exact_lookup_done = false;
        if !game.is_hidden && cover_is_missing(game) {
            if let Some(key) = api_key {
                exact_lookup_done = steam_appid.is_some();
                match self.resolve_sgdb_match(key, game) {
                    Ok(Some(resolution)) => {
                        let matched = resolution.game;
                        game.steamgriddb_id = Some(matched.id);
                        if needs_title
                            && resolution.exact_steam_match
                            && apply_resolved_title(game, &matched.name)
                        {
                            needs_title = false;
                        }
                        match self.download_cover_for_sgdb_id(
                            key,
                            matched.id,
                            &game.id,
                            covers_dir,
                        ) {
                            Ok(Some(saved_path)) => {
                                game.cover_path = Some(saved_path.to_string_lossy().to_string());
                            }
                            Ok(None) => {}
                            Err(e) => outcome.warning = Some(e),
                        }
                    }
                    Ok(None) => {}
                    Err(e) => outcome.warning = Some(e),
                }
            }

            // Steam's own artwork: the fallback when SteamGridDB has nothing (very recent or
            // niche games) and the only source when no API key is configured. The vertical
            // library capsule is preferred; the landscape header is better than no cover.
            if cover_is_missing(game) {
                if let Some(artwork) = steam_appid
                    .as_deref()
                    .and_then(|appid| self.fetch_steam_artwork(appid))
                {
                    for url in [artwork.library_capsule_url, artwork.header_url]
                        .into_iter()
                        .flatten()
                    {
                        match self.download_cover_image(&url, &game.id, covers_dir) {
                            Ok(Some(saved_path)) => {
                                game.cover_path = Some(saved_path.to_string_lossy().to_string());
                                break;
                            }
                            Ok(None) => {}
                            Err(e) => {
                                outcome.warning.get_or_insert(e);
                            }
                        }
                    }
                }
            }
        }

        // 3. The store had no usable name (delisted / region-locked app) and SteamGridDB's
        //    exact appid endpoint was not asked yet: ask it for the name alone.
        if needs_title && !exact_lookup_done && !game.is_hidden {
            if let (Some(key), Some(appid)) = (api_key, steam_appid.as_deref()) {
                match self.lookup_sgdb_by_steam_appid(key, appid) {
                    Ok(Some(exact)) => {
                        apply_resolved_title(game, &exact.name);
                    }
                    Ok(None) => {}
                    Err(e) => {
                        outcome.warning.get_or_insert(e);
                    }
                }
            }
        }

        // 4. Community tags, never fetched or in another language. A Steam game is asked by its
        //    appid; any other game by the store page that matched its title.
        if !game.is_hidden && needs_tags(game, self.lang) {
            let tag_app = match &steam_appid {
                Some(appid) => Ok(Some(appid.clone())),
                None => self.store_app_of(&mut store_app, &game.clean_title),
            };
            let tags = match tag_app {
                Ok(Some(appid)) => self.fetch_steam_tags(&appid),
                Ok(None) => Some(Vec::new()),
                Err(()) => None,
            };
            // An answer, even an empty one, settles the matter; no answer is retried later
            if let Some(tags) = tags {
                game.tags = tags;
                game.tags_lang = Some(lang_code.to_string());
            }
        }

        // 5. The Steam store page of a game from another store: reviews, news and anything
        //    else keyed by a Steam appid (plugins) can then find it. No answer is retried later.
        if needs_store_page(game) {
            if let Ok(found) = self.store_app_of(&mut store_app, &game.clean_title) {
                game.steam_store_appid = Some(found.unwrap_or_default());
            }
        }

        // Compare ignoring bookkeeping: the fields this function always touches, and the notes
        // that the tags and the store page were looked up
        let mut comparable = game.clone();
        comparable.tags_lang = before.tags_lang.clone();
        comparable.steam_store_appid = before.steam_store_appid.clone();
        outcome.changed = comparable != before;
        let now = now_timestamp_ms();
        game.metadata_synced = true;
        game.metadata_attempted_at = now;
        if outcome.changed {
            game.updated_at = now;
        }
        outcome
    }

    /// Applies a manual user selection from "Cerca su SteamGridDB", immediately replacing
    /// the local cover image and refreshing metadata for the chosen title.
    pub fn apply_manual_sgdb_selection(
        &self,
        game: &mut DetectedGame,
        api_key: &str,
        sgdb_game_id: u64,
        matched_name: &str,
        covers_dir: &Path,
    ) -> Result<(), String> {
        let saved_cover =
            self.download_cover_for_sgdb_id(api_key, sgdb_game_id, &game.id, covers_dir)?;

        if let Some(path) = saved_cover {
            game.cover_path = Some(path.to_string_lossy().to_string());
        }

        let trimmed_name = matched_name.trim();
        if !trimmed_name.is_empty() {
            game.title = trimmed_name.to_string();
            game.clean_title = clean_game_title(trimmed_name);
            game.title_provisional = false;
        }

        // Refresh description & genres to match the chosen game
        let steam_lookup_id = if game.platform == GamePlatform::Steam {
            game.platform_app_id.as_deref()
        } else {
            None
        };
        let store_meta = self
            .fetch_steam_store_metadata(None, &game.clean_title)
            .or_else(|| self.fetch_steam_store_metadata(steam_lookup_id, &game.clean_title));

        if let Some(meta) = store_meta {
            if meta.short_description.is_some() {
                game.short_description = meta.short_description;
            }
            if !meta.genres.is_empty() {
                game.genres = meta.genres;
            }
            game.metadata_lang = Some(self.lang.code().to_string());
        }

        game.steamgriddb_id = Some(sgdb_game_id);
        game.custom_override = true;
        game.metadata_synced = true;
        game.updated_at = now_timestamp_ms();

        Ok(())
    }
}

/// Replaces a provisional title with an official `name`, clearing `title_provisional`.
/// Returns `false` (leaving the game untouched) when `name` is itself unusable.
fn apply_resolved_title(game: &mut DetectedGame, name: &str) -> bool {
    let trimmed = name.trim();
    if is_placeholder_title(trimmed) {
        return false;
    }
    game.title = trimmed.to_string();
    game.clean_title = clean_game_title(trimmed);
    game.title_provisional = false;
    true
}

#[derive(Debug, Deserialize)]
struct SteamSearchAppItem {
    appid: Value,
}

impl SteamSearchAppItem {
    fn appid_string(&self) -> Option<String> {
        match &self.appid {
            Value::String(s) if !s.trim().is_empty() => Some(s.trim().to_string()),
            Value::Number(n) => Some(n.to_string()),
            _ => None,
        }
    }
}

fn parse_sgdb_game_object(val: &Value) -> Option<SgdbGameMatch> {
    let id = val.get("id")?.as_u64()?;
    let name = val.get("name")?.as_str()?.trim().to_string();
    if name.is_empty() {
        return None;
    }

    let release_year = val
        .get("release_date")
        .and_then(Value::as_i64)
        .and_then(unix_seconds_to_year);

    let verified = val
        .get("verified")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    let types = val
        .get("types")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(Value::as_str)
                .map(ToString::to_string)
                .collect()
        })
        .unwrap_or_default();

    Some(SgdbGameMatch {
        id,
        name,
        release_year,
        verified,
        types,
    })
}

fn unix_seconds_to_year(ts: i64) -> Option<i32> {
    if ts <= 0 {
        return None;
    }
    // Civil year approximation accurate for game release years (1970..2100)
    let days = ts.div_euclid(86_400);
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let year = y + i64::from(mp >= 10);
    (1970..=2100).contains(&year).then_some(year as i32)
}

pub fn sanitize_filename_id(raw_id: &str) -> String {
    raw_id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect()
}

pub fn remove_existing_cover_variants(covers_dir: &Path, safe_stem: &str) {
    for ext in ["png", "jpg", "jpeg", "webp"] {
        let candidate = covers_dir.join(format!("{safe_stem}.{ext}"));
        if candidate.is_file() {
            let _ = fs::remove_file(candidate);
        }
    }
}

fn detect_image_extension(url: &str, content_type: &str) -> &'static str {
    let ct = content_type.to_ascii_lowercase();
    if ct.contains("webp") {
        return "webp";
    }
    if ct.contains("jpeg") || ct.contains("jpg") {
        return "jpg";
    }
    if ct.contains("png") {
        return "png";
    }

    let clean_url = url.split('?').next().unwrap_or(url).to_ascii_lowercase();
    if clean_url.ends_with(".webp") {
        "webp"
    } else if clean_url.ends_with(".jpg") || clean_url.ends_with(".jpeg") {
        "jpg"
    } else {
        "png"
    }
}

fn percent_encode_path_segment(input: &str) -> String {
    let mut encoded = String::with_capacity(input.len());
    for byte in input.as_bytes() {
        match *byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(*byte as char);
            }
            b => encoded.push_str(&format!("%{b:02X}")),
        }
    }
    encoded
}

fn decode_basic_html_entities(s: &str) -> String {
    s.replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};
    use std::thread;
    use tempfile::tempdir;

    pub(crate) fn spawn_mock_api_server() -> (String, Arc<Mutex<Vec<String>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock server");
        let addr = listener.local_addr().unwrap();
        let base_url = format!("http://{addr}");
        let recorded_paths: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let paths_clone = Arc::clone(&recorded_paths);
        let base_clone = base_url.clone();

        thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let mut reader = BufReader::new(&stream);
                let mut request_line = String::new();
                if reader.read_line(&mut request_line).is_err() || request_line.is_empty() {
                    continue;
                }
                // Drain headers
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).is_err() || line.trim().is_empty() {
                        break;
                    }
                }

                let path = request_line
                    .split_whitespace()
                    .nth(1)
                    .unwrap_or("/")
                    .to_string();
                paths_clone.lock().unwrap().push(path.clone());

                let (status, content_type, body): (&str, &str, Vec<u8>) =
                    if path == "/games/steam/1091500" {
                        (
                            "200 OK",
                            "application/json",
                            br#"{"success":true,"data":{"id":5276290,"name":"Cyberpunk 2077","verified":true,"release_date":1607558400,"types":["steam"]}}"#.to_vec(),
                        )
                    } else if path == "/games/steam/1321440" {
                        (
                            "200 OK",
                            "application/json",
                            br#"{"success":true,"data":{"id":5330001,"name":"Cassette Beasts","verified":true,"release_date":1682467200,"types":["steam"]}}"#.to_vec(),
                        )
                    } else if path == "/games/steam/777777" {
                        (
                            "200 OK",
                            "application/json",
                            br#"{"success":true,"data":{"id":5440002,"name":"Delisted Gem","verified":false,"types":["steam"]}}"#.to_vec(),
                        )
                    } else if path == "/games/steam/999999" {
                        (
                            "404 Not Found",
                            "application/json",
                            br#"{"success":false,"errors":["Game not found"]}"#.to_vec(),
                        )
                    } else if path.starts_with("/search/autocomplete/") {
                        (
                            "200 OK",
                            "application/json",
                            br#"{"success":true,"data":[{"id":777001,"name":"Diablo IV","verified":true,"release_date":1685923200,"types":["bnet"]},{"id":777002,"name":"Diablo II: Resurrected","verified":true,"types":["bnet"]}]}"#.to_vec(),
                        )
                    } else if path.starts_with("/grids/game/") {
                        let body_str = format!(
                            r#"{{"success":true,"data":[{{"id":1,"width":600,"height":900,"nsfw":false,"url":"{base_clone}/mock-cover.png"}}]}}"#
                        );
                        ("200 OK", "application/json", body_str.into_bytes())
                    } else if path == "/mock-cover.png" {
                        ("200 OK", "image/png", b"\x89PNG\r\n\x1a\nmockcover".to_vec())
                    } else if path.starts_with("/actions/SearchApps/Unknown") {
                        ("200 OK", "application/json", b"[]".to_vec())
                    } else if path.starts_with("/actions/SearchApps/") {
                        (
                            "200 OK",
                            "application/json",
                            br#"[{"appid":"2344520","name":"Diablo IV"}]"#.to_vec(),
                        )
                    } else if path.starts_with("/api/appdetails?appids=1091500") {
                        (
                            "200 OK",
                            "application/json",
                            r#"{"1091500":{"success":true,"data":{"short_description":"GDR d'azione futuristico.","genres":[{"id":"3","description":"GDR"}]}}}"#.as_bytes().to_vec(),
                        )
                    } else if path.starts_with("/api/appdetails?appids=1321440")
                        && !path.contains("l=english")
                    {
                        (
                            "200 OK",
                            "application/json",
                            r#"{"1321440":{"success":true,"data":{"type":"game","name":"Cassette Beasts","short_description":"Colleziona mostri e trasformati in loro.","genres":[{"id":"23","description":"Indie"},{"id":"3","description":"GDR"}]}}}"#.as_bytes().to_vec(),
                        )
                    } else if path.starts_with("/IStoreBrowseService/GetItems/v1/")
                        && path.contains("%7D%2C%7B%22appid%22")
                    {
                        // Several apps at once, as the real service answers: a game, a DLC,
                        // an app it does not list, another game
                        (
                            "200 OK",
                            "application/json",
                            br#"{"response":{"store_items":[
                                {"item_type":0,"id":620,"success":1,"visible":true,"name":"Portal 2","appid":620,"type":0,
                                 "assets":{"asset_url_format":"steam/apps/620/${FILENAME}?t=1790187113","header":"header.jpg","library_capsule":"library_600x900.jpg"}},
                                {"item_type":0,"id":1880832,"success":1,"visible":true,"name":"Hogwarts Legacy: Dark Arts Pack","appid":1880832,"type":4},
                                {"item_type":0,"id":228980,"success":15,"visible":false,"name":"","appid":0},
                                {"item_type":0,"id":220,"success":1,"visible":true,"name":"Half-Life\u00ae 2","appid":220,"type":0,
                                 "assets":{"asset_url_format":"steam/apps/220/${FILENAME}?t=1","header":"header.jpg"}},
                                {"item_type":0,"id":105600,"success":1,"visible":true,"name":"Terraria","appid":105600,"type":0}
                            ]}}"#.to_vec(),
                        )
                    } else if path.starts_with("/IStoreBrowseService/GetItems/v1/") && path.contains("3883570") {
                        // Recent app: artwork under hashed paths, as the real service answers
                        (
                            "200 OK",
                            "application/json",
                            br#"{"response":{"store_items":[{"appid":3883570,"name":"Invokyr","assets":{"asset_url_format":"steam/apps/3883570/${FILENAME}?t=1792307731","library_capsule":"1723ba58/library_capsule.jpg","header":"a1d37fb2/header.jpg"}}]}}"#.to_vec(),
                        )
                    } else if path.starts_with("/IStoreBrowseService/GetItems/v1/") && path.contains("4588930") {
                        // App whose vertical capsule is listed but missing on the CDN
                        (
                            "200 OK",
                            "application/json",
                            br#"{"response":{"store_items":[{"appid":4588930,"name":"The Old Flesh of Nuraga","assets":{"asset_url_format":"steam/apps/4588930/${FILENAME}?t=1","library_capsule":"gone/library_capsule.jpg","header":"8da013f4/header.jpg"}}]}}"#.to_vec(),
                        )
                    } else if path.starts_with("/store_item_assets/steam/apps/3883570/1723ba58/library_capsule.jpg") {
                        ("200 OK", "image/jpeg", b"\xff\xd8\xffvertical-capsule".to_vec())
                    } else if path.starts_with("/store_item_assets/steam/apps/4588930/8da013f4/header.jpg") {
                        ("200 OK", "image/jpeg", b"\xff\xd8\xfflandscape-header".to_vec())
                    } else if path.starts_with("/api/appdetails?appids=1321440&l=english") {
                        (
                            "200 OK",
                            "application/json",
                            r#"{"1321440":{"success":true,"data":{"type":"game","name":"Cassette Beasts","short_description":"Collect monsters and transform into them.","genres":[{"id":"23","description":"Indie"},{"id":"3","description":"RPG"}]}}}"#.as_bytes().to_vec(),
                        )
                    } else if path.starts_with("/api/appdetails?appids=2000001") {
                        (
                            "200 OK",
                            "application/json",
                            br#"{"2000001":{"success":true,"data":{"type":"Music","name":"Hades Original Soundtrack","short_description":"La colonna sonora.","genres":[]}}}"#.to_vec(),
                        )
                    } else if path.starts_with("/api/appdetails?appids=777777") {
                        (
                            "200 OK",
                            "application/json",
                            br#"{"777777":{"success":false}}"#.to_vec(),
                        )
                    } else if path.starts_with("/api/appdetails?appids=2344520") {
                        (
                            "200 OK",
                            "application/json",
                            r#"{"2344520":{"success":true,"data":{"short_description":"Action RPG oscuro a Sanctuarium.","genres":[{"id":"1","description":"Azione"},{"id":"3","description":"GDR"}]}}}"#.as_bytes().to_vec(),
                        )
                    } else if path.starts_with("/tagdata/populartags/english") {
                        (
                            "200 OK",
                            "application/json",
                            br#"[{"tagid":29482,"name":"Souls-like"},{"tagid":4325,"name":"Turn-Based Combat"},{"tagid":122,"name":"RPG"},{"tagid":7178,"name":"Party Game"}]"#.to_vec(),
                        )
                    } else if path.starts_with("/tagdata/populartags/") {
                        (
                            "200 OK",
                            "application/json",
                            r#"[{"tagid":29482,"name":"Stile Souls"},{"tagid":4325,"name":"Combattimento a turni"},{"tagid":122,"name":"GDR"},{"tagid":7178,"name":"Giochi di società"}]"#.as_bytes().to_vec(),
                        )
                    } else if path.starts_with("/IStoreBrowseService/GetItems/v1/") && path.contains("1321440") {
                        // Out of order, with a tag Steam has no name for and a repeated one
                        (
                            "200 OK",
                            "application/json",
                            br#"{"response":{"store_items":[{"appid":1321440,"success":1,"tags":[{"tagid":122,"weight":500},{"tagid":999999,"weight":900},{"tagid":4325,"weight":501},{"tagid":122,"weight":100}]}]}}"#.to_vec(),
                        )
                    } else if path.starts_with("/IStoreBrowseService/GetItems/v1/") && path.contains("2344520") {
                        (
                            "200 OK",
                            "application/json",
                            br#"{"response":{"store_items":[{"appid":2344520,"success":1,"tags":[{"tagid":122,"weight":300},{"tagid":7178,"weight":20}]}]}}"#.to_vec(),
                        )
                    } else if path.starts_with("/IStoreBrowseService/GetItems/v1/") {
                        // An app the store does not show: the real service answers with a stub
                        (
                            "200 OK",
                            "application/json",
                            br#"{"response":{"store_items":[{"item_type":0,"id":1,"success":15,"visible":false,"name":"","appid":0}]}}"#.to_vec(),
                        )
                    } else {
                        ("404 Not Found", "application/json", b"{}".to_vec())
                    };

                let header = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let mut writer = stream;
                let _ = writer.write_all(header.as_bytes());
                let _ = writer.write_all(&body);
                let _ = writer.flush();
            }
        });

        (base_url, recorded_paths)
    }

    #[test]
    fn steam_game_uses_exact_steam_appid_endpoint_without_text_autocomplete() {
        let (base_url, recorded) = spawn_mock_api_server();
        let client = MetadataClient::with_base_urls(&base_url, &base_url, &base_url);
        let covers_dir = tempdir().unwrap();

        let mut steam_game = DetectedGame::new_detected(
            "steam:1091500".to_string(),
            "Cyberpunk 2077".to_string(),
            "Cyberpunk 2077".to_string(),
            GamePlatform::Steam,
            Some("1091500".to_string()),
            r"C:\Steam\steamapps\common\Cyberpunk 2077".to_string(),
            None,
            Some("steam://rungameid/1091500".to_string()),
        );

        let outcome = client.enrich_game_automatically(&mut steam_game, Some("test_sgdb_key"), covers_dir.path());
        assert_eq!(outcome.warning, None);

        assert_eq!(steam_game.steamgriddb_id, Some(5276290));
        assert!(steam_game.cover_path.is_some());
        assert!(Path::new(steam_game.cover_path.as_deref().unwrap()).is_file());
        assert_eq!(
            steam_game.short_description.as_deref(),
            Some("GDR d'azione futuristico.")
        );
        assert_eq!(steam_game.genres, vec!["GDR"]);

        let paths = recorded.lock().unwrap().clone();
        assert!(
            paths.iter().any(|p| p == "/games/steam/1091500"),
            "Must call dedicated Steam AppID endpoint: {paths:?}"
        );
        assert!(
            !paths.iter().any(|p| p.starts_with("/search/autocomplete/")),
            "Must NOT call text autocomplete when exact Steam AppID matched: {paths:?}"
        );
    }

    #[test]
    fn steam_game_falls_back_to_autocomplete_when_appid_not_found_and_manual_search_works() {
        let (base_url, recorded) = spawn_mock_api_server();
        let client = MetadataClient::with_base_urls(&base_url, &base_url, &base_url);
        let covers_dir = tempdir().unwrap();

        // Unknown Steam AppID 999999 -> 404 -> falls back to /search/autocomplete/
        let fallback_steam_game = DetectedGame::new_detected(
            "steam:999999".to_string(),
            "Diablo IV".to_string(),
            "Diablo IV".to_string(),
            GamePlatform::Steam,
            Some("999999".to_string()),
            r"C:\Steam\steamapps\common\Diablo IV".to_string(),
            None,
            Some("steam://rungameid/999999".to_string()),
        );

        let resolved = client
            .resolve_sgdb_for_game("test_sgdb_key", &fallback_steam_game)
            .expect("fallback succeeds")
            .expect("found via autocomplete fallback");
        assert_eq!(resolved.id, 777001);

        let paths = recorded.lock().unwrap().clone();
        assert!(paths.iter().any(|p| p == "/games/steam/999999"));
        assert!(paths
            .iter()
            .any(|p| p == "/search/autocomplete/Diablo%20IV"));

        // Manual search & selection ("Cerca su SteamGridDB") on a Battle.net game
        let mut bnet_game = DetectedGame::new_detected(
            "battlenet:fenris".to_string(),
            "Fenris".to_string(),
            "Fenris".to_string(),
            GamePlatform::BattleNet,
            Some("Fenris".to_string()),
            r"C:\Games\Diablo IV".to_string(),
            Some(r"C:\Games\Diablo IV\Diablo IV Launcher.exe".to_string()),
            None,
        );

        let manual_results = client
            .search_steamgriddb("test_sgdb_key", "Diablo")
            .expect("manual search succeeds");
        assert_eq!(manual_results.len(), 2);
        assert_eq!(manual_results[0].name, "Diablo IV");
        assert_eq!(manual_results[0].release_year, Some(2023));

        client
            .apply_manual_sgdb_selection(
                &mut bnet_game,
                "test_sgdb_key",
                manual_results[0].id,
                &manual_results[0].name,
                covers_dir.path(),
            )
            .expect("manual selection updates cover and metadata");

        assert_eq!(bnet_game.title, "Diablo IV");
        assert_eq!(bnet_game.steamgriddb_id, Some(777001));
        assert!(bnet_game.custom_override);
        assert!(Path::new(bnet_game.cover_path.as_deref().unwrap()).is_file());
        assert_eq!(
            bnet_game.short_description.as_deref(),
            Some("Action RPG oscuro a Sanctuarium.")
        );
        assert_eq!(bnet_game.genres, vec!["Azione", "GDR"]);
    }

    fn provisional_steam_game(appid: &str, guessed_title: &str) -> DetectedGame {
        let mut game = DetectedGame::new_detected(
            format!("steam:{appid}"),
            guessed_title.to_string(),
            guessed_title.to_string(),
            GamePlatform::Steam,
            Some(appid.to_string()),
            format!(r"C:\Steam\steamapps\common\{guessed_title}"),
            None,
            Some(format!("steam://rungameid/{appid}")),
        );
        game.title_provisional = true;
        game
    }

    #[test]
    fn provisional_steam_title_is_replaced_by_official_store_name_without_api_key() {
        let (base_url, recorded) = spawn_mock_api_server();
        let client = MetadataClient::with_base_urls(&base_url, &base_url, &base_url);
        let covers_dir = tempdir().unwrap();

        // Manifest said `appid_1321440`; the scanner could only guess from the install folder
        let mut game = provisional_steam_game("1321440", "CassetteBeasts");

        let outcome = client.enrich_game_automatically(&mut game, None, covers_dir.path());
        assert_eq!(outcome.warning, None);

        assert_eq!(game.title, "Cassette Beasts");
        assert_eq!(game.clean_title, "Cassette Beasts");
        assert!(!game.title_provisional);
        assert_eq!(game.genres, vec!["Indie", "GDR"]);

        let paths = recorded.lock().unwrap().clone();
        assert!(
            !paths.iter().any(|p| p.starts_with("/games/steam/")),
            "No SteamGridDB call expected without an API key: {paths:?}"
        );
    }

    #[test]
    fn provisional_title_is_resolved_even_when_cover_and_metadata_are_already_cached() {
        let (base_url, recorded) = spawn_mock_api_server();
        let client = MetadataClient::with_base_urls(&base_url, &base_url, &base_url);
        let covers_dir = tempdir().unwrap();
        let cached_cover = covers_dir.path().join("steam_1321440.png");
        fs::write(&cached_cover, b"cached").unwrap();

        // State of a library synced before the fix: cover, description and genres are all
        // present, only the name is wrong
        let mut game = provisional_steam_game("1321440", "Cassette Beasts (folder)");
        game.cover_path = Some(cached_cover.to_string_lossy().to_string());
        game.short_description = Some("Descrizione in cache.".to_string());
        game.genres = vec!["Indie".to_string()];
        game.metadata_lang = Some("it".to_string());
        game.metadata_synced = true;

        let outcome = client.enrich_game_automatically(&mut game, Some("test_sgdb_key"), covers_dir.path());
        assert_eq!(outcome.warning, None);

        assert_eq!(game.title, "Cassette Beasts");
        assert!(!game.title_provisional);
        // Cached data is left alone
        assert_eq!(game.short_description.as_deref(), Some("Descrizione in cache."));
        assert_eq!(game.genres, vec!["Indie"]);
        assert_eq!(fs::read(&cached_cover).unwrap(), b"cached");

        let paths = recorded.lock().unwrap().clone();
        assert!(
            !paths.iter().any(|p| p.starts_with("/grids/")),
            "Cover must not be downloaded again: {paths:?}"
        );
    }

    #[test]
    fn provisional_title_falls_back_to_exact_sgdb_name_when_store_has_no_entry() {
        let (base_url, recorded) = spawn_mock_api_server();
        let client = MetadataClient::with_base_urls(&base_url, &base_url, &base_url);
        let covers_dir = tempdir().unwrap();

        // a) Cover missing: the exact appid match gives both the name and the cover
        let mut without_cover = provisional_steam_game("777777", "delisted_gem");
        let outcome = client.enrich_game_automatically(&mut without_cover, Some("test_sgdb_key"), covers_dir.path());
        assert_eq!(outcome.warning, None);
        assert_eq!(without_cover.title, "Delisted Gem");
        assert!(!without_cover.title_provisional);
        assert!(without_cover.cover_path.is_some());
        let exact_calls = recorded
            .lock()
            .unwrap()
            .iter()
            .filter(|p| *p == "/games/steam/777777")
            .count();
        assert_eq!(exact_calls, 1, "exact appid endpoint must be queried only once");

        // b) Cover already cached: the name alone is fetched from the exact appid endpoint
        let mut with_cover = provisional_steam_game("777777", "delisted_gem");
        with_cover.cover_path = without_cover.cover_path.clone();
        let outcome = client.enrich_game_automatically(&mut with_cover, Some("test_sgdb_key"), covers_dir.path());
        assert_eq!(outcome.warning, None);
        assert_eq!(with_cover.title, "Delisted Gem");
        assert!(!with_cover.title_provisional);
    }

    #[test]
    fn provisional_title_is_kept_and_stays_flagged_when_nothing_resolves_it() {
        let (base_url, _recorded) = spawn_mock_api_server();
        let client = MetadataClient::with_base_urls(&base_url, &base_url, &base_url);
        let covers_dir = tempdir().unwrap();
        let cached_cover = covers_dir.path().join("steam_424242.png");
        fs::write(&cached_cover, b"cached").unwrap();

        // Unknown everywhere (store 404, SteamGridDB 404). A text-search hit must never be
        // used as the name: it would rename the game to an unrelated title.
        let mut game = provisional_steam_game("424242", "Some Folder Name");
        game.cover_path = Some(cached_cover.to_string_lossy().to_string());

        let _ = client.enrich_game_automatically(&mut game, Some("test_sgdb_key"), covers_dir.path());

        assert_eq!(game.title, "Some Folder Name");
        assert!(game.title_provisional, "must be retried on the next sync");

        // Same guarantee when the cover is missing and SteamGridDB only has a text-search hit
        let mut no_cover = provisional_steam_game("424242", "Some Folder Name");
        let _ = client.enrich_game_automatically(&mut no_cover, Some("test_sgdb_key"), covers_dir.path());
        assert_eq!(no_cover.title, "Some Folder Name");
        assert!(no_cover.title_provisional);
    }

    fn synced_steam_game(appid: &str, title: &str) -> DetectedGame {
        let mut game = DetectedGame::new_detected(
            format!("steam:{appid}"),
            title.to_string(),
            title.to_string(),
            GamePlatform::Steam,
            Some(appid.to_string()),
            format!(r"C:\Steam\steamapps\common\{title}"),
            None,
            Some(format!("steam://rungameid/{appid}")),
        );
        game.short_description = Some("Descrizione.".to_string());
        game.genres = vec!["Azione".to_string()];
        game.metadata_lang = Some("it".to_string());
        game.tags_lang = Some("it".to_string());
        game
    }

    #[test]
    fn steam_soundtrack_is_classified_and_hidden_once() {
        let (base_url, recorded) = spawn_mock_api_server();
        let client = MetadataClient::with_base_urls(&base_url, &base_url, &base_url);
        let covers_dir = tempdir().unwrap();

        let mut soundtrack = synced_steam_game("2000001", "Hades Original Soundtrack");
        assert!(needs_metadata(&soundtrack, false, Lang::It), "store type is still unknown");

        let outcome = client.enrich_game_automatically(&mut soundtrack, Some("key"), covers_dir.path());
        assert!(outcome.auto_hidden && outcome.changed);
        assert_eq!(soundtrack.app_type.as_deref(), Some("music"));
        assert!(soundtrack.is_hidden);
        assert!(
            !recorded.lock().unwrap().iter().any(|p| p.starts_with("/grids/")),
            "no cover is downloaded for an entry that was just hidden"
        );
        assert!(!needs_metadata(&soundtrack, true, Lang::It), "hidden entries are left alone");

        // The user brings it back: later passes never hide it again
        soundtrack.is_hidden = false;
        soundtrack.cover_path = None;
        let outcome = client.enrich_game_automatically(&mut soundtrack, None, covers_dir.path());
        assert!(!outcome.auto_hidden);
        assert!(!soundtrack.is_hidden);

        // A real game is classified and stays visible
        let mut game = synced_steam_game("1321440", "Cassette Beasts");
        let outcome = client.enrich_game_automatically(&mut game, None, covers_dir.path());
        assert_eq!(game.app_type.as_deref(), Some("game"));
        assert!(!game.is_hidden && !outcome.auto_hidden);
    }

    #[test]
    fn unsuccessful_retry_leaves_the_game_untouched_but_records_the_attempt() {
        let (base_url, _recorded) = spawn_mock_api_server();
        let client = MetadataClient::with_base_urls(&base_url, &base_url, &base_url);
        let covers_dir = tempdir().unwrap();

        // Unknown to the store: nothing to fill in, no SteamGridDB key
        let mut game = DetectedGame::new_detected(
            "steam:424242".to_string(),
            "Mystery".to_string(),
            "Mystery".to_string(),
            GamePlatform::Steam,
            Some("424242".to_string()),
            r"C:\Steam\steamapps\common\Mystery".to_string(),
            None,
            None,
        );
        game.app_type = Some("unlisted".to_string());
        game.updated_at = 1_000;

        let outcome = client.enrich_game_automatically(&mut game, None, covers_dir.path());
        assert_eq!(outcome, EnrichOutcome::default());
        assert_eq!(game.updated_at, 1_000, "cover cache-busting stamp must not move");
        assert!(game.metadata_attempted_at > 1_000);
        assert!(game.metadata_synced);
    }

    #[test]
    fn store_without_a_page_marks_the_app_unlisted_instead_of_asking_forever() {
        let (base_url, _recorded) = spawn_mock_api_server();
        let client = MetadataClient::with_base_urls(&base_url, &base_url, &base_url);
        let covers_dir = tempdir().unwrap();

        // Covers are already cached, so only the type lookup is at stake here
        let cover = covers_dir.path().join("cached.png");
        fs::write(&cover, b"cached").unwrap();
        let cover = Some(cover.to_string_lossy().to_string());

        // 777777: the store answers `success: false`
        let mut delisted = synced_steam_game("777777", "Delisted Gem");
        delisted.cover_path = cover.clone();
        client.enrich_game_automatically(&mut delisted, None, covers_dir.path());
        assert_eq!(delisted.app_type.as_deref(), Some("unlisted"));
        assert!(!needs_metadata(&delisted, false, Lang::It));

        // Store unreachable: the type stays unknown and is asked again later
        let offline = MetadataClient::with_base_urls(&base_url, "http://127.0.0.1:9", &base_url);
        let mut pending = synced_steam_game("1321440", "Cassette Beasts");
        pending.cover_path = cover;
        offline.enrich_game_automatically(&mut pending, None, covers_dir.path());
        assert_eq!(pending.app_type, None);
        assert!(needs_metadata(&pending, false, Lang::It));
    }

    #[test]
    fn incomplete_games_are_retried_once_a_day_unless_asked_now() {
        let now = 10 * METADATA_RETRY_INTERVAL_MS;

        // Non-Steam game with nothing found yet
        let mut game = DetectedGame::new_detected(
            "standalone:1".to_string(),
            "Tiny Quest".to_string(),
            "Tiny Quest".to_string(),
            GamePlatform::Standalone,
            None,
            r"E:\Games\Tiny Quest".to_string(),
            Some(r"E:\Games\Tiny Quest\tiny.exe".to_string()),
            None,
        );
        assert!(is_metadata_sync_due(&game, false, Lang::It, now, false), "never attempted");

        game.metadata_attempted_at = now - 60_000;
        assert!(!is_metadata_sync_due(&game, false, Lang::It, now, false), "attempted a minute ago");
        assert!(is_metadata_sync_due(&game, false, Lang::It, now, true), "user asked for a scan");

        game.metadata_attempted_at = now - METADATA_RETRY_INTERVAL_MS;
        assert!(is_metadata_sync_due(&game, false, Lang::It, now, false), "a day has passed");

        // Complete: description, genres and tags present, cover only matters with an API key
        game.short_description = Some("Descrizione.".to_string());
        game.genres = vec!["Indie".to_string()];
        game.metadata_lang = Some("it".to_string());
        assert!(is_metadata_sync_due(&game, false, Lang::It, now, true), "tags never looked up");
        game.tags_lang = Some("it".to_string());
        assert!(
            is_metadata_sync_due(&game, false, Lang::It, now, true),
            "Steam store page never looked up"
        );
        game.steam_store_appid = Some(String::new());
        assert!(!is_metadata_sync_due(&game, false, Lang::It, now, true));
        assert!(is_metadata_sync_due(&game, true, Lang::It, now, true), "cover missing, key present");

        game.is_hidden = true;
        assert!(!is_metadata_sync_due(&game, true, Lang::It, now, true));
    }

    fn bare_steam_game(appid: &str, title: &str) -> DetectedGame {
        let mut game = synced_steam_game(appid, title);
        game.app_type = Some("game".to_string());
        game
    }

    #[test]
    fn steam_artwork_is_the_cover_fallback_and_needs_no_api_key() {
        let (base_url, recorded) = spawn_mock_api_server();
        let client = MetadataClient::with_base_urls(&base_url, &base_url, &base_url);
        let covers_dir = tempdir().unwrap();

        // Recent game unknown to SteamGridDB: the official vertical capsule lives under a
        // hashed path that only Steam's store service reveals
        let mut invokyr = bare_steam_game("3883570", "Invokyr");
        assert!(needs_metadata(&invokyr, false, Lang::It), "a Steam game can get a cover without a key");

        let outcome = client.enrich_game_automatically(&mut invokyr, None, covers_dir.path());
        assert!(outcome.changed);
        assert_eq!(outcome.warning, None);
        let cover = invokyr.cover_path.clone().expect("cover downloaded");
        assert!(cover.ends_with("steam_3883570.jpg"), "{cover}");
        assert_eq!(fs::read(&cover).unwrap(), b"\xff\xd8\xffvertical-capsule");
        assert!(!needs_metadata(&invokyr, false, Lang::It));

        let paths = recorded.lock().unwrap().clone();
        assert!(
            !paths.iter().any(|p| p.starts_with("/games/steam/") || p.starts_with("/search/")),
            "SteamGridDB is not contacted without a key: {paths:?}"
        );
        assert!(paths.iter().any(|p| p
            == "/store_item_assets/steam/apps/3883570/1723ba58/library_capsule.jpg?t=1792307731"));
    }

    #[test]
    fn steam_artwork_fills_in_when_steamgriddb_has_nothing_and_falls_back_to_the_header() {
        let (base_url, recorded) = spawn_mock_api_server();
        let client = MetadataClient::with_base_urls(&base_url, &base_url, &base_url);
        let covers_dir = tempdir().unwrap();

        // 4588930: SteamGridDB has no exact match (404), its text search returns an unrelated
        // game whose grid is downloaded... so make SteamGridDB truly empty for this check by
        // using a key-less pass first: vertical capsule 404 -> landscape header is used
        let mut nuraga = bare_steam_game("4588930", "The Old Flesh of Nuraga");
        client.enrich_game_automatically(&mut nuraga, None, covers_dir.path());
        let cover = nuraga.cover_path.clone().expect("header used as cover");
        assert_eq!(fs::read(&cover).unwrap(), b"\xff\xd8\xfflandscape-header");

        let paths = recorded.lock().unwrap().clone();
        let capsule_at = paths.iter().position(|p| p.contains("gone/library_capsule.jpg"));
        let header_at = paths.iter().position(|p| p.contains("8da013f4/header.jpg"));
        assert!(capsule_at.is_some() && capsule_at < header_at, "capsule is tried first: {paths:?}");

        // With a key, a SteamGridDB cover wins and Steam's artwork is not requested at all
        let (base_url, recorded) = spawn_mock_api_server();
        let client = MetadataClient::with_base_urls(&base_url, &base_url, &base_url);
        let mut cyberpunk = bare_steam_game("1091500", "Cyberpunk 2077");
        client.enrich_game_automatically(&mut cyberpunk, Some("test_sgdb_key"), covers_dir.path());
        assert!(cyberpunk.cover_path.is_some());
        assert!(
            !recorded.lock().unwrap().iter().any(|p| p.starts_with("/IStoreBrowseService/")),
            "no need to ask Steam when SteamGridDB delivered"
        );
    }

    #[test]
    fn cached_texts_are_refetched_when_the_language_changes_but_manual_edits_are_kept() {
        let (base_url, _recorded) = spawn_mock_api_server();
        let english = MetadataClient::with_base_urls(&base_url, &base_url, &base_url)
            .with_language(Lang::En);
        let covers_dir = tempdir().unwrap();
        let cover = covers_dir.path().join("steam_1321440.png");
        fs::write(&cover, b"cached").unwrap();

        // Synced in Italian by a previous session
        let mut game = bare_steam_game("1321440", "Cassette Beasts");
        game.cover_path = Some(cover.to_string_lossy().to_string());
        game.short_description = Some("Colleziona mostri e trasformati in loro.".to_string());
        game.genres = vec!["Indie".to_string(), "GDR".to_string()];
        assert!(!needs_metadata(&game, false, Lang::It));
        assert!(needs_metadata(&game, false, Lang::En), "texts are in another language");

        let outcome = english.enrich_game_automatically(&mut game, None, covers_dir.path());
        assert!(outcome.changed);
        assert_eq!(
            game.short_description.as_deref(),
            Some("Collect monsters and transform into them.")
        );
        assert_eq!(game.genres, vec!["Indie", "RPG"]);
        assert_eq!(game.metadata_lang.as_deref(), Some("en"));
        assert!(!needs_metadata(&game, false, Lang::En));

        // Texts the user wrote by hand are never replaced
        let mut edited = bare_steam_game("1321440", "Cassette Beasts");
        edited.cover_path = Some(cover.to_string_lossy().to_string());
        edited.custom_override = true;
        edited.short_description = Some("La mia descrizione.".to_string());
        // Only the tags, which nobody writes by hand, follow the language
        assert!(needs_metadata(&edited, false, Lang::En));
        english.enrich_game_automatically(&mut edited, None, covers_dir.path());
        assert_eq!(edited.short_description.as_deref(), Some("La mia descrizione."));
        assert_eq!(edited.genres, vec!["Azione"]);
        assert_eq!(edited.tags, vec!["Turn-Based Combat", "RPG"]);
        assert!(!needs_metadata(&edited, false, Lang::En));
    }

    #[test]
    fn steam_tags_are_ranked_named_in_the_interface_language_and_follow_it() {
        let (base_url, _recorded) = spawn_mock_api_server();
        let covers_dir = tempdir().unwrap();
        let cover = covers_dir.path().join("steam_1321440.png");
        fs::write(&cover, b"png").unwrap();

        let mut game = bare_steam_game("1321440", "Cassette Beasts");
        game.cover_path = Some(cover.to_string_lossy().to_string());
        game.tags_lang = None;
        assert!(needs_metadata(&game, false, Lang::It), "tags were never fetched");

        let client = MetadataClient::with_base_urls(&base_url, &base_url, &base_url);
        let outcome = client.enrich_game_automatically(&mut game, None, covers_dir.path());
        assert!(outcome.changed);
        // By weight; the tag without a name and the repeated one are dropped
        assert_eq!(game.tags, vec!["Combattimento a turni", "GDR"]);
        assert_eq!(game.tags_lang.as_deref(), Some("it"));
        assert!(!needs_metadata(&game, false, Lang::It));

        // Another interface language: the tags are fetched again in it
        assert!(needs_metadata(&game, false, Lang::En));
        let english = MetadataClient::with_base_urls(&base_url, &base_url, &base_url)
            .with_language(Lang::En);
        english.enrich_game_automatically(&mut game, None, covers_dir.path());
        assert_eq!(game.tags, vec!["Turn-Based Combat", "RPG"]);
        assert_eq!(game.tags_lang.as_deref(), Some("en"));
        assert!(!needs_metadata(&game, false, Lang::En));
    }

    #[test]
    fn games_from_other_stores_get_the_tags_of_their_steam_store_page() {
        let (base_url, recorded) = spawn_mock_api_server();
        let covers_dir = tempdir().unwrap();
        let client = MetadataClient::with_base_urls(&base_url, &base_url, &base_url);

        // Description and genres already cached: only the tags are missing
        let mut diablo = DetectedGame::new_detected(
            "battlenet:diablo4".to_string(),
            "Diablo IV".to_string(),
            "Diablo IV".to_string(),
            GamePlatform::BattleNet,
            Some("Diablo IV".to_string()),
            r"C:\Games\Diablo IV".to_string(),
            None,
            None,
        );
        diablo.short_description = Some("Action RPG.".to_string());
        diablo.genres = vec!["Azione".to_string()];
        diablo.metadata_lang = Some("it".to_string());

        client.enrich_game_automatically(&mut diablo, None, covers_dir.path());
        assert_eq!(diablo.tags, vec!["GDR", "Giochi di società"]);
        assert_eq!(diablo.tags_lang.as_deref(), Some("it"));
        let paths = recorded.lock().unwrap().clone();
        assert!(paths.iter().any(|p| p.starts_with("/actions/SearchApps/")), "{paths:?}");
        assert!(
            !paths.iter().any(|p| p.starts_with("/api/appdetails")),
            "the cached texts are not fetched again: {paths:?}"
        );

        // The matched store page is remembered (one search served both steps), so reviews,
        // news and plugins can find the game on Steam
        assert_eq!(diablo.steam_store_id(), Some("2344520"));
        assert_eq!(
            paths.iter().filter(|p| p.starts_with("/actions/SearchApps/")).count(),
            1,
            "{paths:?}"
        );
        assert!(!needs_metadata(&diablo, false, Lang::It));

        // A game Steam does not sell: the empty answer is remembered too
        let mut unlisted = diablo.clone();
        unlisted.id = "standalone:x".to_string();
        unlisted.clean_title = "Unknown Homebrew Thing".to_string();
        unlisted.tags_lang = None;
        unlisted.steam_store_appid = None;
        client.enrich_game_automatically(&mut unlisted, None, covers_dir.path());
        assert_eq!(unlisted.steam_store_appid.as_deref(), Some(""));
        assert_eq!(unlisted.steam_store_id(), None);
        assert!(!needs_metadata(&unlisted, false, Lang::It));
    }

    #[test]
    fn tags_stay_pending_while_steam_cannot_be_reached() {
        let covers_dir = tempdir().unwrap();
        let cover = covers_dir.path().join("steam_1.png");
        fs::write(&cover, b"png").unwrap();
        let mut game = bare_steam_game("1", "Offline");
        game.cover_path = Some(cover.to_string_lossy().to_string());
        game.tags_lang = None;

        // Nothing listens here
        let offline = "http://127.0.0.1:9";
        let client = MetadataClient::with_base_urls(offline, offline, offline);
        let outcome = client.enrich_game_automatically(&mut game, None, covers_dir.path());
        assert!(!outcome.changed);
        assert!(game.tags.is_empty());
        assert_eq!(game.tags_lang, None, "asked again at the next retry");
        assert!(needs_metadata(&game, false, Lang::It));
    }
}
