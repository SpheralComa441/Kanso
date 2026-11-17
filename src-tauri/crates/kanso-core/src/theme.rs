//! Community themes: JSON files that restyle the whole interface.
//!
//! A theme sets colors, typography, the geometry of cards, controls and icons, a few layout
//! choices, what the side panel, the Home, Store and News pages are made of and, optionally,
//! the font files it needs. Every value is one of a fixed list of
//! fields and is validated here before it reaches the UI — a color must be a hex value, a size
//! a number in range, a shape one of the known names, a font file a real font. A theme file
//! therefore cannot carry arbitrary CSS, scripts or remote resources: importing one from a
//! stranger is safe.

use crate::i18n::T;
use crate::scanner::exe_pick::slugify;
use base64::engine::general_purpose::{STANDARD, STANDARD_NO_PAD};
use base64::Engine as _;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::fs;
use std::path::{Path, PathBuf};

/// Value of the `kansoTheme` marker written into exported files. Files marked 1 (colors,
/// three sizes and a font) or 2 are still read: everything they do not mention keeps its
/// default. Version 3 added the side panel, the Store and News pages, and the part of Home
/// where plugins put what they add.
pub const THEME_FORMAT_VERSION: u32 = 3;
/// Themes are small unless they embed fonts; this bounds the whole file.
const MAX_THEME_FILE_BYTES: u64 = 12 * 1024 * 1024;
const MAX_FONT_FILES: usize = 6;
/// Size limit of one embedded font, decoded.
const MAX_FONT_BYTES: usize = 2 * 1024 * 1024;
const FONT_FILE_EXTENSIONS: [&str; 4] = ["woff2", "woff", "ttf", "otf"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ColorScheme {
    #[default]
    Dark,
    Light,
}

/// The thirteen colors of a theme, as `#rrggbb` (or `#rrggbbaa`) strings.
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
    /// A second color that stands apart from the accent: by default it marks the pages of
    /// the side panel that are not the library. A theme that does not give one gets the
    /// accent turned around the color wheel.
    #[serde(default)]
    pub accent_secondary: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SidebarSide {
    #[default]
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Scrollbars {
    /// Thin, shown while the pointer is over the scrolling area.
    #[default]
    Auto,
    Always,
    Hidden,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WindowControls {
    /// Full-height minimize / maximize / close buttons.
    #[default]
    Bars,
    /// Small colored dots.
    Dots,
}

/// The color of a group of entries of the side panel: one of the theme's own colors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NavColor {
    Accent,
    /// The theme's second color.
    Secondary,
    /// The color of titles.
    Text,
    /// The quiet color of hints and counters.
    Muted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ThemeLayout {
    /// Minimum width of a cover card in the grid, in pixels.
    pub card_min_width: u32,
    /// Corner radius of cards (or the size of the cut, for cut shapes), in pixels.
    pub card_radius: u32,
    /// Width of the filter sidebar, in pixels.
    pub sidebar_width: u32,
    /// Space between cards, in pixels.
    pub grid_gap: u32,
    /// Height of the top bar, in pixels.
    pub header_height: u32,
    /// Padding around the main content, in pixels.
    pub content_padding: u32,
    pub sidebar_side: SidebarSide,
    /// Hairlines under the top bar and beside the sidebar.
    pub dividers: bool,
    pub scrollbars: Scrollbars,
    pub window_controls: WindowControls,
    /// Color of the headings of the side panel (Library, Status, Platform, Genres, Tags).
    pub nav_headings: NavColor,
    /// Color of the pages of the side panel that are not the library (Home, Statistics,
    /// Emulators).
    pub nav_pages: NavColor,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ThemeTypography {
    /// Font stack of titles; `None` uses the body font.
    pub heading_font: Option<String>,
    /// Weight of titles (100–900); `None` keeps the weight each title has by design.
    pub heading_weight: Option<u32>,
    pub heading_uppercase: bool,
    /// Letter spacing of titles, in em.
    pub heading_spacing: f64,
    /// Size of the whole interface relative to the default.
    pub scale: f64,
    /// Font stack of the small labels over figures and sections; `None` uses the body font.
    pub label_font: Option<String>,
    /// Small labels in capitals, letter-spaced.
    pub label_uppercase: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CardShape {
    #[default]
    Rounded,
    /// All four corners cut at 45°.
    Cut,
    /// Top-right and bottom-left corners cut.
    Slant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum CardAspect {
    #[default]
    #[serde(rename = "2:3")]
    Portrait,
    #[serde(rename = "3:4")]
    Tall,
    #[serde(rename = "4:5")]
    Short,
    #[serde(rename = "1:1")]
    Square,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CardShadow {
    #[default]
    #[serde(rename = "none")]
    Off,
    Soft,
    Strong,
    /// A halo in the accent color.
    Glow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CardHover {
    /// The card rises slightly and its cover zooms a little.
    #[default]
    Lift,
    /// The cover zooms inside a still card.
    Zoom,
    /// Accent border and halo.
    Glow,
    #[serde(rename = "none")]
    Off,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CardCaption {
    /// Title and genres under the cover.
    #[default]
    Below,
    /// Title over the bottom of the cover.
    Overlay,
    /// Cover only; under the pointer (or keyboard focus) the cover blurs and the game's
    /// title, genres and figures show over it.
    Hover,
    /// Cover only.
    #[serde(rename = "none")]
    Off,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ThemeCards {
    pub shape: CardShape,
    pub aspect: CardAspect,
    /// Border thickness, in pixels.
    pub border_width: u32,
    pub shadow: CardShadow,
    pub hover: CardHover,
    pub caption: CardCaption,
}

/// Corner radii of everything that is not a card, in pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ThemeShapes {
    /// Buttons, fields, rows of lists.
    pub control_radius: u32,
    /// Dialogs and large panels.
    pub panel_radius: u32,
    /// Badges, tags and sidebar entries.
    pub chip_radius: u32,
    /// Corners of panels and of the tiles of a game's sheet: rounded by `panel_radius`, or
    /// cut off diagonally by the same measure.
    pub panel_shape: PanelShape,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PanelShape {
    #[default]
    Rounded,
    /// Two opposite corners cut off diagonally.
    Cut,
}

/// What the sheet of a game looks like when it is opened.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DetailLayout {
    /// Separate tiles: the cover, the figures, the description, each in its own.
    #[default]
    Bento,
    /// One sheet: the cover on one side, everything else in a column.
    Sheet,
}

/// What is behind the open sheet of a game.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DetailBackdrop {
    /// The game's own artwork, blurred and veiled.
    #[default]
    Art,
    /// A plain veil over the library.
    Dim,
}

/// How the tiles of a game's sheet are filled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DetailTiles {
    /// The surface color.
    #[default]
    Filled,
    /// Translucent, with what is behind showing through blurred.
    Glass,
    /// The background color and a stronger border.
    Outlined,
}

/// How much the interface moves.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MotionLevel {
    /// Nothing moves.
    #[serde(rename = "none")]
    Off,
    /// Short fades, hardly any movement.
    Subtle,
    /// Fades with a short movement.
    #[default]
    Smooth,
    /// Longer, wider movements with a little bounce.
    Lively,
}

/// How a sheet or a dialog comes in (and goes out, the other way).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SheetMotion {
    /// From slightly below.
    #[default]
    Rise,
    /// Growing from slightly smaller.
    Zoom,
    /// From the side.
    Slide,
    Fade,
}

/// How the content changes when another entry of the side panel is chosen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ViewMotion {
    #[default]
    Rise,
    /// From the side of the panel.
    Slide,
    Fade,
}

/// A part of the Home page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HomeSection {
    /// The games being played, to pick up again.
    Resume,
    /// The games the user lined up to play next, and the ones marked "to play".
    UpNext,
    /// The games played the longest.
    MostPlayed,
    /// Suggestions from the stores, by the genres the user plays.
    Recommendations,
    /// Current discounts.
    Deals,
    /// What the plugins add to the page.
    Plugins,
}

/// What moves behind the top of the Home page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HomeEffects {
    /// Something of the season (snow, leaves…), of a holiday when one is near and, if the
    /// user asked for it, of the weather outside.
    #[default]
    Seasonal,
    #[serde(rename = "none")]
    Off,
}

/// The Home page: which of its parts are shown, in which order, and what moves behind it.
/// The user can still hide any part, and the effect, in Settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ThemeHome {
    /// The parts shown, in order. A part not named is not shown.
    pub sections: Vec<HomeSection>,
    pub effects: HomeEffects,
}

impl Default for ThemeHome {
    fn default() -> Self {
        Self {
            sections: vec![
                HomeSection::Resume,
                HomeSection::UpNext,
                HomeSection::MostPlayed,
                HomeSection::Recommendations,
                HomeSection::Deals,
                HomeSection::Plugins,
            ],
            effects: HomeEffects::Seasonal,
        }
    }
}

/// A page of the side panel beside the library.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NavPage {
    Home,
    Store,
    News,
    /// The statistics.
    Stats,
    Emulators,
}

/// A group of entries of the side panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NavGroup {
    /// Everything, favorites, recently played, not installed, hidden. Always there.
    Library,
    /// Where the user stands with a game: to play, playing, finished, abandoned.
    Status,
    Platform,
    Genres,
    Tags,
}

/// Where the pages of the side panel are, with respect to its groups.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PagesPosition {
    #[default]
    Top,
    Bottom,
}

/// The side panel: which pages and which groups it lists, and in which order. The user can
/// still hide Home, Store, News and the tags in Settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ThemeNav {
    /// The pages listed, in order. A page not named is not offered.
    pub pages: Vec<NavPage>,
    /// The groups listed, in order. A group not named is not shown, except the library:
    /// a theme cannot take that away.
    pub groups: Vec<NavGroup>,
    pub pages_position: PagesPosition,
}

impl Default for ThemeNav {
    fn default() -> Self {
        Self {
            pages: vec![NavPage::Home, NavPage::Store, NavPage::News, NavPage::Stats, NavPage::Emulators],
            groups: vec![NavGroup::Library, NavGroup::Status, NavGroup::Platform, NavGroup::Genres, NavGroup::Tags],
            pages_position: PagesPosition::Top,
        }
    }
}

