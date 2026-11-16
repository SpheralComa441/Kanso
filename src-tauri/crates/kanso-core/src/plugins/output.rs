//! What a plugin may show: plain data that Kanso draws with its own components.
//!
//! A plugin never produces markup, styles or scripts. Whatever it returns is read here into
//! these types: unknown pieces are dropped, texts are cut to size, links must be `https`.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use url::Url;

const MAX_SECTIONS: usize = 6;
const MAX_BLOCKS: usize = 12;
const MAX_ITEMS: usize = 24;
const MAX_GAMES: usize = 12;
const BADGE_CHARS: usize = 14;
const BADGE_LABEL_CHARS: usize = 24;
const HINT_CHARS: usize = 40;
const LABEL_CHARS: usize = 80;
const LINE_CHARS: usize = 240;
const TEXT_CHARS: usize = 1200;
const URL_CHARS: usize = 600;

/// Short label for a game card.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Badge {
    pub text: String,
    /// What the figure is ("To finish", "Steam reviews"), shown next to it where there is room.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tooltip: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatItem {
    pub label: String,
    pub value: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListItem {
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkItem {
    pub label: String,
    pub url: String,
}

/// One piece of a section.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Block {
    /// Small label/value tiles.
    Stats { items: Vec<StatItem> },
    /// A paragraph of plain text.
    Text { text: String },
    /// Rows with a title, an optional second line and an optional link.
    List { items: Vec<ListItem> },
    /// Buttons that open a web page in the browser.
    Links { items: Vec<LinkItem> },
    /// A bar filled to `value` out of `max`.
    #[serde(rename_all = "camelCase")]
    Meter {
        label: String,
        value: f64,
        max: f64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        value_label: Option<String>,
    },
    /// A hint for the user (`tone`: `info` or `warning`), e.g. "add your API key".
    Notice { text: String, tone: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Section {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub blocks: Vec<Block>,
}

/// Answer of `gameDetails(game)`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginGameData {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub badge: Option<Badge>,
    #[serde(default)]
    pub sections: Vec<Section>,
}

impl PluginGameData {
    pub fn is_empty(&self) -> bool {
        self.badge.is_none() && self.sections.is_empty()
    }
}

/// A game a command points at.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GamePick {
    pub id: String,
    pub note: Option<String>,
    /// Shown on the card while it is still covered ([`GamesStyle::Mystery`]).
    pub hint: Option<String>,
}

/// How the games of a command are laid out. A name from this list, nothing else.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum GamesStyle {
    /// Rows: small cover, title, note, buttons.
    #[default]
    List,
    /// Whole covers, as in the library.
    Cards,
    /// Whole covers, face down until the user turns each one.
    Mystery,
}

impl GamesStyle {
    fn from_name(name: &str) -> Option<Self> {
        match name {
            "list" => Some(Self::List),
            "cards" => Some(Self::Cards),
            "mystery" => Some(Self::Mystery),
            _ => None,
        }
    }
}

/// Answer of a command: what the dialog shows.
#[derive(Debug, Clone, PartialEq, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginView {
    pub title: Option<String>,
    pub sections: Vec<Section>,
    pub games: Vec<GamePick>,
    pub games_style: GamesStyle,
}

/// Reads what `gameDetails` returned. `null`/`undefined` mean "nothing for this game".
pub fn parse_game_data(value: &Value) -> PluginGameData {
    PluginGameData {
        badge: value.get("badge").and_then(parse_badge),
        sections: parse_sections(value.get("sections")),
    }
}

/// Reads what a command returned.
pub fn parse_view(value: &Value) -> PluginView {
    let games = value
        .get("games")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| match item {
                    Value::String(id) => Some(GamePick {
                        id: line(id, LINE_CHARS)?,
                        note: None,
                        hint: None,
                    }),
                    other => Some(GamePick {
                        id: text_of(other, "id", LINE_CHARS)?,
                        note: text_of(other, "note", LINE_CHARS),
                        hint: text_of(other, "hint", HINT_CHARS),
                    }),
                })
                .take(MAX_GAMES)
                .collect()
        })
        .unwrap_or_default();
    PluginView {
        title: text_of(value, "title", LABEL_CHARS),
        sections: parse_sections(value.get("sections")),
        games,
        // An unknown name is not a layout: the plain list
        games_style: value
            .get("gamesStyle")
            .and_then(Value::as_str)
            .and_then(GamesStyle::from_name)
            .unwrap_or_default(),
    }
}

fn parse_badge(value: &Value) -> Option<Badge> {
    let (text, label, tooltip) = match value {
        Value::String(text) => (line(text, BADGE_CHARS)?, None, None),
        other => (
            text_of(other, "text", BADGE_CHARS)?,
            text_of(other, "label", BADGE_LABEL_CHARS),
            text_of(other, "tooltip", LINE_CHARS),
        ),
    };
    Some(Badge { text, label, tooltip })
}

