# Kanso

Minimalist, fast Windows game launcher built with **Tauri v2**, **React**, **TypeScript**, **Tailwind CSS**, and **Rust**.

## Architecture

- **`src/`** — React + TypeScript + Tailwind CSS frontend with customizable OLED dark theme tokens in `src/theme.css`:
  - 2:3 cover grid (or compact list), instant search, game detail view
  - **Home** with current discounts and free games from Steam, Epic Games and GOG
  - Interface in Italian, English, Spanish and Russian (`src/i18n/`, one dictionary per language, completeness checked at compile time)
  - **Themes**: built-in ones, a live editor, and import/export of community themes as validated `.json` files — see [`docs/THEMES.md`](docs/THEMES.md)
  - Collapsible sidebar with combinable quick filters: favorites, recently played, hidden, platform, and genres generated from metadata
- **`src-tauri/crates/kanso-core/`** — Pure Rust core library (no GUI dependencies, unit-tested on any OS):
  - Store scanners: Steam, Epic Games, GOG, Ubisoft Connect, EA app, Battle.net, Xbox app / PC Game Pass, plus custom folders of standalone `.exe` games. Registry-based scanners receive already-read entries, so their logic is tested with mocked data. A game reported by several stores is listed once.
  - Steam manifests where Steam left an `appid_<id>` placeholder instead of the game name: the title falls back to the install folder and is replaced by the official store name during metadata sync
  - SteamGridDB v2 (`GET /games/steam/{appid}` exact match + `/search/autocomplete` fallback & manual search) & Steam Store API metadata client, in the interface language. Steam games fall back to Steam's own artwork (`IStoreBrowseService/GetItems`), so they get covers even without a SteamGridDB key.
  - Deals client for the public Steam, Epic and GOG endpoints, and the theme file validator. Incomplete games are retried at most once a day unless the user asks; Steam soundtracks/DLC are hidden automatically.
  - Title cleaner (`GOTY`, `Definitive Edition`, `Remastered`, `Director's Cut`, `™`/`®`)
  - Library database: SQLite via `rusqlite` (`library.db` in the app's local data folder), the single source of truth for games, metadata, favorites, play history and manual edits. `config.json` holds the settings; `covers/` the downloaded cover art.
- **`src-tauri/`** — Tauri v2 desktop shell: IPC commands, `tauri-plugin-dialog`, `tauri-plugin-opener`.
  - System tray icon ("Apri Kanso" / "Esci"); with the *hide to tray on launch* setting the window disappears into the tray when a game starts
  - `tauri-plugin-single-instance`: starting Kanso again while it sits in the tray just brings the running window back
  - Restrictive CSP; the asset protocol can read the `covers/` folder only; games are launched by id from the database
- **`.github/workflows/build-windows.yml`** — CI: a fast Linux job (core tests, clippy, frontend build) followed by the `windows-latest` job building the `.exe` and `.msi` artifacts.

## Development & Testing

```bash
# Install frontend dependencies
npm install

# Run Rust scanner & parser unit tests
npm run test:rust

# Build frontend bundle
npm run build

# Start Tauri desktop app locally (Windows)
npx tauri dev
```

`npm run dev` alone opens a browser preview backed by a small sample library (no scan, no launch).

Set `KANSO_STEAM_ROOT` to a folder containing `steamapps/` to scan it instead of the Steam
installation found in the registry. This also works on Linux/macOS, where it is the only way to
exercise the real scan pipeline end to end:

```bash
KANSO_STEAM_ROOT=/path/to/fake/Steam npx tauri dev
```
