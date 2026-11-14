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
}

export interface Theme {
  id: string;
  name: string;
  author: string;
  description: string;
  colorScheme: "dark" | "light";
  colors: ThemeColors;
  layout: ThemeLayout;
  fontFamily: string | null;
}
