# Plugin di Kanso / Kanso plugins

Un plugin è una cartella con due file, `plugin.json` e `main.js`. Aggiunge informazioni alla scheda
di un gioco (e una piccola etichetta sulle card), una parte alla **Home**, comandi al menu
**Plugin**, oppure una **libreria di giochi** tutta sua (i giochi di un altro store o servizio,
sotto una piattaforma propria).
*A plugin is a folder with two files, `plugin.json` and `main.js`. It adds information to a game's
sheet (and a short label on the cards), a part to the **Home** page, commands to the **Plugins**
menu, or a **library of games** of its own (the games of another store or service, under a
platform of their own).*

I plugin ufficiali stanno in [`plugins/`](../plugins/) e sono il miglior punto di partenza:
[`play-tonight`](../plugins/play-tonight/) (un comando, niente rete),
[`daily-pick`](../plugins/daily-pick/) (una parte della Home, niente rete: da qui si parte per
aggiungere qualcosa alla Home),
[`steam-news`](../plugins/steam-news/) (una richiesta, un elenco),
[`steam-reviews`](../plugins/steam-reviews/) (etichetta sulle card),
[`howlongtobeat`](../plugins/howlongtobeat/) (più richieste, `kanso.storage`),
[`price-history`](../plugins/price-history/) (impostazioni con chiave API),
[`itch-library`](../plugins/itch-library/) (una libreria di giochi: quelli dell'account di
itch.io, letti con la chiave API dell'utente).
*The official plugins in [`plugins/`](../plugins/) are the best starting point;
[`daily-pick`](../plugins/daily-pick/) is the one to copy for a part of Home,
[`itch-library`](../plugins/itch-library/) for a library of games.*

> The rest of this guide is in English: it is the reference for plugin authors.

## How a plugin runs

- Plugin code does **not** run in Kanso's window. It runs in a small JavaScript interpreter
  (QuickJS) embedded in the app, started fresh for every call and thrown away afterwards.
- It has no DOM, no `fetch`, no file system, no timers, no access to Kanso's settings or API keys.
  It gets one global, `kanso`, described below.
- It may contact **only** the hosts listed in its `plugin.json`, over `https`. The list is shown to
  the user before installing and in *Settings → Plugins*.
- It never draws anything itself: it returns plain data (sections made of a few kinds of blocks,
  and games of the library to point at) and Kanso draws them with its own components and the
  user's theme.
- Each call is limited: 30 s in total, 64 MB of memory, 48 requests, 4 MB per response.

A plugin that fails, loops or is hostile can at worst show an error in its own section, or in
its own line on Home.

## Installing

- *Settings → Plugins → Install plugin…* and pick the plugin's `plugin.json`. Kanso shows the
  name, author, and the hosts it may contact, and installs it only after you confirm.
- Or *Open folder* and drop the plugin's folder there (it then appears in the list, switched off).

Installed plugins live in `%LOCALAPPDATA%\com.kanso.launcher\plugins\<id>\`. Only `plugin.json`
and `main.js` are copied. A user plugin with the same `id` as an official one replaces it (remove
it to get the official one back) — which is also how an official plugin can be updated without
waiting for a new version of Kanso.

While developing, install once, then edit the installed copy in place: a changed `main.js` or
changed settings invalidate the plugin's cached answers automatically.

## `plugin.json`

```json
{
  "kansoPlugin": 1,
  "id": "my-plugin",
  "name": { "en": "My plugin", "it": "Il mio plugin" },
  "version": "1.0.0",
  "author": "You",
  "description": { "en": "What it does.", "it": "Che cosa fa." },
  "homepage": "https://example.org/my-plugin",
  "hosts": ["api.example.org"],
  "provides": { "gameDetails": true, "badge": true, "background": true, "home": true },
  "cacheHours": 168,
  "commands": [{ "id": "pick", "title": { "en": "Pick a game" }, "description": "Optional" }],
  "settings": [
    { "key": "apiKey", "type": "secret", "label": { "en": "API key" }, "help": "Where to get it." },
    { "key": "region", "type": "select", "default": "eu", "options": ["eu", { "value": "us", "label": "USA" }] },
    { "key": "compact", "type": "toggle", "default": false },
    { "key": "note", "type": "text", "default": "" }
  ]
}
```

| Field | |
|---|---|
| `kansoPlugin` | Format version. Must be `1`. |
| `id` | 2–40 lower-case letters, digits, dashes. Also the folder name. |
| `name`, `description`, and every `title` / `label` / `help` | A string, or one string per language (`it`, `en`, `es`, `ru`); English is the fallback. |
| `version` | Plain version such as `1.2.0`. |
| `hosts` | Up to 8 exact host names. No scheme, path, port, wildcard, IP address or `localhost`. Omit for a plugin that works offline. |
| `provides.gameDetails` | The plugin implements `gameDetails(game)`. |
| `provides.badge` | `gameDetails` may return a `badge` for the game cards. |
| `provides.background` | Kanso may call `gameDetails` quietly for the whole library (one game every few seconds), so the badges are there before each sheet is opened. Leave it off unless you return a badge. |
| `provides.home` | The plugin implements `home(context)`: a part of the Home page. Only `true` switches it on. |
| `provides.library` | The plugin implements `library(context)`: games that join the library under a platform of their own. Only `true` switches it on. See [A library of games](#a-library-of-games). |
| `library.name` | What the platform is called in the side panel, on the cards and in the statistics (40 characters; a string or one per language). Default: the plugin's name. |
| `library.opens` | The game clients the library's games may be handed to, by name: `itch`, `amazon`, `heroic`. Omit it when the games open web pages only, which need no entry. |
| `cacheHours` | How long an answer of `gameDetails` is reused (1–2160, default 168). An answer of `home` is reused for the same time, but for 24 hours at most. |
| `commands` | Up to 8 entries for the Plugins menu; each `id` is a function under `commands` in `main.js`. |
| `settings` | Up to 12 values the user edits in *Settings → Plugins*. Types: `text`, `secret` (masked), `toggle` (`"true"`/`"false"`), `select`. |

A plugin must provide `gameDetails`, `home`, or at least one command; any mix of the three is
fine. A wrong manifest is refused with a message that names the field to fix.

## `main.js`

The file is evaluated in strict mode, then Kanso calls the hook it needs. Register the hooks once:

```js
kanso.plugin({
  // Called when a game's sheet opens (and in the background if "background" is set).
  // Return null/undefined when you have nothing to say about this game.
  gameDetails(game) {
    const data = kanso.http.getJson("https://api.example.org/games?title=" + encodeURIComponent(game.title));
    if (!data.found) return null;
    return {
      badge: { text: data.score + "%", tooltip: "Example score" },
      sections: [
        {
          title: kanso.t({ en: "Example", it: "Esempio" }),
          blocks: [
            { type: "stats", items: [{ label: "Score", value: data.score + "%" }] },
            { type: "links", items: [{ label: "Open", url: "https://example.org/g/" + data.id }] },
          ],
        },
      ],
    };
  },

  // Called when the Home page is shown, if "home" is set. `context.games` is the visible
  // library, `context.now` the time (ms). Return null/undefined when there is nothing to show
  // today: then nothing is drawn.
  home(context) {
    const waiting = context.games.filter((game) => game.status === "backlog");
    if (waiting.length === 0) return null;
    return {
      title: kanso.t({ en: "Waiting for you", it: "Ti aspettano" }),
      gamesStyle: "cards",
      games: waiting.slice(0, 6).map((game) => ({ id: game.id, note: game.genres[0] })),
    };
  },

  commands: {
    // Called from the Plugins menu, with the same `context` as `home`.
    pick(context) {
      const game = context.games[Math.floor(Math.random() * context.games.length)];
      return { title: "Try this", games: [{ id: game.id, note: "Why not?" }] };
    },
  },
});
```

Hooks may be `async` functions; requests themselves are synchronous (`kanso.http.get` returns
when the answer arrived), so plain sequential code is all you need.

### The game object

```js
{
  id: "steam:990080",        // stable id inside Kanso
  title: "Hogwarts Legacy",
  cleanTitle: "Hogwarts Legacy",   // without edition suffixes and ® ™, better for searches
  platform: "steam",         // steam, epic, gog, ea, ubisoft, xbox, Battle.net, standalone,
                             // or emulator for a game file run by one of the user's emulators
  steamAppId: "990080",      // Steam store page: the game's own, or the one matched to a game
                             // from another store; null when there is none
  description: "…",          // may be null
  genres: ["Azione", "GDR"], // in the interface language
  tags: ["Mondo aperto"],    // Steam community tags
  isFavorite: false,
  installed: true,           // false for a game in the library that is not on disk
  status: "playing",         // backlog, playing, finished, abandoned, or null
  playtimeMinutes: 754,      // may be null
  lastPlayedAt: 1731000000000, // ms, may be null
  playCount: 12,             // launches from Kanso
  achievements: { unlocked: 31, total: 128 }  // or null
}
```

Folders, executables and anything else about the PC are deliberately not included.

### The `kanso` object

| | |
|---|---|
| `kanso.plugin(hooks)` | Registers `gameDetails`, `home`, `library` and/or `commands`. |
| `kanso.http.get(url, options)` / `post(url, options)` / `request(options)` | Returns `{ status, ok, headers, body, json() }`. Options: `headers`, `body` (string) or `json` (sent as JSON), and for `request` also `method` and `url`. A 4xx/5xx is a normal answer (`ok` is `false`); a refused or failed request throws. Redirects are **not** followed: you get the 3xx and its `location` header and may follow it yourself, if that host is declared. |
| `kanso.http.getJson(url, options)` | `get` + `json()`, throws on a non-2xx status. |
| `kanso.storage.get(key, fallback)` / `set(key, value)` / `remove(key)` | Small persistent notes of the plugin (tokens, ids already looked up). JSON values, at most 64 keys and 512 kB in total; saved when the call ends successfully. |
| `kanso.settings` | The user's values for the declared settings, as strings. |
| `kanso.locale` | Interface language: `it`, `en`, `es` or `ru`. |
| `kanso.t(texts)` | Picks the text for `kanso.locale` from `{ en: "…", it: "…" }`. |
| `kanso.log(...)` / `console.log(...)` | Diagnostic lines. When a call fails they are shown under *Technical details* in the plugin's section, together with the error and where it happened. |
| `kanso.apiVersion`, `kanso.appVersion` | `1`, and Kanso's version. |

Requests are refused (the call throws) when the address is not `https`, its host is not listed in
`hosts`, it carries credentials or a non-default port, or it sets connection headers such as `Host`.

### What a hook returns

`gameDetails` returns `{ badge?, sections?, cacheHours? }`; a command returns
`{ title?, sections?, games?, gamesStyle? }`; `home` returns the same as a command, plus
`cacheHours?`.

- **`cacheHours`** (`gameDetails` and `home`): keep *this* answer for less than the manifest's
  `cacheHours` — for a game whose data is not out yet, or when the service could only be reached
  in part. It can shorten the time, never extend it.
- **`title`** (commands and `home`): the title of the dialog, or the heading of the part of Home.
  Without it the command's title, or the plugin's name, stands in.

- **`badge`**: `{ text, label?, tooltip? }` — `text` is the figure, at most 14 characters, shown
  on the game card. `label` (at most 24 characters) says what the figure is — "To finish",
  "Steam rating" — and is written next to it where the card has room for lines; without it the
  plugin's name stands in. `tooltip` is the longer explanation.
- **`games`** (commands and `home`): up to 12 `{ id, note?, hint? }` entries pointing at library
  games. `note` is one line about the game ("From your backlog · never played"). An id that is
  not in the visible library is dropped.
- **`gamesStyle`** (commands and `home`): how those games are laid out. One of three names;
  anything else is read as `list`. All three work on Home as they do in a command's dialog.
  - `list` (default): rows with a small cover, the note and *Open sheet*, plus *Play* when the
    game is installed or *Install* when its store can install it (otherwise *Open sheet* alone).
  - `cards`: whole covers, as in the library, with the note and the same buttons on them.
  - `mystery`: whole covers dealt face down. Each shows a question mark and its `hint` (at most
    40 characters: say what kind of suggestion it is, not which game) until the user turns it
    over; *Reveal all* turns the rest. Until then neither the title nor the cover is shown or
    read out. Running the command again deals new covered cards.
- **`sections`**: up to 6 `{ title?, blocks }` (up to 2 from `home`), each with up to 12 blocks
  of these kinds:

| Block | Shape |
|---|---|
| `stats` | `{ type: "stats", items: [{ label, value, hint? }] }` — small label/value tiles |
| `text` | `{ type: "text", text }` — a paragraph of plain text |
| `list` | `{ type: "list", items: [{ title, subtitle?, url? }] }` — rows, optionally links |
| `links` | `{ type: "links", items: [{ label, url }] }` — buttons that open the browser |
| `meter` | `{ type: "meter", label, value, max, valueLabel? }` — a bar filled to `value / max` |
| `notice` | `{ type: "notice", text, tone? }` — a hint; `tone` is `info` or `warning` |

Everything is plain text: there is no HTML, Markdown or styling. Texts longer than the limits are
cut, unknown blocks are dropped, and links must be `https` (they open in the user's browser, which
shows the host in the tooltip first).

### A part of Home

With `"provides": { "home": true }` the plugin gets a part of the Home page of its own: a heading
(the `title` it returns, or its name), a small button that asks it again, its `sections` drawn
with the blocks above, and its `games`, each with *Open sheet* and, where it applies, *Play*
(the game is installed) or *Install* (its store can install it). The official
[`daily-pick`](../plugins/daily-pick/) is some hundred lines and shows the whole of it.

- **What it gets.** `home(context)` is called with what a command gets: `context.games` (the
  visible library, as game objects) and `context.now` (ms). Take the time from `context.now`.
- **What it returns.** The shape of a command's answer, cut to what one part among several may
  take: at most **2 sections** and **12 games**; what is over is dropped, like every other limit.
  On Home `cards` and `mystery` covers are as wide as those of the page's own shelves and wrap
  under them; `list` rows sit side by side.
- **Nothing to show.** Return `null` (or a view with no sections and no games in the library):
  the part is not drawn at all, heading included, until the plugin has something again.
- **When it runs.** Only while Home is the page shown, never at startup on another page,
  and only if the theme has a place for plugins there. Each plugin is asked on its own: a slow
  one does not hold back the page or the other plugins. Nothing is drawn for a plugin until it
  has answered. While Home stays the page shown the plugins are asked again every quarter of
  an hour, and when the window comes back. This goes on while the window is minimized or
  waits in the tray (for instance while a game runs), so that Home is up to date when the user
  returns. An answer still within its `cacheHours` comes from the cache, so the plugin itself
  runs, and contacts its hosts, at most once per `cacheHours` for as long as Kanso is left on
  Home.
- **Who decides.** The user switches the plugin on in *Settings → Plugins*, and from then on
  finds its part listed in *Settings → Home* next to Kanso's own, where it can be left out
  without switching the plugin off. The theme decides where on the page the parts of plugins
  go, or that they are not shown: the name `plugins` in its `home.sections` (see
  [THEMES.md](THEMES.md#home--la-pagina-home)). The parts of several plugins follow one another
  there, in the order of the plugins in Settings. A plugin cannot choose its place, its size or
  its colors.

### A library of games

With `"provides": { "library": true }` the plugin brings games of its own into the library: the
games of a store Kanso does not read by itself, of a cloud service, of a page of links. They are
filed under a platform of their own (named by `library.name`), next to Steam, Epic and the
others, and from there on they are games like any other: status, favourites, genres, covers,
statistics, the Home shelves, the sheets other plugins fill in. The official
[`itch-library`](../plugins/itch-library/) is the example to copy.

```json
"provides": { "library": true },
"library": { "name": "My store", "opens": ["itch"] }
```

```js
kanso.plugin({
  library(context) {
    const owned = kanso.http.getJson("https://api.example.org/owned?key=" + kanso.settings.apiKey);
    return {
      games: owned.map((game) => ({
        id: game.id,                                   // required
        title: game.name,                              // required
        launch: "https://play.example.org/" + game.id, // what Play opens
        installed: true,
        genres: game.genres,
        description: game.summary,
        playtimeMinutes: game.minutes,
        lastPlayedAt: game.lastPlayed,                 // ms
        cover: game.coverUrl,
        steamAppId: game.steamId,
      })),
    };
  },
});
```

- **What it gets.** `library(context)` is called with `context.now` (ms) and nothing else: a
  library is read from its service, not from the user's games.
- **What it returns.** `{ games: [...] }`, or the list alone; at most **3000 games**.

  | Field | |
  |---|---|
  | `id` | Required. The game's id inside the plugin: 1–64 letters, digits, `.`, `_`, `-` (a number is fine). It must stay the same from one call to the next: it is what the user's status and favourites are attached to. |
  | `title` | Required (200 characters). |
  | `launch` | The one address the game can open: what *Play* opens when the game is `installed`, and what *Install* opens when it is not (the page or the client where it is got). See below for what is allowed. Without it the game is listed and cannot be started from Kanso. |
  | `installed` | Whether the game can be started right now. Default `true`. A plugin cannot see the disk: for a game started through a client that also installs it, say `true` and let the client do the rest. |
  | `genres` | Up to 8 short names. Used when the game has none yet. |
  | `description` | A few lines of plain text (600 characters). Used when the game has none yet. |
  | `playtimeMinutes`, `lastPlayedAt` | What the service knows; Kanso keeps the larger figure and the later date. A date in the future is ignored. |
  | `cover` | A picture of the cover: plain `https`, on one of the plugin's declared `hosts`. It fills an empty place; a cover Kanso found or the user chose is never replaced. |
  | `steamAppId` | The game's id on the Steam store, when the service knows it: the plugins that read Steam (reviews, news) then work for it. |

- **What a game can open.** Only two kinds of address, checked by Kanso every time before
  opening one:
  - a web page: plain `https`, a real host, no credentials in it (the same rule as the links a
    plugin shows);
  - an address of a game client named in `library.opens`. The clients are a closed list:
    `itch` (`itch://…`), `amazon` (`amazon-games://…`), `heroic` (`heroic://…`).

  Nothing else is opened, ever: no file, no program, no other protocol of the system. Only the
  characters of an address are accepted (no spaces, quotes or anything else). An address that
  is not allowed is dropped and its game is still listed, without *Play*. The stores Kanso
  reads by itself (Steam, Epic, GOG, …) are not in the list on purpose. If your store's client
  is missing, open an issue: adding one is a line in `plugins/library.rs`, after a look at what
  its addresses can do.
- **When it runs.** At every scan of the library (at startup, and when the user presses
  *Scan*), and at once when the plugin is switched on or its settings change. The answer is
  reused for `cacheHours`, **capped at 24 hours**; a scan the user asked for always asks again.
- **When something goes wrong, throw.** An `Error` keeps the games already in the library as
  they are and shows the user a line with your message. Returning an empty list (or `null`)
  says the account has no games, and the ones in the library leave: never answer that way for
  a missing key or a service that is down.
- **What stays the user's.** A game no longer listed leaves the library, and so do all the
  games of a plugin that is switched off or removed, unless the user gave the game something
  (a status, a favourite, genres, hours): then it stays, as not installed and with nothing to
  open, and comes back as it was if the plugin lists it again. A title edited by hand is kept.
- **What the user sees first.** *Settings → Plugins* says, next to the sites the plugin
  contacts, that it brings a library, what it is called and what its games can open. In a
  game's sheet, *Files and launch* shows the address *Play* opens, as it is.

## Caching and errors

- An answer of `gameDetails` is stored per game and reused for `cacheHours`. It is asked again when
  it expires, when the user presses the refresh icon of the section, and whenever the plugin's
  code, version, settings or the interface language change.
- An answer of `home` is stored once per plugin and reused for `cacheHours`, **capped at 24
  hours** (Home is about today): a plugin whose content follows the day should set a small
  `cacheHours` in its manifest, as `daily-pick` does. It is asked again under the same
  conditions as `gameDetails`; the refresh icon is next to the part's heading. The games a
  cached answer points at are checked against the library every time it is shown.
- When a call fails, the last good answer stays on screen with a note; with nothing cached the
  section (on Home: one line with the plugin's name) shows the error and *Try again*, with the
  plugin's log lines under *Technical details*. Either way a failure is not retried for 30
  minutes unless the user presses the refresh icon (an older good answer is served from the
  cache meanwhile; on Home the note and the log lines stay until a new attempt answers, as
  long as the app is not closed), and in the background a plugin that fails three times in a
  row is left alone until the next pass.
- Return `null` when a game simply has no data: that answer is cached too, so the service is not
  asked again at every opening.
- An answer of `library` is stored once per plugin and reused for `cacheHours`, capped at 24
  hours. When the call fails the last good list is used, so a service that is down takes no
  game away, and the error is shown as a warning above the library.

## Being a good guest

- Declare only the hosts you need, and ask for an API key through a `secret` setting instead of
  shipping one.
- Use `kanso.storage` to remember what does not change (ids, endpoints), so a game costs one request.
- Set `background` only if the service tolerates one request every few seconds for a whole library.
- Services without an official API change without notice: fail with a clear `Error` message, and
  keep the plugin small enough to fix quickly.

## For developers of Kanso

The implementation is in `src-tauri/crates/kanso-core/src/plugins/`: `manifest.rs` (validation),
`http.rs` (the request gate), `runtime.rs` + `prelude.js` (the sandbox and the `kanso` object),
`output.rs` (the block types), `library.rs` (the games a plugin's library brings: what is let
through, the closed list of game clients, how the games join the library), `registry.rs`
(shipped and user plugins) and `mod.rs` (caching, for the game sheets, for Home and for the
libraries). The app's side is `src-tauri/src/plugins.rs`: `sync_libraries` brings the libraries
up to date, and `check_library_game` is asked again before any address a plugin gave is opened.
`bundled_tests.rs` runs every official plugin in the real sandbox against canned answers of its
service: add a test there when you add or change one. The manifest and the `main.js` example of this
guide are read by the tests too (`the_manifest_in_the_plugin_guide_is_valid`,
`the_example_in_the_plugin_guide_runs_as_written`), so they cannot drift from the code.
