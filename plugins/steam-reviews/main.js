// Review scores: Steam user reviews (public store service, no key) and the Metacritic score.

const STEAM_LANGUAGE = { it: "italian", en: "english", es: "spanish", ru: "russian" };

const TEXT = {
  section: { en: "Reviews", it: "Recensioni", es: "Reseñas", ru: "Оценки" },
  steam: { en: "Steam users", it: "Utenti di Steam", es: "Usuarios de Steam", ru: "Пользователи Steam" },
  positive: { en: "positive", it: "positive", es: "positivas", ru: "положительных" },
  reviews: { en: "reviews", it: "recensioni", es: "reseñas", ru: "отзывов" },
  badgeTip: { en: "Positive reviews on Steam", it: "Recensioni positive su Steam", es: "Reseñas positivas en Steam", ru: "Положительные отзывы в Steam" },
  metacritic: { en: "Metacritic", it: "Metacritic", es: "Metacritic", ru: "Metacritic" },
  openSteam: { en: "Reviews on Steam", it: "Recensioni su Steam", es: "Reseñas en Steam", ru: "Отзывы в Steam" },
  openMetacritic: { en: "Metacritic page", it: "Pagina su Metacritic", es: "Página en Metacritic", ru: "Страница на Metacritic" },
};

function grouped(count) {
  return String(count).replace(/\B(?=(\d{3})+(?!\d))/g, kanso.locale === "en" ? "," : ".");
}

function steamSummary(appId) {
  const input = {
    ids: [{ appid: Number(appId) }],
    context: { language: STEAM_LANGUAGE[kanso.locale] || "english", country_code: "US" },
    data_request: { include_reviews: true },
  };
  const reply = kanso.http.getJson(
    "https://api.steampowered.com/IStoreBrowseService/GetItems/v1/?input_json=" + encodeURIComponent(JSON.stringify(input))
  );
  const item = ((reply.response || {}).store_items || [])[0];
  const summary = item && item.success === 1 && item.reviews && item.reviews.summary_filtered;
  return summary && summary.review_count > 0 ? summary : null;
}

function metacritic(appId) {
  // Optional extra: a failure here must not hide the Steam reviews
  try {
    const reply = kanso.http.getJson("https://store.steampowered.com/api/appdetails?appids=" + appId + "&filters=metacritic");
    const entry = reply[appId];
    const score = entry && entry.success && entry.data && entry.data.metacritic;
    return score && score.score ? score : null;
  } catch (error) {
    kanso.log("metacritic:", error);
    return null;
  }
}

kanso.plugin({
  gameDetails(game) {
    if (!game.steamAppId) return null;

    const summary = steamSummary(game.steamAppId);
    const critic = metacritic(game.steamAppId);
    if (!summary && !critic) return null;

    const blocks = [];
    const links = [];
    if (summary) {
      blocks.push({
        type: "meter",
        label: kanso.t(TEXT.steam) + " · " + summary.review_score_label,
        value: summary.percent_positive,
        max: 100,
        valueLabel: summary.percent_positive + "% " + kanso.t(TEXT.positive) + " · " + grouped(summary.review_count) + " " + kanso.t(TEXT.reviews),
      });
      links.push({ label: kanso.t(TEXT.openSteam), url: "https://store.steampowered.com/app/" + game.steamAppId + "/#app_reviews_hash" });
    }
    if (critic) {
      blocks.push({ type: "stats", items: [{ label: kanso.t(TEXT.metacritic), value: critic.score + " / 100" }] });
      if (critic.url) links.push({ label: kanso.t(TEXT.openMetacritic), url: critic.url });
    }
    blocks.push({ type: "links", items: links });

    return {
      badge: summary ? { text: summary.percent_positive + "%", tooltip: kanso.t(TEXT.badgeTip) + ": " + summary.review_score_label } : undefined,
      sections: [{ title: kanso.t(TEXT.section), blocks }],
    };
  },
});
