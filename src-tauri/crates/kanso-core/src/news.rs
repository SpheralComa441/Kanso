//! PC gaming and hardware news, read from the publishers' own public feeds (RSS and Atom).
//!
//! Kanso asks each site the user keeps for its feed, the same file a feed reader asks for,
//! and shows headline, a few words of summary, picture, source and date; the article itself
//! opens on the publisher's site, in the user's browser. Feeds are text written by others:
//! everything taken from one is reduced to plain text of bounded length, pictures and links
//! are kept only if they are `https`, and a link only if it points to the site of the feed.
//!
//! Feeds in the wild are often not strictly valid XML, so they are not handed to an XML
//! parser: the few elements wanted are looked for by name, leniently.

use crate::i18n::Lang;
use crate::models::now_timestamp_ms;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::io::Read;
use std::time::Duration;

/// The largest feed read: real ones are a few hundred kilobytes.
const MAX_FEED_BYTES: u64 = 4 * 1024 * 1024;
/// Items kept from one feed, newest first.
const MAX_ITEMS_PER_FEED: usize = 30;
const MAX_TITLE_CHARS: usize = 200;
const MAX_SUMMARY_CHARS: usize = 280;

/// What a source writes about.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NewsTopic {
    #[default]
    Gaming,
    Hardware,
}

/// A site whose feed Kanso can read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NewsSource {
    /// How the settings name it: lower-case letters, digits and dashes.
    pub id: &'static str,
    pub name: &'static str,
    pub lang: Lang,
    pub topic: NewsTopic,
    pub feed_url: &'static str,
    /// The site's own address, opened from the source's name.
    pub site_url: &'static str,
    /// The hosts its articles are on: a link elsewhere is not kept. A host stands for its
    /// subdomains too.
    pub hosts: &'static [&'static str],
}

/// A feed the user added by hand in the settings.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CustomFeed {
    pub name: String,
    /// The feed's address, `https`.
    pub url: String,
    pub topic: NewsTopic,
}

/// Most feeds a user can add.
pub const MAX_CUSTOM_FEEDS: usize = 12;

impl CustomFeed {
    /// The host of the feed's address, without the label sites put in front of their feeds
    /// (`www.`, `feeds.`, `rss.`): the site its articles are on.
    pub fn site_host(&self) -> Option<String> {
        let rest = self.url.strip_prefix("https://")?;
        let host = rest.split(['/', '?', '#']).next()?.to_ascii_lowercase();
        if host.is_empty() || host.contains(['@', ':']) || !host.contains('.') {
            return None;
        }
        let site = ["www.", "feeds.", "feed.", "rss."]
            .iter()
            .find_map(|label| host.strip_prefix(label).filter(|rest| rest.contains('.')))
            .unwrap_or(&host);
        Some(site.to_string())
    }

    /// The feed as it may be kept: a plain name, an `https` address with nothing odd in it.
    /// `None` when the address is not one.
    pub fn sanitized(self) -> Option<Self> {
        let url = self.url.trim().to_string();
        if !is_picture_url(&url) || url.len() > 300 {
            return None;
        }
        let feed = Self { name: String::new(), url, topic: self.topic };
        let host = feed.site_host()?;
        let name: String = self.name.trim().chars().filter(|c| !c.is_control()).take(40).collect();
        Some(Self { name: if name.is_empty() { host } else { name }, ..feed })
    }
}

/// A feed to read, whether one Kanso knows or one the user added.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Feed {
    pub id: String,
    pub name: String,
    pub topic: NewsTopic,
    pub feed_url: String,
    /// The hosts its articles are on: a link elsewhere is not kept.
    pub hosts: Vec<String>,
}

impl From<&NewsSource> for Feed {
    fn from(source: &NewsSource) -> Self {
        Self {
            id: source.id.to_string(),
            name: source.name.to_string(),
            topic: source.topic,
            feed_url: source.feed_url.to_string(),
            hosts: source.hosts.iter().map(|host| host.to_string()).collect(),
        }
    }
}

impl Feed {
    /// The `index`-th feed the user added. Its articles are expected on the feed's own site.
    pub fn custom(index: usize, feed: &CustomFeed) -> Option<Self> {
        Some(Self {
            id: format!("custom-{index}"),
            name: feed.name.clone(),
            topic: feed.topic,
            feed_url: feed.url.clone(),
            hosts: vec![feed.site_host()?],
        })
    }
}

/// One piece of news.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewsItem {
    /// Unique across sources: `<source>:<link>`.
    pub id: String,
    pub source: String,
    pub source_name: String,
    pub topic: NewsTopic,
    pub title: String,
    /// The article, `https` on the source's own site.
    pub url: String,
    /// The first words of the article, as plain text; empty when the feed gives none.
    pub summary: String,
    pub image_url: Option<String>,
    /// When it was published, in milliseconds since the epoch.
    pub published_at: Option<u64>,
}

/// What the News page shows.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewsReport {
    /// Newest first, across the sources asked.
    pub items: Vec<NewsItem>,
    /// Sources that could not be read; the others are still listed.
    pub failed_sources: Vec<String>,
    pub fetched_at: u64,
}

