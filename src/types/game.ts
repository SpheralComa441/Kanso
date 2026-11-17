export type GamePlatform =
  | "steam"
  | "epic"
  | "gog"
  | "ea"
  | "ubisoft"
  | "xbox"
  | "Battle.net"
  | "standalone"
  /** A game file run by one of the user's emulators, named by `platformAppId`. */
  | "emulator";

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
  /** Genres the user gave the game, besides the ones its store lists. */
  customGenres: string[];
  /** The version installed, as the client or the files say it; `null` when nothing says. */
  version: string | null;
  /** A Steam game on this PC that belongs to someone else's account (family sharing). */
  isFamilyShared: boolean;
  updatedAt: number;
}

/** A part of the Home page. */
export type HomeSection = "resume" | "upNext" | "mostPlayed" | "recommendations" | "deals";

/** A place the user named for the weather on the Home page, kept coarse (about a kilometre). */
export interface WeatherPlace {
  name: string;
  latitude: number;
  longitude: number;
}

/** A place found for a name, with what tells it from its namesakes. */
export interface PlaceMatch {
  name: string;
  /** Region and country, as one line. */
  region: string;
  latitude: number;
  longitude: number;
}

/** How the user wants the Home page. */
export interface HomeConfig {
  /** Parts left out. */
  hidden: HomeSection[];
  /** The moving background of the season and of the holidays. */
  effects: boolean;
  /** Let the background follow the weather at `place` (asks Open-Meteo; off until chosen). */
  weather: boolean;
  place: WeatherPlace | null;
  /** The games lined up to play next, in order: at most five ids. */
  upNext: string[];
}

/** A part of the side panel the user can leave out. */
export type HideableSection = "home" | "store" | "news" | "tags";

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

/** What a news source writes about. */
export type NewsTopic = "gaming" | "hardware";

/** A feed the user added by hand. */
export interface CustomFeed {
  name: string;
  /** The feed's address, https. */
  url: string;
  topic: NewsTopic;
}

/** One piece of news, from a publisher's public feed. */
export interface NewsItem {
  id: string;
  source: string;
  sourceName: string;
  topic: NewsTopic;
  title: string;
  /** The article, on the source's own site. */
  url: string;
  /** The first words of the article; empty when the feed gives none. */
  summary: string;
  imageUrl: string | null;
  publishedAt: number | null;
}

export interface NewsReport {
  /** Newest first. */
  items: NewsItem[];
  /** Sources that could not be read; the others are still listed. */
  failedSources: string[];
  fetchedAt: number;
}

/** A site whose feed Kanso can read. */
export interface NewsSourceInfo {
  id: string;
  name: string;
  lang: string;
  topic: NewsTopic;
  siteUrl: string;
}

/** One of the three parts of NVIDIA DLSS, each a library of its own in the game's folder. */
export type DlssPart = "superResolution" | "frameGeneration" | "rayReconstruction";

/** A part of DLSS found among a game's files, at one version. */
export interface DlssLibrary {
  part: DlssPart;
  /** As the file declares it: "3.7.10", "310.2.1". */
  version: string;
  /** How many copies at this version the game carries. */
  copies: number;
  /** Kanso replaced a copy at this version and keeps the game's original beside it. */
  replaced: boolean;
}

/** A game that carries DLSS, with what it carries. */
export interface GameDlss {
  gameId: string;
  title: string;
  libraries: DlssLibrary[];
}

/** DLSS across the library. */
export interface DlssStatus {
  /** Whether the user lets Kanso ask NVIDIA's repository. */
  online: boolean;
  /** NVIDIA's newest version, when known: "310.9.16". */
  latest: string | null;
  checkedAt: number | null;
  /** The parts of the newest version already downloaded on this PC. */
  downloaded: DlssPart[];
  games: GameDlss[];
  /** Why the newest version could not be asked, when it could not. */
  error: string | null;
}

