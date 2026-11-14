import type { DetectedGame } from "../types/game";

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
    updatedAt: 1700000000000,
    ...rest,
  };
}

export const PREVIEW_MOCK_GAMES: DetectedGame[] = [
  seed({
    id: "steam:1321440",
    title: "Cassette Beasts",
    platform: "steam",
    platformAppId: "1321440",
    installDir: "D:\\SteamLibrary\\steamapps\\common\\Cassette Beasts",
    launchUri: "steam://rungameid/1321440",
    genres: ["Indie", "GDR"],
    shortDescription:
      "Colleziona fantastici mostri da usare in battaglie a turni in questo GDR open-world. Combina due forme di mostri qualsiasi usando il Sistema di Fusione per crearne di nuove e potenti!",
    colors: ["#7c5cd6", "#3a2a7a"],
    isFavorite: true,
  }),
  seed({
    id: "steam:1091500",
    title: "Cyberpunk 2077",
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
    title: "Hades",
    platform: "steam",
    platformAppId: "1145360",
    installDir: "D:\\SteamLibrary\\steamapps\\common\\Hades",
    launchUri: "steam://rungameid/1145360",
    genres: ["Azione", "Indie", "GDR"],
    shortDescription:
      "Sfida il dio dei morti e fatti strada con la forza fuori dagli Inferi in questo dungeon crawler roguelike.",
    colors: ["#c2372c", "#4a0f1c"],
    isFavorite: true,
  }),
  seed({
    id: "steam:413150",
    title: "Stardew Valley",
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
    title: "Hollow Knight",
    platform: "standalone",
    installDir: "E:\\Games\\Hollow Knight",
    executablePath: "E:\\Games\\Hollow Knight\\hollow_knight.exe",
    genres: ["Azione", "Avventura", "Indie"],
    shortDescription:
      "Forgia il tuo cammino in Hollow Knight! Un'epica avventura d'azione in un vasto regno in rovina.",
    colors: ["#3b4a78", "#12162b"],
  }),
];