/// A tab of the Store page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StoreTab {
    /// Every discount of the stores the user keeps.
    All,
    /// Suggestions by the genres the user plays.
    Picks,
    Steam,
    Epic,
    Gog,
    /// The other shops, as one group.
    Others,
    /// The deals the user liked.
    Liked,
}

/// How the Store page draws its deals.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StoreLayout {
    /// Tiles with the picture on top.
    #[default]
    Grid,
    /// Compact rows, one deal each.
    List,
}

/// The Store page: its tabs, how deals are drawn, and whether the figures are there. The
/// stores the user hid in Settings stay hidden.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ThemeStore {
    /// The tabs offered, in order; the page opens on the first. Never empty.
    pub tabs: Vec<StoreTab>,
    pub layout: StoreLayout,
    /// The row of figures at the top of the tab with every discount.
    pub figures: bool,
}

impl Default for ThemeStore {
    fn default() -> Self {
        Self {
            tabs: vec![
                StoreTab::All,
                StoreTab::Picks,
                StoreTab::Steam,
                StoreTab::Epic,
                StoreTab::Gog,
                StoreTab::Others,
                StoreTab::Liked,
            ],
            layout: StoreLayout::Grid,
            figures: true,
        }
    }
}

/// How the News page lays its items out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NewsLayout {
    /// The first item large, the others in a grid.
    #[default]
    Magazine,
    /// Every item the same size.
    Grid,
    /// Rows with a small picture at the side.
    List,
}

/// The News page: how its items are laid out and what each one shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ThemeNews {
    pub layout: NewsLayout,
    /// Items show their picture.
    pub pictures: bool,
    /// Items show the summary under the title.
    pub summaries: bool,
}

impl Default for ThemeNews {
    fn default() -> Self {
        Self { layout: NewsLayout::Magazine, pictures: true, summaries: true }
    }
}

/// The animations of the interface. The user can switch them off or pick another level in
/// the settings, and the accessibility preference "reduce motion" always wins.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ThemeMotion {
    pub level: MotionLevel,
    pub sheet: SheetMotion,
    pub view: ViewMotion,
    /// Cards come in one after the other when a view opens.
    pub stagger: bool,
}

impl Default for ThemeMotion {
    fn default() -> Self {
        Self {
            level: MotionLevel::Smooth,
            sheet: SheetMotion::Rise,
            view: ViewMotion::Rise,
            stagger: true,
        }
    }
}

/// The open sheet of a game.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ThemeDetail {
    pub layout: DetailLayout,
    pub backdrop: DetailBackdrop,
    /// Blur of the artwork behind the sheet, in pixels.
    pub backdrop_blur: u32,
    /// How much of the theme's background color veils what is behind the sheet (0–1): text
    /// stays legible over any artwork.
    pub backdrop_dim: f64,
    pub tiles: DetailTiles,
    /// Space between tiles, in pixels.
    pub tile_gap: u32,
    /// How the cards after the figures and the achievements (description, activity, what the
    /// plugins add, files) are laid out.
    pub cards: DetailCards,
    /// Color of the titles of what the sheet shows first: the figures, the achievements.
    pub key_titles: NavColor,
    /// Color of the titles of the other cards, and of the edge of the ones kept in sight.
    pub card_titles: NavColor,
}

/// How the cards of a game's sheet that come after its figures are laid out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DetailCards {
    /// Side by side in a box that moves on by itself; a card can be kept in sight.
    #[default]
    Rail,
    /// One under the other, all in sight.
    Stack,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IconCorners {
    #[default]
    Round,
    /// Square line ends and mitred joins.
    Sharp,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ThemeIcons {
    /// Line thickness (icons are drawn on a 24-unit grid).
    pub stroke_width: f64,
    pub corners: IconCorners,
    /// Size relative to the default.
    pub scale: f64,
    /// Opacity of a fill in the icon's own color (0 leaves icons as outlines).
    pub fill: f64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ThemeEffects {
    /// Two or three colors for a gradient window background; empty for the plain color.
    pub background_gradient: Vec<String>,
    /// Direction of the gradient in degrees (180 = top to bottom).
    pub background_angle: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FontStyle {
    #[default]
    Normal,
    Italic,
}

/// A font shipped inside the theme, so it works on PCs where it is not installed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemeFontFile {
    /// Name the theme's font stacks refer to.
    pub family: String,
    /// One weight (`"400"`) or, for variable fonts, a range (`"100 900"`).
    pub weight: String,
    pub style: FontStyle,
    /// The font file (woff2, woff, ttf or otf), base64-encoded.
    pub data: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
    /// CSS font stack of the interface; `None` keeps Kanso's default.
    pub font_family: Option<String>,
    #[serde(default)]
    pub typography: ThemeTypography,
    #[serde(default)]
    pub layout: ThemeLayout,
    #[serde(default)]
    pub cards: ThemeCards,
    #[serde(default)]
    pub shapes: ThemeShapes,
    #[serde(default)]
    pub icons: ThemeIcons,
    #[serde(default)]
    pub effects: ThemeEffects,
    #[serde(default)]
    pub detail: ThemeDetail,
    #[serde(default)]
    pub motion: ThemeMotion,
    #[serde(default)]
    pub home: ThemeHome,
    #[serde(default)]
    pub nav: ThemeNav,
    #[serde(default)]
    pub store: ThemeStore,
    #[serde(default)]
    pub news: ThemeNews,
    #[serde(default)]
    pub font_files: Vec<ThemeFontFile>,
}

impl ThemeColors {
    /// Kanso's own palette for the given scheme; the base every theme starts from.
    pub fn defaults(scheme: ColorScheme) -> Self {
        let colors = match scheme {
            // The slate and the orange of Kanso's logo
            ColorScheme::Dark => [
                "#17181f", "#1e2029", "#282a37", "#343747", "#5b5f78", "#f3f4f8", "#d5d7e1",
                "#a4a8ba", "#868aa1", "#ff4500", "#ff6a33", "#0d0d10", "#3ecbe0",
            ],
            // The logo in ink, on paper
            ColorScheme::Light => [
                "#f5f5f2", "#ffffff", "#eaeae6", "#d9d9d4", "#9d9d98", "#1e1e1e", "#3a3a3a",
                "#595959", "#6b6b6b", "#1e1e1e", "#000000", "#ffffff", "#c43c00",
            ],
        };
        let [background, surface, elevated, border, border_strong, text, text_secondary, text_muted, text_faint, accent, accent_hover, on_accent, accent_secondary] =
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
            accent_secondary,
        }
    }
}

/// A color that stands apart from `accent` with the same strength: the accent turned 165
/// degrees around the color wheel, its saturation and lightness kept. A grey stays itself.
pub fn second_accent(accent: &str) -> String {
    let channel = |at: usize| -> f64 {
        accent.get(at..at + 2).and_then(|hex| u8::from_str_radix(hex, 16).ok()).map(f64::from).unwrap_or(0.0) / 255.0
    };
    let (r, g, b) = (channel(1), channel(3), channel(5));
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let lightness = (max + min) / 2.0;
    let spread = max - min;
    if spread < 1e-6 {
        return accent.get(..7).unwrap_or(accent).to_string();
    }
    let saturation = spread / (1.0 - (2.0 * lightness - 1.0).abs());
    let hue = if max == r {
        ((g - b) / spread).rem_euclid(6.0)
    } else if max == g {
        (b - r) / spread + 2.0
    } else {
        (r - g) / spread + 4.0
    } * 60.0;
    let hue = (hue + 165.0).rem_euclid(360.0);

    let chroma = (1.0 - (2.0 * lightness - 1.0).abs()) * saturation;
    let x = chroma * (1.0 - ((hue / 60.0).rem_euclid(2.0) - 1.0).abs());
    let (r, g, b) = match (hue / 60.0) as u32 {
        0 => (chroma, x, 0.0),
        1 => (x, chroma, 0.0),
        2 => (0.0, chroma, x),
        3 => (0.0, x, chroma),
        4 => (x, 0.0, chroma),
        _ => (chroma, 0.0, x),
    };
    let lift = lightness - chroma / 2.0;
    let byte = |value: f64| ((value + lift) * 255.0).round().clamp(0.0, 255.0) as u8;
    format!("#{:02x}{:02x}{:02x}", byte(r), byte(g), byte(b))
}

impl Default for ThemeLayout {
    fn default() -> Self {
        Self {
            card_min_width: 185,
            card_radius: 10,
            sidebar_width: 208,
            grid_gap: 16,
            header_height: 52,
            content_padding: 24,
            sidebar_side: SidebarSide::Left,
            dividers: false,
            scrollbars: Scrollbars::Auto,
            window_controls: WindowControls::Bars,
            nav_headings: NavColor::Accent,
            nav_pages: NavColor::Secondary,
        }
    }
}

impl Default for ThemeTypography {
    fn default() -> Self {
        Self {
            heading_font: None,
            heading_weight: None,
            heading_uppercase: false,
            heading_spacing: 0.0,
            scale: 1.0,
            label_font: None,
            label_uppercase: true,
        }
    }
}

impl Default for ThemeCards {
    fn default() -> Self {
        Self {
            shape: CardShape::Rounded,
            aspect: CardAspect::Portrait,
            border_width: 1,
            shadow: CardShadow::Off,
            hover: CardHover::Lift,
            caption: CardCaption::Below,
        }
    }
}

impl Default for ThemeShapes {
    fn default() -> Self {
        Self {
            control_radius: 8,
            panel_radius: 12,
            chip_radius: 6,
            panel_shape: PanelShape::Rounded,
        }
    }
}

impl Default for ThemeDetail {
    fn default() -> Self {
        Self {
            layout: DetailLayout::Bento,
            backdrop: DetailBackdrop::Art,
            backdrop_blur: 18,
            backdrop_dim: 0.62,
            tiles: DetailTiles::Filled,
            tile_gap: 12,
            cards: DetailCards::Rail,
            key_titles: NavColor::Accent,
            card_titles: NavColor::Secondary,
        }
    }
}

impl Default for ThemeIcons {
    fn default() -> Self {
        Self {
            stroke_width: 2.0,
            corners: IconCorners::Round,
            scale: 1.0,
            fill: 0.0,
        }
    }
}

impl Default for ThemeEffects {
    fn default() -> Self {
        Self {
            background_gradient: Vec::new(),
            background_angle: 180,
        }
    }
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

/// A CSS font stack made only of plain font names: nothing that could end the declaration it
/// is placed in or reference a resource.
fn is_plain_font_stack(stack: &str) -> bool {
    stack.len() <= 200
        && stack
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, ' ' | ',' | '-' | '_' | '\'' | '"'))
}

