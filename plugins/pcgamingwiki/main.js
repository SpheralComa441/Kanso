// PCGamingWiki: technical facts about the PC version, through the wiki's public Cargo API.
// Each table is asked separately and a table that does not answer is simply left out, so the
// link to the game's page is shown even if the wiki renames a field.

const API = "https://www.pcgamingwiki.com/w/api.php";

const TEXT = {
  open: { en: "Open on PCGamingWiki", it: "Apri su PCGamingWiki", es: "Abrir en PCGamingWiki", ru: "Открыть на PCGamingWiki" },
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

function cargo(params) {
  const query = Object.keys(params)
    .map((name) => name + "=" + encodeURIComponent(params[name]))
    .join("&");
  const reply = kanso.http.getJson(API + "?action=cargoquery&format=json&limit=1&" + query);
  if (reply.error) throw new Error(reply.error.info || reply.error.code || "cargo error");
  const first = (reply.cargoquery || [])[0];
  return first ? first.title : null;
}

function quoted(text) {
  return '"' + String(text).replace(/["\\]/g, " ") + '"';
}

function findPage(game) {
  const fields = "Infobox_game._pageName=Page,Infobox_game._pageID=PageID";
  if (game.steamAppId) {
    const byApp = cargo({ tables: "Infobox_game", fields: fields, where: "Infobox_game.Steam_AppID HOLDS " + quoted(game.steamAppId) });
    if (byApp) return byApp;
  }
  for (const title of [game.title, game.cleanTitle]) {
    if (!title) continue;
    const byName = cargo({ tables: "Infobox_game", fields: fields, where: "Infobox_game._pageName=" + quoted(title) });
    if (byName) return byName;
  }
  return null;
}

// Facts of one table for the page; `fields` maps the alias asked for to its label.
function facts(table, pageId, fields) {
  try {
    const row = cargo({
      tables: table,
      fields: Object.keys(fields).map((alias) => table + "." + fields[alias].field + "=" + alias).join(","),
      where: table + "._pageID=" + quoted(pageId),
    });
    if (!row) return [];
    return Object.keys(fields)
      .map((alias) => {
        const raw = String(row[alias] === null || row[alias] === undefined ? "" : row[alias]).trim().toLowerCase();
        const text = VALUES[raw];
        return text ? { label: kanso.t(fields[alias].label), value: kanso.t(text) } : null;
      })
      .filter(Boolean);
  } catch (error) {
    kanso.log(table + ":", error);
    return [];
  }
}

kanso.plugin({
  gameDetails(game) {
    const page = findPage(game);
    if (!page || !page.Page) return null;

    const items = facts("Video", page.PageID, {
      Ultrawide: { field: "Ultrawidescreen", label: TEXT.ultrawide },
      FourK: { field: "4K_Ultra_HD", label: TEXT.fourK },
      Hdr: { field: "HDR", label: TEXT.hdr },
      Fps120: { field: "120_FPS", label: TEXT.fps120 },
      RayTracing: { field: "Ray_tracing", label: TEXT.rayTracing },
    }).concat(
      facts("Input", page.PageID, {
        Controller: { field: "Controller_support", label: TEXT.controller },
        FullController: { field: "Full_controller_support", label: TEXT.fullController },
      })
    );

    const blocks = [];
    if (items.length > 0) blocks.push({ type: "stats", items: items });
    blocks.push({
      type: "links",
      items: [{ label: kanso.t(TEXT.open), url: "https://www.pcgamingwiki.com/wiki/" + encodeURIComponent(String(page.Page).replace(/ /g, "_")) }],
    });
    return { sections: [{ title: "PCGamingWiki", blocks: blocks }] };
  },
});
