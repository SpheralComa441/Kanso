//! Current discounts from the public, key-less endpoints of Steam, the Epic Games Store and GOG.

use crate::i18n::Lang;
use crate::models::now_timestamp_ms;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;
use std::time::Duration;

const DEFAULT_STEAM_STORE_BASE_URL: &str = "https://store.steampowered.com";
const DEFAULT_EPIC_BASE_URL: &str = "https://store-site-backend-static.ak.epicgames.com";
const DEFAULT_GOG_CATALOG_BASE_URL: &str = "https://catalog.gog.com";

/// How many GOG discounts to list (its catalog has thousands at any time).
const GOG_DEALS_LIMIT: usize = 24;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DealStore {
    Steam,
    Epic,
    Gog,
}

/// One discounted (or free) game on a store.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Deal {
    /// Unique across stores, e.g. `steam:2406770`.
    pub id: String,
    pub store: DealStore,
    pub title: String,
    /// Store page, always `https://` on the store's own domain.
    pub url: String,
    /// Landscape artwork.
    pub image_url: Option<String>,
    /// Price before the discount, in `currency` units (not cents).
    pub original_price: Option<f64>,
    pub final_price: f64,
    /// ISO 4217 code the two prices are expressed in.
    pub currency: String,
    pub discount_percent: u32,
    /// The offer has not started yet (Epic announces next week's free games).
    pub upcoming: bool,
    pub starts_at: Option<u64>,
    /// When the offer ends, in milliseconds since the epoch.
    pub ends_at: Option<u64>,
    /// Steam appid, to recognise games already in the library.
    pub steam_appid: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DealsReport {
    pub deals: Vec<Deal>,
    /// Stores that could not be reached; the others are still listed.
    pub failed_stores: Vec<DealStore>,
    pub fetched_at: u64,
}

#[derive(Debug, Clone)]
pub struct DealsClient {
    steam_store_base_url: String,
    epic_base_url: String,
    gog_catalog_base_url: String,
    agent: ureq::Agent,
}

impl Default for DealsClient {
    fn default() -> Self {
        Self::new()
    }
}

impl DealsClient {
    pub fn new() -> Self {
        Self::with_base_urls(
            DEFAULT_STEAM_STORE_BASE_URL,
            DEFAULT_EPIC_BASE_URL,
            DEFAULT_GOG_CATALOG_BASE_URL,
        )
    }

    pub fn with_base_urls(steam_store: &str, epic: &str, gog_catalog: &str) -> Self {
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_secs(8))
            .timeout_read(Duration::from_secs(15))
            .user_agent("KansoLauncher/0.1.0")
            .build();
        Self {
            steam_store_base_url: steam_store.trim_end_matches('/').to_string(),
            epic_base_url: epic.trim_end_matches('/').to_string(),
            gog_catalog_base_url: gog_catalog.trim_end_matches('/').to_string(),
            agent,
        }
    }

    /// Fetches every store. `country` is an ISO 3166 code (`IT`, `US`...) that selects the
    /// regional prices; a store that fails is reported in `failed_stores` without hiding the rest.
    pub fn fetch_all(&self, lang: Lang, country: &str) -> DealsReport {
        let country = normalize_country(country);
        let mut report = DealsReport {
            fetched_at: now_timestamp_ms(),
            ..Default::default()
        };

        let sources: [(DealStore, Result<Vec<Deal>, String>); 3] = [
            (DealStore::Epic, self.fetch_epic_free_games(lang, &country)),
            (DealStore::Steam, self.fetch_steam_specials(lang, &country)),
            (DealStore::Gog, self.fetch_gog_discounts(lang, &country)),
        ];
        for (store, result) in sources {
            match result {
                Ok(deals) => report.deals.extend(deals),
                Err(_) => report.failed_stores.push(store),
            }
        }
        report
    }

    fn get_json(&self, url: &str) -> Result<Value, String> {
        self.agent
            .get(url)
            .set("Accept", "application/json")
            .call()
            .map_err(|e| e.to_string())?
            .into_json()
            .map_err(|e| e.to_string())
    }

    /// Steam "Specials", plus the discounted entries of its top sellers and new releases.
    pub fn fetch_steam_specials(&self, lang: Lang, country: &str) -> Result<Vec<Deal>, String> {
        let url = format!(
            "{}/api/featuredcategories?l={}&cc={}",
            self.steam_store_base_url,
            lang.steam_language(),
            country
        );
        Ok(parse_steam_featured(&self.get_json(&url)?))
    }

    /// Games Epic gives away this week and the ones announced for the next.
    pub fn fetch_epic_free_games(&self, lang: Lang, country: &str) -> Result<Vec<Deal>, String> {
        let locale = lang.epic_locale();
        let url = format!(
            "{}/freeGamesPromotions?locale={locale}&country={country}&allowCountries={country}",
            self.epic_base_url
        );
        Ok(parse_epic_free_games(
            &self.get_json(&url)?,
            locale,
            now_timestamp_ms(),
        ))
    }

    /// Trending discounted games on GOG.
    pub fn fetch_gog_discounts(&self, lang: Lang, country: &str) -> Result<Vec<Deal>, String> {
        let url = format!(
            "{}/v1/catalog?limit={GOG_DEALS_LIMIT}&order=desc:trending&discounted=eq:true\
             &productType=in:game,pack&page=1&countryCode={country}&locale={}&currencyCode={}",
            self.gog_catalog_base_url,
            lang.gog_locale(),
            currency_for_country(country)
        );
        Ok(parse_gog_catalog(&self.get_json(&url)?))
    }
}

