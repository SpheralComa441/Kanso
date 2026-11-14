//! Community themes: small JSON files that restyle the whole interface.
//!
//! A theme can only set a fixed list of colors, a few layout sizes and a font stack, and every
//! value is validated here before it reaches the UI. A theme file therefore cannot carry
//! arbitrary CSS, scripts or remote resources: importing one from a stranger is safe.

use crate::i18n::T;
use crate::scanner::exe_pick::slugify;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// Value of the `kansoTheme` marker written into exported files.
pub const THEME_FORMAT_VERSION: u32 = 1;
/// Extension-less marker in file names is not required; themes are plain `.json` files.
const MAX_THEME_FILE_BYTES: u64 = 256 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ColorScheme {
    #[default]
    Dark,
    Light,
}

/// The twelve colors of a theme, as `#rrggbb` (or `#rrggbbaa`) strings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemeColors {
    /// Window background.
    pub background: String,
    /// Cards, panels, dialogs.
    pub surface: String,
    /// Hovered rows, selected items, raised controls.
    pub elevated: String,
    /// Hairlines between surfaces.
    pub border: String,
    /// Borders of hovered or focused controls.
    pub border_strong: String,
    /// Titles and primary text.
    pub text: String,
    /// Body text.
    pub text_secondary: String,
    /// Labels and secondary information.
    pub text_muted: String,
    /// Hints, counters, placeholders.
    pub text_faint: String,
    /// Primary buttons, highlights, active markers.
    pub accent: String,
    pub accent_hover: String,
    /// Text and icons drawn on top of the accent color.
    pub on_accent: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemeLayout {
    /// Minimum width of a cover card in the grid, in pixels.
    pub card_min_width: u32,
    /// Corner radius of cards, in pixels.
    pub card_radius: u32,
    /// Width of the filter sidebar, in pixels.
    pub sidebar_width: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Theme {
    /// File name without extension inside the themes folder; assigned by Kanso, never read
    /// from a theme file.
    #[serde(default)]
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub description: String,
    pub color_scheme: ColorScheme,
    pub colors: ThemeColors,
    pub layout: ThemeLayout,
    /// CSS font stack of installed fonts; `None` keeps Kanso's default.
    pub font_family: Option<String>,
}

impl ThemeColors {
    /// Kanso's own palette for the given scheme; the base every theme starts from.
    pub fn defaults(scheme: ColorScheme) -> Self {
        let colors = match scheme {
            ColorScheme::Dark => [
                "#0f0f11", "#16161a", "#1e1e24", "#272730", "#52525b", "#f4f4f5", "#d4d4d8",
                "#a1a1aa", "#71717a", "#e5a93c", "#f0bc59", "#09090b",
            ],
            ColorScheme::Light => [
                "#f4f4f5", "#ffffff", "#e9e9ec", "#d9d9de", "#a1a1aa", "#18181b", "#3f3f46",
                "#5b5b66", "#7c7c87", "#b7791f", "#9c6615", "#ffffff",
            ],
        };
        let [background, surface, elevated, border, border_strong, text, text_secondary, text_muted, text_faint, accent, accent_hover, on_accent] =
            colors.map(str::to_string);
        Self {
            background,
            surface,
            elevated,
            border,
            border_strong,
            text,
            text_secondary,
            text_muted,
            text_faint,
            accent,
            accent_hover,
            on_accent,
        }
    }
}

impl Default for ThemeLayout {
    fn default() -> Self {
        Self {
            card_min_width: 185,
            card_radius: 10,
            sidebar_width: 208,
        }
    }
}

/// A theme file as an author may write it: everything but the name is optional.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct RawTheme {
    kanso_theme: Option<u32>,
    name: String,
    author: String,
    description: String,
    color_scheme: Option<String>,
    colors: Option<RawColors>,
    layout: Option<RawLayout>,
    font_family: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct RawColors {
    background: Option<String>,
    surface: Option<String>,
    elevated: Option<String>,
    border: Option<String>,
    border_strong: Option<String>,
    text: Option<String>,
    text_secondary: Option<String>,
    text_muted: Option<String>,
    text_faint: Option<String>,
    accent: Option<String>,
    accent_hover: Option<String>,
    on_accent: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct RawLayout {
    card_min_width: Option<f64>,
    card_radius: Option<f64>,
    sidebar_width: Option<f64>,
}

fn invalid(detail: impl std::fmt::Display) -> String {
    format!("{}: {detail}", T::ThemeInvalid.s())
}

/// Normalises a `#rgb`, `#rrggbb` or `#rrggbbaa` color to lower-case long form.
pub fn normalize_hex_color(raw: &str) -> Option<String> {
    let hex = raw.trim().strip_prefix('#')?;
    if !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let hex = hex.to_ascii_lowercase();
    match hex.len() {
        3 => Some(format!(
            "#{}",
            hex.chars().flat_map(|c| [c, c]).collect::<String>()
        )),
        6 | 8 => Some(format!("#{hex}")),
        _ => None,
    }
}

fn clean_text(raw: &str, max_chars: usize) -> String {
    raw.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .filter(|c| !c.is_control())
        .take(max_chars)
        .collect()
}

impl Theme {
    /// Parses and validates a theme file. Missing colors and sizes fall back to Kanso's
    /// defaults for the theme's color scheme; anything present but wrong is an error that
    /// names the offending field, so authors know what to fix.
    pub fn from_json(raw: &str) -> Result<Self, String> {
        let raw = raw.trim_start_matches('\u{feff}');
        let parsed: RawTheme = serde_json::from_str(raw).map_err(invalid)?;

        if parsed.kanso_theme.is_some_and(|v| v > THEME_FORMAT_VERSION) {
            return Err(invalid(format!(
                "kansoTheme {} (max {THEME_FORMAT_VERSION})",
                parsed.kanso_theme.unwrap_or_default()
            )));
        }

        let name = clean_text(&parsed.name, 60);
        if name.is_empty() {
            return Err(invalid("name"));
        }
        let Some(raw_colors) = parsed.colors else {
            return Err(invalid("colors"));
        };

        let color_scheme = match parsed.color_scheme.as_deref().map(str::trim) {
            None | Some("") | Some("dark") => ColorScheme::Dark,
            Some("light") => ColorScheme::Light,
            Some(other) => return Err(invalid(format!("colorScheme \"{other}\""))),
        };

        let base = ThemeColors::defaults(color_scheme);
        let color = |field: &str, value: Option<String>, fallback: String| match value {
            None => Ok(fallback),
            Some(text) => normalize_hex_color(&text)
                .ok_or_else(|| invalid(format!("colors.{field} \"{}\"", clean_text(&text, 40)))),
        };
        let colors = ThemeColors {
            background: color("background", raw_colors.background, base.background)?,
            surface: color("surface", raw_colors.surface, base.surface)?,
            elevated: color("elevated", raw_colors.elevated, base.elevated)?,
            border: color("border", raw_colors.border, base.border)?,
            border_strong: color("borderStrong", raw_colors.border_strong, base.border_strong)?,
            text: color("text", raw_colors.text, base.text)?,
            text_secondary: color("textSecondary", raw_colors.text_secondary, base.text_secondary)?,
            text_muted: color("textMuted", raw_colors.text_muted, base.text_muted)?,
            text_faint: color("textFaint", raw_colors.text_faint, base.text_faint)?,
            accent: color("accent", raw_colors.accent, base.accent)?,
            accent_hover: color("accentHover", raw_colors.accent_hover, base.accent_hover)?,
            on_accent: color("onAccent", raw_colors.on_accent, base.on_accent)?,
        };

        let raw_layout = parsed.layout.unwrap_or_default();
        let defaults = ThemeLayout::default();
        // Out-of-range sizes are clamped rather than rejected: the theme still looks as intended
        let size = |value: Option<f64>, fallback: u32, min: u32, max: u32| {
            value
                .filter(|v| v.is_finite())
                .map_or(fallback, |v| (v.round() as i64).clamp(i64::from(min), i64::from(max)) as u32)
        };
        let layout = ThemeLayout {
            card_min_width: size(raw_layout.card_min_width, defaults.card_min_width, 120, 320),
            card_radius: size(raw_layout.card_radius, defaults.card_radius, 0, 28),
            sidebar_width: size(raw_layout.sidebar_width, defaults.sidebar_width, 160, 320),
        };

        let font_family = match parsed.font_family.as_deref().map(str::trim) {
            None | Some("") => None,
            Some(stack) => {
                let allowed = stack.len() <= 200
                    && stack
                        .chars()
                        .all(|c| c.is_alphanumeric() || matches!(c, ' ' | ',' | '-' | '_' | '\'' | '"'));
                if !allowed {
                    return Err(invalid("fontFamily"));
                }
                Some(stack.to_string())
            }
        };

        Ok(Self {
            id: String::new(),
            name,
            author: clean_text(&parsed.author, 60),
            description: clean_text(&parsed.description, 240),
            color_scheme,
            colors,
            layout,
            font_family,
        })
    }

    /// The theme as a shareable file: pretty-printed, with the format marker, without the id.
    pub fn to_json(&self) -> String {
        let value = serde_json::json!({
            "kansoTheme": THEME_FORMAT_VERSION,
            "name": self.name,
            "author": self.author,
            "description": self.description,
            "colorScheme": self.color_scheme,
            "colors": self.colors,
            "layout": self.layout,
            "fontFamily": self.font_family,
        });
        let mut text = serde_json::to_string_pretty(&value).unwrap_or_default();
        text.push('\n');
        text
    }

    /// Re-validates a theme that came from the UI (editor, built-in list) exactly like a file.
    pub fn validated(&self) -> Result<Self, String> {
        let mut theme = Self::from_json(&self.to_json())?;
        theme.id = self.id.clone();
        Ok(theme)
    }
}

fn is_safe_theme_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 80
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

fn theme_path(themes_dir: &Path, id: &str) -> PathBuf {
    themes_dir.join(format!("{id}.json"))
}

/// Themes installed in `themes_dir`, sorted by name, plus a warning for each unreadable file.
pub fn list_user_themes(themes_dir: &Path) -> (Vec<Theme>, Vec<String>) {
    let mut themes = Vec::new();
    let mut warnings = Vec::new();
    let Ok(entries) = fs::read_dir(themes_dir) else {
        return (themes, warnings);
    };

    for path in entries.flatten().map(|entry| entry.path()) {
        let is_json = path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("json"));
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        if !is_json || !path.is_file() {
            continue;
        }
        // A file dropped in by hand may have any name: derive a safe id from it
        let id = slugify(stem);
        if id.is_empty() {
            continue;
        }
        match read_theme_file(&path) {
            Ok(mut theme) => {
                theme.id = id;
                themes.push(theme);
            }
            Err(err) => warnings.push(format!("{} — {err}", path.display())),
        }
    }

    themes.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()).then_with(|| a.id.cmp(&b.id)));
    themes.dedup_by(|later, earlier| later.id == earlier.id);
    (themes, warnings)
}

