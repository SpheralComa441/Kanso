use crate::cleaner::{clean_game_title, is_placeholder_title};
use crate::models::{now_timestamp_ms, DetectedGame, GamePlatform, SgdbGameMatch};
use serde::Deserialize;
use serde_json::Value;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

const DEFAULT_SGDB_BASE_URL: &str = "https://www.steamgriddb.com/api/v2";
const DEFAULT_STEAM_STORE_BASE_URL: &str = "https://store.steampowered.com";
const DEFAULT_STEAM_COMMUNITY_BASE_URL: &str = "https://steamcommunity.com";

/// Metadata extracted from the public Steam Store API (`&l=italian`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SteamStoreMetadata {
    pub steam_appid: String,
    /// Official store name of the app, used to replace `appid_<id>` placeholder titles.
    pub name: Option<String>,
    pub short_description: Option<String>,
    pub genres: Vec<String>,
}

/// Outcome of matching a detected game against SteamGridDB.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SgdbResolution {
    pub game: SgdbGameMatch,
    /// `true` when the match came from `GET /games/steam/{appid}` (authoritative for that
    /// Steam app), `false` when it is only the first hit of a text search.
    pub exact_steam_match: bool,
}

/// HTTP client for SteamGridDB API v2 and the public Steam Store API.
#[derive(Debug, Clone)]
pub struct MetadataClient {
    sgdb_base_url: String,
    steam_store_base_url: String,
    steam_community_base_url: String,
    agent: ureq::Agent,
}

impl Default for MetadataClient {
    fn default() -> Self {
        Self::new()
    }
}

impl MetadataClient {
    pub fn new() -> Self {
        Self::with_base_urls(
            DEFAULT_SGDB_BASE_URL,
            DEFAULT_STEAM_STORE_BASE_URL,
            DEFAULT_STEAM_COMMUNITY_BASE_URL,
        )
    }

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

