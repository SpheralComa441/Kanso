import type { DetectedGame, GamePlatform } from "../types/game";
import { PLATFORM_ORDER } from "./platforms";

/**
 * Which slice of the library is shown. Hidden games appear in `hidden` only; every other
 * collection, count and filter ignores them.
 */
export type LibraryCollection = "all" | "favorites" | "recent" | "hidden";

/** Quick filters combined with AND; `"all"` / `null` means "no constraint". */
export interface LibraryFilters {
  collection: LibraryCollection;
  platform: GamePlatform | null;
  /** Normalized genre key, see {@link genreKey}. */
  genre: string | null;
  /** Normalized key of a Steam community tag. */
  tag: string | null;
}

export const EMPTY_FILTERS: LibraryFilters = {
  collection: "all",
  platform: null,
  genre: null,
  tag: null,
};

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
  /** Games shown in the library (hidden ones excluded). */
  visible: number;
  favorites: number;
  recent: number;
  hidden: number;
  byPlatform: Record<GamePlatform, number>;
}

export function hasActiveFilters(filters: LibraryFilters): boolean {
  return (
    filters.collection !== "all" ||
    filters.platform !== null ||
    filters.genre !== null ||
    filters.tag !== null
  );
}

export function activeFilterCount(filters: LibraryFilters): number {
  return (
    Number(filters.collection !== "all") +
    Number(filters.platform !== null) +
    Number(filters.genre !== null) +
    Number(filters.tag !== null)
  );
}

/** Case- and whitespace-insensitive identity of a genre tag ("GDR" === " gdr "). */
export function genreKey(genre: string): string {
  return genre.trim().toLocaleLowerCase("it");
}

/**
 * Genres found in the visible library's metadata, most common first. A genre typed with
 * different casing on different games is counted once, under its first spelling.
 */
export function summarizeGenres(games: DetectedGame[]): GenreSummary[] {
  const byKey = new Map<string, GenreSummary>();
  for (const game of games) {
    if (game.isHidden) continue;
    const seenInGame = new Set<string>();
    for (const raw of game.genres) {
      const key = genreKey(raw);
      if (!key || seenInGame.has(key)) continue;
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
    (a, b) => b.count - a.count || a.label.localeCompare(b.label, "it")
  );
}

/**
 * Steam community tags found in the visible library, most common first. Tags that merely
 * repeat a genre already listed ("GDR", "Indie") are left to the genre list.
 */
export function summarizeTags(
  games: DetectedGame[],
  genres: GenreSummary[]
): GenreSummary[] {
  const genreKeys = new Set(genres.map((genre) => genre.key));
  const byKey = new Map<string, GenreSummary>();
  for (const game of games) {
    if (game.isHidden) continue;
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
    (a, b) => b.count - a.count || a.label.localeCompare(b.label, "it")
  );
}

export function countLibrary(games: DetectedGame[]): LibraryCounts {
  const byPlatform = Object.fromEntries(
    PLATFORM_ORDER.map((p) => [p, 0])
  ) as Record<GamePlatform, number>;
  const counts: LibraryCounts = {
    visible: 0,
    favorites: 0,
    recent: 0,
    hidden: 0,
    byPlatform,
  };
  for (const game of games) {
    if (game.isHidden) {
      counts.hidden += 1;
      continue;
    }
    counts.visible += 1;
    if (game.isFavorite) counts.favorites += 1;
    if (isRecentlyPlayed(game)) counts.recent += 1;
    byPlatform[game.platform] = (byPlatform[game.platform] ?? 0) + 1;
  }
  return counts;
}

export function applyLibraryFilters(
  games: DetectedGame[],
  filters: LibraryFilters,
  searchQuery: string
): DetectedGame[] {
  const q = searchQuery.trim().toLowerCase();
  const wantHidden = filters.collection === "hidden";

  const matching = games.filter((g) => {
    if (g.isHidden !== wantHidden) return false;
    if (filters.collection === "favorites" && !g.isFavorite) return false;
    if (filters.collection === "recent" && !isRecentlyPlayed(g)) return false;
    if (filters.platform && g.platform !== filters.platform) return false;
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
