import arcadeTheme from "../../docs/themes/arcade.kanso-theme.json";
import auroraTheme from "../../docs/themes/aurora.kanso-theme.json";
import type {
  AccessibilityConfig,
  AnimationsChoice,
  HideableSection,
  MotionLevel,
  NavGroup,
  NavPage,
  Theme,
  ThemeCards,
  ThemeColors,
  ThemeDetail,
  ThemeEffects,
  ThemeHome,
  ThemeFontFile,
  NavColor,
  ThemeIcons,
  ThemeLayout,
  ThemeMotion,
  ThemeNav,
  ThemeNews,
  ThemeShapes,
  ThemeStore,
  ThemeTypography,
} from "../types/game";

/** The `kansoTheme` marker of the theme files this version writes (`kanso-core/src/theme.rs`). */
export const THEME_FORMAT_VERSION = 3;

/** Built-in theme ids carry a prefix no installed theme can have (their ids are plain slugs). */
const BUILTIN_PREFIX = "builtin:";

/* Defaults of every section: the same values as `kanso-core/src/theme.rs`. */
const DEFAULT_TYPOGRAPHY: ThemeTypography = {
  headingFont: null,
  headingWeight: null,
  headingUppercase: false,
  headingSpacing: 0,
  scale: 1,
  labelFont: null,
  labelUppercase: true,
};
const DEFAULT_LAYOUT: ThemeLayout = {
  cardMinWidth: 185,
  cardRadius: 10,
  sidebarWidth: 208,
  gridGap: 16,
  headerHeight: 52,
  contentPadding: 24,
  sidebarSide: "left",
  dividers: false,
  scrollbars: "auto",
  windowControls: "bars",
  navHeadings: "accent",
  navPages: "secondary",
};
const DEFAULT_CARDS: ThemeCards = {
  shape: "rounded",
  aspect: "2:3",
  borderWidth: 1,
  shadow: "none",
  hover: "lift",
  caption: "below",
};
const DEFAULT_SHAPES: ThemeShapes = {
  controlRadius: 8,
  panelRadius: 12,
  chipRadius: 6,
  panelShape: "rounded",
};
const DEFAULT_ICONS: ThemeIcons = { strokeWidth: 2, corners: "round", scale: 1, fill: 0 };
const DEFAULT_EFFECTS: ThemeEffects = { backgroundGradient: [], backgroundAngle: 180 };
const DEFAULT_DETAIL: ThemeDetail = {
  layout: "bento",
  backdrop: "art",
  backdropBlur: 18,
  backdropDim: 0.62,
  tiles: "filled",
  tileGap: 12,
  cards: "rail",
  keyTitles: "accent",
  cardTitles: "secondary",
};

const DEFAULT_MOTION: ThemeMotion = { level: "smooth", sheet: "rise", view: "rise", stagger: true };
export const DEFAULT_HOME: ThemeHome = {
  sections: ["resume", "upNext", "mostPlayed", "recommendations", "deals", "plugins"],
  effects: "seasonal",
};
export const DEFAULT_NAV: ThemeNav = {
  pages: ["home", "store", "news", "stats", "emulators"],
  groups: ["library", "status", "platform", "genres", "tags"],
  pagesPosition: "top",
};
export const DEFAULT_STORE: ThemeStore = {
  tabs: ["all", "picks", "steam", "epic", "gog", "others", "liked"],
  layout: "grid",
  figures: true,
};
export const DEFAULT_NEWS: ThemeNews = { layout: "magazine", pictures: true, summaries: true };

/** A theme whose optional sections may be missing or partial (theme files, older data). */
export type ThemeInput = Pick<Theme, "name" | "colors"> & {
  /** The format marker of a theme file; a theme the app already holds carries none. */
  kansoTheme?: number;
  id?: string;
  author?: string;
  description?: string;
  colorScheme?: string;
  fontFamily?: string | null;
  typography?: Partial<ThemeTypography>;
  layout?: Partial<Record<keyof ThemeLayout, unknown>>;
  cards?: Partial<Record<keyof ThemeCards, unknown>>;
  shapes?: Partial<Record<keyof ThemeShapes, unknown>>;
  icons?: Partial<Record<keyof ThemeIcons, unknown>>;
  effects?: Partial<ThemeEffects>;
  detail?: Partial<Record<keyof ThemeDetail, unknown>>;
  motion?: Partial<Record<keyof ThemeMotion, unknown>>;
  home?: Partial<Record<keyof ThemeHome, unknown>>;
  nav?: Partial<Record<keyof ThemeNav, unknown>>;
  store?: Partial<Record<keyof ThemeStore, unknown>>;
  news?: Partial<Record<keyof ThemeNews, unknown>>;
  fontFiles?: ThemeFontFile[];
};

