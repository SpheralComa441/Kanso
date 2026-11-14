import { useEffect, useRef } from "react";
import { Eye, EyeOff, Pencil, Play, Star, X } from "lucide-react";
import { useI18n, type I18n } from "../i18n";
import { resolveCoverUrl } from "../lib/backend";
import { GAME_STATUSES, genreKey } from "../lib/library";
import { PLATFORM_META, platformLabel } from "../lib/platforms";
import { useBrokenCovers } from "../lib/useBrokenCovers";
import type { DetectedGame, GameStatus, PluginInfo } from "../types/game";
import { Cover } from "./Cover";
import { AchievementsSection, GamePluginSections } from "./GameExtras";
import { STATUS_LABEL_KEYS, StatusIcon } from "./StatusIcon";

interface GameDetailProps {
  game: DetectedGame | null;
  isLaunching: boolean;
  /** `false` while another dialog is stacked on top, so Escape closes only that one. */
  isTopmost: boolean;
  onClose: () => void;
  onLaunch: (game: DetectedGame) => void;
  onToggleFavorite: (game: DetectedGame) => void;
  onToggleHidden: (game: DetectedGame) => void;
  onEdit: (game: DetectedGame) => void;
  onSelectGenre: (genreKey: string) => void;
  onSelectTag: (tagKey: string) => void;
  /** Kanso sees one of the game's processes right now. */
  isRunning: boolean;
  onSetStatus: (game: DetectedGame, status: GameStatus | null) => void;
  /** Enabled plugins that add sections to the sheet. */
  plugins: PluginInfo[];
}

/** Describes how the game will be started, shown as a small technical footnote. */
function launchTarget(game: DetectedGame, t: I18n["t"]): string {
  if (game.launchUri) {
    if (game.platform === "steam") return t("detail.viaSteam");
    if (game.platform === "epic") return t("detail.viaEpic");
    return t("detail.viaClient");
  }
  return game.executablePath ?? t("detail.noExe");
}

