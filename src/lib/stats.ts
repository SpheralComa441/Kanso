import type { DetectedGame, GameSource, GameStatus, PlaySession } from "../types/game";
import { GAME_STATUSES, genreKey } from "./library";
import { PLATFORM_ORDER, pluginOfSource, sourceOf } from "./platforms";

/** Local midnight of the Monday that starts the week containing `timestampMs`. */
export function weekStart(timestampMs: number): number {
  const date = new Date(timestampMs);
  date.setHours(0, 0, 0, 0);
  date.setDate(date.getDate() - ((date.getDay() + 6) % 7));
  return date.getTime();
}

function addDays(timestampMs: number, days: number): number {
  const date = new Date(timestampMs);
  // Through the calendar, so a daylight-saving change does not shift the boundary
  date.setDate(date.getDate() + days);
  return date.getTime();
}

/** Seconds of the sessions that fall between `from` and `to`; sessions are cut at the edges. */
export function secondsWithin(sessions: PlaySession[], from: number, to: number): number {
  let total = 0;
  for (const session of sessions) {
    const end = session.startedAt + session.seconds * 1000;
    const overlap = Math.min(end, to) - Math.max(session.startedAt, from);
    if (overlap > 0) total += overlap / 1000;
  }
  return total;
}

export interface WeekBucket {
  /** Monday 00:00 local time. */
  start: number;
  seconds: number;
}

/** Play time of each of the last `weeks` weeks, oldest first; the last one is the current week. */
export function weeklyBuckets(sessions: PlaySession[], weeks: number, now: number): WeekBucket[] {
  const current = weekStart(now);
  return Array.from({ length: weeks }, (_, index) => {
    const start = addDays(current, -7 * (weeks - 1 - index));
    return { start, seconds: secondsWithin(sessions, start, addDays(start, 7)) };
  });
}

/** First week boundary the statistics need sessions from. */
export function historyStart(weeks: number, now: number): number {
  return addDays(weekStart(now), -7 * (weeks - 1));
}

export interface LibraryStats {
  /** Games counted: the visible library. */
  games: number;
  played: number;
  totalMinutes: number;
  achievementsUnlocked: number;
  achievementsTotal: number;
  /** Games with every achievement unlocked. */
  perfectGames: number;
  mostPlayed: { game: DetectedGame; minutes: number }[];
  /** By store, and by plugin library, most played first. */
  byPlatform: { platform: GameSource; minutes: number }[];
  /** A game counts towards each of its genres. */
  byGenre: { key: string; label: string; minutes: number }[];
  byStatus: Record<GameStatus, number>;
  withoutStatus: number;
}

/** Totals and rankings over the visible library (hidden entries are soundtracks, tools...). */
export function summarizeLibrary(games: DetectedGame[], top = 8): LibraryStats {
  const visible = games.filter((game) => !game.isHidden);
  const platformMinutes = new Map<GameSource, number>();
  const genreMinutes = new Map<string, { key: string; label: string; minutes: number }>();
  const byStatus = Object.fromEntries(GAME_STATUSES.map((status) => [status, 0])) as Record<
    GameStatus,
    number
  >;

  const stats: LibraryStats = {
    games: visible.length,
    played: 0,
    totalMinutes: 0,
    achievementsUnlocked: 0,
    achievementsTotal: 0,
    perfectGames: 0,
    mostPlayed: [],
    byPlatform: [],
    byGenre: [],
    byStatus,
    withoutStatus: 0,
  };

  for (const game of visible) {
    const minutes = game.playtimeMinutes ?? 0;
    stats.totalMinutes += minutes;
    if (minutes > 0 || game.lastPlayedAt !== null) stats.played += 1;
    if (game.status) byStatus[game.status] += 1;
    else stats.withoutStatus += 1;

    if (game.achievementsTotal) {
      const unlocked = Math.min(game.achievementsUnlocked ?? 0, game.achievementsTotal);
      stats.achievementsTotal += game.achievementsTotal;
      stats.achievementsUnlocked += unlocked;
      if (unlocked === game.achievementsTotal) stats.perfectGames += 1;
    }

    if (minutes <= 0) continue;
    const source = sourceOf(game);
    platformMinutes.set(source, (platformMinutes.get(source) ?? 0) + minutes);
    const seen = new Set<string>();
    for (const genre of game.genres) {
      const key = genreKey(genre);
      if (!key || seen.has(key)) continue;
      seen.add(key);
      const entry = genreMinutes.get(key);
      if (entry) entry.minutes += minutes;
      else genreMinutes.set(key, { key, label: genre.trim(), minutes });
    }
  }

  stats.mostPlayed = visible
    .filter((game) => (game.playtimeMinutes ?? 0) > 0)
    .map((game) => ({ game, minutes: game.playtimeMinutes as number }))
    .sort((a, b) => b.minutes - a.minutes || a.game.title.localeCompare(b.game.title))
    .slice(0, top);
  // The stores in their usual order, then the plugins' libraries; the sort keeps that order
  // among equals
  const sources: GameSource[] = [
    ...PLATFORM_ORDER.filter((platform) => platformMinutes.has(platform)),
    ...[...platformMinutes.keys()].filter((source) => pluginOfSource(source) !== null).sort(),
  ];
  stats.byPlatform = sources
    .map((platform) => ({ platform, minutes: platformMinutes.get(platform) as number }))
    .sort((a, b) => b.minutes - a.minutes);
  stats.byGenre = [...genreMinutes.values()]
    .sort((a, b) => b.minutes - a.minutes || a.label.localeCompare(b.label))
    .slice(0, top);
  return stats;
}

/** The smallest "round" number of hours that is not below `hours`: the top of the scale. */
export function niceCeiling(hours: number): number {
  if (hours <= 1) return 1;
  const magnitude = 10 ** Math.floor(Math.log10(hours));
  for (const step of [1, 2, 4, 5, 10]) {
    if (step * magnitude >= hours) return step * magnitude;
  }
  return 10 * magnitude;
}
