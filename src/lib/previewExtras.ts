import type {
  DetectedGame,
  GameAchievements,
  GameDataAnswer,
  PlaySession,
  PluginGameData,
  PluginInfo,
  PluginState,
  PluginView,
} from "../types/game";

/**
 * Stand-ins for what only the desktop app can do (plugins, play history, achievements), so
 * the browser preview shows every screen populated. The desktop app never uses this file.
 */

const DAY = 24 * 60 * 60 * 1000;

type PluginSeed = Omit<PluginInfo, "enabled" | "showBadge" | "bundled"> & {
  enabledByDefault: boolean;
};

const PLUGINS: PluginSeed[] = [
  {
    id: "howlongtobeat",
    name: "HowLongToBeat",
    version: "1.0.0",
    author: "Kanso",
    description:
      "Quanto dura un gioco: storia principale, storia + extra, completista. Dati di howlongtobeat.com.",
    homepage: "https://howlongtobeat.com",
    hosts: ["howlongtobeat.com"],
    gameDetails: true,
    badge: true,
    background: true,
    commands: [],
    settings: [],
    enabledByDefault: false,
  },
  {
    id: "steam-reviews",
    name: "Recensioni",
    version: "1.0.0",
    author: "Kanso",
    description: "Recensioni degli utenti di Steam e voto Metacritic.",
    homepage: "https://store.steampowered.com",
    hosts: ["api.steampowered.com", "store.steampowered.com"],
    gameDetails: true,
    badge: true,
    background: true,
    commands: [],
    settings: [],
    enabledByDefault: true,
  },
  {
    id: "steam-news",
    name: "Novità e note delle patch",
    version: "1.0.0",
    author: "Kanso",
    description: "Gli ultimi annunci pubblicati dagli sviluppatori su Steam.",
    homepage: "https://store.steampowered.com",
    hosts: ["api.steampowered.com"],
    gameDetails: true,
    badge: false,
    background: false,
    commands: [],
    settings: [
      {
        key: "count",
        kind: "select",
        label: "Quante notizie mostrare",
        help: "",
        default: "4",
        options: ["3", "4", "6", "8"].map((value) => ({ value, label: value })),
      },
    ],
    enabledByDefault: true,
  },
  {
    id: "pcgamingwiki",
    name: "PCGamingWiki",
    version: "1.0.0",
    author: "Kanso",
    description: "Dati tecnici da PCGamingWiki e link alla pagina del gioco.",
    homepage: "https://www.pcgamingwiki.com",
    hosts: ["www.pcgamingwiki.com"],
    gameDetails: true,
    badge: false,
    background: false,
    commands: [],
    settings: [],
    enabledByDefault: false,
  },
  {
    id: "price-history",
    name: "Storico dei prezzi",
    version: "1.0.0",
    author: "Kanso",
    description:
      "Miglior prezzo attuale e minimo storico, da IsThereAnyDeal. Richiede la tua chiave API gratuita.",
    homepage: "https://isthereanydeal.com",
    hosts: ["api.isthereanydeal.com"],
    gameDetails: true,
    badge: false,
    background: false,
    commands: [],
    settings: [
      {
        key: "apiKey",
        kind: "secret",
        label: "Chiave API di IsThereAnyDeal",
        help: "Gratuita: accedi a isthereanydeal.com, apri “My apps”, registra un'app e copia la sua chiave API.",
        default: "",
        options: [],
      },
      {
        key: "country",
        kind: "text",
        label: "Paese dei prezzi (due lettere, es. IT, US, ES)",
        help: "",
        default: "IT",
        options: [],
      },
    ],
    enabledByDefault: false,
  },
  {
    id: "nexus-mods",
    name: "Mod su Nexus Mods",
    version: "1.1.0",
    author: "Kanso",
    description:
      "Quante mod ha un gioco su Nexus Mods, con il link alla sua pagina delle mod nel browser. Nessun account. Scarica l'elenco pubblico dei giochi di Nexus Mods e vi cerca il gioco su questo PC, quindi nulla della tua libreria viene inviato; solo quando quell'elenco non è disponibile chiede a Nexus Mods del solo gioco di cui è aperta la scheda, per titolo.",
    homepage: "https://www.nexusmods.com",
    hosts: ["data.nexusmods.com", "api.nexusmods.com"],
    gameDetails: true,
    badge: false,
    background: false,
    commands: [],
    settings: [],
    enabledByDefault: false,
  },
  {
    id: "play-tonight",
    name: "Cosa gioco stasera?",
    version: "1.0.0",
    author: "Kanso",
    description: "Propone tre giochi dalla tua libreria. Funziona offline.",
    homepage: null,
    hosts: [],
    gameDetails: false,
    badge: false,
    background: false,
    commands: [
      {
        id: "pick",
        title: "Cosa gioco stasera?",
        description: "Tre proposte dalla tua libreria",
      },
    ],
    settings: [
      {
        key: "includeFinished",
        kind: "toggle",
        label: "Proponi anche i giochi segnati come finiti",
        help: "",
        default: "false",
        options: [],
      },
    ],
    enabledByDefault: true,
  },
];

