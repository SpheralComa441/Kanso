import { useEffect, useMemo, useState, type ReactNode } from "react";
import { useI18n } from "../i18n";
import { getPlaySessions } from "../lib/backend";
import { GAME_STATUSES } from "../lib/library";
import { platformLabel } from "../lib/platforms";
import {
  historyStart,
  niceCeiling,
  secondsWithin,
  summarizeLibrary,
  weeklyBuckets,
  type WeekBucket,
} from "../lib/stats";
import type { DetectedGame, PlaySession } from "../types/game";
import { STATUS_LABEL_KEYS, StatusIcon } from "./StatusIcon";

const WEEKS = 12;
const DAY_MS = 24 * 60 * 60 * 1000;

interface StatsViewProps {
  games: DetectedGame[];
  onOpenGame: (game: DetectedGame) => void;
  onError: (message: string) => void;
}

function Card({ title, hint, children }: { title: string; hint?: string; children: ReactNode }) {
  return (
    <section className="rounded-xl border border-oled-border bg-oled-surface/50 p-5">
      <h2 className="text-sm font-semibold text-zinc-100">{title}</h2>
      {hint && <p className="mt-0.5 text-2xs leading-relaxed text-zinc-500">{hint}</p>}
      <div className="mt-4">{children}</div>
    </section>
  );
}

function StatTile({ id, label, value, detail }: { id: string; label: string; value: string; detail?: string }) {
  return (
    <div id={id} className="rounded-xl border border-oled-border bg-oled-surface/50 px-4 py-3.5">
      <p className="text-2xs text-zinc-500">{label}</p>
      <p className="mt-1 text-2xl font-semibold leading-none text-zinc-50">{value}</p>
      <p className="mt-1.5 min-h-[1em] text-2xs text-zinc-500">{detail}</p>
    </div>
  );
}

interface BarRow {
  key: string;
  label: string;
  value: number;
  valueLabel: string;
  icon?: ReactNode;
  onClick?: () => void;
}

/**
 * Ranked horizontal bars: one hue (magnitude, not identity), every row named and valued in
 * text, bars growing from a shared baseline with only the data end rounded.
 */
function BarList({ rows, total, unitLabel }: { rows: BarRow[]; total: number; unitLabel: string }) {
  const { t } = useI18n();
  const max = Math.max(...rows.map((row) => row.value), 0);
  return (
    <table
      aria-label={unitLabel}
      className="-my-1.5 w-full border-separate border-spacing-y-1.5 text-xs"
    >
      <tbody>
        {rows.map((row) => {
          const share = total > 0 ? Math.round((row.value / total) * 100) : 0;
          const name = (
            <span className="flex min-w-0 items-center gap-1.5">
              {row.icon}
              <span className="truncate">{row.label}</span>
            </span>
          );
          return (
            <tr
              key={row.key}
              title={`${row.label}: ${row.valueLabel} · ${t("stats.share", { percent: share })}`}
              className="group"
            >
              <th scope="row" className="w-[38%] max-w-0 pr-3 text-left font-normal text-zinc-300">
                {row.onClick ? (
                  <button
                    type="button"
                    onClick={row.onClick}
                    className="block w-full truncate text-left underline-offset-2 hover:text-zinc-100 hover:underline"
                  >
                    {name}
                  </button>
                ) : (
                  name
                )}
              </th>
              <td className="border-l border-oled-border">
                <div
                  className="h-2.5 min-w-[2px] rounded-r-[4px] bg-accent transition-colors group-hover:bg-accent-hover"
                  style={{ width: max > 0 ? `${(row.value / max) * 100}%` : 0 }}
                />
              </td>
              <td className="w-px whitespace-nowrap pl-3 text-right text-2xs tabular-nums text-zinc-400">
                {row.valueLabel}
              </td>
            </tr>
          );
        })}
      </tbody>
    </table>
  );
}

