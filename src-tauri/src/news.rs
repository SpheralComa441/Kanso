//! The News page: the latest PC gaming and hardware news from the publishers' public feeds.

use crate::{load_config, AppState, FALLBACK_LANGUAGE};
use kanso_core::news::{default_sources, is_allowed_news_url, source_by_id, Feed, NewsClient, NewsReport, NewsSourceInfo, NEWS_SOURCES};
use kanso_core::{now_timestamp_ms, T};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;

/// How long the news read from the feeds is good for.
const NEWS_TTL_MS: u64 = 20 * 60 * 1000;

/// The last answer: for which sources, and what they said.
pub(crate) type NewsCache = Arc<Mutex<Option<(Vec<String>, NewsReport)>>>;

/// Every source Kanso can read, for the settings.
#[tauri::command]
pub(crate) fn news_sources() -> Vec<NewsSourceInfo> {
    NEWS_SOURCES.iter().map(NewsSourceInfo::from).collect()
}

/// The feeds the News page reads: the sources the user chose (the ones in the language of
/// the interface until they choose) and the feeds they added by hand.
fn chosen_feeds(app: &AppHandle) -> Vec<Feed> {
    let config = load_config(app);
    let mut feeds: Vec<Feed> = match &config.news_sources {
        Some(chosen) => chosen.iter().filter_map(|id| source_by_id(id)).map(Feed::from).collect(),
        None => default_sources(config.lang().unwrap_or(FALLBACK_LANGUAGE)).into_iter().map(Feed::from).collect(),
    };
    feeds.extend(config.news_feeds.iter().enumerate().filter_map(|(index, feed)| Feed::custom(index, feed)));
    feeds
}

/// The latest news of the sources the user keeps, newest first. Results are reused for
/// twenty minutes unless `force_refresh`. Which sources are asked is read from the settings.
#[tauri::command]
pub(crate) async fn fetch_news(app: AppHandle, state: State<'_, AppState>, force_refresh: bool) -> Result<NewsReport, String> {
    let sources = chosen_feeds(&app);
    // What was asked: a feed added, removed or changed asks afresh
    let ids: Vec<String> = sources.iter().map(|feed| format!("{}|{}|{}|{:?}", feed.id, feed.feed_url, feed.name, feed.topic)).collect();
    let cache = Arc::clone(&state.news_cache);
    if !force_refresh {
        if let Some((cached_ids, report)) = cache.lock().unwrap_or_else(|p| p.into_inner()).as_ref() {
            if *cached_ids == ids && now_timestamp_ms().saturating_sub(report.fetched_at) < NEWS_TTL_MS {
                return Ok(report.clone());
            }
        }
    }
    tauri::async_runtime::spawn_blocking(move || {
        let report = NewsClient::new().fetch(&sources);
        // A fetch that got nothing (offline) is not worth keeping: the next visit tries again
        if !report.items.is_empty() {
            *cache.lock().unwrap_or_else(|p| p.into_inner()) = Some((ids, report.clone()));
        }
        report
    })
    .await
    .map_err(|e| format!("{}: {e}", T::OperationFailed.s()))
}

/// Opens an article, or a source's site, in the default browser. Only `https` pages on the
/// sites of the sources Kanso knows, or of the feeds the user added, are accepted: the
/// addresses come from remote feeds.
#[tauri::command]
pub(crate) fn open_news_link(app: AppHandle, url: String) -> Result<(), String> {
    if !is_allowed_news_url(&url, &load_config(&app).news_feeds) {
        return Err(format!("{}: {url}", T::UrlNotAllowed.s()));
    }
    app.opener()
        .open_url(&url, None::<&str>)
        .map_err(|e| format!("{}: {e}", T::OperationFailed.s()))
}
