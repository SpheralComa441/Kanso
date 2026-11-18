// itch.io library: the games bought or claimed on the account join Kanso's library.
//
// The list comes from itch.io's own API (https://api.itch.io/profile/owned-keys), read with
// the API key the user creates on itch.io and pastes in this plugin's settings. The key is
// sent in the Authorization header of those requests and nowhere else.
//
// A plugin cannot see the disk, so it cannot tell which games are installed: every game is
// listed as ready, and Play hands it to the itch app (which starts it, or offers to install
// it), or opens its page in the browser, as the user chose.

var PAGES_AT_MOST = 40;

var TEXT = {
  noKey: {
    en: "add your itch.io API key in this plugin's settings",
    it: "aggiungi la tua chiave API di itch.io nelle impostazioni di questo plugin",
    es: "añade tu clave API de itch.io en los ajustes de este plugin",
    ru: "добавьте ключ API itch.io в настройках этого плагина",
  },
  refused: {
    en: "itch.io did not accept the API key: check it in this plugin's settings",
    it: "itch.io non ha accettato la chiave API: controllala nelle impostazioni di questo plugin",
    es: "itch.io no aceptó la clave API: revísala en los ajustes de este plugin",
    ru: "itch.io не принял ключ API: проверьте его в настройках этого плагина",
  },
};

function isWebPage(url) {
  return typeof url === "string" && url.indexOf("https://") === 0;
}

function toLibraryGame(game, openInBrowser) {
  var entry = {
    id: String(game.id),
    title: String(game.title),
    installed: true,
    launch: openInBrowser && isWebPage(game.url) ? game.url : "itch://games/" + game.id,
  };
  if (typeof game.short_text === "string" && game.short_text.trim()) entry.description = game.short_text;
  // Covers are served by itch.io's own image host, the only other one this plugin declares
  if (typeof game.cover_url === "string" && game.cover_url.indexOf("https://img.itch.zone/") === 0) {
    entry.cover = game.cover_url;
  }
  return entry;
}

kanso.plugin({
  library: function () {
    var key = String(kanso.settings.apiKey || "").trim();
    // Thrown, not answered with an empty list: an empty list would say the account has no
    // games, and the ones already in the library would leave it
    if (!key) throw new Error(kanso.t(TEXT.noKey));
    var openInBrowser = kanso.settings.open === "web";

    var games = [];
    var seen = {};
    for (var page = 1; page <= PAGES_AT_MOST; page++) {
      var response = kanso.http.get("https://api.itch.io/profile/owned-keys?page=" + page, {
        headers: { Authorization: "Bearer " + key },
      });
      if (response.status === 401 || response.status === 403) throw new Error(kanso.t(TEXT.refused));
      if (!response.ok) throw new Error("HTTP " + response.status + " from api.itch.io");
      var body = response.json() || {};
      if (body.errors) {
        var first = Array.isArray(body.errors) ? body.errors[0] : body.errors;
        throw new Error(/key|auth/i.test(String(first)) ? kanso.t(TEXT.refused) : "itch.io: " + String(first));
      }
      // An empty page ends the list (itch.io writes an empty list as {})
      var keys = Array.isArray(body.owned_keys) ? body.owned_keys : [];
      if (keys.length === 0) break;
      keys.forEach(function (owned) {
        var game = owned && owned.game;
        if (!game || game.id === undefined || game.id === null || !game.title) return;
        // The account also holds soundtracks, books, tools and assets: only games are listed
        if (game.classification && game.classification !== "game") return;
        if (seen[game.id]) return;
        seen[game.id] = true;
        games.push(toLibraryGame(game, openInBrowser));
      });
    }
    return { games: games };
  },
});
