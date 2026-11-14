import { AlertTriangle, FolderOpen, Gamepad2, Play, Star } from "lucide-react";
import { resolveCoverUrl } from "../lib/backend";
import { PLATFORM_META } from "../lib/platforms";
import type { DetectedGame } from "../types/game";
import type { ViewMode } from "./Header";

interface GameListProps {
  games: DetectedGame[];
  totalDetected: number;
  viewMode: ViewMode;
  isScanning: boolean;
  launchingId: string | null;
  warnings: string[];
  onOpenDetail: (game: DetectedGame) => void;
  onLaunch: (game: DetectedGame) => void;
  onToggleFavorite: (game: DetectedGame) => void;
  onOpenSettings: () => void;
}

interface FavoriteButtonProps {
  game: DetectedGame;
  onToggle: (game: DetectedGame) => void;
  className?: string;
}

function FavoriteButton({ game, onToggle, className = "" }: FavoriteButtonProps) {
  const label = game.isFavorite
    ? `Rimuovi ${game.title} dai preferiti`
    : `Aggiungi ${game.title} ai preferiti`;
  return (
    <button
      type="button"
      onClick={() => onToggle(game)}
      aria-pressed={game.isFavorite}
      aria-label={label}
      title={game.isFavorite ? "Rimuovi dai preferiti" : "Aggiungi ai preferiti"}
      className={className}
    >
      <Star
        className={`h-3.5 w-3.5 ${
          game.isFavorite ? "fill-accent text-accent" : ""
        }`}
      />
    </button>
  );
}

export function GameList({
  games,
  totalDetected,
  viewMode,
  isScanning,
  launchingId,
  warnings,
  onOpenDetail,
  onLaunch,
  onToggleFavorite,
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
            Nessun gioco corrisponde alla ricerca o ai filtri attivi.
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
            const platformMeta = PLATFORM_META[game.platform];
            const isLaunching = launchingId === game.id;
            const coverUrl = resolveCoverUrl(game.coverPath, game.updatedAt);

            return (
              <article
                key={game.id}
                className="group relative flex flex-col overflow-hidden rounded-card border border-oled-border bg-oled-surface transition-all duration-200 focus-within:border-zinc-600/80 hover:-translate-y-0.5 hover:border-zinc-600/80"
              >
                {/* 2:3 vertical cover */}
                <div className="relative aspect-[2/3] w-full overflow-hidden bg-oled-bg">
                  {coverUrl ? (
                    <img
                      src={coverUrl}
                      alt=""
                      loading="lazy"
                      draggable={false}
                      className="h-full w-full object-cover transition-transform duration-300 group-hover:scale-[1.03]"
                    />
                  ) : (
                    <div className="flex h-full w-full flex-col justify-end bg-gradient-to-b from-oled-elevated/50 to-oled-bg p-4">
                      <p className="line-clamp-4 text-sm font-semibold leading-snug text-zinc-300">
                        {game.title}
                      </p>
                    </div>
                  )}

                  <span
                    className={`absolute left-2.5 top-2.5 rounded-md border px-2 py-0.5 text-[10px] font-medium backdrop-blur-md ${platformMeta.badgeClass}`}
                  >
                    {platformMeta.label}
                  </span>

                  {/* Whole cover opens the detail view */}
                  <button
                    id={`open-detail-${game.id}`}
                    type="button"
                    onClick={() => onOpenDetail(game)}
                    aria-label={`Apri dettagli di ${game.title}`}
                    className="absolute inset-0 cursor-pointer focus-visible:outline focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-accent"
                  />

                  <FavoriteButton
                    game={game}
                    onToggle={onToggleFavorite}
                    className={`absolute right-2.5 top-2.5 rounded-lg border border-white/10 bg-black/65 p-1.5 text-zinc-300 backdrop-blur-md transition-opacity hover:bg-black/85 hover:text-white focus-visible:opacity-100 group-hover:opacity-100 ${
                      game.isFavorite ? "opacity-100" : "opacity-0"
                    }`}
                  />

                  {/* Hover play bar */}
                  <div className="pointer-events-none absolute inset-x-0 bottom-0 flex items-end bg-gradient-to-t from-black/90 via-black/55 to-transparent p-3 opacity-0 transition-opacity duration-200 focus-within:opacity-100 group-hover:opacity-100">
                    <button
                      id={`launch-grid-${game.id}`}
                      type="button"
                      onClick={() => onLaunch(game)}
                      disabled={isLaunching}
                      aria-label={`Avvia ${game.title}`}
                      className="pointer-events-auto inline-flex w-full items-center justify-center gap-1.5 rounded-lg bg-accent py-1.5 text-xs font-semibold text-zinc-950 shadow-lg transition-colors hover:bg-accent-hover disabled:opacity-60"
                    >
                      <Play className="h-3.5 w-3.5 fill-current" />
                      <span>{isLaunching ? "Avvio..." : "Avvia"}</span>
                    </button>
                  </div>
                </div>

                {/* Card caption */}
                <div className="space-y-1 px-3 py-2.5">
                  <h3
                    className="truncate text-xs font-medium text-zinc-100"
                    title={game.title}
                  >
                    {game.title}
                  </h3>
                  <p className="truncate text-[11px] text-zinc-500">
                    {game.genres.length > 0
                      ? game.genres.slice(0, 2).join(", ")
                      : platformMeta.label}
                  </p>
                </div>
              </article>
            );
          })}
        </div>
      ) : (
        <div className="overflow-hidden rounded-xl border border-oled-border bg-oled-surface/50">
          <ul className="divide-y divide-oled-border">
            {games.map((game) => {
              const platformMeta = PLATFORM_META[game.platform];
              const isLaunching = launchingId === game.id;
              const coverUrl = resolveCoverUrl(game.coverPath, game.updatedAt);

              return (
                <li
                  key={game.id}
                  className="flex items-center justify-between gap-3 px-4 py-2.5 transition-colors hover:bg-oled-elevated/60"
                >
                  <button
                    id={`open-detail-${game.id}`}
                    type="button"
                    onClick={() => onOpenDetail(game)}
                    aria-label={`Apri dettagli di ${game.title}`}
                    className="flex min-w-0 flex-1 items-center gap-3.5 rounded-md text-left focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent"
                  >
                    <div className="h-14 w-[38px] shrink-0 overflow-hidden rounded border border-oled-border bg-oled-bg">
                      {coverUrl && (
                        <img
                          src={coverUrl}
                          alt=""
                          loading="lazy"
                          draggable={false}
                          className="h-full w-full object-cover"
                        />
                      )}
                    </div>

                    <div className="min-w-0 flex-1 space-y-1">
                      <div className="flex items-center gap-2">
                        <span className="truncate text-sm font-medium text-zinc-100">
                          {game.title}
                        </span>
                        <span
                          className={`shrink-0 rounded-md border px-2 py-0.5 text-[10px] font-medium ${platformMeta.badgeClass}`}
                        >
                          {platformMeta.label}
                        </span>
                      </div>
                      <p className="truncate text-[11px] text-zinc-500">
                        {game.genres.length > 0
                          ? game.genres.slice(0, 4).join(" · ")
                          : (game.executablePath ?? game.installDir)}
                      </p>
                    </div>
                  </button>

                  <div className="flex shrink-0 items-center gap-2">
                    <FavoriteButton
                      game={game}
                      onToggle={onToggleFavorite}
                      className="rounded-lg p-2 text-zinc-500 transition-colors hover:bg-oled-elevated hover:text-zinc-200"
                    />
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
