// News and patch notes: the developers' own announcements, from Steam's public news service.

const TEXT = {
  section: { en: "News", it: "Novità", es: "Novedades", ru: "Новости" },
  patch: { en: "Patch notes", it: "Note della patch", es: "Notas del parche", ru: "Обновление" },
  all: { en: "All news on Steam", it: "Tutte le novità su Steam", es: "Todas las novedades en Steam", ru: "Все новости в Steam" },
};

const MONTHS = {
  it: ["gen", "feb", "mar", "apr", "mag", "giu", "lug", "ago", "set", "ott", "nov", "dic"],
  en: ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"],
  es: ["ene", "feb", "mar", "abr", "may", "jun", "jul", "ago", "sept", "oct", "nov", "dic"],
  ru: ["янв.", "февр.", "мар.", "апр.", "мая", "июн.", "июл.", "авг.", "сент.", "окт.", "нояб.", "дек."],
};

function day(seconds) {
  const date = new Date(seconds * 1000);
  const months = MONTHS[kanso.locale] || MONTHS.en;
  return date.getDate() + " " + months[date.getMonth()] + " " + date.getFullYear();
}

// The service returns the text with leftovers of Steam's markup: keep the words only.
function plain(text) {
  return String(text || "")
    .replace(/\{STEAM_CLAN_IMAGE\}\S*/g, " ")
    .replace(/\[\/?[a-z*][^\]]*\]/gi, " ")
    .replace(/<[^>]+>/g, " ")
    .replace(/https?:\/\/\S+/g, " ")
    .replace(/\s+/g, " ")
    .trim();
}

kanso.plugin({
  gameDetails(game) {
    if (!game.steamAppId) return null;
    const count = Number(kanso.settings.count) || 4;
    const reply = kanso.http.getJson(
      "https://api.steampowered.com/ISteamNews/GetNewsForApp/v2/?appid=" + game.steamAppId +
        "&count=" + count + "&maxlength=220&feeds=steam_community_announcements"
    );
    const items = ((reply.appnews || {}).newsitems || []).slice(0, count);
    if (items.length === 0) return null;

    const rows = items.map((item) => {
      const isPatch = (item.tags || []).indexOf("patchnotes") !== -1;
      const summary = plain(item.contents);
      const lead = day(item.date) + (isPatch ? " · " + kanso.t(TEXT.patch) : "");
      return {
        title: plain(item.title) || kanso.t(TEXT.section),
        subtitle: summary ? lead + " — " + summary : lead,
        url: "https://store.steampowered.com/news/app/" + game.steamAppId + "/view/" + item.gid,
      };
    });

    return {
      sections: [
        {
          title: kanso.t(TEXT.section),
          blocks: [
            { type: "list", items: rows },
            { type: "links", items: [{ label: kanso.t(TEXT.all), url: "https://store.steampowered.com/news/app/" + game.steamAppId }] },
          ],
        },
      ],
    };
  },
});
