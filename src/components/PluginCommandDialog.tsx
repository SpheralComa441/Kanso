import { useEffect } from "react";
import { Eye, RefreshCw, X } from "lucide-react";
import { useI18n } from "../i18n";
import type { DetectedGame, PluginView } from "../types/game";
import { PluginSections } from "./PluginBlocks";
import { PluginPicks, resolvePicks, usePickReveal } from "./PluginPicks";

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
  const isOpen = run !== null;
  const view = run?.view ?? null;
  // Face-down cards: which ones the user has turned. Every new answer is a new deal.
  const { deal, revealed, reveal } = usePickReveal(view);

  useEffect(() => {
    if (!isOpen || !isTopmost) return;
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [isOpen, isTopmost, onClose]);

  if (!run) return null;

  const picks = resolvePicks(view, games);
  const style = view?.gamesStyle ?? "list";
  const asCards = style !== "list" && picks.length > 0;
  const covered = style === "mystery" ? picks.filter(({ game }) => !revealed.has(game.id)) : [];
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

          <PluginPicks
            picks={picks}
            style={style}
            idPrefix="pick"
            deal={deal}
            revealed={revealed}
            onReveal={reveal}
            onOpenGame={onOpenGame}
            onLaunch={onLaunch}
          />
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