/// A source as the settings and the page list it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NewsSourceInfo {
    pub id: String,
    pub name: String,
    pub lang: String,
    pub topic: NewsTopic,
    pub site_url: String,
}

impl From<&NewsSource> for NewsSourceInfo {
    fn from(source: &NewsSource) -> Self {
        Self {
            id: source.id.to_string(),
            name: source.name.to_string(),
            lang: source.lang.code().to_string(),
            topic: source.topic,
            site_url: source.site_url.to_string(),
        }
    }
}

include!("news_sources.rs");

/// The source with this id, if Kanso knows it.
pub fn source_by_id(id: &str) -> Option<&'static NewsSource> {
    NEWS_SOURCES.iter().find(|source| source.id == id)
}

/// The sources read for a language unless the user chose otherwise: the ones in that
/// language, or the English ones where there are none.
pub fn default_sources(lang: Lang) -> Vec<&'static NewsSource> {
    let own: Vec<&NewsSource> = NEWS_SOURCES.iter().filter(|source| source.lang == lang).collect();
    if own.is_empty() {
        NEWS_SOURCES.iter().filter(|source| source.lang == Lang::En).collect()
    } else {
        own
    }
}

/// Whether `url` is an `https` address on `host` or one of its subdomains, with nothing odd
/// in it.
fn is_on_host<H: AsRef<str>>(url: &str, hosts: &[H]) -> bool {
    let Some(rest) = url.strip_prefix("https://") else { return false };
    if url.len() > 600 || url.chars().any(|c| c.is_whitespace() || c.is_control() || matches!(c, '"' | '<' | '>' | '\\')) {
        return false;
    }
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    // No user names, no ports: a plain host
    if authority.is_empty() || authority.contains(['@', ':']) {
        return false;
    }
    let host = authority.to_ascii_lowercase();
    hosts.iter().any(|allowed| host == allowed.as_ref() || host.ends_with(&format!(".{}", allowed.as_ref())))
}

/// Whether `url` may be opened as a piece of news: `https`, on the site of a source Kanso
/// knows or of a feed the user added. Links come from remote feeds, so they are checked
/// again before the browser opens.
pub fn is_allowed_news_url(url: &str, custom: &[CustomFeed]) -> bool {
    NEWS_SOURCES.iter().any(|source| is_on_host(url, source.hosts))
        || custom.iter().filter_map(CustomFeed::site_host).any(|host| is_on_host(url, &[host]))
}

/// Whether `url` can be shown as a picture: `https`, on any host, with nothing odd in it.
fn is_picture_url(url: &str) -> bool {
    url.starts_with("https://")
        && url.len() <= 600
        && !url.chars().any(|c| c.is_whitespace() || c.is_control() || matches!(c, '"' | '<' | '>' | '\\'))
}

/* ---------------------------------------------------------------- reading */

/// The pieces of `xml` between `<name …>` and `</name>`, outermost only.
fn blocks<'a>(xml: &'a str, name: &str) -> Vec<&'a str> {
    let close = format!("</{name}>");
    let mut found = Vec::new();
    let mut rest = xml;
    while let Some((_, after_open)) = open_tag(rest, name) {
        let Some(end) = after_open.body.find(&close) else { break };
        found.push(&after_open.body[..end]);
        rest = &after_open.body[end + close.len()..];
    }
    found
}

/// What follows an opening tag: its attributes and everything after its `>`.
struct Opened<'a> {
    attributes: &'a str,
    body: &'a str,
    /// `<name … />`: no content of its own.
    empty: bool,
}

/// The first `<name` in `xml` that is that element (not `<namesake`), with what follows.
fn open_tag<'a>(xml: &'a str, name: &str) -> Option<(usize, Opened<'a>)> {
    let open = format!("<{name}");
    let mut from = 0;
    while let Some(at) = xml[from..].find(&open) {
        let start = from + at;
        let after = &xml[start + open.len()..];
        match after.chars().next() {
            Some(c) if c == '>' || c == '/' || c.is_whitespace() => {
                let end = after.find('>')?;
                let attributes = after[..end].trim_end_matches('/');
                return Some((start, Opened { attributes, body: &after[end + 1..], empty: after[..end].ends_with('/') }));
            }
            _ => from = start + open.len(),
        }
    }
    None
}

/// The raw content of the first `<name>` in `block`, CDATA unwrapped and, outside CDATA, XML
/// entities resolved: what the feed's author wrote, which may itself be HTML.
fn content(block: &str, name: &str) -> Option<String> {
    let (_, opened) = open_tag(block, name)?;
    if opened.empty {
        return None;
    }
    let end = opened.body.find(&format!("</{name}>"))?;
    let raw = opened.body[..end].trim();
    Some(match raw.strip_prefix("<![CDATA[") {
        Some(inner) => inner.strip_suffix("]]>").unwrap_or(inner).replace("]]><![CDATA[", ""),
        None => decode_entities(raw),
    })
}

