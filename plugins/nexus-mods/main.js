// How many mods a game has on Nexus Mods, and a link to its mods page.
//
// Nexus Mods publishes the list of the games it hosts mods for as one public file, with the
// number of mods of each: the same file its own mod manager reads. This plugin downloads it
// about once a week, keeps what it needs of it, and looks each game up in it here, on this
// PC. No account or key is involved, and no request names a game of the library.
//
// That file is not always there (it has been seen to answer "not found" for an hour and
// more). A list downloaded earlier is then used, however old. Only when there is none does
// the plugin ask Nexus Mods' public API about the one game whose sheet is open, by its
// title: still no account, but that request does name the game.
//
// The link opens the game's mods page in the user's browser: signing in, downloading and
// everything else happen there, never in Kanso.

const LIST_URL = "https://data.nexusmods.com/file/nexus-data/games.json";
const API_URL = "https://api.nexusmods.com/v2/graphql";
const MODS_PAGE = "https://www.nexusmods.com/games/";
const HOUR = 60 * 60 * 1000;
const LIST_KEPT_FOR = 7 * 24 * HOUR;
// After the list could not be downloaded it is not asked for again for this long
const LIST_RETRY_AFTER = 6 * HOUR;

const TEXT = {
  mods: { en: "Mods available", it: "Mod disponibili", es: "Mods disponibles", ru: "Доступно модов" },
  matched: { en: "On Nexus Mods as", it: "Su Nexus Mods come", es: "En Nexus Mods como", ru: "На Nexus Mods как" },
  open: { en: "Browse the mods on Nexus Mods", it: "Sfoglia le mod su Nexus Mods", es: "Ver los mods en Nexus Mods", ru: "Открыть моды на Nexus Mods" },
};

// Nexus Mods asks applications to say who they are
function appHeaders() {
  return { "Application-Name": "Kanso", "Application-Version": String(kanso.appVersion) };
}

const ROMAN = { ii: 2, iii: 3, iv: 4, v: 5, vi: 6, vii: 7, viii: 8, ix: 9, x: 10, xi: 11, xii: 12, xiii: 13, xiv: 14, xv: 15, xvi: 16 };
// Everything that is not a letter or a digit, accented and non-Latin letters kept
const NOT_NAME = new RegExp("[^a-z0-9" + String.fromCharCode(0xc0) + "-" + String.fromCharCode(0xffff) + "]+", "g");

// What two spellings of the same name have in common: letters and digits only, and a
// number written the Roman way read as a number ("Dark Souls III" is "Dark Souls 3").
function key(text) {
  return String(text || "")
    .toLowerCase()
    .replace(/&/g, "and")
    .replace(/[a-z]+/g, (word) => (ROMAN[word] ? String(ROMAN[word]) : word))
    .replace(NOT_NAME, "");
}

function reason(error) {
  return error && error.message ? error.message : String(error);
}

// The list as kept between calls: "name|domain|mods" for every game that has mods.
// Returns it parsed, or throws when there is none to be had.
function loadList(now) {
  const kept = kanso.storage.get("list", null);
  const have = kept && Array.isArray(kept.games) ? kept.games : null;
  if (have && now - kept.at < LIST_KEPT_FOR) return have;

  const failedAt = Number(kanso.storage.get("listFailedAt", 0));
  if (now - failedAt < LIST_RETRY_AFTER) {
    if (have) return have;
    throw new Error("the list of games was not there " + Math.max(1, Math.round((now - failedAt) / 60000)) + " min ago");
  }

  try {
    const fetched = kanso.http.getJson(LIST_URL, { headers: appHeaders() });
    if (!Array.isArray(fetched)) throw new Error("the list of games is not a list");
    const games = [];
    for (const entry of fetched) {
      const mods = Number(entry && entry.mods);
      const name = entry && typeof entry.name === "string" ? entry.name.replace(/\|/g, " ") : "";
      const domain = entry && typeof entry.domain_name === "string" ? entry.domain_name : "";
      if (name && mods > 0 && /^[a-z0-9]+$/.test(domain)) games.push(name + "|" + domain + "|" + mods);
    }
    if (games.length === 0) throw new Error("the list of games is empty");
    kanso.storage.set("list", { at: now, games: games });
    kanso.storage.remove("listFailedAt");
    return games;
  } catch (error) {
    kanso.storage.set("listFailedAt", now);
    // Last month's numbers are better than none
    if (have) return have;
    throw error;
  }
}

