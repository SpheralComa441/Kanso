import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { CloudDownload, EyeOff, History, LayoutGrid, Star, X } from "lucide-react";
import { ApiKeyBanner } from "./components/ApiKeyBanner";
import { CategoryGrid, type CategoryKind } from "./components/CategoryGrid";
import { HomeView } from "./components/HomeView";
import { ambienceFor, isSouthernTimeZone, type WeatherNow } from "./lib/ambience";
import { useHomePlugins } from "./lib/useHomePlugins";
import { EmulatorsView } from "./components/EmulatorsView";
import { EditGameModal } from "./components/EditGameModal";
import { GameDetail } from "./components/GameDetail";
import { GameList } from "./components/GameList";
import { Header, type PluginCommandEntry, type ViewMode } from "./components/Header";
import {
  PluginCommandDialog,
  type PluginCommandRun,
} from "./components/PluginCommandDialog";
import { SettingsModal, type SettingsSection } from "./components/SettingsModal";
import { Sidebar, type MainView } from "./components/Sidebar";
import { NewsView } from "./components/NewsView";
import { likedEndingSoon, StoreView } from "./components/StoreView";
import { StatsView } from "./components/StatsView";
import { STATUS_LABEL_KEYS, StatusIcon } from "./components/StatusIcon";
import { ThemeEditor } from "./components/ThemeEditor";
import {
  I18nProvider,
  detectLanguage,
  isLanguageCode,
  useI18n,
  type LanguageCode,
  type MessageKey,
} from "./i18n";
import {
  completeGameDetails,
  completeLibraryInBackground,
  getSettings,
  installGame,
  isRunningInTauri,
  launchEmulator,
  launchGame,
  listEmulators,
  listPlugins,
  listThemes,
  pluginBadges,
  runPluginCommand,
  saveSettings,
  saveTheme,
  scanGames,
  setGameFavorite,
  setGameHidden,
  homeWeather,
  openGameFolder,
  openWeatherSource,
  uninstallGame,
  setGameGenres,
  mergedGenres,
  setGameStatus,
  subscribeToGameBatchUpdates,
  subscribeToGameRemovals,
  subscribeToGameUpdates,
  syncLibraryMetadata,
  watchPluginBadges,
  watchRunningGames,
} from "./lib/backend";
import {
  EMPTY_FILTERS,
  activeFilterCount,
  applyLibraryFilters,
  countLibrary,
  hasActiveFilters,
  isInScope,
  libraryScope,
  readUiPreference,
  sortGamesByTitle,
  summarizeGenres,
  summarizeTags,
  writeUiPreference,
  type LibraryCollection,
  type LibraryFilters,
} from "./lib/library";
import { isEmulated, platformLabel, uninstallMode } from "./lib/platforms";
import { CARD_INFO_DEFAULT } from "./lib/cardInfo";
import { gamesToUpdate, loadDlssStatus, useDlssStatus } from "./lib/dlss";
import {
  DEFAULT_ACCESSIBILITY,
  applyTheme,
  offeredGroups,
  offeredPages,
  resolveTheme,
} from "./lib/themes";
import type {
  AccessibilityConfig,
  AnimationsChoice,
  AppConfig,
  BadgeEntry,
  DetectedGame,
  EmulatorInfo,
  GameStatus,
  PluginInfo,
  ScanReport,
  Theme,
  HomeConfig,
  StoreConfig,
} from "./types/game";

const INITIAL_CONFIG: AppConfig = {
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
  home: { hidden: [], effects: true, weather: false, place: null, upNext: [] },
  store: { hiddenStores: [], hiddenDeals: [], liked: [] },
  newsSources: null,
  newsFeeds: [],
  dlssUpdates: false,
  emulators: [],
  hasGamePass: false,
  gamePassCatalog: false,
};

const SIDEBAR_PREF_KEY = "kanso.ui.sidebar";
const VIEW_MODE_PREF_KEY = "kanso.ui.viewMode";
const MAIN_VIEW_PREF_KEY = "kanso.ui.mainView";
const INSTALLED_ONLY_PREF_KEY = "kanso.ui.installedOnly";

const COLLECTION_KEYS: Record<LibraryCollection, MessageKey> = {
  all: "collection.all",
  favorites: "collection.favorites",
  recent: "collection.recent",
  notInstalled: "collection.notInstalled",
  hidden: "collection.hidden",
};

interface FilterChipProps {
  label: string;
  onRemove: () => void;
  icon?: React.ReactNode;
}

function FilterChip({ label, onRemove, icon }: FilterChipProps) {
  const { t } = useI18n();
  return (
    <button
      type="button"
      onClick={onRemove}
      title={t("filters.remove", { label })}
      className="group inline-flex items-center gap-1.5 rounded-full border border-oled-border bg-oled-surface py-0.5 pl-2.5 pr-1.5 text-2xs font-medium text-zinc-200 transition-colors hover:border-zinc-600"
    >
      {icon}
      <span>{label}</span>
      <X className="h-3 w-3 text-zinc-500 group-hover:text-zinc-200" />
    </button>
  );
}

/** Loads the settings, decides the interface language and hosts the translated shell. */
export function App() {
  const [config, setConfig] = useState<AppConfig>(INITIAL_CONFIG);
  const [settingsLoaded, setSettingsLoaded] = useState(false);
  // Until the settings arrive (and on first run) follow the OS language
  const [detectedLanguage] = useState<LanguageCode>(detectLanguage);

  useEffect(() => {
    let mounted = true;
    (async () => {
      try {
        let loaded = await getSettings();
        if (!isLanguageCode(loaded.language)) {
          // First run: remember the detected language so the backend (tray menu, error
          // messages, metadata language) uses it too
          loaded = await saveSettings({ ...loaded, language: detectedLanguage });
        }
        if (mounted) setConfig(loaded);
      } catch {
        // Proceed with defaults
      } finally {
        if (mounted) setSettingsLoaded(true);
      }
    })();
    return () => {
      mounted = false;
    };
  }, [detectedLanguage]);

  const lang = isLanguageCode(config.language) ? config.language : detectedLanguage;

  useEffect(() => {
    document.documentElement.lang = lang;
  }, [lang]);

  return (
    <I18nProvider lang={lang}>
      <Shell
        config={config}
        onConfigChange={setConfig}
        settingsLoaded={settingsLoaded}
      />
    </I18nProvider>
  );
}

