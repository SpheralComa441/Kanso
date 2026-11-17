// "What should I play tonight?": three suggestions from the library, no network needed.
// A good first example of a command: it receives the visible library and returns a view.

const DAY = 24 * 60 * 60 * 1000;

const TEXT = {
  title: { en: "Tonight you could play…", it: "Stasera potresti giocare a…", es: "Esta noche podrías jugar a…", ru: "Сегодня можно сыграть в…" },
  empty: {
    en: "There is nothing to suggest yet: the library is empty or every game is finished or abandoned.",
    it: "Non c'è ancora nulla da proporre: la libreria è vuota oppure tutti i giochi sono finiti o abbandonati.",
    es: "Todavía no hay nada que proponer: la biblioteca está vacía o todos los juegos están terminados o abandonados.",
    ru: "Пока нечего предложить: библиотека пуста или все игры пройдены либо заброшены.",
  },
  continuing: { en: "Keep going", it: "Continua", es: "Continúa", ru: "Продолжить" },
  backlog: { en: "From your backlog", it: "Dal backlog", es: "De tus pendientes", ru: "Из отложенных" },
  rediscover: { en: "Rediscover", it: "Riscopri", es: "Redescubre", ru: "Вспомнить" },
  played: { en: "{time} played", it: "{time} di gioco", es: "{time} de juego", ru: "{time} в игре" },
  neverPlayed: { en: "never played", it: "mai giocato", es: "nunca jugado", ru: "ещё не запускалась" },
  daysAgo: { en: "not opened for {n} days", it: "non lo apri da {n} giorni", es: "sin abrir desde hace {n} días", ru: "не запускалась {n} дн." },
  monthsAgo: { en: "not opened for {n} months", it: "non lo apri da {n} mesi", es: "sin abrir desde hace {n} meses", ru: "не запускалась {n} мес." },
};

function played(minutes) {
  const time = minutes < 60 ? minutes + " min" : Math.round(minutes / 60) + " h";
  return kanso.t(TEXT.played).replace("{time}", time);
}

function idleNote(game, now) {
  const days = Math.floor((now - game.lastPlayedAt) / DAY);
  if (days >= 60) return kanso.t(TEXT.monthsAgo).replace("{n}", Math.floor(days / 30));
  return kanso.t(TEXT.daysAgo).replace("{n}", days);
}

// Picks one entry at random, favourites twice as likely.
function draw(games, taken) {
  const pool = games.filter((game) => !taken.has(game.id));
  if (pool.length === 0) return null;
  const weights = pool.map((game) => (game.isFavorite ? 2 : 1));
  let roll = Math.random() * weights.reduce((sum, weight) => sum + weight, 0);
  for (let i = 0; i < pool.length; i++) {
    roll -= weights[i];
    if (roll <= 0) return pool[i];
  }
  return pool[pool.length - 1];
}

kanso.plugin({
  commands: {
    pick(context) {
      const now = context.now;
      const includeFinished = kanso.settings.includeFinished === "true";
      // Only what can be started right now: games that are not installed are left out
      const games = context.games.filter(
        (game) =>
          game.installed !== false &&
          game.status !== "abandoned" &&
          (includeFinished || game.status !== "finished")
      );

      const started = (game) => (game.playtimeMinutes || 0) > 0 || game.playCount > 0;
      const recent = (game) => game.lastPlayedAt && now - game.lastPlayedAt < 30 * DAY;

      // In progress: marked as such, or simply played in the last month
      const continuing = games.filter((game) => game.status === "playing" || (started(game) && recent(game) && game.status !== "backlog"));
      // Waiting: marked as backlog, or never started
      const waiting = games.filter((game) => game.status === "backlog" || !started(game));
      // Forgotten: played once, untouched for two months or more
      const forgotten = games.filter((game) => started(game) && game.lastPlayedAt && now - game.lastPlayedAt >= 60 * DAY);

      const taken = new Set();
      const picks = [];
      const add = (pool, label, detail) => {
        const game = draw(pool, taken);
        if (!game) return;
        taken.add(game.id);
        const extra = detail(game);
        // The hint is what the card says while it is still face down
        picks.push({ id: game.id, hint: kanso.t(label), note: extra ? kanso.t(label) + " · " + extra : kanso.t(label) });
      };

      add(continuing, TEXT.continuing, (game) => (game.playtimeMinutes ? played(game.playtimeMinutes) : ""));
      add(waiting, TEXT.backlog, (game) => (started(game) ? "" : kanso.t(TEXT.neverPlayed)));
      add(forgotten, TEXT.rediscover, (game) => idleNote(game, now));
      // Small libraries: fill up to three with anything not yet suggested
      while (picks.length < 3) {
        const before = picks.length;
        add(games, TEXT.rediscover, (game) => (game.playtimeMinutes ? played(game.playtimeMinutes) : kanso.t(TEXT.neverPlayed)));
        if (picks.length === before) break;
      }

      if (picks.length === 0) {
        return { title: kanso.t(TEXT.title), sections: [{ blocks: [{ type: "notice", text: kanso.t(TEXT.empty) }] }] };
      }
      return {
        title: kanso.t(TEXT.title),
        // The cards say what they are themselves: nothing to read first
        sections: [],
        // Whole covers, face down until each one is turned
        gamesStyle: "mystery",
        games: picks,
      };
    },
  },
});
