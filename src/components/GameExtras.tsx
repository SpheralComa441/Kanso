import { useEffect, useState, useRef } from "react";
import { ChevronDown, Lock, RefreshCw, Trophy } from "lucide-react";
import { useI18n } from "../i18n";
import { getGameAchievements, pluginGameData } from "../lib/backend";
import type {
  Achievement,
  DetectedGame,
  GameAchievements,
  GameDataAnswer,
  PluginInfo,
} from "../types/game";
import { PluginBlocks } from "./PluginBlocks";

function SectionHeading({ children }: { children: React.ReactNode }) {
  return (
    <h3 className="micro-label">{children}</h3>
  );
}

function AchievementRow({ achievement }: { achievement: Achievement }) {
  const { t, relativePast, number } = useI18n();
  const [iconBroken, setIconBroken] = useState(false);
  const FallbackIcon = achievement.unlocked ? Trophy : Lock;

  return (
    <li className="flex items-start gap-3 py-2">
      <span
        className={`flex h-9 w-9 shrink-0 items-center justify-center overflow-hidden rounded-md border border-oled-border bg-oled-bg ${
          achievement.unlocked ? "" : "opacity-50 grayscale"
        }`}
      >
        {achievement.iconUrl && !iconBroken ? (
          <img
            src={achievement.iconUrl}
            alt=""
            loading="lazy"
            referrerPolicy="no-referrer"
            onError={() => setIconBroken(true)}
            className="h-full w-full object-cover"
          />
        ) : (
          <FallbackIcon className="h-4 w-4 text-zinc-500" />
        )}
      </span>
      <span className="min-w-0 flex-1">
        <span
          className={`block select-text text-xs font-medium ${
            achievement.unlocked ? "text-zinc-100" : "text-zinc-300"
          }`}
        >
          {achievement.name}
        </span>
        {achievement.description && (
          <span className="block select-text text-2xs leading-relaxed text-zinc-500">
            {achievement.description}
          </span>
        )}
        {achievement.progress !== null && (
          <span className="mt-1 block h-1 w-24 overflow-hidden rounded-full bg-accent/15">
            <span
              className="block h-full rounded-full bg-accent"
              style={{ width: `${Math.round(achievement.progress * 100)}%` }}
            />
          </span>
        )}
      </span>
      <span className="shrink-0 pt-0.5 text-right text-3xs text-zinc-500">
        {achievement.unlocked
          ? achievement.unlockedAt
            ? t("achievements.unlocked", { when: relativePast(achievement.unlockedAt) })
            : ""
          : achievement.globalPercent !== null
            ? t("achievements.global", { percent: number(achievement.globalPercent, 1) })
            : ""}
      </span>
    </li>
  );
}

/** Unlocked achievements shown without asking: the latest ones, a few rows of them. */
const ACHIEVEMENTS_SHOWN_FIRST = 12;

/**
 * Achievement progress of a Steam game. The counts come with the library; the list is read
 * from Steam's cache on this PC when the sheet opens. What the player has unlocked is shown
 * at once, latest first; the whole list, locked ones included, is one click away.
 */
