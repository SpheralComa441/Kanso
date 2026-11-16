import {
  AlertTriangle,
  CloudDownload,
  Download,
  FolderOpen,
  Gamepad2,
  Play,
  Star,
} from "lucide-react";
import { useI18n } from "../i18n";
import { resolveCoverUrl } from "../lib/backend";
import { PLATFORM_META, canInstall, platformLabel } from "../lib/platforms";
import { useBrokenCovers } from "../lib/useBrokenCovers";
import { useWindowedItems } from "../lib/useWindowedItems";
import type { BadgeEntry, CardInfoItem, DetectedGame } from "../types/game";
import { Cover } from "./Cover";
import type { ViewMode } from "./Header";
import { STATUS_LABEL_KEYS, StatusIcon } from "./StatusIcon";

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
  /** Short labels plugins have for the games, by game id. */
  badges: Record<string, BadgeEntry[]>;
  /** Games Kanso sees running right now. */
  runningIds: ReadonlySet<string>;
  /** What a cover tells under the pointer besides the title, as the user chose. */
  cardInfo: readonly CardInfoItem[];
}

/** One line of what a cover tells: what the figure is, and the figure. */
interface CardFact {
  key: string;
  label: string;
  value: string;
  /** The plugin behind the figure, when it is a plugin's. */
  pluginId?: string;
  title?: string;
}

