import {
  AlertTriangle,
  FolderOpen,
  Gamepad2,
  Pencil,
  Play,
} from "lucide-react";
import { resolveCoverUrl } from "../lib/backend";
import type { DetectedGame, GamePlatform } from "../types/game";
import type { ViewMode } from "./Header";

interface GameListProps {
  games: DetectedGame[];
  totalDetected: number;
  viewMode: ViewMode;
  isScanning: boolean;
  launchingId: string | null;
  warnings: string[];
  onLaunch: (game: DetectedGame) => void;
  onEdit: (game: DetectedGame) => void;
  onOpenSettings: () => void;
}

const PLATFORM_STYLES: Record<
  GamePlatform,
  { label: string; badgeClass: string }
> = {
  steam: {
    label: "Steam",
    badgeClass: "border-sky-500/30 bg-sky-500/15 text-sky-300",
  },
  epic: {
    label: "Epic Games",
    badgeClass: "border-zinc-500/30 bg-zinc-500/20 text-zinc-200",
  },
  "Battle.net": {
    label: "Battle.net",
    badgeClass: "border-blue-500/35 bg-blue-500/15 text-blue-300",
  },
  standalone: {
    label: "Locale",
    badgeClass: "border-emerald-500/30 bg-emerald-500/15 text-emerald-300",
  },
};