/**
 * Fills in every section a theme does not spell out, and keeps the few promises the backend
 * makes about the lists for the themes it never reads (the built-in ones, the browser
 * preview): a file older than format 3 that lists the parts of Home gets the part for
 * plugins at the end, the library is always among the groups of the side panel, and the
 * Store page has at least one tab.
 */
export function withThemeDefaults(input: ThemeInput): Theme {
  const home = { ...DEFAULT_HOME, ...input.home } as ThemeHome;
  if (input.kansoTheme !== undefined && input.kansoTheme < 3 && input.home?.sections && !home.sections.includes("plugins")) {
    home.sections = [...home.sections, "plugins"];
  }
  const nav = { ...DEFAULT_NAV, ...input.nav } as ThemeNav;
  if (!nav.groups.includes("library")) nav.groups = ["library", ...nav.groups];
  const store = { ...DEFAULT_STORE, ...input.store } as ThemeStore;
  if (store.tabs.length === 0) store.tabs = DEFAULT_STORE.tabs;
  return {
    id: input.id ?? "",
    name: input.name,
    author: input.author ?? "",
    description: input.description ?? "",
    colorScheme: input.colorScheme === "light" ? "light" : "dark",
    colors: input.colors,
    fontFamily: input.fontFamily ?? null,
    typography: { ...DEFAULT_TYPOGRAPHY, ...input.typography },
    layout: { ...DEFAULT_LAYOUT, ...input.layout } as ThemeLayout,
    cards: { ...DEFAULT_CARDS, ...input.cards } as ThemeCards,
    shapes: { ...DEFAULT_SHAPES, ...input.shapes } as ThemeShapes,
    icons: { ...DEFAULT_ICONS, ...input.icons } as ThemeIcons,
    effects: { ...DEFAULT_EFFECTS, ...input.effects },
    detail: { ...DEFAULT_DETAIL, ...input.detail } as ThemeDetail,
    motion: { ...DEFAULT_MOTION, ...input.motion } as ThemeMotion,
    home,
    nav,
    store,
    news: { ...DEFAULT_NEWS, ...input.news } as ThemeNews,
    fontFiles: input.fontFiles ?? [],
  };
}

/*
 * Kanso's own look. The six themes below take their colors from the logo's own versions
 * (the same slate with one vivid color each) and share everything else: soft, rounded
 * shapes, covers without a caption that tell about the game under the pointer, and gentle
 * movement.
 */
const LOGO_STYLE = {
  fontFamily: "Manrope",
  typography: {
    headingFont: "Tektur",
    headingWeight: 600,
    headingSpacing: 0.01,
    labelFont: "JetBrains Mono",
    labelUppercase: true,
  },
  layout: { cardRadius: 16 },
  cards: { shape: "rounded", hover: "lift", caption: "hover", shadow: "soft" },
  shapes: { controlRadius: 10, panelRadius: 18, chipRadius: 8, panelShape: "rounded" },
  icons: { corners: "round" },
  motion: { level: "smooth", sheet: "rise", view: "rise", stagger: true },
} satisfies Partial<ThemeInput>;

/** The slate of the logo, as a ramp of surfaces and of the text read on them. */
const SLATE = {
  background: "#17181f",
  surface: "#1e2029",
  elevated: "#282a37",
  border: "#343747",
  borderStrong: "#5b5f78",
  text: "#f3f4f8",
  textSecondary: "#d5d7e1",
  textMuted: "#a4a8ba",
  textFaint: "#868aa1",
};

