import { convertFileSrc, invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { open, save } from "@tauri-apps/plugin-dialog";
import type {
  AppConfig,
  BadgeEntry,
  DealsReport,
  DetectedGame,
  EmulatorIdentity,
  EmulatorInfo,
  EmulatorSetting,
  GameAchievements,
  GameDataAnswer,
  GameManualUpdate,
  GamePlatform,
  GameStatus,
  PlaySession,
  PluginInfo,
  PluginPreview,
  PluginState,
  PluginView,
  ScanReport,
  SgdbGameMatch,
  StoreAccountInfo,
  Theme,
  WindowSignIn,
  HideableSection,
} from "../types/game";
import {
  PREVIEW_MOCK_DEALS,
  PREVIEW_MOCK_EMULATED_GAMES,
  PREVIEW_MOCK_EMULATORS,
  PREVIEW_MOCK_GAMES,
} from "./previewData";
import {
  previewAchievements,
  previewCommand,
  previewGameData,
  previewPlugins,
  previewSessions,
} from "./previewExtras";
import {
  DEFAULT_ACCESSIBILITY,
  withAccessibilityDefaults,
  withAnimationsChoice,
  withThemeDefaults,
  type ThemeInput,
} from "./themes";
import { CARD_INFO_DEFAULT, withCardInfo } from "./cardInfo";

const BROWSER_STORAGE_KEY = "kanso.browser_preview.config";

const PREVIEW_THEMES_KEY = "kanso.browser_preview.themes";

const DEFAULT_CONFIG: AppConfig = {
  steamgriddbApiKey: "",
  customFolders: [],
  minimizeToTrayOnLaunch: false,
  language: null,
  themeId: null,
  accessibility: DEFAULT_ACCESSIBILITY,
  trackPlaytime: true,
  plugins: {},
  animations: "theme",
  cardInfo: [...CARD_INFO_DEFAULT],
  hiddenSections: [],
  emulators: [],
  hasGamePass: false,
  gamePassCatalog: false,
};

/** Keeps only well-formed per-plugin choices (the preview reads them from localStorage). */
function withPluginDefaults(raw: unknown): Record<string, PluginState> {
  if (!raw || typeof raw !== "object") return {};
  const states: Record<string, PluginState> = {};
  for (const [id, value] of Object.entries(raw as Record<string, Partial<PluginState>>)) {
    if (!value || typeof value !== "object") continue;
    states[id] = {
      enabled: Boolean(value.enabled),
      showBadge: value.showBadge !== false,
      settings:
        value.settings && typeof value.settings === "object" ? { ...value.settings } : {},
    };
  }
  return states;
}

/**
 * The browser preview's library: the built-in samples, or the games a page put in
 * `__KANSO_PREVIEW_GAMES__` before the app loaded (screenshots and interface tests against a
 * library of realistic size). The desktop app never reads either.
 */
function previewLibrary(): DetectedGame[] {
  const supplied = (globalThis as { __KANSO_PREVIEW_GAMES__?: unknown }).__KANSO_PREVIEW_GAMES__;
  return Array.isArray(supplied)
    ? (supplied as DetectedGame[])
    : [...PREVIEW_MOCK_GAMES, ...PREVIEW_MOCK_EMULATED_GAMES];
}

// Same ordering the desktop backend applies to a real scan
let previewGamesState: DetectedGame[] = [...previewLibrary()].sort((a, b) =>
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
      accessibility: withAccessibilityDefaults(parsed.accessibility),
      trackPlaytime: parsed.trackPlaytime !== false,
      plugins: withPluginDefaults(parsed.plugins),
      animations: withAnimationsChoice(parsed.animations),
      cardInfo: withCardInfo(parsed.cardInfo),
      hiddenSections: withHiddenSections(parsed.hiddenSections),
      emulators: withEmulatorSettings(parsed.emulators),
      hasGamePass: parsed.hasGamePass === true,
      gamePassCatalog: parsed.gamePassCatalog === true,
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
    accessibility: withAccessibilityDefaults(config.accessibility),
    trackPlaytime: config.trackPlaytime,
    plugins: withPluginDefaults(config.plugins),
    animations: withAnimationsChoice(config.animations),
    cardInfo: withCardInfo(config.cardInfo),
    hiddenSections: withHiddenSections(config.hiddenSections),
    emulators: withEmulatorSettings(config.emulators),
    hasGamePass: config.hasGamePass === true,
    gamePassCatalog: config.gamePassCatalog === true,
  };

  if (isTauri()) {
    return invoke<AppConfig>("save_settings", { config: sanitized });
  }

  localStorage.setItem(BROWSER_STORAGE_KEY, JSON.stringify(sanitized));
  return sanitized;
}

