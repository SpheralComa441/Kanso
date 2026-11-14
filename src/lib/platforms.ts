import type { GamePlatform } from "../types/game";

/** Display order of the supported game sources across the UI. */
export const PLATFORM_ORDER: GamePlatform[] = [
  "steam",
  "epic",
  "gog",
  "ea",
  "ubisoft",
  "xbox",
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
  gog: {
    label: "GOG",
    badgeClass: "border-fuchsia-500/30 bg-fuchsia-500/15 text-fuchsia-300",
  },
  ea: {
    label: "EA app",
    badgeClass: "border-rose-500/30 bg-rose-500/15 text-rose-300",
  },
  ubisoft: {
    label: "Ubisoft",
    badgeClass: "border-indigo-400/35 bg-indigo-400/15 text-indigo-300",
  },
  xbox: {
    label: "Xbox",
    badgeClass: "border-green-500/30 bg-green-500/15 text-green-300",
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
