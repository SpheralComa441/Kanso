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
}
