//! The weather at a place the user named, for the background of the Home page.
//!
//! Off until the user turns it on and chooses a place in Settings. Then, and only then, Kanso
//! asks [Open-Meteo](https://open-meteo.com) two things: where a place name is (once, when the
//! user searches for it) and what the weather is at the place's coordinates, which Kanso
//! keeps rounded to about a kilometre. No key, no account, nothing about the user or the
//! library is sent: a place name, or two numbers.

use crate::config::WeatherPlace;
use crate::i18n::Lang;
use serde::Serialize;
use serde_json::Value;
use std::time::Duration;

const DEFAULT_FORECAST_BASE_URL: &str = "https://api.open-meteo.com";
const DEFAULT_GEOCODING_BASE_URL: &str = "https://geocoding-api.open-meteo.com";
/// Where the data comes from, for the credit shown next to it.
pub const WEATHER_SOURCE_URL: &str = "https://open-meteo.com/";
/// Places offered for a name: enough to tell two towns of the same name apart.
const MAX_PLACES: usize = 6;

/// The weather right now at a place.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WeatherNow {
    /// WMO weather code: 0 clear, 61–65 rain, 71–75 snow, 95 thunderstorm…
    pub code: u32,
    pub is_day: bool,
}

/// A place found for a name, with what tells it from its namesakes.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaceMatch {
    pub name: String,
    /// Region and country, as one line: "Piemonte, Italia".
    pub region: String,
    pub latitude: f64,
    pub longitude: f64,
}

pub struct WeatherClient {
    forecast_base_url: String,
    geocoding_base_url: String,
    agent: ureq::Agent,
}

impl Default for WeatherClient {
    fn default() -> Self {
        Self::new()
    }
}

impl WeatherClient {
    pub fn new() -> Self {
        Self::with_base_urls(DEFAULT_FORECAST_BASE_URL, DEFAULT_GEOCODING_BASE_URL)
    }

    pub fn with_base_urls(forecast: &str, geocoding: &str) -> Self {
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_secs(8))
            .timeout_read(Duration::from_secs(12))
            .user_agent(crate::USER_AGENT)
            .build();
        Self {
            forecast_base_url: forecast.trim_end_matches('/').to_string(),
            geocoding_base_url: geocoding.trim_end_matches('/').to_string(),
            agent,
        }
    }

    fn get_json(&self, url: &str) -> Result<Value, String> {
        match self.agent.get(url).set("Accept", "application/json").call() {
            Ok(response) => response.into_json().map_err(|e| e.to_string()),
            // The service explains a refusal in the body: say that, not just the status
            Err(ureq::Error::Status(status, response)) => Err(response
                .into_json::<Value>()
                .ok()
                .and_then(|body| body.get("reason").and_then(Value::as_str).map(str::to_string))
                .unwrap_or_else(|| format!("HTTP {status}"))),
            Err(other) => Err(other.to_string()),
        }
    }

    /// The weather now at `place`. Only its coordinates are sent.
    pub fn current(&self, place: &WeatherPlace) -> Result<WeatherNow, String> {
        let url = format!(
            "{}/v1/forecast?latitude={:.2}&longitude={:.2}&current=weather_code,is_day",
            self.forecast_base_url, place.latitude, place.longitude
        );
        parse_current(&self.get_json(&url)?)
    }

    /// The places called `query`, most populous first as the service orders them.
    pub fn search_places(&self, query: &str, lang: Lang) -> Result<Vec<PlaceMatch>, String> {
        let query: String = query.trim().chars().filter(|c| !c.is_control()).take(60).collect();
        if query.chars().count() < 2 {
            return Ok(Vec::new());
        }
        let url = format!(
            "{}/v1/search?name={}&count={MAX_PLACES}&language={}&format=json",
            self.geocoding_base_url,
            percent_encode(&query),
            lang.code()
        );
        Ok(parse_places(&self.get_json(&url)?))
    }
}

/// A query value for a URL: everything but letters, digits and a few marks is escaped.
fn percent_encode(text: &str) -> String {
    text.bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (byte as char).to_string(),
            other => format!("%{other:02X}"),
        })
        .collect()
}

/// Reads `current.weather_code` and `current.is_day` of a forecast answer.
pub fn parse_current(answer: &Value) -> Result<WeatherNow, String> {
    if answer.get("error").and_then(Value::as_bool) == Some(true) {
        return Err(answer.get("reason").and_then(Value::as_str).unwrap_or("error").to_string());
    }
    let current = answer.get("current").ok_or("current")?;
    let code = current
        .get("weather_code")
        .and_then(Value::as_f64)
        .filter(|code| (0.0..=99.0).contains(code))
        .ok_or("weather_code")?;
    Ok(WeatherNow {
        code: code as u32,
        // Without it, daylight is the safer guess: the night look is the special one
        is_day: current.get("is_day").and_then(Value::as_f64).is_none_or(|day| day >= 0.5),
    })
}

