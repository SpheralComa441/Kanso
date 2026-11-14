import { convertFileSrc, invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import Database from "@tauri-apps/plugin-sql";
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

async function mirrorGamesToSqlite(games: DetectedGame[]): Promise<void> {
  if (!isTauri()) return;
  try {
    const db = await Database.load("sqlite:kanso.db");
    for (const g of games) {
      await db.execute(
        `INSERT INTO games (
          id, title, clean_title, platform, platform_app_id, install_dir,
          executable_path, launch_uri, cover_path, short_description, genres, is_favorite, updated_at
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, CURRENT_TIMESTAMP)
        ON CONFLICT(id) DO UPDATE SET
          title = excluded.title,
          clean_title = excluded.clean_title,
          platform = excluded.platform,
          executable_path = excluded.executable_path,
          cover_path = excluded.cover_path,
          short_description = excluded.short_description,
          genres = excluded.genres,
          is_favorite = excluded.is_favorite,
          updated_at = CURRENT_TIMESTAMP`,
        [
          g.id,
          g.title,
          g.cleanTitle,
          g.platform,
          g.platformAppId,
          g.installDir,
          g.executablePath,
          g.launchUri,
          g.coverPath,
          g.shortDescription,
          JSON.stringify(g.genres),
          g.isFavorite ? 1 : 0,
        ]
      );
    }
  } catch {
    // Non-fatal: primary JSON store in AppData remains intact
  }
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
    const saved = await invoke<AppConfig>("save_settings", {
      config: sanitized,
    });
    try {
      const db = await Database.load("sqlite:kanso.db");
      await db.execute(
        `INSERT INTO settings (key, value, updated_at)
         VALUES ('has_sgdb_key', $1, CURRENT_TIMESTAMP)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = CURRENT_TIMESTAMP`,
        [saved.steamgriddbApiKey ? "true" : "false"]
      );
    } catch {
      // Non-fatal
    }
    return saved;
  }

  localStorage.setItem(BROWSER_STORAGE_KEY, JSON.stringify(sanitized));
  return sanitized;
}

export async function scanGames(): Promise<ScanReport> {
  if (isTauri()) {
    const report = await invoke<ScanReport>("scan_games");
    void mirrorGamesToSqlite(report.games);
    return report;
  }

  return {
    games: [...previewGamesState],
    steamCount: previewGamesState.filter((g) => g.platform === "steam").length,
    epicCount: previewGamesState.filter((g) => g.platform === "epic").length,
    battlenetCount: previewGamesState.filter(
      (g) => g.platform === "Battle.net"
    ).length,
    standaloneCount: previewGamesState.filter(
      (g) => g.platform === "standalone"
    ).length,
    warnings: [],
  };
}

export async function syncLibraryMetadata(
  forceRefresh = false
): Promise<ScanReport> {
  if (isTauri()) {
    const report = await invoke<ScanReport>("sync_library_metadata", {
      forceRefresh,
    });
    void mirrorGamesToSqlite(report.games);
    return report;
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
    void mirrorGamesToSqlite([ev.payload]);
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
    const updated = await invoke<DetectedGame>("apply_steamgriddb_match", {
      gameId,
      sgdbGameId,
      matchedName,
    });
    void mirrorGamesToSqlite([updated]);
    return updated;
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
    const updated = await invoke<DetectedGame>("update_game_details", {
      payload,
    });
    void mirrorGamesToSqlite([updated]);
    return updated;
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
    const updated = await invoke<DetectedGame>("set_game_local_cover", {
      gameId,
      imagePath,
    });
    void mirrorGamesToSqlite([updated]);
    return updated;
  }
  throw new Error("Selezione file locale disponibile nell'app Windows.");
}

export async function setGameFavorite(
  gameId: string,
  isFavorite: boolean
): Promise<DetectedGame> {
  if (isTauri()) {
    const updated = await invoke<DetectedGame>("set_game_favorite", {
      gameId,
      isFavorite,
    });
    void mirrorGamesToSqlite([updated]);
    return updated;
  }

  const idx = previewGamesState.findIndex((g) => g.id === gameId);
  if (idx === -1) throw new Error("Gioco non trovato.");
  const updated: DetectedGame = { ...previewGamesState[idx], isFavorite };
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
