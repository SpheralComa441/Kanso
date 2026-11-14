export type GamePlatform =
  | "steam"
  | "epic"
  | "gog"
  | "ea"
  | "ubisoft"
  | "xbox"
  | "Battle.net"
  | "standalone";

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
  /** Last launch from Kanso (ms since epoch). */
  lastPlayedAt: number | null;
  playCount: number;
  /** Language the cached description and genres were fetched in. */
  metadataLang: string | null;
  updatedAt: number;
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

export interface AppConfig {
  steamgriddbApiKey: string;
  customFolders: string[];
  minimizeToTrayOnLaunch: boolean;
  /** `it`, `en`, `es` or `ru`; `null` until chosen or detected on first run. */
  language: string | null;
  /** Selected theme (`builtin:<name>` or an installed theme's id); `null` = default. */
  themeId: string | null;
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
