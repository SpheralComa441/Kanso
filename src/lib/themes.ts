import type { Theme, ThemeColors } from "../types/game";

/** Built-in theme ids carry a prefix no installed theme can have (their ids are plain slugs). */
const BUILTIN_PREFIX = "builtin:";

const DEFAULT_LAYOUT = { cardMinWidth: 185, cardRadius: 10, sidebarWidth: 208 };

function builtin(
  slug: string,
  name: string,
  description: string,
  colorScheme: Theme["colorScheme"],
  colors: ThemeColors,
  layout = DEFAULT_LAYOUT
): Theme {
  return {
    id: `${BUILTIN_PREFIX}${slug}`,
    name,
    author: "Kanso",
    description,
    colorScheme,
    colors,
    layout,
    fontFamily: null,
  };
}

/** Themes shipped with Kanso; they double as examples for theme authors. */
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

const DEFAULT_FONT_STACK =
  '"Inter", "Segoe UI Variable", "Segoe UI", system-ui, -apple-system, sans-serif';

/**
 * Applies a theme to the running app by setting the CSS variables of `src/theme.css` on the
 * root element. Values were validated by the backend (hex colors, clamped sizes, plain font
 * names), so nothing a theme file contains can reach the page as arbitrary CSS.
 */
export function applyTheme(theme: Theme): void {
  const root = document.documentElement;
  for (const [key, cssVar] of THEME_COLOR_TOKENS) {
    root.style.setProperty(cssVar, theme.colors[key]);
  }
  root.style.setProperty("--card-min-width", `${theme.layout.cardMinWidth}px`);
  root.style.setProperty("--card-radius", `${theme.layout.cardRadius}px`);
  root.style.setProperty("--sidebar-width", `${theme.layout.sidebarWidth}px`);
  root.style.setProperty(
    "--font-sans",
    theme.fontFamily ? `${theme.fontFamily}, ${DEFAULT_FONT_STACK}` : DEFAULT_FONT_STACK
  );
  root.dataset.scheme = theme.colorScheme;
  root.style.colorScheme = theme.colorScheme;
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