fn is_plain_font_name(name: &str) -> bool {
    !name.is_empty()
        && name.chars().count() <= 60
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, ' ' | '-' | '_'))
}

/// One object of a theme file, read field by field. A missing (or `null`) field takes its
/// default; a field of the wrong kind is an error naming it, so authors know what to fix;
/// numbers out of range are brought to the nearest limit, so the theme still looks as meant.
struct Fields<'a> {
    path: &'a str,
    map: Option<&'a Map<String, Value>>,
}

impl<'a> Fields<'a> {
    /// The object under `key` of `root`; absent is fine, anything but an object is not.
    fn section(root: &'a Map<String, Value>, key: &'a str) -> Result<Self, String> {
        match root.get(key) {
            None | Some(Value::Null) => Ok(Self { path: key, map: None }),
            Some(Value::Object(map)) => Ok(Self {
                path: key,
                map: Some(map),
            }),
            Some(_) => Err(invalid(key)),
        }
    }

    fn name_of(&self, key: &str) -> String {
        if self.path.is_empty() {
            key.to_string()
        } else {
            format!("{}.{key}", self.path)
        }
    }

    fn get(&self, key: &str) -> Option<&'a Value> {
        self.map?.get(key).filter(|value| !value.is_null())
    }

    fn wrong(&self, key: &str, value: &Value) -> String {
        invalid(format!(
            "{} {}",
            self.name_of(key),
            clean_text(&value.to_string(), 48)
        ))
    }

    fn number(&self, key: &str, default: f64, min: f64, max: f64) -> Result<f64, String> {
        match self.get(key) {
            None => Ok(default),
            Some(value) => value
                .as_f64()
                .filter(|n| n.is_finite())
                .map(|n| n.clamp(min, max))
                .ok_or_else(|| self.wrong(key, value)),
        }
    }

    fn int(&self, key: &str, default: u32, min: u32, max: u32) -> Result<u32, String> {
        self.number(key, f64::from(default), f64::from(min), f64::from(max))
            .map(|n| n.round() as u32)
    }

    /// A number kept to two decimals.
    fn decimal(&self, key: &str, default: f64, min: f64, max: f64) -> Result<f64, String> {
        self.number(key, default, min, max)
            .map(|n| (n * 100.0).round() / 100.0)
    }

    fn flag(&self, key: &str, default: bool) -> Result<bool, String> {
        match self.get(key) {
            None => Ok(default),
            Some(value) => value.as_bool().ok_or_else(|| self.wrong(key, value)),
        }
    }

    fn text(&self, key: &str) -> Result<Option<&'a str>, String> {
        match self.get(key) {
            None => Ok(None),
            Some(value) => value
                .as_str()
                .map(Some)
                .ok_or_else(|| self.wrong(key, value)),
        }
    }

    /// One of the names an enum accepts.
    fn choice<C: DeserializeOwned>(&self, key: &str, default: C) -> Result<C, String> {
        match self.get(key) {
            None => Ok(default),
            Some(value) => {
                serde_json::from_value(value.clone()).map_err(|_| self.wrong(key, value))
            }
        }
    }

    /// A list of names an enum accepts, each kept once, in the order written. `None` when
    /// the theme does not say; anything that is not a list of known names is an error.
    fn names<C: DeserializeOwned + PartialEq>(&self, key: &str) -> Result<Option<Vec<C>>, String> {
        match self.get(key) {
            None => Ok(None),
            Some(value @ Value::Array(entries)) => {
                let mut names: Vec<C> = Vec::new();
                for entry in entries {
                    let name: C =
                        serde_json::from_value(entry.clone()).map_err(|_| self.wrong(key, value))?;
                    if !names.contains(&name) {
                        names.push(name);
                    }
                }
                Ok(Some(names))
            }
            Some(other) => Err(self.wrong(key, other)),
        }
    }

    fn color(&self, key: &str) -> Result<Option<String>, String> {
        match self.get(key) {
            None => Ok(None),
            Some(value) => value
                .as_str()
                .and_then(normalize_hex_color)
                .map(Some)
                .ok_or_else(|| self.wrong(key, value)),
        }
    }

    fn font_stack(&self, key: &str) -> Result<Option<String>, String> {
        match self.text(key)?.map(str::trim) {
            None | Some("") => Ok(None),
            Some(stack) if is_plain_font_stack(stack) => Ok(Some(stack.to_string())),
            Some(_) => Err(invalid(self.name_of(key))),
        }
    }
}

fn looks_like_font(bytes: &[u8]) -> bool {
    const SIGNATURES: [&[u8]; 5] = [b"wOF2", b"wOFF", b"OTTO", b"true", &[0, 1, 0, 0]];
    SIGNATURES.iter().any(|signature| bytes.starts_with(signature))
}

/// Decodes the `data` of a font entry: base64, optionally as a `data:` URL.
fn decode_font_data(raw: &str) -> Option<Vec<u8>> {
    let payload = match raw.trim().strip_prefix("data:") {
        Some(rest) => rest.split_once("base64,")?.1,
        None => raw,
    };
    // Checked before decoding: an oversized entry must not be decoded at all
    if payload.len() > MAX_FONT_BYTES / 3 * 4 + 1024 {
        return None;
    }
    let compact: String = payload.chars().filter(|c| !c.is_whitespace()).collect();
    STANDARD
        .decode(&compact)
        .or_else(|_| STANDARD_NO_PAD.decode(compact.trim_end_matches('=')))
        .ok()
}

/// Reads a font a theme file names with `file`: a plain file name next to the theme, never a
/// path, so a theme cannot make Kanso read anything else.
fn read_sibling_font(dir: Option<&Path>, name: &str) -> Result<Vec<u8>, String> {
    let dir = dir.ok_or("not available here")?;
    let extension_ok = Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| FONT_FILE_EXTENSIONS.iter().any(|known| e.eq_ignore_ascii_case(known)));
    let is_plain_name = Path::new(name).file_name().and_then(|n| n.to_str()) == Some(name)
        && !name.contains(['/', '\\', ':']);
    if !is_plain_name || !extension_ok {
        return Err(format!("\"{}\"", clean_text(name, 60)));
    }
    let path = dir.join(name);
    let size = fs::metadata(&path)
        .map_err(|e| format!("{}: {e}", path.display()))?
        .len();
    if size > MAX_FONT_BYTES as u64 {
        return Err(format!("{size} bytes"));
    }
    fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))
}

fn parse_font_files(
    root: &Map<String, Value>,
    fonts_dir: Option<&Path>,
) -> Result<Vec<ThemeFontFile>, String> {
    let entries = match root.get("fontFiles") {
        None | Some(Value::Null) => return Ok(Vec::new()),
        Some(Value::Array(entries)) => entries,
        Some(_) => return Err(invalid("fontFiles")),
    };
    if entries.len() > MAX_FONT_FILES {
        return Err(invalid(format!(
            "fontFiles ({} > {MAX_FONT_FILES})",
            entries.len()
        )));
    }

    let mut fonts = Vec::with_capacity(entries.len());
    for (index, entry) in entries.iter().enumerate() {
        let path = format!("fontFiles[{index}]");
        let Value::Object(map) = entry else {
            return Err(invalid(&path));
        };
        let fields = Fields {
            path: &path,
            map: Some(map),
        };

        let family = fields
            .text("family")?
            .map(str::trim)
            .filter(|name| is_plain_font_name(name))
            .ok_or_else(|| invalid(fields.name_of("family")))?
            .to_string();

        let weight = match fields.get("weight") {
            None => "400".to_string(),
            Some(Value::Number(n)) => n.to_string(),
            Some(Value::String(s)) => s.split_whitespace().collect::<Vec<_>>().join(" "),
            Some(other) => return Err(fields.wrong("weight", other)),
        };
        let weights: Vec<&str> = weight.split(' ').collect();
        let weight_ok = (1..=2).contains(&weights.len())
            && weights
                .iter()
                .all(|w| w.parse::<u32>().is_ok_and(|w| (1..=1000).contains(&w)));
        if !weight_ok {
            return Err(invalid(fields.name_of("weight")));
        }

        let style = fields.choice("style", FontStyle::Normal)?;

        let bytes = match (fields.text("data")?, fields.text("file")?) {
            (Some(data), _) => {
                decode_font_data(data).ok_or_else(|| invalid(fields.name_of("data")))?
            }
            (None, Some(name)) => read_sibling_font(fonts_dir, name)
                .map_err(|detail| invalid(format!("{} {detail}", fields.name_of("file"))))?,
            (None, None) => return Err(invalid(fields.name_of("data"))),
        };
        if bytes.len() > MAX_FONT_BYTES || !looks_like_font(&bytes) {
            return Err(invalid(format!(
                "{path} (woff2/woff/ttf/otf, max {} MB)",
                MAX_FONT_BYTES / (1024 * 1024)
            )));
        }

        fonts.push(ThemeFontFile {
            family,
            weight,
            style,
            data: STANDARD.encode(bytes),
        });
    }
    Ok(fonts)
}

impl Theme {
    /// Parses and validates a theme. Missing values fall back to Kanso's defaults (colors: for
    /// the theme's color scheme); anything present but wrong is an error that names the
    /// offending field.
    pub fn from_json(raw: &str) -> Result<Self, String> {
        Self::parse(raw, None)
    }

    /// Like [`Theme::from_json`] for a theme file on disk: font files it names with `file`
    /// are read from `dir`, the folder the theme is in, and embedded.
    pub fn from_json_in(raw: &str, dir: &Path) -> Result<Self, String> {
        Self::parse(raw, Some(dir))
    }