        Self {
            sgdb_base_url: sgdb_base_url.trim_end_matches('/').to_string(),
            steam_store_base_url: steam_store_base_url.trim_end_matches('/').to_string(),
            steam_community_base_url: steam_community_base_url.trim_end_matches('/').to_string(),
            agent,
        }
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
            return Err("SteamGridDB API Key mancante.".to_string());
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
                return Err(
                    "SteamGridDB API Key non valida o non autorizzata. Verifica la chiave nelle Impostazioni."
                        .to_string(),
                );
            }
            Err(ureq::Error::Status(404, _)) => return Ok(None),
            Err(ureq::Error::Status(code, _)) => {
                return Err(format!(
                    "Errore SteamGridDB HTTP {code} su lookup Steam AppID {clean_appid}"
                ));
            }
            Err(e) => return Err(format!("Errore di rete SteamGridDB: {e}")),
        };

        let json: Value = response
            .into_json()
            .map_err(|e| format!("Risposta JSON SteamGridDB non valida: {e}"))?;

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
            return Err(
                "Configura prima la SteamGridDB API Key nelle Impostazioni.".to_string(),
            );
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
                return Err(
                    "SteamGridDB API Key non valida o non autorizzata. Verifica la chiave nelle Impostazioni."
                        .to_string(),
                );
            }
            Err(ureq::Error::Status(404, _)) => return Ok(Vec::new()),
            Err(ureq::Error::Status(code, _)) => {
                return Err(format!("Errore SteamGridDB HTTP {code} durante la ricerca"));
            }
            Err(e) => return Err(format!("Errore di rete durante la ricerca SteamGridDB: {e}")),
        };

        let json: Value = response
            .into_json()
            .map_err(|e| format!("Risposta JSON SteamGridDB non valida: {e}"))?;

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
                    Err(err) if err.contains("API Key") => return Err(err),
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
            return Err("SteamGridDB API Key mancante.".to_string());
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

        let resp = self
            .agent
            .get(&img_url)
            .call()
            .map_err(|e| format!("Impossibile scaricare l'immagine di copertina: {e}"))?;

        let content_type = resp.content_type().to_string();
        let ext = detect_image_extension(&img_url, &content_type);

        let mut bytes = Vec::new();
        resp.into_reader()
            .take(15 * 1024 * 1024)
            .read_to_end(&mut bytes)
            .map_err(|e| format!("Errore lettura copertina scaricata: {e}"))?;

        if bytes.is_empty() {
            return Ok(None);
        }

        fs::create_dir_all(covers_dir).map_err(|e| {
            format!(
                "Impossibile creare la cartella copertine {}: {e}",
                covers_dir.display()
            )
        })?;

        let safe_stem = sanitize_filename_id(local_game_id);
        remove_existing_cover_variants(covers_dir, &safe_stem);

        let dest = covers_dir.join(format!("{safe_stem}.{ext}"));
        fs::write(&dest, &bytes).map_err(|e| {
            format!(
                "Impossibile salvare la copertina in {}: {e}",
                dest.display()
            )
        })?;

        Ok(Some(dest))
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
                return Err("SteamGridDB API Key non valida.".to_string())
            }
            Err(e) => return Err(format!("Errore recupero griglie SteamGridDB: {e}")),
        };

        let json: Value = resp
            .into_json()
            .map_err(|e| format!("JSON griglie SteamGridDB non valido: {e}"))?;

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

    /// Queries the public Steam Store API (no API key required) for the app's official `name`
    /// plus Italian `short_description` and `genres`.
    pub fn fetch_steam_store_metadata(
        &self,
        known_steam_appid: Option<&str>,
        lookup_title: &str,
    ) -> Option<SteamStoreMetadata> {
        let appid = match known_steam_appid
            .map(str::trim)
            .filter(|id| !id.is_empty())
        {
            Some(id) => id.to_string(),
            None => self.search_steam_store_appid(lookup_title)?,
        };

        let url = format!(
            "{}/api/appdetails?appids={}&l=italian",
            self.steam_store_base_url,
            percent_encode_path_segment(&appid)
        );

        let resp = self.agent.get(&url).call().ok()?;
        let root: Value = resp.into_json().ok()?;
        let entry = root.get(&appid)?;
        if entry.get("success").and_then(Value::as_bool) != Some(true) {
            return None;
        }
        let data = entry.get("data")?;

        let name = data
            .get("name")
            .and_then(Value::as_str)
            .map(decode_basic_html_entities)
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        let short_description = data
            .get("short_description")
            .and_then(Value::as_str)
            .map(decode_basic_html_entities)
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

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

        Some(SteamStoreMetadata {
            steam_appid: appid,
            name,
            short_description,
            genres,
        })
    }

    fn search_steam_store_appid(&self, raw_title: &str) -> Option<String> {
        let cleaned = clean_game_title(raw_title);
        if cleaned.is_empty() {
            return None;
        }
        let url = format!(
            "{}/actions/SearchApps/{}",
            self.steam_community_base_url,
            percent_encode_path_segment(&cleaned)
        );
        let resp = self.agent.get(&url).call().ok()?;
        let items: Vec<SteamSearchAppItem> = resp.into_json().ok()?;
        items.into_iter().next().and_then(|item| item.appid_string())
    }

    /// Automatically enriches a game with its official Steam name (when the scanner could only
    /// produce a provisional title), public Steam Store Italian description & genres, and
    /// SteamGridDB cover art (when `api_key` is non-empty).
    pub fn enrich_game_automatically(
        &self,
        game: &mut DetectedGame,
        api_key: Option<&str>,
        covers_dir: &Path,
    ) -> Result<(), String> {
        let mut sgdb_warning: Option<String> = None;
        let api_key = api_key.map(str::trim).filter(|k| !k.is_empty());

        let steam_appid = if game.platform == GamePlatform::Steam {
            game.platform_app_id
                .as_deref()
                .map(str::trim)
                .filter(|id| !id.is_empty())
                .map(ToString::to_string)
        } else {
            None
        };

        // A provisional title can only be resolved reliably through the exact Steam appid
        let mut needs_title = game.title_provisional && steam_appid.is_some();

        // 1. Official name, description & genres via the public Steam Store API. Runs before
        //    the cover lookup so a freshly resolved title also drives SteamGridDB's text search.
        if needs_title || game.short_description.is_none() || game.genres.is_empty() {
            if let Some(meta) =
                self.fetch_steam_store_metadata(steam_appid.as_deref(), &game.clean_title)
            {
                if needs_title && meta.name.as_deref().is_some_and(|n| apply_resolved_title(game, n))
                {
                    needs_title = false;
                }
                if game.short_description.is_none() {
                    game.short_description = meta.short_description;
                }
                if game.genres.is_empty() && !meta.genres.is_empty() {
                    game.genres = meta.genres;
                }
            }
        }

        if let Some(key) = api_key {
            // 2. Cover art via SteamGridDB when the cover is missing
            let needs_cover = game
                .cover_path
                .as_deref()
                .is_none_or(|p| !Path::new(p).is_file());
            let mut exact_lookup_done = false;

            if needs_cover {
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
                            Err(e) => sgdb_warning = Some(e),
                        }
                    }
                    Ok(None) => {}
                    Err(e) => sgdb_warning = Some(e),
                }
            }

            // 3. The store had no usable name (delisted / region-locked app) and the cover was
            //    already cached: ask SteamGridDB's exact appid endpoint for the name alone.
            if needs_title && !exact_lookup_done {
                if let Some(appid) = steam_appid.as_deref() {
                    match self.lookup_sgdb_by_steam_appid(key, appid) {
                        Ok(Some(exact)) => {
                            apply_resolved_title(game, &exact.name);
                        }
                        Ok(None) => {}
                        Err(e) => {
                            sgdb_warning.get_or_insert(e);
                        }
                    }
                }
            }
        }

        game.metadata_synced = true;
        game.updated_at = now_timestamp_ms();

        if let Some(err) = sgdb_warning {
            return Err(err);
        }
        Ok(())
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
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};
    use std::thread;
    use tempfile::tempdir;

    fn spawn_mock_api_server() -> (String, Arc<Mutex<Vec<String>>>) {
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
                    } else if path.starts_with("/api/appdetails?appids=1321440") {
                        (
                            "200 OK",
                            "application/json",
                            r#"{"1321440":{"success":true,"data":{"type":"game","name":"Cassette Beasts","short_description":"Colleziona mostri e trasformati in loro.","genres":[{"id":"23","description":"Indie"},{"id":"3","description":"GDR"}]}}}"#.as_bytes().to_vec(),
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

        client
            .enrich_game_automatically(&mut steam_game, Some("test_sgdb_key"), covers_dir.path())
            .expect("enrichment succeeds");

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

        client
            .enrich_game_automatically(&mut game, None, covers_dir.path())
            .expect("enrichment succeeds without SteamGridDB key");

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
        game.metadata_synced = true;

        client
            .enrich_game_automatically(&mut game, Some("test_sgdb_key"), covers_dir.path())
            .expect("enrichment succeeds");

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
        client
            .enrich_game_automatically(&mut without_cover, Some("test_sgdb_key"), covers_dir.path())
            .expect("enrichment succeeds");
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
        client
            .enrich_game_automatically(&mut with_cover, Some("test_sgdb_key"), covers_dir.path())
            .expect("enrichment succeeds");
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
}