/** What changing a game's libraries did. */
export interface DlssChange {
  /** How many files were replaced, or put back. */
  changed: number;
  libraries: DlssLibrary[];
}

/** Where a newer library comes from. */
export type DlssSource = { kind: "nvidia" } | { kind: "game"; gameId: string } | { kind: "file"; path: string };

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
  /** How much the interface moves: as the theme says, not at all, or a level of the user's. */
  animations: AnimationsChoice;
  /** What a cover tells about its game under the pointer, besides the title. */
  cardInfo: CardInfoItem[];
  /** Parts of the side panel the user hid; with Home hidden, Kanso opens on the library. */
  hiddenSections: HideableSection[];
  home: HomeConfig;
  store: StoreConfig;
  /** The news sources the user chose, by id; `null` until they choose (then: the language's own). */
  newsSources: string[] | null;
  /** Feeds the user added by hand, read along with the sources chosen. */
  newsFeeds: CustomFeed[];
  /**
   * Whether Kanso may ask NVIDIA's public repository for the newest DLSS and download its
   * libraries from there. Off until the user turns it on.
   */
  dlssUpdates: boolean;
  /** Emulators added by the user, and folders added to the ones Kanso found. */
  emulators: EmulatorSetting[];
  /**
   * The user says they have Xbox Game Pass for PC linked to Battle.net: the games it makes
   * installable there are listed. Kanso cannot find this out by itself.
   */
  hasGamePass: boolean;
  /** List the games of the Game Pass catalog for PC (Microsoft's public catalog). */
  gamePassCatalog: boolean;
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
  badge?: { text: string; label?: string; tooltip?: string };
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
  /** What the figure is ("To finish"), as the plugin says. */
  label: string | null;
  tooltip: string | null;
}

/** What a plugin command shows in its dialog. */
export interface PluginView {
  title: string | null;
  sections: PluginSection[];
  games: { id: string; note: string | null; hint: string | null }[];
  /** Rows, whole covers, or whole covers face down until the user turns each one. */
  gamesStyle: "list" | "cards" | "mystery";
}

/** A store Kanso asks for its deals; "others" is every other shop, as CheapShark lists them. */
export type DealStore = "steam" | "epic" | "gog" | "others";

export interface Deal {
  id: string;
  store: DealStore;
  /** The shop selling it, among the "others": "Humble Store", "Fanatical"… */
  seller: string | null;
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
  /** The game's genres where the store says them; empty when it does not. */
  genres: string[];
}

/** A deal the user liked: enough to show it, open it and say when it is about to end. */
export interface LikedDeal {
  id: string;
  store: DealStore;
  title: string;
  url: string;
  endsAt: number | null;
}

/** How the user wants the Store page. */
export interface StoreConfig {
  /** Stores left out. */
  hiddenStores: DealStore[];
  /** Deals the user does not want to see, by id. */
  hiddenDeals: string[];
  liked: LikedDeal[];
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
  /** A second color apart from the accent: by default, the pages beside the library. */
  accentSecondary: string;
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
  /** Color of the headings of the side panel. */
  navHeadings: NavColor;
  /** Color of the pages of the side panel that are not the library. */
  navPages: NavColor;
}

/** One of the theme's own colors, by name: its accent, its second color, titles, hints. */
export type NavColor = "accent" | "secondary" | "text" | "muted";

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
  /** Font stack of the small labels over figures and sections; `null` uses the body font. */
  labelFont: string | null;
  /** Small labels in capitals, letter-spaced. */
  labelUppercase: boolean;
}

export interface ThemeCards {
  shape: "rounded" | "cut" | "slant";
  aspect: "2:3" | "3:4" | "4:5" | "1:1";
  borderWidth: number;
  shadow: "none" | "soft" | "strong" | "glow";
  hover: "lift" | "zoom" | "glow" | "none";
  /**
   * Title and genres under the cover; over its bottom; or only under the pointer (the cover
   * blurs and the game's title, genres and figures show over it); or never.
   */
  caption: "below" | "overlay" | "hover" | "none";
}

