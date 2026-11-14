import arcadeTheme from "../../docs/themes/arcade.kanso-theme.json";
import auroraTheme from "../../docs/themes/aurora.kanso-theme.json";
import type {
  AccessibilityConfig,
  Theme,
  ThemeCards,
  ThemeColors,
  ThemeEffects,
  ThemeFontFile,
  ThemeIcons,
  ThemeLayout,
  ThemeShapes,
  ThemeTypography,
} from "../types/game";

/** Built-in theme ids carry a prefix no installed theme can have (their ids are plain slugs). */
const BUILTIN_PREFIX = "builtin:";

/* Defaults of every section: the same values as `kanso-core/src/theme.rs`. */
const DEFAULT_TYPOGRAPHY: ThemeTypography = {
  headingFont: null,
  headingWeight: null,
  headingUppercase: false,
  headingSpacing: 0,
  scale: 1,
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
};
const DEFAULT_CARDS: ThemeCards = {
  shape: "rounded",
  aspect: "2:3",
  borderWidth: 1,
  shadow: "none",
  hover: "lift",
  caption: "below",
};
const DEFAULT_SHAPES: ThemeShapes = { controlRadius: 8, panelRadius: 12, chipRadius: 6 };
const DEFAULT_ICONS: ThemeIcons = { strokeWidth: 2, corners: "round", scale: 1, fill: 0 };
const DEFAULT_EFFECTS: ThemeEffects = { backgroundGradient: [], backgroundAngle: 180 };

/** A theme whose optional sections may be missing or partial (theme files, older data). */
export type ThemeInput = Pick<Theme, "name" | "colors"> & {
  id?: string;
  author?: string;
  description?: string;
  colorScheme?: string;
  fontFamily?: string | null;
  typography?: Partial<ThemeTypography>;
  layout?: Partial<Record<keyof ThemeLayout, unknown>>;
  cards?: Partial<Record<keyof ThemeCards, unknown>>;
  shapes?: Partial<ThemeShapes>;
  icons?: Partial<Record<keyof ThemeIcons, unknown>>;
  effects?: Partial<ThemeEffects>;
  fontFiles?: ThemeFontFile[];
};

/** Fills in every section a theme does not spell out. */
export function withThemeDefaults(input: ThemeInput): Theme {
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
    shapes: { ...DEFAULT_SHAPES, ...input.shapes },
    icons: { ...DEFAULT_ICONS, ...input.icons } as ThemeIcons,
    effects: { ...DEFAULT_EFFECTS, ...input.effects },
    fontFiles: input.fontFiles ?? [],
  };
}

function builtin(
  slug: string,
  name: string,
  description: string,
  colorScheme: Theme["colorScheme"],
  colors: ThemeColors
): Theme {
  return withThemeDefaults({
    id: `${BUILTIN_PREFIX}${slug}`,
    name,
    author: "Kanso",
    description,
    colorScheme,
    colors,
  });
}

/**
 * Themes shipped with Kanso. The first four change colors only; Arcade and Aurora are the
 * example files of `docs/themes/` and show what an imported theme can reshape.
 */
export const BUILTIN_THEMES: Theme[] = [
  builtin("oled", "Kanso OLED", "Deep black, amber accent.", "dark", {
    background: "#0f0f11",
    surface: "#16161a",
    elevated: "#1e1e24",
    border: "#272730",
    borderStrong: "#52525b",
    text: "#f4f4f5",
    textSecondary: "#d4d4d8",
    textMuted: "#a1a1aa",
    textFaint: "#71717a",
    accent: "#e5a93c",
    accentHover: "#f0bc59",
    onAccent: "#09090b",
  }),
  builtin("graphite", "Grafite", "Soft dark grey, teal accent.", "dark", {
    background: "#1b1d21",
    surface: "#23262b",
    elevated: "#2c3036",
    border: "#363a42",
    borderStrong: "#5a606b",
    text: "#eef0f3",
    textSecondary: "#cfd3da",
    textMuted: "#9aa1ad",
    textFaint: "#737a86",
    accent: "#4fd1c5",
    accentHover: "#76e0d6",
    onAccent: "#0c1f1d",
  }),
  builtin("nord", "Nord", "Arctic blue-grey palette.", "dark", {
    background: "#2e3440",
    surface: "#3b4252",
    elevated: "#434c5e",
    border: "#4c566a",
    borderStrong: "#6b7890",
    text: "#eceff4",
    textSecondary: "#d8dee9",
    textMuted: "#aab4c5",
    textFaint: "#8a94a8",
    accent: "#88c0d0",
    accentHover: "#a3d4e2",
    onAccent: "#2e3440",
  }),
  builtin("paper", "Carta", "Light theme, warm accent.", "light", {
    background: "#f4f4f5",
    surface: "#ffffff",
    elevated: "#e9e9ec",
    border: "#d9d9de",
    borderStrong: "#a1a1aa",
    text: "#18181b",
    textSecondary: "#3f3f46",
    textMuted: "#5b5b66",
    textFaint: "#7c7c87",
    accent: "#b7791f",
    accentHover: "#9c6615",
    onAccent: "#ffffff",
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
];

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
 * contrast, legible font, motion, scrollbars.
 */
export function applyTheme(
  theme: Theme,
  accessibility: AccessibilityConfig = DEFAULT_ACCESSIBILITY
): void {
  const root = document.documentElement;
  const set = (name: string, value: string) => root.style.setProperty(name, value);
  const { typography, layout, cards, shapes, icons, effects } = theme;
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

  // Icons
  const sharp = icons.corners === "sharp";
  set("--icon-stroke", String(icons.strokeWidth));
  set("--icon-cap", sharp ? "square" : "round");
  set("--icon-join", sharp ? "miter" : "round");
  set("--icon-scale", String(icons.scale));
  set("--icon-fill", String(icons.fill));

  // Remaining accessibility switches; the OS-wide "reduce motion" setting counts too
  const osReducesMotion =
    typeof matchMedia === "function" && matchMedia("(prefers-reduced-motion: reduce)").matches;
  root.toggleAttribute("data-reduce-motion", accessibility.reduceMotion || osReducesMotion);
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