/** Hours per week as columns, with a tooltip on hover and keyboard focus and a table for screen readers. */
function WeeklyChart({ buckets }: { buckets: WeekBucket[] }) {
  const { t, shortDate, duration, number } = useI18n();
  const [active, setActive] = useState<number | null>(null);
  const maxHours = Math.max(...buckets.map((bucket) => bucket.seconds / 3600), 0);
  const top = niceCeiling(maxHours);
  const ticks = [top, top / 2, 0];
  const describe = (bucket: WeekBucket) => duration(Math.round(bucket.seconds / 60));
  const activeBucket = active === null ? null : buckets[active];

  return (
    <div id="stats-weekly-chart">
      <div className="flex">
        {/* Scale */}
        <div className="flex h-40 w-10 shrink-0 flex-col justify-between pr-2 text-right text-3xs tabular-nums text-zinc-500">
          {ticks.map((tick) => (
            <span key={tick} className="-translate-y-1/2 first:translate-y-0 last:translate-y-1/2">
              {t("time.hours", { count: number(tick, 1) })}
            </span>
          ))}
        </div>

        <div className="relative h-40 min-w-0 flex-1">
          {ticks.map((tick, index) => (
            <div
              key={tick}
              className="absolute inset-x-0 border-t border-oled-border/70"
              style={{ top: `${(index / (ticks.length - 1)) * 100}%` }}
            />
          ))}

          <div className="absolute inset-0 flex">
            {buckets.map((bucket, index) => {
              const hours = bucket.seconds / 3600;
              return (
                // The whole slot is the target, not just the painted column
                <div
                  key={bucket.start}
                  tabIndex={0}
                  role="img"
                  aria-label={`${t("stats.weekOf", { date: shortDate(bucket.start) })}: ${describe(bucket)}`}
                  onPointerEnter={() => setActive(index)}
                  onPointerLeave={() => setActive((current) => (current === index ? null : current))}
                  onFocus={() => setActive(index)}
                  onBlur={() => setActive((current) => (current === index ? null : current))}
                  className="flex h-full min-w-0 flex-1 items-end justify-center px-px outline-none focus-visible:bg-oled-elevated/60"
                >
                  <div
                    className={`w-full max-w-[24px] rounded-t-[4px] transition-colors ${
                      active === index ? "bg-accent-hover" : "bg-accent"
                    }`}
                    style={{ height: `${(hours / top) * 100}%`, minHeight: hours > 0 ? 2 : 0 }}
                  />
                </div>
              );
            })}
          </div>

          {activeBucket && active !== null && (
            <div
              role="status"
              className="pointer-events-none absolute -top-2 z-10 -translate-x-1/2 -translate-y-full whitespace-nowrap rounded-lg border border-oled-border bg-oled-elevated px-2.5 py-1.5 text-center shadow-xl"
              style={{ left: `${((active + 0.5) / buckets.length) * 100}%` }}
            >
              <p className="text-xs font-semibold text-zinc-50">{describe(activeBucket)}</p>
              <p className="text-3xs text-zinc-400">
                {t("stats.weekOf", { date: shortDate(activeBucket.start) })}
              </p>
            </div>
          )}
        </div>
      </div>

      {/* Week labels: every third one, ending on the current week */}
      <div className="ml-10 mt-1.5 flex text-3xs text-zinc-500" aria-hidden="true">
        {buckets.map((bucket, index) => (
          <span key={bucket.start} className="min-w-0 flex-1 text-center">
            {(buckets.length - 1 - index) % 3 === 0 ? shortDate(bucket.start) : ""}
          </span>
        ))}
      </div>

      <table className="sr-only">
        <caption>{t("stats.activity")}</caption>
        <thead>
          <tr>
            <th scope="col">{t("stats.week")}</th>
            <th scope="col">{t("stats.hours")}</th>
          </tr>
        </thead>
        <tbody>
          {buckets.map((bucket) => (
            <tr key={bucket.start}>
              <th scope="row">{shortDate(bucket.start)}</th>
              <td>{describe(bucket)}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

/** Numbers about the library: how much was played, when, of what. */
export function StatsView({ games, onOpenGame, onError }: StatsViewProps) {
  const { t, plural, duration, number } = useI18n();
  const [sessions, setSessions] = useState<PlaySession[]>([]);
  const [now] = useState(() => Date.now());

  useEffect(() => {
    let alive = true;
    // Enough history for the weekly chart, which reaches further back than the 30-day tile
    getPlaySessions(Math.min(historyStart(WEEKS, now), now - 31 * DAY_MS))
      .then((loaded) => alive && setSessions(loaded))
      .catch((error) => onError(typeof error === "string" ? error : t("stats.loadError")));
    return () => {
      alive = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [now]);

  const stats = useMemo(() => summarizeLibrary(games), [games]);
  const buckets = useMemo(() => weeklyBuckets(sessions, WEEKS, now), [sessions, now]);
  const last30Minutes = useMemo(
    () => Math.round(secondsWithin(sessions, now - 30 * DAY_MS, now + DAY_MS) / 60),
    [sessions, now]
  );
  const hasHistory = buckets.some((bucket) => bucket.seconds > 0);

  /** Long times in whole hours, short ones with their minutes. */
  const compact = (minutes: number) =>
    minutes >= 600 ? t("time.hours", { count: number(Math.round(minutes / 60)) }) : duration(minutes);

  const statusRows: BarRow[] = [
    ...GAME_STATUSES.map((status) => ({
      key: status,
      label: t(STATUS_LABEL_KEYS[status]),
      value: stats.byStatus[status],
      valueLabel: plural("games", stats.byStatus[status]),
      icon: <StatusIcon status={status} className="h-3 w-3 shrink-0 text-zinc-500" />,
    })),
    {
      key: "none",
      label: t("stats.noStatus"),
      value: stats.withoutStatus,
      valueLabel: plural("games", stats.withoutStatus),
    },
  ];

  return (
    <div id="stats-view" className="space-y-4">
      <header>
        <h1 className="text-lg font-semibold tracking-tight text-zinc-50">{t("stats.title")}</h1>
        <p className="mt-0.5 text-xs text-zinc-500">{t("stats.subtitle")}</p>
      </header>

      <div className="grid grid-cols-2 gap-3 lg:grid-cols-4">
        <StatTile
          id="stat-total-hours"
          label={t("stats.totalHours")}
          value={t("time.hours", { count: number(Math.round(stats.totalMinutes / 60)) })}
        />
        <StatTile id="stat-last-30" label={t("stats.last30")} value={duration(last30Minutes)} />
        <StatTile
          id="stat-games-played"
          label={t("stats.gamesPlayed")}
          value={number(stats.played)}
          detail={t("stats.ofTotal", { total: number(stats.games) })}
        />
        <StatTile
          id="stat-achievements"
          label={t("stats.achievements")}
          value={number(stats.achievementsUnlocked)}
          detail={
            stats.achievementsTotal > 0
              ? `${t("stats.ofTotal", { total: number(stats.achievementsTotal) })} · ${plural(
                  "stats.perfect",
                  stats.perfectGames
                )}`
              : undefined
          }
        />
      </div>

      {stats.totalMinutes === 0 && !hasHistory ? (
        <p className="rounded-xl border border-oled-border bg-oled-surface/40 px-6 py-10 text-center text-sm leading-relaxed text-zinc-400">
          {t("stats.empty")}
        </p>
      ) : (
        <>
          <Card title={t("stats.activity")} hint={t("stats.activityHint")}>
            {hasHistory ? (
              <WeeklyChart buckets={buckets} />
            ) : (
              <p className="py-6 text-center text-xs text-zinc-500">{t("stats.noHistory")}</p>
            )}
          </Card>

          <div className="grid items-start gap-4 xl:grid-cols-2">
            {stats.mostPlayed.length > 0 && (
              <Card title={t("stats.mostPlayed")}>
                <BarList
                  unitLabel={t("stats.hours")}
                  total={stats.totalMinutes}
                  rows={stats.mostPlayed.map(({ game, minutes }) => ({
                    key: game.id,
                    label: game.title,
                    value: minutes,
                    valueLabel: compact(minutes),
                    onClick: () => onOpenGame(game),
                  }))}
                />
              </Card>
            )}

            <div className="space-y-4">
              {stats.byPlatform.length > 0 && (
                <Card title={t("stats.byPlatform")}>
                  <BarList
                    unitLabel={t("stats.hours")}
                    total={stats.totalMinutes}
                    rows={stats.byPlatform.map(({ platform, minutes }) => ({
                      key: platform,
                      label: platformLabel(platform, t),
                      value: minutes,
                      valueLabel: compact(minutes),
                    }))}
                  />
                </Card>
              )}
              <Card title={t("stats.byStatus")}>
                <BarList unitLabel={t("stats.byStatus")} total={stats.games} rows={statusRows} />
              </Card>
            </div>

            {stats.byGenre.length > 0 && (
              <Card title={t("stats.byGenre")} hint={t("stats.byGenreHint")}>
                <BarList
                  unitLabel={t("stats.hours")}
                  total={stats.totalMinutes}
                  rows={stats.byGenre.map(({ key, label, minutes }) => ({
                    key,
                    label,
                    value: minutes,
                    valueLabel: compact(minutes),
                  }))}
                />
              </Card>
            )}
          </div>
        </>
      )}
    </div>
  );
}