function slate(
  slug: string,
  name: string,
  description: string,
  accent: string,
  accentHover: string,
  onAccent: string,
  accentSecondary: string
): Theme {
  return withThemeDefaults({
    ...LOGO_STYLE,
    id: `${BUILTIN_PREFIX}${slug}`,
    name,
    author: "Kanso",
    description,
    colorScheme: "dark",
    colors: { ...SLATE, accent, accentHover, onAccent, accentSecondary },
  });
}

/**
 * Themes shipped with Kanso. The first six are the logo's own versions: the same slate with
 * one color each, and the logo in ink on paper. Arcade and Aurora are the example files of
 * `docs/themes/` and show how far an imported theme can reshape the interface.
 */
export const BUILTIN_THEMES: Theme[] = [
  slate("ember", "Kanso Brace", "Slate and ember orange: the logo's own colors.", "#ff4500", "#ff6a33", "#0d0d10", "#3ecbe0"),
  slate("amber", "Kanso Ambra", "Slate with an amber accent.", "#f6ba1b", "#ffcf4d", "#17140a", "#62b0ff"),
  slate("cyan", "Kanso Ciano", "Slate with a cyan accent.", "#00ffff", "#8affff", "#041a1c", "#ff8a5c"),
  // The logo's violet (#8a2be1) is too dark to read as text on slate: a lighter step of it
  slate("violet", "Kanso Viola", "Slate with a violet accent.", "#a968f5", "#bd8af8", "#120621", "#f6c443"),
  slate("mint", "Kanso Menta", "Slate with a mint green accent.", "#50fb7b", "#86fda3", "#06210f", "#b98cff"),
  withThemeDefaults({
    ...LOGO_STYLE,
    id: `${BUILTIN_PREFIX}ink`,
    name: "Kanso Inchiostro",
    author: "Kanso",
    description: "The logo in ink, on paper: light and monochrome.",
    colorScheme: "light",
    colors: {
      background: "#f5f5f2",
      surface: "#ffffff",
      elevated: "#eaeae6",
      border: "#d9d9d4",
      borderStrong: "#9d9d98",
      text: "#1e1e1e",
      textSecondary: "#3a3a3a",
      textMuted: "#595959",
      textFaint: "#6b6b6b",
      accent: "#1e1e1e",
      accentHover: "#000000",
      onAccent: "#ffffff",
      accentSecondary: "#c43c00",
    },
  }),
  withThemeDefaults({ ...arcadeTheme, id: `${BUILTIN_PREFIX}arcade` }),
  withThemeDefaults({ ...auroraTheme, id: `${BUILTIN_PREFIX}aurora` }),
];

export const DEFAULT_THEME = BUILTIN_THEMES[0];

export function isBuiltinTheme(theme: Theme): boolean {
  return theme.id.startsWith(BUILTIN_PREFIX);
}

/** The theme with `id` among built-in and installed ones, or the default. */
export function resolveTheme(id: string | null, userThemes: Theme[]): Theme {
  return (
    [...BUILTIN_THEMES, ...userThemes].find((theme) => theme.id === id) ??
    DEFAULT_THEME
  );
}

/**
 * The pages the side panel offers: the theme's, in its order, less the ones the user chose
 * to hide in Settings. A page that is not here has no entry and no link leading to it.
 *
 * `hasEmulation`: the user has emulators, or games that run in one. The Emulators page is
 * the only place that lists those games, so a theme that leaves it out gets it all the same,
 * after its own pages: like the library among the groups, it can be moved but not taken away.
 */
export function offeredPages(
  nav: ThemeNav,
  hidden: readonly HideableSection[],
  hasEmulation: boolean
): NavPage[] {
  const pages = nav.pages.filter((page) => !(hidden as readonly string[]).includes(page));
  return hasEmulation && !pages.includes("emulators") ? [...pages, "emulators"] : pages;
}

/**
 * The groups the side panel shows: the theme's, in its order, less the tags when the user
 * hid them. The library is always among them.
 */
export function offeredGroups(nav: ThemeNav, hidden: readonly HideableSection[]): NavGroup[] {
  const groups: NavGroup[] = nav.groups.includes("library") ? nav.groups : ["library", ...nav.groups];
  return groups.filter((group) => group !== "tags" || !hidden.includes("tags"));
}