export function previewPlugins(states: Record<string, PluginState>): PluginInfo[] {
  return PLUGINS.map(({ enabledByDefault, ...plugin }) => ({
    ...plugin,
    bundled: true,
    enabled: states[plugin.id]?.enabled ?? enabledByDefault,
    showBadge: plugin.badge && (states[plugin.id]?.showBadge ?? true),
  }));
}

/** A stable pseudo-random number in [0, 1) from a text, so samples do not change on reload. */
function hash(text: string): number {
  let value = 2166136261;
  for (let i = 0; i < text.length; i += 1) {
    value = Math.imul(value ^ text.charCodeAt(i), 16777619);
  }
  return ((value >>> 0) % 10000) / 10000;
}

function sampleData(pluginId: string, game: DetectedGame): PluginGameData {
  const roll = hash(pluginId + game.id);
  if (pluginId === "howlongtobeat") {
    const main = Math.round(6 + roll * 50);
    return {
      badge: { text: `${main} h`, label: "Per finirlo", tooltip: "HowLongToBeat · Storia principale" },
      sections: [
        {
          title: "HowLongToBeat",
          blocks: [
            {
              type: "stats",
              items: [
                { label: "Storia principale", value: `${main} h` },
                { label: "Storia + extra", value: `${Math.round(main * 1.7)} h` },
                { label: "Completista", value: `${Math.round(main * 2.9)} h` },
              ],
            },
            {
              type: "links",
              items: [{ label: "Apri su HowLongToBeat", url: "https://howlongtobeat.com/game/1" }],
            },
          ],
        },
      ],
    };
  }
  if (pluginId === "steam-reviews") {
    if (game.platform !== "steam" && !game.steamStoreAppid) return { sections: [] };
    const percent = Math.round(72 + roll * 26);
    return {
      badge: { text: `${percent}%`, label: "Gradimento Steam", tooltip: "Recensioni positive su Steam: Molto positive" },
      sections: [
        {
          title: "Recensioni",
          blocks: [
            {
              type: "meter",
              label: "Utenti di Steam · Molto positive",
              value: percent,
              max: 100,
              valueLabel: `${percent}% positive · 41.208 recensioni`,
            },
            { type: "stats", items: [{ label: "Metacritic", value: `${percent - 4} / 100` }] },
            {
              type: "links",
              items: [{ label: "Recensioni su Steam", url: "https://store.steampowered.com/app/1" }],
            },
          ],
        },
      ],
    };
  }
  if (pluginId === "steam-news") {
    if (game.platform !== "steam") return { sections: [] };
    return {
      sections: [
        {
          title: "Novità",
          blocks: [
            {
              type: "list",
              items: [
                {
                  title: `${game.title}: aggiornamento 1.4`,
                  subtitle: "5 nov 2026 · Note della patch — Correzioni di stabilità e nuove opzioni grafiche.",
                  url: "https://store.steampowered.com/news/app/1/view/1",
                },
                {
                  title: "Saldi d'autunno",
                  subtitle: "1 ott 2026 — Sconti su tutto il catalogo dello studio.",
                  url: "https://store.steampowered.com/news/app/1/view/2",
                },
              ],
            },
          ],
        },
      ],
    };
  }
  if (pluginId === "nexus-mods") {
    // Not for game files run by an emulator, and only for games Nexus Mods lists
    if (game.platform === "emulator" || roll < 0.25) return { sections: [] };
    const mods = Math.round(40 + roll * 26000);
    const slug = game.title.toLowerCase().replace(/[^a-z0-9]+/g, "");
    return {
      sections: [
        {
          title: "Nexus Mods",
          blocks: [
            { type: "stats", items: [{ label: "Mod disponibili", value: mods.toLocaleString("it-IT") }] },
            {
              type: "links",
              items: [{ label: "Sfoglia le mod su Nexus Mods", url: `https://www.nexusmods.com/games/${slug}/mods` }],
            },
          ],
        },
      ],
    };
  }
  if (pluginId === "pcgamingwiki") {
    return {
      sections: [
        {
          title: "PCGamingWiki",
          blocks: [
            {
              type: "stats",
              items: [
                { label: "Ultrawide", value: roll > 0.4 ? "Sì" : "Con una modifica" },
                { label: "HDR", value: roll > 0.6 ? "Sì" : "No" },
                { label: "Controller", value: "Sì" },
              ],
            },
            {
              type: "links",
              items: [{ label: "Apri su PCGamingWiki", url: "https://www.pcgamingwiki.com/wiki/Home" }],
            },
          ],
        },
      ],
    };
  }
  return {
    sections: [
      {
        title: "Prezzi",
        blocks: [
          {
            type: "notice",
            tone: "warning",
            text: "Aggiungi la tua chiave API gratuita di IsThereAnyDeal in Impostazioni → Plugin → Storico dei prezzi per vedere i prezzi.",
          },
          {
            type: "links",
            items: [{ label: "Ottieni una chiave API", url: "https://isthereanydeal.com/apps/my/" }],
          },
        ],
      },
    ],
  };
}

