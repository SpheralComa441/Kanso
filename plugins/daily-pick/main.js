// "Today's pick": one game a day from the library, shown as a part of the Home page. No network.
// A good first example of a part of Home: `home(context)` receives the visible library and
// the time, and returns the same kind of view as a command.
//
// plugin.json keeps an answer for one hour ("cacheHours": 1), so the pick changes soon after
// midnight. Asked again within the day, the plugin gives the same game.

const DAY = 24 * 60 * 60 * 1000;

const TEXT = {
  title: { en: "Today's pick", it: "Il gioco di oggi", es: "El juego de hoy", ru: "Игра дня" },
  backlog: { en: "From your backlog", it: "Dal backlog", es: "De tus pendientes", ru: "Из отложенных" },
  rediscover: { en: "Rediscover", it: "Riscopri", es: "Redescubre", ru: "Вспомнить" },
  library: { en: "From your library", it: "Dalla tua libreria", es: "De tu biblioteca", ru: "Из вашей библиотеки" },
  neverPlayed: { en: "never played", it: "mai giocato", es: "nunca jugado", ru: "ещё не запускалась" },
  monthsAgo: { en: "not opened for {n} months", it: "non lo apri da {n} mesi", es: "sin abrir desde hace {n} meses", ru: "не запускалась {n} мес." },
  played: { en: "{time} played", it: "{time} di gioco", es: "{time} de juego", ru: "{time} в игре" },
  // The units are words of the line too: each language writes them its own way
  minutes: { en: "{n} min", it: "{n} min", es: "{n} min", ru: "{n} мин" },
  hours: { en: "{n} h", it: "{n} h", es: "{n} h", ru: "{n} ч" },
};

const started = (game) => (game.playtimeMinutes || 0) > 0 || game.playCount > 0;
// Waiting: marked "to play", or never started. A game marked as being played is not waiting,
// even when Kanso has no hours for it (played elsewhere, or before Kanso was there)
const waiting = (game) => game.status === "backlog" || (!started(game) && game.status !== "playing");
// Forgotten: played once, untouched for two months or more
const forgotten = (game, now) => started(game) && game.lastPlayedAt && now - game.lastPlayedAt >= 60 * DAY;

// The one line under the game: why it is today's.
function reason(game, now) {
  if (waiting(game)) {
    return kanso.t(TEXT.backlog) + (started(game) ? "" : " · " + kanso.t(TEXT.neverPlayed));
  }
  if (forgotten(game, now)) {
    const months = Math.floor((now - game.lastPlayedAt) / (30 * DAY));
    return kanso.t(TEXT.rediscover) + " · " + kanso.t(TEXT.monthsAgo).replace("{n}", months);
  }
  const minutes = game.playtimeMinutes || 0;
  if (minutes === 0) return kanso.t(TEXT.library);
  const time =
    minutes < 60
      ? kanso.t(TEXT.minutes).replace("{n}", minutes)
      : kanso.t(TEXT.hours).replace("{n}", Math.round(minutes / 60));
  return kanso.t(TEXT.library) + " · " + kanso.t(TEXT.played).replace("{time}", time);
}

// A number from a text, the same every time: the date chooses the game, not chance.
function numberFrom(text) {
  let value = 7;
  for (let i = 0; i < text.length; i++) value = (value * 31 + text.charCodeAt(i)) % 1000003;
  return value;
}

// What can be started right now comes first. Among those, and then among the games not on
// disk: the backlog, then what was left untouched for a long time, then anything.
// Not the same game two days running, when there is another: a group that holds nothing but
// yesterday's game is passed over, and that game comes back only when the library has no
// other to propose.
function choose(games, now, day, yesterdays) {
  const kinds = [waiting, (game) => forgotten(game, now), () => true];
  // True once a group was passed over because it held only yesterday's game: that game has
  // one day in two, and the group after it the days in between
  let everyOtherDay = false;
  for (const installed of [true, false]) {
    for (const kind of kinds) {
      const pool = games.filter((game) => (game.installed !== false) === installed && kind(game));
      if (pool.every((game) => game.id === yesterdays)) {
        if (pool.length > 0) everyOtherDay = true;
        continue;
      }
      if (everyOtherDay) {
        // From one day to the next the number grows by one, and this group is used every
        // second day: half the number grows by one a turn, so every game has its turn.
        // Yesterday's game has its own days and is not counted here.
        const others = pool.filter((game) => game.id !== yesterdays);
        return others[Math.floor(numberFrom(day) / 2) % others.length];
      }
      // The date points at one game of the whole group, and yesterday's gives way to the one
      // after it. (Taken out before counting, the game after it would hardly ever come up:
      // from one day to the next the number grows by one, and so would its place.)
      const index = numberFrom(day) % pool.length;
      return pool[pool[index].id === yesterdays ? (index + 1) % pool.length : index];
    }
  }
  return games.find((game) => game.id === yesterdays) || null;
}

kanso.plugin({
  home(context) {
    const now = context.now;
    const date = new Date(now);
    const day = date.getFullYear() + "-" + (date.getMonth() + 1) + "-" + date.getDate();
    // Finished and abandoned games are not proposed. Sorted by id, so that the order the
    // library happens to be in does not change the pick.
    const games = context.games
      .filter((game) => game.status !== "finished" && game.status !== "abandoned")
      .sort((a, b) => (a.id < b.id ? -1 : 1));

    // Today's pick is remembered: starting it does not make another game take its place
    const saved = kanso.storage.get("pick");
    let game = saved && saved.day === day ? games.find((entry) => entry.id === saved.id) : null;
    if (!game) {
      game = choose(games, now, day, saved ? saved.id : null);
      // Nothing to propose: no part on Home today
      if (!game) return null;
      kanso.storage.set("pick", { day: day, id: game.id });
    }

    return {
      title: kanso.t(TEXT.title),
      // One whole cover, with the line under its title and the buttons to open or start it
      gamesStyle: "cards",
      games: [{ id: game.id, note: reason(game, now) }],
    };
  },
});
