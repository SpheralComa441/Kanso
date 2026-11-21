import type { DetectedGame, GamePlatform, GameSource } from "../types/game";

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
  // The library of a plugin: its games go by the library's own name (see `platformLabel`)
  plugin: {
    label: "Plugin",
    badgeClass: "border-violet-400/35 bg-violet-400/15 text-violet-300",
  },
};

const PLUGIN_SOURCE = "plugin:";

/** Where a game is filed among the platforms: its store, or the library of its plugin. */
export function sourceOf(game: DetectedGame): GameSource {
  return game.platform === "plugin" && game.platformAppId
    ? `${PLUGIN_SOURCE}${game.platformAppId}`
    : game.platform;
}

/** The plugin whose library a source is, or `null` for a store. */
export function pluginOfSource(source: GameSource): string | null {
  return source.startsWith(PLUGIN_SOURCE) ? source.slice(PLUGIN_SOURCE.length) : null;
}

/** The platform a source belongs to: itself, or `plugin` for any plugin's library. */
export function platformOfSource(source: GameSource): GamePlatform {
  return pluginOfSource(source) === null ? (source as GamePlatform) : "plugin";
}

/**
 * What the libraries of the plugins are called, by plugin id (a map, not a plain object: an
 * id is any word, and a plain object answers to a few words of its own). Kept here, not
 * passed around:
 * the name of a platform is asked for in every corner of the interface. The app sets it
 * while it renders, whenever the list of plugins changes, before anything below it draws.
 */
let pluginLibraryNames: ReadonlyMap<string, string> = new Map();

export function setPluginLibraryNames(names: ReadonlyMap<string, string>): void {
  pluginLibraryNames = names;
}

/**
 * The platforms offered as filters, in the order of the side panel: the stores, then the
 * libraries the plugins brought, by name. Those with games in `byPlatform`, and the one
 * already `chosen` even without any, so that it can be switched off.
 */
export function offeredPlatforms(
  byPlatform: Record<string, number>,
  chosen: GameSource | null,
  t: (key: "platform.local" | "platform.emulator" | "platform.plugin") => string
): GameSource[] {
  const pluginLibraries = Object.keys(byPlatform)
    .filter((source): source is GameSource => pluginOfSource(source as GameSource) !== null)
    .concat(chosen !== null && pluginOfSource(chosen) !== null ? [chosen] : [])
    .filter((source, index, all) => all.indexOf(source) === index)
    .sort((a, b) => platformLabel(a, t).localeCompare(platformLabel(b, t)));
  return [...PLATFORM_ORDER, ...pluginLibraries].filter(
    (platform) => (byPlatform[platform] ?? 0) > 0 || chosen === platform
  );
}

/** Display name of a platform, or of a plugin's library, in the current language. */
export function platformLabel(
  source: GameSource,
  t: (key: "platform.local" | "platform.emulator" | "platform.plugin") => string
): string {
  const plugin = pluginOfSource(source);
  // A library whose plugin is gone (its games stay if the user gave them something) goes by
  // the plugin's id: better than a name that says nothing
  if (plugin !== null) return pluginLibraryNames.get(plugin) ?? plugin;
  return source === "standalone"
    ? t("platform.local")
    : source === "emulator"
      ? t("platform.emulator")
      : source === "plugin"
        ? t("platform.plugin")
        : PLATFORM_META[source as GamePlatform].label;
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
/**
 * How an installed game can be uninstalled from Kanso: `uninstall` when its client takes the
 * request (and asks for confirmation itself), `client` when all Kanso can do is open the
 * game in its client, `null` when there is no client to ask (a loose program, a game file).
 * Mirrors `DetectedGame::uninstall_uri` of the backend. Kanso never deletes a game itself.
 */
export function uninstallMode(game: DetectedGame): "uninstall" | "client" | null {
  if (!game.isInstalled) return null;
  const appId = game.platformAppId?.trim() ?? "";
  switch (game.platform) {
    case "steam":
    case "ubisoft":
      return /^\d+$/.test(appId) ? "uninstall" : null;
    case "gog":
      return /^\d+$/.test(appId) ? "client" : null;
    case "epic":
    case "ea":
    case "Battle.net":
    case "xbox":
      return "client";
    default:
      return null;
  }
}

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
    // The one address its plugin gave: where the game is got is where it is started
    case "plugin":
      return launchUri.trim() !== "";
    default:
      return false;
  }
}