/// Upper-case two-letter country code, `US` when the input is not one.
pub fn normalize_country(raw: &str) -> String {
    let code = raw.trim().to_ascii_uppercase();
    if code.len() == 2 && code.chars().all(|c| c.is_ascii_alphabetic()) {
        code
    } else {
        "US".to_string()
    }
}

/// Currency GOG prices are requested in for a country (GOG requires one explicitly).
pub fn currency_for_country(country: &str) -> &'static str {
    const EUROZONE: &[&str] = &[
        "AT", "BE", "BG", "HR", "CY", "EE", "FI", "FR", "DE", "GR", "IE", "IT", "LV", "LT", "LU",
        "MT", "NL", "PT", "SK", "SI", "ES",
    ];
    match country {
        c if EUROZONE.contains(&c) => "EUR",
        "GB" => "GBP",
        "PL" => "PLN",
        "CH" => "CHF",
        "SE" => "SEK",
        "NO" => "NOK",
        "DK" => "DKK",
        "CA" => "CAD",
        "AU" => "AUD",
        "BR" => "BRL",
        "CN" => "CNY",
        _ => "USD",
    }
}

pub fn parse_steam_featured(root: &Value) -> Vec<Deal> {
    let mut deals = Vec::new();
    let mut seen = HashSet::new();

    for category in ["specials", "top_sellers", "new_releases"] {
        let Some(items) = root
            .get(category)
            .and_then(|c| c.get("items"))
            .and_then(Value::as_array)
        else {
            continue;
        };
        for item in items {
            let discount = item
                .get("discount_percent")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            // type 0 = app; bundles and packages have store pages under other paths
            if discount == 0 || item.get("type").and_then(Value::as_u64) != Some(0) {
                continue;
            }
            let (Some(id), Some(name), Some(final_cents)) = (
                item.get("id").and_then(Value::as_u64),
                item.get("name").and_then(Value::as_str),
                item.get("final_price").and_then(Value::as_u64),
            ) else {
                continue;
            };
            if !seen.insert(id) {
                continue;
            }
            deals.push(Deal {
                id: format!("steam:{id}"),
                store: DealStore::Steam,
                title: name.trim().to_string(),
                url: format!("https://store.steampowered.com/app/{id}/"),
                image_url: https_url(item.get("header_image"))
                    .or_else(|| https_url(item.get("large_capsule_image"))),
                original_price: item
                    .get("original_price")
                    .and_then(Value::as_u64)
                    .map(|cents| cents as f64 / 100.0),
                final_price: final_cents as f64 / 100.0,
                currency: item
                    .get("currency")
                    .and_then(Value::as_str)
                    .unwrap_or("USD")
                    .to_string(),
                discount_percent: discount.min(100) as u32,
                upcoming: false,
                starts_at: None,
                ends_at: item
                    .get("discount_expiration")
                    .and_then(Value::as_u64)
                    .filter(|secs| *secs > 0)
                    .map(|secs| secs * 1000),
                steam_appid: Some(id.to_string()),
            });
        }
    }

    deals.sort_by_key(|deal| std::cmp::Reverse(deal.discount_percent));
    deals
}

