import { useEffect, useState } from "react";
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

/**
 * Achievement progress of a Steam game. The counts come with the library; the list is read
 * from Steam's cache only when the user asks to see it.
 */
export function AchievementsSection({ game }: { game: DetectedGame }) {
  const { t, number } = useI18n();
  const [isOpen, setIsOpen] = useState(false);
  const [details, setDetails] = useState<GameAchievements | null | "loading">(null);
  const total = game.achievementsTotal ?? 0;
  const unlocked = Math.min(game.achievementsUnlocked ?? 0, total);

  // Another game: start closed again
  useEffect(() => {
    setIsOpen(false);
    setDetails(null);
  }, [game.id]);

  if (total <= 0) return null;

  const toggle = () => {
    const next = !isOpen;
    setIsOpen(next);
    if (next && details === null) {
      setDetails("loading");
      getGameAchievements(game)
        .then((loaded) => setDetails(loaded ?? { total, unlocked, items: [] }))
        .catch(() => setDetails({ total, unlocked, items: [] }));
    }
  };

  const percent = Math.round((unlocked / total) * 100);
  const loaded = details !== null && details !== "loading" ? details : null;

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

      <button
        id="toggle-achievements-button"
        type="button"
        onClick={toggle}
        aria-expanded={isOpen}
        className="inline-flex items-center gap-1 text-2xs text-zinc-400 transition-colors hover:text-zinc-100"
      >
        <ChevronDown className={`h-3 w-3 transition-transform ${isOpen ? "rotate-180" : ""}`} />
        <span>{isOpen ? t("achievements.hide") : t("achievements.show")}</span>
      </button>

      {isOpen && details === "loading" && (
        <p className="text-2xs text-zinc-500">{t("achievements.loading")}</p>
      )}
      {isOpen && loaded && loaded.items.length === 0 && (
        <p className="text-2xs leading-relaxed text-zinc-500">{t("achievements.unavailable")}</p>
      )}
      {isOpen && loaded && loaded.items.length > 0 && (
        <>
          <ul className="divide-y divide-oled-border/60">
            {loaded.items.map((achievement) => (
              <AchievementRow key={achievement.id} achievement={achievement} />
            ))}
          </ul>
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

function PluginPanel({ plugin, game }: { plugin: PluginInfo; game: DetectedGame }) {
  const { t, shortDate } = useI18n();
  const [state, setState] = useState<PanelState>({ phase: "loading" });
  const [attempt, setAttempt] = useState(0);
  const gameId = game.id;

  useEffect(() => {
    let alive = true;
    setState({ phase: "loading" });
    const failed = (error: unknown): GameDataAnswer => ({
      pluginId: plugin.id,
      pluginName: plugin.name,
      data: null,
      error: typeof error === "string" ? error : t("plugins.actionError"),
      logs: [],
      fetchedAt: Date.now(),
    });
    // The first load may use the cache; "try again" and "refresh" always ask the plugin
    pluginGameData(plugin.id, game, attempt > 0)
      .then((answer) => alive && setState({ phase: "done", answer }))
      .catch((error) => alive && setState({ phase: "done", answer: failed(error) }));
    return () => {
      alive = false;
    };
    // `game` and `t` change identity on every library update; the id is what matters
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [plugin.id, gameId, attempt]);

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

  const refreshButton = (
    <button
      type="button"
      onClick={() => setAttempt((n) => n + 1)}
      title={t("plugins.refresh", { name: plugin.name })}
      aria-label={t("plugins.refresh", { name: plugin.name })}
      className="rounded p-0.5 text-zinc-600 transition-colors hover:text-zinc-200"
    >
      <RefreshCw className="h-3 w-3" />
    </button>
  );

  return (
    <>
      {sections.map((section, index) => (
        <section key={index} className="space-y-2" data-plugin-section={plugin.id}>
          <div className="flex items-center justify-between gap-3">
            <SectionHeading>{section.title ?? plugin.name}</SectionHeading>
            {index === 0 && refreshButton}
          </div>
          <PluginBlocks blocks={section.blocks} />
        </section>
      ))}

      {answer.error && (
        <div className="space-y-1 text-2xs" data-plugin-error={plugin.id}>
          <p className="text-zinc-500">
            <span className="select-text">
              {sections.length > 0
                ? `${plugin.name} — ${t("plugins.stale", { date: shortDate(answer.fetchedAt) })}`
                : `${plugin.name} — ${answer.error}`}
            </span>{" "}
            <button
              type="button"
              onClick={() => setAttempt((n) => n + 1)}
              className="text-zinc-300 underline-offset-2 hover:text-zinc-100 hover:underline"
            >
              {t("plugins.retry")}
            </button>
          </p>
          {answer.logs.length > 0 && (
            <details className="text-zinc-600">
              <summary className="cursor-pointer hover:text-zinc-400">
                {t("plugins.technical")}
              </summary>
              <pre className="mt-1 max-h-32 select-text overflow-auto whitespace-pre-wrap rounded-md bg-oled-bg/60 p-2 font-mono text-3xs leading-relaxed text-zinc-500">
                {(sections.length > 0 ? [answer.error, ...answer.logs] : answer.logs).join("\n")}
              </pre>
            </details>
          )}
        </div>
      )}
    </>
  );
}

interface GamePluginSectionsProps {
  game: DetectedGame;
  /** The enabled plugins that add something to a game's sheet. */
  plugins: PluginInfo[];
}

/** What each enabled plugin has to say about a game; every plugin loads on its own. */
export function GamePluginSections({ game, plugins }: GamePluginSectionsProps) {
  return (
    <>
      {plugins.map((plugin) => (
        <PluginPanel key={plugin.id} plugin={plugin} game={game} />
      ))}
    </>
  );
}
