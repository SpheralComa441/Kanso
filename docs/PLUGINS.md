# Plugin di Kanso / Kanso plugins

Un plugin è una cartella con due file, `plugin.json` e `main.js`. Aggiunge informazioni alla scheda
di un gioco (e una piccola etichetta sulle card) oppure comandi al menu **Plugin**.
*A plugin is a folder with two files, `plugin.json` and `main.js`. It adds information to a game's
sheet (and a short label on the cards) or commands to the **Plugins** menu.*

I plugin ufficiali stanno in [`plugins/`](../plugins/) e sono il miglior punto di partenza:
[`play-tonight`](../plugins/play-tonight/) (un comando, niente rete),
[`steam-news`](../plugins/steam-news/) (una richiesta, un elenco),
[`steam-reviews`](../plugins/steam-reviews/) (etichetta sulle card),
[`howlongtobeat`](../plugins/howlongtobeat/) (più richieste, `kanso.storage`),
[`price-history`](../plugins/price-history/) (impostazioni con chiave API).
*The official plugins in [`plugins/`](../plugins/) are the best starting point.*

> The rest of this guide is in English: it is the reference for plugin authors.

## How a plugin runs

- Plugin code does **not** run in Kanso's window. It runs in a small JavaScript interpreter
  (QuickJS) embedded in the app, started fresh for every call and thrown away afterwards.
- It has no DOM, no `fetch`, no file system, no timers, no access to Kanso's settings or API keys.
  It gets one global, `kanso`, described below.
- It may contact **only** the hosts listed in its `plugin.json`, over `https`. The list is shown to
  the user before installing and in *Settings → Plugins*.
- It never draws anything itself: it returns plain data (sections made of a few kinds of blocks)
  and Kanso draws them with its own components and the user's theme.
- Each call is limited: 30 s in total, 64 MB of memory, 48 requests, 4 MB per response.

A plugin that fails, loops or is hostile can at worst show an error in its own section.

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
  "provides": { "gameDetails": true, "badge": true, "background": true },
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
| `cacheHours` | How long an answer of `gameDetails` is reused (1–2160, default 168). |
| `commands` | Up to 8 entries for the Plugins menu; each `id` is a function under `commands` in `main.js`. |
| `settings` | Up to 12 values the user edits in *Settings → Plugins*. Types: `text`, `secret` (masked), `toggle` (`"true"`/`"false"`), `select`. |

A plugin must provide `gameDetails`, at least one command, or both. A wrong manifest is refused
with a message that names the field to fix.

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

  commands: {
    // Called from the Plugins menu. `context.games` is the visible library, `context.now` the time (ms).
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
  platform: "steam",         // steam, epic, gog, ea, ubisoft, xbox, Battle.net, standalone
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
| `kanso.plugin(hooks)` | Registers `gameDetails` and/or `commands`. |
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
`{ title?, sections?, games?, gamesStyle? }`.

- **`cacheHours`** (`gameDetails` only): keep *this* answer for less than the manifest's
  `cacheHours` — for a game whose data is not out yet, or when the service could only be reached
  in part. It can shorten the time, never extend it.

- **`badge`**: `{ text, label?, tooltip? }` — `text` is the figure, at most 14 characters, shown
  on the game card. `label` (at most 24 characters) says what the figure is — "To finish",
  "Steam rating" — and is written next to it where the card has room for lines; without it the
  plugin's name stands in. `tooltip` is the longer explanation.
- **`games`** (commands only): up to 12 `{ id, note?, hint? }` entries pointing at library games.
  `note` is one line about the game ("From your backlog · never played").
- **`gamesStyle`** (commands only): how those games are laid out. One of three names; anything
  else is read as `list`.
  - `list` (default): rows with a small cover, the note and *Open sheet* / *Play*.
  - `cards`: whole covers, as in the library, with the note and the same buttons on them.
  - `mystery`: whole covers dealt face down. Each shows a question mark and its `hint` (at most
    40 characters: say what kind of suggestion it is, not which game) until the user turns it
    over; *Reveal all* turns the rest. Until then neither the title nor the cover is shown or
    read out. Running the command again deals new covered cards.
- **`sections`**: up to 6 `{ title?, blocks }`, each with up to 12 blocks of these kinds:

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

## Caching and errors

- An answer of `gameDetails` is stored per game and reused for `cacheHours`. It is asked again when
  it expires, when the user presses the refresh icon of the section, and whenever the plugin's
  code, version, settings or the interface language change.
- When a call fails, the last good answer stays on screen with a note; with nothing cached the
  section shows the error and *Try again*. Failures are not retried for 30 minutes in the
  background, and a plugin that fails three times in a row is left alone until the next pass.
- Return `null` when a game simply has no data: that answer is cached too, so the service is not
  asked again at every opening.

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
`output.rs` (the block types), `registry.rs` (shipped and user plugins) and `mod.rs` (caching).
`bundled_tests.rs` runs every official plugin in the real sandbox against canned answers of its
service: add a test there when you add or change one.