pub fn parse_epic_free_games(root: &Value, locale: &str, now_ms: u64) -> Vec<Deal> {
    let Some(elements) = root
        .pointer("/data/Catalog/searchStore/elements")
        .and_then(Value::as_array)
    else {
        return Vec::new();
    };

    let mut deals = Vec::new();
    for element in elements {
        let Some(title) = element.get("title").and_then(Value::as_str) else {
            continue;
        };

        // A giveaway is a promotional offer with a 0% price multiplier
        let free_offer = |list_key: &str| {
            element
                .pointer(&format!("/promotions/{list_key}"))
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|group| group.get("promotionalOffers").and_then(Value::as_array))
                .flatten()
                .find(|offer| {
                    offer
                        .pointer("/discountSetting/discountPercentage")
                        .and_then(Value::as_u64)
                        == Some(0)
                })
        };

        let (offer, upcoming) = match (free_offer("promotionalOffers"), free_offer("upcomingPromotionalOffers")) {
            (Some(current), _) => (current, false),
            (None, Some(next)) => (next, true),
            (None, None) => continue,
        };
        let starts_at = offer.get("startDate").and_then(Value::as_str).and_then(parse_iso8601_ms);
        let ends_at = offer.get("endDate").and_then(Value::as_str).and_then(parse_iso8601_ms);
        if ends_at.is_some_and(|end| end <= now_ms) {
            continue;
        }

        let Some(slug) = epic_page_slug(element) else {
            continue;
        };
        let decimals = element
            .pointer("/price/totalPrice/currencyInfo/decimals")
            .and_then(Value::as_u64)
            .unwrap_or(2)
            .min(4) as i32;
        let original_price = element
            .pointer("/price/totalPrice/originalPrice")
            .and_then(Value::as_u64)
            .map(|minor| minor as f64 / 10f64.powi(decimals));

        deals.push(Deal {
            id: format!(
                "epic:{}",
                element.get("id").and_then(Value::as_str).unwrap_or(&slug)
            ),
            store: DealStore::Epic,
            title: title.trim().to_string(),
            url: format!("https://store.epicgames.com/{locale}/p/{slug}"),
            image_url: epic_image(element),
            original_price: original_price.filter(|price| *price > 0.0),
            final_price: 0.0,
            currency: element
                .pointer("/price/totalPrice/currencyCode")
                .and_then(Value::as_str)
                .unwrap_or("USD")
                .to_string(),
            discount_percent: 100,
            upcoming,
            starts_at,
            ends_at,
            steam_appid: None,
        });
    }

    // Available now first, then by start date
    deals.sort_by_key(|deal| (deal.upcoming, deal.starts_at.unwrap_or(0)));
    deals
}

fn epic_page_slug(element: &Value) -> Option<String> {
    let mapped = |pointer: &str| {
        element
            .pointer(pointer)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|mapping| mapping.get("pageSlug").and_then(Value::as_str))
            .map(str::trim)
            .find(|slug| !slug.is_empty())
            .map(ToString::to_string)
    };
    mapped("/catalogNs/mappings")
        .or_else(|| mapped("/offerMappings"))
        .or_else(|| {
            element
                .get("productSlug")
                .and_then(Value::as_str)
                .map(|slug| slug.trim_end_matches("/home").trim().to_string())
                .filter(|slug| !slug.is_empty())
        })
        // Slugs go straight into a URL: accept nothing that could change its shape
        .filter(|slug| {
            slug.chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
        })
}

fn epic_image(element: &Value) -> Option<String> {
    let images = element.get("keyImages").and_then(Value::as_array)?;
    ["OfferImageWide", "DieselStoreFrontWide", "featuredMedia", "Thumbnail"]
        .iter()
        .find_map(|wanted| {
            images
                .iter()
                .find(|img| img.get("type").and_then(Value::as_str) == Some(wanted))
                .and_then(|img| https_url(img.get("url")))
        })
}

pub fn parse_gog_catalog(root: &Value) -> Vec<Deal> {
    let Some(products) = root.get("products").and_then(Value::as_array) else {
        return Vec::new();
    };

    let money = |product: &Value, pointer: &str| {
        product
            .pointer(pointer)
            .and_then(Value::as_str)
            .and_then(|amount| amount.trim().parse::<f64>().ok())
    };

    let mut deals = Vec::new();
    for product in products {
        let (Some(id), Some(title), Some(link), Some(final_price), Some(base_price)) = (
            product.get("id").and_then(Value::as_str),
            product.get("title").and_then(Value::as_str),
            https_url(product.get("storeLink")),
            money(product, "/price/finalMoney/amount"),
            money(product, "/price/baseMoney/amount"),
        ) else {
            continue;
        };
        if !link.starts_with("https://www.gog.com/") || base_price <= 0.0 || final_price >= base_price {
            continue;
        }
        deals.push(Deal {
            id: format!("gog:{id}"),
            store: DealStore::Gog,
            title: title.trim().to_string(),
            url: link,
            image_url: https_url(product.get("coverHorizontal")),
            original_price: Some(base_price),
            final_price,
            currency: product
                .pointer("/price/finalMoney/currency")
                .and_then(Value::as_str)
                .unwrap_or("USD")
                .to_string(),
            discount_percent: (((base_price - final_price) / base_price) * 100.0).round() as u32,
            upcoming: false,
            starts_at: None,
            ends_at: None,
            steam_appid: None,
        });
    }
    deals
}

