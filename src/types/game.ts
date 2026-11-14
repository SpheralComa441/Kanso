export type GamePlatform = "steam" | "epic" | "Battle.net" | "standalone";

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
  steamCount: number;
  epicCount: number;
  battlenetCount: number;
  standaloneCount: number;
  warnings: string[];
}

export interface AppConfig {
  steamgriddbApiKey: string;
  customFolders: string[];
  minimizeToTrayOnLaunch: boolean;
}
