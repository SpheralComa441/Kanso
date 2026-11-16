// HowLongToBeat: how long a game takes to finish.
//
// howlongtobeat.com has no official API. This plugin talks to the same search endpoint the
// site's own pages use, the way the maintained open-source clients do:
//   1. the path of the search endpoint is read from the site's scripts (it changes over time);
//   2. `<path>/init` hands out a short-lived token;
//   3. the search is a POST to `<path>` carrying that token.
// The path and the token are remembered in kanso.storage, so a game normally costs one request.

const SITE = "https://howlongtobeat.com";
const USER_AGENT = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";
const DEFAULT_SEARCH_PATH = "api/s";
const PATH_TTL = 3 * 24 * 60 * 60 * 1000;
const TOKEN_TTL = 10 * 60 * 1000;
const MAX_SCRIPTS = 30;

const TEXT = {
  main: { en: "Main story", it: "Storia principale", es: "Historia principal", ru: "Основной сюжет" },
  extra: { en: "Main + extras", it: "Storia + extra", es: "Historia + extras", ru: "Сюжет + дополнительное" },
  complete: { en: "Completionist", it: "Completista", es: "Completista", ru: "Полное прохождение" },
  all: { en: "All styles", it: "Tutti gli stili", es: "Todos los estilos", ru: "Все стили" },
  open: { en: "Open on HowLongToBeat", it: "Apri su HowLongToBeat", es: "Abrir en HowLongToBeat", ru: "Открыть на HowLongToBeat" },
  badgeTip: { en: "HowLongToBeat", it: "HowLongToBeat", es: "HowLongToBeat", ru: "HowLongToBeat" },
  badgeLabel: { en: "To finish", it: "Per finirlo", es: "Para terminarlo", ru: "Прохождение" },
  matched: { en: "Matched with", it: "Abbinato a", es: "Emparejado con", ru: "Сопоставлено с" },
};

function pageHeaders() {
  return { "User-Agent": USER_AGENT, Referer: SITE + "/" };
}

