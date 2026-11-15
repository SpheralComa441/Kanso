export type GamePlatform =
  | "steam"
  | "epic"
  | "gog"
  | "ea"
  | "ubisoft"
  | "xbox"
  | "Battle.net"
  | "standalone";

/** Where the user stands with a game; set by hand in the game's sheet. */
export type GameStatus = "backlog" | "playing" | "finished" | "abandoned";

export interface DetectedGame {
  id: string;
  title: string;
  cleanTitle: string;
  platform: GamePlatform;
  platformAppId: string | null;
  installDir: string;
  executablePath: string | null;
  launchUri: string | null;
  steamgriddbId: number | null;
  coverPath: string | null;
  shortDescription: string | null;
  genres: string[];
  isFavorite: boolean;
  metadataSynced: boolean;
  customOverride: boolean;
  /** Title is a local guess (Steam manifest had `appid_<id>`); sync will replace it. */
  titleProvisional: boolean;
  /** Steam store classification (`game`, `music`, `dlc`, ...), `null` until looked up. */
  appType: string | null;
  /** Shown only in the "Nascosti" view. */
  isHidden: boolean;
  /** Last automatic metadata attempt (ms since epoch), `0` if never. */
  metadataAttemptedAt: number;
  /** Last time played (ms since epoch): a launch from Kanso or what Steam recorded. */
  lastPlayedAt: number | null;
  /** Launches from Kanso. */
  playCount: number;
  /** Language the cached description and genres were fetched in. */
  metadataLang: string | null;
  /** Steam community tags ("Souls-like", "Co-op online", ...), most relevant first. */
  tags: string[];
  tagsLang: string | null;
  /**
   * Total playtime in minutes: recorded by the game's client (Steam) or measured by Kanso
   * while the game runs (other stores); `null` if unknown.
   */
  playtimeMinutes: number | null;
  status: GameStatus | null;
  /** Achievements unlocked / available according to Steam; `null` when unknown or none. */
  achievementsUnlocked: number | null;
  achievementsTotal: number | null;
  /** Steam store page matched to a game from another store (empty when there is none). */
  steamStoreAppid: string | null;
  /** `false` for a game in the library that is not on disk (uninstalled, or owned only). */
  isInstalled: boolean;
  updatedAt: number;
}

/** A stretch of play time, the unit of the statistics page. */
export interface PlaySession {
  gameId: string;
  startedAt: number;
  seconds: number;
  /** `kanso` = measured by Kanso, `steam` = growth of Steam's counter between two scans. */
  source: string;
}

export interface Achievement {
  id: string;
  name: string;
  description: string | null;
  iconUrl: string | null;
  unlocked: boolean;
  unlockedAt: number | null;
  /** Share of all players who have it, 0–100. */
  globalPercent: number | null;
  /** How far along a counted achievement is, 0–1. */
  progress: number | null;
}

/** What Steam's own cache says about a game's achievements. `items` may be shorter than `total`. */
export interface GameAchievements {
  total: number;
  unlocked: number;
  items: Achievement[];
}

export interface SgdbGameMatch {
  id: number;
  name: string;
  releaseYear: number | null;
  verified: boolean;
  types: string[];
}

export interface GameManualUpdate {
  id: string;
  title: string;
  executablePath: string | null;
  shortDescription: string | null;
  genres: string[];
  isFavorite?: boolean;
}

export interface ScanReport {
  games: DetectedGame[];
  warnings: string[];
}

/** Preferences that apply on top of any theme (Settings → Accessibility). */
export interface AccessibilityConfig {
  /** Size of text and controls, in percent of the theme's own size. */
  textScale: number;
  highContrast: boolean;
  reduceMotion: boolean;
  /** A font chosen for legibility instead of the theme's fonts. */
  readableFont: "atkinson" | "opendyslexic" | null;
  alwaysShowScrollbars: boolean;
  strongFocus: boolean;
}

export interface AppConfig {
  steamgriddbApiKey: string;
  customFolders: string[];
  minimizeToTrayOnLaunch: boolean;
  /** `it`, `en`, `es` or `ru`; `null` until chosen or detected on first run. */
  language: string | null;
  /** Selected theme (`builtin:<name>` or an installed theme's id); `null` = default. */
  themeId: string | null;
  accessibility: AccessibilityConfig;
  /** Whether Kanso measures play time for games whose store does not expose it. */
  trackPlaytime: boolean;
  /** Per-plugin choices, by plugin id; a plugin without an entry uses its own default. */
  plugins: Record<string, PluginState>;
}

export interface PluginState {
  enabled: boolean;
  /** Whether the plugin's short label is shown on the game cards. */
  showBadge: boolean;
  settings: Record<string, string>;
}

export interface PluginSettingInfo {
  key: string;
  kind: "text" | "secret" | "toggle" | "select";
  label: string;
  help: string;
  default: string;
  options: { value: string; label: string }[];
}

/** A plugin as listed by the backend, texts already in the interface language. */
export interface PluginInfo {
  id: string;
  name: string;
  version: string;
  author: string | null;
  description: string;
  homepage: string | null;
  /** The only hosts the plugin may contact. */
  hosts: string[];
  /** Shipped with Kanso: can be switched off but not removed. */
  bundled: boolean;
  enabled: boolean;
  showBadge: boolean;
  gameDetails: boolean;
  badge: boolean;
  background: boolean;
  commands: { id: string; title: string; description: string }[];
  settings: PluginSettingInfo[];
}

