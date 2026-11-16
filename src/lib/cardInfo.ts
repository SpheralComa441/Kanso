import type { CardInfoItem } from "../types/game";

/**
 * What the cover of a game can tell under the pointer, besides its title, in the order it is
 * shown. The same closed list the backend keeps (`CARD_INFO_CHOICES` in `config.rs`).
 */
export const CARD_INFO_CHOICES: readonly CardInfoItem[] = [
  "genres",
  "tags",
  "playtime",
  "lastPlayed",
  "status",
  "achievements",
  "badges",
];

/** What it tells until the user chooses. */
export const CARD_INFO_DEFAULT: readonly CardInfoItem[] = ["genres", "playtime", "badges"];

/** A stored choice read back: known names only, once each, in Kanso's order. */
export function withCardInfo(raw: unknown): CardInfoItem[] {
  if (!Array.isArray(raw)) return [...CARD_INFO_DEFAULT];
  return CARD_INFO_CHOICES.filter((choice) => raw.includes(choice));
}