/// Reads the `results` of a place search; anything without a name or coordinates is left out.
pub fn parse_places(answer: &Value) -> Vec<PlaceMatch> {
    let plain = |value: Option<&Value>| -> String {
        value
            .and_then(Value::as_str)
            .map(|text| text.trim().chars().filter(|c| !c.is_control()).take(80).collect())
            .unwrap_or_default()
    };
    answer
        .get("results")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .filter_map(|place| {
            let name = plain(place.get("name"));
            let latitude = place.get("latitude").and_then(Value::as_f64)?;
            let longitude = place.get("longitude").and_then(Value::as_f64)?;
            if name.is_empty() || !(-90.0..=90.0).contains(&latitude) || !(-180.0..=180.0).contains(&longitude) {
                return None;
            }
            let region = [plain(place.get("admin1")), plain(place.get("country"))]
                .into_iter()
                .filter(|part| !part.is_empty())
                .collect::<Vec<_>>()
                .join(", ");
            Some(PlaceMatch {
                name,
                region,
                // As coarse as Kanso keeps it
                latitude: (latitude * 100.0).round() / 100.0,
                longitude: (longitude * 100.0).round() / 100.0,
            })
        })
        .take(MAX_PLACES)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn reads_the_weather_of_a_forecast_answer() {
        // The shape Open-Meteo documents for `current=weather_code,is_day`
        let answer = json!({
            "latitude": 45.06, "longitude": 7.7, "timezone": "GMT",
            "current_units": {"time": "iso8601", "interval": "seconds", "weather_code": "wmo code", "is_day": ""},
            "current": {"time": "2026-11-16T21:00", "interval": 900, "weather_code": 71, "is_day": 0}
        });
        assert_eq!(parse_current(&answer), Ok(WeatherNow { code: 71, is_day: false }));
        assert_eq!(
            parse_current(&json!({"current": {"weather_code": 3.0}})),
            Ok(WeatherNow { code: 3, is_day: true }),
            "no word about the day: daylight"
        );
        // What the service really answers when it refuses (seen from a shared address)
        let refused = json!({"error": true, "reason": "Daily API request limit exceeded. Please try again tomorrow."});
        assert_eq!(parse_current(&refused), Err("Daily API request limit exceeded. Please try again tomorrow.".to_string()));
        // Not a forecast, or a code that is not one
        assert!(parse_current(&json!({})).is_err());
        assert!(parse_current(&json!({"current": {"weather_code": "rain"}})).is_err());
        assert!(parse_current(&json!({"current": {"weather_code": 480}})).is_err());
    }

    #[test]
    fn reads_the_places_found_for_a_name() {
        // Cut from a real answer to `search?name=Torino&language=it`
        let answer = json!({"results": [
            {"id": 3165524, "name": "Torino", "latitude": 45.07049, "longitude": 7.68682, "elevation": 239.0, "country_code": "IT", "timezone": "Europe/Rome", "population": 847287, "country": "Italia", "admin1": "Piemonte", "admin2": "Provincia di Torino"},
            {"id": 482079, "name": "Torino", "latitude": 58.07222, "longitude": 43.84208, "country_code": "RU", "country": "Russia", "admin1": "oblast' di Kostroma"},
            {"id": 1, "name": "Nowhere", "latitude": 123.0, "longitude": 0.0},
            {"id": 2, "name": "", "latitude": 1.0, "longitude": 1.0},
            {"id": 3, "name": "No coordinates"},
            {"id": 4, "name": "Isola\u{7}", "latitude": -8.5, "longitude": 179.2}
        ]});
        let places = parse_places(&answer);
        assert_eq!(
            places,
            [
                PlaceMatch { name: "Torino".into(), region: "Piemonte, Italia".into(), latitude: 45.07, longitude: 7.69 },
                PlaceMatch { name: "Torino".into(), region: "oblast' di Kostroma, Russia".into(), latitude: 58.07, longitude: 43.84 },
                PlaceMatch { name: "Isola".into(), region: String::new(), latitude: -8.5, longitude: 179.2 },
            ],
            "namesakes are told apart by region; coordinates are kept coarse"
        );
        assert!(parse_places(&json!({"generationtime_ms": 0.4})).is_empty(), "no results is no places");
        assert!(parse_places(&json!({"results": "none"})).is_empty());
    }

    #[test]
    fn a_place_name_is_sent_as_one_plain_value() {
        assert_eq!(percent_encode("Reggio nell'Emilia"), "Reggio%20nell%27Emilia");
        assert_eq!(percent_encode("São Paulo&count=99"), "S%C3%A3o%20Paulo%26count%3D99");
        // Too short to search for: nothing is asked
        assert_eq!(WeatherClient::with_base_urls("http://127.0.0.1:9", "http://127.0.0.1:9").search_places(" a ", Lang::It), Ok(Vec::new()));
    }
}
