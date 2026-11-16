import { useEffect, useMemo, useRef, useState, useLayoutEffect } from "react";
import { CloudDownload, Download, Eye, EyeOff, Pencil, Play, Plus, Star, X } from "lucide-react";
import { useI18n, type I18n } from "../i18n";
import { getPlaySessions, resolveCoverUrl } from "../lib/backend";
import { GAME_STATUSES, genreKey } from "../lib/library";
import { PLATFORM_META, canInstall, platformLabel } from "../lib/platforms";
import { useBrokenCovers } from "../lib/useBrokenCovers";
import type { DetectedGame, GameStatus, PlaySession, PluginInfo, ThemeDetail } from "../types/game";
import { Cover } from "./Cover";
import { CardRail, PinnedCard, readPinnedCards, writePinnedCards, type DetailCard } from "./CardRail";
import {
  AchievementsSection,
  GamePluginSections,
  PluginPanelBody,
  PluginRefreshButton,
  panelHasContent,
  panelTitle,
  usePluginPanels,
} from "./GameExtras";
import { STATUS_LABEL_KEYS, StatusIcon } from "./StatusIcon";

interface GameDetailProps {
  game: DetectedGame | null;
  /** How the active theme lays the sheet out: separate tiles, or one sheet. */
  layout: ThemeDetail["layout"];
  /** How the active theme lays out the cards after the figures: scrolling, or all in sight. */
  cards: ThemeDetail["cards"];
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
  /** Replaces the genres the user gave the game (the store's own are not touched). */
  onSetGenres: (game: DetectedGame, customGenres: string[]) => void;
  /** The genres of the library, offered while a new one is typed. */
  knownGenres: string[];
  /** Enabled plugins that add sections to the sheet. */
  plugins: PluginInfo[];
  /** For an emulated game, the name of the emulator that runs it. */
  emulatorName: string | null;
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

/* ------------------------------------------------- pieces both layouts share */

type Piece = Pick<GameDetailProps, "onSelectGenre" | "onSelectTag" | "onSetStatus" | "onSetGenres" | "knownGenres"> & {
  game: DetectedGame;
};

function Badges({
  game,
  isRunning,
  emulatorName,
}: {
  game: DetectedGame;
  isRunning: boolean;
  emulatorName: string | null;
}) {
  const { t } = useI18n();
  return (
    <div className="flex flex-wrap items-center gap-2">
      <span
        className={`inline-block rounded-md border px-2 py-0.5 text-3xs font-medium ${PLATFORM_META[game.platform].badgeClass}`}
      >
        {/* An emulated game says which emulator runs it, where a store game names its store */}
        {(game.platform === "emulator" && emulatorName) || platformLabel(game.platform, t)}
      </span>
      {!game.isInstalled && (
        <span
          id="detail-not-installed-badge"
          className="inline-flex items-center gap-1.5 rounded-md border border-oled-border bg-oled-bg/60 px-2 py-0.5 text-3xs font-medium text-zinc-300"
        >
          <CloudDownload className="h-3 w-3" />
          {t("game.notInstalled")}
        </span>
      )}
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
  );
}

function Genres({ game, onSelectGenre, onSelectTag, onSetGenres, knownGenres }: Omit<Piece, "onSetStatus">) {
  const { t } = useI18n();
  const [adding, setAdding] = useState(false);
  const [draft, setDraft] = useState("");
  const own = game.customGenres ?? [];
  const isOwn = (genre: string) => own.some((entry) => entry.toLowerCase() === genre.toLowerCase());
  const chip =
    "rounded-full border border-oled-border bg-oled-bg/60 px-2.5 py-0.5 text-2xs text-zinc-300 transition-colors hover:border-accent/50 hover:text-accent";

  // Another game: nothing half typed is carried over
  useEffect(() => {
    setAdding(false);
    setDraft("");
  }, [game.id]);

  const add = () => {
    const name = draft.trim().replace(/\s+/g, " ");
    setAdding(false);
    setDraft("");
    // A genre the game already has is not added twice: there is nothing to change
    if (!name || game.genres.some((genre) => genre.toLowerCase() === name.toLowerCase())) return;
    onSetGenres(game, [...own, name]);
  };

  return (
    <>
      <ul id="detail-genres" className="flex flex-wrap items-center gap-1.5" aria-label={t("detail.genres")}>
        {game.genres.map((genre) => (
          <li key={genre} className="flex items-center" data-own-genre={isOwn(genre) ? "" : undefined}>
            <button
              type="button"
              onClick={() => onSelectGenre(genreKey(genre))}
              title={t("detail.showGenre", { genre })}
              className={`${chip} ${isOwn(genre) ? "rounded-r-none border-r-0 pr-1.5" : ""}`}
            >
              {genre}
            </button>
            {/* A genre the user added can be taken away again; the store's own cannot */}
            {isOwn(genre) && (
              <button
                type="button"
                onClick={() => onSetGenres(game, own.filter((entry) => entry.toLowerCase() !== genre.toLowerCase()))}
                aria-label={t("detail.removeGenre", { genre })}
                title={t("detail.removeGenre", { genre })}
                className="rounded-r-full border border-oled-border bg-oled-bg/60 py-0.5 pl-1 pr-1.5 text-2xs text-zinc-500 transition-colors hover:border-red-400/50 hover:text-red-300"
              >
                <X className="h-3 w-3" />
              </button>
            )}
          </li>
        ))}
        <li>
          {adding ? (
            <form
              onSubmit={(event) => {
                event.preventDefault();
                add();
              }}
            >
              <input
                id="detail-genre-input"
                autoFocus
                value={draft}
                maxLength={40}
                list="detail-known-genres"
                placeholder={t("detail.genrePlaceholder")}
                aria-label={t("detail.addGenre")}
                onChange={(event) => setDraft(event.target.value)}
                onBlur={add}
                onKeyDown={(event) => {
                  if (event.key === "Escape") {
                    // Leaves the field, not the sheet
                    event.stopPropagation();
                    setDraft("");
                    setAdding(false);
                  }
                }}
                className="w-36 rounded-full border border-accent/60 bg-oled-bg px-2.5 py-0.5 text-2xs text-zinc-100 placeholder:text-zinc-600 focus:outline-none"
              />
              <datalist id="detail-known-genres">
                {knownGenres
                  .filter((genre) => !game.genres.some((own) => own.toLowerCase() === genre.toLowerCase()))
                  .map((genre) => (
                    <option key={genre} value={genre} />
                  ))}
              </datalist>
            </form>
          ) : (
            <button
              id="detail-add-genre-button"
              type="button"
              onClick={() => setAdding(true)}
              aria-label={t("detail.addGenre")}
              title={t("detail.addGenre")}
              className="inline-flex items-center gap-1 rounded-full border border-dashed border-oled-border px-2 py-0.5 text-2xs text-zinc-500 transition-colors hover:border-accent/50 hover:text-accent"
            >
              <Plus className="h-3 w-3" />
              {game.genres.length === 0 && <span>{t("detail.addGenre")}</span>}
            </button>
          )}
        </li>
      </ul>

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
    </>
  );
}

/** Where the user stands with the game: one click sets it, another clears it. */
function StatusPicker({ game, onSetStatus }: Pick<Piece, "game" | "onSetStatus">) {
  const { t } = useI18n();
  return (
    <div role="group" aria-label={t("detail.status")} className="flex flex-wrap items-center gap-1.5">
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
  );
}

function CoverArt({ game, coverUrl, onBroken }: { game: DetectedGame; coverUrl: string | null; onBroken: () => void }) {
  const { t } = useI18n();
  return (
    <div className="relative aspect-[2/3] w-full overflow-hidden">
      {coverUrl ? (
        <Cover url={coverUrl} alt={t("detail.coverAlt", { title: game.title })} onError={onBroken} />
      ) : (
        <div className="flex h-full w-full flex-col items-center justify-center gap-1 bg-gradient-to-b from-oled-elevated/50 to-oled-bg p-6 text-center">
          <span className="line-clamp-4 text-sm font-semibold text-zinc-300">{game.title}</span>
          <span className="text-2xs text-zinc-500">{t("detail.noCover")}</span>
        </div>
      )}
    </div>
  );
}

type ActionsProps = Pick<
  GameDetailProps,
  "isLaunching" | "onLaunch" | "onToggleFavorite" | "onToggleHidden" | "onEdit"
> & {
  game: DetectedGame;
  playButtonRef: React.RefObject<HTMLButtonElement>;
  /** Stacked in a narrow column, the main button as wide as the column. */
  stacked?: boolean;
};

function Actions({
  game,
  isLaunching,
  onLaunch,
  onToggleFavorite,
  onToggleHidden,
  onEdit,
  playButtonRef,
  stacked = false,
}: ActionsProps) {
  const { t } = useI18n();
  // The main button starts the game, or asks its store's client to install it
  const canLaunch = game.isInstalled ? Boolean(game.launchUri || game.executablePath) : canInstall(game);
  const hideLabel = game.isHidden ? t("detail.unhide") : t("detail.hide");
  const secondary =
    "inline-flex items-center justify-center gap-1.5 rounded-lg border px-3.5 py-2 text-xs font-medium transition-colors";

  return (
    <div className={stacked ? "flex flex-col gap-2" : "flex flex-wrap items-center gap-2"}>
      <button
        ref={playButtonRef}
        id="detail-launch-button"
        type="button"
        onClick={() => onLaunch(game)}
        disabled={isLaunching || !canLaunch}
        className={`inline-flex items-center justify-center gap-2 rounded-lg bg-accent px-5 text-sm font-semibold text-zinc-950 transition-colors hover:bg-accent-hover focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent disabled:cursor-not-allowed disabled:opacity-60 ${
          stacked ? "w-full py-2.5" : "min-w-[9rem] py-2"
        }`}
      >
        {game.isInstalled ? <Play className="h-4 w-4 fill-current" /> : <Download className="h-4 w-4" />}
        <span>
          {!game.isInstalled ? t("common.install") : isLaunching ? t("common.launching") : t("common.play")}
        </span>
      </button>

      <div className={stacked ? "flex items-center gap-2" : "contents"}>
        <button
          id="detail-favorite-button"
          type="button"
          onClick={() => onToggleFavorite(game)}
          aria-pressed={game.isFavorite}
          className={`${secondary} ${stacked ? "min-w-0 flex-1" : ""} ${
            game.isFavorite
              ? "border-accent/40 bg-accent/10 text-accent hover:bg-accent/15"
              : "border-oled-border bg-oled-bg/60 text-zinc-300 hover:border-zinc-600"
          }`}
        >
          <Star className={`h-3.5 w-3.5 shrink-0 ${game.isFavorite ? "fill-accent" : ""}`} />
          <span className="truncate">{game.isFavorite ? t("detail.inFavorites") : t("detail.favorite")}</span>
        </button>

        <button
          id="detail-edit-button"
          type="button"
          onClick={() => onEdit(game)}
          title={stacked ? t("common.edit") : undefined}
          aria-label={stacked ? t("common.edit") : undefined}
          className={`${secondary} border-oled-border bg-oled-bg/60 text-zinc-300 hover:border-zinc-600`}
        >
          <Pencil className="h-3.5 w-3.5" />
          {!stacked && <span>{t("common.edit")}</span>}
        </button>

        <button
          id="detail-hide-button"
          type="button"
          onClick={() => onToggleHidden(game)}
          title={hideLabel}
          aria-label={hideLabel}
          className={`rounded-lg p-2 text-zinc-500 transition-colors hover:bg-oled-bg/60 hover:text-zinc-200 ${
            stacked ? "" : "ml-auto"
          }`}
        >
          {game.isHidden ? <Eye className="h-4 w-4" /> : <EyeOff className="h-4 w-4" />}
        </button>
      </div>
    </div>
  );
}

/** Where the game is and how it starts (or how to install it): a technical footnote. */
function TechnicalNotes({ game, withPlayFacts }: { game: DetectedGame; withPlayFacts: boolean }) {
  const { t, plural, relativePast, duration } = useI18n();
  const client = platformLabel(game.platform, t);
  const row = "flex gap-3";
  const term = "w-24 shrink-0 text-zinc-500";
  return (
    <dl className="space-y-2 text-2xs">
      {game.isInstalled ? (
        <>
          <div className={row}>
            <dt className={term}>{t("detail.folder")}</dt>
            <dd className="min-w-0 select-text truncate font-mono text-zinc-400" title={game.installDir}>
              {game.installDir}
            </dd>
          </div>
          <div className={row}>
            <dt className={term}>{t(game.platform === "emulator" ? "detail.gameFile" : "detail.launch")}</dt>
            <dd className="min-w-0 select-text truncate font-mono text-zinc-400" title={launchTarget(game, t)}>
              {launchTarget(game, t)}
            </dd>
          </div>
        </>
      ) : (
        <div className={row}>
          <dt className={term}>{t("detail.launch")}</dt>
          <dd id="detail-install-note" className="min-w-0 text-zinc-400">
            {canInstall(game)
              ? t("detail.installVia", { client })
              : game.platform === "standalone"
                ? t("detail.installLocal")
                : t("detail.installManually", { client })}
          </dd>
        </div>
      )}
      {withPlayFacts && game.playtimeMinutes !== null && (
        <div className={row}>
          <dt className={term}>{t("detail.playtime")}</dt>
          <dd
            id="detail-playtime"
            className="min-w-0 truncate text-zinc-400"
            title={t(game.platform === "steam" ? "detail.playtimeSteam" : "detail.playtimeMeasured")}
          >
            {game.playtimeMinutes > 0 ? duration(game.playtimeMinutes) : t("detail.neverPlayed")}
          </dd>
        </div>
      )}
      {withPlayFacts && game.lastPlayedAt !== null && (
        <div className={row}>
          <dt className={term}>{t("detail.played")}</dt>
          <dd className="min-w-0 truncate text-zinc-400">
            {relativePast(game.lastPlayedAt)}
            {game.playCount > 1 ? ` · ${plural("launches", game.playCount)}` : ""}
          </dd>
        </div>
      )}
    </dl>
  );
}

/* ----------------------------------------------------------------- figures */

/** Days the activity chart looks back on, today included: a month. */
const ACTIVITY_DAYS = 30;
const DAY_MS = 24 * 60 * 60 * 1000;

function startOfDay(timestampMs: number): number {
  const day = new Date(timestampMs);
  day.setHours(0, 0, 0, 0);
  return day.getTime();
}

interface ActivityDay {
  /** Midnight (local time) the day starts on. */
  start: number;
  minutes: number;
}

/** The game's play sessions of the last month, as minutes per day. */
function useActivity(gameId: string | null): ActivityDay[] | null {
  const [sessions, setSessions] = useState<PlaySession[] | null>(null);
  const firstDay = useMemo(
    () => startOfDay(Date.now()) - (ACTIVITY_DAYS - 1) * DAY_MS,
    // A new game is a new look at the calendar
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [gameId]
  );

  useEffect(() => {
    setSessions(null);
    if (!gameId) return;
    let alive = true;
    getPlaySessions(firstDay)
      .then((all) => alive && setSessions(all.filter((session) => session.gameId === gameId)))
      .catch(() => alive && setSessions([]));
    return () => {
      alive = false;
    };
  }, [gameId, firstDay]);

  return useMemo(() => {
    if (!sessions) return null;
    const days: ActivityDay[] = Array.from({ length: ACTIVITY_DAYS }, (_, index) => ({
      // Midnight by the calendar, not 24 hours later: days are not all as long
      start: startOfDay(firstDay + index * DAY_MS + DAY_MS / 2),
      minutes: 0,
    }));
    for (const session of sessions) {
      // The last day that had begun when the session started
      for (let index = days.length - 1; index >= 0; index -= 1) {
        if (days[index].start <= session.startedAt) {
          days[index].minutes += session.seconds / 60;
          break;
        }
      }
    }
    return days;
  }, [sessions, firstDay]);
}

/**
 * Minutes played per day over the last month: one thin bar per day, on one baseline.
 * Shown only when there is something to show. Every value is in the hover/focus label of its
 * bar and in a table for screen readers.
 */
function ActivityChart({ days }: { days: ActivityDay[] }) {
  const { t, duration, shortDate } = useI18n();
  const peak = Math.max(...days.map((day) => day.minutes));
  const total = days.reduce((sum, day) => sum + day.minutes, 0);
  const describe = (day: ActivityDay) =>
    day.minutes > 0
      ? t("detail.activityDay", { date: shortDate(day.start), time: duration(Math.max(1, day.minutes)) })
      : t("detail.activityNoPlay", { date: shortDate(day.start) });

  return (
    <figure id="detail-activity" className="space-y-1.5">
      <figcaption className="flex items-baseline justify-between gap-3">
        <span className="micro-label">{t("detail.activity", { days: ACTIVITY_DAYS })}</span>
        <span className="text-2xs text-zinc-400">{duration(Math.max(1, total))}</span>
      </figcaption>

      <div
        className="grid h-16 items-end gap-0.5 border-b border-oled-border"
        style={{ gridTemplateColumns: `repeat(${days.length}, minmax(0, 1fr))` }}
        aria-hidden="true"
      >
        {days.map((day) => (
          <div
            key={day.start}
            tabIndex={0}
            className="activity-day relative flex h-full items-end justify-center outline-none focus-visible:bg-accent/10"
          >
            {day.minutes > 0 && (
              <span
                className="activity-bar"
                style={{ height: `${Math.max(6, Math.round((day.minutes / peak) * 100))}%` }}
              />
            )}
            <span className="activity-tip">{describe(day)}</span>
          </div>
        ))}
      </div>
      <div className="flex justify-between text-3xs text-zinc-500" aria-hidden="true">
        <span>{shortDate(days[0].start)}</span>
        <span>{t("detail.today")}</span>
      </div>

      <table className="sr-only">
        <caption>{t("detail.activity", { days: ACTIVITY_DAYS })}</caption>
        <tbody>
          {days.map((day) => (
            <tr key={day.start}>
              <td>{describe(day)}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </figure>
  );
}

/** A small figure with its label: the label above, the value, a line of context. */
function Stat({ id, label, value, note }: { id?: string; label: string; value: string; note?: string }) {
  return (
    <div className="tile flex min-w-0 flex-col justify-between gap-2 p-4" data-rank="key">
      <span className="micro-label">{label}</span>
      <div className="min-w-0">
        <p id={id} className="truncate text-xl font-semibold leading-tight text-zinc-50">
          {value}
        </p>
        {note && <p className="mt-0.5 truncate text-2xs text-zinc-500">{note}</p>}
      </div>
    </div>
  );
}

/**
 * Time played: the figure, who counted it, and how much of it was in the last month. The
 * month comes from the sessions Kanso logged: when the game was played in that time and
 * nothing was logged (Kanso was not there to see), the line is left out rather than say zero.
 */
function PlaytimeTile({ game, activity }: { game: DetectedGame; activity: ActivityDay[] | null }) {
  const { t, number, duration } = useI18n();
  const minutes = game.playtimeMinutes;
  // Hours once there is at least one; minutes before
  const [value, unit] =
    minutes === null
      ? ["—", ""]
      : minutes >= 60
        ? [number(minutes / 60, minutes < 600 ? 1 : 0), t("detail.hoursUnit")]
        : [number(minutes), t("detail.minutesUnit")];
  const note =
    minutes === null
      ? t("detail.playtimeUnknown")
      : minutes === 0
        ? t("detail.neverPlayed")
        : t(game.platform === "steam" ? "detail.playtimeSteam" : "detail.playtimeMeasured");
  const recent = activity === null ? null : activity.reduce((sum, day) => sum + day.minutes, 0);
  const playedThisMonth =
    game.lastPlayedAt !== null && game.lastPlayedAt >= Date.now() - ACTIVITY_DAYS * DAY_MS;
  const showRecent = recent !== null && (minutes ?? 0) > 0 && (recent > 0 || !playedThisMonth);

  return (
    <div className="tile flex min-w-0 flex-col justify-between gap-2 p-4" data-rank="key">
      <span className="micro-label">{t("detail.playtime")}</span>
      <div className="min-w-0">
        <p id="detail-playtime" className="flex items-baseline gap-1.5 text-zinc-50">
          <span className="text-3xl font-semibold leading-none tracking-tight">{value}</span>{" "}
          {unit && <span className="text-sm font-medium text-zinc-400">{unit}</span>}
        </p>
        <p className="mt-1 truncate text-2xs text-zinc-500" title={note}>
          {note}
        </p>
      </div>
      {showRecent && (
        <p
          id="detail-playtime-recent"
          className="flex items-baseline justify-between gap-3 border-t border-oled-border pt-2 text-2xs"
        >
          <span className="truncate text-zinc-500">{t("detail.activity", { days: ACTIVITY_DAYS })}</span>
          <span className="shrink-0 font-medium tabular-nums text-zinc-100">
            {recent > 0 ? duration(Math.max(1, Math.round(recent))) : t("detail.recentNone")}
          </span>
        </p>
      )}
    </div>
  );
}

/* ----------------------------------------------------------------- layouts */

/** Narrower than this, the sheet has no room for a column of its own for cover and actions. */
const WIDE_SHEET = "(min-width: 640px)";

/** Whether the window is wide enough for the cover column; follows the window as it resizes. */
function useWideSheet(): boolean {
  const [wide, setWide] = useState(() => window.matchMedia(WIDE_SHEET).matches);
  useEffect(() => {
    const query = window.matchMedia(WIDE_SHEET);
    const update = () => setWide(query.matches);
    query.addEventListener("change", update);
    return () => query.removeEventListener("change", update);
  }, []);
  return wide;
}

type LayoutProps = Omit<GameDetailProps, "game" | "layout" | "isTopmost"> & {
  game: DetectedGame;
  coverUrl: string | null;
  onCoverBroken: () => void;
  playButtonRef: React.RefObject<HTMLButtonElement>;
};

function CloseButton({ onClose }: { onClose: () => void }) {
  const { t } = useI18n();
  return (
    <button
      id="close-game-detail-button"
      type="button"
      onClick={onClose}
      aria-label={t("detail.close")}
      className="absolute right-3 top-3 z-10 rounded-lg p-1.5 text-zinc-400 transition-colors hover:bg-oled-elevated hover:text-zinc-100"
    >
      <X className="h-4 w-4" />
    </button>
  );
}

/**
 * Separate tiles over the game's artwork. On the left, always within reach: the cover, the
 * actions and where the user stands with the game. On the right, in sight at once: the title,
 * the figures and the achievements; then the cards the user keeps in sight, and the other
 * cards (description, activity, what the plugins add, files) in a box that moves on by itself.
 */
function BentoLayout(props: LayoutProps) {
  const { game, coverUrl, onCoverBroken, isRunning, onClose, onSelectGenre, onSelectTag, onSetStatus, plugins } = props;
  const { t, relativePast, shortDate, number } = useI18n();
  const activity = useActivity(game.id);
  const hasAchievements = (game.achievementsTotal ?? 0) > 0;
  // The actions exist once: in the cover column, or under the title where there is none
  const wide = useWideSheet();
  const panels = usePluginPanels(game, plugins);
  // The cards kept in sight are the same on every game: the choice is about the kind of card
  const [pinned, setPinned] = useState<string[]>(readPinnedCards);
  const keepPinned = (next: string[]) => {
    setPinned(next);
    writePinnedCards(next);
  };

  const cards: DetailCard[] = [
    {
      key: "about",
      title: t("detail.about"),
      content: (
        <p
          className={`select-text text-sm leading-relaxed ${
            game.shortDescription ? "text-zinc-300" : "text-zinc-500"
          }`}
        >
          {game.shortDescription ?? t("detail.noDescription")}
        </p>
      ),
    },
    ...(activity !== null && activity.some((day) => day.minutes > 0)
      ? [{ key: "activity", title: t("detail.activityTitle"), content: <ActivityChart days={activity} /> }]
      : []),
    // A plugin with nothing to say about this game has no card
    ...panels.filter(panelHasContent).map((panel) => ({
      key: `plugin:${panel.plugin.id}`,
      title: panelTitle(panel),
      action: <PluginRefreshButton panel={panel} />,
      content: (
        <div className="space-y-4" data-detail-plugin={panel.plugin.id}>
          <PluginPanelBody panel={panel} titled />
        </div>
      ),
    })),
    { key: "files", title: t("detail.files"), content: <TechnicalNotes game={game} withPlayFacts={false} /> },
  ];
  const scrolling = props.cards === "rail";
  const kept = scrolling
    ? pinned.flatMap((key) => cards.filter((card) => card.key === key))
    : cards;
  const moving = scrolling ? cards.filter((card) => !pinned.includes(card.key)) : [];

  return (
    <div
      role="dialog"
      aria-modal="true"
      aria-labelledby="game-detail-title"
      data-detail-layout="bento"
      className="detail-sheet motion-sheet detail-grid relative flex max-h-full w-full"
    >
      {wide && (
        <div className="detail-side detail-grid scroll-area flex shrink-0 flex-col overflow-y-auto">
          <div className="tile shrink-0 overflow-hidden">
            <CoverArt game={game} coverUrl={coverUrl} onBroken={onCoverBroken} />
          </div>
          <div className="tile shrink-0 p-3">
            <Actions {...props} stacked />
          </div>
          <div className="tile shrink-0 space-y-2.5 p-4">
            <span className="micro-label">{t("detail.status")}</span>
            <StatusPicker game={game} onSetStatus={onSetStatus} />
          </div>
        </div>
      )}

      <div className="detail-grid scroll-area flex min-w-0 flex-1 flex-col overflow-y-auto">
        <div className="tile shrink-0 space-y-3 p-5 pr-12">
          <CloseButton onClose={onClose} />
          <Badges game={game} isRunning={isRunning} emulatorName={props.emulatorName} />
          <h2
            id="game-detail-title"
            className="select-text text-3xl font-semibold leading-tight tracking-tight text-zinc-50"
          >
            {game.title}
          </h2>
          <Genres
            game={game}
            onSelectGenre={onSelectGenre}
            onSelectTag={onSelectTag}
            onSetGenres={props.onSetGenres}
            knownGenres={props.knownGenres}
          />
        </div>

        {/* Narrow windows have no left column: the actions come right after the title */}
        {!wide && (
          <div className="tile shrink-0 space-y-3 p-4">
            <Actions {...props} />
            <StatusPicker game={game} onSetStatus={onSetStatus} />
          </div>
        )}

        <div className="detail-figures detail-grid grid shrink-0">
          <PlaytimeTile game={game} activity={activity} />
          <Stat
            id="detail-last-played"
            label={t("detail.lastPlayed")}
            value={game.lastPlayedAt !== null ? relativePast(game.lastPlayedAt) : t("detail.never")}
            note={game.lastPlayedAt !== null ? shortDate(game.lastPlayedAt) : undefined}
          />
          <Stat
            id="detail-launches"
            label={t("detail.launches")}
            value={number(game.playCount)}
            note={t(game.playCount > 0 ? "detail.launchesFrom" : "detail.launchesNone")}
          />
        </div>

        {hasAchievements && (
          <div className="tile shrink-0 p-4" data-rank="key">
            <AchievementsSection game={game} />
          </div>
        )}

        {kept.map((card) => (
          <PinnedCard
            key={card.key}
            card={card}
            onUnpin={scrolling ? (key) => keepPinned(pinned.filter((entry) => entry !== key)) : undefined}
          />
        ))}

        <CardRail key={game.id} cards={moving} onPin={(key) => keepPinned([...pinned.filter((entry) => entry !== key), key])} />
      </div>
    </div>
  );
}

/** One sheet: the cover on one side, everything else in a column, the actions at the foot. */
function SheetLayout(props: LayoutProps) {
  const { game, coverUrl, onCoverBroken, isRunning, onClose, onSelectGenre, onSelectTag, onSetStatus, plugins } = props;
  const { t } = useI18n();

  return (
    <div
      role="dialog"
      aria-modal="true"
      aria-labelledby="game-detail-title"
      data-detail-layout="sheet"
      className="detail-sheet motion-sheet relative flex max-h-full w-full overflow-hidden rounded-xl border border-oled-border bg-oled-surface shadow-2xl"
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
      <CloseButton onClose={onClose} />

      {/* Translucent: under a cover shorter than the sheet the ambient wash shows through */}
      <div className="relative hidden w-[17rem] shrink-0 self-stretch bg-oled-bg/50 sm:block">
        <CoverArt game={game} coverUrl={coverUrl} onBroken={onCoverBroken} />
      </div>

      {/* Details: everything scrolls except the actions, which stay within reach */}
      <div className="relative flex min-w-0 flex-1 flex-col">
        <div className="scroll-area flex min-h-0 flex-1 flex-col gap-5 overflow-y-auto p-7 pb-5">
          <div className="space-y-3 pr-8">
            <Badges game={game} isRunning={isRunning} emulatorName={props.emulatorName} />
            <h2
              id="game-detail-title"
              className="select-text text-2xl font-semibold leading-tight tracking-tight text-zinc-50"
            >
              {game.title}
            </h2>
            <Genres
            game={game}
            onSelectGenre={onSelectGenre}
            onSelectTag={onSelectTag}
            onSetGenres={props.onSetGenres}
            knownGenres={props.knownGenres}
          />
          </div>

          <StatusPicker game={game} onSetStatus={onSetStatus} />

          <p
            className={`select-text text-sm leading-relaxed ${
              game.shortDescription ? "text-zinc-300" : "text-zinc-500"
            }`}
          >
            {game.shortDescription ?? t("detail.noDescription")}
          </p>

          <AchievementsSection game={game} />
          <GamePluginSections game={game} plugins={plugins} />

          <div className="mt-auto border-t border-oled-border/70 pt-4">
            <TechnicalNotes game={game} withPlayFacts />
          </div>
        </div>

        <div className="shrink-0 border-t border-oled-border/70 px-7 py-4">
          <Actions {...props} />
        </div>
      </div>
    </div>
  );
}

/** How long the sheet takes to leave, as the theme and the settings say; 0 with animations off. */
function leaveMs(): number {
  const root = document.documentElement;
  if (root.hasAttribute("data-reduce-motion")) return 0;
  return parseFloat(getComputedStyle(root).getPropertyValue("--motion-base")) || 0;
}

/**
 * The game whose sheet is on screen: the open one, or, for the moment the sheet takes to
 * leave, the one that has just been closed (`closing`).
 */
function useLeaving(game: DetectedGame | null): { shown: DetectedGame | null; closing: boolean } {
  const [last, setLast] = useState<DetectedGame | null>(game);
  const [closing, setClosing] = useState(false);

  // Before the next paint: a sheet that closes with animations off is never drawn again
  useLayoutEffect(() => {
    if (game) {
      setLast(game);
      setClosing(false);
      return;
    }
    if (!last) return;
    const wait = leaveMs();
    if (wait <= 0) {
      setLast(null);
      return;
    }
    setClosing(true);
    const timer = window.setTimeout(() => {
      setLast(null);
      setClosing(false);
    }, wait);
    return () => window.clearTimeout(timer);
    // `last` is what this effect keeps: only the open game decides when it runs
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [game]);

  return { shown: game ?? last, closing: !game && closing };
}

export function GameDetail({ game: openGame, layout, isTopmost, ...rest }: GameDetailProps) {
  const playButtonRef = useRef<HTMLButtonElement>(null);
  const { usableCover, markBroken } = useBrokenCovers();
  const gameId = openGame?.id ?? null;
  const { onClose } = rest;
  const { shown: game, closing } = useLeaving(openGame);
  const overlayRef = useRef<HTMLDivElement>(null);

  // A sheet on its way out is no longer there for the keyboard or a screen reader
  useEffect(() => {
    if (overlayRef.current) overlayRef.current.inert = closing;
  }, [closing]);

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

  const coverUrl = usableCover(resolveCoverUrl(game.coverPath, game.updatedAt));
  const Layout = layout === "sheet" ? SheetLayout : BentoLayout;

  return (
    <div
      className={`detail-overlay motion-backdrop fixed inset-0 z-40 flex items-center justify-center overflow-hidden px-6 pb-6 ${
        closing ? "motion-closing" : ""
      }`}
      ref={overlayRef}
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      {/* Behind the sheet, bar included: the game's artwork, blurred, under a veil of the
          theme's background */}
      {coverUrl && <img src={coverUrl} alt="" aria-hidden="true" className="detail-art" />}
      <div className="detail-veil" data-no-art={coverUrl ? undefined : ""} />
      <Layout
        {...rest}
        game={game}
        coverUrl={coverUrl}
        onCoverBroken={() => coverUrl && markBroken(coverUrl)}
        playButtonRef={playButtonRef}
      />
    </div>
  );
}
