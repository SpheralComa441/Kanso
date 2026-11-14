import { convertFileSrc, invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open, save } from "@tauri-apps/plugin-dialog";
import type {
  AppConfig,
  DealsReport,
  DetectedGame,
  GameManualUpdate,
  ScanReport,
  SgdbGameMatch,
  Theme,
} from "../types/game";
import { PREVIEW_MOCK_DEALS, PREVIEW_MOCK_GAMES } from "./previewData";

const BROWSER_STORAGE_KEY = "kanso.browser_preview.config";

const PREVIEW_THEMES_KEY = "kanso.browser_preview.themes";

const DEFAULT_CONFIG: AppConfig = {
  steamgriddbApiKey: "",
  customFolders: [],
  minimizeToTrayOnLaunch: false,
  language: null,
  themeId: null,
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
      language: typeof parsed.language === "string" ? parsed.language : null,
      themeId: typeof parsed.themeId === "string" ? parsed.themeId : null,
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
    language: config.language,
    themeId: config.themeId,
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

/* ------------------------------------------------------------------ deals */

/**
 * Current discounts on Steam, Epic and GOG. `country` (ISO code) selects regional prices;
 * the backend reuses its last answer for half an hour unless `forceRefresh`.
 */
export async function fetchDeals(
  country: string,
  forceRefresh = false
): Promise<DealsReport> {
  if (isTauri()) {
    return invoke<DealsReport>("fetch_deals", { country, forceRefresh });
  }
  return { deals: PREVIEW_MOCK_DEALS, failedStores: [], fetchedAt: Date.now() };
}

/** Opens a deal's store page in the default browser. */
export async function openStorePage(url: string): Promise<void> {
  if (isTauri()) {
    await invoke<void>("open_store_page", { url });
    return;
  }
  window.open(url, "_blank", "noopener,noreferrer");
}

/* ----------------------------------------------------------------- themes */

function readPreviewThemes(): Theme[] {
  try {
    const raw = localStorage.getItem(PREVIEW_THEMES_KEY);
    const parsed: unknown = raw ? JSON.parse(raw) : [];
    return Array.isArray(parsed) ? (parsed as Theme[]) : [];
  } catch {
    return [];
  }
}

function writePreviewThemes(themes: Theme[]): void {
  try {
    localStorage.setItem(PREVIEW_THEMES_KEY, JSON.stringify(themes));
  } catch {
    // Preview only: themes simply won't survive a reload
  }
}

/** Themes installed by the user, plus a warning per invalid file in the themes folder. */
export async function listThemes(): Promise<{
  themes: Theme[];
  warnings: string[];
}> {
  if (isTauri()) {
    return invoke<{ themes: Theme[]; warnings: string[] }>("list_themes");
  }
  return { themes: readPreviewThemes(), warnings: [] };
}

/**
 * Stores a theme made in the editor. `replaceId` overwrites that installed theme; without
 * it a new theme is created.
 */
export async function saveTheme(
  theme: Theme,
  replaceId: string | null
): Promise<Theme> {
  if (isTauri()) {
    return invoke<Theme>("save_theme", { theme, replaceId });
  }
  const themes = readPreviewThemes();
  const slug =
    theme.name
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, "-")
      .replace(/^-+|-+$/g, "") || "theme";
  let id = replaceId ?? slug;
  for (let n = 2; !replaceId && themes.some((t) => t.id === id); n += 1) {
    id = `${slug}-${n}`;
  }
  const saved: Theme = { ...theme, id };
  writePreviewThemes([...themes.filter((t) => t.id !== id), saved]);
  return saved;
}

export async function deleteTheme(id: string): Promise<void> {
  if (isTauri()) {
    await invoke<void>("delete_theme", { id });
    return;
  }
  writePreviewThemes(readPreviewThemes().filter((t) => t.id !== id));
}

/** Lets the user pick a theme file and installs it. `null` if the dialog was cancelled. */
export async function importThemeFromFile(): Promise<Theme | null> {
  if (!isTauri()) return null;
  const selected = await open({
    directory: false,
    multiple: false,
    filters: [{ name: "Kanso theme", extensions: ["json"] }],
  });
  if (typeof selected !== "string" || !selected.trim()) return null;
  return invoke<Theme>("import_theme", { path: selected });
}

/** Lets the user choose where to write `theme`. `false` if the dialog was cancelled. */
export async function exportThemeToFile(theme: Theme): Promise<boolean> {
  if (!isTauri()) return false;
  const fileName =
    theme.name
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, "-")
      .replace(/^-+|-+$/g, "") || "theme";
  const path = await save({
    defaultPath: `${fileName}.kanso-theme.json`,
    filters: [{ name: "Kanso theme", extensions: ["json"] }],
  });
  if (!path) return false;
  await invoke<void>("export_theme", { theme, path });
  return true;
}

export async function openThemesFolder(): Promise<void> {
  if (isTauri()) {
    await invoke<void>("open_themes_folder");
  }
}
