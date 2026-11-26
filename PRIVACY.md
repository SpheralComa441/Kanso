# Privacy

Kanso has no server, no account and no telemetry, so there is nowhere of ours for your data
to go. What it does do is ask other services for things: covers, prices, news. This page
lists every request Kanso makes, to whom, when, and what it carries. Each one goes from your
PC straight to the service named.

As with any program that talks to a website, a request shows that service your IP address
and the name Kanso gives itself: `KansoLauncher/<version>`, or `Kanso/<version>` for the
requests of plugins.

## What Kanso asks for by itself

Without these the library would be a list of names, so they are on from the start.

| What for | To whom | When | What the request carries |
|---|---|---|---|
| Covers, descriptions, genres and tags | Steam's public store (`store.steampowered.com`, `api.steampowered.com`, `steamcommunity.com`) | After a scan, for the games that still lack them | A game's Steam id, or the title of a game from another store to find its page on Steam; the language of the interface |
| Discounts and free games | Steam, Epic Games, GOG and [CheapShark](https://www.cheapshark.com) | When the Home or the Store page shows them | The language of the interface and a country code taken from Windows' language settings, to get prices in the right currency. Nothing about your library |
| News | The public feeds of the news sites left on in the News page | When you open News | Nothing but the request itself |
| Reviews and news of a game (the two official plugins that are on from the start) | Steam (`api.steampowered.com`, `store.steampowered.com`) | When you open a game's sheet; the review score also for the labels on the covers | The game's Steam id and the language |
| Pictures | The image servers of the store or site a picture comes from | When a cover, a screenshot or a news picture is shown | The picture's address |
| App updates | GitHub's public Releases API (`api.github.com/repos/SpheralComa441/Kanso/releases/latest`, and `github.com` / `*.githubusercontent.com` only if you choose to download an update) | Once at startup (can be switched off in *Settings → General*), or when you click *Check now* | Nothing about you or your library: only Kanso's name and version (`User-Agent`) |

## What Kanso asks for only if you turn it on

| What for | To whom | What the request carries |
|---|---|---|
| More covers | [SteamGridDB](https://www.steamgriddb.com), once you enter your own key | A game's Steam id or title, and your key |
| The weather behind the Home page | [Open-Meteo](https://open-meteo.com) | The name you type when you search for a place; afterwards that place's coordinates, rounded to two decimals |
| Newer DLSS libraries | NVIDIA's public repository on GitHub | Nothing about you or your games: the list of versions, then the files |
| The Game Pass catalog for PC | Microsoft's public catalog (`catalog.gamepass.com`, `displaycatalog.mp.microsoft.com`) | A country code and a language |
| The games of a store account | That store's own services, after you sign in on the store's page | The session the store issued to you. Store by store: [docs/ACCOUNTS.md](docs/ACCOUNTS.md) |
| The other official plugins | The site each one names: HowLongToBeat, PCGamingWiki, Nexus Mods, IsThereAnyDeal, itch.io | A game's title or Steam id; for IsThereAnyDeal and itch.io, the key you entered for that service |
| A plugin you install | Only the hosts its manifest declares, which you see before switching it on | Whatever the plugin chooses to send of what Kanso shows it: your games' titles, stores, Steam ids, genres and tags, play time and last session, status, favourites and achievement counts, plus the plugin's own settings. Never folders or files, Kanso's settings or the keys of other services |

Signing in to a store opens the store's own page, in your browser or in a separate window
with a profile of its own. What you type there goes to the store and to nobody else; Kanso
never sees your password or your two-factor codes.

Opening a store page, a news article or any other link hands the address to your browser.

## What stays on your PC

Everything Kanso knows is kept in `%LOCALAPPDATA%\com.kanso.launcher`: your settings and the
keys you entered, the library with your play time, statuses and favourites, the covers it
downloaded, and, if you signed in to a store, the session that store issued (encrypted for
your Windows user). None of it is sent anywhere.

Kanso also reads, without changing them, the files the store clients keep on your PC: that is
how it finds your games, your play time on Steam and your achievements with no sign-in.

Removing Kanso from Windows' *Installed apps* removes the program; the uninstaller of the
`-setup.exe` installer also offers to delete that folder. Otherwise delete it by hand.

## What Kanso does not do

- It does not collect statistics about how you use it, and it sends no crash reports.
- It never downloads or installs an update without asking: automatic update checks can be
  switched off in *Settings → General*, and releases are always available on the
  [Releases page](https://github.com/SpheralComa441/Kanso/releases).
- It shows no advertising and earns nothing from the links it opens. A deal found through
  CheapShark opens through CheapShark's own redirect page, which is how that service asks to
  be linked.

## Checking it yourself

Every address Kanso can reach is written in its source: `src-tauri/crates/kanso-core/src/`
holds the code that talks to each service (`metadata.rs`, `deals.rs`, `news.rs`,
`weather.rs`, `dlss.rs`, `gamepass.rs`, `updater.rs`, one file per store account), and each
official plugin lists its hosts in `plugins/<name>/plugin.json`. The interface itself cannot
make requests: its content security policy allows it to load pictures and nothing else.

Something here does not match what you see Kanso doing? That is a bug worth reporting:
[SECURITY.md](SECURITY.md).