export function GameList({
  games,
  totalDetected,
  viewMode,
  isScanning,
  launchingId,
  warnings,
  onLaunch,
  onEdit,
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
          Lettura librerie Steam, Epic Games, Battle.net e cartelle
          personalizzate
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
            Kanso rileva automaticamente i giochi installati su Steam, Epic
            Games e Battle.net. Puoi anche aggiungere manualmente le cartelle
            dei tuoi giochi standalone dalle Impostazioni.
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
      ) : viewMode === "grid" ? (
        <div
          className="grid gap-4"
          style={{
            gridTemplateColumns:
              "repeat(auto-fill, minmax(var(--card-min-width), 1fr))",
          }}
        >
          {games.map((game) => {
            const platformMeta = PLATFORM_STYLES[game.platform];
            const isLaunching = launchingId === game.id;
            const coverUrl = resolveCoverUrl(game.coverPath, game.updatedAt);

            return (
              <article
                key={game.id}
                className="group relative flex flex-col overflow-hidden rounded-card border border-oled-border bg-oled-surface transition-all duration-200 hover:-translate-y-0.5 hover:border-zinc-600/80"
              >
                {/* 2:3 Vertical Cover */}
                <div className="relative aspect-[2/3] w-full overflow-hidden bg-oled-bg">
                  {coverUrl ? (
                    <img
                      src={coverUrl}
                      alt={game.title}
                      loading="lazy"
                      className="h-full w-full object-cover transition-transform duration-300 group-hover:scale-[1.03]"
                    />
                  ) : (
                    <div className="flex h-full w-full flex-col justify-between p-4 bg-gradient-to-b from-oled-elevated/50 to-oled-bg">
                      <span
                        className={`self-start rounded-md border px-2 py-0.5 text-[10px] font-medium ${platformMeta.badgeClass}`}
                      >
                        {platformMeta.label}
                      </span>
                      <div className="space-y-1">
                        <p className="line-clamp-3 text-sm font-semibold leading-snug text-zinc-200">
                          {game.title}
                        </p>
                        {game.genres.length > 0 && (
                          <p className="truncate text-[11px] text-zinc-500">
                            {game.genres.slice(0, 2).join(" · ")}
                          </p>
                        )}
                      </div>
                    </div>
                  )}

                  {/* Platform pill overlay when cover image is present */}
                  {coverUrl && (
                    <span
                      className={`absolute left-2.5 top-2.5 rounded-md border px-2 py-0.5 text-[10px] font-medium backdrop-blur-md ${platformMeta.badgeClass}`}
                    >
                      {platformMeta.label}
                    </span>
                  )}

                  {/* Edit button in top-right corner */}
                  <button
                    id={`edit-grid-${game.id}`}
                    type="button"
                    onClick={() => onEdit(game)}
                    title={`Modifica ${game.title}`}
                    aria-label={`Modifica ${game.title}`}
                    className="absolute right-2.5 top-2.5 rounded-lg border border-white/10 bg-black/65 p-1.5 text-zinc-300 opacity-0 backdrop-blur-md transition-opacity hover:bg-black/85 hover:text-white focus:opacity-100 group-hover:opacity-100"
                  >
                    <Pencil className="h-3.5 w-3.5" />
                  </button>

                  {/* Hover play bar */}
                  <div className="absolute inset-x-0 bottom-0 flex items-end bg-gradient-to-t from-black/90 via-black/55 to-transparent p-3 opacity-0 transition-opacity duration-200 focus-within:opacity-100 group-hover:opacity-100">
                    <button
                      id={`launch-grid-${game.id}`}
                      type="button"
                      onClick={() => onLaunch(game)}
                      disabled={isLaunching}
                      className="inline-flex w-full items-center justify-center gap-1.5 rounded-lg bg-accent py-1.5 text-xs font-semibold text-zinc-950 shadow-lg transition-colors hover:bg-accent-hover disabled:opacity-60"
                    >
                      <Play className="h-3.5 w-3.5 fill-current" />
                      <span>{isLaunching ? "Avvio..." : "Avvia"}</span>
                    </button>
                  </div>
                </div>

                {/* Card caption */}
                <div className="flex flex-1 flex-col justify-between gap-1 px-3 py-2.5">
                  <h3
                    className="truncate text-xs font-medium text-zinc-100"
                    title={game.title}
                  >
                    {game.title}
                  </h3>
                  <div className="flex items-center justify-between gap-2 text-[11px] text-zinc-500">
                    <span className="truncate">
                      {game.genres.length > 0
                        ? game.genres.slice(0, 2).join(", ")
                        : platformMeta.label}
                    </span>
                    <button
                      type="button"
                      onClick={() => onEdit(game)}
                      className="shrink-0 text-zinc-400 hover:text-zinc-200 sm:hidden"
                    >
                      Modifica
                    </button>
                  </div>
                </div>
              </article>
            );
          })}
        </div>
      ) : (
        <div className="overflow-hidden rounded-xl border border-oled-border bg-oled-surface/50">
          <ul className="divide-y divide-oled-border">
            {games.map((game) => {
              const platformMeta = PLATFORM_STYLES[game.platform];
              const isLaunching = launchingId === game.id;
              const coverUrl = resolveCoverUrl(game.coverPath, game.updatedAt);

              return (
                <li
                  key={game.id}
                  className="flex flex-col justify-between gap-3 px-4 py-3 transition-colors hover:bg-oled-elevated/60 sm:flex-row sm:items-center"
                >
                  <div className="flex min-w-0 flex-1 items-center gap-3.5">
                    <div className="h-14 w-10 shrink-0 overflow-hidden rounded border border-oled-border bg-oled-bg">
                      {coverUrl ? (
                        <img
                          src={coverUrl}
                          alt={game.title}
                          className="h-full w-full object-cover"
                        />
                      ) : (
                        <div className="flex h-full w-full items-center justify-center text-[10px] font-semibold text-zinc-600">
                          2:3
                        </div>
                      )}
                    </div>

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
                        {game.genres.length > 0 && (
                          <span className="text-[11px] text-zinc-400">
                            {game.genres.slice(0, 3).join(" · ")}
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
                      </div>
                    </div>
                  </div>

                  <div className="flex shrink-0 items-center gap-2">
                    <button
                      id={`edit-list-${game.id}`}
                      type="button"
                      onClick={() => onEdit(game)}
                      className="inline-flex items-center gap-1.5 rounded-lg border border-oled-border bg-oled-elevated px-3 py-1.5 text-xs font-medium text-zinc-200 transition-colors hover:border-zinc-600"
                    >
                      <Pencil className="h-3.5 w-3.5 text-zinc-400" />
                      <span>Modifica</span>
                    </button>
                    <button
                      id={`launch-list-${game.id}`}
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