/** Order and CSS variable of each theme color; also drives the editor. */
export const THEME_COLOR_TOKENS: [keyof ThemeColors, string][] = [
  ["background", "--bg-primary"],
  ["surface", "--bg-surface"],
  ["elevated", "--bg-elevated"],
  ["border", "--border-subtle"],
  ["borderStrong", "--border-strong"],
  ["text", "--text-primary"],
  ["textSecondary", "--text-secondary"],
  ["textMuted", "--text-muted"],
  ["textFaint", "--text-faint"],
  ["accent", "--accent-color"],
  ["accentHover", "--accent-hover"],
  ["onAccent", "--accent-contrast"],
  ["accentSecondary", "--accent-secondary"],
];

/** How light a "#rrggbb" color is, from 0 (black) to 1 (white), as contrast is reckoned. */
export function luminance(hex: string): number {
  const value = /^#([0-9a-f]{6})$/i.exec(hex.trim())?.[1];
  if (!value) return 1;
  const channel = (index: number) => {
    const part = parseInt(value.slice(index, index + 2), 16) / 255;
    return part <= 0.03928 ? part / 12.92 : ((part + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * channel(0) + 0.7152 * channel(2) + 0.0722 * channel(4);
}

/** The theme color each choice for the side panel stands for. */
const NAV_COLORS: Record<NavColor, string> = {
  accent: "var(--accent-color)",
  secondary: "var(--accent-secondary)",
  text: "var(--text-primary)",
  muted: "var(--text-faint)",
};

/* ------------------------------------------------------------------ fonts */

export interface BundledFont {
  family: string;
  /** Generic family to fall back on, also a hint of the font's character. */
  kind: "sans-serif" | "serif" | "monospace";
}

/**
 * Fonts shipped inside Kanso (`src/fonts.css`), so a theme using them looks the same on every
 * PC. All of them cover Latin, Latin Extended and Cyrillic.
 */
export const BUNDLED_FONTS: BundledFont[] = [
  { family: "Inter", kind: "sans-serif" },
  { family: "Manrope", kind: "sans-serif" },
  { family: "Montserrat", kind: "sans-serif" },
  { family: "Nunito", kind: "sans-serif" },
  { family: "Rubik", kind: "sans-serif" },
  { family: "Exo 2", kind: "sans-serif" },
  { family: "Tektur", kind: "sans-serif" },
  { family: "Oswald", kind: "sans-serif" },
  { family: "Unbounded", kind: "sans-serif" },
  { family: "Comfortaa", kind: "sans-serif" },
  { family: "Lora", kind: "serif" },
  { family: "Playfair Display", kind: "serif" },
  { family: "JetBrains Mono", kind: "monospace" },
  { family: "Pixelify Sans", kind: "sans-serif" },
  // Made for legibility; Latin scripts only
  { family: "Atkinson Hyperlegible", kind: "sans-serif" },
  { family: "OpenDyslexic", kind: "sans-serif" },
];

const GENERIC_FONT_FAMILIES = new Set([
  "serif",
  "sans-serif",
  "monospace",
  "cursive",
  "fantasy",
  "system-ui",
  "ui-serif",
  "ui-sans-serif",
  "ui-monospace",
  "ui-rounded",
]);

const SYSTEM_FONT_STACK =
  '"Segoe UI Variable", "Segoe UI", system-ui, -apple-system, sans-serif';

/** The font names of a theme's stack, without quotes: `"Exo 2", serif` -> `["Exo 2", "serif"]`. */
export function fontStackNames(stack: string | null): string[] {
  return (stack ?? "")
    .split(",")
    .map((name) => name.replace(/["']/g, "").trim())
    .filter(Boolean);
}

/**
 * A theme's font stack as CSS: every name quoted (so `Exo 2` or `Press Start 2P` are valid),
 * generic families left bare. Authors write plain names; quoting is Kanso's job.
 */
function cssFontStack(stack: string | null): string | null {
  const names = fontStackNames(stack).map((name) =>
    GENERIC_FONT_FAMILIES.has(name.toLowerCase()) ? name.toLowerCase() : `"${name}"`
  );
  return names.length > 0 ? names.join(", ") : null;
}

let loadedFontFiles: ThemeFontFile[] | null = null;
let loadedFontFaces: FontFace[] = [];

/** Registers the fonts embedded in the active theme, replacing those of the previous one. */
function syncEmbeddedFonts(files: ThemeFontFile[]): void {
  // Same list as last time (the editor re-applies its draft on every change): nothing to do
  if (files === loadedFontFiles || typeof FontFace === "undefined") return;
  for (const face of loadedFontFaces) document.fonts.delete(face);
  loadedFontFaces = [];
  loadedFontFiles = files;

  for (const file of files) {
    try {
      const bytes = Uint8Array.from(atob(file.data), (char) => char.charCodeAt(0));
      const face = new FontFace(file.family, bytes.buffer, {
        weight: file.weight,
        style: file.style,
      });
      document.fonts.add(face);
      loadedFontFaces.push(face);
      // A file the webview cannot read is skipped: the next font of the stack is used
      void face.load().catch(() => document.fonts.delete(face));
    } catch {
      // Same as above
    }
  }
}

/* ---------------------------------------------------------- accessibility */

export const DEFAULT_ACCESSIBILITY: AccessibilityConfig = {
  textScale: 100,
  highContrast: false,
  reduceMotion: false,
  readableFont: null,
  alwaysShowScrollbars: false,
  strongFocus: false,
};

export const TEXT_SCALE_OPTIONS = [90, 100, 115, 130, 150] as const;

/** Fonts offered in Settings → Accessibility, by the value stored in the configuration. */
export const READABLE_FONTS: Record<NonNullable<AccessibilityConfig["readableFont"]>, string> = {
  atkinson: "Atkinson Hyperlegible",
  opendyslexic: "OpenDyslexic",
};

/* -------------------------------------------------------------- animations */

/** What the user may choose for the animations, in the order Settings lists them. */
export const ANIMATION_CHOICES: AnimationsChoice[] = ["theme", "off", "subtle", "smooth", "lively"];

/** Reads the animations choice from stored data; anything odd means "as the theme says". */
export function withAnimationsChoice(input: unknown): AnimationsChoice {
  return ANIMATION_CHOICES.includes(input as AnimationsChoice) ? (input as AnimationsChoice) : "theme";
}

/**
 * Durations (ms), the distance things travel (px), how small a sheet starts, the pause
 * between one card and the next (ms) and the curve of the movement, for each level.
 */
const MOTION_LEVELS: Record<
  Exclude<MotionLevel, "none">,
  { fast: number; base: number; slow: number; distance: number; scale: number; stagger: number; ease: string }
> = {
  subtle: { fast: 90, base: 140, slow: 180, distance: 4, scale: 0.995, stagger: 0, ease: "cubic-bezier(0.2, 0.8, 0.2, 1)" },
  smooth: { fast: 130, base: 220, slow: 300, distance: 14, scale: 0.975, stagger: 18, ease: "cubic-bezier(0.2, 0.8, 0.2, 1)" },
  lively: { fast: 170, base: 320, slow: 460, distance: 30, scale: 0.93, stagger: 32, ease: "cubic-bezier(0.34, 1.4, 0.64, 1)" },
};

/** Reads accessibility preferences from stored data, tolerating missing or odd values. */
export function withAccessibilityDefaults(input: unknown): AccessibilityConfig {
  const raw = (input ?? {}) as Partial<Record<keyof AccessibilityConfig, unknown>>;
  const scale = Number(raw.textScale);
  return {
    textScale: Number.isFinite(scale) ? Math.min(160, Math.max(80, Math.round(scale))) : 100,
    highContrast: raw.highContrast === true,
    reduceMotion: raw.reduceMotion === true,
    readableFont:
      raw.readableFont === "atkinson" || raw.readableFont === "opendyslexic"
        ? raw.readableFont
        : null,
    alwaysShowScrollbars: raw.alwaysShowScrollbars === true,
    strongFocus: raw.strongFocus === true,
  };
}

/* ------------------------------------------------------------------ apply */

const CARD_SHADOWS: Record<ThemeCards["shadow"], string> = {
  none: "none",
  soft: "0 6px 18px rgb(0 0 0 / 0.28)",
  strong: "0 14px 34px rgb(0 0 0 / 0.5)",
  glow: "0 0 20px color-mix(in srgb, var(--accent-color) 32%, transparent)",
};

const GLOW_HOVER_SHADOW =
  "0 0 0 1px var(--accent-color), 0 0 24px color-mix(in srgb, var(--accent-color) 45%, transparent)";

/**
 * Applies a theme to the running app by setting the CSS variables of `src/theme.css` (and a
 * few `data-` attributes for the either/or choices) on the root element. Every value was
 * validated by the backend — hex colors, numbers in range, names from fixed lists, plain font
 * names — so nothing a theme file contains reaches the page as arbitrary CSS.
 *
 * The user's accessibility preferences are applied on top and win over the theme: text size,
 * contrast, legible font, motion, scrollbars. So does their choice for the animations: the
 * theme says how things move, the user may say how much, or not at all.
 */
export function applyTheme(
  theme: Theme,
  accessibility: AccessibilityConfig = DEFAULT_ACCESSIBILITY,
  animations: AnimationsChoice = "theme"
): void {
  const root = document.documentElement;
  const set = (name: string, value: string) => root.style.setProperty(name, value);
  const { typography, layout, cards, shapes, icons, effects, detail, motion } = theme;
  // On the dark chips drawn over covers the accent must still read: a theme whose accent is
  // dark (ink on paper) shows white there instead
  set("--accent-on-dark", luminance(theme.colors.accent) < 0.2 ? "#ffffff" : theme.colors.accent);
  const scale = typography.scale * (accessibility.textScale / 100);
  // Pixel sizes follow the interface scale, like everything sized in rem
  const px = (value: number) => `${Math.round(value * scale * 100) / 100}px`;

  // High contrast: every text and border color moves one step towards the strongest
  const colors: ThemeColors = accessibility.highContrast
    ? {
        ...theme.colors,
        textSecondary: theme.colors.text,
        textMuted: theme.colors.textSecondary,
        textFaint: theme.colors.textMuted,
        border: theme.colors.borderStrong,
        borderStrong: theme.colors.textMuted,
      }
    : theme.colors;
  for (const [key, cssVar] of THEME_COLOR_TOKENS) set(cssVar, colors[key]);
  set(
    "--app-background",
    effects.backgroundGradient.length >= 2
      ? `linear-gradient(${effects.backgroundAngle}deg, ${effects.backgroundGradient.join(", ")})`
      : "var(--bg-primary)"
  );
  root.dataset.scheme = theme.colorScheme;
  root.style.colorScheme = theme.colorScheme;

  // Typography
  syncEmbeddedFonts(theme.fontFiles);
  const readableFont = accessibility.readableFont
    ? READABLE_FONTS[accessibility.readableFont]
    : null;
  const bodyFont = readableFont ? `"${readableFont}"` : cssFontStack(theme.fontFamily);
  set("--font-sans", bodyFont ? `${bodyFont}, ${SYSTEM_FONT_STACK}` : SYSTEM_FONT_STACK);
  const headingFont = readableFont ? null : cssFontStack(typography.headingFont);
  set("--font-heading", headingFont ? `${headingFont}, var(--font-sans)` : "var(--font-sans)");
  set("--ui-scale", String(scale));
  const labelFont = readableFont ? null : cssFontStack(typography.labelFont);
  set("--font-label", labelFont ? `${labelFont}, var(--font-sans)` : "var(--font-sans)");
  root.toggleAttribute("data-label-uppercase", typography.labelUppercase);
  set("--heading-weight", String(typography.headingWeight ?? 600));
  set("--heading-spacing", `${typography.headingSpacing}em`);
  root.toggleAttribute("data-heading-weight", typography.headingWeight !== null);
  root.toggleAttribute("data-heading-uppercase", typography.headingUppercase);
  root.toggleAttribute("data-heading-spacing", typography.headingSpacing !== 0);

  // Layout
  set("--card-min-width", px(layout.cardMinWidth));
  set("--card-radius", px(layout.cardRadius));
  set("--sidebar-width", px(layout.sidebarWidth));
  set("--grid-gap", px(layout.gridGap));
  set("--header-height", px(layout.headerHeight));
  set("--content-padding", px(layout.contentPadding));
  set("--divider-width", layout.dividers ? "1px" : "0px");
  root.dataset.sidebarSide = layout.sidebarSide;
  root.dataset.scrollbars = accessibility.alwaysShowScrollbars ? "always" : layout.scrollbars;
  root.dataset.windowControls = layout.windowControls;
  // Names from a closed list, each standing for one of the theme's own colors
  set("--nav-heading-color", NAV_COLORS[layout.navHeadings] ?? NAV_COLORS.accent);
  set("--nav-page-color", NAV_COLORS[layout.navPages] ?? NAV_COLORS.secondary);

  // Cards
  const restShadow = CARD_SHADOWS[cards.shadow];
  set("--cover-aspect", cards.aspect.replace(":", " / "));
  set("--card-border-width", `${cards.borderWidth}px`);
  set("--card-shadow", restShadow);
  set("--card-hover-shadow", cards.hover === "glow" ? GLOW_HOVER_SHADOW : restShadow);
  set("--card-hover-lift", cards.hover === "lift" ? "-2px" : "0px");
  set(
    "--card-hover-zoom",
    cards.hover === "zoom" ? "1.08" : cards.hover === "none" ? "1" : "1.03"
  );
  set(
    "--card-hover-border",
    cards.hover === "glow"
      ? "var(--accent-color)"
      : cards.hover === "none"
        ? "var(--border-subtle)"
        : "color-mix(in srgb, var(--border-strong) 80%, transparent)"
  );
  root.dataset.cardShape = cards.shape;
  root.dataset.cardCaption = cards.caption;

  // Controls, panels, badges
  set("--radius-control", px(shapes.controlRadius));
  set("--radius-panel", px(shapes.panelRadius));
  set("--radius-chip", px(shapes.chipRadius));
  root.dataset.panelShape = shapes.panelShape;

  // The open sheet of a game
  root.dataset.detailBackdrop = detail.backdrop;
  root.dataset.detailTiles = detail.tiles;
  set("--detail-blur", `${detail.backdropBlur}px`);
  set("--detail-dim", `${Math.round(detail.backdropDim * 100)}%`);
  set("--detail-gap", px(detail.tileGap));
  set("--detail-key-title", NAV_COLORS[detail.keyTitles] ?? NAV_COLORS.accent);
  set("--detail-card-title", NAV_COLORS[detail.cardTitles] ?? NAV_COLORS.secondary);

  // Icons
  const sharp = icons.corners === "sharp";
  set("--icon-stroke", String(icons.strokeWidth));
  set("--icon-cap", sharp ? "square" : "round");
  set("--icon-join", sharp ? "miter" : "round");
  set("--icon-scale", String(icons.scale));
  set("--icon-fill", String(icons.fill));

  // Animations: the user's level, else the theme's; "reduce motion" (here or in the system)
  // switches everything off
  const osReducesMotion =
    typeof matchMedia === "function" && matchMedia("(prefers-reduced-motion: reduce)").matches;
  const chosen: MotionLevel =
    animations === "theme" ? motion.level : animations === "off" ? "none" : animations;
  const level: MotionLevel = accessibility.reduceMotion || osReducesMotion ? "none" : chosen;
  root.dataset.motion = level;
  root.dataset.motionSheet = motion.sheet;
  root.dataset.motionView = motion.view;
  const moves = level === "none" ? MOTION_LEVELS.subtle : MOTION_LEVELS[level];
  set("--motion-fast", `${moves.fast}ms`);
  set("--motion-base", `${moves.base}ms`);
  set("--motion-slow", `${moves.slow}ms`);
  set("--motion-distance", `${moves.distance}px`);
  set("--motion-scale", String(moves.scale));
  set("--motion-ease", moves.ease);
  set("--motion-stagger", `${motion.stagger ? moves.stagger : 0}ms`);
  root.toggleAttribute("data-reduce-motion", level === "none");
  root.toggleAttribute("data-strong-focus", accessibility.strongFocus);
}

/** `#rrggbb` for an `<input type="color">`, which cannot show alpha or short forms. */
export function toColorInputValue(color: string): string {
  const hex = color.trim().replace(/^#/, "");
  if (/^[0-9a-f]{3}$/i.test(hex)) {
    return `#${[...hex].map((c) => c + c).join("")}`.toLowerCase();
  }
  if (/^[0-9a-f]{6}([0-9a-f]{2})?$/i.test(hex)) {
    return `#${hex.slice(0, 6)}`.toLowerCase();
  }
  return "#000000";
}
