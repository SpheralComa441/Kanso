// Price history: best current price and all-time low, from the IsThereAnyDeal API (v2).
// The user's own API key is a setting of the plugin; it is sent to api.isthereanydeal.com only.

const API = "https://api.isthereanydeal.com";

const TEXT = {
  section: { en: "Prices", it: "Prezzi", es: "Precios", ru: "Цены" },
  needKey: {
    en: "Add your free IsThereAnyDeal API key in Settings → Plugins → Price history to see prices.",
    it: "Aggiungi la tua chiave API gratuita di IsThereAnyDeal in Impostazioni → Plugin → Storico dei prezzi per vedere i prezzi.",
    es: "Añade tu clave API gratuita de IsThereAnyDeal en Ajustes → Plugins → Historial de precios para ver los precios.",
    ru: "Добавьте бесплатный ключ API IsThereAnyDeal в «Настройки → Плагины → История цен», чтобы видеть цены.",
  },
  badKey: {
    en: "IsThereAnyDeal refused the API key: check it in the plugin's settings.",
    it: "IsThereAnyDeal ha rifiutato la chiave API: controllala nelle impostazioni del plugin.",
    es: "IsThereAnyDeal rechazó la clave API: revísala en los ajustes del plugin.",
    ru: "IsThereAnyDeal отклонил ключ API: проверьте его в настройках плагина.",
  },
  getKey: { en: "Get an API key", it: "Ottieni una chiave API", es: "Obtener una clave API", ru: "Получить ключ API" },
  now: { en: "Best price now", it: "Miglior prezzo ora", es: "Mejor precio ahora", ru: "Лучшая цена сейчас" },
  low: { en: "All-time low", it: "Minimo storico", es: "Mínimo histórico", ru: "Исторический минимум" },
  open: { en: "Open on IsThereAnyDeal", it: "Apri su IsThereAnyDeal", es: "Abrir en IsThereAnyDeal", ru: "Открыть на IsThereAnyDeal" },
  deal: { en: "Go to the offer", it: "Vai all'offerta", es: "Ir a la oferta", ru: "Перейти к предложению" },
};

function money(price) {
  if (!price || typeof price.amount !== "number") return null;
  const amount = price.amount.toFixed(2);
  return (kanso.locale === "en" ? amount : amount.replace(".", ",")) + " " + (price.currency || "");
}

function describe(offer) {
  const price = offer && money(offer.price);
  if (!price) return null;
  const parts = [price];
  if (offer.cut) parts.push("−" + offer.cut + "%");
  if (offer.shop && offer.shop.name) parts.push(offer.shop.name);
  return parts.join(" · ");
}

function request(method, path, key, options) {
  const reply = kanso.http.request(Object.assign({ method: method, url: API + path + (path.indexOf("?") === -1 ? "?" : "&") + "key=" + encodeURIComponent(key) }, options || {}));
  if (reply.status === 401 || reply.status === 403) return { denied: true };
  if (!reply.ok) throw new Error("IsThereAnyDeal answered " + reply.status);
  return { data: reply.json() };
}

function notice(text) {
  return {
    sections: [
      {
        title: kanso.t(TEXT.section),
        blocks: [
          { type: "notice", tone: "warning", text: kanso.t(text) },
          { type: "links", items: [{ label: kanso.t(TEXT.getKey), url: "https://isthereanydeal.com/apps/my/" }] },
        ],
      },
    ],
  };
}

// IsThereAnyDeal's own id of the game, remembered once found.
function gameId(game, key) {
  const saved = kanso.storage.get("id:" + game.id);
  if (saved) return { id: saved };
  const query = game.steamAppId ? "appid=" + encodeURIComponent(game.steamAppId) : "title=" + encodeURIComponent(game.cleanTitle || game.title);
  const lookup = request("GET", "/games/lookup/v1?" + query, key);
  if (lookup.denied) return lookup;
  const found = lookup.data && lookup.data.found && lookup.data.game && lookup.data.game.id;
  if (found) kanso.storage.set("id:" + game.id, found);
  return { id: found || null };
}

kanso.plugin({
  gameDetails(game) {
    const key = (kanso.settings.apiKey || "").trim();
    if (!key) return notice(TEXT.needKey);
    const country = /^[A-Za-z]{2}$/.test(kanso.settings.country || "") ? kanso.settings.country.toUpperCase() : "US";

    const match = gameId(game, key);
    if (match.denied) return notice(TEXT.badKey);
    if (!match.id) return null;

    const overview = request("POST", "/games/overview/v2?country=" + country, key, { json: [match.id] });
    if (overview.denied) return notice(TEXT.badKey);
    const entry = ((overview.data && overview.data.prices) || [])[0];
    if (!entry) return null;

    const items = [];
    const current = describe(entry.current);
    if (current) items.push({ label: kanso.t(TEXT.now), value: current });
    const lowest = describe(entry.lowest);
    if (lowest) {
      const when = entry.lowest.timestamp ? String(entry.lowest.timestamp).slice(0, 10) : undefined;
      items.push({ label: kanso.t(TEXT.low), value: lowest, hint: when });
    }
    if (items.length === 0) return null;

    const links = [];
    if (entry.urls && entry.urls.game) links.push({ label: kanso.t(TEXT.open), url: entry.urls.game });
    if (entry.current && entry.current.url) links.push({ label: kanso.t(TEXT.deal), url: entry.current.url });

    const blocks = [{ type: "stats", items: items }];
    if (links.length > 0) blocks.push({ type: "links", items: links });
    return { sections: [{ title: kanso.t(TEXT.section), blocks: blocks }] };
  },
});