    fn parse(raw: &str, fonts_dir: Option<&Path>) -> Result<Self, String> {
        let raw = raw.trim_start_matches('\u{feff}');
        let Value::Object(root) = serde_json::from_str::<Value>(raw).map_err(invalid)? else {
            return Err(invalid("{ ... }"));
        };
        let top = Fields {
            path: "",
            map: Some(&root),
        };

        let version = top.int("kansoTheme", 1, 0, u32::MAX)?;
        if version > THEME_FORMAT_VERSION {
            return Err(invalid(format!(
                "kansoTheme {version} (max {THEME_FORMAT_VERSION})"
            )));
        }

        let name = clean_text(top.text("name")?.unwrap_or_default(), 60);
        if name.is_empty() {
            return Err(invalid("name"));
        }

        let color_scheme = match top.text("colorScheme")?.map(str::trim) {
            None | Some("") | Some("dark") => ColorScheme::Dark,
            Some("light") => ColorScheme::Light,
            Some(other) => return Err(invalid(format!("colorScheme \"{other}\""))),
        };

        let c = Fields::section(&root, "colors")?;
        if c.map.is_none() {
            return Err(invalid("colors"));
        }
        let base = ThemeColors::defaults(color_scheme);
        // A theme with its own accent and no second color gets one drawn from that accent;
        // one that keeps Kanso's accent keeps Kanso's second color too
        // (an interface older than this color sends none: an empty value is no value)
        let own_second = match c.get("accentSecondary") {
            Some(Value::String(text)) if text.trim().is_empty() => None,
            _ => c.color("accentSecondary")?,
        };
        let accent_secondary = match (own_second, c.color("accent")?) {
            (Some(own), _) => own,
            (None, Some(accent)) => second_accent(&accent),
            (None, None) => base.accent_secondary,
        };
        let colors = ThemeColors {
            background: c.color("background")?.unwrap_or(base.background),
            surface: c.color("surface")?.unwrap_or(base.surface),
            elevated: c.color("elevated")?.unwrap_or(base.elevated),
            border: c.color("border")?.unwrap_or(base.border),
            border_strong: c.color("borderStrong")?.unwrap_or(base.border_strong),
            text: c.color("text")?.unwrap_or(base.text),
            text_secondary: c.color("textSecondary")?.unwrap_or(base.text_secondary),
            text_muted: c.color("textMuted")?.unwrap_or(base.text_muted),
            text_faint: c.color("textFaint")?.unwrap_or(base.text_faint),
            accent: c.color("accent")?.unwrap_or(base.accent),
            accent_hover: c.color("accentHover")?.unwrap_or(base.accent_hover),
            on_accent: c.color("onAccent")?.unwrap_or(base.on_accent),
            accent_secondary,
        };

        let t = Fields::section(&root, "typography")?;
        let typography = ThemeTypography {
            heading_font: t.font_stack("headingFont")?,
            heading_weight: match t.get("headingWeight") {
                None => None,
                // Font weights come in hundreds
                Some(_) => Some((t.int("headingWeight", 600, 100, 900)? + 50) / 100 * 100),
            },
            heading_uppercase: t.flag("headingUppercase", false)?,
            heading_spacing: t.decimal("headingSpacing", 0.0, -0.05, 0.3)?,
            scale: t.decimal("scale", 1.0, 0.85, 1.25)?,
            label_font: t.font_stack("labelFont")?,
            label_uppercase: t.flag("labelUppercase", true)?,
        };

        let l = Fields::section(&root, "layout")?;
        let d = ThemeLayout::default();
        let layout = ThemeLayout {
            card_min_width: l.int("cardMinWidth", d.card_min_width, 120, 320)?,
            card_radius: l.int("cardRadius", d.card_radius, 0, 40)?,
            sidebar_width: l.int("sidebarWidth", d.sidebar_width, 160, 320)?,
            grid_gap: l.int("gridGap", d.grid_gap, 2, 40)?,
            header_height: l.int("headerHeight", d.header_height, 44, 72)?,
            content_padding: l.int("contentPadding", d.content_padding, 8, 56)?,
            sidebar_side: l.choice("sidebarSide", d.sidebar_side)?,
            dividers: l.flag("dividers", d.dividers)?,
            scrollbars: l.choice("scrollbars", d.scrollbars)?,
            window_controls: l.choice("windowControls", d.window_controls)?,
            nav_headings: l.choice("navHeadings", d.nav_headings)?,
            nav_pages: l.choice("navPages", d.nav_pages)?,
        };

        let k = Fields::section(&root, "cards")?;
        let d = ThemeCards::default();
        let cards = ThemeCards {
            shape: k.choice("shape", d.shape)?,
            aspect: k.choice("aspect", d.aspect)?,
            border_width: k.int("borderWidth", d.border_width, 0, 4)?,
            shadow: k.choice("shadow", d.shadow)?,
            hover: k.choice("hover", d.hover)?,
            caption: k.choice("caption", d.caption)?,
        };

        let s = Fields::section(&root, "shapes")?;
        let d = ThemeShapes::default();
        let shapes = ThemeShapes {
            control_radius: s.int("controlRadius", d.control_radius, 0, 24)?,
            panel_radius: s.int("panelRadius", d.panel_radius, 0, 32)?,
            chip_radius: s.int("chipRadius", d.chip_radius, 0, 16)?,
            panel_shape: s.choice("panelShape", d.panel_shape)?,
        };

        let g = Fields::section(&root, "detail")?;
        let d = ThemeDetail::default();
        let detail = ThemeDetail {
            layout: g.choice("layout", d.layout)?,
            backdrop: g.choice("backdrop", d.backdrop)?,
            backdrop_blur: g.int("backdropBlur", d.backdrop_blur, 0, 48)?,
            // Never so thin that text over a bright cover cannot be read
            backdrop_dim: g.decimal("backdropDim", d.backdrop_dim, 0.35, 0.95)?,
            tiles: g.choice("tiles", d.tiles)?,
            tile_gap: g.int("tileGap", d.tile_gap, 0, 28)?,
            cards: g.choice("cards", d.cards)?,
            key_titles: g.choice("keyTitles", d.key_titles)?,
            card_titles: g.choice("cardTitles", d.card_titles)?,
        };

        let m = Fields::section(&root, "motion")?;
        let d = ThemeMotion::default();
        let motion = ThemeMotion {
            level: m.choice("level", d.level)?,
            sheet: m.choice("sheet", d.sheet)?,
            view: m.choice("view", d.view)?,
            stagger: m.flag("stagger", d.stagger)?,
        };

        let h = Fields::section(&root, "home")?;
        let d = ThemeHome::default();
        let home = ThemeHome {
            sections: match h.names::<HomeSection>("sections")? {
                None => d.sections,
                Some(mut sections) => {
                    // A file older than format 3 could not name the part where plugins put
                    // what they add: its list gets that part at the end. A format 3 file is
                    // taken as written, and a part it does not name is not shown.
                    if version < 3 && !sections.contains(&HomeSection::Plugins) {
                        sections.push(HomeSection::Plugins);
                    }
                    sections
                }
            },
            effects: h.choice("effects", d.effects)?,
        };

        let n = Fields::section(&root, "nav")?;
        let d = ThemeNav::default();
        let nav = ThemeNav {
            pages: n.names("pages")?.unwrap_or(d.pages),
            groups: match n.names::<NavGroup>("groups")? {
                None => d.groups,
                // The library is always there: a theme that leaves it out gets it first
                Some(mut groups) => {
                    if !groups.contains(&NavGroup::Library) {
                        groups.insert(0, NavGroup::Library);
                    }
                    groups
                }
            },
            pages_position: n.choice("pagesPosition", d.pages_position)?,
        };

        let o = Fields::section(&root, "store")?;
        let d = ThemeStore::default();
        let store = ThemeStore {
            tabs: match o.names::<StoreTab>("tabs")? {
                None => d.tabs,
                // A Store page with no tab would have nothing to open on
                Some(tabs) if tabs.is_empty() => return Err(invalid(format!("{} []", o.name_of("tabs")))),
                Some(tabs) => tabs,
            },
            layout: o.choice("layout", d.layout)?,
            figures: o.flag("figures", d.figures)?,
        };

        let w = Fields::section(&root, "news")?;
        let d = ThemeNews::default();
        let news = ThemeNews {
            layout: w.choice("layout", d.layout)?,
            pictures: w.flag("pictures", d.pictures)?,
            summaries: w.flag("summaries", d.summaries)?,
        };

        let i = Fields::section(&root, "icons")?;
        let d = ThemeIcons::default();
        let icons = ThemeIcons {
            stroke_width: i.decimal("strokeWidth", d.stroke_width, 1.0, 3.0)?,
            corners: i.choice("corners", d.corners)?,
            scale: i.decimal("scale", d.scale, 0.8, 1.4)?,
            fill: i.decimal("fill", d.fill, 0.0, 0.6)?,
        };

        let e = Fields::section(&root, "effects")?;
        let background_gradient = match e.get("backgroundGradient") {
            None => Vec::new(),
            Some(Value::Array(stops)) if stops.is_empty() => Vec::new(),
            Some(value @ Value::Array(stops)) if (2..=3).contains(&stops.len()) => stops
                .iter()
                .map(|stop| stop.as_str().and_then(normalize_hex_color))
                .collect::<Option<Vec<_>>>()
                .ok_or_else(|| e.wrong("backgroundGradient", value))?,
            Some(other) => return Err(e.wrong("backgroundGradient", other)),
        };
        let effects = ThemeEffects {
            background_gradient,
            background_angle: e.int("backgroundAngle", 180, 0, 360)?,
        };

        Ok(Self {
            id: String::new(),
            name,
            author: clean_text(top.text("author")?.unwrap_or_default(), 60),
            description: clean_text(top.text("description")?.unwrap_or_default(), 240),
            color_scheme,
            colors,
            font_family: top.font_stack("fontFamily")?,
            typography,
            layout,
            cards,
            shapes,
            icons,
            effects,
            detail,
            motion,
            home,
            nav,
            store,
            news,
            font_files: parse_font_files(&root, fonts_dir)?,
        })
    }