/** The plugins' labels for a game, e.g. "27 h · 89%", each explained by its tooltip. */
function PluginBadges({ entries }: { entries: BadgeEntry[] | undefined }) {
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

interface FavoriteButtonProps {
  game: DetectedGame;
  onToggle: (game: DetectedGame) => void;
  className?: string;
}

function FavoriteButton({ game, onToggle, className = "" }: FavoriteButtonProps) {
  const { t } = useI18n();
  return (
    <button
      type="button"
      onClick={() => onToggle(game)}
      aria-pressed={game.isFavorite}
      aria-label={t(game.isFavorite ? "fav.removeNamed" : "fav.addNamed", {
        title: game.title,
      })}
      title={t(game.isFavorite ? "fav.remove" : "fav.add")}
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
  badges,
  runningIds,
  cardInfo,
}: GameListProps) {
  const { t, duration, relativePast } = useI18n();

  /** The lines a cover shows under the pointer for this game, of those the user asked for. */
  const factsOf = (game: DetectedGame): CardFact[] => {
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
  const { usableCover, markBroken } = useBrokenCovers();
  // Only the rows on screen exist: a library of thousands costs what a screenful costs
  const windowed = useWindowedItems<HTMLElement>(games.length, viewMode);

  if (isScanning && totalDetected === 0) {
    return (
      <div className="flex flex-col items-center justify-center py-24 text-center">
        <div className="mb-3 h-6 w-6 animate-spin rounded-full border-2 border-oled-border border-t-accent" />
        <p className="text-sm font-medium text-zinc-300">{t("list.searching")}</p>
        <p className="mt-1 max-w-md text-xs text-zinc-500">
          {t("list.searchingHint")}
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
            <ul className="min-w-0 select-text space-y-1 break-words">
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
            {t("list.emptyTitle")}
          </h2>
          <p className="mx-auto mt-1.5 max-w-md text-xs leading-relaxed text-zinc-400">
            {t("list.emptyBody")}
          </p>
          <button
            id="empty-add-folder-button"
            type="button"
            onClick={onOpenSettings}
            className="mt-5 inline-flex items-center gap-1.5 rounded-lg bg-accent px-3.5 py-2 text-xs font-semibold text-zinc-950 transition-colors hover:bg-accent-hover"
          >
            <FolderOpen className="h-3.5 w-3.5" />
            <span>{t("list.addFolders")}</span>
          </button>
        </div>
      ) : games.length === 0 ? (
        <div className="rounded-xl border border-oled-border bg-oled-surface/40 px-6 py-12 text-center">
          <p className="text-sm text-zinc-400">{t("list.noMatch")}</p>
        </div>
      ) : viewMode === "grid" ? (
        <div
          className="game-grid"
          ref={windowed.containerRef as React.Ref<HTMLDivElement>}
          style={windowed.style}
          data-total={games.length}
        >
          {games.slice(windowed.start, windowed.end).map((game, offset) => {
            const index = windowed.start + offset;
            const isLaunching = launchingId === game.id;
            const label = platformLabel(game.platform, t);
            const coverUrl = usableCover(
              resolveCoverUrl(game.coverPath, game.updatedAt)
            );
            const actionable = game.isInstalled || canInstall(game);
            const facts = factsOf(game);

            return (
              <article
                key={game.id}
                className={`game-card group ${coverUrl ? "" : "game-card-no-art"}`}
                // Its place in the grid: cards come in one after the other with the view;
                // those that scrolling brings in are simply there
                style={{ "--i": index } as React.CSSProperties}
                data-arrived={windowed.arrivesLater() || undefined}
              >
                {/* 2:3 vertical cover; washed out for a game that is not on disk */}
                <div className="game-card-cover relative w-full overflow-hidden bg-oled-bg">
                  {coverUrl ? (
                    <div
                      className={`h-full w-full ${
                        game.isInstalled ? "" : "opacity-45 grayscale"
                      }`}
                    >
                      <Cover
                        url={coverUrl}
                        zoomOnHover
                        onError={() => markBroken(coverUrl)}
                      />
                    </div>
                  ) : (
                    <div className="game-card-blank flex h-full w-full flex-col justify-end bg-gradient-to-b from-oled-elevated/50 to-oled-bg p-4">
                      <p className="line-clamp-4 text-sm font-semibold leading-snug text-zinc-300">
                        {game.title}
                      </p>
                    </div>
                  )}

                  {/* Over artwork of any color: fixed dark chips, independent of the theme */}
                  <div className="absolute left-2.5 top-2.5 flex max-w-[calc(100%-3.25rem)] items-center gap-1">
                    <span className="truncate rounded-md border border-white/15 bg-black/75 px-2 py-0.5 text-3xs font-medium text-white">
                      {label}
                    </span>
                    {game.status && (
                      <span
                        data-status={game.status}
                        title={t("game.statusBadge", { status: t(STATUS_LABEL_KEYS[game.status]) })}
                        className="shrink-0 rounded-md border border-white/15 bg-black/75 p-1 text-white"
                      >
                        <StatusIcon status={game.status} className="h-3 w-3" />
                      </span>
                    )}
                    {runningIds.has(game.id) && (
                      <span
                        data-running={game.id}
                        title={t("detail.running")}
                        className="shrink-0 rounded-md border border-emerald-400/40 bg-black/75 p-[0.3125rem]"
                      >
                        <span className="block h-2 w-2 rounded-full bg-emerald-400" />
                      </span>
                    )}
                    {!game.isInstalled && (
                      <span
                        data-not-installed={game.id}
                        title={t("game.notInstalled")}
                        className="shrink-0 rounded-md border border-white/15 bg-black/75 p-1 text-white"
                      >
                        <CloudDownload className="h-3 w-3" aria-hidden="true" />
                      </span>
                    )}
                  </div>

                  {/* Whole cover opens the detail view */}
                  <button
                    id={`open-detail-${game.id}`}
                    type="button"
                    onClick={() => onOpenDetail(game)}
                    aria-label={t("game.openDetails", { title: game.title })}
                    className="absolute inset-0 cursor-pointer focus-visible:outline focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-accent"
                  />

                  <FavoriteButton
                    game={game}
                    onToggle={onToggleFavorite}
                    className={`absolute right-2.5 top-2.5 rounded-lg border border-white/10 bg-black/75 p-1.5 text-white/80 transition-opacity hover:bg-black/85 hover:text-white focus-visible:opacity-100 group-hover:opacity-100 ${
                      game.isFavorite ? "opacity-100" : "opacity-0"
                    }`}
                  />

                  {/* Under the pointer: play, or install through the store's client, and, in
                      themes whose cards have no caption, what matters about the game over
                      its blurred cover. Clicks pass through to the cover, which opens the
                      sheet; only the button takes them. */}
                  <div className="game-card-hover pointer-events-none absolute inset-x-0 bottom-0 flex items-end bg-gradient-to-t from-black/90 via-black/55 to-transparent p-3 opacity-0 transition-opacity duration-200 focus-within:opacity-100 group-hover:opacity-100">
                    <div className="game-card-info space-y-1.5 text-white">
                      <h3 className="line-clamp-3 text-sm font-semibold leading-snug">{game.title}</h3>
                      {!game.isInstalled && (
                        <p data-card-absent={game.id} className="text-2xs text-white/70">
                          {t("game.notInstalled")}
                        </p>
                      )}
                      {cardInfo.includes("genres") && (
                        <p data-card-info="genres" className="truncate text-2xs text-white/75">
                          {game.genres.length > 0 ? game.genres.slice(0, 3).join(" · ") : label}
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
                              <dd
                                data-badge={fact.pluginId}
                                className="shrink-0 font-medium tabular-nums text-white"
                              >
                                {fact.value}
                              </dd>
                            </div>
                          ))}
                        </dl>
                      )}
                    </div>
                    {actionable && (
                      <button
                        id={`launch-grid-${game.id}`}
                        type="button"
                        onClick={() => onLaunch(game)}
                        disabled={isLaunching}
                        aria-label={t(
                          game.isInstalled ? "game.launchNamed" : "game.installNamed",
                          { title: game.title }
                        )}
                        className="pointer-events-auto inline-flex w-full items-center justify-center gap-1.5 rounded-lg bg-accent py-1.5 text-xs font-semibold text-zinc-950 shadow-lg transition-colors hover:bg-accent-hover disabled:opacity-60"
                      >
                        {game.isInstalled ? (
                          <Play className="h-3.5 w-3.5 fill-current" />
                        ) : (
                          <Download className="h-3.5 w-3.5" />
                        )}
                        <span>
                          {!game.isInstalled
                            ? t("common.install")
                            : isLaunching
                              ? t("common.launching")
                              : t("common.play")}
                        </span>
                      </button>
                    )}
                  </div>
                </div>

                {/* Card caption */}
                <div className="game-card-caption space-y-1 px-3 py-2.5">
                  <h3
                    className="truncate text-xs font-medium text-zinc-100"
                    title={game.title}
                  >
                    {game.title}
                  </h3>
                  <div className="flex items-center justify-between gap-2 text-2xs text-zinc-500">
                    <p className="min-w-0 truncate">
                      {game.genres.length > 0
                        ? game.genres.slice(0, 2).join(", ")
                        : label}
                    </p>
                    <PluginBadges entries={badges[game.id]} />
                  </div>
                </div>
              </article>
            );
          })}
        </div>
      ) : (
        <div className="overflow-hidden rounded-xl border border-oled-border bg-oled-surface/50">
          <ul
            className="divide-y divide-oled-border"
            ref={windowed.containerRef as React.Ref<HTMLUListElement>}
            style={windowed.style}
            data-total={games.length}
          >
            {games.slice(windowed.start, windowed.end).map((game) => {
              const platformMeta = PLATFORM_META[game.platform];
              const label = platformLabel(game.platform, t);
              const isLaunching = launchingId === game.id;
              const coverUrl = usableCover(
                resolveCoverUrl(game.coverPath, game.updatedAt)
              );

              return (
                <li
                  key={game.id}
                  className="flex items-center justify-between gap-3 px-4 py-2.5 transition-colors hover:bg-oled-elevated/60"
                >
                  <button
                    id={`open-detail-${game.id}`}
                    type="button"
                    onClick={() => onOpenDetail(game)}
                    aria-label={t("game.openDetails", { title: game.title })}
                    className="flex min-w-0 flex-1 items-center gap-3.5 rounded-md text-left focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent"
                  >
                    <div
                      className={`relative h-14 w-[38px] shrink-0 overflow-hidden rounded border border-oled-border bg-oled-bg ${
                        game.isInstalled ? "" : "opacity-45 grayscale"
                      }`}
                    >
                      {coverUrl && (
                        <Cover url={coverUrl} onError={() => markBroken(coverUrl)} />
                      )}
                    </div>

                    <div className="min-w-0 flex-1 space-y-1">
                      <div className="flex items-center gap-2">
                        <span className="truncate text-sm font-medium text-zinc-100">
                          {game.title}
                        </span>
                        <span
                          className={`shrink-0 rounded-md border px-2 py-0.5 text-3xs font-medium ${platformMeta.badgeClass}`}
                        >
                          {platformLabel(game.platform, t)}
                        </span>
                        {game.status && (
                          <span
                            data-status={game.status}
                            className="inline-flex shrink-0 items-center gap-1 text-3xs text-zinc-400"
                          >
                            <StatusIcon status={game.status} className="h-3 w-3" />
                            {t(STATUS_LABEL_KEYS[game.status])}
                          </span>
                        )}
                        {runningIds.has(game.id) && (
                          <span
                            data-running={game.id}
                            className="inline-flex shrink-0 items-center gap-1 text-3xs text-emerald-300"
                          >
                            <span className="h-1.5 w-1.5 rounded-full bg-emerald-400" />
                            {t("detail.running")}
                          </span>
                        )}
                        {!game.isInstalled && (
                          <span
                            data-not-installed={game.id}
                            className="inline-flex shrink-0 items-center gap-1 text-3xs text-zinc-400"
                          >
                            <CloudDownload className="h-3 w-3" aria-hidden="true" />
                            {t("game.notInstalled")}
                          </span>
                        )}
                      </div>
                      <p className="truncate text-2xs text-zinc-500">
                        {game.genres.length > 0
                          ? game.genres.slice(0, 4).join(" · ")
                          : (game.executablePath ?? game.installDir) || label}
                      </p>
                    </div>
                  </button>

                  <div className="flex shrink-0 items-center gap-2">
                    <span className="text-2xs">
                      <PluginBadges entries={badges[game.id]} />
                    </span>
                    <FavoriteButton
                      game={game}
                      onToggle={onToggleFavorite}
                      className="rounded-lg p-2 text-zinc-500 transition-colors hover:bg-oled-elevated hover:text-zinc-200"
                    />
                    {(game.isInstalled || canInstall(game)) && (
                      <button
                        id={`launch-list-${game.id}`}
                        type="button"
                        onClick={() => onLaunch(game)}
                        disabled={isLaunching}
                        className="inline-flex items-center gap-1.5 rounded-lg bg-accent px-3.5 py-1.5 text-xs font-semibold text-zinc-950 transition-colors hover:bg-accent-hover disabled:cursor-wait disabled:opacity-60"
                      >
                        {game.isInstalled ? (
                          <Play className="h-3.5 w-3.5 fill-current" />
                        ) : (
                          <Download className="h-3.5 w-3.5" />
                        )}
                        <span>
                          {!game.isInstalled
                            ? t("common.install")
                            : isLaunching
                              ? t("common.launching")
                              : t("common.play")}
                        </span>
                      </button>
                    )}
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