interface ShellProps {
  config: AppConfig;
  onConfigChange: (config: AppConfig) => void;
  settingsLoaded: boolean;
}

function Shell({ config, onConfigChange, settingsLoaded }: ShellProps) {
  const { t, lang, plural } = useI18n();
  const [report, setReport] = useState<ScanReport>({ games: [], warnings: [] });
  const [isScanning, setIsScanning] = useState(true);
  const [isSyncingMetadata, setIsSyncingMetadata] = useState(false);
  const [isSettingsOpen, setIsSettingsOpen] = useState(false);
  // The category Settings opens on when it is opened for something in particular
  const [settingsSection, setSettingsSection] = useState<SettingsSection | null>(null);
  const openSettings = (section: SettingsSection | null = null) => {
    setSettingsSection(section);
    setIsSettingsOpen(true);
  };
  const [detailGameId, setDetailGameId] = useState<string | null>(null);
  const [editingGameId, setEditingGameId] = useState<string | null>(null);
  const [bannerDismissed, setBannerDismissed] = useState(false);
  const [searchQuery, setSearchQuery] = useState("");
  /** The emulators of this PC, as the last scan found them. */
  const [emulators, setEmulators] = useState<EmulatorInfo[]>([]);
  /** They were read once: until then nobody knows whether this PC has any. */
  const [emulatorsRead, setEmulatorsRead] = useState(false);
  /** The emulator whose page is open in the Emulators section. */
  const [openEmulatorId, setOpenEmulatorId] = useState<string | null>(null);
  const [filters, setFilters] = useState<LibraryFilters>(EMPTY_FILTERS);
  /** The page the user chose; what is on screen is `mainView`, further down. */
  const [chosenView, setMainView] = useState<MainView>(() =>
    readUiPreference(
      MAIN_VIEW_PREF_KEY,
      ["home", "store", "news", "library", "stats", "emulators", "genres", "tags"],
      "library"
    )
  );
  /** The global switch: games that are not on disk are left out of every list. */
  const [installedOnly, setInstalledOnly] = useState(
    () => readUiPreference(INSTALLED_ONLY_PREF_KEY, ["on", "off"], "off") === "on"
  );
  const [isSidebarOpen, setIsSidebarOpen] = useState(
    () => readUiPreference(SIDEBAR_PREF_KEY, ["open", "closed"], "open") === "open"
  );
  const [viewMode, setViewMode] = useState<ViewMode>(() =>
    readUiPreference(VIEW_MODE_PREF_KEY, ["grid", "list"], "grid")
  );
  const [launchingId, setLaunchingId] = useState<string | null>(null);
  const [statusMessage, setStatusMessage] = useState<string | null>(null);
  const [userThemes, setUserThemes] = useState<Theme[]>([]);
  /** The installed themes were read once: until then the saved theme may not be the one on screen. */
  const [themesLoaded, setThemesLoaded] = useState(false);
  /** Theme shown but not saved: a selection in Settings or a draft in the editor. */
  const [previewTheme, setPreviewTheme] = useState<Theme | null>(null);
  const [themeEditorBase, setThemeEditorBase] = useState<Theme | null>(null);
  /** Accessibility choices being tried in Settings, before they are saved. */
  const [previewAccessibility, setPreviewAccessibility] =
    useState<AccessibilityConfig | null>(null);
  /** The level of animations being tried in Settings, before it is saved. */
  const [previewAnimations, setPreviewAnimations] = useState<AnimationsChoice | null>(null);
  const [plugins, setPlugins] = useState<PluginInfo[]>([]);
  const [pluginWarnings, setPluginWarnings] = useState<string[]>([]);
  /** Short labels plugins have for the game cards, by game id. */
  const [badges, setBadges] = useState<Record<string, BadgeEntry[]>>({});
  const [runningIds, setRunningIds] = useState<ReadonlySet<string>>(new Set());
  const [commandRun, setCommandRun] = useState<PluginCommandRun | null>(null);

  /* ------------------------------------------------------------------ theme */

  const reloadThemes = useCallback(async () => {
    try {
      const { themes } = await listThemes();
      setUserThemes(themes);
      return themes;
    } catch {
      return [];
    }
  }, []);

  useEffect(() => {
    void reloadThemes().finally(() => setThemesLoaded(true));
  }, [reloadThemes]);

  const savedTheme = useMemo(
    () => resolveTheme(config.themeId, userThemes),
    [config.themeId, userThemes]
  );
  /** The theme on screen: the saved one, or the one being tried in Settings or the editor. */
  const activeTheme = previewTheme ?? savedTheme;
  useEffect(() => {
    applyTheme(
      activeTheme,
      previewAccessibility ?? config.accessibility,
      previewAnimations ?? config.animations
    );
  }, [
    activeTheme,
    previewAccessibility,
    config.accessibility,
    previewAnimations,
    config.animations,
  ]);

  // What the side panel offers besides the library: the pages and groups the theme lists,
  // less what the user hid in Settings. Entries and links are drawn from this alone
  const hiddenSections = config.hiddenSections;
  // The Emulators page is the only one that lists the games emulators run: while there are
  // emulators, or such games, it stays offered whatever the theme lists. Until the first
  // scan has said, the page is kept for who was left on it, and not shown to anyone else
  const hasEmulation = emulatorsRead
    ? emulators.length > 0 || report.games.some(isEmulated)
    : chosenView === "emulators";
  const navPages = useMemo(
    () => offeredPages(activeTheme.nav, hiddenSections, hasEmulation),
    [activeTheme.nav, hiddenSections, hasEmulation]
  );
  const navGroups = useMemo(
    () => offeredGroups(activeTheme.nav, hiddenSections),
    [activeTheme.nav, hiddenSections]
  );
  // A page the user hid, or one the theme does not list, is not a place to be left on:
  // the library takes over. The same goes for the tiles of genres and tags
  const viewOffered =
    chosenView === "library" ||
    (chosenView === "genres" || chosenView === "tags"
      ? navGroups.includes(chosenView)
      : navPages.includes(chosenView));
  /** The page on screen: the one the user chose, while the panel offers it. */
  const mainView: MainView = viewOffered ? chosenView : "library";
  // The library becomes the place the user is in, and where Kanso opens next time. Not while
  // a theme is only being tried in Settings or in the editor: leaving it brings the page back
  const tryingTheme = previewTheme !== null;
  useEffect(() => {
    if (!viewOffered && !tryingTheme) {
      setMainView("library");
      writeUiPreference(MAIN_VIEW_PREF_KEY, "library");
    }
  }, [viewOffered, tryingTheme]);

  /* ---------------------------------------------------------------- plugins */

  const reloadBadges = useCallback(async () => {
    try {
      setBadges(await pluginBadges());
    } catch {
      // Labels are a nicety: the library works without them
    }
  }, []);

  const reloadPlugins = useCallback(async () => {
    try {
      const listed = await listPlugins();
      setPlugins(listed.plugins);
      setPluginWarnings(listed.warnings);
    } catch {
      setPlugins([]);
    }
    await reloadBadges();
  }, [reloadBadges]);

  // Plugin names are in the interface language: read them again when it changes
  useEffect(() => {
    if (settingsLoaded) void reloadPlugins();
  }, [settingsLoaded, lang, reloadPlugins]);

  // A plugin folder may have been dropped in by hand since the last look
  useEffect(() => {
    if (isSettingsOpen) void reloadPlugins();
  }, [isSettingsOpen, reloadPlugins]);

  // Labels arrive one game at a time while plugins catch up in the background
  const badgeTimer = useRef<number | null>(null);
  useEffect(() => {
    let active = true;
    let stop: (() => void) | null = null;
    void watchPluginBadges(() => {
      if (badgeTimer.current !== null) window.clearTimeout(badgeTimer.current);
      badgeTimer.current = window.setTimeout(() => void reloadBadges(), 400);
    }).then((unlisten) => {
      if (active) stop = unlisten;
      else unlisten();
    });
    return () => {
      active = false;
      stop?.();
      if (badgeTimer.current !== null) window.clearTimeout(badgeTimer.current);
    };
  }, [reloadBadges]);

  useEffect(() => {
    let active = true;
    let stop: (() => void) | null = null;
    void watchRunningGames((ids) => {
      if (active) setRunningIds(new Set(ids));
    }).then((unlisten) => {
      if (active) stop = unlisten;
      else unlisten();
    });
    return () => {
      active = false;
      stop?.();
    };
  }, []);

  const startCommand = useCallback(
    async (command: Pick<PluginCommandRun, "pluginId" | "commandId" | "title">) => {
      setCommandRun((previous) => ({
        ...command,
        // Running the same command again keeps its last answer on screen meanwhile
        view:
          previous?.pluginId === command.pluginId && previous.commandId === command.commandId
            ? previous.view
            : null,
        error: null,
        isRunning: true,
      }));
      try {
        const view = await runPluginCommand(command.pluginId, command.commandId);
        setCommandRun((current) =>
          current && current.isRunning ? { ...current, view, isRunning: false } : current
        );
      } catch (err) {
        const error = typeof err === "string" ? err : t("plugins.actionError");
        setCommandRun((current) =>
          current && current.isRunning ? { ...current, error, isRunning: false } : current
        );
      }
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    []
  );

  /* ---------------------------------------------------------------- library */

  const upsertSingleGameInState = useCallback((updated: DetectedGame) => {
    setReport((prev) => {
      const exists = prev.games.some((g) => g.id === updated.id);
      const nextGames = exists
        ? prev.games.map((g) => (g.id === updated.id ? updated : g))
        : [...prev.games, updated];
      return { ...prev, games: sortGamesByTitle(nextGames) };
    });
  }, []);

  /** Games completed in the background, several at a time: one update of the library. */
  const upsertGamesInState = useCallback((updated: DetectedGame[]) => {
    if (updated.length === 0) return;
    setReport((prev) => {
      const byId = new Map(updated.map((game) => [game.id, game]));
      const kept = prev.games.map((game) => {
        const next = byId.get(game.id);
        byId.delete(game.id);
        return next ?? game;
      });
      return { ...prev, games: sortGamesByTitle([...kept, ...byId.values()]) };
    });
  }, []);

  const runMetadataSync = useCallback(async (retryNow: boolean) => {
    if (!isRunningInTauri()) return;
    setIsSyncingMetadata(true);
    try {
      const syncedReport = await syncLibraryMetadata(retryNow);
      setReport((prev) => ({ ...syncedReport, warnings: prev.warnings }));
    } catch {
      // Individual errors do not block library browsing
    } finally {
      setIsSyncingMetadata(false);
    }
    // The games of the store accounts, then genres and tags for the games of other stores
    // that are not installed: a slow, quiet pass whose results arrive in batches
    void completeLibraryInBackground().catch(() => {});
  }, []);

  /**
   * `userRequested`: the user asked for this scan, so games whose metadata lookup already
   * failed today are retried too. The automatic scan at startup stays quiet about them.
   */
  const performScan = useCallback(
    async (userRequested: boolean) => {
      setIsScanning(true);
      setStatusMessage(null);
      try {
        // A scan the user asked for also searches the disks for emulators
        const res = await scanGames(userRequested);
        setReport(res);
        // The scan is what finds the emulators kept in the user's games folders
        void listEmulators()
          .then(setEmulators)
          .catch(() => {})
          .finally(() => setEmulatorsRead(true));
        void runMetadataSync(userRequested);
      } catch (err) {
        setStatusMessage(typeof err === "string" ? err : t("app.scanError"));
      } finally {
        setIsScanning(false);
      }
    },
    // `t` changes with the language; a language switch must not trigger a rescan
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [runMetadataSync]
  );

  /** Games the backend took out of the library (entries that should never have been there). */
  const removeGamesFromState = useCallback((ids: string[]) => {
    if (ids.length === 0) return;
    const gone = new Set(ids);
    setReport((prev) => ({ ...prev, games: prev.games.filter((game) => !gone.has(game.id)) }));
  }, []);

  // First scan once the settings (and with them the backend language) are in place
  useEffect(() => {
    if (!settingsLoaded) return;
    let mounted = true;
    let unlistenFn: (() => void) | null = null;
    let unlistenBatchFn: (() => void) | null = null;
    let unlistenRemovalsFn: (() => void) | null = null;

    (async () => {
      unlistenFn = await subscribeToGameUpdates((updated) => {
        if (mounted) upsertSingleGameInState(updated);
      });
      unlistenBatchFn = await subscribeToGameBatchUpdates((updated) => {
        if (mounted) upsertGamesInState(updated);
      });
      unlistenRemovalsFn = await subscribeToGameRemovals((ids) => {
        if (mounted) removeGamesFromState(ids);
      });
      if (mounted) await performScan(false);
    })();

    return () => {
      mounted = false;
      if (unlistenFn) unlistenFn();
      if (unlistenBatchFn) unlistenBatchFn();
      if (unlistenRemovalsFn) unlistenRemovalsFn();
    };
  }, [settingsLoaded, performScan, upsertSingleGameInState, upsertGamesInState, removeGamesFromState]);

  const handleSaveSettings = async (updated: AppConfig) => {
    const foldersChanged =
      JSON.stringify(updated.customFolders) !== JSON.stringify(config.customFolders) ||
      // An emulator added, or a folder of games for one: the scan reads them
      JSON.stringify(updated.emulators) !== JSON.stringify(config.emulators);
    const apiKeyChanged =
      Boolean(updated.steamgriddbApiKey.trim()) &&
      updated.steamgriddbApiKey.trim() !== config.steamgriddbApiKey.trim();
    const languageChanged = updated.language !== lang;
    // Games to add: the ones Game Pass gives on Battle.net, or its catalog for PC
    const gamePassAdded =
      (updated.hasGamePass && !config.hasGamePass) ||
      (updated.gamePassCatalog && !config.gamePassCatalog);

    const saved = await saveSettings(updated);
    onConfigChange(saved);
    // Plugins may have been switched on or off, or their labels hidden
    void reloadPlugins();

    if (foldersChanged) {
      await performScan(true);
    } else if (apiKeyChanged || languageChanged) {
      // A new key can find covers earlier attempts could not; a new language means
      // descriptions and genres have to be fetched again
      void runMetadataSync(true);
    }
    // The background pass reads the catalog, and finds the covers the games still lack
    if (gamePassAdded) {
      void completeLibraryInBackground().catch(() => {});
    }
  };

  /** Starts a game, or hands one that is not on disk to its store's client for installing. */
  const handleLaunchGame = async (game: DetectedGame) => {
    setLaunchingId(game.id);
    setStatusMessage(null);
    try {
      if (!game.isInstalled) {
        if (!isRunningInTauri()) throw t("app.installPreview");
        await installGame(game);
      } else {
        if (!isRunningInTauri()) throw t("app.launchPreview");
        await launchGame(game);
      }
    } catch (err) {
      setStatusMessage(
        typeof err === "string"
          ? err
          : t(game.isInstalled ? "app.launchError" : "app.installError", { title: game.title })
      );
    } finally {
      setLaunchingId(null);
    }
  };

  const handleToggleFavorite = async (game: DetectedGame) => {
    const nextValue = !game.isFavorite;
    // Optimistic: the star reacts instantly, then the backend confirms
    upsertSingleGameInState({ ...game, isFavorite: nextValue });
    try {
      upsertSingleGameInState(await setGameFavorite(game.id, nextValue));
    } catch (err) {
      upsertSingleGameInState(game);
      setStatusMessage(typeof err === "string" ? err : t("app.favError"));
    }
  };

  const handleToggleHidden = async (game: DetectedGame) => {
    const nextValue = !game.isHidden;
    upsertSingleGameInState({ ...game, isHidden: nextValue });
    // The game leaves the view it was opened from
    setDetailGameId(null);
    try {
      upsertSingleGameInState(await setGameHidden(game.id, nextValue));
    } catch (err) {
      upsertSingleGameInState(game);
      setStatusMessage(typeof err === "string" ? err : t("app.hideError"));
    }
  };

  const handleSetStatus = async (game: DetectedGame, status: GameStatus | null) => {
    upsertSingleGameInState({ ...game, status });
    try {
      upsertSingleGameInState(await setGameStatus(game.id, status));
    } catch (err) {
      upsertSingleGameInState(game);
      setStatusMessage(typeof err === "string" ? err : t("app.statusError"));
    }
  };

  /** The genres the user gives a game, besides its store's own. */
  const handleSetGenres = async (game: DetectedGame, customGenres: string[]) => {
    try {
      upsertSingleGameInState(await setGameGenres(game.id, customGenres));
    } catch (err) {
      setStatusMessage(typeof err === "string" ? err : t("app.statusError"));
    }
  };

  /* ------------------------------------------------------------- navigation */

  /**
   * What the content area shows, as far as the side panel decides it: when this changes the
   * content is replaced (and animated in), while typing in the search box only narrows it.
   */
  const viewKey = [
    mainView,
    filters.collection,
    filters.platform ?? "",
    filters.genre ?? "",
    filters.tag ?? "",
    filters.status ?? "",
    mainView === "emulators" ? (openEmulatorId ?? "") : "",
  ].join("|");

  const showView = (view: MainView) => {
    setMainView(view);
    writeUiPreference(MAIN_VIEW_PREF_KEY, view);
  };

  const changeFilters = (next: LibraryFilters) => {
    setFilters(next);
    showView("library");
  };

  const changeSearch = (value: string) => {
    setSearchQuery(value);
    // The search box searches the library, wherever it is typed from; on an emulator's
    // page it searches that emulator's games
    const onEmulatorPage = mainView === "emulators" && openEmulatorId !== null;
    // The page the user chose is what counts, not the one on screen: while a theme that
    // does not offer it is being tried, the library is already shown in its place, and a
    // search typed there must leave the user on the library
    if (value.trim() && chosenView !== "library" && !onEmulatorPage) showView("library");
  };

  const toggleSidebar = () => {
    setIsSidebarOpen((open) => {
      writeUiPreference(SIDEBAR_PREF_KEY, open ? "closed" : "open");
      return !open;
    });
  };

  const changeInstalledOnly = (next: boolean) => {
    setInstalledOnly(next);
    writeUiPreference(INSTALLED_ONLY_PREF_KEY, next ? "on" : "off");
  };

  /** A tile of the genres or tags wall: the library on that category alone. */
  const openCategory = (kind: CategoryKind, key: string) => {
    setFilters((current) => ({
      ...EMPTY_FILTERS,
      // The tiles were counted on the games being browsed: stay among those
      collection: current.collection === "notInstalled" ? "notInstalled" : "all",
      [kind === "genres" ? "genre" : "tag"]: key,
    }));
    setSearchQuery("");
    showView("library");
  };

  const changeViewMode = (mode: ViewMode) => {
    setViewMode(mode);
    writeUiPreference(VIEW_MODE_PREF_KEY, mode);
  };

  const closeDetail = useCallback(() => setDetailGameId(null), []);
  const closeCommand = useCallback(() => setCommandRun(null), []);
  const closeEdit = useCallback(() => setEditingGameId(null), []);
  const closeThemeEditor = useCallback(() => {
    setThemeEditorBase(null);
    setPreviewTheme(null);
  }, []);

  /* --------------------------------------------------------------- derived */

  // Emulated games have their own section: the library, its counts, genres and tags are
  // A game shows its store's genres and the user's as one list
  const allGames = useMemo(
    () =>
      report.games.map((game) =>
        game.customGenres?.length ? { ...game, genres: mergedGenres(game.genres, game.customGenres) } : game
      ),
    [report.games]
  );
  // about everything else. A hidden emulated game is the exception: "Nascosti" is the one
  // place hidden games can be brought back from, whatever they are.
  const games = useMemo(
    () => allGames.filter((game) => !isEmulated(game) || game.isHidden),
    [allGames]
  );
  const emulatedGames = useMemo(() => allGames.filter(isEmulated), [allGames]);
  // Home is about every game the user plays, whatever runs it
  const homeGames = useMemo(() => allGames.filter((game) => !game.isHidden), [allGames]);
  // The background of Home moves only where things are allowed to move at all
  const animationsChoice = previewAnimations ?? config.animations;
  const motionOff =
    animationsChoice === "off" ||
    (animationsChoice === "theme" && activeTheme.motion.level === "none") ||
    (previewAccessibility ?? config.accessibility).reduceMotion ||
    (typeof matchMedia === "function" && matchMedia("(prefers-reduced-motion: reduce)").matches);
  const showsAmbience = config.home.effects && activeTheme.home.effects === "seasonal" && !motionOff;
  // The weather outside, when the user follows it: asked while Home is in view, and again
  // every half hour. Anything that goes wrong leaves the season to show
  const [weather, setWeather] = useState<WeatherNow | null>(null);
  // DLSS: with the setting on, Kanso asks NVIDIA for its newest version and looks through the
  // games' folders once, in the background, to say at the top how many games are behind.
  // With it off nothing of this runs until the user opens a place that shows DLSS.
  const dlss = useDlssStatus();
  const followsDlss = config.dlssUpdates;
  useEffect(() => {
    if (followsDlss) void loadDlssStatus().catch(() => {});
  }, [followsDlss]);
  const followsWeather = showsAmbience && config.home.weather && config.home.place !== null;
  const weatherPlaceKey = config.home.place ? `${config.home.place.latitude},${config.home.place.longitude}` : "";
  useEffect(() => {
    if (!followsWeather || mainView !== "home") {
      setWeather(null);
      return;
    }
    let current = true;
    const ask = () => {
      homeWeather()
        .then((answer) => current && setWeather(answer))
        .catch(() => current && setWeather(null));
    };
    ask();
    const timer = window.setInterval(ask, 30 * 60 * 1000);
    return () => {
      current = false;
      window.clearInterval(timer);
    };
  }, [followsWeather, weatherPlaceKey, mainView]);
  const homeAmbience = useMemo(
    () =>
      showsAmbience
        ? ambienceFor(
            new Date(),
            // South of the equator the seasons are the other way round: the place says so,
            // or else the time zone of this PC
            config.home.place && config.home.weather
              ? config.home.place.latitude < 0
              : isSouthernTimeZone(Intl.DateTimeFormat().resolvedOptions().timeZone ?? ""),
            weather
          )
        : null,
    [showsAmbience, weather, config.home.place, config.home.weather]
  );
  /** Saves a change to the Home settings made on the page itself (the queue of next games). */
  const changeHome = (next: HomeConfig) => {
    const previous = config;
    onConfigChange({ ...config, home: next });
    saveSettings({ ...config, language: lang, home: next })
      .then(onConfigChange)
      .catch((err) => {
        onConfigChange(previous);
        setStatusMessage(typeof err === "string" ? err : t("settings.saveError"));
      });
  };
  /** Saves a change made on the Store page: a deal liked, or hidden. */
  const changeStore = (next: StoreConfig) => {
    const previous = config;
    onConfigChange({ ...config, store: next });
    saveSettings({ ...config, language: lang, store: next })
      .then(onConfigChange)
      .catch((err) => {
        onConfigChange(previous);
        setStatusMessage(typeof err === "string" ? err : t("settings.saveError"));
      });
  };
  // The way from Home to the Store is there only while the Store has its page
  const storeOffered = navPages.includes("store");
  /** One Store view or another: the page, or what the Home page shows of it. */
  const storeView = (compact?: "picks" | "top") => (
    <StoreView
      games={homeGames}
      ownedSteamAppIds={ownedSteamAppIds}
      store={config.store}
      onChangeStore={changeStore}
      onOpenSettings={() => openSettings("store")}
      onError={setStatusMessage}
      theme={activeTheme.store}
      compact={compact}
      onOpenStore={storeOffered ? () => showView("store") : undefined}
    />
  );
  // What genres, tags, platforms and statuses are counted on: everything, what is on disk
  // ("installed only") or, inside the "not installed" collection, what is not
  const scope = libraryScope(filters.collection, installedOnly);
  const genres = useMemo(() => summarizeGenres(games, scope), [games, scope]);
  const tags = useMemo(() => summarizeTags(games, genres, scope), [games, genres, scope]);
  // Where the user stands with a game is about the game, whatever runs it: the statuses
  // count the emulated games too, and choosing one lists them with the others
  const counts = useMemo(() => {
    const counted = countLibrary(games, installedOnly, scope);
    for (const game of emulatedGames) {
      if (!game.isHidden && game.status && isInScope(game, scope)) counted.byStatus[game.status] += 1;
    }
    return counted;
  }, [games, emulatedGames, installedOnly, scope]);
  const filteredGames = useMemo(
    () =>
      applyLibraryFilters(
        filters.status === null
          ? games
          : allGames.filter((game) => !isEmulated(game) || game.isHidden || game.status === filters.status),
        filters,
        searchQuery,
        installedOnly
      ),
    [games, allGames, filters, searchQuery, installedOnly]
  );
  const scopedGames = useMemo(
    () => games.filter((game) => isInScope(game, scope)),
    [games, scope]
  );
  const ownedSteamAppIds = useMemo(
    () =>
      new Set(
        games
          .filter((g) => g.platform === "steam" && g.platformAppId)
          .map((g) => g.platformAppId as string)
      ),
    [games]
  );

  const detailPlugins = useMemo(
    () => plugins.filter((plugin) => plugin.enabled && plugin.gameDetails),
    [plugins]
  );
  // What the plugins add to Home: asked only while Home is the page on screen and its theme
  // has a place for them (once the installed themes are read: before that the theme on screen
  // may not be the user's), never at startup on another page
  const homePluginParts = useHomePlugins(
    plugins,
    config.home.hidden,
    settingsLoaded && themesLoaded && mainView === "home" && activeTheme.home.sections.includes("plugins")
  );
  const pluginCommands = useMemo<PluginCommandEntry[]>(
    () =>
      plugins
        .filter((plugin) => plugin.enabled)
        .flatMap((plugin) =>
          plugin.commands.map((command) => ({
            pluginId: plugin.id,
            commandId: command.id,
            title: command.title,
            description: command.description,
          }))
        ),
    [plugins]
  );

  // A genre filter whose last game was edited or uninstalled would show an empty library
  const activeGenre = genres.find((g) => g.key === filters.genre) ?? null;
  useEffect(() => {
    if (filters.genre && !activeGenre && games.length > 0) {
      setFilters((f) => ({ ...f, genre: null }));
    }
  }, [filters.genre, activeGenre, games.length]);

  // Same for a tag (tags also change with the interface language)
  const activeTag = tags.find((tag) => tag.key === filters.tag) ?? null;
  useEffect(() => {
    if (filters.tag && !activeTag && games.length > 0) {
      setFilters((f) => ({ ...f, tag: null }));
    }
  }, [filters.tag, activeTag, games.length]);

  // Showing the last hidden game again empties that pool: go back to the library
  useEffect(() => {
    if (filters.collection === "hidden" && counts.hidden === 0) {
      setFilters(EMPTY_FILTERS);
    }
  }, [filters.collection, counts.hidden]);

  // Same when the last game that was not on disk gets installed (or hidden)
  useEffect(() => {
    if (filters.collection === "notInstalled" && counts.notInstalled === 0 && games.length > 0) {
      setFilters(EMPTY_FILTERS);
    }
  }, [filters.collection, counts.notInstalled, games.length]);

  const detailGame = useMemo(
    () => allGames.find((g) => g.id === detailGameId) ?? null,
    [allGames, detailGameId]
  );
  const editingGame = useMemo(
    // The editor works on the store's genres alone: the user's own are kept apart
    () => report.games.find((g) => g.id === editingGameId) ?? null,
    [report.games, editingGameId]
  );

  // A game that is not on disk arrives with name and cover only: the rest is looked up the
  // first time its sheet is opened (once per session, whether or not anything is found)
  const completedIds = useRef(new Set<string>());
  useEffect(() => {
    const game = detailGame;
    if (
      !game ||
      game.isInstalled ||
      game.shortDescription ||
      game.genres.length > 0 ||
      completedIds.current.has(game.id)
    ) {
      return;
    }
    completedIds.current.add(game.id);
    completeGameDetails(game)
      .then(upsertSingleGameInState)
      .catch(() => {
        // The sheet simply stays as it is; opening it in a later session tries again
      });
  }, [detailGame, upsertSingleGameInState]);

  /**
   * Where a genre or tag picked on `game`'s sheet is listed: with "installed only" on, a game
   * that is not on disk leads to the others like it, or it would not be among the results.
   */
  const collectionOf = (game: DetectedGame | null): LibraryCollection =>
    game && !game.isInstalled && installedOnly ? "notInstalled" : "all";

  const hasApiKey = config.steamgriddbApiKey.trim().length > 0;
  const filterCount = activeFilterCount(filters);
  const isNarrowed = hasActiveFilters(filters) || searchQuery.trim().length > 0;
  // What "N of M" is measured against: the pool the current view draws from
  const poolSize =
    filters.collection === "hidden"
      ? counts.hidden
      : filters.collection === "notInstalled"
        ? counts.notInstalled
        : counts.visible;
  // Games the search or the filters would also find, were "installed only" switched off
  const absentMatches = useMemo(
    () =>
      installedOnly && isNarrowed && scope === "installed"
        ? applyLibraryFilters(games, filters, searchQuery, false).length - filteredGames.length
        : 0,
    [installedOnly, isNarrowed, scope, games, filters, searchQuery, filteredGames.length]
  );

  return (
    <div className="flex h-full flex-col text-zinc-100">
      <Header
        searchQuery={searchQuery}
        onSearchChange={changeSearch}
        isSidebarOpen={isSidebarOpen}
        onToggleSidebar={toggleSidebar}
        activeFilterCount={mainView === "library" ? filterCount : 0}
        viewMode={viewMode}
        onViewModeChange={changeViewMode}
        showViewMode={mainView === "library" || (mainView === "emulators" && openEmulatorId !== null)}
        isScanning={isScanning}
        isSyncingMetadata={isSyncingMetadata}
        hasApiKey={hasApiKey}
        overlayDepth={
          Number(Boolean(detailGame)) +
          Number(Boolean(editingGame)) +
          Number(isSettingsOpen) +
          Number(Boolean(commandRun))
        }
        sheetOpen={Boolean(detailGame)}
        settingsDisabled={themeEditorBase !== null}
        onScan={() => void performScan(true)}
        onOpenSettings={() => openSettings()}
        dlssUpdates={config.dlssUpdates ? gamesToUpdate(dlss) : 0}
        onOpenDlss={() => openSettings("dlss")}
        pluginCommands={pluginCommands}
        onRunPluginCommand={(command) => void startCommand(command)}
      />

      {settingsLoaded && !hasApiKey && !bannerDismissed && (
        <ApiKeyBanner
          onConfigure={() => openSettings("library")}
          onDismiss={() => setBannerDismissed(true)}
        />
      )}

      <div className="app-body flex min-h-0 flex-1">
        <Sidebar
          isOpen={isSidebarOpen}
          view={mainView}
          onOpenHome={() => showView("home")}
          onOpenStore={() => showView("store")}
          onOpenNews={() => showView("news")}
          storeAlerts={likedEndingSoon(config.store.liked).length}
          onOpenStats={() => showView("stats")}
          onOpenEmulators={() => {
            // The entry always leads to the wall of emulators, also from an emulator's page
            setOpenEmulatorId(null);
            setSearchQuery("");
            showView("emulators");
          }}
          emulatorCount={emulators.length}
          onOpenOverview={showView}
          filters={filters}
          onFiltersChange={changeFilters}
          installedOnly={installedOnly}
          onInstalledOnlyChange={changeInstalledOnly}
          counts={counts}
          genres={genres}
          tags={tags}
          nav={activeTheme.nav}
          pages={navPages}
          hidden={config.hiddenSections}
        />

        <main className="app-main scroll-area fade-top min-w-0 flex-1 space-y-4 overflow-y-auto">
          {!isRunningInTauri() && (
            <div className="rounded-lg border border-oled-border bg-oled-surface/70 px-4 py-2 text-xs text-zinc-400">
              {t("app.previewNotice")}
            </div>
          )}

          {/* Another entry of the side panel: the content is new, and comes in as such */}
          <div key={viewKey} className="motion-view space-y-4">
          {mainView === "home" ? (
            <HomeView
              games={homeGames}
              home={config.home}
              theme={activeTheme.home}
              themeKey={`${activeTheme.id}|${activeTheme.colors.accent}|${activeTheme.colors.accentSecondary}`}
              ambience={homeAmbience}
              weatherPlace={weather && config.home.place ? config.home.place.name : null}
              onOpenWeatherSource={() => {
                openWeatherSource().catch((err) =>
                  setStatusMessage(err instanceof Error ? err.message : typeof err === "string" ? err : t("app.statusError"))
                );
              }}
              onChangeHome={changeHome}
              onOpenGame={(game) => setDetailGameId(game.id)}
              onLaunch={handleLaunchGame}
              launchingId={launchingId}
              onOpenSettings={() => openSettings("home")}
              deals={storeView("top")}
              recommendations={storeView("picks")}
              pluginParts={homePluginParts}
            />
          ) : mainView === "store" ? (
            storeView()
          ) : mainView === "news" ? (
            // Not before the settings are in: the sources to ask are theirs to say
            settingsLoaded && <NewsView
              theme={activeTheme.news}
              sourcesKey={`${lang}|${config.newsSources === null ? "default" : config.newsSources.join(",")}|${config.newsFeeds.map((feed) => `${feed.url} ${feed.topic} ${feed.name}`).join(",")}`}
              onOpenSettings={() => openSettings("news")}
              onError={setStatusMessage}
            />
          ) : mainView === "stats" ? (
            <StatsView
              games={allGames}
              onOpenGame={(game) => setDetailGameId(game.id)}
              onError={setStatusMessage}
            />
          ) : mainView === "emulators" ? (
            <EmulatorsView
              emulators={emulators}
              games={emulatedGames}
              openId={openEmulatorId}
              onOpen={(id) => {
                setSearchQuery("");
                setOpenEmulatorId(id);
              }}
              onLaunchEmulator={(emulator) => {
                setStatusMessage(null);
                launchEmulator(emulator).catch((err) =>
                  setStatusMessage(
                    isRunningInTauri()
                      ? typeof err === "string"
                        ? err
                        : t("emulators.launchError", { name: emulator.name })
                      : t("emulators.launchPreview", { name: emulator.name })
                  )
                );
              }}
              onOpenSettings={() => openSettings("emulators")}
              onSearch={() => void performScan(true)}
              searchQuery={searchQuery}
              viewMode={viewMode}
              isScanning={isScanning}
              launchingId={launchingId}
              onOpenDetail={(game) => setDetailGameId(game.id)}
              onLaunch={handleLaunchGame}
              onToggleFavorite={handleToggleFavorite}
              badges={badges}
              runningIds={runningIds}
              cardInfo={config.cardInfo}
            />
          ) : mainView === "genres" || mainView === "tags" ? (
            <CategoryGrid
              kind={mainView}
              categories={mainView === "genres" ? genres : tags}
              games={scopedGames}
              onOpen={(key) => openCategory(mainView, key)}
            />
          ) : (
            <>
              {/* Active filters: always visible, even with the sidebar collapsed */}
              {isNarrowed && games.length > 0 && (
                <div className="flex flex-wrap items-center gap-2 text-xs text-zinc-500">
                  {/* The way back to the tiles the genre or tag was picked from */}
                  {(activeGenre ? navGroups.includes("genres") : activeTag !== null && navGroups.includes("tags")) && (
                    <button
                      id="back-to-overview"
                      type="button"
                      onClick={() => showView(activeGenre ? "genres" : "tags")}
                      className="inline-flex items-center gap-1.5 rounded-full py-0.5 pr-1 text-2xs font-medium text-zinc-400 transition-colors hover:text-zinc-100"
                    >
                      <LayoutGrid className="h-3 w-3" />
                      <span>{t(activeGenre ? "overview.allGenres" : "overview.allTags")}</span>
                    </button>
                  )}
                  {filters.collection !== "all" && (
                    <FilterChip
                      label={t(COLLECTION_KEYS[filters.collection])}
                      icon={
                        filters.collection === "favorites" ? (
                          <Star className="h-3 w-3 fill-accent text-accent" />
                        ) : filters.collection === "recent" ? (
                          <History className="h-3 w-3 text-zinc-400" />
                        ) : filters.collection === "notInstalled" ? (
                          <CloudDownload className="h-3 w-3 text-zinc-400" />
                        ) : (
                          <EyeOff className="h-3 w-3 text-zinc-400" />
                        )
                      }
                      onRemove={() => setFilters((f) => ({ ...f, collection: "all" }))}
                    />
                  )}
                  {filters.platform && (
                    <FilterChip
                      label={platformLabel(filters.platform, t)}
                      onRemove={() => setFilters((f) => ({ ...f, platform: null }))}
                    />
                  )}
                  {activeGenre && (
                    <FilterChip
                      label={activeGenre.label}
                      onRemove={() => setFilters((f) => ({ ...f, genre: null }))}
                    />
                  )}
                  {activeTag && (
                    <FilterChip
                      label={activeTag.label}
                      onRemove={() => setFilters((f) => ({ ...f, tag: null }))}
                    />
                  )}
                  {filters.status && (
                    <FilterChip
                      label={t(STATUS_LABEL_KEYS[filters.status])}
                      icon={<StatusIcon status={filters.status} className="h-3 w-3 text-zinc-400" />}
                      onRemove={() => setFilters((f) => ({ ...f, status: null }))}
                    />
                  )}
                  <span className="tabular-nums">
                    {t("filters.count", {
                      shown: filteredGames.length,
                      total: poolSize,
                    })}
                  </span>
                  {absentMatches > 0 && (
                    <button
                      id="show-absent-matches"
                      type="button"
                      onClick={() => changeInstalledOnly(false)}
                      title={t("filters.absentMatchesHint")}
                      className="tabular-nums text-zinc-400 underline-offset-2 hover:text-zinc-200 hover:underline"
                    >
                      {plural("filters.absentMatches", absentMatches)}
                    </button>
                  )}
                  {filterCount > 1 && (
                    <button
                      type="button"
                      onClick={() => setFilters(EMPTY_FILTERS)}
                      className="text-zinc-400 underline-offset-2 hover:text-zinc-200 hover:underline"
                    >
                      {t("filters.reset")}
                    </button>
                  )}
                </div>
              )}

              <GameList
                games={filteredGames}
                totalDetected={games.length}
                viewMode={viewMode}
                isScanning={isScanning}
                launchingId={launchingId}
                warnings={report.warnings}
                onOpenDetail={(game) => setDetailGameId(game.id)}
                onLaunch={handleLaunchGame}
                onToggleFavorite={handleToggleFavorite}
                onOpenSettings={() => openSettings("library")}
                badges={badges}
                runningIds={runningIds}
                cardInfo={config.cardInfo}
              />
            </>
          )}
          </div>
        </main>
      </div>

      <GameDetail
        game={detailGame}
        layout={activeTheme.detail.layout}
        cards={activeTheme.detail.cards}
        emulatorName={
          detailGame?.platform === "emulator"
            ? (emulators.find((emulator) => emulator.id === detailGame.platformAppId)?.name ?? null)
            : null
        }
        isLaunching={launchingId !== null && launchingId === detailGameId}
        isTopmost={!editingGame && !isSettingsOpen}
        onClose={closeDetail}
        onLaunch={handleLaunchGame}
        onToggleFavorite={handleToggleFavorite}
        onToggleHidden={handleToggleHidden}
        onEdit={(game) => setEditingGameId(game.id)}
        onSelectGenre={(key) => {
          setFilters({ ...EMPTY_FILTERS, collection: collectionOf(detailGame), genre: key });
          setSearchQuery("");
          showView("library");
          closeDetail();
        }}
        onSelectTag={(key) => {
          setFilters({ ...EMPTY_FILTERS, collection: collectionOf(detailGame), tag: key });
          setSearchQuery("");
          showView("library");
          closeDetail();
        }}
        isRunning={detailGame !== null && runningIds.has(detailGame.id)}
        onSetStatus={handleSetStatus}
        onSetGenres={handleSetGenres}
        onUninstall={(game) => {
          uninstallGame(game)
            .then(() =>
              setStatusMessage(
                t(uninstallMode(game) === "uninstall" ? "app.uninstallSent" : "app.uninstallOpened", {
                  client: game.platform === "xbox" ? "Windows" : platformLabel(game.platform, t),
                })
              )
            )
            .catch((err) =>
              setStatusMessage(typeof err === "string" ? err : err instanceof Error ? err.message : t("app.statusError"))
            );
        }}
        onOpenFolder={(game) => {
          openGameFolder(game).catch((err) =>
            setStatusMessage(typeof err === "string" ? err : err instanceof Error ? err.message : t("app.statusError"))
          );
        }}
        knownGenres={genres.map((genre) => genre.label)}
        plugins={detailPlugins}
      />

      <PluginCommandDialog
        run={commandRun}
        games={allGames}
        isTopmost={!detailGame && !editingGame && !isSettingsOpen}
        onClose={closeCommand}
        onRunAgain={() => {
          if (commandRun) void startCommand(commandRun);
        }}
        onOpenGame={(game) => {
          closeCommand();
          setDetailGameId(game.id);
        }}
        onLaunch={handleLaunchGame}
      />

      {/* Errors float above every dialog: a failed launch from the detail view stays readable.
          Fixed colors: the toast must stand out on any theme. */}
      {statusMessage && (
        <div
          role="alert"
          className="fixed bottom-5 left-1/2 z-[60] flex w-[min(40rem,calc(100vw-2rem))] -translate-x-1/2 items-start justify-between gap-4 rounded-lg border border-red-500/40 bg-red-950 px-4 py-3 text-xs leading-relaxed text-red-100 shadow-2xl"
        >
          <span className="min-w-0 select-text break-words">{statusMessage}</span>
          <button
            type="button"
            onClick={() => setStatusMessage(null)}
            className="shrink-0 font-medium text-red-100 underline-offset-2 hover:underline"
          >
            {t("common.close")}
          </button>
        </div>
      )}

      <SettingsModal
        isOpen={isSettingsOpen}
        initialSection={settingsSection}
        initialConfig={config}
        currentLanguage={lang}
        userThemes={userThemes}
        onClose={() => setIsSettingsOpen(false)}
        onSave={handleSaveSettings}
        onThemesChanged={reloadThemes}
        onPreviewTheme={setPreviewTheme}
        onPreviewAccessibility={setPreviewAccessibility}
        onPreviewAnimations={setPreviewAnimations}
        onCustomizeTheme={(theme) => {
          // The editor is a side panel: close the dialog so the app shows the draft live
          setIsSettingsOpen(false);
          setPreviewAccessibility(null);
          setPreviewTheme(theme);
          setThemeEditorBase(theme);
        }}
        plugins={plugins}
        pluginWarnings={pluginWarnings}
        onPluginsChanged={async (saved) => {
          onConfigChange(saved);
          await reloadPlugins();
        }}
        onStoreSignedIn={() => void completeLibraryInBackground().catch(() => {})}
      />

      <ThemeEditor
        baseTheme={themeEditorBase}
        onPreview={setPreviewTheme}
        onClose={closeThemeEditor}
        onSave={async (draft, replaceId) => {
          const saved = await saveTheme(draft, replaceId);
          await reloadThemes();
          // A theme the user just made is the one they want to use
          onConfigChange(await saveSettings({ ...config, language: lang, themeId: saved.id }));
          closeThemeEditor();
        }}
      />

      <EditGameModal
        game={editingGame}
        hasApiKey={hasApiKey}
        onClose={closeEdit}
        onGameUpdated={upsertSingleGameInState}
        onOpenSettings={() => openSettings("library")}
      />
    </div>
  );
}

export default App;
