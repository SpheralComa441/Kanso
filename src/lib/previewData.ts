import type { Deal, DetectedGame, DlssLibrary, EmulatorInfo, NewsItem, NewsSourceInfo } from "../types/game";

/**
 * Sample library shown only in the browser preview (`npm run dev` outside Tauri),
 * where no real scan can run. The desktop app never uses it.
 */

/** Generates a self-contained 600x900 SVG "cover" so the preview grid looks populated. */
function previewCover(title: string, from: string, to: string): string {
  const words = title.split(" ");
  const lines: string[] = [];
  let current = "";
  for (const word of words) {
    if (current && (current + " " + word).length > 13) {
      lines.push(current);
      current = word;
    } else {
      current = current ? `${current} ${word}` : word;
    }
  }
  if (current) lines.push(current);

  const escape = (s: string) =>
    s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
  const startY = 470 - (lines.length - 1) * 38;
  const text = lines
    .map(
      (line, i) =>
        `<text x="300" y="${startY + i * 76}" text-anchor="middle" font-family="Segoe UI, Inter, sans-serif" font-size="62" font-weight="700" fill="#ffffff" fill-opacity="0.92">${escape(line)}</text>`
    )
    .join("");
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 600 900"><defs><linearGradient id="g" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="${from}"/><stop offset="1" stop-color="${to}"/></linearGradient></defs><rect width="600" height="900" fill="url(#g)"/><circle cx="470" cy="170" r="210" fill="#ffffff" fill-opacity="0.06"/><circle cx="110" cy="780" r="260" fill="#000000" fill-opacity="0.18"/>${text}</svg>`;
  return `data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}`;
}

type PreviewSeed = Pick<
  DetectedGame,
  "id" | "title" | "platform" | "installDir" | "genres" | "shortDescription"
> &
  Partial<DetectedGame> & { colors?: [string, string] };

function seed(game: PreviewSeed): DetectedGame {
  const { colors, ...rest } = game;
  return {
    cleanTitle: game.title,
    platformAppId: null,
    executablePath: null,
    launchUri: null,
    steamgriddbId: null,
    coverPath: colors ? previewCover(game.title, colors[0], colors[1]) : null,
    isFavorite: false,
    metadataSynced: true,
    customOverride: false,
    titleProvisional: false,
    appType: null,
    isHidden: false,
    metadataAttemptedAt: 1700000000000,
    lastPlayedAt: null,
    playCount: 0,
    metadataLang: "it",
    tags: [],
    tagsLang: "it",
    playtimeMinutes: null,
    status: null,
    achievementsUnlocked: null,
    achievementsTotal: null,
    steamStoreAppid: null,
    isInstalled: true,
    customGenres: [],
    version: null,
    isFamilyShared: false,
    updatedAt: 1700000000000,
    ...rest,
  };
}

/**
 * What the sample plugin that brings a library lists while it is switched on: invented
 * games, filed under the plugin's own platform and started through its client.
 */
export const PREVIEW_PLUGIN_LIBRARY_GAMES: DetectedGame[] = [
  { id: "4101", title: "Lanterne", genres: ["Avventura", "Indie"], colors: ["#b45309", "#451a03"], playtimeMinutes: 190, lastPlayedAt: Date.now() - 4 * 86_400_000 },
  { id: "4102", title: "Paper Boats", genres: ["Puzzle", "Indie"], colors: ["#0e7490", "#083344"] },
  { id: "4103", title: "Moss and Gears", genres: ["Platform"], colors: ["#3f6212", "#1a2e05"], playtimeMinutes: 45 },
  { id: "4104", title: "Tiny Harbor", genres: ["Gestionale", "Indie"], colors: ["#7e22ce", "#3b0764"] },
].map(({ id, colors, ...game }) =>
  seed({
    ...game,
    id: `plugin:itch-library:${id}`,
    platform: "plugin",
    platformAppId: "itch-library",
    installDir: "",
    launchUri: `itch://games/${id}`,
    shortDescription: null,
    appType: "game",
    colors: colors as [string, string],
  })
);

export const PREVIEW_MOCK_GAMES: DetectedGame[] = [
  // In the library but not on disk: one finished long ago, one never installed here
  seed({
    id: "steam:620",
    title: "Portal 2",
    platform: "steam",
    platformAppId: "620",
    installDir: "",
    launchUri: "steam://rungameid/620",
    genres: ["Azione", "Avventura"],
    tags: ["Rompicapo", "Co-op online", "Giocatore singolo"],
    shortDescription:
      "Il seguito di Portal: nuovi personaggi, nuovi enigmi e una modalità cooperativa.",
    colors: ["#1f6fa8", "#0b1c2c"],
    isInstalled: false,
    status: "finished",
    playtimeMinutes: 1260,
    lastPlayedAt: Date.now() - 400 * 24 * 60 * 60 * 1000,
    achievementsUnlocked: 38,
    achievementsTotal: 51,
  }),
  seed({
    id: "ea:dead-space",
    title: "Dead Space",
    platform: "ea",
    installDir: "",
    genres: ["Azione", "Horror"],
    shortDescription: "Un classico dell'horror fantascientifico, ricostruito da zero.",
    colors: ["#3a4a52", "#0c1114"],
    isInstalled: false,
    playtimeMinutes: 95,
    lastPlayedAt: Date.now() - 200 * 24 * 60 * 60 * 1000,
  }),
  seed({
    id: "steam:1321440",
    status: "playing",
    achievementsUnlocked: 31,
    achievementsTotal: 128,
    title: "Cassette Beasts",
    tags: ["Collezione di creature", "Combattimento a turni", "Pixel art", "Co-op locale"],
    playtimeMinutes: 754,
    platform: "steam",
    platformAppId: "1321440",
    installDir: "D:\\SteamLibrary\\steamapps\\common\\Cassette Beasts",
    launchUri: "steam://rungameid/1321440",
    genres: ["Indie", "GDR"],
    shortDescription:
      "Colleziona fantastici mostri da usare in battaglie a turni in questo GDR open-world. Combina due forme di mostri qualsiasi usando il Sistema di Fusione per crearne di nuove e potenti!",
    colors: ["#7c5cd6", "#3a2a7a"],
    isFavorite: true,
    lastPlayedAt: Date.now() - 2 * 60 * 60 * 1000,
    playCount: 12,
  }),
  seed({
    id: "steam:1091500",
    status: "finished",
    achievementsUnlocked: 44,
    achievementsTotal: 57,
    lastPlayedAt: Date.now() - 95 * 24 * 60 * 60 * 1000,
    title: "Cyberpunk 2077",
    version: "build 15738292",
    tags: ["Mondo aperto", "Cyberpunk", "Giocatore singolo", "Fantascienza"],
    playtimeMinutes: 4380,
    platform: "steam",
    platformAppId: "1091500",
    installDir:
      "C:\\Program Files (x86)\\Steam\\steamapps\\common\\Cyberpunk 2077",
    launchUri: "steam://rungameid/1091500",
    steamgriddbId: 5276290,
    genres: ["Azione", "GDR"],
    shortDescription:
      "Cyberpunk 2077 è un GDR d'azione e avventura open-world ambientato a Night City.",
    colors: ["#e3c81c", "#8a5a0a"],
  }),
  seed({
    id: "steam:1145360",
    achievementsUnlocked: 49,
    achievementsTotal: 49,
    title: "Hades",
    tags: ["Stile Rogue", "Giocatore singolo", "Hack and slash", "Difficili"],
    playtimeMinutes: 2112,
    platform: "steam",
    platformAppId: "1145360",
    installDir: "D:\\SteamLibrary\\steamapps\\common\\Hades",
    launchUri: "steam://rungameid/1145360",
    genres: ["Azione", "Indie", "GDR"],
    shortDescription:
      "Sfida il dio dei morti e fatti strada con la forza fuori dagli Inferi in questo dungeon crawler roguelike.",
    colors: ["#c2372c", "#4a0f1c"],
    isFavorite: true,
    lastPlayedAt: Date.now() - 3 * 24 * 60 * 60 * 1000,
    playCount: 40,
  }),
  seed({
    id: "steam:1145361",
    title: "Hades Original Soundtrack",
    platform: "steam",
    platformAppId: "1145361",
    installDir: "D:\\SteamLibrary\\steamapps\\music\\Hades OST",
    launchUri: "steam://rungameid/1145361",
    genres: [],
    shortDescription: "La colonna sonora originale di Hades.",
    appType: "music",
    isHidden: true,
  }),
  seed({
    id: "gog:1207658924",
    status: "backlog",
    steamStoreAppid: "292030",
    title: "The Witcher 3: Wild Hunt",
    tags: ["Mondo aperto", "Giocatore singolo", "Fantasy", "Ricchi di storia"],
    platform: "gog",
    platformAppId: "1207658924",
    installDir: "E:\\GOG Games\\The Witcher 3 Wild Hunt",
    executablePath:
      "E:\\GOG Games\\The Witcher 3 Wild Hunt\\bin\\x64\\witcher3.exe",
    genres: ["GDR", "Avventura"],
    shortDescription:
      "Sei Geralt di Rivia, cacciatore di mostri, in un mondo aperto devastato dalla guerra.",
    colors: ["#6b1f1f", "#1a0c0c"],
  }),
  seed({
    id: "xbox:Microsoft.624F8B84B80",
    title: "Forza Horizon 5",
    platform: "xbox",
    platformAppId: "Microsoft.624F8B84B80",
    installDir: "C:\\XboxGames\\Forza Horizon 5",
    executablePath:
      "C:\\XboxGames\\Forza Horizon 5\\Content\\gamelaunchhelper.exe",
    genres: ["Corse", "Sport"],
    shortDescription:
      "Esplora i vivaci paesaggi open world del Messico al volante di centinaia di auto.",
    colors: ["#d6338a", "#3a1466"],
  }),
  seed({
    id: "steam:413150",
    status: "abandoned",
    achievementsUnlocked: 2,
    achievementsTotal: 49,
    title: "Stardew Valley",
    version: "build 14126492",
    isFamilyShared: true,
    tags: ["Simulatore di fattoria", "Pixel art", "Co-op online", "Rilassanti"],
    playtimeMinutes: 35,
    platform: "steam",
    platformAppId: "413150",
    installDir: "D:\\SteamLibrary\\steamapps\\common\\Stardew Valley",
    launchUri: "steam://rungameid/413150",
    genres: ["Indie", "GDR", "Simulazione"],
    shortDescription:
      "Hai ereditato la vecchia fattoria di tuo nonno a Stardew Valley. Riuscirai a vivere dei frutti della terra?",
    colors: ["#3f9b5a", "#1d4a3a"],
  }),
  seed({
    id: "steam:567090",
    title: "8-Bit Bayonetta",
    platform: "steam",
    platformAppId: "567090",
    installDir: "D:\\SteamLibrary\\steamapps\\common\\8-Bit Bayonetta",
    launchUri: "steam://rungameid/567090",
    genres: [],
    shortDescription: null,
  }),
  seed({
    id: "battlenet:fenris",
    title: "Diablo® IV",
    tags: ["Hack and slash", "Co-op online", "Fantasy dark"],
    cleanTitle: "Diablo IV",
    platform: "Battle.net",
    platformAppId: "Fenris",
    installDir: "C:\\Program Files (x86)\\Diablo IV",
    executablePath:
      "C:\\Program Files (x86)\\Diablo IV\\Diablo IV Launcher.exe",
    genres: ["Azione", "GDR"],
    shortDescription:
      "Unisciti alla lotta infinita tra gli Alti Cieli e gli Inferi Fiammeggianti.",
    colors: ["#8c1d18", "#1c0a0a"],
  }),
  seed({
    id: "epic:dc9d2e595d0e4650b35d659f90d41059",
    playtimeMinutes: 380,
    lastPlayedAt: Date.now() - 6 * 24 * 60 * 60 * 1000,
    steamStoreAppid: "",
    title: "Alan Wake 2 Deluxe Edition",
    cleanTitle: "Alan Wake 2",
    platform: "epic",
    platformAppId: "dc9d2e595d0e4650b35d659f90d41059",
    installDir: "D:\\Epic Games\\AlanWake2",
    executablePath: "D:\\Epic Games\\AlanWake2\\Binaries\\Win64\\AlanWake2.exe",
    launchUri:
      "com.epicgames.launcher://apps/da155068a9034375839136b4697593c1%3A235b28b9c6e2469da792fbabbc2fd25e%3Adc9d2e595d0e4650b35d659f90d41059?action=launch&silent=true",
    genres: ["Azione", "Avventura"],
    shortDescription: "Un thriller psicologico survival horror.",
    colors: ["#2c5f7a", "#0d1c26"],
  }),
  seed({
    id: "standalone:8f31c9a04d2e11b7",
    status: "finished",
    playtimeMinutes: 2460,
    lastPlayedAt: Date.now() - 40 * 24 * 60 * 60 * 1000,
    steamStoreAppid: "367520",
    title: "Hollow Knight",
    version: "1.5.78.11833",
    tags: ["Metroidvania", "Stile Souls", "Difficili", "Giocatore singolo"],
    platform: "standalone",
    installDir: "E:\\Games\\Hollow Knight",
    executablePath: "E:\\Games\\Hollow Knight\\hollow_knight.exe",
    genres: ["Azione", "Avventura", "Indie"],
    shortDescription:
      "Forgia il tuo cammino in Hollow Knight! Un'epica avventura d'azione in un vasto regno in rovina.",
    colors: ["#3b4a78", "#12162b"],
  }),
];

/** Emulators of the browser preview, as the desktop backend would report them. */
export const PREVIEW_MOCK_EMULATORS: EmulatorInfo[] = [
  {
    id: "dolphin",
    name: "Dolphin",
    systems: "GameCube · Wii",
    exePath: "D:\\Emulatori\\Dolphin\\Dolphin.exe",
    folders: [{ path: "D:\\Roms\\GameCube", fromEmulator: true }],
    addedByUser: false,
    version: "2509",
  },
  {
    id: "duckstation",
    name: "DuckStation",
    systems: "PlayStation",
    exePath: "D:\\Emulatori\\DuckStation\\duckstation-qt-x64-ReleaseLTCG.exe",
    folders: [{ path: "D:\\Roms\\PSX", fromEmulator: true }],
    addedByUser: false,
    version: "0.1.9226",
  },
  {
    id: "pcsx2",
    name: "PCSX2",
    systems: "PlayStation 2",
    exePath: "C:\\Program Files\\PCSX2\\pcsx2-qt.exe",
    folders: [],
    addedByUser: false,
    version: "2.4.0",
  },
];

function emulated(
  emulator: string,
  file: string,
  title: string,
  colors: [string, string],
  more: Partial<DetectedGame> = {}
): DetectedGame {
  const folder = emulator === "dolphin" ? "D:\\Roms\\GameCube" : "D:\\Roms\\PSX";
  return seed({
    id: `emu:${emulator}:${file.toLowerCase().replace(/[^a-z0-9]+/g, "-")}`,
    title,
    platform: "emulator",
    platformAppId: emulator,
    installDir: folder,
    executablePath: `${folder}\\${file}`,
    genres: [],
    shortDescription: null,
    colors,
    // A game file has no version of its own: it shows its emulator's
    version: emulator === "dolphin" ? "2509" : "0.1.9226",
    ...more,
  });
}

/** The game files those emulators run: kept apart from the library, in their own section. */
export const PREVIEW_MOCK_EMULATED_GAMES: DetectedGame[] = [
  emulated("dolphin", "Metroid Prime (Europe).rvz", "Metroid Prime", ["#b3541e", "#2a1206"], {
    playtimeMinutes: 540,
    lastPlayedAt: Date.now() - 6 * 24 * 60 * 60 * 1000,
    playCount: 9,
    genres: ["Azione", "Avventura"],
  }),
  emulated("dolphin", "Super Mario Sunshine (Europe).iso", "Super Mario Sunshine", ["#1f8fd6", "#0b2f52"]),
  emulated("dolphin", "Legend of Zelda, The - The Wind Waker (Europe).rvz", "The Legend of Zelda: The Wind Waker", ["#2f9d6b", "#0c3324"], {
    isFavorite: true,
    status: "playing",
  }),
  emulated("duckstation", "Final Fantasy VII (Italy) (Disc 1).chd", "Final Fantasy VII", ["#3a4f9c", "#10162e"], {
    playtimeMinutes: 1980,
    lastPlayedAt: Date.now() - 70 * 24 * 60 * 60 * 1000,
    playCount: 30,
  }),
  emulated("duckstation", "Crash Bandicoot (Europe).cue", "Crash Bandicoot", ["#d9731a", "#40200a"]),
];

const DAY_MS = 24 * 60 * 60 * 1000;

function previewBanner(title: string, from: string, to: string): string {
  const escape = (text: string) =>
    text.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 460 215"><defs><linearGradient id="g" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="${from}"/><stop offset="1" stop-color="${to}"/></linearGradient></defs><rect width="460" height="215" fill="url(#g)"/><circle cx="380" cy="40" r="120" fill="#ffffff" fill-opacity="0.07"/><text x="28" y="120" font-family="Segoe UI, Inter, sans-serif" font-size="34" font-weight="700" fill="#ffffff" fill-opacity="0.94">${escape(title)}</text></svg>`;
  return `data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}`;
}

/** Sample deals for the browser preview, shaped like the real store answers. */
/** A few sample sources and news, for the browser preview. */
export const PREVIEW_NEWS_SOURCES: NewsSourceInfo[] = [
  { id: "sample-it-giochi", name: "Giochi PC Oggi", lang: "it", topic: "gaming", siteUrl: "https://example.com/" },
  { id: "sample-it-hardware", name: "Hardware Italia", lang: "it", topic: "hardware", siteUrl: "https://example.org/" },
  { id: "sample-en-gaming", name: "PC Games Daily", lang: "en", topic: "gaming", siteUrl: "https://example.net/" },
  { id: "sample-en-hardware", name: "Silicon Report", lang: "en", topic: "hardware", siteUrl: "https://example.edu/" },
];

const sampleNews = (
  source: NewsSourceInfo,
  slug: string,
  title: string,
  summary: string,
  hoursAgo: number,
  colors: [string, string] | null
): NewsItem => ({
  id: `${source.id}:${source.siteUrl}${slug}`,
  source: source.id,
  sourceName: source.name,
  topic: source.topic,
  title,
  url: `${source.siteUrl}${slug}`,
  summary,
  imageUrl: colors ? previewBanner(title, colors[0], colors[1]) : null,
  publishedAt: Date.now() - hoursAgo * 60 * 60 * 1000,
});

export const PREVIEW_NEWS: NewsItem[] = [
  sampleNews(PREVIEW_NEWS_SOURCES[0], "gdr-open-world", "Il GDR open world più atteso ha una data di uscita su PC", "Lo studio ha confermato il giorno di lancio e i requisiti di sistema, con supporto a DLSS e FSR fin dal primo giorno.", 1, ["#7a3b1d", "#1d0f08"]),
  sampleNews(PREVIEW_NEWS_SOURCES[1], "nuova-gpu", "La nuova scheda video di fascia media alla prova: numeri e consumi", "Dodici giochi in tre risoluzioni, temperature e rumore: quanto conviene rispetto alla generazione precedente.", 3, ["#1d5a7a", "#081a24"]),
  sampleNews(PREVIEW_NEWS_SOURCES[0], "saldi-autunno", "Saldi d'autunno: i giochi da non lasciarsi sfuggire", "Una selezione ragionata tra indie e grandi produzioni, con i prezzi più bassi di sempre.", 6, null),
  sampleNews(PREVIEW_NEWS_SOURCES[1], "monitor-oled", "Monitor OLED a 240 Hz: i modelli in arrivo", "Pannelli più luminosi e garanzia estesa contro il burn-in: che cosa cambia per chi gioca.", 11, ["#4a2a6b", "#140a1f"]),
  sampleNews(PREVIEW_NEWS_SOURCES[0], "patch-strategia", "Lo strategico più giocato riceve la patch più grande di sempre", "", 20, ["#2f6b3a", "#0b1f10"]),
  sampleNews(PREVIEW_NEWS_SOURCES[1], "cpu-desktop", "Processori desktop: trapelano le specifiche della prossima serie", "Più core efficienti e una nuova piattaforma: le indiscrezioni e che cosa aspettarsi.", 30, null),
  sampleNews(PREVIEW_NEWS_SOURCES[2], "roguelike", "The roguelike everyone is playing gets a free expansion", "A new biome, two characters and mod support arrive next week on PC.", 2, ["#6b2a3a", "#1f0a10"]),
  sampleNews(PREVIEW_NEWS_SOURCES[3], "ssd-prices", "SSD prices are dropping again: what to buy now", "We compare the best 2 TB drives for gaming and look at where prices are heading.", 5, ["#3a3f6b", "#0e1020"]),
];

/** What the DLSS libraries of two sample games would read. */
export const PREVIEW_DLSS_LATEST = "310.9.16";
export const PREVIEW_DLSS: Record<string, DlssLibrary[]> = {
  "steam:1091500": [
    { part: "superResolution", version: "3.7.10", copies: 1, replaced: false },
    { part: "frameGeneration", version: "3.7.10", copies: 1, replaced: false },
    { part: "rayReconstruction", version: "3.5.10", copies: 1, replaced: false },
  ],
  "epic:dc9d2e595d0e4650b35d659f90d41059": [
    { part: "superResolution", version: "310.2.1", copies: 2, replaced: false },
    { part: "superResolution", version: "3.5", copies: 1, replaced: false },
  ],
};

export const PREVIEW_MOCK_DEALS: Deal[] = [
  {
    id: "epic:knightica",
    store: "epic",
    seller: null,
    title: "Knightica",
    url: "https://store.epicgames.com/it/p/knightica-9a3e",
    imageUrl: previewBanner("Knightica", "#31506b", "#101c26"),
    originalPrice: 13.49,
    finalPrice: 0,
    currency: "EUR",
    discountPercent: 100,
    upcoming: false,
    startsAt: Date.now() - 2 * DAY_MS,
    endsAt: Date.now() + 5 * DAY_MS,
    steamAppid: null,
    genres: [],
  },
  {
    id: "epic:alien-isolation",
    store: "epic",
    seller: null,
    title: "Alien: Isolation",
    url: "https://store.epicgames.com/it/p/alien-isolation",
    imageUrl: previewBanner("Alien: Isolation", "#1d3b2a", "#060d09"),
    originalPrice: 39.99,
    finalPrice: 0,
    currency: "EUR",
    discountPercent: 100,
    upcoming: true,
    startsAt: Date.now() + 5 * DAY_MS,
    endsAt: Date.now() + 12 * DAY_MS,
    steamAppid: null,
    genres: [],
  },
  {
    id: "steam:1145360",
    store: "steam",
    seller: null,
    title: "Hades",
    url: "https://store.steampowered.com/app/1145360/",
    imageUrl: previewBanner("Hades", "#c2372c", "#4a0f1c"),
    originalPrice: 24.5,
    finalPrice: 6.12,
    currency: "EUR",
    discountPercent: 75,
    upcoming: false,
    startsAt: null,
    endsAt: Date.now() + 3 * DAY_MS,
    steamAppid: "1145360",
    genres: ["Azione", "Indie", "GDR"],
  },
  {
    id: "steam:2406770",
    store: "steam",
    seller: null,
    title: "Bodycam",
    url: "https://store.steampowered.com/app/2406770/",
    imageUrl: previewBanner("Bodycam", "#4a4f57", "#15171a"),
    originalPrice: 33.32,
    finalPrice: 26.65,
    currency: "EUR",
    discountPercent: 20,
    upcoming: false,
    startsAt: null,
    endsAt: Date.now() + 6 * DAY_MS,
    steamAppid: "2406770",
    genres: ["Azione", "Sparatutto"],
  },
  {
    id: "steam:1903340",
    store: "steam",
    seller: null,
    title: "Clair Obscur: Expedition 33",
    url: "https://store.steampowered.com/app/1903340/",
    imageUrl: null,
    originalPrice: 49.99,
    finalPrice: 39.99,
    currency: "EUR",
    discountPercent: 20,
    upcoming: false,
    startsAt: null,
    endsAt: null,
    steamAppid: "1903340",
    genres: ["GDR", "Avventura"],
  },
  {
    id: "gog:1207658787",
    store: "gog",
    seller: null,
    title: "Heroes of Might and Magic® 3: Complete",
    url: "https://www.gog.com/en/game/heroes_of_might_and_magic_3_complete_edition",
    imageUrl: previewBanner("Heroes III", "#6b4a1d", "#1f1508"),
    originalPrice: 9.99,
    finalPrice: 4.99,
    currency: "EUR",
    discountPercent: 50,
    upcoming: false,
    startsAt: null,
    endsAt: null,
    steamAppid: null,
    genres: ["Strategia", "Fantasy"],
  },
  {
    id: "others:11:201",
    store: "others",
    seller: "Humble Store",
    title: "Citizen Sleeper",
    url: "https://www.cheapshark.com/redirect?dealID=previewHumble%3D",
    imageUrl: previewBanner("Citizen Sleeper", "#2b3a67", "#0d1226"),
    originalPrice: 19.99,
    finalPrice: 5.99,
    currency: "USD",
    discountPercent: 70,
    upcoming: false,
    startsAt: null,
    endsAt: null,
    steamAppid: "1578650",
    genres: ["GDR", "Avventura", "Indie"],
  },
  {
    id: "others:15:202",
    store: "others",
    seller: "Fanatical",
    title: "Cities: Skylines",
    url: "https://www.cheapshark.com/redirect?dealID=previewFanatical%3D",
    imageUrl: previewBanner("Cities: Skylines", "#3f7d6b", "#0f2a24"),
    originalPrice: 29.99,
    finalPrice: 8.99,
    currency: "USD",
    discountPercent: 70,
    upcoming: false,
    startsAt: null,
    endsAt: null,
    steamAppid: "255710",
    genres: ["Simulazione", "Strategia"],
  },
  {
    id: "others:27:203",
    store: "others",
    seller: "Gamesplanet",
    title: "Dead Island 2 Ultimate Edition",
    url: "https://www.cheapshark.com/redirect?dealID=previewGamesplanet%3D",
    imageUrl: null,
    originalPrice: 69.99,
    finalPrice: 6.99,
    currency: "USD",
    discountPercent: 90,
    upcoming: false,
    startsAt: null,
    endsAt: null,
    steamAppid: null,
    genres: [],
  }
];
