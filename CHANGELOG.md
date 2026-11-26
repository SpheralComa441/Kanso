# Changelog

What changes from one release of Kanso to the next, most recent first.

## Unreleased

- **Customizable page navigation tabs.** *Home*, *Store*, *News*, *Statistics* and *Emulators*
  now sit by default in a horizontal top bar with muted theme-tinted tabs that light up on hover
  and cross-fade in and out with a soft wave-illuminated frame. Every aspect is customizable in
  `.kanso-theme.json` under `nav`: placement (`top`, `bottom`, `sidebarTop`, `sidebarBottom`) and
  alignment, graphics (`tabStyle`, radius, border width, padding, spacing, bar backdrop,
  icon/label/count toggles), custom colors and resting tint, plus highlight animations (`wave`,
  `pulse`, `shimmer`, `none`), speed, glow, and selection transitions (`fade`, `slide`, `none`).

## 0.3.2

- **Translated user categories with no cross-language duplicates.** Genres added by hand from a
  game's sheet or edited manually in *Edit game* now follow interface language switches across
  Italian, English, Spanish and Russian (covering official Steam genres, aliases and common
  sub-genres such as *Shooter* / *Sparatutto* or *Pixel Art* / *Пиксельная графика*), normalize
  into the active language on input, and deduplicate against store genres across languages while
  keeping personal labels untouched.

## 0.3.1

- **Consistent genres, tags and descriptions across language switches.** Switching the interface
  language back and forth (for example Italian → Russian → Italian, or across English and Spanish)
  cancels any previous-language metadata sync in flight, refreshes installed games in batches
  instead of hitting per-app rate limits, and heals any genres left in another language.

## 0.3.0

- **In-app updates.** Kanso checks GitHub Releases automatically at startup (can be toggled or
  triggered manually in *Settings → General*) and proposes new versions in a dialog styled to
  match the active theme. Choosing *Update now* downloads the matching Windows installer
  directly in-app, checks its SHA-256 sum, runs the upgrade and restarts into the new version.

## 0.2.3

- **Emulated games in Favorites.** Emulated games marked with the star are now counted in
  *Favorites* and listed there alongside the rest of the library.

## 0.2.2

- **Accurate counts on the Genres and Tags tabs.** The platform tabs at the top of *Genres* and
  *Tags* (including *Emulators*) now count only the games that actually have at least one genre
  or tag, leaving out entries without any.

## 0.2.1

- **Emulators on the Genres page.** Next to the stores at the top of *Genres*, an *Emulators*
  tab divides the emulated games by genre and opens them on the chosen genre.

## 0.2.0

- **Genres and tags by platform.** The tiles of genres and of tags have a row of platforms
  above them: pick one and the tiles count that platform's games alone, and a tile opens the
  library on that genre and that platform. While the tiles are open, the platforms of the side
  panel do the same instead of leaving the page. A theme can take the row away
  (`categories.platforms`).
- Requests now name the version of Kanso that makes them; they all said 0.1.0.
- New pages in the repository: [PRIVACY.md](https://github.com/SpheralComa441/Kanso/blob/main/PRIVACY.md)
  lists every request Kanso makes, to whom and when, and the guides say how to share a theme
  or a plugin (the GitHub topics `kanso-theme` and `kanso-plugin`).
- The README says that Kanso needs Microsoft's WebView2, which the installer downloads where
  it is missing.

## 0.1.0

The first public release.

- **One library for every store.** Steam, Epic Games, GOG, Ubisoft Connect, EA app,
  Battle.net and the Xbox app are found on the PC, together with the folders of loose games
  you point Kanso at. A game installed through two stores is listed once.
- **The games you own but have not installed**, read from what the store clients keep on the
  PC (Steam, Epic, GOG Galaxy, Ubisoft Connect) with no sign-in, and, if you choose to sign
  in, from the accounts of Epic, GOG, Ubisoft, EA, Battle.net and Xbox. The Game Pass catalog
  for PC is an option of its own. *Install* hands a game to its store's client. The games
  that reach you through Steam family sharing are marked, installed or not.
- **Covers, descriptions, genres and tags** from SteamGridDB (with your own free key) and the
  Steam store, in the language of the interface.
- **Play time, last session, status and statistics.** Steam's own counters are read from its
  local files; games of other stores are timed by Kanso while they run. Each game can be
  marked as backlog, playing, finished or abandoned. Steam achievements are read from the
  client's local cache.
- **Home**, with what to resume, a queue of what to play next, the most played, suggestions
  by genre and the best discounts, over a pixel-art background that follows the season, the
  holidays and, if you like, the weather.
- **Store**: current discounts and free games of Steam, Epic, GOG and other shops, with the
  ones you like kept aside and an alert when they are about to end.
- **News** about PC games and hardware from the sites' own public feeds.
- **Emulators**: about twenty are recognised, each with the games in the folders its own
  settings list.
- **DLSS**: the versions a game carries, and, on request, an update that keeps the original
  and accepts only libraries signed by NVIDIA.
- **Themes**: eight built in, an editor for colours and fonts, and theme files that can
  restyle and rearrange the whole interface without carrying any code.
- **Plugins**, run in a sandbox: sections in a game's sheet, labels on the covers, commands,
  parts of the Home page and whole libraries of games. Nine official ones are included.
- **Accessibility**: text size, high contrast, reduced motion, legible fonts, stronger focus,
  on top of any theme.
- Italian, English, Spanish and Russian.