fn read_theme_file(path: &Path) -> Result<Theme, String> {
    let size = fs::metadata(path)
        .map_err(|e| format!("{} {}: {e}", T::CannotRead.s(), path.display()))?
        .len();
    if size > MAX_THEME_FILE_BYTES {
        return Err(invalid(format!("{size} bytes")));
    }
    let raw = fs::read_to_string(path)
        .map_err(|e| format!("{} {}: {e}", T::CannotRead.s(), path.display()))?;
    Theme::from_json(&raw)
}

/// Stores `theme` in the themes folder. With `replace_id` the existing theme of that id is
/// overwritten (editing); otherwise a new id is derived from the name, never clobbering
/// another theme.
pub fn save_user_theme(
    themes_dir: &Path,
    theme: &Theme,
    replace_id: Option<&str>,
) -> Result<Theme, String> {
    let mut theme = theme.validated()?;
    fs::create_dir_all(themes_dir)
        .map_err(|e| format!("{} {}: {e}", T::CannotCreateFolder.s(), themes_dir.display()))?;

    theme.id = match replace_id.filter(|id| is_safe_theme_id(id)) {
        Some(id) => id.to_string(),
        None => {
            let base = match slugify(&theme.name) {
                slug if slug.is_empty() => "theme".to_string(),
                slug => slug,
            };
            let mut candidate = base.clone();
            let mut suffix = 2;
            while theme_path(themes_dir, &candidate).exists() {
                candidate = format!("{base}-{suffix}");
                suffix += 1;
            }
            candidate
        }
    };

    let dest = theme_path(themes_dir, &theme.id);
    fs::write(&dest, theme.to_json())
        .map_err(|e| format!("{} {}: {e}", T::CannotSave.s(), dest.display()))?;
    Ok(theme)
}

