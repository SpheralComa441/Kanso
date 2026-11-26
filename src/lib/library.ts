import type { DetectedGame, GameStatus, GameSource } from "../types/game";
import { PLATFORM_ORDER, sourceOf } from "./platforms";

/**
 * Which slice of the library is shown. Hidden games appear in `hidden` only; every other
 * collection, count and filter ignores them. Games that are not on disk are part of the
 * library like the others, unless the user asked to see installed games only; `notInstalled`
 * lists them either way.
 */
export type LibraryCollection = "all" | "favorites" | "recent" | "notInstalled" | "hidden";

/**
 * The games lists and counts are drawn from, hidden ones always left out: everything, what
 * is on disk ("installed only" is switched on), or what is not (the `notInstalled` collection).
 */
export type LibraryScope = "all" | "installed" | "absent";

export function libraryScope(
  collection: LibraryCollection,
  installedOnly: boolean
): LibraryScope {
  if (collection === "notInstalled") return "absent";
  return installedOnly ? "installed" : "all";
}

export function isInScope(game: DetectedGame, scope: LibraryScope): boolean {
  if (game.isHidden) return false;
  return scope === "all" || (scope === "installed") === game.isInstalled;
}

/** Quick filters combined with AND; `"all"` / `null` means "no constraint". */
export interface LibraryFilters {
  collection: LibraryCollection;
  platform: GameSource | null;
  /** Normalized genre key, see {@link genreKey}. */
  genre: string | null;
  /** Normalized key of a Steam community tag. */
  tag: string | null;
  status: GameStatus | null;
}

export const EMPTY_FILTERS: LibraryFilters = {
  collection: "all",
  platform: null,
  genre: null,
  tag: null,
  status: null,
};

/** The statuses in the order they are listed everywhere. */
export const GAME_STATUSES: readonly GameStatus[] = [
  "backlog",
  "playing",
  "finished",
  "abandoned",
];

/** "Recently played" means within this many days. */
const RECENT_WINDOW_MS = 30 * 24 * 60 * 60 * 1000;

/**
 * Played in the last 30 days. Steam's own records count, so games played years ago must not
 * crowd the list the way "ever launched" would.
 */
export function isRecentlyPlayed(game: DetectedGame, now = Date.now()): boolean {
  return game.lastPlayedAt !== null && now - game.lastPlayedAt <= RECENT_WINDOW_MS;
}

export interface GenreSummary {
  key: string;
  label: string;
  count: number;
}

export interface LibraryCounts {
  /** Games shown in the library: not hidden and, with "installed only", on disk. */
  visible: number;
  favorites: number;
  recent: number;
  /** In the library but not on disk (hidden ones excluded), whatever "installed only" says. */
  notInstalled: number;
  hidden: number;
  /** Platforms and statuses of the games in the scope being browsed. */
  /** By store, and by plugin library (`plugin:<id>`) for the plugins that have games. */
  byPlatform: Record<string, number>;
  byStatus: Record<GameStatus, number>;
}

export function hasActiveFilters(filters: LibraryFilters): boolean {
  return (
    filters.collection !== "all" ||
    filters.platform !== null ||
    filters.genre !== null ||
    filters.tag !== null ||
    filters.status !== null
  );
}

export function activeFilterCount(filters: LibraryFilters): number {
  return (
    Number(filters.collection !== "all") +
    Number(filters.platform !== null) +
    Number(filters.genre !== null) +
    Number(filters.tag !== null) +
    Number(filters.status !== null)
  );
}

/** Case- and whitespace-insensitive identity of a genre tag ("GDR" === " gdr "). */
export function genreKey(genre: string): string {
  return genre.trim().toLocaleLowerCase("it");
}

/**
 * Whether `game` actually appears among the tiles of `kind`: it has at least one genre, or at
 * least one tag among `listedKeys` (tags that merely repeat a genre are not tiles).
 */
export function hasCategory(
  game: DetectedGame,
  kind: "genres" | "tags",
  listedKeys?: ReadonlySet<string>
): boolean {
  const values = kind === "genres" ? game.genres : game.tags;
  return values.some((raw) => {
    const key = genreKey(raw);
    return key !== "" && (listedKeys === undefined || listedKeys.has(key));
  });
}

/**
 * Genres found in the metadata of the games in `scope`, most common first. A genre typed
 * with different casing on different games is counted once, under its first spelling (or the
 * spelling `knownLabels` already gave it in the library).
 */
export function summarizeGenres(
  games: DetectedGame[],
  scope: LibraryScope,
  knownLabels?: ReadonlyMap<string, string>,
  locale = "it"
): GenreSummary[] {
  const byKey = new Map<string, GenreSummary>();
  for (const game of games) {
    if (!isInScope(game, scope)) continue;
    const seenInGame = new Set<string>();
    for (const raw of game.genres) {
      const key = genreKey(raw);
      if (!key || seenInGame.has(key)) continue;
      seenInGame.add(key);
      const entry = byKey.get(key);
      if (entry) {
        entry.count += 1;
      } else {
        byKey.set(key, { key, label: knownLabels?.get(key) ?? raw.trim(), count: 1 });
      }
    }
  }
  return [...byKey.values()].sort(
    (a, b) => b.count - a.count || a.label.localeCompare(b.label, locale)
  );
}

/**
 * Steam community tags found on the games in `scope`, most common first. Tags that merely
 * repeat a genre already listed ("GDR", "Indie") are left to the genre list.
 */