/// The value of `attribute` in a tag's attributes, entities resolved.
fn attribute(attributes: &str, attribute: &str) -> Option<String> {
    let mut from = 0;
    while let Some(at) = attributes[from..].find(attribute) {
        let start = from + at;
        from = start + attribute.len();
        // The name itself, not the end of a longer one
        if start > 0 && !attributes[..start].ends_with(|c: char| c.is_whitespace()) {
            continue;
        }
        let rest = attributes[from..].trim_start();
        let Some(rest) = rest.strip_prefix('=') else { continue };
        let rest = rest.trim_start();
        let quote = rest.chars().next().filter(|c| *c == '"' || *c == '\'')?;
        let value = &rest[1..];
        return value.find(quote).map(|end| decode_entities(&value[..end]));
    }
    None
}

/// Every `<name …>` in `block`, as its attributes.
fn tags<'a>(block: &'a str, name: &str) -> Vec<&'a str> {
    let mut found = Vec::new();
    let mut rest = block;
    while let Some((_, opened)) = open_tag(rest, name) {
        found.push(opened.attributes);
        rest = opened.body;
    }
    found
}

/// Named characters feeds use beyond the five of XML.
const NAMED: &[(&str, &str)] = &[
    ("amp", "&"), ("lt", "<"), ("gt", ">"), ("quot", "\""), ("apos", "'"), ("nbsp", " "),
    ("hellip", "…"), ("mdash", "—"), ("ndash", "–"), ("lsquo", "‘"), ("rsquo", "’"), ("ldquo", "“"), ("rdquo", "”"),
    ("laquo", "«"), ("raquo", "»"), ("euro", "€"), ("copy", "©"), ("reg", "®"), ("trade", "™"), ("deg", "°"), ("times", "×"),
    ("agrave", "à"), ("egrave", "è"), ("eacute", "é"), ("igrave", "ì"), ("ograve", "ò"), ("ugrave", "ù"),
    ("Agrave", "À"), ("Egrave", "È"), ("Eacute", "É"), ("Igrave", "Ì"), ("Ograve", "Ò"), ("Ugrave", "Ù"),
    ("aacute", "á"), ("iacute", "í"), ("oacute", "ó"), ("uacute", "ú"), ("ntilde", "ñ"), ("uuml", "ü"),
    ("Aacute", "Á"), ("Iacute", "Í"), ("Oacute", "Ó"), ("Uacute", "Ú"), ("Ntilde", "Ñ"), ("Uuml", "Ü"),
    ("iexcl", "¡"), ("iquest", "¿"), ("ccedil", "ç"), ("auml", "ä"), ("ouml", "ö"), ("szlig", "ß"),
];

