import type { DetectedGame, GamePlatform } from "../types/game";

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

/**
 * Store names are brand names and stay untranslated; only "local" games get a translated
 * label, through `platformLabel`.
 */
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
  // Not a store: these games live in the Emulators section, not among the platforms
  emulator: {
    label: "Emulatore",
    badgeClass: "border-amber-500/30 bg-amber-500/15 text-amber-300",
  },
};

/** Display name of a platform in the current language. */
export function platformLabel(
  platform: GamePlatform,
  t: (key: "platform.local" | "platform.emulator") => string
): string {
  return platform === "standalone"
    ? t("platform.local")
    : platform === "emulator"
      ? t("platform.emulator")
      : PLATFORM_META[platform].label;
}

/** A game file run by an emulator: it lives in the Emulators section, not in the library. */
export function isEmulated(game: DetectedGame): boolean {
  return game.platform === "emulator";
}

/**
 * Whether Kanso can hand a game that is not on disk to its store's client for installing.
 * Mirrors what the backend can build an address from (`DetectedGame::install_uri`): a game
 * listed by an account may lack what its client knows it by.
 */
export function canInstall(game: DetectedGame): boolean {
  if (game.isInstalled) return false;
  const appId = game.platformAppId?.trim() ?? "";
  const launchUri = game.launchUri ?? "";
  switch (game.platform) {
    case "steam":
    case "gog":
      return /^\d+$/.test(appId);
    case "epic":
      return launchUri.startsWith("com.epicgames.launcher://apps/");
    case "ubisoft":
      return launchUri.startsWith("uplay://");
    case "Battle.net":
      return launchUri.startsWith("battlenet://");
    case "xbox":
      return (
        launchUri.startsWith("msxbox://game/?productId=") ||
        launchUri.startsWith("ms-windows-store://pdp/?PFN=")
      );
    default:
      return false;
  }
}