export function summarizeTags(
  games: DetectedGame[],
  genres: GenreSummary[],
  scope: LibraryScope,
  locale = "it"
): GenreSummary[] {
  const genreKeys = new Set(genres.map((genre) => genre.key));
  const byKey = new Map<string, GenreSummary>();
  for (const game of games) {
    if (!isInScope(game, scope)) continue;
    const seenInGame = new Set<string>();
    for (const raw of game.tags) {
      const key = genreKey(raw);
      if (!key || genreKeys.has(key) || seenInGame.has(key)) continue;
      seenInGame.add(key);
      const entry = byKey.get(key);
      if (entry) {
        entry.count += 1;
      } else {
        byKey.set(key, { key, label: raw.trim(), count: 1 });
      }
    }
  }
  return [...byKey.values()].sort(
    (a, b) => b.count - a.count || a.label.localeCompare(b.label, locale)
  );
}

/**
 * The genres or tags of `summary` counted again on `games` alone (one platform's, already in
 * the scope being browsed). Keys and labels stay those of the whole list, so that a tile opens
 * the same filter as the side panel and a tag that repeats a genre stays left out; what none
 * of `games` has is dropped. Most common first, as the whole list.
 */
export function recountCategories(
  summary: GenreSummary[],
  games: DetectedGame[],
  kind: "genres" | "tags",
  locale = "it"
): GenreSummary[] {
  const counts = new Map<string, number>();
  for (const game of games) {
    const seenInGame = new Set<string>();
    for (const raw of kind === "genres" ? game.genres : game.tags) {
      const key = genreKey(raw);
      if (!key || seenInGame.has(key)) continue;
      seenInGame.add(key);
      counts.set(key, (counts.get(key) ?? 0) + 1);
    }
  }
  return summary
    .flatMap((category) => {
      const count = counts.get(category.key);
      return count === undefined ? [] : [{ ...category, count }];
    })
    .sort((a, b) => b.count - a.count || a.label.localeCompare(b.label, locale));
}

/**
 * `installedOnly` decides what the collections hold; `scope` what platforms and statuses are
 * counted on (it differs from the former while the `notInstalled` collection is open).
 */
export function countLibrary(
  games: DetectedGame[],
  installedOnly: boolean,
  scope: LibraryScope
): LibraryCounts {
  const byPlatform = Object.fromEntries(
    PLATFORM_ORDER.map((p) => [p, 0])
  ) as Record<string, number>;
  const counts: LibraryCounts = {
    visible: 0,
    favorites: 0,
    recent: 0,
    notInstalled: 0,
    hidden: 0,
    byPlatform,
    byStatus: { backlog: 0, playing: 0, finished: 0, abandoned: 0 },
  };
  for (const game of games) {
    if (game.isHidden) {
      counts.hidden += 1;
      continue;
    }
    if (!game.isInstalled) counts.notInstalled += 1;
    if (isInScope(game, scope)) {
      if (game.status) counts.byStatus[game.status] += 1;
      const source = sourceOf(game);
      byPlatform[source] = (byPlatform[source] ?? 0) + 1;
    }
    if (installedOnly && !game.isInstalled) continue;
    counts.visible += 1;
    if (game.isFavorite) counts.favorites += 1;
    if (isRecentlyPlayed(game)) counts.recent += 1;
  }
  return counts;
}

/**
 * The games the current view shows. `installedOnly` is the user's global switch: it leaves
 * out what is not on disk everywhere except in the collection made of exactly those games
 * (and among the hidden ones, a pool of its own).
 */
export function applyLibraryFilters(
  games: DetectedGame[],
  filters: LibraryFilters,
  searchQuery: string,
  installedOnly: boolean
): DetectedGame[] {
  const q = searchQuery.trim().toLowerCase();
  const wantHidden = filters.collection === "hidden";
  const scope = libraryScope(filters.collection, installedOnly);

  const matching = games.filter((g) => {
    if (g.isHidden !== wantHidden) return false;
    if (!wantHidden && !isInScope(g, scope)) return false;
    if (filters.collection === "favorites" && !g.isFavorite) return false;
    if (filters.collection === "recent" && !isRecentlyPlayed(g)) return false;
    if (filters.platform && sourceOf(g) !== filters.platform) return false;
    if (filters.status && g.status !== filters.status) return false;
    if (
      filters.genre &&
      !g.genres.some((genre) => genreKey(genre) === filters.genre)
    ) {
      return false;
    }
    if (filters.tag && !g.tags.some((tag) => genreKey(tag) === filters.tag)) {
      return false;
    }
    if (!q) return true;
    return (
      g.title.toLowerCase().includes(q) ||
      g.cleanTitle.toLowerCase().includes(q) ||
      g.genres.some((genre) => genre.toLowerCase().includes(q)) ||
      g.tags.some((tag) => tag.toLowerCase().includes(q))
    );
  });

  if (filters.collection === "recent") {
    // Most recently played first
    matching.sort((a, b) => (b.lastPlayedAt ?? 0) - (a.lastPlayedAt ?? 0));
  }
  return matching;
}

export function sortGamesByTitle(games: DetectedGame[]): DetectedGame[] {
  return [...games].sort(
    (a, b) =>
      a.title.toLowerCase().localeCompare(b.title.toLowerCase()) ||
      a.id.localeCompare(b.id)
  );
}

/** Reads a small per-device UI preference; never throws (private mode, blocked storage). */
export function readUiPreference<T extends string>(
  key: string,
  allowed: readonly T[],
  fallback: T
): T {
  try {
    const raw = localStorage.getItem(key);
    return allowed.includes(raw as T) ? (raw as T) : fallback;
  } catch {
    return fallback;
  }
}

export function writeUiPreference(key: string, value: string): void {
  try {
    localStorage.setItem(key, value);
  } catch {
    // The preference simply won't be remembered
  }
}