export function AchievementsSection({ game }: { game: DetectedGame }) {
  const { t, number } = useI18n();
  const [isOpen, setIsOpen] = useState(false);
  const [details, setDetails] = useState<GameAchievements | null | "loading">(null);
  const total = game.achievementsTotal ?? 0;
  const unlocked = Math.min(game.achievementsUnlocked ?? 0, total);
  const gameRef = useRef(game);
  gameRef.current = game;

  // Another game: its own list, closed to the latest unlocked again
  useEffect(() => {
    setIsOpen(false);
    if (total <= 0) {
      setDetails(null);
      return;
    }
    let current = true;
    const none = { total, unlocked, items: [] };
    setDetails("loading");
    getGameAchievements(gameRef.current)
      .then((loaded) => current && setDetails(loaded ?? none))
      .catch(() => current && setDetails(none));
    return () => {
      current = false;
    };
    // The list belongs to the game, not to each update of its counts
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [game.id]);

  if (total <= 0) return null;

  const percent = Math.round((unlocked / total) * 100);
  const loaded = details !== null && details !== "loading" ? details : null;
  // The latest unlocked, shown before anyone asks
  const latest = (loaded?.items ?? [])
    .filter((achievement) => achievement.unlocked)
    .sort((a, b) => (b.unlockedAt ?? 0) - (a.unlockedAt ?? 0))
    .slice(0, ACHIEVEMENTS_SHOWN_FIRST);
  const shown = isOpen ? (loaded?.items ?? []) : latest;
  const hasMore = loaded !== null && loaded.items.length > latest.length;
  // With nothing unlocked, or no list in Steam's cache, there is still something to ask for
  const canToggle = hasMore || (loaded !== null && loaded.items.length === 0);

  return (
    <section id="detail-achievements" className="space-y-2">
      <div className="flex items-baseline justify-between gap-3">
        <SectionHeading>{t("achievements.title")}</SectionHeading>
        <span className="text-2xs tabular-nums text-zinc-400">
          {t("achievements.count", { unlocked: number(unlocked), total: number(total) })} ·{" "}
          {percent}%
        </span>
      </div>
      <div
        role="meter"
        aria-label={t("achievements.title")}
        aria-valuemin={0}
        aria-valuemax={total}
        aria-valuenow={unlocked}
        className="h-2 overflow-hidden rounded-full bg-accent/15"
      >
        <div className="h-full rounded-full bg-accent" style={{ width: `${percent}%` }} />
      </div>

      {details === "loading" && (
        <p className="text-2xs text-zinc-500">{t("achievements.loading")}</p>
      )}
      {isOpen && loaded && loaded.items.length === 0 && (
        <p className="text-2xs leading-relaxed text-zinc-500">{t("achievements.unavailable")}</p>
      )}
      {shown.length > 0 && (
        <ul
          id="achievements-list"
          data-achievements={isOpen ? "all" : "latest"}
          className="achievement-list"
        >
          {shown.map((achievement) => (
            <AchievementRow key={achievement.id} achievement={achievement} />
          ))}
        </ul>
      )}

      {canToggle && (
        <button
          id="toggle-achievements-button"
          type="button"
          onClick={() => setIsOpen(!isOpen)}
          aria-expanded={isOpen}
          aria-controls="achievements-list"
          className="inline-flex items-center gap-1 text-2xs text-zinc-400 transition-colors hover:text-zinc-100"
        >
          <ChevronDown className={`h-3 w-3 transition-transform ${isOpen ? "rotate-180" : ""}`} />
          <span>
            {isOpen
              ? t(latest.length > 0 ? "achievements.showLatest" : "achievements.hide")
              : latest.length > 0
                ? t("achievements.showAll", { total: number(loaded?.items.length ?? total) })
                : t("achievements.show")}
          </span>
        </button>
      )}

      {isOpen && loaded && loaded.items.length > 0 && (
        <>
          <p className="text-3xs leading-relaxed text-zinc-500">
            {loaded.items.length < loaded.total
              ? `${t("achievements.partial", {
                  shown: number(loaded.items.length),
                  total: number(loaded.total),
                })} `
              : ""}
            {t("achievements.source")}
          </p>
        </>
      )}
    </section>
  );
}

type PanelState = { phase: "loading" } | { phase: "done"; answer: GameDataAnswer };

/** What one plugin has to say about a game, and how to ask it again. */
export interface PluginPanelData {
  plugin: PluginInfo;
  state: PanelState;
  /** Asks the plugin again, past its cache. */
  refresh: () => void;
}

/**
 * What every enabled plugin says about a game. Each plugin loads on its own: one that is slow
 * or fails does not hold the others back.
 */
export function usePluginPanels(game: DetectedGame, plugins: PluginInfo[]): PluginPanelData[] {
  const { t } = useI18n();
  const [states, setStates] = useState<Record<string, PanelState>>({});
  const gameRef = useRef(game);
  gameRef.current = game;
  const gameId = game.id;
  const pluginIds = plugins.map((plugin) => plugin.id).join("\n");
  // Which answers still belong on screen: those of this game and of these plugins
  const loadRef = useRef(0);

  const load = (plugin: PluginInfo, force: boolean, generation: number) => {
    const settle = (answer: GameDataAnswer) => {
      if (loadRef.current === generation) {
        setStates((current) => ({ ...current, [plugin.id]: { phase: "done", answer } }));
      }
    };
    setStates((current) => ({ ...current, [plugin.id]: { phase: "loading" } }));
    // The first load may use the cache; "try again" and "refresh" always ask the plugin
    pluginGameData(plugin.id, gameRef.current, force)
      .then(settle)
      .catch((error) =>
        settle({
          pluginId: plugin.id,
          pluginName: plugin.name,
          data: null,
          error: typeof error === "string" ? error : t("plugins.actionError"),
          logs: [],
          fetchedAt: Date.now(),
        })
      );
  };

  useEffect(() => {
    const generation = loadRef.current + 1;
    loadRef.current = generation;
    setStates({});
    for (const plugin of plugins) load(plugin, false, generation);
    // `game`, `plugins` and `t` change identity on every library update; ids are what matter
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [gameId, pluginIds]);

  return plugins.map((plugin) => ({
    plugin,
    state: states[plugin.id] ?? { phase: "loading" },
    refresh: () => load(plugin, true, loadRef.current),
  }));
}

/** Whether the plugin has something to show for this game: sections, or an error to report. */
export function panelHasContent(panel: PluginPanelData): boolean {
  if (panel.state.phase !== "done") return false;
  const { answer } = panel.state;
  return (answer.data?.sections ?? []).length > 0 || Boolean(answer.error);
}

/** What a plugin's card is called: its first section's title, or the plugin's own name. */
export function panelTitle(panel: PluginPanelData): string {
  const first = panel.state.phase === "done" ? panel.state.answer.data?.sections?.[0] : undefined;
  return first?.title ?? panel.plugin.name;
}

/** The button that asks a plugin again for what it shows. */
export function PluginRefreshButton({ panel }: { panel: PluginPanelData }) {
  const { t } = useI18n();
  return (
    <button
      type="button"
      onClick={panel.refresh}
      title={t("plugins.refresh", { name: panel.plugin.name })}
      aria-label={t("plugins.refresh", { name: panel.plugin.name })}
      className="rounded p-1 text-zinc-500 transition-colors hover:text-zinc-100"
    >
      <RefreshCw className="h-3 w-3" />
    </button>
  );
}

/**
 * What a plugin shows for a game. `titled` leaves out the heading of the first section: the
 * card around it already carries it.
 */
export function PluginPanelBody({ panel, titled = false }: { panel: PluginPanelData; titled?: boolean }) {
  const { t } = useI18n();
  const { plugin, state } = panel;

  if (state.phase === "loading") {
    return (
      <p className="animate-pulse text-2xs text-zinc-500" data-plugin-loading={plugin.id}>
        {t("plugins.loading", { name: plugin.name })}
      </p>
    );
  }

  const { answer } = state;
  const sections = answer.data?.sections ?? [];
  // Nothing to say about this game: the plugin stays out of the way
  if (sections.length === 0 && !answer.error) return null;

  return (
    <>
      {sections.map((section, index) => (
        <section key={index} className="space-y-2" data-plugin-section={plugin.id}>
          {!(titled && index === 0) && (
            <div className="flex items-center justify-between gap-3">
              <SectionHeading>{section.title ?? plugin.name}</SectionHeading>
              {index === 0 && <PluginRefreshButton panel={panel} />}
            </div>
          )}
          <PluginBlocks blocks={section.blocks} />
        </section>
      ))}

      {answer.error && (
        <PluginFailure
          pluginId={plugin.id}
          name={plugin.name}
          error={answer.error}
          logs={answer.logs}
          staleSince={sections.length > 0 ? answer.fetchedAt : null}
          onRetry={panel.refresh}
        />
      )}
    </>
  );
}

interface PluginFailureProps {
  pluginId: string;
  name: string;
  error: string;
  /** The plugin's last log lines, for its author. */
  logs: string[];
  /** When the answer still on screen is from (ms); `null` when there is nothing older to show. */
  staleSince: number | null;
  onRetry: () => void;
}

/**
 * The line a plugin gets when its last attempt failed: its name, what went wrong (or, under an
 * older answer that is still shown, since when that answer is) and "Try again", with the
 * technical details folded away. The same in a game's sheet and on the Home page.
 */
export function PluginFailure({ pluginId, name, error, logs, staleSince, onRetry }: PluginFailureProps) {
  const { t, shortDate } = useI18n();
  return (
    <div className="space-y-1 text-2xs" data-plugin-error={pluginId}>
      <p className="text-zinc-500">
        <span className="select-text">
          {staleSince !== null
            ? `${name} — ${t("plugins.stale", { date: shortDate(staleSince) })}`
            : `${name} — ${error}`}
        </span>{" "}
        <button
          type="button"
          onClick={onRetry}
          className="text-zinc-300 underline-offset-2 hover:text-zinc-100 hover:underline"
        >
          {t("plugins.retry")}
        </button>
      </p>
      {logs.length > 0 && (
        <details className="text-zinc-600">
          <summary className="cursor-pointer hover:text-zinc-400">{t("plugins.technical")}</summary>
          <pre className="mt-1 max-h-32 select-text overflow-auto whitespace-pre-wrap rounded-md bg-oled-bg/60 p-2 font-mono text-3xs leading-relaxed text-zinc-500">
            {(staleSince !== null ? [error, ...logs] : logs).join("\n")}
          </pre>
        </details>
      )}
    </div>
  );
}

interface GamePluginSectionsProps {
  game: DetectedGame;
  /** The enabled plugins that add something to a game's sheet. */
  plugins: PluginInfo[];
}

/** What each enabled plugin has to say about a game, one under the other. */
export function GamePluginSections({ game, plugins }: GamePluginSectionsProps) {
  const panels = usePluginPanels(game, plugins);
  return (
    <>
      {panels.map((panel) => (
        <PluginPanelBody key={panel.plugin.id} panel={panel} />
      ))}
    </>
  );
}