fn parse_sections(value: Option<&Value>) -> Vec<Section> {
    let Some(items) = value.and_then(Value::as_array) else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|section| {
            let blocks: Vec<Block> = section
                .get("blocks")
                .and_then(Value::as_array)?
                .iter()
                .filter_map(parse_block)
                .take(MAX_BLOCKS)
                .collect();
            (!blocks.is_empty()).then(|| Section {
                title: text_of(section, "title", LABEL_CHARS),
                blocks,
            })
        })
        .take(MAX_SECTIONS)
        .collect()
}

fn parse_block(value: &Value) -> Option<Block> {
    let items = || {
        value
            .get("items")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or_default()
            .iter()
    };
    match value.get("type")?.as_str()? {
        "stats" => {
            let items: Vec<StatItem> = items()
                .filter_map(|item| {
                    Some(StatItem {
                        label: text_of(item, "label", LABEL_CHARS)?,
                        value: text_of(item, "value", LABEL_CHARS)?,
                        hint: text_of(item, "hint", LINE_CHARS),
                    })
                })
                .take(MAX_ITEMS)
                .collect();
            (!items.is_empty()).then_some(Block::Stats { items })
        }
        "text" => Some(Block::Text {
            text: text_of(value, "text", TEXT_CHARS)?,
        }),
        "list" => {
            let items: Vec<ListItem> = items()
                .filter_map(|item| {
                    Some(ListItem {
                        title: text_of(item, "title", LINE_CHARS)?,
                        subtitle: text_of(item, "subtitle", LINE_CHARS),
                        url: link_of(item, "url"),
                    })
                })
                .take(MAX_ITEMS)
                .collect();
            (!items.is_empty()).then_some(Block::List { items })
        }
        "links" => {
            let items: Vec<LinkItem> = items()
                .filter_map(|item| {
                    Some(LinkItem {
                        label: text_of(item, "label", LABEL_CHARS)?,
                        url: link_of(item, "url")?,
                    })
                })
                .take(MAX_ITEMS)
                .collect();
            (!items.is_empty()).then_some(Block::Links { items })
        }
        "meter" => {
            let number = |key: &str| value.get(key).and_then(Value::as_f64).filter(|n| n.is_finite());
            let max = number("max").filter(|max| *max > 0.0)?;
            Some(Block::Meter {
                label: text_of(value, "label", LABEL_CHARS)?,
                value: number("value")?.clamp(0.0, max),
                max,
                value_label: text_of(value, "valueLabel", LABEL_CHARS),
            })
        }
        "notice" => Some(Block::Notice {
            text: text_of(value, "text", TEXT_CHARS)?,
            tone: match value.get("tone").and_then(Value::as_str) {
                Some("warning") => "warning",
                _ => "info",
            }
            .to_string(),
        }),
        _ => None,
    }
}

/// A text field as a trimmed, size-limited string; numbers are accepted and written out.
fn text_of(value: &Value, key: &str, max_chars: usize) -> Option<String> {
    match value.get(key)? {
        Value::String(text) => line(text, max_chars),
        Value::Number(number) => Some(number.to_string()),
        _ => None,
    }
}

fn line(raw: &str, max_chars: usize) -> Option<String> {
    // Control characters (new lines aside) have no place in a label
    let cleaned: String = raw
        .trim()
        .chars()
        .filter(|c| !c.is_control() || *c == '\n')
        .take(max_chars)
        .collect();
    (!cleaned.is_empty()).then_some(cleaned)
}

fn link_of(value: &Value, key: &str) -> Option<String> {
    let raw = value.get(key)?.as_str()?.trim();
    is_safe_link(raw).then(|| raw.to_string())
}