/// Resolves `&name;`, `&#123;` and `&#x7b;`. What is not a character reference stays as it is.
fn decode_entities(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        let after = &rest[at + 1..];
        // A reference is a few characters long: the ';' is looked for no further than that
        let resolved = after.bytes().take(11).position(|byte| byte == b';').and_then(|end| {
            let name = &after[..end];
            let character = match name.strip_prefix('#') {
                Some(number) => {
                    let code = match number.strip_prefix(['x', 'X']) {
                        Some(hex) => u32::from_str_radix(hex, 16).ok(),
                        None => number.parse().ok(),
                    };
                    code.and_then(char::from_u32).map(String::from)
                }
                None => NAMED.iter().find(|(known, _)| *known == name).map(|(_, value)| value.to_string()),
            };
            character.map(|character| (character, end))
        });
        match resolved {
            Some((character, end)) => {
                out.push_str(&character);
                rest = &after[end + 1..];
            }
            None => {
                out.push('&');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// HTML reduced to the words a person reads: no tags, no scripts or styles, entities
/// resolved, spaces collapsed, no control characters, at most `max` characters.
pub fn plain_text(html: &str, max: usize) -> String {
    let mut text = String::with_capacity(html.len());
    // Lower-cased once, for finding where a script or a style ends: same length, same offsets
    let lowered = html.to_ascii_lowercase();
    let mut rest = html;
    while let Some(at) = rest.find('<') {
        text.push_str(&rest[..at]);
        let tag = &rest[at..];
        let name = &tag.as_bytes()[1..];
        // A '<' that opens nothing ("I <3 this", "for <$250") is a character like any other
        if !name.first().is_some_and(|byte| byte.is_ascii_alphabetic() || matches!(byte, b'/' | b'!' | b'?')) {
            text.push('<');
            rest = &tag[1..];
            continue;
        }
        text.push(' ');
        // What a script or a style holds is not text
        let holds_code = ["script", "style"].iter().find(|kind| name.get(..kind.len()).is_some_and(|start| start.eq_ignore_ascii_case(kind.as_bytes())));
        let after = match holds_code {
            Some(kind) => {
                let from = html.len() - tag.len();
                match lowered[from..].find(&format!("</{kind}")) {
                    Some(end) => &tag[end..],
                    // Never closed: nothing after it is text either
                    None => {
                        rest = "";
                        break;
                    }
                }
            }
            None => tag,
        };
        match after.find('>') {
            Some(end) => rest = &after[end + 1..],
            None => {
                rest = "";
                break;
            }
        }
    }
    text.push_str(rest);
    let decoded = decode_entities(&text);
    let mut words: Vec<&str> = Vec::new();
    for word in decoded.split(|c: char| c.is_whitespace() || c.is_control()) {
        if !word.is_empty() {
            words.push(word);
        }
    }
    let joined = words.join(" ");
    if joined.chars().count() <= max {
        return joined;
    }
    // Cut at a word, and say there was more
    let cut: String = joined.chars().take(max).collect();
    let cut = cut.rfind(' ').filter(|at| *at > max / 2).map(|at| &cut[..at]).unwrap_or(&cut);
    format!("{}…", cut.trim_end_matches(|c: char| c.is_ascii_punctuation() || c == ' '))
}

/// The first picture address in a piece of HTML: the `src` of its first `<img>`.
fn first_picture(html: &str) -> Option<String> {
    tags(html, "img").into_iter().filter_map(|attributes| attribute(attributes, "src")).find(|url| is_picture_url(url))
}

/// The picture a feed item comes with, wherever its feed puts it.
fn picture_of(block: &str, html: &[&str]) -> Option<String> {
    let from_tags = |name: &str, needs_image_type: bool| {
        tags(block, name).into_iter().find_map(|attributes| {
            let kind = attribute(attributes, "type").unwrap_or_default();
            let medium = attribute(attributes, "medium").unwrap_or_default();
            let is_picture = !needs_image_type || kind.starts_with("image/") || medium == "image" || (kind.is_empty() && medium.is_empty());
            attribute(attributes, "url").filter(|url| is_picture && is_picture_url(url))
        })
    };
    from_tags("media:content", true)
        .or_else(|| from_tags("media:thumbnail", false))
        .or_else(|| {
            tags(block, "enclosure").into_iter().find_map(|attributes| {
                let kind = attribute(attributes, "type").unwrap_or_default();
                attribute(attributes, "url").filter(|url| kind.starts_with("image/") && is_picture_url(url))
            })
        })
        .or_else(|| html.iter().find_map(|piece| first_picture(piece)))
}

/// Days from 1970-01-01 to a civil date (Howard Hinnant's algorithm).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn to_epoch_ms(year: i64, month: i64, day: i64, hour: i64, minute: i64, second: i64, offset_minutes: i64) -> Option<u64> {
    if !(1970..=2200).contains(&year)
        || !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || !(0..=23).contains(&hour)
        || !(0..=59).contains(&minute)
        || !(0..=60).contains(&second)
    {
        return None;
    }
    let seconds = days_from_civil(year, month, day) * 86_400 + hour * 3600 + minute * 60 + second - offset_minutes * 60;
    u64::try_from(seconds).ok().map(|s| s * 1000)
}

/// `+0100`, `+01:00`, `Z`, `GMT`, `UT` and the North American names RFC 822 allows: minutes
/// east of UTC. Any other name counts as UTC.
fn offset_minutes(zone: &str) -> i64 {
    let zone = zone.trim();
    let sign = match zone.chars().next() {
        Some('+') => 1,
        Some('-') => -1,
        _ => {
            return match zone.to_ascii_uppercase().as_str() {
                "EDT" => -240,
                "EST" | "CDT" => -300,
                "CST" | "MDT" => -360,
                "MST" | "PDT" => -420,
                "PST" => -480,
                _ => 0,
            }
        }
    };
    let digits: String = zone[1..].chars().filter(char::is_ascii_digit).collect();
    if digits.len() != 4 {
        return 0;
    }
    let hours: i64 = digits[..2].parse().unwrap_or(0);
    let minutes: i64 = digits[2..].parse().unwrap_or(0);
    sign * (hours * 60 + minutes)
}

/// A date as feeds write it: RFC 2822 (`Mon, 16 Nov 2026 21:11:04 +0100`) in RSS, RFC 3339
/// (`2026-11-16T21:11:04+01:00`) in Atom.
pub fn parse_feed_date(raw: &str) -> Option<u64> {
    let raw = raw.trim();
    // RFC 3339: starts with the date, year first ("GMT" has a T too, but no such start)
    let year_first = raw.as_bytes().get(4) == Some(&b'-') && raw.as_bytes().get(7) == Some(&b'-');
    if let Some((date, time)) = raw.split_once(['T', 't']).filter(|_| year_first) {
        let mut date_parts = date.split('-').map(|part| part.parse::<i64>().ok());
        let (year, month, day) = (date_parts.next()??, date_parts.next()??, date_parts.next()??);
        let zone_at = time.find(['Z', 'z', '+', '-']).unwrap_or(time.len());
        let (clock, zone) = time.split_at(zone_at);
        let mut clock_parts = clock.split(':').map(|part| part.split('.').next().and_then(|whole| whole.parse::<i64>().ok()));
        let (hour, minute, second) = (clock_parts.next()??, clock_parts.next()??, clock_parts.next().flatten().unwrap_or(0));
        return to_epoch_ms(year, month, day, hour, minute, second, offset_minutes(zone));
    }
    // RFC 2822, with or without the day of the week
    const MONTHS: [&str; 12] = ["jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec"];
    let after_weekday = raw.split_once(',').map(|(_, rest)| rest).unwrap_or(raw);
    let mut parts = after_weekday.split_whitespace();
    let day: i64 = parts.next()?.parse().ok()?;
    let month_name = parts.next()?.to_ascii_lowercase();
    let month = MONTHS.iter().position(|name| month_name.starts_with(name))? as i64 + 1;
    let year: i64 = parts.next()?.parse().ok()?;
    let year = if year < 100 { year + 2000 } else { year };
    let mut clock = parts.next()?.split(':').map(|part| part.parse::<i64>().ok());
    let (hour, minute, second) = (clock.next()??, clock.next()??, clock.next().flatten().unwrap_or(0));
    to_epoch_ms(year, month, day, hour, minute, second, offset_minutes(parts.next().unwrap_or("")))
}

/// The news of a feed, RSS or Atom: what can be shown safely of each item, newest first.
/// Items without a title or without a link to the source's own site are left out.
pub fn parse_feed(xml: &str, source: &Feed) -> Vec<NewsItem> {
    let rss = blocks(xml, "item");
    let is_atom = rss.is_empty();
    let entries = if is_atom { blocks(xml, "entry") } else { rss };
    let mut seen = HashSet::new();
    let mut items: Vec<NewsItem> = entries
        .into_iter()
        .filter_map(|block| {
            let title = plain_text(&content(block, "title")?, MAX_TITLE_CHARS);
            let url = if is_atom {
                // The article is the "alternate" link, or the one that says nothing of itself
                tags(block, "link")
                    .into_iter()
                    .filter(|attributes| matches!(attribute(attributes, "rel").as_deref(), None | Some("alternate")))
                    .find_map(|attributes| attribute(attributes, "href"))
            } else {
                content(block, "link")
            }?
            .trim()
            .to_string();
            if title.is_empty() || !is_on_host(&url, &source.hosts) || !seen.insert(url.clone()) {
                return None;
            }
            let short = ["description", "summary"].iter().find_map(|name| content(block, name));
            let long = ["content:encoded", "content"].iter().find_map(|name| content(block, name));
            let summary = plain_text(short.as_deref().or(long.as_deref()).unwrap_or_default(), MAX_SUMMARY_CHARS);
            let html: Vec<&str> = [short.as_deref(), long.as_deref()].into_iter().flatten().collect();
            let published_at = ["pubDate", "published", "updated", "dc:date"]
                .iter()
                .find_map(|name| content(block, name))
                .and_then(|raw| parse_feed_date(&raw));
            Some(NewsItem {
                id: format!("{}:{url}", source.id),
                source: source.id.clone(),
                source_name: source.name.clone(),
                topic: source.topic,
                title,
                summary,
                url,
                image_url: picture_of(block, &html),
                published_at,
            })
        })
        .collect();
    items.sort_by_key(|item| std::cmp::Reverse(item.published_at));
    items.truncate(MAX_ITEMS_PER_FEED);
    items
}

/// The text of a feed's bytes. Most feeds are UTF-8; an older site may still write in a
/// national encoding (windows-1251, ISO-8859-1…), named by the server or in the first line
/// of the file (`<?xml … encoding="…"?>`).
pub fn decode_feed(bytes: &[u8], declared: Option<&str>) -> String {
    let in_file = || {
        let head = String::from_utf8_lossy(&bytes[..bytes.len().min(200)]).to_ascii_lowercase();
        let after = head.split("encoding=").nth(1)?;
        let quote = after.chars().next().filter(|c| *c == '"' || *c == '\'')?;
        after[1..].split(quote).next().map(str::to_string)
    };
    let label = declared
        // "windows-1251"; other=parameter -> windows-1251
        .map(|label| label.split(';').next().unwrap_or_default().trim_matches(|c: char| c == '"' || c == '\'' || c.is_whitespace()).to_string())
        .filter(|label| !label.is_empty())
        .or_else(in_file);
    match label.and_then(|label| encoding_rs::Encoding::for_label(label.as_bytes())) {
        Some(encoding) if encoding != encoding_rs::UTF_8 => encoding.decode(bytes).0.into_owned(),
        _ => String::from_utf8_lossy(bytes).into_owned(),
    }
}

/// Reads the feeds of the publishers. Each request asks one site for its public feed and
/// says nothing of the user.
pub struct NewsClient {
    agent: ureq::Agent,
}

impl Default for NewsClient {
    fn default() -> Self {
        Self::new()
    }
}

impl NewsClient {
    pub fn new() -> Self {
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_secs(8))
            // The whole answer, not each piece of it: a site that sends a few bytes now and
            // then would otherwise keep the page waiting, and with it every other source
            .timeout(Duration::from_secs(25))
            .user_agent("KansoLauncher/0.1.0 (feed reader)")
            .build();
        Self { agent }
    }

    /// The text of a feed, read up to a bound.
    pub fn fetch_text(&self, url: &str) -> Result<String, String> {
        let response = self
            .agent
            .get(url)
            .set("Accept", "application/rss+xml, application/atom+xml, application/xml, text/xml")
            .call()
            .map_err(|e| e.to_string())?;
        // What the server says the text is written in, if it says
        let declared = response.header("Content-Type").and_then(|kind| kind.to_ascii_lowercase().split("charset=").nth(1).map(str::to_string));
        let mut bytes = Vec::new();
        response.into_reader().take(MAX_FEED_BYTES).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
        Ok(decode_feed(&bytes, declared.as_deref()))
    }

    /// The news of the feeds named, newest first; a feed that fails is reported without
    /// hiding the rest. Feeds are asked at the same time.
    pub fn fetch(&self, feeds: &[Feed]) -> NewsReport {
        let answers: Vec<(&Feed, Result<Vec<NewsItem>, String>)> = std::thread::scope(|scope| {
            let workers: Vec<_> = feeds
                .iter()
                .map(|feed| scope.spawn(move || (feed, self.fetch_text(&feed.feed_url).map(|xml| parse_feed(&xml, feed)))))
                .collect();
            workers.into_iter().filter_map(|worker| worker.join().ok()).collect()
        });
        let mut report = NewsReport { fetched_at: now_timestamp_ms(), ..Default::default() };
        for (feed, answer) in answers {
            match answer {
                // A feed that answers with no news at all did not really answer
                Ok(items) if !items.is_empty() => report.items.extend(items),
                _ => report.failed_sources.push(feed.name.clone()),
            }
        }
        report.items.sort_by_key(|item| std::cmp::Reverse(item.published_at));
        report
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source() -> Feed {
        Feed {
            id: "example".into(),
            name: "Example Games".into(),
            topic: NewsTopic::Gaming,
            feed_url: "https://www.example.com/feed".into(),
            hosts: vec!["example.com".into()],
        }
    }

    #[test]
    fn reads_an_rss_feed() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0" xmlns:media="http://search.yahoo.com/mrss/" xmlns:content="http://purl.org/rss/1.0/modules/content/">
<channel><title>Example Games</title><link>https://www.example.com</link>
<image><url>https://www.example.com/logo.png</url><title>Example</title><link>https://www.example.com</link></image>
<item>
  <title><![CDATA[New GPU beats <em>everything</em> &amp; more]]></title>
  <link>https://www.example.com/news/new-gpu?utm=rss</link>
  <pubDate>Mon, 16 Nov 2026 21:11:04 +0100</pubDate>
  <description><![CDATA[<p>The <b>card</b> is&nbsp;fast.<script>alert(1)</script> Really   fast.</p><img src="https://cdn.example.com/a.jpg">]]></description>
  <media:content url="https://cdn.example.net/gpu.jpg" medium="image" />
</item>
<item>
  <title>Older &amp; plainer</title>
  <link>https://example.com/news/older</link>
  <pubDate>Sun, 15 Nov 2026 08:00:00 GMT</pubDate>
  <description>&lt;p&gt;Escaped &amp;amp; fine&lt;/p&gt;</description>
  <enclosure url="https://cdn.example.com/b.png" type="image/png" length="1"/>
</item>
<item><title>Elsewhere</title><link>https://evil.example.org/x</link></item>
<item><title>Not https</title><link>http://www.example.com/x</link></item>
<item><title>Look-alike host</title><link>https://www.example.com.evil.org/x</link></item>
<item><title></title><link>https://www.example.com/untitled</link></item>
<item><title>Again</title><link>https://www.example.com/news/new-gpu?utm=rss</link></item>
</channel></rss>"#;
        let items = parse_feed(xml, &source());
        assert_eq!(items.len(), 2, "only items with a title and a link to the source's own site, each once");
        let first = &items[0];
        assert_eq!(first.title, "New GPU beats everything & more");
        assert_eq!(first.url, "https://www.example.com/news/new-gpu?utm=rss");
        assert_eq!(first.summary, "The card is fast. Really fast.", "tags, scripts and extra spaces are gone");
        assert_eq!(first.image_url.as_deref(), Some("https://cdn.example.net/gpu.jpg"));
        assert_eq!(first.published_at, Some(1_794_859_864_000), "21:11:04 +0100 is 20:11:04 UTC");
        assert_eq!((first.source.as_str(), first.source_name.as_str(), first.topic), ("example", "Example Games", NewsTopic::Gaming));
        assert_eq!(first.id, "example:https://www.example.com/news/new-gpu?utm=rss");
        let second = &items[1];
        assert_eq!(second.title, "Older & plainer");
        assert_eq!(second.summary, "Escaped & fine");
        assert_eq!(second.image_url.as_deref(), Some("https://cdn.example.com/b.png"));
        assert!(second.published_at < first.published_at, "newest first");
    }

    #[test]
    fn reads_an_atom_feed() {
        let xml = r#"<feed xmlns="http://www.w3.org/2005/Atom"><title>Example</title>
<link href="https://www.example.com/"/>
<entry>
  <title type="html">Patch &lt;b&gt;1.2&lt;/b&gt; is out</title>
  <link rel="replies" href="https://www.example.com/comments/1"/>
  <link rel="alternate" type="text/html" href="https://www.example.com/patch-1-2"/>
  <updated>2026-11-16T20:11:04Z</updated>
  <summary type="html">&lt;img src="https://www.example.com/p.webp"/&gt; Fixes &amp;amp; changes.</summary>
</entry>
<entry>
  <title>No link of its own kind</title>
  <link href="https://www.example.com/plain"/>
  <published>2026-11-17T01:00:00+02:00</published>
  <content type="html"><![CDATA[<p>Body only.</p>]]></content>
</entry>
</feed>"#;
        let items = parse_feed(xml, &source());
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].url, "https://www.example.com/plain", "23:00 UTC is the newer one");
        assert_eq!(items[0].summary, "Body only.");
        assert_eq!(items[1].title, "Patch 1.2 is out");
        assert_eq!(items[1].url, "https://www.example.com/patch-1-2", "the article, not its comments");
        assert_eq!(items[1].summary, "Fixes & changes.");
        assert_eq!(items[1].image_url.as_deref(), Some("https://www.example.com/p.webp"));
    }

    #[test]
    fn what_is_not_a_feed_gives_no_news() {
        assert!(parse_feed("", &source()).is_empty());
        assert!(parse_feed("<html><body>Blocked</body></html>", &source()).is_empty());
        assert!(parse_feed("<rss><channel><item><title>Cut off", &source()).is_empty());
        assert!(parse_feed("<items><itemized>no</itemized></items>", &source()).is_empty());
    }

    #[test]
    fn text_from_a_feed_is_plain_and_bounded() {
        assert_eq!(plain_text("<p>Hello<br/>world</p>", 50), "Hello world");
        assert_eq!(plain_text("a\u{7}b \n\t c &#8217; &#x41; &hellip; &unknown; & co", 50), "a b c ’ A … &unknown; & co");
        assert_eq!(plain_text("<style>p{color:red}</style>Text<script type=\"x\">evil()</script> end", 50), "Text end");
        assert_eq!(plain_text("<a href=\"javascript:x\" onclick=\"y\">Link</a> <unclosed", 50), "Link");
        let long = plain_text(&"word ".repeat(100), 40);
        assert!(long.chars().count() <= 41 && long.ends_with('…') && !long.contains("  "), "{long}");
    }

    #[test]
    fn hostile_text_costs_no_more_than_its_length() {
        // Each of these once took time growing with the square of the length
        let started = std::time::Instant::now();
        assert_eq!(plain_text(&"<script>".repeat(200_000), 50), "");
        assert!(plain_text(&"<я>я".repeat(200_000), 5).starts_with("<я>я"), "not a tag: kept as written");
        assert_eq!(plain_text(&"<p>я".repeat(200_000), 5), "я я…");
        assert!(plain_text(&"&".repeat(1_000_000), 20).chars().count() <= 21);
        assert_eq!(decode_entities(&"&".repeat(1_000_000)).len(), 1_000_000);
        assert!(started.elapsed() < std::time::Duration::from_secs(5), "{:?}", started.elapsed());
    }

    #[test]
    fn a_less_than_sign_that_opens_nothing_is_kept() {
        assert_eq!(plain_text("I <3 this game", 50), "I <3 this game");
        assert_eq!(plain_text("Arc B580 for <$250 is a steal", 50), "Arc B580 for <$250 is a steal");
        assert_eq!(plain_text("5 > 3 & 2 < 4", 50), "5 > 3 & 2 < 4");
        assert_eq!(plain_text("a <b>bold</b> move<br/>here", 50), "a bold move here");
        assert_eq!(plain_text("ends with <", 50), "ends with <");
        assert_eq!(plain_text("<!-- note -->kept<?pi?> too", 50), "kept too");
    }

    #[test]
    fn feeds_in_a_national_encoding_are_read() {
        // "Новость дня" in windows-1251, as an older Russian site would send it
        let cyrillic = [0xCD, 0xEE, 0xE2, 0xEE, 0xF1, 0xF2, 0xFC, 0x20, 0xE4, 0xED, 0xFF];
        let mut feed = b"<?xml version=\"1.0\" encoding=\"windows-1251\"?><rss><title>".to_vec();
        feed.extend_from_slice(&cyrillic);
        assert!(decode_feed(&feed, None).ends_with("<title>Новость дня"), "named in the file");
        assert_eq!(decode_feed(&cyrillic, Some("windows-1251")), "Новость дня", "named by the server");
        assert_eq!(decode_feed(&cyrillic, Some("\"Windows-1251\"; boundary=x")), "Новость дня");
        // "Análisis" in ISO-8859-1
        assert_eq!(decode_feed(&[0x41, 0x6E, 0xE1, 0x6C, 0x69, 0x73, 0x69, 0x73], Some("iso-8859-1")), "Análisis");
        // UTF-8 by default, by name, and when the name means nothing
        assert_eq!(decode_feed("Новость".as_bytes(), None), "Новость");
        assert_eq!(decode_feed("Новость".as_bytes(), Some("utf-8")), "Новость");
        assert_eq!(decode_feed("Новость".as_bytes(), Some("no-such-encoding")), "Новость");
        assert_eq!(decode_feed(b"", None), "");
    }

    #[test]
    fn reads_the_dates_feeds_use() {
        assert_eq!(parse_feed_date("Mon, 16 Nov 2026 21:11:04 +0000"), Some(1_794_863_464_000));
        assert_eq!(parse_feed_date("16 Nov 2026 21:11:04 GMT"), Some(1_794_863_464_000));
        assert_eq!(parse_feed_date("Mon, 16 Nov 2026 16:11:04 -0500"), Some(1_794_863_464_000));
        assert_eq!(parse_feed_date("2026-11-16T21:11:04Z"), Some(1_794_863_464_000));
        assert_eq!(parse_feed_date("2026-11-16T22:11:04.123+01:00"), Some(1_794_863_464_000));
        assert_eq!(parse_feed_date("2026-11-16T21:11Z"), Some(1_794_863_460_000));
        assert_eq!(parse_feed_date("Mon, 16 Nov 2026 16:11:04 EST"), Some(1_794_863_464_000), "the zone names RFC 822 allows");
        assert_eq!(parse_feed_date("Mon, 16 Nov 2026 13:11:04 pst"), Some(1_794_863_464_000));
        assert_eq!(parse_feed_date("1 Jan 2026 -99999999999999999:00 GMT"), None, "no clock runs backwards");
        assert_eq!(parse_feed_date("yesterday"), None);
        assert_eq!(parse_feed_date("32 Nov 2026 21:11:04 GMT"), None);
        assert_eq!(parse_feed_date(""), None);
    }

    #[test]
    fn only_articles_of_known_sources_may_be_opened() {
        for source in NEWS_SOURCES {
            assert!(source.feed_url.starts_with("https://"), "{}", source.id);
            assert!(source.site_url.starts_with("https://"), "{}", source.id);
            assert!(!source.hosts.is_empty() && source.hosts.iter().all(|host| !host.contains('/') && host.contains('.')), "{}", source.id);
            assert!(source.id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'), "{}", source.id);
            assert!(is_allowed_news_url(&format!("https://{}/article", source.hosts[0]), &[]), "{}", source.id);
            assert!(is_on_host(source.site_url, source.hosts), "{}: the site is on its own hosts", source.id);
        }
        let ids: HashSet<&str> = NEWS_SOURCES.iter().map(|source| source.id).collect();
        assert_eq!(ids.len(), NEWS_SOURCES.len(), "ids are unique");
        assert!(!is_allowed_news_url("https://evil.example.org/article", &[]));
        assert!(!is_allowed_news_url("javascript:alert(1)", &[]));
        assert!(!is_on_host("https://user@example.com/x", &["example.com"]));
        assert!(!is_on_host("https://example.com:8443/x", &["example.com"]));
        assert!(!is_on_host("https://notexample.com/x", &["example.com"]));
        assert!(!is_on_host("https://example.com/a b", &["example.com"]));
        assert!(is_on_host("https://news.example.com/x?y=1#z", &["example.com"]));
    }

    #[test]
    fn a_feed_added_by_hand_is_read_like_the_others() {
        let added = CustomFeed { name: "  My\u{7} site  ".into(), url: " https://feeds.example.org/news.xml ".into(), topic: NewsTopic::Hardware };
        let kept = added.sanitized().expect("a plain https address is kept");
        assert_eq!((kept.name.as_str(), kept.url.as_str()), ("My site", "https://feeds.example.org/news.xml"));
        assert_eq!(kept.site_host().as_deref(), Some("example.org"), "the site, not the label its feeds live under");
        let feed = Feed::custom(3, &kept).unwrap();
        assert_eq!((feed.id.as_str(), feed.hosts.as_slice(), feed.topic), ("custom-3", &["example.org".to_string()][..], NewsTopic::Hardware));
        let items = parse_feed(
            "<rss><channel><item><title>On the site</title><link>https://www.example.org/a</link></item>\
             <item><title>Elsewhere</title><link>https://other.example.net/b</link></item></channel></rss>",
            &feed,
        );
        assert_eq!(items.len(), 1, "articles are kept only on the feed's own site");
        assert_eq!(items[0].id, "custom-3:https://www.example.org/a");
        // Its articles may be opened; nobody else's
        assert!(is_allowed_news_url("https://www.example.org/a", std::slice::from_ref(&kept)));
        assert!(!is_allowed_news_url("https://www.example.org/a", &[]));
        assert!(!is_allowed_news_url("https://example.org.evil.net/a", std::slice::from_ref(&kept)));
        // A feed with no name is called by its site; what is not an https address is not kept
        assert_eq!(CustomFeed { name: String::new(), url: "https://www.site.example/feed".into(), topic: NewsTopic::Gaming }.sanitized().unwrap().name, "site.example");
        for bad in ["http://example.org/feed", "https://", "https://localhost/feed", "https://user@example.org/feed", "https://example.org:8080/feed", "file:///etc/passwd", "https://example.org/a b"] {
            assert!(CustomFeed { name: "x".into(), url: bad.into(), topic: NewsTopic::Gaming }.sanitized().is_none(), "{bad}");
        }
    }

    #[test]
    fn every_language_has_sources_to_start_from() {
        for lang in [Lang::It, Lang::En, Lang::Es, Lang::Ru] {
            let sources = default_sources(lang);
            assert!(!sources.is_empty(), "{lang:?}");
        }
        assert!(source_by_id("no-such-source").is_none());
    }
}
