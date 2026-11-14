import { convertFileSrc, invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import type {
  AppConfig,
  DetectedGame,
  GameManualUpdate,
  ScanReport,
  SgdbGameMatch,
} from "../types/game";
import { PREVIEW_MOCK_GAMES } from "./previewData";

const BROWSER_STORAGE_KEY = "kanso.browser_preview.config";

const DEFAULT_CONFIG: AppConfig = {
  steamgriddbApiKey: "",
  customFolders: [],
  minimizeToTrayOnLaunch: false,
};

// Same ordering the desktop backend applies to a real scan
let previewGamesState: DetectedGame[] = [...PREVIEW_MOCK_GAMES].sort((a, b) =>
  a.title.toLowerCase().localeCompare(b.title.toLowerCase())
);

export function isRunningInTauri(): boolean {
  return isTauri();
}

/**
 * Converts a local filesystem cover image path in AppData to a WebView-renderable URL,
 * appending `updatedAt` so replaced covers bust browser cache immediately.
 */
export function resolveCoverUrl(
  coverPath: string | null,
  updatedAt: number
): string | null {
  if (!coverPath) return null;
  if (
    coverPath.startsWith("http://") ||
    coverPath.startsWith("https://") ||
    coverPath.startsWith("data:")
  ) {
    return coverPath;
  }
  if (!isTauri()) {
    return null;
  }
  const assetUrl = convertFileSrc(coverPath);
  const sep = assetUrl.includes("?") ? "&" : "?";
  return `${assetUrl}${sep}v=${updatedAt || 1}`;
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

  return { games: [...previewGamesState], warnings: [] };
}

/**
 * Fills in missing covers, descriptions and names. Games still incomplete after an attempt are
 * retried once a day at most, unless `retryNow` (the user explicitly asked for a scan, or just
 * configured an API key).
 */
export async function syncLibraryMetadata(
  retryNow = false
): Promise<ScanReport> {
  if (isTauri()) {
    return invoke<ScanReport>("sync_library_metadata", { retryNow });
  }

  return scanGames();
}

export async function subscribeToGameUpdates(
  onUpdate: (game: DetectedGame) => void
): Promise<() => void> {
  if (!isTauri()) {
    return () => {};
  }
  const unlisten = await listen<DetectedGame>("library:game-updated", (ev) => {
    onUpdate(ev.payload);
  });
  return unlisten;
}

export async function searchSteamGridDbGames(
  query: string
): Promise<SgdbGameMatch[]> {
  if (isTauri()) {
    return invoke<SgdbGameMatch[]>("search_steamgriddb_games", { query });
  }

  const q = query.trim().toLowerCase();
  if (!q) return [];
  return [
    {
      id: 5276290,
      name: `${query.trim()}`,
      releaseYear: 2023,
      verified: true,
      types: ["steam"],
    },
    {
      id: 5276291,
      name: `${query.trim()} (Classic Edition)`,
      releaseYear: 2018,
      verified: false,
      types: ["custom"],
    },
  ];
}

export async function applySteamGridDbMatch(
  gameId: string,
  sgdbGameId: number,
  matchedName: string
): Promise<DetectedGame> {
  if (isTauri()) {
    return invoke<DetectedGame>("apply_steamgriddb_match", {
      gameId,
      sgdbGameId,
      matchedName,
    });
  }

  const idx = previewGamesState.findIndex((g) => g.id === gameId);
  if (idx === -1) {
    throw new Error("Gioco non trovato.");
  }
  const updated: DetectedGame = {
    ...previewGamesState[idx],
    title: matchedName,
    cleanTitle: matchedName,
    steamgriddbId: sgdbGameId,
    customOverride: true,
    titleProvisional: false,
    updatedAt: Date.now(),
  };
  previewGamesState[idx] = updated;
  return updated;
}

export async function updateGameDetails(
  payload: GameManualUpdate
): Promise<DetectedGame> {
  if (isTauri()) {
    return invoke<DetectedGame>("update_game_details", {
      payload,
    });
  }

  const idx = previewGamesState.findIndex((g) => g.id === payload.id);
  if (idx === -1) throw new Error("Gioco non trovato.");
  const updated: DetectedGame = {
    ...previewGamesState[idx],
    title: payload.title.trim() || previewGamesState[idx].title,
    executablePath: payload.executablePath,
    shortDescription: payload.shortDescription,
    genres: payload.genres,
    isFavorite: payload.isFavorite ?? previewGamesState[idx].isFavorite,
    customOverride: true,
    titleProvisional: false,
    updatedAt: Date.now(),
  };
  previewGamesState[idx] = updated;
  return updated;
}

export async function setGameLocalCover(
  gameId: string,
  imagePath: string
): Promise<DetectedGame> {
  if (isTauri()) {
    return invoke<DetectedGame>("set_game_local_cover", {
      gameId,
      imagePath,
    });
  }
  throw new Error("Selezione file locale disponibile nell'app Windows.");
}

export async function setGameFavorite(
  gameId: string,
  isFavorite: boolean
): Promise<DetectedGame> {
  if (isTauri()) {
    return invoke<DetectedGame>("set_game_favorite", {
      gameId,
      isFavorite,
    });
  }

  const idx = previewGamesState.findIndex((g) => g.id === gameId);
  if (idx === -1) throw new Error("Gioco non trovato.");
  const updated: DetectedGame = { ...previewGamesState[idx], isFavorite };
  previewGamesState[idx] = updated;
  return updated;
}

export async function setGameHidden(
  gameId: string,
  isHidden: boolean
): Promise<DetectedGame> {
  if (isTauri()) {
    return invoke<DetectedGame>("set_game_hidden", { gameId, isHidden });
  }

  const idx = previewGamesState.findIndex((g) => g.id === gameId);
  if (idx === -1) throw new Error("Gioco non trovato.");
  const updated: DetectedGame = { ...previewGamesState[idx], isHidden };
  previewGamesState[idx] = updated;
  return updated;
}

export async function launchGame(game: DetectedGame): Promise<void> {
  if (isTauri()) {
    // The backend launches what is stored for this id, not what the webview sends
    await invoke<void>("launch_game", { gameId: game.id });
    return;
  }

  throw new Error(
    `Avvio disponibile nell'applicazione desktop Windows (${game.title}).`
  );
}

export async function pickCustomFolder(): Promise<string | null> {
  if (!isTauri()) return null;

  const selected = await open({
    directory: true,
    multiple: false,
    title: "Seleziona cartella giochi",
  });

  return typeof selected === "string" && selected.trim().length > 0
    ? selected
    : null;
}

export async function pickExecutableFile(): Promise<string | null> {
  if (!isTauri()) return null;

  const selected = await open({
    directory: false,
    multiple: false,
    title: "Seleziona eseguibile del gioco (.exe)",
    filters: [{ name: "Eseguibile Windows", extensions: ["exe"] }],
  });

  return typeof selected === "string" && selected.trim().length > 0
    ? selected
    : null;
}

export async function pickCoverImageFile(): Promise<string | null> {
  if (!isTauri()) return null;

  const selected = await open({
    directory: false,
    multiple: false,
    title: "Seleziona immagine di copertina",
    filters: [
      {
        name: "Immagini",
        extensions: ["png", "jpg", "jpeg", "webp"],
      },
    ],
  });

  return typeof selected === "string" && selected.trim().length > 0
    ? selected
    : null;
}