fn https_url(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|url| url.starts_with("https://"))
        .map(ToString::to_string)
}

/// Parses `YYYY-MM-DDTHH:MM:SS[.fff]Z` (always UTC in the store APIs) into epoch milliseconds.
pub fn parse_iso8601_ms(raw: &str) -> Option<u64> {
    let raw = raw.trim();
    let (date, time) = raw.split_once('T')?;
    let mut date_parts = date.split('-').map(|part| part.parse::<i64>().ok());
    let (year, month, day) = (date_parts.next()??, date_parts.next()??, date_parts.next()??);
    let time = time.trim_end_matches('Z');
    let time = time.split('.').next()?;
    let mut time_parts = time.split(':').map(|part| part.parse::<i64>().ok());
    let (hour, minute, second) = (
        time_parts.next()??,
        time_parts.next()??,
        time_parts.next().flatten().unwrap_or(0),
    );
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) || hour > 23 || minute > 59 || second > 60 {
        return None;
    }

    // Days from the civil date (Howard Hinnant's algorithm)
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;

    let seconds = days * 86_400 + hour * 3600 + minute * 60 + second;
    u64::try_from(seconds).ok().map(|s| s * 1000)
}

/// `true` for a store page Kanso is willing to open in the browser: `https` on one of the
/// three stores' own domains. Deal URLs come from remote JSON, so they are checked again here.
pub fn is_allowed_store_url(url: &str) -> bool {
    const ALLOWED_PREFIXES: &[&str] = &[
        "https://store.steampowered.com/",
        "https://store.epicgames.com/",
        "https://www.gog.com/",
    ];
    ALLOWED_PREFIXES.iter().any(|prefix| url.starts_with(prefix))
        && !url.chars().any(|c| c.is_whitespace() || c.is_control())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_iso_dates_from_the_store_apis() {
        assert_eq!(parse_iso8601_ms("1970-01-01T00:00:00.000Z"), Some(0));
        assert_eq!(parse_iso8601_ms("2026-11-19T16:00:00.000Z"), Some(1_795_104_000_000));
        assert_eq!(parse_iso8601_ms("2024-02-29T23:59:59Z"), Some(1_709_251_199_000));
        assert_eq!(parse_iso8601_ms("not a date"), None);
        assert_eq!(parse_iso8601_ms("2026-13-01T00:00:00Z"), None);
    }

    #[test]
    fn parses_steam_specials_and_discounted_entries_of_other_lists() {
        // Shape of https://store.steampowered.com/api/featuredcategories
        let root = json!({
            "specials": { "name": "Offerte speciali", "items": [
                { "id": 2406770, "type": 0, "name": "Bodycam", "discounted": true, "discount_percent": 20,
                  "original_price": 3332, "final_price": 2665, "currency": "EUR",
                  "header_image": "https://shared.akamai.steamstatic.com/store_item_assets/steam/apps/2406770/b5/header.jpg?t=1",
                  "discount_expiration": 1795208400u64 },
                { "id": 55, "type": 1, "name": "Some Bundle", "discount_percent": 90, "final_price": 100 }
            ]},
            "top_sellers": { "items": [
                { "id": 2406770, "type": 0, "name": "Bodycam", "discount_percent": 20, "final_price": 2665 },
                { "id": 10, "type": 0, "name": "Full Price", "discount_percent": 0, "final_price": 5999 },
                { "id": 20, "type": 0, "name": "Big Cut ", "discount_percent": 75, "original_price": 1999,
                  "final_price": 499, "currency": "EUR", "header_image": "http://insecure.example/x.jpg",
                  "large_capsule_image": "https://shared.akamai.steamstatic.com/capsule.jpg" }
            ]},
            "coming_soon": { "items": [ { "id": 30, "type": 0, "name": "Later", "discount_percent": 10, "final_price": 1 } ] }
        });

        let deals = parse_steam_featured(&root);
        let titles: Vec<&str> = deals.iter().map(|d| d.title.as_str()).collect();
        assert_eq!(titles, vec!["Big Cut", "Bodycam"], "sorted by discount, deduplicated, apps only");

        let bodycam = &deals[1];
        assert_eq!(bodycam.id, "steam:2406770");
        assert_eq!(bodycam.url, "https://store.steampowered.com/app/2406770/");
        assert_eq!(bodycam.original_price, Some(33.32));
        assert_eq!(bodycam.final_price, 26.65);
        assert_eq!(bodycam.currency, "EUR");
        assert_eq!(bodycam.discount_percent, 20);
        assert_eq!(bodycam.ends_at, Some(1_795_208_400_000));
        assert_eq!(bodycam.steam_appid.as_deref(), Some("2406770"));
        // Non-https artwork is dropped in favour of the next candidate
        assert_eq!(deals[0].image_url.as_deref(), Some("https://shared.akamai.steamstatic.com/capsule.jpg"));
    }

    #[test]
    fn parses_epic_current_and_upcoming_giveaways_only() {
        let offer = |start: &str, end: &str, pct: u64| json!({ "promotionalOffers": [
            { "startDate": start, "endDate": end, "discountSetting": { "discountType": "PERCENTAGE", "discountPercentage": pct } }
        ]});
        let price = |original: u64, discounted: u64| json!({ "totalPrice": {
            "originalPrice": original, "discountPrice": discounted, "currencyCode": "EUR", "currencyInfo": { "decimals": 2 }
        }});
        // Shape of .../freeGamesPromotions
        let root = json!({ "data": { "Catalog": { "searchStore": { "elements": [
            { "title": "Alien: Isolation", "id": "a1", "price": price(3999, 3999),
              "keyImages": [ { "type": "OfferImageTall", "url": "https://cdn1.epicgames.com/tall.jpg" },
                             { "type": "OfferImageWide", "url": "https://cdn1.epicgames.com/wide.jpg" } ],
              "catalogNs": { "mappings": [ { "pageSlug": "alien-isolation", "pageType": "productHome" } ] },
              "promotions": { "promotionalOffers": [],
                              "upcomingPromotionalOffers": [ offer("2026-11-19T16:00:00.000Z", "2026-11-26T16:00:00.000Z", 0) ] } },
            { "title": "Knightica", "id": "k1", "price": price(1349, 0),
              "keyImages": [ { "type": "Thumbnail", "url": "https://cdn1.epicgames.com/thumb.png" } ],
              "catalogNs": { "mappings": null }, "offerMappings": [ { "pageSlug": "knightica-9a3e" } ],
              "promotions": { "promotionalOffers": [ offer("2026-11-12T16:00:00.000Z", "2026-11-19T16:00:00.000Z", 0) ],
                              "upcomingPromotionalOffers": [] } },
            // A plain upcoming discount, not a giveaway
            { "title": "Ghostrunner 2", "id": "g2", "price": price(3999, 3999), "productSlug": "ghostrunner-2",
              "promotions": { "promotionalOffers": [],
                              "upcomingPromotionalOffers": [ offer("2026-12-10T16:00:00.000Z", "2027-01-07T16:00:00.000Z", 20) ] } },
            // No promotion at all
            { "title": "Eternal Threads", "id": "e1", "price": price(1799, 1799), "promotions": null },
            // Giveaway that already ended
            { "title": "Old Gift", "id": "o1", "price": price(999, 0), "productSlug": "old-gift",
              "promotions": { "promotionalOffers": [ offer("2026-01-01T16:00:00.000Z", "2026-01-08T16:00:00.000Z", 0) ] } },
            // Slug that would break out of the URL path
            { "title": "Sneaky", "id": "s1", "price": price(999, 0), "productSlug": "x/../../evil?",
              "promotions": { "promotionalOffers": [ offer("2026-11-12T16:00:00.000Z", "2026-11-19T16:00:00.000Z", 0) ] } }
        ]}}}});

        let now = parse_iso8601_ms("2026-11-14T12:00:00Z").unwrap();
        let deals = parse_epic_free_games(&root, "it", now);
        let titles: Vec<&str> = deals.iter().map(|d| d.title.as_str()).collect();
        assert_eq!(titles, vec!["Knightica", "Alien: Isolation"], "free now first, then upcoming");

        let knightica = &deals[0];
        assert!(!knightica.upcoming);
        assert_eq!(knightica.final_price, 0.0);
        assert_eq!(knightica.original_price, Some(13.49));
        assert_eq!(knightica.discount_percent, 100);
        assert_eq!(knightica.url, "https://store.epicgames.com/it/p/knightica-9a3e");
        assert_eq!(knightica.image_url.as_deref(), Some("https://cdn1.epicgames.com/thumb.png"));
        assert_eq!(knightica.ends_at, Some(1_795_104_000_000));

        let alien = &deals[1];
        assert!(alien.upcoming);
        assert_eq!(alien.image_url.as_deref(), Some("https://cdn1.epicgames.com/wide.jpg"));
        assert_eq!(alien.starts_at, Some(1_795_104_000_000));
        assert!(deals.iter().all(|d| is_allowed_store_url(&d.url)));
    }

    #[test]
    fn parses_gog_discounted_products() {
        // Shape of https://catalog.gog.com/v1/catalog
        let root = json!({ "productCount": 4151, "products": [
            { "id": "1207658787", "slug": "heroes_3", "title": "Heroes of Might and Magic® 3: Complete",
              "coverHorizontal": "https://images.gog-statics.com/a18a.png",
              "storeLink": "https://www.gog.com/en/game/heroes_of_might_and_magic_3_complete_edition",
              "price": { "final": "€4.99", "base": "€9.99", "discount": "-50%",
                         "finalMoney": { "amount": "4.99", "currency": "EUR", "discount": "5.00" },
                         "baseMoney": { "amount": "9.99", "currency": "EUR" } } },
            // Not actually discounted
            { "id": "2", "title": "Same Price", "storeLink": "https://www.gog.com/en/game/same",
              "price": { "finalMoney": { "amount": "9.99", "currency": "EUR" }, "baseMoney": { "amount": "9.99", "currency": "EUR" } } },
            // Link outside gog.com
            { "id": "3", "title": "Elsewhere", "storeLink": "https://example.com/game",
              "price": { "finalMoney": { "amount": "1.00", "currency": "EUR" }, "baseMoney": { "amount": "9.99", "currency": "EUR" } } }
        ]});

        let deals = parse_gog_catalog(&root);
        assert_eq!(deals.len(), 1);
        let heroes = &deals[0];
        assert_eq!(heroes.id, "gog:1207658787");
        assert_eq!(heroes.store, DealStore::Gog);
        assert_eq!(heroes.original_price, Some(9.99));
        assert_eq!(heroes.final_price, 4.99);
        assert_eq!(heroes.discount_percent, 50);
        assert_eq!(heroes.currency, "EUR");
        assert!(parse_gog_catalog(&json!({ "products": null })).is_empty());
    }

    #[test]
    fn only_store_pages_on_known_https_domains_may_be_opened() {
        assert!(is_allowed_store_url("https://store.steampowered.com/app/2406770/"));
        assert!(is_allowed_store_url("https://store.epicgames.com/it/p/knightica-9a3e"));
        assert!(is_allowed_store_url("https://www.gog.com/en/game/heroes_3"));
        assert!(!is_allowed_store_url("http://store.steampowered.com/app/1/"));
        assert!(!is_allowed_store_url("https://store.steampowered.com.evil.example/app/1/"));
        assert!(!is_allowed_store_url("file:///C:/Windows/System32/calc.exe"));
        assert!(!is_allowed_store_url("https://www.gog.com/a b"));
    }

    #[test]
    fn country_and_currency_normalisation() {
        assert_eq!(normalize_country(" it "), "IT");
        assert_eq!(normalize_country("ita"), "US");
        assert_eq!(normalize_country(""), "US");
        assert_eq!(currency_for_country("IT"), "EUR");
        assert_eq!(currency_for_country("GB"), "GBP");
        assert_eq!(currency_for_country("RU"), "USD");
    }

    #[test]
    fn a_failing_store_does_not_hide_the_others() {
        // Nothing listens on these ports: every store fails fast
        let client = DealsClient::with_base_urls("http://127.0.0.1:9", "http://127.0.0.1:9", "http://127.0.0.1:9");
        let report = client.fetch_all(Lang::It, "IT");
        assert!(report.deals.is_empty());
        assert_eq!(report.failed_stores, vec![DealStore::Epic, DealStore::Steam, DealStore::Gog]);
        assert!(report.fetched_at > 0);
    }
}
