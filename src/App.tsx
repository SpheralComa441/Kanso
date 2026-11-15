import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { CloudDownload, EyeOff, History, LayoutGrid, Star, X } from "lucide-react";
import { ApiKeyBanner } from "./components/ApiKeyBanner";
import { CategoryGrid, type CategoryKind } from "./components/CategoryGrid";
import { DealsView } from "./components/DealsView";
import { EditGameModal } from "./components/EditGameModal";
import { GameDetail } from "./components/GameDetail";
import { GameList } from "./components/GameList";
import { Header, type PluginCommandEntry, type ViewMode } from "./components/Header";
import {
  PluginCommandDialog,
  type PluginCommandRun,
} from "./components/PluginCommandDialog";
import { SettingsModal } from "./components/SettingsModal";
import { Sidebar, type MainView } from "./components/Sidebar";
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
  launchGame,
  listPlugins,
  listThemes,
  pluginBadges,
  runPluginCommand,
  saveSettings,
  saveTheme,
  scanGames,
  setGameFavorite,
  setGameHidden,
  setGameStatus,
  subscribeToGameBatchUpdates,
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
import { platformLabel } from "./lib/platforms";
import { DEFAULT_ACCESSIBILITY, applyTheme, resolveTheme } from "./lib/themes";
import type {
  AccessibilityConfig,
  AppConfig,
  BadgeEntry,
  DetectedGame,
  GameStatus,
  PluginInfo,
  ScanReport,
  Theme,
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
  const [detailGameId, setDetailGameId] = useState<string | null>(null);
  const [editingGameId, setEditingGameId] = useState<string | null>(null);
  const [bannerDismissed, setBannerDismissed] = useState(false);
  const [searchQuery, setSearchQuery] = useState("");
  const [filters, setFilters] = useState<LibraryFilters>(EMPTY_FILTERS);
  const [mainView, setMainView] = useState<MainView>(() =>
    readUiPreference(
      MAIN_VIEW_PREF_KEY,
      ["home", "library", "stats", "genres", "tags"],
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
  /** Theme shown but not saved: a selection in Settings or a draft in the editor. */
  const [previewTheme, setPreviewTheme] = useState<Theme | null>(null);
  const [themeEditorBase, setThemeEditorBase] = useState<Theme | null>(null);
  /** Accessibility choices being tried in Settings, before they are saved. */
  const [previewAccessibility, setPreviewAccessibility] =
    useState<AccessibilityConfig | null>(null);
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
    void reloadThemes();
  }, [reloadThemes]);

  const savedTheme = useMemo(
    () => resolveTheme(config.themeId, userThemes),
    [config.themeId, userThemes]
  );
  useEffect(() => {
    applyTheme(previewTheme ?? savedTheme, previewAccessibility ?? config.accessibility);
  }, [previewTheme, savedTheme, previewAccessibility, config.accessibility]);

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
        const res = await scanGames();
        setReport(res);
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

  // First scan once the settings (and with them the backend language) are in place
  useEffect(() => {
    if (!settingsLoaded) return;
    let mounted = true;
    let unlistenFn: (() => void) | null = null;
    let unlistenBatchFn: (() => void) | null = null;

    (async () => {
      unlistenFn = await subscribeToGameUpdates((updated) => {
        if (mounted) upsertSingleGameInState(updated);
      });
      unlistenBatchFn = await subscribeToGameBatchUpdates((updated) => {
        if (mounted) upsertGamesInState(updated);
      });
      if (mounted) await performScan(false);
    })();

    return () => {
      mounted = false;
      if (unlistenFn) unlistenFn();
      if (unlistenBatchFn) unlistenBatchFn();
    };
  }, [settingsLoaded, performScan, upsertSingleGameInState, upsertGamesInState]);

  const handleSaveSettings = async (updated: AppConfig) => {
    const foldersChanged =
      JSON.stringify(updated.customFolders) !== JSON.stringify(config.customFolders);
    const apiKeyChanged =
      Boolean(updated.steamgriddbApiKey.trim()) &&
      updated.steamgriddbApiKey.trim() !== config.steamgriddbApiKey.trim();
    const languageChanged = updated.language !== lang;

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

  /* ------------------------------------------------------------- navigation */

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
    // The search box searches the library, wherever it is typed from
    if (value.trim() && mainView !== "library") showView("library");
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

  const games = report.games;
  // What genres, tags, platforms and statuses are counted on: everything, what is on disk
  // ("installed only") or, inside the "not installed" collection, what is not
  const scope = libraryScope(filters.collection, installedOnly);
  const genres = useMemo(() => summarizeGenres(games, scope), [games, scope]);
  const tags = useMemo(() => summarizeTags(games, genres, scope), [games, genres, scope]);
  const counts = useMemo(
    () => countLibrary(games, installedOnly, scope),
    [games, installedOnly, scope]
  );
  const filteredGames = useMemo(
    () => applyLibraryFilters(games, filters, searchQuery, installedOnly),
    [games, filters, searchQuery, installedOnly]
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
    () => games.find((g) => g.id === detailGameId) ?? null,
    [games, detailGameId]
  );
  const editingGame = useMemo(
    () => games.find((g) => g.id === editingGameId) ?? null,
    [games, editingGameId]
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
        totalGames={counts.visible}
        searchQuery={searchQuery}
        onSearchChange={changeSearch}
        isSidebarOpen={isSidebarOpen}
        onToggleSidebar={toggleSidebar}
        activeFilterCount={mainView === "library" ? filterCount : 0}
        viewMode={viewMode}
        onViewModeChange={changeViewMode}
        showViewMode={mainView === "library"}
        isScanning={isScanning}
        isSyncingMetadata={isSyncingMetadata}
        hasApiKey={hasApiKey}
        overlayDepth={
          Number(Boolean(detailGame)) +
          Number(Boolean(editingGame)) +
          Number(isSettingsOpen) +
          Number(Boolean(commandRun))
        }
        settingsDisabled={themeEditorBase !== null}
        onScan={() => void performScan(true)}
        onOpenSettings={() => setIsSettingsOpen(true)}
        pluginCommands={pluginCommands}
        onRunPluginCommand={(command) => void startCommand(command)}
      />

      {settingsLoaded && !hasApiKey && !bannerDismissed && (
        <ApiKeyBanner
          onConfigure={() => setIsSettingsOpen(true)}
          onDismiss={() => setBannerDismissed(true)}
        />
      )}

      <div className="app-body flex min-h-0 flex-1">
        <Sidebar
          isOpen={isSidebarOpen}
          view={mainView}
          onOpenHome={() => showView("home")}
          onOpenStats={() => showView("stats")}
          onOpenOverview={showView}
          filters={filters}
          onFiltersChange={changeFilters}
          installedOnly={installedOnly}
          onInstalledOnlyChange={changeInstalledOnly}
          counts={counts}
          genres={genres}
          tags={tags}
        />

        <main className="app-main scroll-area fade-top min-w-0 flex-1 space-y-4 overflow-y-auto">
          {!isRunningInTauri() && (
            <div className="rounded-lg border border-oled-border bg-oled-surface/70 px-4 py-2 text-xs text-zinc-400">
              {t("app.previewNotice")}
            </div>
          )}

          {mainView === "home" ? (
            <DealsView
              ownedSteamAppIds={ownedSteamAppIds}
              onError={setStatusMessage}
            />
          ) : mainView === "stats" ? (
            <StatsView
              games={games}
              onOpenGame={(game) => setDetailGameId(game.id)}
              onError={setStatusMessage}
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
                  {(activeGenre || activeTag) && (
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
                onOpenSettings={() => setIsSettingsOpen(true)}
                badges={badges}
                runningIds={runningIds}
              />
            </>
          )}
        </main>
      </div>

      <GameDetail
        game={detailGame}
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
        plugins={detailPlugins}
      />

      <PluginCommandDialog
        run={commandRun}
        games={games}
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
        initialConfig={config}
        currentLanguage={lang}
        userThemes={userThemes}
        onClose={() => setIsSettingsOpen(false)}
        onSave={handleSaveSettings}
        onThemesChanged={reloadThemes}
        onPreviewTheme={setPreviewTheme}
        onPreviewAccessibility={setPreviewAccessibility}
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
        onOpenSettings={() => setIsSettingsOpen(true)}
      />
    </div>
  );
}

export default App;
