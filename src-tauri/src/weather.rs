//! The weather for the background of the Home page: asked only when the user turned it on
//! and named a place, and no more often than every half hour.

use crate::{load_config, AppState, FALLBACK_LANGUAGE};
use kanso_core::weather::{PlaceMatch, WeatherClient, WeatherNow, WEATHER_SOURCE_URL};
use kanso_core::{now_timestamp_ms, T};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;

/// How long an answer about the weather is good for.
const WEATHER_TTL_MS: u64 = 30 * 60 * 1000;

/// The last answer: for which coordinates, when, and what it said.
pub(crate) type WeatherCache = Arc<Mutex<Option<(String, u64, WeatherNow)>>>;

/// The weather at the place chosen in Settings; `null` when the user does not follow the
/// weather, has no place, or the service cannot be reached (the Home page then shows the
/// season, as it does for everyone else). The place is read from the settings, not taken
/// from the webview.
#[tauri::command]
pub(crate) async fn home_weather(app: AppHandle, state: State<'_, AppState>) -> Result<Option<WeatherNow>, String> {
    let home = load_config(&app).home;
    let Some(place) = home.place.filter(|_| home.weather) else {
        return Ok(None);
    };
    let key = format!("{:.2},{:.2}", place.latitude, place.longitude);
    let cache = Arc::clone(&state.weather_cache);
    if let Some((cached_key, fetched_at, weather)) = cache.lock().unwrap_or_else(|p| p.into_inner()).as_ref() {
        if *cached_key == key && now_timestamp_ms().saturating_sub(*fetched_at) < WEATHER_TTL_MS {
            return Ok(Some(*weather));
        }
    }

    tauri::async_runtime::spawn_blocking(move || {
        let weather = WeatherClient::new().current(&place).ok()?;
        *cache.lock().unwrap_or_else(|p| p.into_inner()) = Some((key, now_timestamp_ms(), weather));
        Some(weather)
    })
    .await
    .map_err(|e| format!("{}: {e}", T::OperationFailed.s()))
}

/// The places called `query`, for the user to choose one in Settings. Sends the name typed
/// to Open-Meteo, and nothing else.
#[tauri::command]
pub(crate) async fn search_weather_places(app: AppHandle, query: String) -> Result<Vec<PlaceMatch>, String> {
    let lang = load_config(&app).lang().unwrap_or(FALLBACK_LANGUAGE);
    tauri::async_runtime::spawn_blocking(move || WeatherClient::new().search_places(&query, lang))
        .await
        .map_err(|e| format!("{}: {e}", T::OperationFailed.s()))?
}

/// Opens the site the weather comes from, credited next to it.
#[tauri::command]
pub(crate) fn open_weather_source(app: AppHandle) -> Result<(), String> {
    app.opener()
        .open_url(WEATHER_SOURCE_URL, None::<&str>)
        .map_err(|e| format!("{}: {e}", T::OperationFailed.s()))
}
