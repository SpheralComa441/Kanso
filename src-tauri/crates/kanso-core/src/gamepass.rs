//! The Xbox Game Pass catalog for PC, from Microsoft's public catalog.
//!
//! Nothing here needs an account, and nothing about the user is sent: the two services
//! below answer anyone. The first lists the ids of the games currently in Game Pass for PC;
//! the second describes products of the Microsoft Store by id (name, artwork, package). They
//! are the services the Xbox app and xbox.com use for the same pages, read the way the
//! open-source "Game Pass Catalog Browser" for Playnite reads them.
//!
//! Kanso cannot know whether the user has Game Pass (Microsoft does not tell a third-party
//! program), so the catalog is listed only when the user asks for it in the settings. Its
//! games join the library as not installed, each with the address that opens its page in the
//! Xbox app, where it can be installed. A game that leaves the catalog leaves the library
//! too, unless it is installed (see [`crate::owned::sync_game_pass_catalog`]).

use crate::accounts::StoreError;
use crate::cleaner::clean_game_title;
use crate::i18n::Lang;
use crate::models::{DetectedGame, GamePlatform};
use crate::owned::OwnedGame;
use crate::xbox_account::package_identity;
use serde_json::Value;
use std::time::Duration;

const CATALOG_SERVICE: &str = "https://catalog.gamepass.com";
const PRODUCT_SERVICE: &str = "https://displaycatalog.mp.microsoft.com";

/// The list "all the games of Game Pass for PC".
const PC_GAMES_LIST: &str = "fdd9e2a7-0fee-49f6-ad69-4354098401ff";

/// Products described per request.
const PRODUCTS_PER_REQUEST: usize = 20;
/// A catalog has a few hundred games: more than this is not a catalog.
const MAX_CATALOG_GAMES: usize = 2000;

/// How the address that opens a game's page in the Xbox app begins. It also marks an entry
/// of the library as one the catalog brought.
pub const XBOX_APP_PAGE_PREFIX: &str = "msxbox://game/?productId=";

/// Whether `game` is an entry the Game Pass catalog brought and that is still not installed.
pub fn is_catalog_entry(game: &DetectedGame) -> bool {
    game.platform == GamePlatform::Xbox
        && !game.is_installed
        && game.launch_uri.as_deref().is_some_and(|uri| uri.starts_with(XBOX_APP_PAGE_PREFIX))
}

/// The country whose catalog is read, from the language of the interface: Kanso knows
/// nothing else about where the user is. The catalogs differ by a handful of games.
fn market_of(lang: Lang) -> &'static str {
    match lang.code() {
        "it" => "IT",
        "es" => "ES",
        _ => "US",
    }
}

/// Client for Microsoft's two public catalog services.
#[derive(Debug, Clone)]
pub struct GamePassApi {
    catalog: String,
    products: String,
    agent: ureq::Agent,
}

impl Default for GamePassApi {
    fn default() -> Self {
        Self::new()
    }
}

impl GamePassApi {
    pub fn new() -> Self {
        Self::with_services(CATALOG_SERVICE, PRODUCT_SERVICE)
    }

    /// Client against other endpoints (tests).
    pub fn with_services(catalog: &str, products: &str) -> Self {
        Self {
            catalog: catalog.trim_end_matches('/').to_string(),
            products: products.trim_end_matches('/').to_string(),
            agent: ureq::AgentBuilder::new()
                .timeout_connect(Duration::from_secs(8))
                .timeout_read(Duration::from_secs(25))
                .user_agent(crate::USER_AGENT)
                .build(),
        }
    }

