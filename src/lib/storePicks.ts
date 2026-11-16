import type { Deal, DetectedGame } from "../types/game";

/** How many genres make up "what the user plays": the ones with the most hours. */
const TOP_GENRES = 5;

const keyOf = (genre: string) => genre.trim().toLowerCase();

/** How much the user plays each genre: the hours of the games that have it. */
function genreHours(games: DetectedGame[]): Map<string, { label: string; weight: number }> {
  const hours = new Map<string, { label: string; weight: number }>();
  for (const game of games) {
    if (game.isHidden) continue;
    // A game with no time on record still counts a little, so that a library that was never
    // timed says something
    const weight = Math.max(game.playtimeMinutes ?? 0, 10);
    for (const genre of game.genres) {
      const key = keyOf(genre);
      if (!key) continue;
      const known = hours.get(key);
      if (known) known.weight += weight;
      else hours.set(key, { label: genre, weight });
    }
  }
  return hours;
}

/** The genres the user plays, by hours: the most played first. */
export function playedGenres(games: DetectedGame[]): string[] {
  return [...genreHours(games).values()].sort((a, b) => b.weight - a.weight).map((entry) => entry.label);
}

export interface StorePicks {
  /** The genres the picks were made by: the user's most played. */
  favourites: string[];
  /** Deals of the genres the user plays the most. */
  alike: Deal[];
  /** Deals of none of those genres: something else for a change, the least familiar first. */
  different: Deal[];
}

/**
 * Suggestions among the deals, by what the user plays: the ones of their most played genres,
 * and, for a change, the ones that have none of them, starting from what is furthest from
 * the user's habits. Only deals whose store says their genres can be suggested; among equals
 * the best discounts come first. All of it is worked out here, from the library: nothing
 * about what the user plays is sent anywhere.
 */
export function storePicks(deals: Deal[], games: DetectedGame[], ownedSteamAppIds: ReadonlySet<string>): StorePicks {
  const hours = genreHours(games);
  const favourites = [...hours.values()]
    .sort((a, b) => b.weight - a.weight)
    .slice(0, TOP_GENRES)
    .map((entry) => entry.label);
  if (favourites.length === 0) return { favourites, alike: [], different: [] };
  const favouriteKeys = new Set(favourites.map(keyOf));
  const total = [...hours.values()].reduce((sum, entry) => sum + entry.weight, 0) || 1;
  /** How much of what the user plays a deal's genres cover: 0 is something never played. */
  const familiarity = (deal: Deal) => deal.genres.reduce((sum, genre) => sum + (hours.get(keyOf(genre))?.weight ?? 0), 0) / total;
  const candidates = deals
    .filter((deal) => !deal.upcoming && deal.genres.length > 0)
    // What is already in the library is not a suggestion
    .filter((deal) => deal.steamAppid === null || !ownedSteamAppIds.has(deal.steamAppid))
    .sort((a, b) => b.discountPercent - a.discountPercent);
  return {
    favourites,
    alike: candidates.filter((deal) => deal.genres.some((genre) => favouriteKeys.has(keyOf(genre)))),
    different: candidates
      .filter((deal) => !deal.genres.some((genre) => favouriteKeys.has(keyOf(genre))))
      .sort((a, b) => familiarity(a) - familiarity(b) || b.discountPercent - a.discountPercent),
  };
}
