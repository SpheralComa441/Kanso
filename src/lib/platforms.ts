import type { GamePlatform } from "../types/game";

/** Display order of the supported game sources across the UI. */
export const PLATFORM_ORDER: GamePlatform[] = [
  "steam",
  "epic",
  "Battle.net",
  "standalone",
];

export const PLATFORM_META: Record<
  GamePlatform,
  { label: string; badgeClass: string }
> = {
  steam: {
    label: "Steam",
    badgeClass: "border-sky-500/30 bg-sky-500/15 text-sky-300",
  },
  epic: {
    label: "Epic Games",
    badgeClass: "border-zinc-500/30 bg-zinc-500/20 text-zinc-200",
  },
  "Battle.net": {
    label: "Battle.net",
    badgeClass: "border-blue-500/35 bg-blue-500/15 text-blue-300",
  },
  standalone: {
    label: "Locale",
    badgeClass: "border-emerald-500/30 bg-emerald-500/15 text-emerald-300",
  },
};