    /// The games of Game Pass for PC, ready to join the library as not installed. Names are
    /// asked in English: they are what the same game is called on the other stores, which is
    /// how its genres and tags are found.
    pub fn pc_catalog(&self, lang: Lang) -> Result<Vec<OwnedGame>, StoreError> {
        let market = market_of(lang);
        let listed = self.get(
            &format!("{}/sigls/v2?id={PC_GAMES_LIST}&language=en-us&market={market}", self.catalog),
            "game pass catalog",
        )?;
        // The first entry describes the list; the others are `{"id": "<product id>"}`
        let ids: Vec<&str> = listed
            .as_array()
            .ok_or_else(|| StoreError::Unexpected("game pass catalog: the answer is not a list".into()))?
            .iter()
            .filter_map(|entry| entry.get("id")?.as_str())
            .filter(|id| is_product_id(id))
            .take(MAX_CATALOG_GAMES)
            .collect();
        if ids.is_empty() {
            return Err(StoreError::Unexpected("game pass catalog: the list names no games".into()));
        }

        let mut games: Vec<OwnedGame> = Vec::new();
        for chunk in ids.chunks(PRODUCTS_PER_REQUEST) {
            let answer = self.get(
                &format!(
                    "{}/v7.0/products?bigIds={}&market={market}&languages=en-us",
                    self.products,
                    chunk.join(",")
                ),
                "game pass products",
            )?;
            let products = answer
                .get("Products")
                .and_then(Value::as_array)
                .ok_or_else(|| StoreError::Unexpected("game pass products: the answer has no products".into()))?;
            for product in products {
                if let Some(game) = game_of(product) {
                    if !games.iter().any(|known| known.game.id.eq_ignore_ascii_case(&game.game.id)) {
                        games.push(game);
                    }
                }
            }
        }
        Ok(games)
    }

    fn get(&self, url: &str, what: &str) -> Result<Value, StoreError> {
        match self.agent.get(url).call() {
            Ok(response) => response
                .into_json::<Value>()
                .map_err(|_| StoreError::Unexpected(format!("{what}: the answer is not JSON"))),
            Err(ureq::Error::Status(status, _)) => Err(StoreError::Unexpected(format!("{what}: HTTP {status}"))),
            Err(ureq::Error::Transport(_)) => Err(StoreError::Unreachable),
        }
    }
}

/// A product id of the Microsoft Store: twelve letters and digits.
fn is_product_id(id: &str) -> bool {
    id.len() == 12 && id.bytes().all(|b| b.is_ascii_alphanumeric())
}

/// The library entry for a product of the Store, if it is a game.
fn game_of(product: &Value) -> Option<OwnedGame> {
    let product_id = product.get("ProductId")?.as_str().filter(|id| is_product_id(id))?.to_ascii_uppercase();
    // Add-ons are products too; the list should not name any, and none is taken
    if product.get("ProductKind").and_then(Value::as_str).is_some_and(|kind| !kind.eq_ignore_ascii_case("Game")) {
        return None;
    }
    let texts = product.pointer("/LocalizedProperties/0")?;
    let name = texts.get("ProductTitle")?.as_str()?.trim();
    if name.is_empty() || name.len() > 200 {
        return None;
    }

    // Known to the scanner, once installed, by the name of its package; a product that names
    // none (a bundle) is known by its Store id until then
    let properties = product.get("Properties");
    let named = |key: &str| properties.and_then(|properties| properties.get(key)).and_then(Value::as_str);
    let identity = named("PackageIdentityName")
        .and_then(package_identity)
        .or_else(|| named("PackageFamilyName").and_then(package_identity));
    let id = match identity {
        Some(identity) => format!("xbox:{identity}"),
        None => format!("xbox:store-{}", product_id.to_ascii_lowercase()),
    };

    Some(OwnedGame::new(
        GamePlatform::Xbox,
        id,
        name,
        clean_game_title(name),
        product_id.clone(),
        Some(format!("{XBOX_APP_PAGE_PREFIX}{product_id}")),
        cover_of(texts),
    ))
}

