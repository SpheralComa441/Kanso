import { useCallback, useEffect, useMemo, useState } from "react";
import { EyeOff, History, Star, X } from "lucide-react";
import { ApiKeyBanner } from "./components/ApiKeyBanner";
import { EditGameModal } from "./components/EditGameModal";
import { GameDetail } from "./components/GameDetail";
import { GameList } from "./components/GameList";
import { Header, type ViewMode } from "./components/Header";
import { SettingsModal } from "./components/SettingsModal";
import { Sidebar } from "./components/Sidebar";
import {
  getSettings,
  isRunningInTauri,
  launchGame,
  saveSettings,
  scanGames,
  setGameFavorite,
  setGameHidden,
  subscribeToGameUpdates,
  syncLibraryMetadata,
} from "./lib/backend";
import {
  COLLECTION_LABELS,
  EMPTY_FILTERS,
  activeFilterCount,
  applyLibraryFilters,
  countLibrary,
  hasActiveFilters,
  readUiPreference,
  sortGamesByTitle,
  summarizeGenres,
  writeUiPreference,
  type LibraryFilters,
} from "./lib/library";
import { PLATFORM_META } from "./lib/platforms";
import type { AppConfig, DetectedGame, ScanReport } from "./types/game";

const INITIAL_CONFIG: AppConfig = {
  steamgriddbApiKey: "",
  customFolders: [],
  minimizeToTrayOnLaunch: false,
};

const SIDEBAR_PREF_KEY = "kanso.ui.sidebar";
const VIEW_MODE_PREF_KEY = "kanso.ui.viewMode";

interface FilterChipProps {
  label: string;
  onRemove: () => void;
  icon?: React.ReactNode;
}

function FilterChip({ label, onRemove, icon }: FilterChipProps) {
  return (
    <button
      type="button"
      onClick={onRemove}
      title={`Rimuovi filtro: ${label}`}
      className="group inline-flex items-center gap-1.5 rounded-full border border-oled-border bg-oled-surface py-0.5 pl-2.5 pr-1.5 text-[11px] font-medium text-zinc-200 transition-colors hover:border-zinc-600"
    >
      {icon}
      <span>{label}</span>
      <X className="h-3 w-3 text-zinc-500 group-hover:text-zinc-200" />
    </button>
  );
}