// Step 1: which /api/... path the site's own code POSTs searches to.
function discoverSearchPath() {
  const home = kanso.http.get(SITE + "/", { headers: pageHeaders() });
  if (!home.ok) throw new Error("howlongtobeat.com answered " + home.status);

  const scripts = [];
  const tag = /<script[^>]+src=["']([^"']+)["']/g;
  let match;
  while ((match = tag.exec(home.body)) !== null && scripts.length < MAX_SCRIPTS) {
    const src = match[1];
    if (src.indexOf("/_next/static/chunks/") === -1) continue;
    if (src.charAt(0) === "/") scripts.push(SITE + src);
    else if (src.indexOf(SITE + "/") === 0) scripts.push(src);
  }

  const call = /fetch\s*\(\s*["']\/api\/([a-zA-Z0-9_\/]+)[^"']*["']\s*,\s*\{[^}]*method:\s*["']POST["'][^}]*\}/i;
  for (const url of scripts) {
    let script;
    try {
      script = kanso.http.get(url, { headers: pageHeaders() });
    } catch (error) {
      kanso.log("skipping", url, error);
      continue;
    }
    const found = script.ok && call.exec(script.body);
    if (found) return "api/" + found[1].replace(/\/+$/, "");
  }
  kanso.log("search path not found in " + scripts.length + " scripts, using the default");
  return DEFAULT_SEARCH_PATH;
}

function searchPath(refresh) {
  const saved = kanso.storage.get("searchPath");
  if (!refresh && saved && saved.path && Date.now() - saved.at < PATH_TTL) return saved.path;
  const path = discoverSearchPath();
  kanso.storage.set("searchPath", { path: path, at: Date.now() });
  kanso.storage.remove("token");
  return path;
}

// Step 2: the token, plus a field whose name and value the site picks ("...key..." / "...val...").
function authToken(path, refresh) {
  const saved = kanso.storage.get("token");
  if (!refresh && saved && saved.path === path && Date.now() - saved.at < TOKEN_TTL) return saved;

  const reply = kanso.http.get(SITE + "/" + path + "/init?t=" + Date.now(), { headers: pageHeaders() });
  if (!reply.ok) throw new Error("token request answered " + reply.status);
  const data = reply.json();
  const token = { path: path, at: Date.now(), token: data.token, key: null, value: null };
  Object.keys(data).forEach((name) => {
    const lower = name.toLowerCase();
    if (lower.indexOf("key") !== -1) token.key = data[name];
    else if (lower.indexOf("val") !== -1) token.value = data[name];
  });
  kanso.storage.set("token", token);
  return token;
}

// Step 3: the search itself.
function search(title, path, token) {
  const payload = {
    searchType: "games",
    searchTerms: title.split(/\s+/).filter(Boolean),
    searchPage: 1,
    size: 20,
    searchOptions: {
      games: {
        userId: 0,
        platform: "",
        sortCategory: "popular",
        rangeCategory: "main",
        rangeTime: { min: 0, max: 0 },
        gameplay: { perspective: "", flow: "", genre: "", difficulty: "" },
        rangeYear: { max: "", min: "" },
        modifier: "",
      },
      users: { sortCategory: "postcount" },
      lists: { sortCategory: "follows" },
      filter: "",
      sort: 0,
      randomizer: 0,
    },
    useCache: true,
  };
  const headers = {
    "Content-Type": "application/json",
    Accept: "*/*",
    "User-Agent": USER_AGENT,
    Referer: SITE + "/",
    Origin: SITE,
  };
  if (token.token) headers["x-auth-token"] = String(token.token);
  if (token.key !== null && token.value !== null) {
    headers["x-hp-key"] = String(token.key);
    headers["x-hp-val"] = String(token.value);
    payload[token.key] = token.value;
  }
  return kanso.http.post(SITE + "/" + path, { headers: headers, json: payload });
}

function findGames(title) {
  let path = searchPath(false);
  let reply = search(title, path, authToken(path, false));
  if (!reply.ok) {
    // The endpoint moved or the token expired: look everything up again, once
    kanso.log("search answered " + reply.status + ", refreshing path and token");
    path = searchPath(true);
    reply = search(title, path, authToken(path, true));
  }
  if (!reply.ok) throw new Error("search answered " + reply.status);
  return reply.json().data || [];
}

// Letters and digits only, so "Baldur's Gate 3" and "Baldurs Gate 3" compare equal.
function key(text) {
  return String(text || "")
    .toLowerCase()
    .normalize("NFD")
    .replace(/[\u0300-\u036f]/g, "")
    .replace(/&/g, "and")
    .replace(/[^a-z0-9\u0400-\u04ff]+/g, "");
}

function bestMatch(game, results) {
  const wanted = [key(game.cleanTitle), key(game.title)].filter(Boolean);
  const names = (entry) => [key(entry.game_name), key(entry.game_alias)].filter(Boolean);
  const exact = results.find((entry) => names(entry).some((name) => wanted.indexOf(name) !== -1));
  if (exact) return exact;
  // The site sorts by popularity: accept its first answer only when one title contains the other
  const first = results[0];
  const close = first && names(first).some((name) => wanted.some((want) => name.indexOf(want) !== -1 || want.indexOf(name) !== -1));
  return close ? first : null;
}

function duration(seconds) {
  if (!seconds || seconds <= 0) return null;
  const hours = seconds / 3600;
  if (hours < 1) return Math.max(1, Math.round(seconds / 60)) + " min";
  const rounded = hours >= 20 ? Math.round(hours) : Math.round(hours * 2) / 2;
  const text = String(rounded);
  return (kanso.locale === "en" ? text : text.replace(".", ",")) + " h";
}

kanso.plugin({
  gameDetails(game) {
    const title = game.cleanTitle || game.title;
    if (!title) return null;

    const entry = bestMatch(game, findGames(title));
    if (!entry) return null;

    const rows = [
      { label: kanso.t(TEXT.main), value: duration(entry.comp_main) },
      { label: kanso.t(TEXT.extra), value: duration(entry.comp_plus) },
      { label: kanso.t(TEXT.complete), value: duration(entry.comp_100) },
      { label: kanso.t(TEXT.all), value: duration(entry.comp_all) },
    ].filter((row) => row.value);
    if (rows.length === 0) return null;

    const sameName = key(entry.game_name) === key(game.title) || key(entry.game_name) === key(game.cleanTitle);
    const blocks = [{ type: "stats", items: rows }];
    if (!sameName) blocks.push({ type: "text", text: kanso.t(TEXT.matched) + " “" + entry.game_name + "”" });
    blocks.push({ type: "links", items: [{ label: kanso.t(TEXT.open), url: SITE + "/game/" + entry.game_id }] });

    return {
      badge: { text: rows[0].value, label: kanso.t(TEXT.badgeLabel), tooltip: kanso.t(TEXT.badgeTip) + " · " + rows[0].label },
      sections: [{ title: "HowLongToBeat", blocks: blocks }],
    };
  },
});
