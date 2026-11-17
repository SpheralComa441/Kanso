import { useState, type CSSProperties } from "react";
import { Download, PanelRightOpen, Play } from "lucide-react";
import { useI18n } from "../i18n";
import { resolveCoverUrl } from "../lib/backend";
import { canInstall, platformLabel } from "../lib/platforms";
import { useBrokenCovers } from "../lib/useBrokenCovers";
import type { DetectedGame, PluginView } from "../types/game";
import { Cover } from "./Cover";

/** A library game a plugin points at, with what the plugin says about it. */
export interface PickedGame {
  game: DetectedGame;
  note: string | null;
  hint: string | null;
}

/** The games a plugin's answer points at, in its order; ids that are not in `games` are left out. */
export function resolvePicks(view: PluginView | null, games: DetectedGame[]): PickedGame[] {
  return (view?.games ?? []).flatMap((pick) => {
    const game = games.find((candidate) => candidate.id === pick.id);
    return game ? [{ game, note: pick.note, hint: pick.hint }] : [];
  });
}

/**
 * Face-down cards (`gamesStyle: "mystery"`): which ones the user has turned. Every new answer
 * is a new deal, with every card covered again.
 */
export function usePickReveal(view: PluginView | null) {
  const [dealt, setDealt] = useState(view);
  const [deal, setDeal] = useState(0);
  const [revealed, setRevealed] = useState<ReadonlySet<string>>(new Set());
  if (view !== dealt) {
    setDealt(view);
    setDeal(deal + 1);
    setRevealed(new Set());
  }
  const reveal = (ids: string[]) => setRevealed((current) => new Set([...current, ...ids]));
  return { deal, revealed, reveal };
}

interface PluginPicksProps {
  picks: PickedGame[];
  style: PluginView["gamesStyle"];
  /** Starts the ids of the buttons: unique among the picks on screen (`pick` in the dialog). */
  idPrefix: string;
  /** From {@link usePickReveal}: the deal, the cards turned so far, and how to turn more. */
  deal: number;
  revealed: ReadonlySet<string>;
  onReveal: (ids: string[]) => void;
  /**
   * The picks are on a page, not in a dialog, and this is how many covers its shelves hold
   * in a row: the covers take that width, start at the left and wrap; rows sit side by side.
   */
  columns?: number;
  onOpenGame: (game: DetectedGame) => void;
  onLaunch: (game: DetectedGame) => void;
}

/**
 * The games a plugin suggests, laid out as it asked: rows, whole covers, or whole covers
 * face down until the user turns each one. Used by the dialog of a plugin command and by the
 * parts plugins add to the Home page.
 */
export function PluginPicks({
  picks,
  style,
  idPrefix,
  deal,
  revealed,
  onReveal,
  columns,
  onOpenGame,
  onLaunch,
}: PluginPicksProps) {
  const { t } = useI18n();
  const { usableCover, markBroken } = useBrokenCovers();
  if (picks.length === 0) return null;
  const wide = columns !== undefined;

  const turn = (game: DetectedGame, index: number) => {
    onReveal([game.id]);
    // The card that was a button is now a game: the keyboard goes on to what starts it
    requestAnimationFrame(() =>
      document.getElementById(`${idPrefix}-launch-${index}`)?.focus({ preventScroll: true })
    );
  };

  if (style === "list") {
    return (
      <ul
        data-picks-style="list"
        className={wide ? "grid grid-cols-[repeat(auto-fill,minmax(min(100%,24rem),1fr))] gap-2" : "space-y-2"}
      >
        {picks.map(({ game, note }, index) => {
          const coverUrl = usableCover(resolveCoverUrl(game.coverPath, game.updatedAt));
          // Something to start, or to hand to its store for installing; otherwise the sheet alone
          const actionable = game.isInstalled || canInstall(game);
          return (
            <li
              key={game.id}
              data-pick={game.id}
              className="motion-row flex items-center gap-3.5 rounded-lg border border-oled-border bg-oled-bg/50 p-2.5"
              style={{ "--i": index } as CSSProperties}
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
                {actionable && (
                  <button
                    type="button"
                    onClick={() => onLaunch(game)}
                    aria-label={t(game.isInstalled ? "game.launchNamed" : "game.installNamed", {
                      title: game.title,
                    })}
                    className="inline-flex items-center gap-1.5 rounded-lg bg-accent px-3 py-1.5 text-2xs font-semibold text-zinc-950 transition-colors hover:bg-accent-hover"
                  >
                    {game.isInstalled ? (
                      <Play className="h-3 w-3 fill-current" />
                    ) : (
                      <Download className="h-3 w-3" />
                    )}
                    <span>{t(game.isInstalled ? "common.play" : "common.install")}</span>
                  </button>
                )}
              </div>
            </li>
          );
        })}
      </ul>
    );
  }

  return (
    <ul
      key={deal}
      className="pick-grid"
      data-picks-style={style}
      data-picks-wide={wide || undefined}
      style={{ "--picks": wide ? columns : Math.min(picks.length, 4) } as CSSProperties}
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
                        <h3 className="line-clamp-2 text-sm font-semibold leading-snug">{game.title}</h3>
                        <p className="line-clamp-2 text-2xs text-white/75">
                          {note ?? platformLabel(game.platform, t)}
                        </p>
                      </div>
                      <div className="pointer-events-auto flex items-stretch gap-1.5">
                        {actionable && (
                          <button
                            id={`${idPrefix}-launch-${index}`}
                            type="button"
                            onClick={() => onLaunch(game)}
                            aria-label={t(game.isInstalled ? "game.launchNamed" : "game.installNamed", {
                              title: game.title,
                            })}
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
                          id={actionable ? undefined : `${idPrefix}-launch-${index}`}
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
                  id={`${idPrefix}-reveal-${index}`}
                  type="button"
                  onClick={() => turn(game, index)}
                  disabled={open}
                  aria-hidden={open}
                  aria-label={t("plugins.reveal", { n: index + 1 }) + (hint ? `: ${hint}` : "")}
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
  );
}
