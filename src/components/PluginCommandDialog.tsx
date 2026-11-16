import { useEffect, useState, type CSSProperties } from "react";
import { Download, Eye, PanelRightOpen, Play, RefreshCw, X } from "lucide-react";
import { useI18n } from "../i18n";
import { resolveCoverUrl } from "../lib/backend";
import { canInstall, platformLabel } from "../lib/platforms";
import { useBrokenCovers } from "../lib/useBrokenCovers";
import type { DetectedGame, PluginView } from "../types/game";
import { Cover } from "./Cover";
import { PluginSections } from "./PluginBlocks";

export interface PluginCommandRun {
  pluginId: string;
  commandId: string;
  /** Shown while the plugin has not answered yet, and when it gives no title of its own. */
  title: string;
  view: PluginView | null;
  error: string | null;
  isRunning: boolean;
}

interface PluginCommandDialogProps {
  run: PluginCommandRun | null;
  games: DetectedGame[];
  /** `false` while another dialog is stacked on top, so Escape closes only that one. */
  isTopmost: boolean;
  onClose: () => void;
  onRunAgain: () => void;
  onOpenGame: (game: DetectedGame) => void;
  onLaunch: (game: DetectedGame) => void;
}

/** What a plugin command answered: its sections and the games it points at. */
export function PluginCommandDialog({
  run,
  games,
  isTopmost,
  onClose,
  onRunAgain,
  onOpenGame,
  onLaunch,
}: PluginCommandDialogProps) {
  const { t } = useI18n();
  const { usableCover, markBroken } = useBrokenCovers();
  const isOpen = run !== null;
  const view = run?.view ?? null;

  // Face-down cards: which ones the user has turned. Every new answer is a new deal.
  const [dealt, setDealt] = useState(view);
  const [deal, setDeal] = useState(0);
  const [revealed, setRevealed] = useState<ReadonlySet<string>>(new Set());
  if (view !== dealt) {
    setDealt(view);
    setDeal(deal + 1);
    setRevealed(new Set());
  }

  useEffect(() => {
    if (!isOpen || !isTopmost) return;
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [isOpen, isTopmost, onClose]);

  if (!run) return null;

  const picks = (view?.games ?? []).flatMap((pick) => {
    const game = games.find((candidate) => candidate.id === pick.id);
    return game ? [{ game, note: pick.note, hint: pick.hint }] : [];
  });
  const style = view?.gamesStyle ?? "list";
  const asCards = style !== "list" && picks.length > 0;
  const covered = style === "mystery" ? picks.filter(({ game }) => !revealed.has(game.id)) : [];

  const reveal = (ids: string[], focusIndex?: number) => {
    setRevealed((current) => new Set([...current, ...ids]));
    // The card that was a button is now a game: the keyboard goes on to what starts it
    if (focusIndex !== undefined) {
      requestAnimationFrame(() =>
        document.getElementById(`pick-launch-${focusIndex}`)?.focus({ preventScroll: true })
      );
    }
  };
  const nothingToShow =
    !run.isRunning && !run.error && picks.length === 0 && (view?.sections.length ?? 0) === 0;

  return (
    <div
      className="below-header fixed inset-x-0 bottom-0 z-40 flex items-center justify-center bg-black/75 p-6 backdrop-blur-sm motion-backdrop"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div
        id="plugin-command-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="plugin-command-title"
        className={`motion-sheet flex max-h-full w-full flex-col overflow-hidden rounded-xl border border-oled-border bg-oled-surface shadow-2xl ${
          asCards ? "max-w-3xl" : "max-w-lg"
        }`}
      >
        <div className="flex shrink-0 items-center justify-between gap-4 border-b border-oled-border px-6 py-4">
          <h2 id="plugin-command-title" className="text-base font-semibold text-zinc-100">
            {view?.title ?? run.title}
          </h2>
          <button
            type="button"
            onClick={onClose}
            aria-label={t("common.close")}
            className="rounded-lg p-1.5 text-zinc-400 transition-colors hover:bg-oled-elevated hover:text-zinc-200"
          >
            <X className="h-4 w-4" />
          </button>
        </div>

        <div
          className={`scroll-area min-h-0 flex-1 space-y-4 overflow-y-auto px-6 py-5 transition-opacity ${
            run.isRunning && view ? "opacity-50" : ""
          }`}
        >
          {run.isRunning && !view && (
            <p className="animate-pulse py-6 text-center text-xs text-zinc-500">
              {t("plugins.working")}
            </p>
          )}
          {run.error && (
            <p className="select-text rounded-lg border border-red-500/30 bg-red-500/10 px-3.5 py-2.5 text-xs text-red-200">
              {run.error}
            </p>
          )}
          {nothingToShow && (
            <p className="py-6 text-center text-xs text-zinc-500">{t("plugins.noResult")}</p>
          )}

          {view && view.sections.length > 0 && <PluginSections sections={view.sections} />}

          {asCards && (
            <ul
              key={deal}
              className="pick-grid"
              data-picks-style={style}
              style={{ "--picks": Math.min(picks.length, 4) } as CSSProperties}
            >
              {picks.map(({ game, note, hint }, index) => {
                const coverUrl = usableCover(resolveCoverUrl(game.coverPath, game.updatedAt));
                const open = style !== "mystery" || revealed.has(game.id);
                const actionable = game.isInstalled || canInstall(game);
                return (
                  <li
                    key={game.id}
                    data-pick={game.id}
                    data-revealed={open}
                    className="pick-card"
                    style={{ "--i": index } as CSSProperties}
                  >
                    <div className="pick-card-inner">
                      {/* The game: a whole cover, as in the library */}
                      <div className="pick-face pick-front" aria-hidden={!open}>
                        {coverUrl && <Cover url={coverUrl} onError={() => markBroken(coverUrl)} />}
                        {open && (
                          <>
                            <button
                              type="button"
                              onClick={() => onOpenGame(game)}
                              aria-label={t("game.openDetails", { title: game.title })}
                              className="absolute inset-0 cursor-pointer focus-visible:outline focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-accent"
                            />
                            <div className="pointer-events-none absolute inset-x-0 bottom-0 space-y-2 bg-gradient-to-t from-black/90 via-black/70 to-transparent p-3 pt-10 text-white">
                              <div className="space-y-0.5">
                                <h3 className="line-clamp-2 text-sm font-semibold leading-snug">
                                  {game.title}
                                </h3>
                                <p className="line-clamp-2 text-2xs text-white/75">
                                  {note ?? platformLabel(game.platform, t)}
                                </p>
                              </div>
                              <div className="pointer-events-auto flex items-stretch gap-1.5">
                                {actionable && (
                                  <button
                                    id={`pick-launch-${index}`}
                                    type="button"
                                    onClick={() => onLaunch(game)}
                                    aria-label={t(
                                      game.isInstalled ? "game.launchNamed" : "game.installNamed",
                                      { title: game.title }
                                    )}
                                    className="inline-flex min-w-0 flex-1 items-center justify-center gap-1.5 rounded-lg bg-accent py-1.5 text-xs font-semibold text-zinc-950 shadow-lg transition-colors hover:bg-accent-hover"
                                  >
                                    {game.isInstalled ? (
                                      <Play className="h-3.5 w-3.5 fill-current" />
                                    ) : (
                                      <Download className="h-3.5 w-3.5" />
                                    )}
                                    <span>{t(game.isInstalled ? "common.play" : "common.install")}</span>
                                  </button>
                                )}
                                <button
                                  id={actionable ? undefined : `pick-launch-${index}`}
                                  type="button"
                                  onClick={() => onOpenGame(game)}
                                  title={t("plugins.openGame")}
                                  className={`inline-flex items-center justify-center gap-1.5 rounded-lg border border-white/25 bg-black/50 px-2.5 py-1.5 text-xs font-medium text-white backdrop-blur-md transition-colors hover:bg-black/75 ${
                                    actionable ? "" : "flex-1"
                                  }`}
                                >
                                  {actionable ? (
                                    <span className="sr-only">{t("plugins.openGame")}</span>
                                  ) : (
                                    <span>{t("plugins.openGame")}</span>
                                  )}
                                  {actionable && <PanelRightOpen className="h-3.5 w-3.5" aria-hidden="true" />}
                                </button>
                              </div>
                            </div>
                          </>
                        )}
                      </div>

                      {/* Its back: a question mark that will not sit still, until it is turned */}
                      {style === "mystery" && (
                        <button
                          id={`pick-reveal-${index}`}
                          type="button"
                          onClick={() => reveal([game.id], index)}
                          disabled={open}
                          aria-hidden={open}
                          aria-label={
                            t("plugins.reveal", { n: index + 1 }) + (hint ? `: ${hint}` : "")
                          }
                          className="pick-face pick-back"
                        >
                          <span className="pick-mark" aria-hidden="true">
                            ?
                          </span>
                          {hint && <span className="micro-label text-zinc-300">{hint}</span>}
                          <span className="text-2xs text-zinc-500">{t("plugins.revealHint")}</span>
                        </button>
                      )}
                    </div>
                  </li>
                );
              })}
            </ul>
          )}

          {!asCards && picks.length > 0 && (
            <ul className="space-y-2">
              {picks.map(({ game, note }) => {
                const coverUrl = usableCover(resolveCoverUrl(game.coverPath, game.updatedAt));
                return (
                  <li
                    key={game.id}
                    data-pick={game.id}
                    className="flex items-center gap-3.5 rounded-lg border border-oled-border bg-oled-bg/50 p-2.5"
                  >
                    <div className="relative h-[4.5rem] w-12 shrink-0 overflow-hidden rounded border border-oled-border bg-oled-bg">
                      {coverUrl && <Cover url={coverUrl} onError={() => markBroken(coverUrl)} />}
                    </div>
                    <div className="min-w-0 flex-1">
                      <p className="truncate text-sm font-medium text-zinc-100" title={game.title}>
                        {game.title}
                      </p>
                      <p className="truncate text-2xs text-zinc-500">
                        {note ?? platformLabel(game.platform, t)}
                      </p>
                    </div>
                    <div className="flex shrink-0 items-center gap-1.5">
                      <button
                        type="button"
                        onClick={() => onOpenGame(game)}
                        className="rounded-lg border border-oled-border bg-oled-bg/60 px-2.5 py-1.5 text-2xs font-medium text-zinc-300 transition-colors hover:border-zinc-600"
                      >
                        {t("plugins.openGame")}
                      </button>
                      <button
                        type="button"
                        onClick={() => onLaunch(game)}
                        aria-label={t("game.launchNamed", { title: game.title })}
                        className="inline-flex items-center gap-1.5 rounded-lg bg-accent px-3 py-1.5 text-2xs font-semibold text-zinc-950 transition-colors hover:bg-accent-hover"
                      >
                        <Play className="h-3 w-3 fill-current" />
                        <span>{t("common.play")}</span>
                      </button>
                    </div>
                  </li>
                );
              })}
            </ul>
          )}
        </div>

        <div className="flex shrink-0 items-center justify-end gap-2.5 border-t border-oled-border px-6 py-4">
          {covered.length > 0 && (
            <button
              id="plugin-command-reveal-all"
              type="button"
              onClick={() => reveal(covered.map(({ game }) => game.id))}
              className="mr-auto inline-flex items-center gap-1.5 rounded-lg border border-oled-border bg-oled-bg px-3.5 py-1.5 text-xs font-medium text-zinc-300 transition-colors hover:bg-oled-elevated"
            >
              <Eye className="h-3.5 w-3.5" />
              <span>{t("plugins.revealAll")}</span>
            </button>
          )}
          <button
            id="plugin-command-again"
            type="button"
            onClick={onRunAgain}
            disabled={run.isRunning}
            className="inline-flex items-center gap-1.5 rounded-lg border border-oled-border bg-oled-bg px-3.5 py-1.5 text-xs font-medium text-zinc-300 transition-colors hover:bg-oled-elevated disabled:opacity-60"
          >
            <RefreshCw className={`h-3.5 w-3.5 ${run.isRunning ? "animate-spin" : ""}`} />
            <span>{t("plugins.again")}</span>
          </button>
          <button
            type="button"
            onClick={onClose}
            className="rounded-lg bg-accent px-4 py-1.5 text-xs font-semibold text-zinc-950 transition-colors hover:bg-accent-hover"
          >
            {t("common.close")}
          </button>
        </div>
      </div>
    </div>
  );
}