/** A plugin read from disk before installing it. */
export interface PluginPreview {
  plugin: PluginInfo;
  replacesVersion: string | null;
}

/** What plugins may show: plain data, drawn by Kanso's own components. */
export type PluginBlock =
  | { type: "stats"; items: { label: string; value: string; hint?: string }[] }
  | { type: "text"; text: string }
  | { type: "list"; items: { title: string; subtitle?: string; url?: string }[] }
  | { type: "links"; items: { label: string; url: string }[] }
  | { type: "meter"; label: string; value: number; max: number; valueLabel?: string }
  | { type: "notice"; text: string; tone: "info" | "warning" };

export interface PluginSection {
  title?: string;
  blocks: PluginBlock[];
}

export interface PluginGameData {
  badge?: { text: string; tooltip?: string };
  sections: PluginSection[];
}

export interface GameDataAnswer {
  pluginId: string;
  pluginName: string;
  data: PluginGameData | null;
  error: string | null;
  /** The plugin's last log lines when it failed, for its author. */
  logs: string[];
  fetchedAt: number;
}

export interface BadgeEntry {
  pluginId: string;
  pluginName: string;
  text: string;
  tooltip: string | null;
}

/** What a plugin command shows in its dialog. */
export interface PluginView {
  title: string | null;
  sections: PluginSection[];
  games: { id: string; note: string | null }[];
}

export type DealStore = "steam" | "epic" | "gog";

export interface Deal {
  id: string;
  store: DealStore;
  title: string;
  url: string;
  imageUrl: string | null;
  originalPrice: number | null;
  finalPrice: number;
  currency: string;
  discountPercent: number;
  /** Not started yet (next week's free game on Epic). */
  upcoming: boolean;
  startsAt: number | null;
  endsAt: number | null;
  steamAppid: string | null;
}

export interface DealsReport {
  deals: Deal[];
  failedStores: DealStore[];
  fetchedAt: number;
}

export interface ThemeColors {
  background: string;
  surface: string;
  elevated: string;
  border: string;
  borderStrong: string;
  text: string;
  textSecondary: string;
  textMuted: string;
  textFaint: string;
  accent: string;
  accentHover: string;
  onAccent: string;
}

export interface ThemeLayout {
  cardMinWidth: number;
  cardRadius: number;
  sidebarWidth: number;
  gridGap: number;
  headerHeight: number;
  contentPadding: number;
  sidebarSide: "left" | "right";
  dividers: boolean;
  scrollbars: "auto" | "always" | "hidden";
  windowControls: "bars" | "dots";
}

export interface ThemeTypography {
  /** Font stack of titles; `null` uses the body font. */
  headingFont: string | null;
  /** `null` keeps the weight each title has by design. */
  headingWeight: number | null;
  headingUppercase: boolean;
  /** Letter spacing of titles, in em. */
  headingSpacing: number;
  /** Size of the whole interface relative to the default. */
  scale: number;
}

export interface ThemeCards {
  shape: "rounded" | "cut" | "slant";
  aspect: "2:3" | "3:4" | "4:5" | "1:1";
  borderWidth: number;
  shadow: "none" | "soft" | "strong" | "glow";
  hover: "lift" | "zoom" | "glow" | "none";
  caption: "below" | "overlay" | "none";
}

export interface ThemeShapes {
  controlRadius: number;
  panelRadius: number;
  chipRadius: number;
}

export interface ThemeIcons {
  strokeWidth: number;
  corners: "round" | "sharp";
  scale: number;
  fill: number;
}

export interface ThemeEffects {
  /** Two or three colors; empty for a plain background. */
  backgroundGradient: string[];
  backgroundAngle: number;
}

/** A font file shipped inside a theme, base64-encoded. */
export interface ThemeFontFile {
  family: string;
  weight: string;
  style: "normal" | "italic";
  data: string;
}

/**
 * A theme as validated by the backend (see `docs/THEMES.md`). The in-app editor changes only
 * colors and fonts; the other sections come from imported theme files.
 */
export interface Theme {
  id: string;
  name: string;
  author: string;
  description: string;
  colorScheme: "dark" | "light";
  colors: ThemeColors;
  fontFamily: string | null;
  typography: ThemeTypography;
  layout: ThemeLayout;
  cards: ThemeCards;
  shapes: ThemeShapes;
  icons: ThemeIcons;
  effects: ThemeEffects;
  fontFiles: ThemeFontFile[];
}

/** A store's account, as far as the interface may know it: never a token. */
export interface StoreAccountInfo {
  platform: GamePlatform;
  /**
   * How the user signs in: in the browser, ending on a code to paste ("code"), or on the
   * store's own page shown in a window of Kanso ("window"). Either way the password is typed
   * on the store's page and never reaches Kanso.
   */
  method: "code" | "window";
  /** Someone signed in (even if the session has since been refused). */
  connected: boolean;
  displayName: string | null;
  /** The store no longer accepts the saved session: sign in again. */
  needsSignIn: boolean;
  /**
   * What the last reading of the account's library found, in words: how many games, how many
   * through a subscription, how many left out and why. `null` until it has been read.
   */
  library: string | null;
}

/** How a sign-in on the store's page, in a window of Kanso, ended. */
export interface WindowSignIn {
  /** `false`: the user closed the window before signing in. */
  signedIn: boolean;
  account: StoreAccountInfo;
  /** Signed in, but the first reading of the library did not go through: why. */
  note: string | null;
}