export function previewGameData(pluginId: string, game: DetectedGame): GameDataAnswer {
  const plugin = PLUGINS.find((entry) => entry.id === pluginId);
  return {
    pluginId,
    pluginName: plugin?.name ?? pluginId,
    data: sampleData(pluginId, game),
    error: null,
    logs: [],
    fetchedAt: Date.now(),
  };
}

export function previewCommand(games: DetectedGame[]): PluginView {
  const visible = games.filter((game) => !game.isHidden);
  const notes = [
    { hint: "Continua", note: "Continua · 12 h di gioco" },
    { hint: "Dal backlog", note: "Dal backlog · mai giocato" },
    { hint: "Riscopri", note: "Riscopri · non lo apri da 3 mesi" },
  ];
  const start = Math.floor(Math.random() * Math.max(1, visible.length));
  const picks = notes
    .map((entry, index) => ({ game: visible[(start + index * 2) % visible.length], ...entry }))
    .filter((pick, index, all) => pick.game && all.findIndex((p) => p.game === pick.game) === index);
  return {
    title: "Stasera potresti giocare a…",
    sections: [],
    gamesStyle: "mystery",
    games: picks.map((pick) => ({ id: pick.game.id, note: pick.note, hint: pick.hint })),
  };
}

/** Twelve weeks of sample play history spread over the games that have play time. */
export function previewSessions(games: DetectedGame[]): PlaySession[] {
  // The sample history is spread over the library's games: the emulated ones are left out
  // so that it stays what the interface tests were written against
  const played = games.filter(
    (game) => !game.isHidden && game.platform !== "emulator" && (game.playtimeMinutes ?? 0) > 0
  );
  if (played.length === 0) return [];
  const sessions: PlaySession[] = [];
  const now = Date.now();
  for (let day = 83; day >= 0; day -= 1) {
    const roll = hash(`day-${day}`);
    if (roll < 0.45) continue;
    const game = played[Math.floor(hash(`game-${day}`) * played.length)];
    const minutes = Math.round(25 + hash(`len-${day}`) * 170);
    sessions.push({
      gameId: game.id,
      startedAt: now - day * DAY - Math.round(roll * 6 * 60 * 60 * 1000),
      seconds: minutes * 60,
      source: game.platform === "steam" ? "steam" : "kanso",
    });
  }
  return sessions;
}

export function previewAchievements(game: DetectedGame): GameAchievements | null {
  if (game.platform !== "steam" || !game.achievementsTotal) return null;
  const unlocked = game.achievementsUnlocked ?? 0;
  const names = [
    ["Primi passi", "Completa il prologo"],
    ["Collezionista", "Trova 50 oggetti rari"],
    ["Senza un graffio", "Sconfiggi un boss senza subire danni"],
    ["Esploratore", "Scopri ogni regione della mappa"],
    ["Maestro artigiano", "Crea un oggetto leggendario"],
    ["Fino in fondo", "Completa la storia principale"],
  ];
  return {
    total: game.achievementsTotal,
    unlocked,
    items: names.map(([name, description], index) => {
      const isUnlocked = index < Math.min(unlocked, 3);
      return {
        id: `A${index}`,
        name,
        description,
        iconUrl: null,
        unlocked: isUnlocked,
        unlockedAt: isUnlocked ? Date.now() - (index + 1) * 9 * DAY : null,
        globalPercent: Math.round((80 - index * 13.5) * 10) / 10,
        progress: !isUnlocked && index === 3 ? 0.6 : null,
      };
    }),
  };
}
