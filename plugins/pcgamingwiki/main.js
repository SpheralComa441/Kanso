// PCGamingWiki: technical facts about the PC version and a link to the game's page.
//
// The page is found through the wiki's own redirect for Steam appids, or by title; the facts
// are read from the page's source through the standard MediaWiki API (the wiki's Cargo query
// API is not open to anonymous users). Whatever fails, the link to the wiki is still shown.

const SITE = "https://www.pcgamingwiki.com";
const API = SITE + "/w/api.php?format=json&formatversion=2&action=query&redirects=1";

const TEXT = {
  open: { en: "Open on PCGamingWiki", it: "Apri su PCGamingWiki", es: "Abrir en PCGamingWiki", ru: "Открыть на PCGamingWiki" },
  search: { en: "Search on PCGamingWiki", it: "Cerca su PCGamingWiki", es: "Buscar en PCGamingWiki", ru: "Искать на PCGamingWiki" },
  ultrawide: { en: "Ultrawide", it: "Ultrawide", es: "Ultrapanorámico", ru: "Сверхширокий экран" },
  fourK: { en: "4K", it: "4K", es: "4K", ru: "4K" },
  hdr: { en: "HDR", it: "HDR", es: "HDR", ru: "HDR" },
  fps120: { en: "120+ FPS", it: "120+ FPS", es: "120+ FPS", ru: "120+ к/с" },
  rayTracing: { en: "Ray tracing", it: "Ray tracing", es: "Trazado de rayos", ru: "Трассировка лучей" },
  controller: { en: "Controller", it: "Controller", es: "Mando", ru: "Геймпад" },
  fullController: { en: "Full controller support", it: "Controller: supporto completo", es: "Mando: soporte completo", ru: "Геймпад: полная поддержка" },
  yes: { en: "Yes", it: "Sì", es: "Sí", ru: "Да" },
  no: { en: "No", it: "No", es: "No", ru: "Нет" },
  hackable: { en: "With a fix", it: "Con una modifica", es: "Con un ajuste", ru: "С исправлением" },
  limited: { en: "Limited", it: "Limitato", es: "Limitado", ru: "Ограниченно" },
  always: { en: "Always on", it: "Sempre attivo", es: "Siempre activo", ru: "Всегда включено" },
};

const VALUES = { true: TEXT.yes, false: TEXT.no, hackable: TEXT.hackable, limited: TEXT.limited, "always on": TEXT.always };

// Parameters of the page's {{Video}} and {{Input}} templates worth a tile.
const FACTS = [
  { template: "Video", param: "ultrawidescreen", label: TEXT.ultrawide },
  { template: "Video", param: "4k ultra hd", label: TEXT.fourK },
  { template: "Video", param: "hdr", label: TEXT.hdr },
  { template: "Video", param: "120 fps", label: TEXT.fps120 },
  { template: "Video", param: "ray tracing", label: TEXT.rayTracing },
  { template: "Input", param: "controller support", label: TEXT.controller },
  { template: "Input", param: "full controller", label: TEXT.fullController },
];

function pageUrl(title) {
  return SITE + "/wiki/" + encodeURIComponent(String(title).replace(/ /g, "_"));
}

// The API answers with a list of pages (or, in its older form, a map by page id).
function firstPage(reply) {
  const pages = reply && reply.query && reply.query.pages;
  if (!pages) return null;
  const list = Array.isArray(pages) ? pages : Object.keys(pages).map((id) => pages[id]);
  const page = list[0];
  return page && !("missing" in page) && !("invalid" in page) ? page : null;
}

// The wiki sends /api/appid.php?appid=N to the game's page: the address it points at is the title.
function titleFromSteam(appId) {
  const reply = kanso.http.get(SITE + "/api/appid.php?appid=" + encodeURIComponent(appId));
  const target = /\/wiki\/([^?#]+)/.exec(reply.headers.location || "");
  if (reply.status < 300 || reply.status >= 400 || !target) return null;
  const title = decodeURIComponent(target[1]).replace(/_/g, " ");
  return /^(special|home)\b/i.test(title) ? null : title;
}

// Source text of the page with this title; null when the wiki has no such page.
function pageSource(title) {
  const page = firstPage(kanso.http.getJson(API + "&prop=revisions&rvprop=content&rvslots=main&titles=" + encodeURIComponent(title)));
  if (!page) return null;
  const revision = (page.revisions || [])[0] || {};
  const slot = (revision.slots && revision.slots.main) || revision;
  return { title: page.title || title, text: String(slot.content || slot["*"] || "") };
}

// "name = value" pairs of the first {{Template ...}} block of the page.
function templateParams(text, template) {
  const start = new RegExp("\\{\\{\\s*" + template + "\\s*[\\n|]", "i").exec(text);
  if (!start) return {};
  const rest = text.slice(start.index + 2);
  // Usually one parameter per line and the closing braces on their own line
  const end = /\n\}\}/.exec(rest) || /\}\}/.exec(rest);
  const block = end ? rest.slice(0, end.index) : rest.slice(0, 6000);
  const params = {};
  const pair = /\|\s*([^=\n|{}\[\]]+?)\s*=[ \t]*([^\n|{}\[\]]*)/g;
  let match;
  while ((match = pair.exec(block)) !== null) {
    const name = match[1].toLowerCase();
    if (!(name in params)) params[name] = match[2].trim().toLowerCase();
  }
  return params;
}

function facts(text) {
  const cache = {};
  return FACTS.map((fact) => {
    const params = cache[fact.template] || (cache[fact.template] = templateParams(text, fact.template));
    const value = VALUES[params[fact.param]];
    return value ? { label: kanso.t(fact.label), value: kanso.t(value) } : null;
  }).filter(Boolean);
}

function section(blocks) {
  return { title: "PCGamingWiki", blocks: blocks };
}

kanso.plugin({
  gameDetails(game) {
    let title = null;
    let failed = false;

    if (game.steamAppId) {
      try {
        title = titleFromSteam(game.steamAppId);
      } catch (error) {
        failed = true;
        kanso.log("appid lookup:", error);
      }
    }

    let source = null;
    for (const candidate of [title, game.title, game.cleanTitle]) {
      if (!candidate || source) continue;
      try {
        source = pageSource(candidate);
      } catch (error) {
        failed = true;
        kanso.log("page source:", error);
        break;
      }
    }

    const known = (source && source.title) || title;
    if (known) {
      const items = source ? facts(source.text) : [];
      const blocks = [];
      if (items.length > 0) blocks.push({ type: "stats", items: items });
      blocks.push({ type: "links", items: [{ label: kanso.t(TEXT.open), url: pageUrl(known) }] });
      // Facts that could not be read this time are worth another try soon
      return { cacheHours: source ? undefined : 12, sections: [section(blocks)] };
    }

    // The wiki could not be asked: leave a way to get there, and try again soon
    if (failed) {
      const link = game.steamAppId
        ? { label: kanso.t(TEXT.open), url: SITE + "/api/appid.php?appid=" + encodeURIComponent(game.steamAppId) }
        : { label: kanso.t(TEXT.search), url: SITE + "/w/index.php?search=" + encodeURIComponent(game.cleanTitle || game.title) };
      return { cacheHours: 12, sections: [section([{ type: "links", items: [link] }])] };
    }
    return null;
  },
});