/// Validates the theme file at `source` and installs a copy in the themes folder.
pub fn import_theme_file(themes_dir: &Path, source: &Path) -> Result<Theme, String> {
    let theme = read_theme_file(source)?;
    save_user_theme(themes_dir, &theme, None)
}

/// Writes `theme` to `dest`, to be shared.
pub fn export_theme_file(theme: &Theme, dest: &Path) -> Result<(), String> {
    let theme = theme.validated()?;
    fs::write(dest, theme.to_json())
        .map_err(|e| format!("{} {}: {e}", T::CannotSave.s(), dest.display()))
}

pub fn delete_user_theme(themes_dir: &Path, id: &str) -> Result<(), String> {
    if !is_safe_theme_id(id) {
        return Err(format!("{}: {id}", T::ThemeNotFound.s()));
    }
    let path = theme_path(themes_dir, id);
    if !path.is_file() {
        return Err(format!("{}: {id}", T::ThemeNotFound.s()));
    }
    fs::remove_file(&path).map_err(|e| format!("{}: {e}", T::OperationFailed.s()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    const NORD: &str = r##"{
        "kansoTheme": 1,
        "name": "  Nord   Night ",
        "author": "Arctic Ice Studio",
        "colorScheme": "dark",
        "colors": { "background": "#2E3440", "surface": "#3b4252", "accent": "#8CD", "onAccent": "#2e3440ff" },
        "layout": { "cardMinWidth": 9999, "cardRadius": 14.4 },
        "fontFamily": "Segoe UI, 'Fira Sans', sans-serif"
    }"##;

    #[test]
    fn parses_a_partial_theme_filling_defaults_and_normalising_values() {
        let theme = Theme::from_json(NORD).expect("valid theme");
        assert_eq!(theme.name, "Nord Night");
        assert_eq!(theme.author, "Arctic Ice Studio");
        assert_eq!(theme.color_scheme, ColorScheme::Dark);
        assert_eq!(theme.colors.background, "#2e3440");
        assert_eq!(theme.colors.accent, "#88ccdd", "short hex is expanded");
        assert_eq!(theme.colors.on_accent, "#2e3440ff");
        // Not specified: Kanso's dark defaults
        assert_eq!(theme.colors.text, ThemeColors::defaults(ColorScheme::Dark).text);
        assert_eq!(theme.layout.card_min_width, 320, "clamped");
        assert_eq!(theme.layout.card_radius, 14);
        assert_eq!(theme.layout.sidebar_width, 208);
        assert_eq!(theme.font_family.as_deref(), Some("Segoe UI, 'Fira Sans', sans-serif"));

        let light = Theme::from_json(r#"{"name":"Carta","colorScheme":"light","colors":{}}"#).unwrap();
        assert_eq!(light.colors, ThemeColors::defaults(ColorScheme::Light));
    }

    #[test]
    fn rejects_anything_that_is_not_a_plain_validated_value() {
        let bad = [
            (r#"{"colors":{}}"#, "name"),
            (r#"{"name":"X"}"#, "colors"),
            (r#"{"name":"X","colors":{"accent":"red"}}"#, "colors.accent"),
            (r#"{"name":"X","colors":{"background":"url(https://evil.example/x.png)"}}"#, "colors.background"),
            (r##"{"name":"X","colors":{"text":"#12345"}}"##, "colors.text"),
            (r#"{"name":"X","colorScheme":"sepia","colors":{}}"#, "colorScheme"),
            (r#"{"name":"X","colors":{},"fontFamily":"x; } body { display:none"}"#, "fontFamily"),
            (r#"{"name":"X","colors":{},"fontFamily":"url(https://evil.example/f.woff)"}"#, "fontFamily"),
            (r#"{"kansoTheme":2,"name":"X","colors":{}}"#, "kansoTheme"),
            ("not json", "Tema non valido"),
        ];
        for (raw, expected) in bad {
            let err = Theme::from_json(raw).expect_err(raw);
            assert!(err.contains(expected), "{raw} -> {err}");
        }
    }

    #[test]
    fn exported_file_round_trips_and_carries_no_id() {
        let mut theme = Theme::from_json(NORD).unwrap();
        theme.id = "nord-night".to_string();
        let json = theme.to_json();
        assert!(json.contains("\"kansoTheme\": 1"));
        assert!(!json.contains("\"id\""));

        let mut reparsed = Theme::from_json(&json).unwrap();
        assert_eq!(reparsed.id, "");
        reparsed.id = theme.id.clone();
        assert_eq!(reparsed, theme);
    }

    #[test]
    fn imports_lists_overwrites_and_deletes_user_themes() {
        let tmp = tempdir().unwrap();
        let themes_dir = tmp.path().join("themes");
        let source = tmp.path().join("downloaded theme.json");
        fs::write(&source, NORD).unwrap();

        let first = import_theme_file(&themes_dir, &source).expect("import");
        assert_eq!(first.id, "nord-night");
        // Importing the same file again never clobbers the installed one
        let second = import_theme_file(&themes_dir, &source).expect("import again");
        assert_eq!(second.id, "nord-night-2");

        // A file dropped into the folder by hand, plus one that is not a theme
        fs::write(themes_dir.join("My Theme!.json"), r##"{"name":"Zeta","colors":{"accent":"#ff0000"}}"##).unwrap();
        fs::write(themes_dir.join("broken.json"), "{ nope").unwrap();
        fs::write(themes_dir.join("notes.txt"), "ignored").unwrap();

        let (themes, warnings) = list_user_themes(&themes_dir);
        let ids: Vec<&str> = themes.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids, vec!["nord-night", "nord-night-2", "my-theme"]);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("broken.json"));

        // Editing keeps the id
        let mut edited = first.clone();
        edited.colors.accent = "#00ff00".to_string();
        let saved = save_user_theme(&themes_dir, &edited, Some("nord-night")).unwrap();
        assert_eq!(saved.id, "nord-night");
        let (themes, _) = list_user_themes(&themes_dir);
        assert_eq!(themes[0].colors.accent, "#00ff00");

        delete_user_theme(&themes_dir, "nord-night-2").unwrap();
        assert!(delete_user_theme(&themes_dir, "nord-night-2").is_err());
        assert!(delete_user_theme(&themes_dir, "../config").is_err(), "no path traversal");
        assert_eq!(list_user_themes(&themes_dir).0.len(), 2);

        // Exporting writes a file that imports back
        let exported = tmp.path().join("share.json");
        export_theme_file(&saved, &exported).unwrap();
        assert_eq!(Theme::from_json(&fs::read_to_string(&exported).unwrap()).unwrap().colors.accent, "#00ff00");
    }

    #[test]
    fn a_missing_themes_folder_is_just_an_empty_list() {
        let tmp = tempdir().unwrap();
        let (themes, warnings) = list_user_themes(&tmp.path().join("nope"));
        assert!(themes.is_empty() && warnings.is_empty());
    }

    #[test]
    fn the_example_theme_in_the_docs_is_valid() {
        let example = include_str!("../../../../docs/themes/nord.kanso-theme.json");
        let theme = Theme::from_json(example).expect("docs/themes/nord.kanso-theme.json must stay importable");
        assert_eq!(theme.name, "Nord");
        assert_eq!(theme.colors.accent, "#88c0d0");
        assert_eq!(theme.font_family, None);
    }
}
