import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { EyeOff, History, Star, X } from "lucide-react";
import { ApiKeyBanner } from "./components/ApiKeyBanner";
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
  getSettings,
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

const COLLECTION_KEYS: Record<LibraryCollection, MessageKey> = {
  all: "collection.all",
  favorites: "collection.favorites",
  recent: "collection.recent",
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
  const { t, lang } = useI18n();
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
    readUiPreference(MAIN_VIEW_PREF_KEY, ["home", "library", "stats"], "library")
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

    (async () => {
      unlistenFn = await subscribeToGameUpdates((updated) => {
        if (mounted) upsertSingleGameInState(updated);
      });
      if (mounted) await performScan(false);
    })();

    return () => {
      mounted = false;
      if (unlistenFn) unlistenFn();
    };
  }, [settingsLoaded, performScan, upsertSingleGameInState]);

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

  const handleLaunchGame = async (game: DetectedGame) => {
    setLaunchingId(game.id);
    setStatusMessage(null);
    try {
      if (!isRunningInTauri()) throw t("app.launchPreview");
      await launchGame(game);
    } catch (err) {
      setStatusMessage(
        typeof err === "string" ? err : t("app.launchError", { title: game.title })
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
  const genres = useMemo(() => summarizeGenres(games), [games]);
  const tags = useMemo(() => summarizeTags(games, genres), [games, genres]);
  const counts = useMemo(() => countLibrary(games), [games]);
  const filteredGames = useMemo(
    () => applyLibraryFilters(games, filters, searchQuery),
    [games, filters, searchQuery]
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

  const detailGame = useMemo(
    () => games.find((g) => g.id === detailGameId) ?? null,
    [games, detailGameId]
  );
  const editingGame = useMemo(
    () => games.find((g) => g.id === editingGameId) ?? null,
    [games, editingGameId]
  );

  const hasApiKey = config.steamgriddbApiKey.trim().length > 0;
  const filterCount = activeFilterCount(filters);
  const isNarrowed = hasActiveFilters(filters) || searchQuery.trim().length > 0;
  // What "N of M" is measured against: the hidden pool or the visible library
  const poolSize = filters.collection === "hidden" ? counts.hidden : counts.visible;

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
          filters={filters}
          onFiltersChange={changeFilters}
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
          ) : (
            <>
              {/* Active filters: always visible, even with the sidebar collapsed */}
              {isNarrowed && games.length > 0 && (
                <div className="flex flex-wrap items-center gap-2 text-xs text-zinc-500">
                  {filters.collection !== "all" && (
                    <FilterChip
                      label={t(COLLECTION_KEYS[filters.collection])}
                      icon={
                        filters.collection === "favorites" ? (
                          <Star className="h-3 w-3 fill-accent text-accent" />
                        ) : filters.collection === "recent" ? (
                          <History className="h-3 w-3 text-zinc-400" />
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
          setFilters({ ...EMPTY_FILTERS, genre: key });
          setSearchQuery("");
          showView("library");
          closeDetail();
        }}
        onSelectTag={(key) => {
          setFilters({ ...EMPTY_FILTERS, tag: key });
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