export function App() {
  const [config, setConfig] = useState<AppConfig>(INITIAL_CONFIG);
  const [report, setReport] = useState<ScanReport>({
    games: [],
    warnings: [],
  });
  const [isScanning, setIsScanning] = useState(true);
  const [isSyncingMetadata, setIsSyncingMetadata] = useState(false);
  const [isSettingsOpen, setIsSettingsOpen] = useState(false);
  const [detailGameId, setDetailGameId] = useState<string | null>(null);
  const [editingGameId, setEditingGameId] = useState<string | null>(null);
  const [bannerDismissed, setBannerDismissed] = useState(false);
  const [searchQuery, setSearchQuery] = useState("");
  const [filters, setFilters] = useState<LibraryFilters>(EMPTY_FILTERS);
  const [isSidebarOpen, setIsSidebarOpen] = useState(
    () => readUiPreference(SIDEBAR_PREF_KEY, ["open", "closed"], "open") === "open"
  );
  const [viewMode, setViewMode] = useState<ViewMode>(() =>
    readUiPreference(VIEW_MODE_PREF_KEY, ["grid", "list"], "grid")
  );
  const [launchingId, setLaunchingId] = useState<string | null>(null);
  const [statusMessage, setStatusMessage] = useState<string | null>(null);

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
      setReport((prev) => ({
        ...syncedReport,
        warnings: prev.warnings,
      }));
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
  const performScan = useCallback(async (userRequested: boolean) => {
    setIsScanning(true);
    setStatusMessage(null);
    try {
      const res = await scanGames();
      setReport(res);
      void runMetadataSync(userRequested);
    } catch (err) {
      setStatusMessage(
        err instanceof Error
          ? err.message
          : "Errore imprevisto durante la scansione dei giochi."
      );
    } finally {
      setIsScanning(false);
    }
  }, [runMetadataSync]);

  useEffect(() => {
    let mounted = true;
    let unlistenFn: (() => void) | null = null;

    (async () => {
      unlistenFn = await subscribeToGameUpdates((updated) => {
        if (mounted) {
          upsertSingleGameInState(updated);
        }
      });

      try {
        const loaded = await getSettings();
        if (mounted) setConfig(loaded);
      } catch {
        // Proceed with defaults
      }

      if (mounted) {
        await performScan(false);
      }
    })();

    return () => {
      mounted = false;
      if (unlistenFn) unlistenFn();
    };
  }, [performScan, upsertSingleGameInState]);

  const handleSaveSettings = async (updated: AppConfig) => {
    const foldersChanged =
      JSON.stringify(updated.customFolders) !==
      JSON.stringify(config.customFolders);
    const apiKeyChanged =
      Boolean(updated.steamgriddbApiKey.trim()) &&
      updated.steamgriddbApiKey.trim() !== config.steamgriddbApiKey.trim();

    const saved = await saveSettings(updated);
    setConfig(saved);

    if (foldersChanged) {
      await performScan(true);
    } else if (apiKeyChanged) {
      // A new key can find covers that earlier attempts could not
      void runMetadataSync(true);
    }
  };

  const handleLaunchGame = async (game: DetectedGame) => {
    setLaunchingId(game.id);
    setStatusMessage(null);
    try {
      await launchGame(game);
    } catch (err) {
      setStatusMessage(
        typeof err === "string"
          ? err
          : err instanceof Error
            ? err.message
            : `Impossibile avviare ${game.title}.`
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
      setStatusMessage(
        typeof err === "string"
          ? err
          : "Impossibile aggiornare i preferiti. Riprova."
      );
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
      setStatusMessage(
        typeof err === "string"
          ? err
          : "Impossibile aggiornare la visibilità del gioco. Riprova."
      );
    }
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
  const closeEdit = useCallback(() => setEditingGameId(null), []);

  const games = report.games;
  const genres = useMemo(() => summarizeGenres(games), [games]);
  const counts = useMemo(() => countLibrary(games), [games]);
  const filteredGames = useMemo(
    () => applyLibraryFilters(games, filters, searchQuery),
    [games, filters, searchQuery]
  );

  // A genre filter whose last game was edited or uninstalled would show an empty library
  const activeGenre = genres.find((g) => g.key === filters.genre) ?? null;
  useEffect(() => {
    if (filters.genre && !activeGenre && games.length > 0) {
      setFilters((f) => ({ ...f, genre: null }));
    }
  }, [filters.genre, activeGenre, games.length]);

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
  const isNarrowed =
    hasActiveFilters(filters) || searchQuery.trim().length > 0;
  // What "N di M" is measured against: the hidden pool or the visible library
  const poolSize =
    filters.collection === "hidden" ? counts.hidden : counts.visible;

  // Leaving the last hidden game visible again empties the pool: go back to the library
  useEffect(() => {
    if (filters.collection === "hidden" && counts.hidden === 0) {
      setFilters(EMPTY_FILTERS);
    }
  }, [filters.collection, counts.hidden]);

  return (
    <div className="flex h-full flex-col bg-oled-bg text-zinc-100">
      <Header
        totalGames={counts.visible}
        searchQuery={searchQuery}
        onSearchChange={setSearchQuery}
        isSidebarOpen={isSidebarOpen}
        onToggleSidebar={toggleSidebar}
        activeFilterCount={filterCount}
        viewMode={viewMode}
        onViewModeChange={changeViewMode}
        isScanning={isScanning}
        isSyncingMetadata={isSyncingMetadata}
        hasApiKey={hasApiKey}
        onScan={() => void performScan(true)}
        onOpenSettings={() => setIsSettingsOpen(true)}
      />

      {!hasApiKey && !bannerDismissed && (
        <ApiKeyBanner
          onConfigure={() => setIsSettingsOpen(true)}
          onDismiss={() => setBannerDismissed(true)}
        />
      )}

      <div className="flex min-h-0 flex-1">
        <Sidebar
          isOpen={isSidebarOpen}
          filters={filters}
          onFiltersChange={setFilters}
          counts={counts}
          genres={genres}
        />

        <main className="min-w-0 flex-1 space-y-4 overflow-y-auto px-6 py-5">
          {!isRunningInTauri() && (
            <div className="rounded-lg border border-oled-border bg-oled-surface/70 px-4 py-2 text-xs text-zinc-400">
              Modalità anteprima browser attiva — i dati mostrati sono
              simulati. Nel binario Windows (
              <code className="text-zinc-300">.exe</code>) Kanso scansiona
              realmente registro Steam, Epic Games, Battle.net e cartelle
              locali.
            </div>
          )}

          {/* Active filters: always visible, even with the sidebar collapsed */}
          {isNarrowed && games.length > 0 && (
            <div className="flex flex-wrap items-center gap-2 text-xs text-zinc-500">
              {filters.collection !== "all" && (
                <FilterChip
                  label={COLLECTION_LABELS[filters.collection]}
                  icon={
                    filters.collection === "favorites" ? (
                      <Star className="h-3 w-3 fill-accent text-accent" />
                    ) : filters.collection === "recent" ? (
                      <History className="h-3 w-3 text-zinc-400" />
                    ) : (
                      <EyeOff className="h-3 w-3 text-zinc-400" />
                    )
                  }
                  onRemove={() =>
                    setFilters((f) => ({ ...f, collection: "all" }))
                  }
                />
              )}
              {filters.platform && (
                <FilterChip
                  label={PLATFORM_META[filters.platform].label}
                  onRemove={() => setFilters((f) => ({ ...f, platform: null }))}
                />
              )}
              {activeGenre && (
                <FilterChip
                  label={activeGenre.label}
                  onRemove={() => setFilters((f) => ({ ...f, genre: null }))}
                />
              )}
              <span className="tabular-nums">
                {filteredGames.length} di {poolSize}
              </span>
              {filterCount > 1 && (
                <button
                  type="button"
                  onClick={() => setFilters(EMPTY_FILTERS)}
                  className="text-zinc-400 underline-offset-2 hover:text-zinc-200 hover:underline"
                >
                  Azzera filtri
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
          />
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
          closeDetail();
        }}
      />

      {/* Errors float above every dialog: a failed launch from the detail view stays readable */}
      {statusMessage && (
        <div
          role="alert"
          className="fixed bottom-5 left-1/2 z-[60] flex w-[min(40rem,calc(100vw-2rem))] -translate-x-1/2 items-start justify-between gap-4 rounded-lg border border-red-500/40 bg-red-950 px-4 py-3 text-xs leading-relaxed text-red-100 shadow-2xl"
        >
          <span className="min-w-0 select-text break-words">{statusMessage}</span>
          <button
            type="button"
            onClick={() => setStatusMessage(null)}
            className="shrink-0 font-medium text-red-300 hover:text-red-50"
          >
            Chiudi
          </button>
        </div>
      )}

      <SettingsModal
        isOpen={isSettingsOpen}
        initialConfig={config}
        onClose={() => setIsSettingsOpen(false)}
        onSave={handleSaveSettings}
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
