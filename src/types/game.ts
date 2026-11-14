export type GamePlatform = "steam" | "epic" | "standalone";

export interface DetectedGame {
  id: string;
  title: string;
  cleanTitle: string;
  platform: GamePlatform;
  platformAppId: string | null;
  installDir: string;
  executablePath: string | null;
  launchUri: string | null;
}

export interface ScanReport {
  games: DetectedGame[];
  steamCount: number;
  epicCount: number;
  standaloneCount: number;
  warnings: string[];
}

export interface AppConfig {
  steamgriddbApiKey: string;
  customFolders: string[];
  minimizeToTrayOnLaunch: boolean;
}