// The names a library title may go by on Nexus Mods, most exact first. Nexus Mods calls
// games by their everyday name: "Skyrim Special Edition" for "The Elder Scrolls V: Skyrim
// Special Edition", "The Witcher 3" where a store says "The Witcher 3: Wild Hunt".
function spellings(game) {
  const titles = [game.title, game.cleanTitle].filter(Boolean).map((title) => String(title).trim());
  const afterColon = [];
  const beforeColon = [];
  for (const title of titles) {
    const cut = title.search(/:\s| - /);
    if (cut > 0) {
      beforeColon.push(title.slice(0, cut).trim());
      afterColon.push(title.slice(cut + 1).replace(/^[\s:\-]+/, "").trim());
    }
  }
  const seen = {};
  return titles.concat(afterColon, beforeColon).filter((text) => {
    const k = key(text);
    if (k.length < 3 || seen[k]) return false;
    seen[k] = true;
    return true;
  });
}

// The entry that goes by one of the wanted names, the most exact name winning.
function pick(entries, wanted) {
  let found = null;
  let rank = wanted.length;
  for (const entry of entries) {
    const at = wanted.indexOf(key(entry.name));
    if (at !== -1 && at < rank && entry.mods > 0 && /^[a-z0-9]+$/.test(entry.domain)) {
      rank = at;
      found = entry;
      if (rank === 0) break;
    }
  }
  return found;
}

// Asks Nexus Mods' public API which of its games go by the given names. One request per
// way of searching (`op`), each name under an alias of its own.
function searchApi(names, op) {
  const fields = " { nodes { name domainName modCount } }";
  const declared = names.map((_, i) => "$n" + i + ": String!").join(", ");
  const searches = names
    .map((_, i) => "s" + i + ": games(filter: { name: [{ value: $n" + i + ", op: " + op + " }] }, count: 10)" + fields)
    .join(" ");
  const variables = {};
  names.forEach((name, i) => {
    variables["n" + i] = op === "WILDCARD" ? "*" + name + "*" : name;
  });
  const reply = kanso.http.post(API_URL, {
    headers: appHeaders(),
    json: { query: "query(" + declared + ") { " + searches + " }", variables: variables },
  });
  if (!reply.ok) throw new Error("HTTP " + reply.status + " from " + API_URL);
  const answer = reply.json() || {};
  if (!answer.data) {
    const first = Array.isArray(answer.errors) && answer.errors[0] ? answer.errors[0].message : "no data";
    throw new Error(String(first).slice(0, 160));
  }
  const entries = [];
  for (const alias of Object.keys(answer.data)) {
    const nodes = answer.data[alias] && answer.data[alias].nodes;
    if (!Array.isArray(nodes)) continue;
    for (const node of nodes) {
      if (node && typeof node.name === "string") {
        entries.push({ name: node.name, domain: String(node.domainName || ""), mods: Number(node.modCount) || 0 });
      }
    }
  }
  return entries;
}

// The game as Nexus Mods' API knows it, or null. The two ways of searching are tried in
// turn: the second only when the first found nothing or was refused.
function askApi(names, wanted) {
  let refused = null;
  for (const op of ["WILDCARD", "MATCHES"]) {
    try {
      const found = pick(searchApi(names.slice(0, 4), op), wanted);
      if (found) return found;
      refused = null;
    } catch (error) {
      refused = error;
    }
  }
  if (refused) throw refused;
  return null;
}

function grouped(count) {
  const separator = kanso.locale === "en" ? "," : kanso.locale === "ru" ? String.fromCharCode(160) : ".";
  return String(count).replace(/\B(?=(\d{3})+(?!\d))/g, separator);
}

kanso.plugin({
  gameDetails(game) {
    // A game file run by an emulator is not what the mods on Nexus Mods are for
    if (game.platform === "emulator") return null;

    const names = spellings(game);
    const wanted = names.map(key);
    if (wanted.length === 0) return null;

    let found = null;
    let list = null;
    let listError = null;
    try {
      list = loadList(Date.now());
    } catch (error) {
      listError = error;
    }
    if (list) {
      const entries = list.map((line) => {
        const parts = line.split("|");
        return { name: parts[0], domain: parts[1], mods: Number(parts[2]) };
      });
      found = pick(entries, wanted);
    } else {
      // No list at all: the one case in which Nexus Mods is asked about this game by name
      try {
        found = askApi(names, wanted);
      } catch (apiError) {
        throw new Error("Nexus Mods could not be read. Its list of games: " + reason(listError) + ". Its API: " + reason(apiError));
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
