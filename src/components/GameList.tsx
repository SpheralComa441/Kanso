import { AlertTriangle, FolderOpen, Gamepad2, Play } from "lucide-react";
import type { DetectedGame, GamePlatform } from "../types/game";

interface GameListProps {
  games: DetectedGame[];
  totalDetected: number;
  isScanning: boolean;
  launchingId: string | null;
  warnings: string[];
  onLaunch: (game: DetectedGame) => void;
  onOpenSettings: () => void;
}

const PLATFORM_STYLES: Record<
  GamePlatform,
  { label: string; badgeClass: string }
> = {
  steam: {
    label: "Steam",
    badgeClass: "border-sky-500/30 bg-sky-500/10 text-sky-300",
  },
  epic: {
    label: "Epic Games",
    badgeClass: "border-zinc-500/30 bg-zinc-500/15 text-zinc-200",
  },
  standalone: {
    label: "Locale",
    badgeClass: "border-emerald-500/30 bg-emerald-500/10 text-emerald-300",
  },
};

export function GameList({
  games,
  totalDetected,
  isScanning,
  launchingId,
  warnings,
  onLaunch,
  onOpenSettings,
}: GameListProps) {
  if (isScanning && totalDetected === 0) {
    return (
      <div className="flex flex-col items-center justify-center py-24 text-center">
        <div className="mb-3 h-6 w-6 animate-spin rounded-full border-2 border-oled-border border-t-accent" />
        <p className="text-sm font-medium text-zinc-300">
          Ricerca dei giochi installati in corso...
        </p>
        <p className="mt-1 text-xs text-zinc-500">
          Lettura librerie Steam, manifest Epic Games e cartelle personalizzate
        </p>
      </div>
    );
  }

  return (
    <div className="space-y-4">
      {warnings.length > 0 && (
        <div className="rounded-lg border border-amber-500/25 bg-amber-500/10 px-4 py-3 text-xs text-amber-200">
          <div className="flex items-start gap-2">
            <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0 text-amber-400" />
            <ul className="space-y-1">
              {warnings.map((w, idx) => (
                <li key={idx}>{w}</li>
              ))}
            </ul>
          </div>
        </div>
      )}

      {totalDetected === 0 ? (
        <div className="rounded-xl border border-oled-border bg-oled-surface/60 px-6 py-16 text-center">
          <Gamepad2 className="mx-auto mb-3 h-8 w-8 text-zinc-500" />
          <h2 className="text-sm font-semibold text-zinc-200">
            Nessun gioco rilevato sul sistema
          </h2>
          <p className="mx-auto mt-1.5 max-w-md text-xs leading-relaxed text-zinc-400">
            Kanso cerca automaticamente i giochi installati su Steam ed Epic
            Games. Puoi anche aggiungere manualmente le tue cartelle di giochi
            standalone dalle Impostazioni.
          </p>
          <button
            id="empty-add-folder-button"
            type="button"
            onClick={onOpenSettings}
            className="mt-5 inline-flex items-center gap-1.5 rounded-lg bg-accent px-3.5 py-2 text-xs font-semibold text-zinc-950 transition-colors hover:bg-accent-hover"
          >
            <FolderOpen className="h-3.5 w-3.5" />
            <span>Aggiungi cartelle giochi</span>
          </button>
        </div>
      ) : games.length === 0 ? (
        <div className="rounded-xl border border-oled-border bg-oled-surface/40 px-6 py-12 text-center">
          <p className="text-sm text-zinc-400">
            Nessun gioco corrisponde ai filtri di ricerca attuali.
          </p>
        </div>
      ) : (
        <div className="overflow-hidden rounded-xl border border-oled-border bg-oled-surface/50">
          <ul className="divide-y divide-oled-border">
            {games.map((game) => {
              const platformMeta = PLATFORM_STYLES[game.platform];
              const isLaunching = launchingId === game.id;
              const titleWasCleaned =
                game.cleanTitle.toLowerCase() !== game.title.toLowerCase();

              return (
                <li
                  key={game.id}
                  className="flex flex-col justify-between gap-3 px-4 py-3.5 transition-colors hover:bg-oled-elevated/60 sm:flex-row sm:items-center"
                >
                  <div className="min-w-0 flex-1 space-y-1">
                    <div className="flex flex-wrap items-center gap-2">
                      <span className="truncate text-sm font-medium text-zinc-100">
                        {game.title}
                      </span>
                      <span
                        className={`inline-flex items-center rounded-md border px-2 py-0.5 text-[11px] font-medium ${platformMeta.badgeClass}`}
                      >
                        {platformMeta.label}
                      </span>
                      {game.platformAppId && (
                        <span className="font-mono text-[11px] text-zinc-500">
                          ID: {game.platformAppId}
                        </span>
                      )}
                    </div>

                    <div className="flex flex-wrap items-center gap-x-4 gap-y-1 text-xs text-zinc-500">
                      <span
                        className="truncate font-mono text-[11px]"
                        title={game.executablePath ?? game.installDir}
                      >
                        {game.executablePath ?? game.installDir}
                      </span>
                      {titleWasCleaned && (
                        <span className="text-[11px] text-zinc-400">
                          Titolo ricerca:{" "}
                          <strong className="font-medium text-zinc-300">
                            {game.cleanTitle}
                          </strong>
                        </span>
                      )}
                    </div>
                  </div>

                  <div className="flex shrink-0 items-center gap-2">
                    <button
                      id={`launch-${game.id}`}
                      type="button"
                      onClick={() => onLaunch(game)}
                      disabled={isLaunching}
                      className="inline-flex items-center gap-1.5 rounded-lg bg-accent px-3.5 py-1.5 text-xs font-semibold text-zinc-950 transition-colors hover:bg-accent-hover disabled:cursor-wait disabled:opacity-60"
                    >
                      <Play className="h-3.5 w-3.5 fill-current" />
                      <span>{isLaunching ? "Avvio..." : "Avvia"}</span>
                    </button>
                  </div>
                </li>
              );
            })}
          </ul>
        </div>
      )}
    </div>
  );
}