/// The vertical artwork of a product, at the size of a cover. The Store names its images by
/// an address without scheme (`//store-images.s-microsoft.com/...`) and resizes on request.
fn cover_of(texts: &Value) -> Option<String> {
    let images = texts.get("Images")?.as_array()?;
    let of_purpose = |purpose: &str| {
        images
            .iter()
            .find(|image| image.get("ImagePurpose").and_then(Value::as_str) == Some(purpose))
            .and_then(|image| image.get("Uri")?.as_str())
    };
    let uri = of_purpose("Poster").or_else(|| of_purpose("BoxArt"))?.trim();
    let path = uri.strip_prefix("https://").or_else(|| uri.strip_prefix("//"))?;
    let host = path.split('/').next()?;
    let plain = path.len() > host.len() + 1
        && path.len() <= 300
        && host.ends_with(".s-microsoft.com")
        && path.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_' | b'/'));
    plain.then(|| format!("https://{path}?mode=scale&q=90&h=900&w=600"))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::test_http::{spawn, Request};

    /// Microsoft's public catalog on a local port: a list of 22 products, described twenty
    /// at a time.
    pub(crate) fn spawn_mock_catalog() -> String {
        spawn(|request: &Request| {
            if request.method != "GET" {
                return (404, "{}".to_string());
            }
            if request.path.starts_with(&format!("/sigls/v2?id={PC_GAMES_LIST}&language=en-us&market=")) {
                let mut entries = vec![r#"{"siglId":"fdd9e2a7","title":"All PC Games","description":"","requiresShuffling":"False","imageUrl":""}"#.to_string()];
                entries.push(r#"{"id":"9NBLGGH4R2R6"}"#.to_string());
                entries.push(r#"{"id":"9P2N57MC619K"}"#.to_string());
                entries.push(r#"{"id":"not a product id"}"#.to_string());
                entries.push(r#"{"id":"9ADDON000000"}"#.to_string());
                entries.push(r#"{"id":"9BUNDLE00000"}"#.to_string());
                entries.push(r#"{"id":"9NAMELESS000"}"#.to_string());
                // Filler, so that the products take two requests
                entries.extend((0..17).map(|n| format!(r#"{{"id":"9FILLER000{n:02}"}}"#)));
                return (200, format!("[{}]", entries.join(",")));
            }
            let Some(query) = request.path.strip_prefix("/v7.0/products?bigIds=") else {
                return (404, "{}".to_string());
            };
            if !query.contains("&languages=en-us") {
                return (400, "{}".to_string());
            }
            let ids: Vec<&str> = query.split('&').next().unwrap_or_default().split(',').collect();
            if ids.len() > PRODUCTS_PER_REQUEST {
                return (400, "{}".to_string());
            }
            let products: Vec<String> = ids
                .iter()
                .filter_map(|id| match *id {
                    "9NBLGGH4R2R6" => Some(r#"{"ProductId":"9NBLGGH4R2R6","ProductKind":"Game","LocalizedProperties":[{"ProductTitle":"Forza Horizon 5 Standard Edition","Images":[{"ImagePurpose":"Logo","Uri":"//store-images.s-microsoft.com/image/apps.logo"},{"ImagePurpose":"Poster","Uri":"//store-images.s-microsoft.com/image/apps.49800.13718773309227929.bebdcc0e-1ed5-4778-8732-f4ef65a2f445"}]}],"Properties":{"PackageFamilyName":"Microsoft.624F8B84B80_8wekyb3d8bbwe","PackageIdentityName":"Microsoft.624F8B84B80"}}"#.to_string()),
                    "9P2N57MC619K" => Some(r#"{"ProductId":"9P2N57MC619K","ProductKind":"Game","LocalizedProperties":[{"ProductTitle":"Sea of Thieves: 2026 Edition","Images":[{"ImagePurpose":"BoxArt","Uri":"//store-images.s-microsoft.com/image/apps.sot-box"},{"ImagePurpose":"Screenshot","Uri":"//store-images.s-microsoft.com/image/apps.shot"}]}],"Properties":{"PackageFamilyName":"Microsoft.SeaofThieves_8wekyb3d8bbwe"}}"#.to_string()),
                    "9ADDON000000" => Some(r#"{"ProductId":"9ADDON000000","ProductKind":"Durable","LocalizedProperties":[{"ProductTitle":"Some Add-On"}]}"#.to_string()),
                    "9BUNDLE00000" => Some(r#"{"ProductId":"9BUNDLE00000","ProductKind":"Game","LocalizedProperties":[{"ProductTitle":"A Bundle™","Images":[{"ImagePurpose":"Poster","Uri":"//elsewhere.example/poster"}]}],"Properties":{}}"#.to_string()),
                    "9NAMELESS000" => Some(r#"{"ProductId":"9NAMELESS000","ProductKind":"Game","LocalizedProperties":[{"ProductTitle":"  "}]}"#.to_string()),
                    // Named twice by the list: the same package as Forza
                    "9FILLER00016" => Some(r#"{"ProductId":"9FILLER00016","ProductKind":"Game","LocalizedProperties":[{"ProductTitle":"Forza Horizon 5 Premium Edition"}],"Properties":{"PackageIdentityName":"MICROSOFT.624F8B84B80"}}"#.to_string()),
                    filler if filler.starts_with("9FILLER") => Some(format!(
                        r#"{{"ProductId":"{filler}","ProductKind":"Game","LocalizedProperties":[{{"ProductTitle":"Filler {filler}"}}],"Properties":{{"PackageIdentityName":"Studio.{filler}"}}}}"#
                    )),
                    _ => None,
                })
                .collect();
            (200, format!(r#"{{"Products":[{}]}}"#, products.join(",")))
        })
    }

    #[test]
    fn the_catalog_lists_the_games_of_game_pass_for_pc() {
        let base_url = spawn_mock_catalog();
        let api = GamePassApi::with_services(&base_url, &base_url);

        let games = api.pc_catalog(Lang::It).expect("catalog");
        assert_eq!(games.len(), 19, "two games, a bundle and sixteen fillers; no add-on, no nameless product, no package twice");

        let forza = &games[0].game;
        assert_eq!(forza.id, "xbox:Microsoft.624F8B84B80", "the id the scanner gives the game once installed");
        assert_eq!(forza.title, "Forza Horizon 5 Standard Edition");
        assert_eq!(forza.platform, GamePlatform::Xbox);
        assert!(!forza.is_installed);
        assert_eq!(forza.platform_app_id.as_deref(), Some("9NBLGGH4R2R6"));
        assert_eq!(forza.install_uri().as_deref(), Some("msxbox://game/?productId=9NBLGGH4R2R6"));
        assert_eq!(
            forza.cover_path.as_deref(),
            Some("https://store-images.s-microsoft.com/image/apps.49800.13718773309227929.bebdcc0e-1ed5-4778-8732-f4ef65a2f445?mode=scale&q=90&h=900&w=600")
        );
        assert!(is_catalog_entry(forza));

        let thieves = &games[1].game;
        assert_eq!(thieves.id, "xbox:Microsoft.SeaofThieves", "from the family name when the package is not named alone");
        assert!(thieves.cover_path.as_deref().is_some_and(|url| url.contains("apps.sot-box")), "box art where there is no poster");

        let bundle = &games[2].game;
        assert_eq!(bundle.id, "xbox:store-9bundle00000");
        assert_eq!(bundle.clean_title, "A Bundle");
        assert_eq!(bundle.cover_path, None, "artwork is taken from the Store's own image host only");
    }

    #[test]
    fn a_catalog_that_cannot_be_read_is_an_error_not_an_empty_catalog() {
        let dead = GamePassApi::with_services("http://127.0.0.1:9", "http://127.0.0.1:9");
        assert_eq!(dead.pc_catalog(Lang::En), Err(StoreError::Unreachable));

        // The list answers, the products do not
        let base_url = spawn_mock_catalog();
        let half = GamePassApi::with_services(&base_url, "http://127.0.0.1:9");
        assert_eq!(half.pc_catalog(Lang::En), Err(StoreError::Unreachable));

        let empty = spawn(|_request: &Request| (200, r#"[{"siglId":"x","title":"All PC Games"}]"#.to_string()));
        let api = GamePassApi::with_services(&empty, &empty);
        assert!(matches!(api.pc_catalog(Lang::En), Err(StoreError::Unexpected(_))));
    }

    #[test]
    fn only_entries_the_catalog_brought_are_catalog_entries() {
        let base_url = spawn_mock_catalog();
        let mut game = GamePassApi::with_services(&base_url, &base_url).pc_catalog(Lang::Es).unwrap().remove(0).game;
        assert!(is_catalog_entry(&game));
        game.is_installed = true;
        assert!(!is_catalog_entry(&game), "an installed game is nobody's to remove");
        game.is_installed = false;
        game.launch_uri = Some("ms-windows-store://pdp/?PFN=Microsoft.624F8B84B80_8wekyb3d8bbwe".into());
        assert!(!is_catalog_entry(&game), "a game the account itself has played");
        game.launch_uri = Some(format!("{XBOX_APP_PAGE_PREFIX}9NBLGGH4R2R6"));
        game.platform = GamePlatform::Steam;
        assert!(!is_catalog_entry(&game));
    }
}