    /// The theme as a shareable file: pretty-printed, with the format marker, without the id
    /// and with every value spelled out, so an exported theme doubles as a template.
    pub fn to_json(&self) -> String {
        let value = serde_json::json!({
            "kansoTheme": THEME_FORMAT_VERSION,
            "name": self.name,
            "author": self.author,
            "description": self.description,
            "colorScheme": self.color_scheme,
            "colors": self.colors,
            "fontFamily": self.font_family,
            "typography": self.typography,
            "layout": self.layout,
            "cards": self.cards,
            "shapes": self.shapes,
            "icons": self.icons,
            "effects": self.effects,
            "detail": self.detail,
            "motion": self.motion,
            "home": self.home,
            "nav": self.nav,
            "store": self.store,
            "news": self.news,
            "fontFiles": self.font_files,
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
    match path.parent() {
        Some(dir) => Theme::from_json_in(&raw, dir),
        None => Theme::from_json(&raw),
    }
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

/// Validates the theme file at `source` and installs a copy in the themes folder. Fonts the
/// file refers to by name are embedded, so the installed copy stands on its own.
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

    /// The smallest thing that passes for a font file: a woff2 signature and some bytes.
    const FAKE_WOFF2: &[u8] = b"wOF2\x00\x01\x00\x00kanso-test-font";

    #[test]
    fn parses_a_version_1_theme_filling_defaults_and_normalising_values() {
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
        // Everything version 1 could not express keeps Kanso's look
        assert_eq!(theme.typography, ThemeTypography::default());
        assert_eq!(theme.cards, ThemeCards::default());
        assert_eq!(theme.shapes, ThemeShapes::default());
        assert_eq!(theme.icons, ThemeIcons::default());
        assert_eq!(theme.effects, ThemeEffects::default());
        assert!(theme.font_files.is_empty());

        let light = Theme::from_json(r#"{"name":"Carta","colorScheme":"light","colors":{}}"#).unwrap();
        assert_eq!(light.colors, ThemeColors::defaults(ColorScheme::Light));
    }

    #[test]
    fn parses_shapes_typography_icons_and_effects() {
        let theme = Theme::from_json(
            r##"{
                "kansoTheme": 2,
                "name": "Arcade",
                "colors": {},
                "fontFamily": "Pixelify Sans",
                "typography": { "headingFont": "Tektur", "headingWeight": 760, "headingUppercase": true, "headingSpacing": 0.084, "scale": 9 },
                "layout": { "gridGap": 0, "headerHeight": 60, "contentPadding": 32, "sidebarSide": "right", "dividers": true, "scrollbars": "hidden", "windowControls": "dots" },
                "cards": { "shape": "cut", "aspect": "1:1", "borderWidth": 9, "shadow": "glow", "hover": "none", "caption": "overlay" },
                "shapes": { "controlRadius": 0, "panelRadius": 99, "chipRadius": 2.6 },
                "icons": { "strokeWidth": 1.256, "corners": "sharp", "scale": 1.2, "fill": 5 },
                "effects": { "backgroundGradient": ["#ABC", "#102030"], "backgroundAngle": 135 }
            }"##,
        )
        .expect("valid theme");

        assert_eq!(theme.font_family.as_deref(), Some("Pixelify Sans"));
        assert_eq!(theme.typography.heading_font.as_deref(), Some("Tektur"));
        assert_eq!(theme.typography.heading_weight, Some(800), "rounded to a real weight");
        assert!(theme.typography.heading_uppercase);
        assert_eq!(theme.typography.heading_spacing, 0.08);
        assert_eq!(theme.typography.scale, 1.25, "clamped");

        assert_eq!(theme.layout.grid_gap, 2, "clamped");
        assert_eq!(theme.layout.header_height, 60);
        assert_eq!(theme.layout.content_padding, 32);
        assert_eq!(theme.layout.sidebar_side, SidebarSide::Right);
        assert!(theme.layout.dividers);
        assert_eq!(theme.layout.scrollbars, Scrollbars::Hidden);
        assert_eq!(theme.layout.window_controls, WindowControls::Dots);
        assert_eq!(theme.layout.card_min_width, 185, "untouched fields keep their default");

        assert_eq!(theme.cards.shape, CardShape::Cut);
        assert_eq!(theme.cards.aspect, CardAspect::Square);
        assert_eq!(theme.cards.border_width, 4, "clamped");
        assert_eq!(theme.cards.shadow, CardShadow::Glow);
        assert_eq!(theme.cards.hover, CardHover::Off);
        assert_eq!(theme.cards.caption, CardCaption::Overlay);

        assert_eq!(theme.shapes.control_radius, 0);
        assert_eq!(theme.shapes.panel_radius, 32, "clamped");
        assert_eq!(theme.shapes.chip_radius, 3);

        assert_eq!(theme.icons.stroke_width, 1.26);
        assert_eq!(theme.icons.corners, IconCorners::Sharp);
        assert_eq!(theme.icons.scale, 1.2);
        assert_eq!(theme.icons.fill, 0.6, "clamped");

        assert_eq!(theme.effects.background_gradient, vec!["#aabbcc", "#102030"]);
        assert_eq!(theme.effects.background_angle, 135);
    }

    #[test]
    fn parses_the_sheet_of_a_game_and_the_small_labels() {
        // Nothing said: the defaults
        let plain = Theme::from_json(r#"{"name":"X","colors":{}}"#).unwrap();
        assert_eq!(plain.detail, ThemeDetail::default());
        assert_eq!(plain.detail.layout, DetailLayout::Bento);
        assert_eq!(plain.detail.backdrop, DetailBackdrop::Art);
        assert_eq!(plain.shapes.panel_shape, PanelShape::Rounded);
        assert_eq!(plain.typography.label_font, None);
        assert!(plain.typography.label_uppercase);

        let theme = Theme::from_json(
            r#"{"name":"X","colors":{},
                "typography":{"labelFont":"JetBrains Mono, monospace","labelUppercase":false},
                "shapes":{"panelShape":"cut"},
                "detail":{"layout":"sheet","backdrop":"dim","backdropBlur":999,"backdropDim":0.05,"tiles":"glass","tileGap":40}}"#,
        )
        .unwrap();
        assert_eq!(theme.typography.label_font.as_deref(), Some("JetBrains Mono, monospace"));
        assert!(!theme.typography.label_uppercase);
        assert_eq!(theme.shapes.panel_shape, PanelShape::Cut);
        assert_eq!(theme.detail.layout, DetailLayout::Sheet);
        assert_eq!(theme.detail.backdrop, DetailBackdrop::Dim);
        assert_eq!(theme.detail.tiles, DetailTiles::Glass);
        // Out of range: brought to the nearest limit, and the veil never so thin that text
        // over a bright cover cannot be read
        assert_eq!(theme.detail.backdrop_blur, 48);
        assert_eq!(theme.detail.backdrop_dim, 0.35);
        assert_eq!(theme.detail.tile_gap, 28);

        // Animations: a level and a way of moving, from closed lists
        assert_eq!(plain.motion, ThemeMotion::default());
        assert_eq!(plain.motion.level, MotionLevel::Smooth);
        let moving = Theme::from_json(
            r#"{"name":"X","colors":{},"cards":{"caption":"hover"},
                "motion":{"level":"lively","sheet":"zoom","view":"slide","stagger":false}}"#,
        )
        .unwrap();
        assert_eq!(moving.cards.caption, CardCaption::Hover);
        assert_eq!(
            moving.motion,
            ThemeMotion { level: MotionLevel::Lively, sheet: SheetMotion::Zoom, view: ViewMotion::Slide, stagger: false }
        );
        let still = Theme::from_json(r#"{"name":"X","colors":{},"motion":{"level":"none"}}"#).unwrap();
        assert_eq!(still.motion.level, MotionLevel::Off);
        assert!(still.to_json().contains("\"level\": \"none\""));
        assert_eq!(Theme::from_json(&moving.to_json()).unwrap().motion, moving.motion);

        // An exported theme spells the new sections out, and reads back the same
        let exported = theme.to_json();
        assert!(exported.contains("\"detail\""));
        assert!(exported.contains("\"panelShape\": \"cut\""));
        assert_eq!(Theme::from_json(&exported).unwrap().detail, theme.detail);
    }

    #[test]
    fn rejects_anything_that_is_not_a_plain_validated_value() {
        let bad = [
            (r#"{"colors":{}}"#, "name"),
            (r#"{"name":"X"}"#, "colors"),
            (r#"{"name":"X","colors":"red"}"#, "colors"),
            (r#"{"name":"X","colors":{"accent":"red"}}"#, "colors.accent"),
            (r#"{"name":"X","colors":{"accent":12}}"#, "colors.accent"),
            (r#"{"name":"X","colors":{"background":"url(https://evil.example/x.png)"}}"#, "colors.background"),
            (r##"{"name":"X","colors":{"text":"#12345"}}"##, "colors.text"),
            (r#"{"name":"X","colorScheme":"sepia","colors":{}}"#, "colorScheme"),
            (r#"{"name":"X","colors":{},"fontFamily":"x; } body { display:none"}"#, "fontFamily"),
            (r#"{"name":"X","colors":{},"fontFamily":"url(https://evil.example/f.woff)"}"#, "fontFamily"),
            (r#"{"name":"X","colors":{},"typography":{"headingFont":"a; } * { display:none"}}"#, "typography.headingFont"),
            (r#"{"name":"X","colors":{},"typography":{"scale":"big"}}"#, "typography.scale"),
            (r#"{"name":"X","colors":{},"layout":{"cardRadius":"10px"}}"#, "layout.cardRadius"),
            (r#"{"name":"X","colors":{},"layout":{"dividers":"yes"}}"#, "layout.dividers"),
            (r#"{"name":"X","colors":{},"layout":{"sidebarSide":"top"}}"#, "layout.sidebarSide"),
            (r#"{"name":"X","colors":{},"cards":{"shape":"polygon(0 0, 100% 0)"}}"#, "cards.shape"),
            (r#"{"name":"X","colors":{},"cards":{"aspect":"16:9"}}"#, "cards.aspect"),
            (r#"{"name":"X","colors":{},"cards":[]}"#, "cards"),
            (r#"{"name":"X","colors":{},"icons":{"corners":"wavy"}}"#, "icons.corners"),
            (r#"{"name":"X","colors":{},"effects":{"backgroundGradient":["red","blue"]}}"#, "effects.backgroundGradient"),
            (r##"{"name":"X","colors":{},"effects":{"backgroundGradient":["#000"]}}"##, "effects.backgroundGradient"),
            (r#"{"name":"X","colors":{},"effects":{"backgroundGradient":"linear-gradient(red, blue)"}}"#, "effects.backgroundGradient"),
            (r#"{"name":"X","colors":{},"typography":{"labelFont":"a; } * { display:none"}}"#, "typography.labelFont"),
            (r#"{"name":"X","colors":{},"shapes":{"panelShape":"polygon(0 0)"}}"#, "shapes.panelShape"),
            (r#"{"name":"X","colors":{},"detail":{"layout":"url(https://evil.example)"}}"#, "detail.layout"),
            (r#"{"name":"X","colors":{},"detail":{"backdrop":"https://evil.example/x.png"}}"#, "detail.backdrop"),
            (r#"{"name":"X","colors":{},"detail":{"backdropBlur":"18px"}}"#, "detail.backdropBlur"),
            (r#"{"name":"X","colors":{},"detail":{"tiles":"backdrop-filter: none"}}"#, "detail.tiles"),
            (r#"{"name":"X","colors":{},"detail":"bento"}"#, "detail"),
            (r#"{"name":"X","colors":{},"motion":{"level":"1s ease infinite"}}"#, "motion.level"),
            (r#"{"name":"X","colors":{},"motion":{"sheet":"@keyframes x"}}"#, "motion.sheet"),
            (r#"{"name":"X","colors":{},"motion":{"view":"spin"}}"#, "motion.view"),
            (r#"{"name":"X","colors":{},"motion":{"stagger":"yes"}}"#, "motion.stagger"),
            (r#"{"name":"X","colors":{},"cards":{"caption":"always"}}"#, "cards.caption"),
            (r#"{"kansoTheme":4,"name":"X","colors":{}}"#, "kansoTheme"),
            (r#"["not", "a", "theme"]"#, "Tema non valido"),
            ("not json", "Tema non valido"),
        ];
        for (raw, expected) in bad {
            let err = Theme::from_json(raw).expect_err(raw);
            assert!(err.contains(expected), "{raw} -> {err}");
        }
    }

    #[test]
    fn a_theme_has_a_second_color_and_says_how_the_side_panel_uses_its_colors() {
        // Kanso's accent kept: Kanso's second color too, and its usual use of both
        let plain = Theme::from_json(r#"{"name":"X","colors":{}}"#).unwrap();
        assert_eq!(plain.colors.accent_secondary, ThemeColors::defaults(ColorScheme::Dark).accent_secondary);
        assert_eq!((plain.layout.nav_headings, plain.layout.nav_pages), (NavColor::Accent, NavColor::Secondary));
        let light = Theme::from_json(r#"{"name":"X","colorScheme":"light","colors":{}}"#).unwrap();
        assert_eq!(light.colors.accent_secondary, ThemeColors::defaults(ColorScheme::Light).accent_secondary);

        // Its own accent and no second color: one drawn from the accent, as strong as it is
        let nord = Theme::from_json(NORD).unwrap();
        assert_eq!(nord.colors.accent, "#88ccdd");
        assert_eq!(nord.colors.accent_secondary, second_accent("#88ccdd"));
        for (accent, second) in [("#ff0000", "#00ffbf"), ("#ff4500", "#00faff"), ("#00ffff", "#ff0040"), ("#50fb7b", "#fb50fb")] {
            assert_eq!(second_accent(accent), second, "{accent}");
        }
        // A grey has no hue to turn; transparency is not carried over
        assert_eq!(second_accent("#1e1e1e"), "#1e1e1e");
        assert_eq!(second_accent("#808080cc"), "#808080");

        // Its own second color, and its own use of the colors
        let own = Theme::from_json(
            r##"{"name":"X","colors":{"accent":"#f00","accentSecondary":"#0AF"},"layout":{"navHeadings":"secondary","navPages":"muted"}}"##,
        )
        .unwrap();
        assert_eq!(own.colors.accent_secondary, "#00aaff");
        assert_eq!((own.layout.nav_headings, own.layout.nav_pages), (NavColor::Secondary, NavColor::Muted));
        let exported = own.to_json();
        assert!(exported.contains("\"accentSecondary\": \"#00aaff\"") && exported.contains("\"navHeadings\": \"secondary\""));
        assert_eq!(Theme::from_json(&exported).unwrap().layout, own.layout);

        // The sheet of a game: its titles take two of the theme's colors, its cards scroll
        assert_eq!((plain.detail.cards, plain.detail.key_titles, plain.detail.card_titles), (DetailCards::Rail, NavColor::Accent, NavColor::Secondary));
        let sheet = Theme::from_json(r##"{"name":"X","colors":{},"detail":{"cards":"stack","keyTitles":"text","cardTitles":"accent"}}"##).unwrap();
        assert_eq!((sheet.detail.cards, sheet.detail.key_titles, sheet.detail.card_titles), (DetailCards::Stack, NavColor::Text, NavColor::Accent));
        assert_eq!(Theme::from_json(&sheet.to_json()).unwrap().detail, sheet.detail);

        // Names from the list and colors only: anything else is refused, never passed on
        for (raw, expected) in [
            (r##"{"name":"X","colors":{"accentSecondary":"red"}}"##, "colors.accentSecondary"),
            (r##"{"name":"X","colors":{"accentSecondary":"var(--x)"}}"##, "colors.accentSecondary"),
            (r##"{"name":"X","colors":{},"layout":{"navHeadings":"#ff0000"}}"##, "layout.navHeadings"),
            (r##"{"name":"X","colors":{},"layout":{"navPages":"url(x)"}}"##, "layout.navPages"),
            (r##"{"name":"X","colors":{},"detail":{"cards":"marquee"}}"##, "detail.cards"),
            (r##"{"name":"X","colors":{},"detail":{"keyTitles":"red"}}"##, "detail.keyTitles"),
            (r##"{"name":"X","colors":{},"detail":{"cardTitles":"#fff"}}"##, "detail.cardTitles"),
        ] {
            let err = Theme::from_json(raw).expect_err(raw);
            assert!(err.contains(expected), "{raw} -> {err}");
        }
    }

    #[test]
    fn a_theme_arranges_the_home_page() {
        let plain = Theme::from_json(r#"{"name":"X","colors":{}}"#).unwrap();
        assert_eq!(plain.home, ThemeHome::default());
        assert_eq!(plain.home.sections.len(), 6, "every part, in Kanso's order");
        assert_eq!(plain.home.sections.last(), Some(&HomeSection::Plugins), "what the plugins add comes last");

        let own = Theme::from_json(
            r##"{"kansoTheme":3,"name":"X","colors":{},"home":{"sections":["deals","resume","deals","mostPlayed"],"effects":"none"}}"##,
        )
        .unwrap();
        assert_eq!(own.home.sections, [HomeSection::Deals, HomeSection::Resume, HomeSection::MostPlayed], "its order, each part once; the rest is left out");
        assert_eq!(own.home.effects, HomeEffects::Off);
        assert_eq!(Theme::from_json(&own.to_json()).unwrap().home, own.home);
        let bare = Theme::from_json(r##"{"kansoTheme":3,"name":"X","colors":{},"home":{"sections":[]}}"##).unwrap();
        assert!(bare.home.sections.is_empty(), "a theme may leave Home empty");

        for (raw, expected) in [
            (r##"{"name":"X","colors":{},"home":{"sections":["resume","<script>"]}}"##, "home.sections"),
            (r##"{"name":"X","colors":{},"home":{"sections":"resume"}}"##, "home.sections"),
            (r##"{"name":"X","colors":{},"home":{"effects":"url(x.gif)"}}"##, "home.effects"),
        ] {
            let err = Theme::from_json(raw).expect_err(raw);
            assert!(err.contains(expected), "{raw} -> {err}");
        }
    }

    #[test]
    fn older_files_get_the_part_of_home_for_plugins_and_format_3_is_taken_as_written() {
        let with = |marker: &str, sections: &str| {
            Theme::from_json(&format!(r##"{{{marker}"name":"X","colors":{{}},"home":{{"sections":{sections}}}}}"##))
                .unwrap()
                .home
                .sections
        };
        // Written before plugins had a place on Home: that place is added at the end
        for marker in ["", r#""kansoTheme":1,"#, r#""kansoTheme":2,"#] {
            assert_eq!(
                with(marker, r#"["deals","resume"]"#),
                [HomeSection::Deals, HomeSection::Resume, HomeSection::Plugins],
                "{marker}"
            );
            assert_eq!(with(marker, "[]"), [HomeSection::Plugins], "{marker}");
            // The name is known to every version: a file that has it keeps it where it is
            assert_eq!(with(marker, r#"["plugins","deals"]"#), [HomeSection::Plugins, HomeSection::Deals], "{marker}");
        }
        // Format 3: a part not named is not shown, this one included
        let marker = r#""kansoTheme":3,"#;
        assert_eq!(with(marker, r#"["deals","resume"]"#), [HomeSection::Deals, HomeSection::Resume]);
        assert!(with(marker, "[]").is_empty());
        assert_eq!(with(marker, r#"["plugins","deals","plugins"]"#), [HomeSection::Plugins, HomeSection::Deals]);

        // A list nobody wrote is Kanso's own, whatever the version
        for raw in [r#"{"name":"X","colors":{}}"#, r#"{"kansoTheme":2,"name":"X","colors":{},"home":{"effects":"none"}}"#] {
            assert_eq!(Theme::from_json(raw).unwrap().home.sections, ThemeHome::default().sections, "{raw}");
        }

        // A theme installed by an earlier Kanso (a format 2 file with everything spelled out)
        // comes back with the new part, is saved as format 3 and then stays as it is
        let installed = r##"{"kansoTheme":2,"name":"X","colors":{},"home":{"sections":["resume","upNext","mostPlayed","recommendations","deals"],"effects":"seasonal"}}"##;
        let read = Theme::from_json(installed).unwrap();
        assert_eq!(read.home, ThemeHome::default());
        let saved = read.to_json();
        assert!(saved.contains("\"kansoTheme\": 3") && saved.contains("\"plugins\""));
        assert_eq!(Theme::from_json(&saved).unwrap(), read);
        // ...and one whose author left a part out keeps it out through a save
        let trimmed = Theme::from_json(r##"{"kansoTheme":3,"name":"X","colors":{},"home":{"sections":["resume"]}}"##).unwrap();
        assert_eq!(trimmed.validated().unwrap().home.sections, [HomeSection::Resume]);
    }

    #[test]
    fn a_theme_arranges_the_side_panel() {
        let plain = Theme::from_json(r#"{"name":"X","colors":{}}"#).unwrap();
        assert_eq!(plain.nav, ThemeNav::default());
        assert_eq!(
            plain.nav.pages,
            [NavPage::Home, NavPage::Store, NavPage::News, NavPage::Stats, NavPage::Emulators],
            "every page, in Kanso's order"
        );
        assert_eq!(
            plain.nav.groups,
            [NavGroup::Library, NavGroup::Status, NavGroup::Platform, NavGroup::Genres, NavGroup::Tags]
        );
        assert_eq!(plain.nav.pages_position, PagesPosition::Top);
        // A file of an earlier format says nothing of the panel: it keeps Kanso's
        assert_eq!(Theme::from_json(NORD).unwrap().nav, ThemeNav::default());

        let own = Theme::from_json(
            r##"{"kansoTheme":3,"name":"X","colors":{},"nav":{"pages":["stats","home","stats"],"groups":["tags","library","tags","platform"],"pagesPosition":"bottom"}}"##,
        )
        .unwrap();
        assert_eq!(own.nav.pages, [NavPage::Stats, NavPage::Home], "its order, each page once; the rest is not offered");
        assert_eq!(own.nav.groups, [NavGroup::Tags, NavGroup::Library, NavGroup::Platform], "the library where the theme puts it");
        assert_eq!(own.nav.pages_position, PagesPosition::Bottom);
        let exported = own.to_json();
        assert!(exported.contains("\"nav\"") && exported.contains("\"pagesPosition\": \"bottom\""));
        assert_eq!(Theme::from_json(&exported).unwrap().nav, own.nav);

        // A theme may offer no page at all, but it cannot take the library away
        let bare = Theme::from_json(r##"{"name":"X","colors":{},"nav":{"pages":[],"groups":[]}}"##).unwrap();
        assert!(bare.nav.pages.is_empty());
        assert_eq!(bare.nav.groups, [NavGroup::Library]);
        let without = Theme::from_json(r##"{"name":"X","colors":{},"nav":{"groups":["genres","status"]}}"##).unwrap();
        assert_eq!(without.nav.groups, [NavGroup::Library, NavGroup::Genres, NavGroup::Status], "left out: put first");
        assert_eq!(Theme::from_json(&without.to_json()).unwrap().nav, without.nav);
        // The same holds for a theme that reaches the backend from the interface
        let mut from_ui = plain.clone();
        from_ui.nav.groups = vec![NavGroup::Tags];
        assert_eq!(from_ui.validated().unwrap().nav.groups, [NavGroup::Library, NavGroup::Tags]);

        for (raw, expected) in [
            (r##"{"name":"X","colors":{},"nav":{"pages":["home","library"]}}"##, "nav.pages"),
            (r##"{"name":"X","colors":{},"nav":{"pages":["home","<script>"]}}"##, "nav.pages"),
            (r##"{"name":"X","colors":{},"nav":{"pages":"home"}}"##, "nav.pages"),
            (r##"{"name":"X","colors":{},"nav":{"pages":[1]}}"##, "nav.pages"),
            (r##"{"name":"X","colors":{},"nav":{"groups":["library","home"]}}"##, "nav.groups"),
            (r##"{"name":"X","colors":{},"nav":{"groups":["Library"]}}"##, "nav.groups"),
            (r##"{"name":"X","colors":{},"nav":{"groups":{"library":true}}}"##, "nav.groups"),
            (r##"{"name":"X","colors":{},"nav":{"pagesPosition":"left"}}"##, "nav.pagesPosition"),
            (r##"{"name":"X","colors":{},"nav":{"pagesPosition":true}}"##, "nav.pagesPosition"),
            (r##"{"name":"X","colors":{},"nav":["home"]}"##, "nav"),
        ] {
            let err = Theme::from_json(raw).expect_err(raw);
            assert!(err.contains(expected), "{raw} -> {err}");
        }
    }

    #[test]
    fn a_theme_arranges_the_store_page() {
        let plain = Theme::from_json(r#"{"name":"X","colors":{}}"#).unwrap();
        assert_eq!(plain.store, ThemeStore::default());
        assert_eq!(
            plain.store.tabs,
            [StoreTab::All, StoreTab::Picks, StoreTab::Steam, StoreTab::Epic, StoreTab::Gog, StoreTab::Others, StoreTab::Liked],
            "every tab, in Kanso's order"
        );
        assert_eq!(plain.store.layout, StoreLayout::Grid);
        assert!(plain.store.figures);
        assert_eq!(Theme::from_json(NORD).unwrap().store, ThemeStore::default());

        let own = Theme::from_json(
            r##"{"kansoTheme":3,"name":"X","colors":{},"store":{"tabs":["liked","gog","liked","all"],"layout":"list","figures":false}}"##,
        )
        .unwrap();
        assert_eq!(own.store.tabs, [StoreTab::Liked, StoreTab::Gog, StoreTab::All], "its order, each tab once; the first is where the page opens");
        assert_eq!(own.store.layout, StoreLayout::List);
        assert!(!own.store.figures);
        let exported = own.to_json();
        assert!(exported.contains("\"store\"") && exported.contains("\"layout\": \"list\""));
        assert_eq!(Theme::from_json(&exported).unwrap().store, own.store);

        for (raw, expected) in [
            // A page with no tab has nothing to open on
            (r##"{"name":"X","colors":{},"store":{"tabs":[]}}"##, "store.tabs"),
            (r##"{"name":"X","colors":{},"store":{"tabs":["all","amazon"]}}"##, "store.tabs"),
            (r##"{"name":"X","colors":{},"store":{"tabs":["https://evil.example/deals"]}}"##, "store.tabs"),
            (r##"{"name":"X","colors":{},"store":{"tabs":"all"}}"##, "store.tabs"),
            (r##"{"name":"X","colors":{},"store":{"tabs":[["all"]]}}"##, "store.tabs"),
            (r##"{"name":"X","colors":{},"store":{"layout":"table"}}"##, "store.layout"),
            (r##"{"name":"X","colors":{},"store":{"layout":"display: none"}}"##, "store.layout"),
            (r##"{"name":"X","colors":{},"store":{"figures":"no"}}"##, "store.figures"),
            (r##"{"name":"X","colors":{},"store":{"figures":0}}"##, "store.figures"),
            (r##"{"name":"X","colors":{},"store":"list"}"##, "store"),
        ] {
            let err = Theme::from_json(raw).expect_err(raw);
            assert!(err.contains(expected), "{raw} -> {err}");
        }
        // An empty list is refused from the interface too, not quietly replaced
        let mut from_ui = plain.clone();
        from_ui.store.tabs.clear();
        assert!(from_ui.validated().unwrap_err().contains("store.tabs"));
    }

    #[test]
    fn a_theme_arranges_the_news_page() {
        let plain = Theme::from_json(r#"{"name":"X","colors":{}}"#).unwrap();
        assert_eq!(plain.news, ThemeNews::default());
        assert_eq!(
            (plain.news.layout, plain.news.pictures, plain.news.summaries),
            (NewsLayout::Magazine, true, true)
        );
        assert_eq!(Theme::from_json(NORD).unwrap().news, ThemeNews::default());

        let own = Theme::from_json(
            r##"{"kansoTheme":3,"name":"X","colors":{},"news":{"layout":"list","pictures":false,"summaries":false}}"##,
        )
        .unwrap();
        assert_eq!(own.news, ThemeNews { layout: NewsLayout::List, pictures: false, summaries: false });
        let exported = own.to_json();
        assert!(exported.contains("\"news\"") && exported.contains("\"summaries\": false"));
        assert_eq!(Theme::from_json(&exported).unwrap().news, own.news);
        let grid = Theme::from_json(r##"{"name":"X","colors":{},"news":{"layout":"grid"}}"##).unwrap();
        assert_eq!(grid.news, ThemeNews { layout: NewsLayout::Grid, pictures: true, summaries: true }, "what is not said keeps its default");

        for (raw, expected) in [
            (r##"{"name":"X","colors":{},"news":{"layout":"masonry"}}"##, "news.layout"),
            (r##"{"name":"X","colors":{},"news":{"layout":["list"]}}"##, "news.layout"),
            (r##"{"name":"X","colors":{},"news":{"pictures":"https://evil.example/x.png"}}"##, "news.pictures"),
            (r##"{"name":"X","colors":{},"news":{"pictures":1}}"##, "news.pictures"),
            (r##"{"name":"X","colors":{},"news":{"summaries":"yes"}}"##, "news.summaries"),
            (r##"{"name":"X","colors":{},"news":true}"##, "news"),
        ] {
            let err = Theme::from_json(raw).expect_err(raw);
            assert!(err.contains(expected), "{raw} -> {err}");
        }
    }

    #[test]
    fn exported_file_round_trips_and_carries_no_id() {
        let mut theme = Theme::from_json(NORD).unwrap();
        theme.id = "nord-night".to_string();
        theme.cards.shape = CardShape::Slant;
        theme.icons.stroke_width = 1.5;
        theme.effects.background_gradient = vec!["#000000".to_string(), "#112233".to_string()];
        let json = theme.to_json();
        assert!(json.contains("\"kansoTheme\": 3"));
        assert!(!json.contains("\"id\""));
        // Every section is spelled out: an exported theme is a complete template
        for section in ["typography", "layout", "cards", "shapes", "icons", "effects", "detail", "motion", "home", "nav", "store", "news", "fontFiles"] {
            assert!(json.contains(&format!("\"{section}\"")), "{section} missing");
        }
        assert!(json.contains("\"aspect\": \"2:3\"") && json.contains("\"shadow\": \"none\""));

        let mut reparsed = Theme::from_json(&json).unwrap();
        assert_eq!(reparsed.id, "");
        reparsed.id = theme.id.clone();
        assert_eq!(reparsed, theme);

        // A theme object as an older interface would send it (no new sections) is still valid
        let from_ui: Theme = serde_json::from_str(
            r##"{"id":"x","name":"X","colorScheme":"dark","colors":{"background":"#000000","surface":"#000000","elevated":"#000000","border":"#000000","borderStrong":"#000000","text":"#ffffff","textSecondary":"#ffffff","textMuted":"#ffffff","textFaint":"#ffffff","accent":"#ff0000","accentHover":"#ff0000","onAccent":"#000000"},"layout":{"cardMinWidth":200,"cardRadius":4,"sidebarWidth":200},"fontFamily":null}"##,
        )
        .expect("deserializes");
        let validated = from_ui.validated().unwrap();
        assert_eq!(validated.colors.accent_secondary, "#00ffbf", "drawn from its accent");
        assert_eq!(validated.layout.card_min_width, 200);
        assert_eq!(validated.layout.grid_gap, 16);
        assert_eq!(validated.cards, ThemeCards::default());
        assert_eq!((&validated.nav, &validated.store, &validated.news), (&ThemeNav::default(), &ThemeStore::default(), &ThemeNews::default()));
        assert_eq!(validated.home, ThemeHome::default());
    }

    #[test]
    fn embeds_fonts_given_as_data_or_as_a_file_next_to_the_theme() {
        let tmp = tempdir().unwrap();
        fs::write(tmp.path().join("Pixel.woff2"), FAKE_WOFF2).unwrap();
        let encoded = STANDARD.encode(FAKE_WOFF2);

        let raw = format!(
            r#"{{"name":"Fonts","colors":{{}},"fontFamily":"Pixel, Mono Two","fontFiles":[
                {{"family":"Pixel","file":"Pixel.woff2"}},
                {{"family":"Mono Two","weight":"100  900","style":"italic","data":"data:font/woff2;base64,{encoded}"}},
                {{"family":"Three","weight":700,"data":"{}"}}
            ]}}"#,
            encoded.trim_end_matches('=')
        );
        let theme = Theme::from_json_in(&raw, tmp.path()).expect("valid theme");
        assert_eq!(theme.font_files.len(), 3);
        assert_eq!(theme.font_files[0].family, "Pixel");
        assert_eq!(theme.font_files[0].weight, "400");
        assert_eq!(theme.font_files[0].style, FontStyle::Normal);
        assert_eq!(theme.font_files[0].data, encoded, "the file was read and embedded");
        assert_eq!(theme.font_files[1].weight, "100 900");
        assert_eq!(theme.font_files[1].style, FontStyle::Italic);
        assert_eq!(theme.font_files[1].data, encoded);
        assert_eq!(theme.font_files[2].weight, "700");
        assert_eq!(theme.font_files[2].data, encoded, "missing padding is tolerated");

        // The exported file needs no companion files any more
        assert_eq!(Theme::from_json(&theme.to_json()).unwrap().font_files, theme.font_files);

        // Importing embeds too
        let source = tmp.path().join("fonts.kanso-theme.json");
        fs::write(&source, &raw).unwrap();
        let themes_dir = tmp.path().join("themes");
        let imported = import_theme_file(&themes_dir, &source).expect("import");
        assert_eq!(imported.font_files[0].data, encoded);
        let (listed, warnings) = list_user_themes(&themes_dir);
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(listed[0].font_files.len(), 3);
    }

    #[test]
    fn rejects_font_entries_that_are_not_fonts_or_reach_outside_the_theme_folder() {
        let tmp = tempdir().unwrap();
        let themes = tmp.path().join("themes");
        fs::create_dir_all(&themes).unwrap();
        fs::write(tmp.path().join("secret.ttf"), FAKE_WOFF2).unwrap();
        fs::write(themes.join("notes.woff2"), b"just some text, not a font").unwrap();
        fs::write(themes.join("ok.woff2"), FAKE_WOFF2).unwrap();
        fs::write(themes.join("config.json"), FAKE_WOFF2).unwrap();
        let not_a_font = STANDARD.encode(b"<script>alert(1)</script>");
        let font = STANDARD.encode(FAKE_WOFF2);

        let theme_with = |entry: String| format!(r#"{{"name":"X","colors":{{}},"fontFiles":[{entry}]}}"#);
        let bad = [
            (r#"{"family":"A","file":"../secret.ttf"}"#.to_string(), "fontFiles[0].file"),
            (format!(r#"{{"family":"A","file":{:?}}}"#, tmp.path().join("secret.ttf")), "fontFiles[0].file"),
            (r#"{"family":"A","file":"config.json"}"#.to_string(), "fontFiles[0].file"),
            (r#"{"family":"A","file":"missing.woff2"}"#.to_string(), "fontFiles[0].file"),
            (r#"{"family":"A","file":"notes.woff2"}"#.to_string(), "fontFiles[0]"),
            (format!(r#"{{"family":"A","data":"{not_a_font}"}}"#), "fontFiles[0]"),
            (r#"{"family":"A","data":"***"}"#.to_string(), "fontFiles[0].data"),
            (r#"{"family":"A"}"#.to_string(), "fontFiles[0].data"),
            (format!(r#"{{"family":"A; }} body {{","data":"{font}"}}"#), "fontFiles[0].family"),
            (format!(r#"{{"data":"{font}"}}"#), "fontFiles[0].family"),
            (format!(r#"{{"family":"A","weight":"bold","data":"{font}"}}"#), "fontFiles[0].weight"),
            (format!(r#"{{"family":"A","style":"oblique 20deg","data":"{font}"}}"#), "fontFiles[0].style"),
            (r#""ok.woff2""#.to_string(), "fontFiles[0]"),
        ];
        for (entry, expected) in bad {
            let raw = theme_with(entry);
            let err = Theme::from_json_in(&raw, &themes).expect_err(&raw);
            assert!(err.contains(expected), "{raw} -> {err}");
        }

        // The good file works from its folder, and only there: a theme arriving from the UI
        // cannot name files at all
        let good = theme_with(r#"{"family":"A","file":"ok.woff2"}"#.to_string());
        assert!(Theme::from_json_in(&good, &themes).is_ok());
        assert!(Theme::from_json(&good).unwrap_err().contains("fontFiles[0].file"));

        let too_many = theme_with(vec![format!(r#"{{"family":"A","data":"{font}"}}"#); 7].join(","));
        assert!(Theme::from_json(&too_many).unwrap_err().contains("fontFiles (7"));

        let oversized = STANDARD.encode(vec![0u8; MAX_FONT_BYTES + 4096]);
        let too_big = theme_with(format!(r#"{{"family":"A","data":"{oversized}"}}"#));
        assert!(Theme::from_json(&too_big).unwrap_err().contains("fontFiles[0]"));
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
    fn the_example_themes_in_the_docs_are_valid() {
        let nord = Theme::from_json(include_str!("../../../../docs/themes/nord.kanso-theme.json"))
            .expect("docs/themes/nord.kanso-theme.json must stay importable");
        assert_eq!(nord.name, "Nord");
        assert_eq!(nord.colors.accent, "#88c0d0");
        assert_eq!(nord.font_family, None);
        // A format 1 file: the side panel and the pages keep Kanso's arrangement
        assert_eq!((&nord.nav, &nord.store, &nord.news), (&ThemeNav::default(), &ThemeStore::default(), &ThemeNews::default()));
        assert_eq!(nord.home, ThemeHome::default());

        let arcade = Theme::from_json(include_str!("../../../../docs/themes/arcade.kanso-theme.json"))
            .expect("docs/themes/arcade.kanso-theme.json must stay importable");
        assert_eq!(arcade.cards.shape, CardShape::Cut);
        assert_eq!(arcade.icons.corners, IconCorners::Sharp);
        // Format 3: the emulators right after Home, the Store and the News as lists
        assert_eq!(arcade.nav.pages, [NavPage::Home, NavPage::Emulators, NavPage::Store, NavPage::News, NavPage::Stats]);
        assert_eq!((arcade.store.layout, arcade.store.figures, arcade.news.layout), (StoreLayout::List, false, NewsLayout::List));

        let aurora = Theme::from_json(include_str!("../../../../docs/themes/aurora.kanso-theme.json"))
            .expect("docs/themes/aurora.kanso-theme.json must stay importable");
        assert_eq!(aurora.cards.caption, CardCaption::Hover);
        assert_eq!(aurora.effects.background_gradient.len(), 3);
        // Format 3: the pages at the bottom, the Store opening on the picks, the News as a grid
        assert_eq!(aurora.nav.pages_position, PagesPosition::Bottom);
        assert_eq!(aurora.nav.groups[..3], [NavGroup::Library, NavGroup::Genres, NavGroup::Tags]);
        assert_eq!((aurora.store.tabs[0], aurora.news.layout), (StoreTab::Picks, NewsLayout::Grid));

        // The two that ship inside the app are written as the app itself would export them:
        // every name they use is known, and reading them back changes nothing
        for theme in [&arcade, &aurora] {
            assert!(theme.to_json().contains("\"kansoTheme\": 3"));
            assert_eq!(theme.home, ThemeHome::default(), "{}", theme.name);
            assert_eq!(&Theme::from_json(&theme.to_json()).unwrap(), theme, "{}", theme.name);
        }
    }

    #[test]
    fn the_full_example_in_the_theme_guide_is_valid_and_shows_the_default_arrangement() {
        // The guide's first block of JSON is the template people copy: every name in it must
        // be one this version accepts
        // On a Windows checkout the guide's lines may end in CR LF
        let guide = include_str!("../../../../docs/THEMES.md").replace("\r\n", "\n");
        let start = guide.find("```json\n").expect("the guide has a full example") + "```json\n".len();
        let example = &guide[start..start + guide[start..].find("\n```").expect("the example ends")];
        let theme = Theme::from_json(example).expect("the full example in docs/THEMES.md must stay importable");

        assert!(example.contains(&format!("\"kansoTheme\": {THEME_FORMAT_VERSION}")), "the example is of the current format");
        // The guide says the values it shows are Kanso's: for what a theme arranges, they are
        assert_eq!(theme.home, ThemeHome::default());
        assert_eq!(theme.nav, ThemeNav::default());
        assert_eq!(theme.store, ThemeStore::default());
        assert_eq!(theme.news, ThemeNews::default());
        assert_eq!(theme.motion, ThemeMotion::default());
        assert_eq!(theme.detail, ThemeDetail::default());
        // Every section the app writes is in the example, so that none is left undocumented
        let exported: Value = serde_json::from_str(&theme.to_json()).unwrap();
        let shown: Value = serde_json::from_str(example).unwrap();
        for (section, written) in exported.as_object().unwrap() {
            let documented = shown.get(section).unwrap_or_else(|| panic!("{section} is missing from the example"));
            if let (Some(written), Some(documented)) = (written.as_object(), documented.as_object()) {
                for key in written.keys() {
                    assert!(documented.contains_key(key), "{section}.{key} is missing from the example");
                }
            }
        }
        assert_eq!(Theme::from_json(&theme.to_json()).unwrap(), theme);
    }
}
