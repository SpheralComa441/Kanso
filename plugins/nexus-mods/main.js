// How many mods a game has on Nexus Mods, and a link to its mods page.
//
// Nexus Mods publishes the list of the games it hosts mods for as one public file, with the
// number of mods of each: the same file its own mod manager reads. This plugin downloads it
// about once a week, keeps what it needs of it, and looks each game up in it here, on this
// PC. No account or key is involved, and no request names a game of the library.
//
// The link opens the game's mods page in the user's browser: signing in, downloading and
// everything else happen there, never in Kanso.

const LIST_URL = "https://data.nexusmods.com/file/nexus-data/games.json";
const MODS_PAGE = "https://www.nexusmods.com/games/";
const LIST_DAYS = 7;
const DAY = 24 * 60 * 60 * 1000;

const TEXT = {
  mods: { en: "Mods available", it: "Mod disponibili", es: "Mods disponibles", ru: "Доступно модов" },
  matched: { en: "On Nexus Mods as", it: "Su Nexus Mods come", es: "En Nexus Mods como", ru: "На Nexus Mods как" },
  open: { en: "Browse the mods on Nexus Mods", it: "Sfoglia le mod su Nexus Mods", es: "Ver los mods en Nexus Mods", ru: "Открыть моды на Nexus Mods" },
};

const ROMAN = { ii: 2, iii: 3, iv: 4, v: 5, vi: 6, vii: 7, viii: 8, ix: 9, x: 10, xi: 11, xii: 12, xiii: 13, xiv: 14, xv: 15, xvi: 16 };

// What two spellings of the same name have in common: letters and digits only, and a
// number written the Roman way read as a number ("Dark Souls III" is "Dark Souls 3").
function key(text) {
  return String(text || "")
    .toLowerCase()
    .replace(/&/g, "and")
    .replace(/[a-z]+/g, (word) => (ROMAN[word] ? String(ROMAN[word]) : word))
    .replace(/[^a-z0-9\u00c0-\uffff]+/g, "");
}

// The list as kept between calls: "name|domain|mods" for every game that has mods.
function loadList(now) {
  const kept = kanso.storage.get("list", null);
  if (kept && Array.isArray(kept.games) && now - kept.at < LIST_DAYS * DAY) return kept.games;

  let fetched;
  try {
    // Nexus Mods asks applications to say who they are
    fetched = kanso.http.getJson(LIST_URL, {
      headers: { "Application-Name": "Kanso", "Application-Version": String(kanso.appVersion) },
    });
  } catch (error) {
    // Offline, or the file moved: last week's numbers are better than none
    if (kept && Array.isArray(kept.games)) return kept.games;
    throw error;
  }
  if (!Array.isArray(fetched)) throw new Error("Nexus Mods' list of games is not a list");

  const games = [];
  for (const entry of fetched) {
    const mods = Number(entry && entry.mods);
    const name = entry && typeof entry.name === "string" ? entry.name.replace(/\|/g, " ") : "";
    const domain = entry && typeof entry.domain_name === "string" ? entry.domain_name : "";
    if (name && mods > 0 && /^[a-z0-9]+$/.test(domain)) games.push(name + "|" + domain + "|" + mods);
  }
  if (games.length === 0) throw new Error("Nexus Mods' list of games is empty");
  kanso.storage.set("list", { at: now, games: games });
  return games;
}

// The spellings under which a library title may be listed, most exact first. Nexus Mods
// calls games by their everyday name: "Skyrim Special Edition" for "The Elder Scrolls V:
// Skyrim Special Edition", "The Witcher 3" where a store says "The Witcher 3: Wild Hunt".
function candidates(game) {
  const titles = [game.title, game.cleanTitle].filter(Boolean);
  const whole = titles.map(key);
  const afterColon = [];
  const beforeColon = [];
  for (const title of titles) {
    const cut = title.search(/:\s| - /);
    if (cut > 0) {
      beforeColon.push(key(title.slice(0, cut)));
      afterColon.push(key(title.slice(cut + 1)));
    }
  }
  return whole.concat(afterColon, beforeColon).filter((candidate) => candidate.length >= 3);
}

function grouped(count) {
  const separator = kanso.locale === "en" ? "," : kanso.locale === "ru" ? " " : ".";
  return String(count).replace(/\B(?=(\d{3})+(?!\d))/g, separator);
}

kanso.plugin({
  gameDetails(game) {
    // A game file run by an emulator is not what the mods on Nexus Mods are for
    if (game.platform === "emulator") return null;

    const wanted = candidates(game);
    if (wanted.length === 0) return null;
    const list = loadList(Date.now());

    let found = null;
    let rank = wanted.length;
    for (const line of list) {
      const parts = line.split("|");
      const at = wanted.indexOf(key(parts[0]));
      if (at !== -1 && at < rank) {
        rank = at;
        found = { name: parts[0], domain: parts[1], mods: Number(parts[2]) };
        if (rank === 0) break;
      }
    }
    if (!found) return null;

    const sameName = key(found.name) === key(game.title);
    const count = { label: kanso.t(TEXT.mods), value: grouped(found.mods) };
    // Matched under a shorter or longer name: say which, so that a wrong match shows
    if (!sameName) count.hint = kanso.t(TEXT.matched) + " “" + found.name + "”";
    return {
      sections: [
        {
          title: "Nexus Mods",
          blocks: [
            { type: "stats", items: [count] },
            { type: "links", items: [{ label: kanso.t(TEXT.open), url: MODS_PAGE + found.domain + "/mods" }] },
          ],
        },
      ],
    };
  },
});
