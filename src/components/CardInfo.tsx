import { useI18n } from "../i18n";
import { platformLabel } from "../lib/platforms";
import type { BadgeEntry, CardInfoItem, DetectedGame } from "../types/game";
import { STATUS_LABEL_KEYS } from "./StatusIcon";

/** One line of what a cover tells: what the figure is, and the figure. */
export interface CardFact {
  key: string;
  label: string;
  value: string;
  /** The plugin behind the figure, when it is a plugin's. */
  pluginId?: string;
  title?: string;
}

/** The plugins' labels for a game, e.g. "27 h · 89%", each explained by its tooltip. */
export function PluginBadges({ entries }: { entries: BadgeEntry[] | undefined }) {
  if (!entries || entries.length === 0) return null;
  return (
    <span className="flex shrink-0 items-center gap-1.5 tabular-nums text-zinc-400">
      {entries.map((entry, index) => (
        <span
          key={entry.pluginId}
          data-badge={entry.pluginId}
          title={entry.tooltip ?? entry.pluginName}
          className={index > 0 ? "border-l border-oled-border pl-1.5" : ""}
        >
          {entry.text}
        </span>
      ))}
    </span>
  );
}

/**
 * The lines a cover shows under the pointer for a game, of those the user asked for: the
 * same on every page that draws covers.
 */
export function useCardFacts(
  cardInfo: readonly CardInfoItem[],
  badges: Record<string, BadgeEntry[]>
): (game: DetectedGame) => CardFact[] {
  const { t, duration, relativePast } = useI18n();

  return (game) => {
    const facts: CardFact[] = [];
    // A game that is not on disk and was never played has no play time worth a line
    if (cardInfo.includes("playtime") && (game.isInstalled || game.playtimeMinutes)) {
      facts.push({
        key: "playtime",
        label: t("cardInfo.playtime"),
        value: game.playtimeMinutes ? duration(game.playtimeMinutes) : t("detail.never"),
      });
    }
    if (cardInfo.includes("lastPlayed") && game.lastPlayedAt !== null) {
      facts.push({
        key: "lastPlayed",
        label: t("cardInfo.lastPlayed"),
        value: relativePast(game.lastPlayedAt),
      });
    }
    if (cardInfo.includes("status") && game.status) {
      facts.push({
        key: "status",
        label: t("cardInfo.status"),
        value: t(STATUS_LABEL_KEYS[game.status]),
      });
    }
    if (cardInfo.includes("achievements") && game.achievementsTotal) {
      facts.push({
        key: "achievements",
        label: t("cardInfo.achievements"),
        value: t("achievements.count", {
          unlocked: game.achievementsUnlocked ?? 0,
          total: game.achievementsTotal,
        }),
      });
    }
    if (cardInfo.includes("badges")) {
      for (const entry of badges[game.id] ?? []) {
        facts.push({
          key: `badge-${entry.pluginId}`,
          // A plugin that does not say what its figure is goes by its own name
          label: entry.label ?? entry.pluginName,
          value: entry.text,
          pluginId: entry.pluginId,
          title: entry.tooltip ?? entry.pluginName,
        });
      }
    }
    return facts;
  };
}

interface CardInfoPanelProps {
  game: DetectedGame;
  cardInfo: readonly CardInfoItem[];
  facts: CardFact[];
  /** Left out where the title already stands under the cover. */
  showTitle?: boolean;
}

/**
 * What a cover tells about its game under the pointer, in themes whose cards have no
 * caption: title, genres, tags and the figures the user asked for. Shown by the style sheet
 * (`.game-card-info`), over the blurred cover.
 */
export function CardInfoPanel({ game, cardInfo, facts, showTitle = true }: CardInfoPanelProps) {
  const { t } = useI18n();

  return (
    <div className="game-card-info space-y-1.5 text-white">
      {showTitle && <h3 className="line-clamp-3 text-sm font-semibold leading-snug">{game.title}</h3>}
      {!game.isInstalled && (
        <p data-card-absent={game.id} className="text-2xs text-white/70">
          {t("game.notInstalled")}
        </p>
      )}
      {cardInfo.includes("genres") && (
        <p data-card-info="genres" className="truncate text-2xs text-white/75">
          {game.genres.length > 0 ? game.genres.slice(0, 3).join(" · ") : platformLabel(game.platform, t)}
        </p>
      )}
      {cardInfo.includes("tags") && game.tags.length > 0 && (
        <p data-card-info="tags" className="line-clamp-2 text-2xs text-white/60">
          {game.tags.slice(0, 4).join(" · ")}
        </p>
      )}
      {facts.length > 0 && (
        <dl className="game-card-facts space-y-1 pt-0.5 text-2xs">
          {facts.map((fact) => (
            <div
              key={fact.key}
              data-card-info={fact.key}
              title={fact.title}
              className="flex items-baseline justify-between gap-3"
            >
              <dt className="min-w-0 truncate text-white/60">{fact.label}</dt>
              <dd data-badge={fact.pluginId} className="shrink-0 font-medium tabular-nums text-white">
                {fact.value}
              </dd>
            </div>
          ))}
        </dl>
      )}
    </div>
  );
}