export function GameDetail({
  game,
  isLaunching,
  isTopmost,
  onClose,
  onLaunch,
  onToggleFavorite,
  onToggleHidden,
  onEdit,
  onSelectGenre,
  onSelectTag,
  isRunning,
  onSetStatus,
  plugins,
}: GameDetailProps) {
  const { t, plural, relativePast, duration } = useI18n();
  const playButtonRef = useRef<HTMLButtonElement>(null);
  const { usableCover, markBroken } = useBrokenCovers();
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
  const coverUrl = usableCover(
    resolveCoverUrl(game.coverPath, game.updatedAt)
  );
  const canLaunch = Boolean(game.launchUri || game.executablePath);
  const hideLabel = game.isHidden ? t("detail.unhide") : t("detail.hide");

  return (
    <div
      className="below-header fixed inset-x-0 bottom-0 z-40 flex items-center justify-center bg-black/75 p-6 backdrop-blur-sm"
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
          aria-label={t("detail.close")}
          className="absolute right-3 top-3 z-10 rounded-lg p-1.5 text-zinc-400 transition-colors hover:bg-oled-elevated hover:text-zinc-100"
        >
          <X className="h-4 w-4" />
        </button>

        {/* Cover */}
        {/* Translucent: under a cover shorter than the sheet the ambient wash shows through */}
        <div className="relative hidden w-[17rem] shrink-0 self-stretch bg-oled-bg/50 sm:block">
          <div className="relative aspect-[2/3] w-full overflow-hidden">
            {coverUrl ? (
              <Cover
                url={coverUrl}
                alt={t("detail.coverAlt", { title: game.title })}
                onError={() => markBroken(coverUrl)}
              />
            ) : (
              <div className="flex h-full w-full flex-col items-center justify-center gap-1 bg-gradient-to-b from-oled-elevated/50 to-oled-bg p-6 text-center">
                <span className="line-clamp-4 text-sm font-semibold text-zinc-300">
                  {game.title}
                </span>
                <span className="text-2xs text-zinc-500">
                  {t("detail.noCover")}
                </span>
              </div>
            )}
          </div>
        </div>

        {/* Details: everything scrolls except the actions, which stay within reach */}
        <div className="relative flex min-w-0 flex-1 flex-col">
          <div className="scroll-area flex min-h-0 flex-1 flex-col gap-5 overflow-y-auto p-7 pb-5">
            <div className="space-y-3 pr-8">
              <div className="flex flex-wrap items-center gap-2">
                <span
                  className={`inline-block rounded-md border px-2 py-0.5 text-3xs font-medium ${platformMeta.badgeClass}`}
                >
                  {platformLabel(game.platform, t)}
                </span>
                {isRunning && (
                  <span
                    id="detail-running-badge"
                    className="inline-flex items-center gap-1.5 rounded-md border border-emerald-500/30 bg-emerald-500/10 px-2 py-0.5 text-3xs font-medium text-emerald-300"
                  >
                    <span className="h-1.5 w-1.5 rounded-full bg-emerald-400" />
                    {t("detail.running")}
                  </span>
                )}
              </div>
              <h2
                id="game-detail-title"
                className="select-text text-2xl font-semibold leading-tight tracking-tight text-zinc-50"
              >
                {game.title}
              </h2>

              {game.genres.length > 0 && (
                <ul className="flex flex-wrap gap-1.5" aria-label={t("detail.genres")}>
                  {game.genres.map((genre) => (
                    <li key={genre}>
                      <button
                        type="button"
                        onClick={() => onSelectGenre(genreKey(genre))}
                        title={t("detail.showGenre", { genre })}
                        className="rounded-full border border-oled-border bg-oled-bg/60 px-2.5 py-0.5 text-2xs text-zinc-300 transition-colors hover:border-accent/50 hover:text-accent"
                      >
                        {genre}
                      </button>
                    </li>
                  ))}
                </ul>
              )}

              {/* Steam community tags: finer than genres ("Souls-like", "Co-op online") */}
              {game.tags.length > 0 && (
                <ul className="flex flex-wrap gap-x-2.5 gap-y-1" aria-label={t("detail.tags")}>
                  {game.tags.map((tag) => (
                    <li key={tag}>
                      <button
                        type="button"
                        onClick={() => onSelectTag(genreKey(tag))}
                        title={t("detail.showTag", { tag })}
                        className="text-2xs text-zinc-400 underline-offset-2 transition-colors hover:text-accent hover:underline"
                      >
                        {tag}
                      </button>
                    </li>
                  ))}
                </ul>
              )}
            </div>

            {/* Where the user stands with the game: one click sets it, another clears it */}
            <div
              role="group"
              aria-label={t("detail.status")}
              className="flex flex-wrap items-center gap-1.5"
            >
              {GAME_STATUSES.map((status) => {
                const active = game.status === status;
                const label = t(STATUS_LABEL_KEYS[status]);
                return (
                  <button
                    key={status}
                    id={`detail-status-${status}`}
                    type="button"
                    onClick={() => onSetStatus(game, active ? null : status)}
                    aria-pressed={active}
                    title={t(active ? "status.clear" : "status.set", { status: label })}
                    className={`inline-flex items-center gap-1.5 rounded-full border px-2.5 py-1 text-2xs font-medium transition-colors ${
                      active
                        ? "border-accent/50 bg-accent/10 text-accent"
                        : "border-oled-border bg-oled-bg/60 text-zinc-400 hover:border-zinc-600 hover:text-zinc-200"
                    }`}
                  >
                    <StatusIcon status={status} className="h-3 w-3" />
                    <span>{label}</span>
                  </button>
                );
              })}
            </div>

            <p
              className={`select-text text-sm leading-relaxed ${
                game.shortDescription ? "text-zinc-300" : "text-zinc-500"
              }`}
            >
              {game.shortDescription ?? t("detail.noDescription")}
            </p>

            <AchievementsSection game={game} />
            <GamePluginSections game={game} plugins={plugins} />

            <dl className="mt-auto space-y-2 border-t border-oled-border/70 pt-4 text-2xs">
              <div className="flex gap-3">
                <dt className="w-24 shrink-0 text-zinc-500">{t("detail.folder")}</dt>
                <dd
                  className="min-w-0 select-text truncate font-mono text-zinc-400"
                  title={game.installDir}
                >
                  {game.installDir}
                </dd>
              </div>
              <div className="flex gap-3">
                <dt className="w-24 shrink-0 text-zinc-500">{t("detail.launch")}</dt>
                <dd
                  className="min-w-0 select-text truncate font-mono text-zinc-400"
                  title={launchTarget(game, t)}
                >
                  {launchTarget(game, t)}
                </dd>
              </div>
              {game.playtimeMinutes !== null && (
                <div className="flex gap-3">
                  <dt className="w-24 shrink-0 text-zinc-500">{t("detail.playtime")}</dt>
                  <dd
                    id="detail-playtime"
                    className="min-w-0 truncate text-zinc-400"
                    title={t(
                      game.platform === "steam" ? "detail.playtimeSteam" : "detail.playtimeMeasured"
                    )}
                  >
                    {game.playtimeMinutes > 0
                      ? duration(game.playtimeMinutes)
                      : t("detail.neverPlayed")}
                  </dd>
                </div>
              )}
              {game.lastPlayedAt !== null && (
                <div className="flex gap-3">
                  <dt className="w-24 shrink-0 text-zinc-500">{t("detail.played")}</dt>
                  <dd className="min-w-0 truncate text-zinc-400">
                    {relativePast(game.lastPlayedAt)}
                    {game.playCount > 1
                      ? ` · ${plural("launches", game.playCount)}`
                      : ""}
                  </dd>
                </div>
              )}
            </dl>
          </div>

          <div className="flex shrink-0 flex-wrap items-center gap-2 border-t border-oled-border/70 px-7 py-4">
            <button
              ref={playButtonRef}
              id="detail-launch-button"
              type="button"
              onClick={() => onLaunch(game)}
              disabled={isLaunching || !canLaunch}
              className="inline-flex min-w-[9rem] items-center justify-center gap-2 rounded-lg bg-accent px-5 py-2 text-sm font-semibold text-zinc-950 transition-colors hover:bg-accent-hover focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent disabled:cursor-not-allowed disabled:opacity-60"
            >
              <Play className="h-4 w-4 fill-current" />
              <span>{isLaunching ? t("common.launching") : t("common.play")}</span>
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
              <span>
                {game.isFavorite ? t("detail.inFavorites") : t("detail.favorite")}
              </span>
            </button>

            <button
              id="detail-edit-button"
              type="button"
              onClick={() => onEdit(game)}
              className="inline-flex items-center gap-1.5 rounded-lg border border-oled-border bg-oled-bg/60 px-3.5 py-2 text-xs font-medium text-zinc-300 transition-colors hover:border-zinc-600"
            >
              <Pencil className="h-3.5 w-3.5" />
              <span>{t("common.edit")}</span>
            </button>

            <button
              id="detail-hide-button"
              type="button"
              onClick={() => onToggleHidden(game)}
              title={hideLabel}
              aria-label={hideLabel}
              className="ml-auto rounded-lg p-2 text-zinc-500 transition-colors hover:bg-oled-bg/60 hover:text-zinc-200"
            >
              {game.isHidden ? (
                <Eye className="h-4 w-4" />
              ) : (
                <EyeOff className="h-4 w-4" />
              )}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
