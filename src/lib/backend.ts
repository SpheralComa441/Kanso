import { invoke, isTauri } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import type { AppConfig, DetectedGame, ScanReport } from "../types/game";

const BROWSER_STORAGE_KEY = "kanso.browser_preview.config";

const DEFAULT_CONFIG: AppConfig = {
  steamgriddbApiKey: "",
  customFolders: [],
  minimizeToTrayOnLaunch: false,
};

const PREVIEW_MOCK_GAMES: DetectedGame[] = [
  {
    id: "steam:1091500",
    title: "Cyberpunk 2077",
    cleanTitle: "Cyberpunk 2077",
    platform: "steam",
    platformAppId: "1091500",
    installDir: "C:\\Program Files (x86)\\Steam\\steamapps\\common\\Cyberpunk 2077",
    executablePath: null,
    launchUri: "steam://rungameid/1091500",
  },
  {
    id: "steam:292030",
    title: "The Witcher® 3: Wild Hunt - Game of the Year Edition",
    cleanTitle: "The Witcher 3: Wild Hunt",
    platform: "steam",
    platformAppId: "292030",
    installDir: "D:\\SteamLibrary\\steamapps\\common\\The Witcher 3",
    executablePath: null,
    launchUri: "steam://rungameid/292030",
  },
  {
    id: "epic:dc9d2e595d0e4650b35d659f90d41059",
    title: "Alan Wake 2 Deluxe Edition",
    cleanTitle: "Alan Wake 2",
    platform: "epic",
    platformAppId: "dc9d2e595d0e4650b35d659f90d41059",
    installDir: "D:\\Epic Games\\AlanWake2",
    executablePath: "D:\\Epic Games\\AlanWake2\\Binaries\\Win64\\AlanWake2.exe",
    launchUri:
      "com.epicgames.launcher://apps/da155068a9034375839136b4697593c1%3A235b28b9c6e2469da792fbabbc2fd25e%3Adc9d2e595d0e4650b35d659f90d41059?action=launch&silent=true",
  },
  {
    id: "standalone:8f31c9a04d2e11b7",
    title: "Hollow Knight",
    cleanTitle: "Hollow Knight",
    platform: "standalone",
    platformAppId: null,
    installDir: "E:\\Games\\Hollow Knight",
    executablePath: "E:\\Games\\Hollow Knight\\hollow_knight.exe",
    launchUri: null,
  },
];

export function isRunningInTauri(): boolean {
  return isTauri();
}

export async function getSettings(): Promise<AppConfig> {
  if (isTauri()) {
    return invoke<AppConfig>("get_settings");
  }

  try {
    const raw = localStorage.getItem(BROWSER_STORAGE_KEY);
    if (!raw) return { ...DEFAULT_CONFIG };
    const parsed = JSON.parse(raw) as Partial<AppConfig>;
    return {
      steamgriddbApiKey: parsed.steamgriddbApiKey?.trim() ?? "",
      customFolders: Array.isArray(parsed.customFolders)
        ? parsed.customFolders
        : [],
      minimizeToTrayOnLaunch: Boolean(parsed.minimizeToTrayOnLaunch),
    };
  } catch {
    return { ...DEFAULT_CONFIG };
  }
}

export async function saveSettings(config: AppConfig): Promise<AppConfig> {
  const sanitized: AppConfig = {
    steamgriddbApiKey: config.steamgriddbApiKey.trim(),
    customFolders: Array.from(
      new Set(config.customFolders.map((f) => f.trim()).filter(Boolean))
    ),
    minimizeToTrayOnLaunch: config.minimizeToTrayOnLaunch,
  };

  if (isTauri()) {
    return invoke<AppConfig>("save_settings", { config: sanitized });
  }

  localStorage.setItem(BROWSER_STORAGE_KEY, JSON.stringify(sanitized));
  return sanitized;
}

export async function scanGames(): Promise<ScanReport> {
  if (isTauri()) {
    return invoke<ScanReport>("scan_games");
  }

  return {
    games: PREVIEW_MOCK_GAMES,
    steamCount: 2,
    epicCount: 1,
    standaloneCount: 1,
    warnings: [],
  };
}

export async function launchGame(game: DetectedGame): Promise<void> {
  if (isTauri()) {
    await invoke<void>("launch_game", { game });
    return;
  }

  throw new Error(
    `Avvio disponibile nell'applicazione desktop Windows (${game.title}).`
  );
}

export async function pickCustomFolder(): Promise<string | null> {
  if (!isTauri()) {
    return null;
  }

  const selected = await open({
    directory: true,
    multiple: false,
    title: "Seleziona cartella giochi",
  });

  if (typeof selected === "string" && selected.trim().length > 0) {
    return selected;
  }
  return null;
}
