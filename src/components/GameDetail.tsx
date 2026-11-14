import { useEffect, useRef } from "react";
import { Pencil, Play, Star, X } from "lucide-react";
import { resolveCoverUrl } from "../lib/backend";
import { genreKey } from "../lib/library";
import { PLATFORM_META } from "../lib/platforms";
import type { DetectedGame } from "../types/game";

interface GameDetailProps {
  game: DetectedGame | null;
  isLaunching: boolean;
  /** `false` while another dialog is stacked on top, so Escape closes only that one. */
  isTopmost: boolean;
  onClose: () => void;
  onLaunch: (game: DetectedGame) => void;
  onToggleFavorite: (game: DetectedGame) => void;
  onEdit: (game: DetectedGame) => void;
  onSelectGenre: (genreKey: string) => void;
}

/** Describes how the game will be started, shown as a small technical footnote. */
function launchTarget(game: DetectedGame): string {
  if (game.launchUri) {
    if (game.platform === "steam") return "Avvio tramite Steam";
    if (game.platform === "epic") return "Avvio tramite Epic Games Launcher";
    return "Avvio tramite client della piattaforma";
  }
  return game.executablePath ?? "Nessun eseguibile configurato";
}

export function GameDetail({
  game,
  isLaunching,
  isTopmost,
  onClose,
  onLaunch,
  onToggleFavorite,
  onEdit,
  onSelectGenre,
}: GameDetailProps) {
  const playButtonRef = useRef<HTMLButtonElement>(null);
  const gameId = game?.id ?? null;

  useEffect(() => {
    if (!gameId || !isTopmost) return;
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [gameId, isTopmost, onClose]);

  // Enter right after opening a game starts it
  useEffect(() => {
    if (gameId) playButtonRef.current?.focus();
  }, [gameId]);

  if (!game) return null;

  const platformMeta = PLATFORM_META[game.platform];
  const coverUrl = resolveCoverUrl(game.coverPath, game.updatedAt);
  const canLaunch = Boolean(game.launchUri || game.executablePath);

  return (
    <div
      className="fixed inset-0 z-40 flex items-center justify-center bg-black/80 p-6 backdrop-blur-sm"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby="game-detail-title"
        className="relative flex max-h-full w-full max-w-3xl overflow-hidden rounded-xl border border-oled-border bg-oled-surface shadow-2xl"
      >
        {/* Ambient wash of the cover art behind the content */}
        {coverUrl && (
          <img
            src={coverUrl}
            alt=""
            aria-hidden="true"
            className="pointer-events-none absolute inset-0 h-full w-full scale-125 object-cover opacity-[0.14] blur-3xl"
          />
        )}

        <button
          id="close-game-detail-button"
          type="button"
          onClick={onClose}
          aria-label="Chiudi dettagli"
          className="absolute right-3 top-3 z-10 rounded-lg p-1.5 text-zinc-400 transition-colors hover:bg-oled-elevated hover:text-zinc-100"
        >
          <X className="h-4 w-4" />
        </button>

        {/* Cover */}
        <div className="relative hidden w-[17rem] shrink-0 self-stretch bg-oled-bg sm:block">
          <div className="aspect-[2/3] w-full">
            {coverUrl ? (
              <img
                src={coverUrl}
                alt={`Copertina di ${game.title}`}
                draggable={false}
                className="h-full w-full object-cover"
              />
            ) : (
              <div className="flex h-full w-full flex-col items-center justify-center gap-1 bg-gradient-to-b from-oled-elevated/50 to-oled-bg p-6 text-center">
                <span className="line-clamp-4 text-sm font-semibold text-zinc-300">
                  {game.title}
                </span>
                <span className="text-[11px] text-zinc-600">
                  Nessuna copertina
                </span>
              </div>
            )}
          </div>
        </div>

        {/* Details */}
        <div className="relative flex min-w-0 flex-1 flex-col gap-5 overflow-y-auto p-7">
          <div className="space-y-3 pr-8">
            <span
              className={`inline-block rounded-md border px-2 py-0.5 text-[10px] font-medium ${platformMeta.badgeClass}`}
            >
              {platformMeta.label}
            </span>
            <h2
              id="game-detail-title"
              className="select-text text-2xl font-semibold leading-tight tracking-tight text-zinc-50"
            >
              {game.title}
            </h2>

            {game.genres.length > 0 && (
              <ul className="flex flex-wrap gap-1.5" aria-label="Generi">
                {game.genres.map((genre) => (
                  <li key={genre}>
                    <button
                      type="button"
                      onClick={() => onSelectGenre(genreKey(genre))}
                      title={`Mostra tutti i giochi "${genre}"`}
                      className="rounded-full border border-oled-border bg-oled-bg/60 px-2.5 py-0.5 text-[11px] text-zinc-300 transition-colors hover:border-accent/50 hover:text-accent"
                    >
                      {genre}
                    </button>
                  </li>
                ))}
              </ul>
            )}
          </div>

          <p
            className={`select-text text-sm leading-relaxed ${
              game.shortDescription ? "text-zinc-300" : "text-zinc-600"
            }`}
          >
            {game.shortDescription ??
              "Nessuna descrizione disponibile. Puoi aggiungerla da Modifica."}
          </p>

          <dl className="mt-auto space-y-2 border-t border-oled-border/70 pt-4 text-[11px]">
            <div className="flex gap-3">
              <dt className="w-16 shrink-0 text-zinc-600">Cartella</dt>
              <dd
                className="min-w-0 select-text truncate font-mono text-zinc-400"
                title={game.installDir}
              >
                {game.installDir}
              </dd>
            </div>
            <div className="flex gap-3">
              <dt className="w-16 shrink-0 text-zinc-600">Avvio</dt>
              <dd
                className="min-w-0 select-text truncate font-mono text-zinc-400"
                title={launchTarget(game)}
              >
                {launchTarget(game)}
              </dd>
            </div>
          </dl>

          <div className="flex flex-wrap items-center gap-2">
            <button
              ref={playButtonRef}
              id="detail-launch-button"
              type="button"
              onClick={() => onLaunch(game)}
              disabled={isLaunching || !canLaunch}
              className="inline-flex min-w-[9rem] items-center justify-center gap-2 rounded-lg bg-accent px-5 py-2 text-sm font-semibold text-zinc-950 transition-colors hover:bg-accent-hover focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent disabled:cursor-not-allowed disabled:opacity-60"
            >
              <Play className="h-4 w-4 fill-current" />
              <span>{isLaunching ? "Avvio..." : "Avvia"}</span>
            </button>

            <button
              id="detail-favorite-button"
              type="button"
              onClick={() => onToggleFavorite(game)}
              aria-pressed={game.isFavorite}
              className={`inline-flex items-center gap-1.5 rounded-lg border px-3.5 py-2 text-xs font-medium transition-colors ${
                game.isFavorite
                  ? "border-accent/40 bg-accent/10 text-accent hover:bg-accent/15"
                  : "border-oled-border bg-oled-bg/60 text-zinc-300 hover:border-zinc-600"
              }`}
            >
              <Star
                className={`h-3.5 w-3.5 ${game.isFavorite ? "fill-accent" : ""}`}
              />
              <span>{game.isFavorite ? "Nei preferiti" : "Preferito"}</span>
            </button>

            <button
              id="detail-edit-button"
              type="button"
              onClick={() => onEdit(game)}
              className="inline-flex items-center gap-1.5 rounded-lg border border-oled-border bg-oled-bg/60 px-3.5 py-2 text-xs font-medium text-zinc-300 transition-colors hover:border-zinc-600"
            >
              <Pencil className="h-3.5 w-3.5" />
              <span>Modifica</span>
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