export async function scanGames(thorough = false): Promise<ScanReport> {
  if (isTauri()) {
    // `thorough`: the user asked for it, so the disks are searched for emulators as well
    return invoke<ScanReport>("scan_games", { thorough });
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

/** Several games changed at once (a pass that completes the library in the background). */
export async function subscribeToGameBatchUpdates(
  onUpdate: (games: DetectedGame[]) => void
): Promise<() => void> {
  if (!isTauri()) {
    return () => {};
  }
  return listen<DetectedGame[]>("library:games-updated", (ev) => {
    onUpdate(ev.payload);
  });
}

/** Games left the library (entries a store's list had been wrongly read into): their ids. */
export async function subscribeToGameRemovals(
  onRemoved: (ids: string[]) => void
): Promise<() => void> {
  if (!isTauri()) {
    return () => {};
  }
  return listen<string[]>("library:games-removed", (ev) => {
    onRemoved(ev.payload);
  });
}

/**
 * The slow part of completing the library: the games of the store accounts the user signed
 * in to, then the Steam store page of the games of other stores that are not installed (to
 * file them under its genres and tags). It can take minutes on a large library: what it
 * completes arrives through {@link subscribeToGameBatchUpdates}, and the promise settles
 * when it is done or the stores cannot be reached.
 */
export async function completeLibraryInBackground(): Promise<void> {
  if (isTauri()) {
    await invoke("complete_library");
  }
}

/* ----------------------------------------------------------- store accounts */

// Preview only: pretend accounts, so the sign-in panel can be tried in the browser
let previewAccounts: StoreAccountInfo[] = (
  [
    ["epic", "code"],
    ["gog", "code"],
    ["ubisoft", "window"],
    ["ea", "window"],
    ["Battle.net", "window"],
    ["xbox", "browser"],
  ] as const
).map(([platform, method]) => ({
  platform,
  method,
  connected: false,
  displayName: null,
  needsSignIn: false,
  library: null,
}));

function previewAccount(platform: GamePlatform): StoreAccountInfo {
  return (
    previewAccounts.find((account) => account.platform === platform) ?? {
      platform,
      method: "code",
      connected: false,
      displayName: null,
      needsSignIn: false,
      library: null,
    }
  );
}

function setPreviewAccount(info: StoreAccountInfo): StoreAccountInfo {
  previewAccounts = previewAccounts.map((account) =>
    account.platform === info.platform ? info : account
  );
  return info;
}

/** The stores Kanso can sign in to, and who is signed in to each. */
export async function listStoreAccounts(): Promise<StoreAccountInfo[]> {
  if (isTauri()) {
    return invoke<StoreAccountInfo[]>("list_store_accounts");
  }
  return previewAccounts;
}

/** Opens the store's own sign-in page in the browser (the backend builds the address). */
export async function openStoreSignIn(platform: GamePlatform): Promise<void> {
  if (isTauri()) {
    await invoke("open_store_sign_in", { platform });
  }
}

/**
 * Signs in with what the store's page showed at the end: the code alone or the whole text.
 * Rejects with a message for the user when the code is missing, spent or refused.
 */
export async function storeSignIn(
  platform: GamePlatform,
  pasted: string
): Promise<StoreAccountInfo> {
  if (isTauri()) {
    return invoke<StoreAccountInfo>("store_sign_in", { platform, pasted });
  }
  if (pasted.trim().length < 8) {
    throw "Non trovo un codice in quello che hai incollato.";
  }
  return setPreviewAccount({
    ...previewAccount(platform),
    connected: true,
    displayName: "Giocatore di prova",
    needsSignIn: false,
    library: null,
  });
}

/** The user gave up on the sign-in they started in the browser. */
export async function cancelStoreSignInBrowser(): Promise<void> {
  if (isTauri()) {
    await invoke("cancel_store_sign_in_browser");
  }
}

/**
 * Opens the store's own sign-in page in the user's browser and resolves once the store has
 * sent the browser back to Kanso (on this PC), or the user has given up. The password is
 * typed on the store's page, in the browser: Kanso never handles it.
 */
export async function storeSignInBrowser(platform: GamePlatform): Promise<WindowSignIn> {
  if (isTauri()) {
    return invoke<WindowSignIn>("store_sign_in_browser", { platform });
  }
  return storeSignInWindow(platform);
}

/**
 * Shows the store's own sign-in page in a window and resolves once the user has signed in
 * there, or has closed the window. The password is typed on the store's page: neither the
 * interface nor the backend ever handles it. Rejects with a message for the user when the
 * store did not answer as expected.
 */
export async function storeSignInWindow(platform: GamePlatform): Promise<WindowSignIn> {
  if (isTauri()) {
    return invoke<WindowSignIn>("store_sign_in_window", { platform });
  }
  await new Promise((resolve) => setTimeout(resolve, 600));
  const account = setPreviewAccount({
    ...previewAccount(platform),
    connected: true,
    displayName: platform === "Battle.net" ? null : "Giocatore di prova",
    needsSignIn: false,
    library:
      platform === "ubisoft"
        ? "Giochi letti dall'account: 42 · di cui 7 dall'abbonamento (118 nel suo catalogo) · 3 lasciati fuori perché non per PC (PLAYSTATION 2, SWITCH 1)"
        : "Giochi letti dall'account: 5",
  });
  return { signedIn: true, account, note: null };
}

/**
 * Forgets the store's session (and the browser profile of its sign-in window). Games already
 * in the library stay.
 */
export async function storeSignOut(platform: GamePlatform): Promise<void> {
  if (isTauri()) {
    await invoke("store_sign_out", { platform });
    return;
  }
  setPreviewAccount({
    ...previewAccount(platform),
    connected: false,
    displayName: null,
    needsSignIn: false,
    library: null,
  });
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

/** Sets where the user stands with a game, or clears it with `null`. */
export async function setGameStatus(
  gameId: string,
  status: GameStatus | null
): Promise<DetectedGame> {
  if (isTauri()) {
    return invoke<DetectedGame>("set_game_status", { gameId, status });
  }

  const idx = previewGamesState.findIndex((g) => g.id === gameId);
  if (idx === -1) throw new Error("Gioco non trovato.");
  const updated: DetectedGame = { ...previewGamesState[idx], status };
  previewGamesState[idx] = updated;
  return updated;
}

/** The genres a game shows: its store's and the user's, each once whatever the capitals. */
export function mergedGenres(storeGenres: string[], customGenres: string[]): string[] {
  const seen = new Set<string>();
  return [...storeGenres, ...customGenres].filter((genre) => {
    const key = genre.trim().toLowerCase();
    if (!key || seen.has(key)) return false;
    seen.add(key);
    return true;
  });
}

/** Sets the genres the user gives a game besides the ones its store lists. */
export async function setGameGenres(gameId: string, genres: string[]): Promise<DetectedGame> {
  if (isTauri()) {
    return invoke<DetectedGame>("set_game_genres", { gameId, genres });
  }

  const idx = previewGamesState.findIndex((g) => g.id === gameId);
  if (idx === -1) throw new Error("Gioco non trovato.");
  const own = previewGamesState[idx].genres.map((genre) => genre.toLowerCase());
  const customGenres = mergedGenres(
    [],
    genres.map((genre) => genre.trim().replace(/\s+/g, " ").slice(0, 40))
  )
    .filter((genre) => !own.includes(genre.toLowerCase()))
    .slice(0, 24);
  const updated: DetectedGame = { ...previewGamesState[idx], customGenres };
  previewGamesState[idx] = updated;
  return updated;
}

/** Asks the game's own client to uninstall it (or to show it, where that is all it takes). */
export async function uninstallGame(game: DetectedGame): Promise<void> {
  if (isTauri()) {
    await invoke<void>("uninstall_game", { gameId: game.id });
    return;
  }
  throw new Error(`Anteprima: nell'app la richiesta per ${game.title} andrebbe al client dello store.`);
}

/** Opens the folder a game is in, in the file manager. */
export async function openGameFolder(game: DetectedGame): Promise<void> {
  if (isTauri()) {
    await invoke<void>("open_game_folder", { gameId: game.id });
    return;
  }
  throw new Error(`Anteprima: nell'app si aprirebbe la cartella di ${game.title}.`);
}

/** The parts of the side panel that can be hidden, in the order Settings lists them. */
export const HIDEABLE_SECTIONS: HideableSection[] = ["home", "store", "tags"];

function withHiddenSections(value: unknown): HideableSection[] {
  const chosen = Array.isArray(value) ? value : [];
  return HIDEABLE_SECTIONS.filter((section) => chosen.includes(section));
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

/**
 * Looks up what a single game still lacks (description, genres, tags, a local cover). Games
 * that are not on disk are left out of the automatic sync and completed through this.
 */
export async function completeGameDetails(game: DetectedGame): Promise<DetectedGame> {
  if (isTauri()) {
    return invoke<DetectedGame>("complete_game_details", { gameId: game.id });
  }
  return game;
}

/** Asks the game's store client to install a game that is not on disk. */
export async function installGame(game: DetectedGame): Promise<void> {
  if (isTauri()) {
    await invoke<void>("install_game", { gameId: game.id });
    return;
  }
  throw new Error(`Installazione disponibile nell'app Windows (${game.title}).`);
}

/** Stored emulator settings read back: well-formed entries only. */
function withEmulatorSettings(raw: unknown): EmulatorSetting[] {
  if (!Array.isArray(raw)) return [];
  const texts = (value: unknown): string[] =>
    Array.isArray(value) ? value.filter((item): item is string => typeof item === "string") : [];
  return raw
    .filter((entry): entry is Record<string, unknown> => typeof entry === "object" && entry !== null)
    .filter((entry) => typeof entry.id === "string" && entry.id.trim() !== "")
    .map((entry) => ({
      id: String(entry.id).trim(),
      name: typeof entry.name === "string" ? entry.name : "",
      exePath: typeof entry.exePath === "string" ? entry.exePath : "",
      folders: texts(entry.folders),
      extensions: texts(entry.extensions),
      args: typeof entry.args === "string" ? entry.args : "",
    }));
}

/**
 * The emulators on this PC: the ones Kanso found and the ones added in Settings, each with
 * the folders its games are read from. Their games come with the library (`platform:
 * "emulator"`, `platformAppId` naming the emulator).
 */
export async function listEmulators(): Promise<EmulatorInfo[]> {
  if (isTauri()) {
    return invoke<EmulatorInfo[]>("list_emulators");
  }
  const config = await getSettings();
  const added: EmulatorInfo[] = config.emulators
    .filter((setting) => !PREVIEW_MOCK_EMULATORS.some((known) => known.id === setting.id))
    .map((setting) => ({
      id: setting.id,
      name: setting.name || setting.id,
      systems: "",
      exePath: setting.exePath,
      folders: setting.folders.map((path) => ({ path, fromEmulator: false })),
      addedByUser: true,
      version: null,
    }));
  return [...PREVIEW_MOCK_EMULATORS, ...added];
}

/** Opens the emulator itself, with no game. */
export async function launchEmulator(emulator: EmulatorInfo): Promise<void> {
  if (isTauri()) {
    await invoke<void>("launch_emulator", { emulatorId: emulator.id });
    return;
  }
  throw new Error(`Avvio disponibile nell'applicazione desktop Windows (${emulator.name}).`);
}

/** What Kanso recognises an executable as; `null` for a program it does not know. */
export async function identifyEmulator(exePath: string): Promise<EmulatorIdentity | null> {
  if (isTauri()) {
    return invoke<EmulatorIdentity | null>("identify_emulator", { exePath });
  }
  const file = exePath.replace(/\\/g, "/").split("/").pop()?.toLowerCase() ?? "";
  if (file.startsWith("dolphin")) {
    return { id: "dolphin", name: "Dolphin", systems: "GameCube · Wii", extensions: ["iso", "rvz", "gcz", "wbfs"], args: '-b -e "{rom}"' };
  }
  return null;
}

export async function pickEmulatorExecutable(): Promise<string | null> {
  if (!isTauri()) return null;
  const selected = await open({
    directory: false,
    multiple: false,
    filters: [{ name: "Windows", extensions: ["exe"] }],
  });
  return typeof selected === "string" && selected.trim().length > 0 ? selected : null;
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
    // Themes saved by an earlier version lack the newer sections
    return Array.isArray(parsed)
      ? (parsed as ThemeInput[]).map(withThemeDefaults)
      : [];
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

/* ----------------------------------------------------------------- window */

export type WindowAction = "minimize" | "toggleMaximize" | "close";

/** The title bar is drawn by Kanso: its buttons drive the native window. */
export async function windowAction(action: WindowAction): Promise<void> {
  if (!isTauri()) return;
  const appWindow = getCurrentWindow();
  if (action === "minimize") await appWindow.minimize();
  else if (action === "toggleMaximize") await appWindow.toggleMaximize();
  else await appWindow.close();
}

/**
 * Reports whether the window is maximized, now and whenever it is resized (snapping and
 * double-clicking the title bar maximize it without going through Kanso's button).
 * Resolves to the function that stops watching.
 */
export async function watchWindowMaximized(
  onChange: (isMaximized: boolean) => void
): Promise<() => void> {
  if (!isTauri()) return () => {};
  const appWindow = getCurrentWindow();
  const sync = () => {
    appWindow
      .isMaximized()
      .then(onChange)
      .catch(() => {});
  };
  sync();
  return appWindow.onResized(sync);
}

/* ------------------------------------------------- play time & achievements */

/** Play sessions that started at or after `sinceMs`, oldest first. */
export async function getPlaySessions(sinceMs: number): Promise<PlaySession[]> {
  if (isTauri()) {
    return invoke<PlaySession[]>("get_play_sessions", { sinceMs });
  }
  return previewSessions(previewGamesState).filter((s) => s.startedAt >= sinceMs);
}

/** The achievements Steam lists for a game; `null` when there is nothing to show. */
export async function getGameAchievements(
  game: DetectedGame
): Promise<GameAchievements | null> {
  if (isTauri()) {
    return invoke<GameAchievements | null>("get_game_achievements", { gameId: game.id });
  }
  return previewAchievements(game);
}

/**
 * Reports the ids of the games Kanso sees running, now and whenever that changes. Resolves
 * to the function that stops watching.
 */
export async function watchRunningGames(
  onChange: (gameIds: string[]) => void
): Promise<() => void> {
  if (!isTauri()) return () => {};
  const unlisten = await listen<string[]>("playtime:running", (ev) => onChange(ev.payload));
  invoke<string[]>("get_running_games")
    .then(onChange)
    .catch(() => {});
  return unlisten;
}

/* ---------------------------------------------------------------- plugins */

/** Every available plugin, plus a warning per folder that is not a valid plugin. */
export async function listPlugins(): Promise<{
  plugins: PluginInfo[];
  warnings: string[];
}> {
  if (isTauri()) {
    return invoke<{ plugins: PluginInfo[]; warnings: string[] }>("list_plugins");
  }
  const config = await getSettings();
  return { plugins: previewPlugins(config.plugins), warnings: [] };
}

/**
 * Lets the user pick a plugin's `plugin.json` and reads it without installing, so they can
 * see what it is and which hosts it contacts first. `null` if the dialog was cancelled.
 */
export async function pickPluginToInstall(): Promise<
  { path: string; preview: PluginPreview } | null
> {
  if (!isTauri()) return null;
  const selected = await open({
    directory: false,
    multiple: false,
    filters: [{ name: "Kanso plugin (plugin.json)", extensions: ["json"] }],
  });
  if (typeof selected !== "string" || !selected.trim()) return null;
  const preview = await invoke<PluginPreview>("inspect_plugin", { path: selected });
  return { path: selected, preview };
}

/** Installs the plugin at `path` and switches it on. Resolves to the settings as saved. */
export async function installPlugin(path: string): Promise<AppConfig> {
  return invoke<AppConfig>("install_plugin", { path });
}

/** Removes a plugin the user installed. Resolves to the settings as saved. */
export async function removePlugin(id: string): Promise<AppConfig> {
  return invoke<AppConfig>("remove_plugin", { id });
}

export async function openPluginsFolder(): Promise<void> {
  if (isTauri()) {
    await invoke<void>("open_plugins_folder");
  }
}

/** What a plugin shows in a game's sheet; `force` asks again instead of using the cache. */
export async function pluginGameData(
  pluginId: string,
  game: DetectedGame,
  force = false
): Promise<GameDataAnswer> {
  if (isTauri()) {
    return invoke<GameDataAnswer>("plugin_game_data", {
      pluginId,
      gameId: game.id,
      force,
    });
  }
  return previewGameData(pluginId, game);
}

/** The labels plugins have for the game cards, by game id. */
export async function pluginBadges(): Promise<Record<string, BadgeEntry[]>> {
  if (isTauri()) {
    return invoke<Record<string, BadgeEntry[]>>("plugin_badges");
  }
  const config = await getSettings();
  const labelled = previewPlugins(config.plugins).filter((p) => p.enabled && p.showBadge);
  const badges: Record<string, BadgeEntry[]> = {};
  for (const game of previewGamesState) {
    const entries = labelled.flatMap((plugin) => {
      const badge = previewGameData(plugin.id, game).data?.badge;
      return badge
        ? [
            {
              pluginId: plugin.id,
              pluginName: plugin.name,
              text: badge.text,
              label: badge.label ?? null,
              tooltip: badge.tooltip ?? null,
            },
          ]
        : [];
    });
    if (entries.length > 0) badges[game.id] = entries;
  }
  return badges;
}

/** Calls `onChange` whenever a plugin produced a new label. Resolves to the unsubscriber. */
export async function watchPluginBadges(onChange: () => void): Promise<() => void> {
  if (!isTauri()) return () => {};
  return listen("plugins:badges-updated", () => onChange());
}

export async function runPluginCommand(
  pluginId: string,
  commandId: string
): Promise<PluginView> {
  if (isTauri()) {
    return invoke<PluginView>("run_plugin_command", { pluginId, commandId });
  }
  return previewCommand(previewGamesState);
}

/** Opens a link shown by a plugin in the default browser (`https` only). */
export async function openPluginLink(url: string): Promise<void> {
  if (isTauri()) {
    await invoke<void>("open_plugin_link", { url });
    return;
  }
  window.open(url, "_blank", "noopener,noreferrer");
}