export interface ThemeShapes {
  controlRadius: number;
  panelRadius: number;
  chipRadius: number;
  /** Corners of panels and tiles: rounded by `panelRadius`, or cut diagonally by it. */
  panelShape: "rounded" | "cut";
}

/** The Home page as a theme arranges it. */
export interface ThemeHome {
  /** The parts shown, in order; a part not named is not shown. */
  sections: HomeSection[];
  /** What moves behind the top of the page: of the season, or nothing. */
  effects: "seasonal" | "none";
}

/** How much the interface moves, and how. */
export interface ThemeMotion {
  level: MotionLevel;
  /** How a sheet or a dialog comes in. */
  sheet: "rise" | "zoom" | "slide" | "fade";
  /** How the content changes when another entry of the side panel is chosen. */
  view: "rise" | "slide" | "fade";
  /** Cards come in one after the other when a view opens. */
  stagger: boolean;
}

export type MotionLevel = "none" | "subtle" | "smooth" | "lively";

/** An emulator on this PC: one Kanso found, or one the user added in Settings. */
export interface EmulatorInfo {
  /** `dolphin`, `retroarch`…, or `custom-…` for one Kanso does not know. */
  id: string;
  name: string;
  /** What it emulates: "GameCube · Wii". Empty for an emulator Kanso does not know. */
  systems: string;
  exePath: string;
  /** Where its games are read from: the folders its own settings list, and the user's. */
  folders: { path: string; fromEmulator: boolean }[];
  /** Added in Settings rather than found by Kanso. */
  addedByUser: boolean;
  /** The version its executable carries, when it carries one. */
  version: string | null;
}

/** What Settings keeps about an emulator (`emulators` in `config.json`). */
export interface EmulatorSetting {
  id: string;
  /** Name of an emulator Kanso does not know. */
  name: string;
  /** Empty for a known emulator that Kanso found by itself. */
  exePath: string;
  /** Folders with its games, besides those its own settings list. */
  folders: string[];
  /** File extensions of its games; a known emulator has its own list. */
  extensions: string[];
  /** Arguments that start a game, `{rom}` being the game file; empty for the usual ones. */
  args: string;
}

/** What Kanso recognises an executable as, when the user picks one in Settings. */
export interface EmulatorIdentity {
  id: string;
  name: string;
  systems: string;
  /** Extensions of the game files it opens. */
  extensions: string[];
  /** The arguments Kanso starts a game with. */
  args: string;
}

/** One thing a cover can tell about its game under the pointer. */
export type CardInfoItem =
  | "genres"
  | "tags"
  | "playtime"
  | "lastPlayed"
  | "status"
  | "achievements"
  | "badges";

/** The user's choice for the animations: what the theme says, off, or a level of their own. */
export type AnimationsChoice = "theme" | "off" | "subtle" | "smooth" | "lively";

/** The open sheet of a game. */
export interface ThemeDetail {
  /** Separate tiles, or one sheet with the cover on one side. */
  layout: "bento" | "sheet";
  /** Behind the sheet: the game's own artwork, blurred and veiled, or a plain veil. */
  backdrop: "art" | "dim";
  /** Blur of the artwork, in pixels. */
  backdropBlur: number;
  /** How much of the background color veils what is behind (0–1). */
  backdropDim: number;
  tiles: "filled" | "glass" | "outlined";
  /** Space between tiles, in pixels. */
  tileGap: number;
  /** The cards after the figures: in a box that moves on by itself, or one under the other. */
  cards: "rail" | "stack";
  /** Color of the titles of what the sheet shows first (figures, achievements). */
  keyTitles: NavColor;
  /** Color of the titles of the other cards, and of the edge of the ones kept in sight. */
  cardTitles: NavColor;
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
  detail: ThemeDetail;
  motion: ThemeMotion;
  home: ThemeHome;
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
  method: "code" | "window" | "browser";
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