/// Links open in the user's browser: only well-formed `https` addresses with a host and no
/// embedded credentials are let through.
pub fn is_safe_link(raw: &str) -> bool {
    if raw.len() > URL_CHARS {
        return false;
    }
    Url::parse(raw).is_ok_and(|url| {
        url.scheme() == "https"
            && url.host_str().is_some_and(|host| host.contains('.'))
            && url.username().is_empty()
            && url.password().is_none()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn reads_every_kind_of_block() {
        let data = parse_game_data(&json!({
            "badge": {"text": "12 h", "label": "To finish, and a good deal more than fits", "tooltip": "Main story"},
            "sections": [{
                "title": "HowLongToBeat",
                "blocks": [
                    {"type": "stats", "items": [
                        {"label": "Main", "value": "12 h"},
                        {"label": "Extra", "value": 20, "hint": "with side quests"},
                        {"label": "no value"}
                    ]},
                    {"type": "text", "text": "  Plain words.  "},
                    {"type": "list", "items": [
                        {"title": "Patch 1.2", "subtitle": "yesterday", "url": "https://example.org/news/1"},
                        {"title": "Bad link", "url": "javascript:alert(1)"}
                    ]},
                    {"type": "links", "items": [
                        {"label": "Open", "url": "https://example.org/game"},
                        {"label": "Local file", "url": "file:///C:/Windows/win.ini"},
                        {"label": "Plain http", "url": "http://example.org"}
                    ]},
                    {"type": "meter", "label": "Positive", "value": 150, "max": 100, "valueLabel": "92%"},
                    {"type": "notice", "text": "Add your key", "tone": "warning"},
                    {"type": "notice", "text": "FYI", "tone": "<b>"},
                    {"type": "html", "html": "<script>alert(1)</script>"},
                    {"type": "meter", "label": "Broken", "value": 1, "max": 0}
                ]
            }]
        }));

        assert_eq!(data.badge, Some(Badge {
                text: "12 h".into(),
                label: Some("To finish, and a good de".into()),
                tooltip: Some("Main story".into())
            }));
        let blocks = &data.sections[0].blocks;
        assert_eq!(blocks.len(), 7, "unknown and invalid blocks are dropped: {blocks:?}");
        assert_eq!(
            blocks[0],
            Block::Stats {
                items: vec![
                    StatItem { label: "Main".into(), value: "12 h".into(), hint: None },
                    StatItem { label: "Extra".into(), value: "20".into(), hint: Some("with side quests".into()) },
                ]
            }
        );
        assert_eq!(blocks[1], Block::Text { text: "Plain words.".into() });
        let Block::List { items } = &blocks[2] else { panic!("list") };
        assert_eq!(items[0].url.as_deref(), Some("https://example.org/news/1"));
        assert_eq!(items[1].url, None, "the row stays, the unsafe link goes");
        let Block::Links { items } = &blocks[3] else { panic!("links") };
        assert_eq!(items.len(), 1, "only https links: {items:?}");
        assert_eq!(
            blocks[4],
            Block::Meter { label: "Positive".into(), value: 100.0, max: 100.0, value_label: Some("92%".into()) }
        );
        assert_eq!(blocks[5], Block::Notice { text: "Add your key".into(), tone: "warning".into() });
        assert_eq!(blocks[6], Block::Notice { text: "FYI".into(), tone: "info".into() });

        // What is stored in the cache reads back identically
        let stored = serde_json::to_string(&data).unwrap();
        assert_eq!(serde_json::from_str::<PluginGameData>(&stored).unwrap(), data);
    }

    #[test]
    fn nothing_and_nonsense_become_an_empty_answer() {
        for value in [json!(null), json!("text"), json!(42), json!({"sections": "no"}), json!({"sections": [{"blocks": []}]})] {
            assert!(parse_game_data(&value).is_empty(), "{value}");
        }
        let long = "x".repeat(5000);
        let data = parse_game_data(&json!({
            "badge": long,
            "sections": (0..20).map(|_| json!({"title": long, "blocks": [{"type": "text", "text": long}]})).collect::<Vec<_>>()
        }));
        assert_eq!(data.badge.unwrap().text.chars().count(), BADGE_CHARS);
        assert_eq!(data.sections.len(), MAX_SECTIONS);
        assert_eq!(data.sections[0].title.as_deref().map(str::len), Some(LABEL_CHARS));
    }

    #[test]
    fn reads_a_command_view_with_game_picks() {
        let view = parse_view(&json!({
            "title": "Tonight",
            "sections": [{"blocks": [{"type": "text", "text": "Try this."}]}],
            "gamesStyle": "mystery",
            "games": ["steam:1", {"id": "epic:x", "note": "Never played", "hint": "Backlog"}, {"note": "no id"}, 7]
        }));
        assert_eq!(view.title.as_deref(), Some("Tonight"));
        assert_eq!(view.sections.len(), 1);
        assert_eq!(
            view.games,
            vec![
                GamePick { id: "steam:1".into(), note: None, hint: None },
                GamePick { id: "epic:x".into(), note: Some("Never played".into()), hint: Some("Backlog".into()) },
            ]
        );
        assert_eq!(view.games_style, GamesStyle::Mystery);
        assert_eq!(parse_view(&json!({"gamesStyle": "cards"})).games_style, GamesStyle::Cards);
        // A layout is a name from the list, never anything a plugin makes up
        for other in [json!("carousel"), json!("<style>"), json!(3), json!({"css": "x"})] {
            assert_eq!(parse_view(&json!({"gamesStyle": other})).games_style, GamesStyle::List);
        }
        assert_eq!(parse_view(&json!(null)), PluginView::default());
    }

    #[test]
    fn links_must_be_plain_https() {
        assert!(is_safe_link("https://www.pcgamingwiki.com/wiki/Portal_2"));
        for bad in [
            "http://example.org",
            "https://user:pass@example.org",
            "https://localhost/x",
            "steam://run/1",
            "//example.org",
            "",
        ] {
            assert!(!is_safe_link(bad), "{bad}");
        }
    }
}
