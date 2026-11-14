import { useCallback, useEffect, useMemo, useState } from "react";
import { ApiKeyBanner } from "./components/ApiKeyBanner";
import { EditGameModal } from "./components/EditGameModal";
import { GameList } from "./components/GameList";
import { Header, type PlatformFilter, type ViewMode } from "./components/Header";
import { SettingsModal } from "./components/SettingsModal";
import {
  getSettings,
  isRunningInTauri,
  launchGame,
  saveSettings,
  scanGames,
  subscribeToGameUpdates,
  syncLibraryMetadata,
} from "./lib/backend";
import type { AppConfig, DetectedGame, ScanReport } from "./types/game";

const INITIAL_CONFIG: AppConfig = {
  steamgriddbApiKey: "",
  customFolders: [],
  minimizeToTrayOnLaunch: false,
};

export function App() {
  const [config, setConfig] = useState<AppConfig>(INITIAL_CONFIG);
  const [report, setReport] = useState<ScanReport>({
    games: [],
    steamCount: 0,
    epicCount: 0,
    battlenetCount: 0,
    standaloneCount: 0,
    warnings: [],
  });
  const [isScanning, setIsScanning] = useState(true);
  const [isSyncingMetadata, setIsSyncingMetadata] = useState(false);
  const [isSettingsOpen, setIsSettingsOpen] = useState(false);
  const [editingGameId, setEditingGameId] = useState<string | null>(null);
  const [bannerDismissed, setBannerDismissed] = useState(false);
  const [searchQuery, setSearchQuery] = useState("");
  const [platformFilter, setPlatformFilter] = useState<PlatformFilter>("all");
  const [viewMode, setViewMode] = useState<ViewMode>("grid");
  const [launchingId, setLaunchingId] = useState<string | null>(null);
  const [statusMessage, setStatusMessage] = useState<string | null>(null);

  const upsertSingleGameInState = useCallback((updated: DetectedGame) => {
    setReport((prev) => {
      const idx = prev.games.findIndex((g) => g.id === updated.id);
      const nextGames =
        idx === -1
          ? [...prev.games, updated]
          : prev.games.map((g) => (g.id === updated.id ? updated : g));
      nextGames.sort((a, b) =>
        a.title.toLowerCase().localeCompare(b.title.toLowerCase())
      );
      return {
        ...prev,
        games: nextGames,
      };
    });
  }, []);

  const runMetadataSync = useCallback(async (forceRefresh = false) => {
    if (!isRunningInTauri()) return;
    setIsSyncingMetadata(true);
    try {
      const syncedReport = await syncLibraryMetadata(forceRefresh);
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

  const performScan = useCallback(async () => {
    setIsScanning(true);
    setStatusMessage(null);
    try {
      const res = await scanGames();
      setReport(res);
      void runMetadataSync(false);
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
        await performScan();
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
    const apiKeyNewlyAdded =
      !config.steamgriddbApiKey.trim() &&
      Boolean(updated.steamgriddbApiKey.trim());

    const saved = await saveSettings(updated);
    setConfig(saved);

    if (foldersChanged) {
      await performScan();
    } else if (apiKeyNewlyAdded) {
      void runMetadataSync(false);
    }
  };

  const handleLaunchGame = async (game: DetectedGame) => {
    setLaunchingId(game.id);
    setStatusMessage(null);
    try {
      await launchGame(game);
    } catch (err) {
      setStatusMessage(
        err instanceof Error
          ? err.message
          : `Impossibile avviare ${game.title}.`
      );
    } finally {
      setLaunchingId(null);
    }
  };

  const editingGame = useMemo(
    () => report.games.find((g) => g.id === editingGameId) ?? null,
    [report.games, editingGameId]
  );

  const filteredGames = useMemo(() => {
    const q = searchQuery.trim().toLowerCase();
    return report.games.filter((g) => {
      if (platformFilter !== "all" && g.platform !== platformFilter) {
        return false;
      }
      if (!q) return true;
      return (
        g.title.toLowerCase().includes(q) ||
        g.cleanTitle.toLowerCase().includes(q) ||
        g.genres.some((genre) => genre.toLowerCase().includes(q))
      );
    });
  }, [report.games, searchQuery, platformFilter]);

  const hasApiKey = config.steamgriddbApiKey.trim().length > 0;

  return (
    <div className="min-h-screen bg-oled-bg text-zinc-100 flex flex-col">
      <Header
        totalGames={report.games.length}
        searchQuery={searchQuery}
        onSearchChange={setSearchQuery}
        platformFilter={platformFilter}
        onPlatformFilterChange={setPlatformFilter}
        viewMode={viewMode}
        onViewModeChange={setViewMode}
        isScanning={isScanning}
        isSyncingMetadata={isSyncingMetadata}
        hasApiKey={hasApiKey}
        onScan={performScan}
        onOpenSettings={() => setIsSettingsOpen(true)}
      />

      {!hasApiKey && !bannerDismissed && (
        <ApiKeyBanner
          onConfigure={() => setIsSettingsOpen(true)}
          onDismiss={() => setBannerDismissed(true)}
        />
      )}

      <main className="mx-auto w-full max-w-6xl flex-1 px-6 py-6 space-y-4">
        {!isRunningInTauri() && (
          <div className="rounded-lg border border-oled-border bg-oled-surface/70 px-4 py-2 text-xs text-zinc-400">
            Modalità anteprima browser attiva — i dati mostrati sono simulati.
            Nel binario Windows (<code className="text-zinc-300">.exe</code>)
            Kanso scansiona realmente registro Steam, Epic Games, Battle.net e
            cartelle locali.
          </div>
        )}

        {statusMessage && (
          <div className="flex items-center justify-between rounded-lg border border-red-500/30 bg-red-500/10 px-4 py-2.5 text-xs text-red-200">
            <span>{statusMessage}</span>
            <button
              type="button"
              onClick={() => setStatusMessage(null)}
              className="ml-4 text-red-300 hover:text-red-100"
            >
              Chiudi
            </button>
          </div>
        )}

        {/* Source summary strip */}
        <div className="flex flex-wrap items-center justify-between gap-2 text-xs text-zinc-400">
          <div className="flex flex-wrap items-center gap-4">
            <span>
              Steam:{" "}
              <strong className="font-medium text-zinc-200 tabular-nums">
                {report.steamCount}
              </strong>
            </span>
            <span>
              Epic Games:{" "}
              <strong className="font-medium text-zinc-200 tabular-nums">
                {report.epicCount}
              </strong>
            </span>
            <span>
              Battle.net:{" "}
              <strong className="font-medium text-zinc-200 tabular-nums">
                {report.battlenetCount}
              </strong>
            </span>
            <span>
              Locale:{" "}
              <strong className="font-medium text-zinc-200 tabular-nums">
                {report.standaloneCount}
              </strong>
            </span>
          </div>
          {filteredGames.length !== report.games.length && (
            <span>
              Visualizzati {filteredGames.length} di {report.games.length}
            </span>
          )}
        </div>

        <GameList
          games={filteredGames}
          totalDetected={report.games.length}
          viewMode={viewMode}
          isScanning={isScanning}
          launchingId={launchingId}
          warnings={report.warnings}
          onLaunch={handleLaunchGame}
          onEdit={(game) => setEditingGameId(game.id)}
          onOpenSettings={() => setIsSettingsOpen(true)}
        />
      </main>

      <SettingsModal
        isOpen={isSettingsOpen}
        initialConfig={config}
        onClose={() => setIsSettingsOpen(false)}
        onSave={handleSaveSettings}
      />

      <EditGameModal
        game={editingGame}
        hasApiKey={hasApiKey}
        onClose={() => setEditingGameId(null)}
        onGameUpdated={upsertSingleGameInState}
        onOpenSettings={() => setIsSettingsOpen(true)}
      />
    </div>
  );
}

export default App;
