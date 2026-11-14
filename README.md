# Kanso

Minimalist, fast Windows game launcher built with **Tauri v2**, **React**, **TypeScript**, **Tailwind CSS**, and **Rust**.

## Architecture

- **`src/`** — React + TypeScript + Tailwind CSS frontend with customizable OLED dark theme tokens in `src/theme.css`:
  - 2:3 cover grid (or compact list), instant search, game detail view
  - Collapsible sidebar with combinable quick filters: favorites, platform, and genres generated from metadata
- **`src-tauri/crates/kanso-core/`** — Pure Rust core library containing:
  - Steam scanner (`HKCU\Software\Valve\Steam`, `libraryfolders.vdf`, `appmanifest_*.acf`), including manifests where Steam left an `appid_<id>` placeholder instead of the game name: the title falls back to the install folder and is replaced by the official store name during metadata sync
  - Epic Games scanner (`C:\ProgramData\Epic\EpicGamesLauncher\Data\Manifests\*.item`)
  - Battle.net scanner (`HKLM\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall`)
  - Custom / Standalone `.exe` folder scanner with service binary exclusion filters
  - SteamGridDB v2 (`GET /games/steam/{appid}` exact match + `/search/autocomplete` fallback & manual search) & Steam Store API metadata client
  - Title cleaner (`GOTY`, `Definitive Edition`, `Remastered`, `Director's Cut`, `™`/`®`)
  - Local `AppData` configuration & cover art cache (`config.json`, `library.json`, `covers/`)
- **`src-tauri/`** — Tauri v2 desktop shell, IPC commands, `tauri-plugin-sql` (`sqlite:kanso.db`), `tauri-plugin-dialog`, and `tauri-plugin-opener`.
  - System tray icon ("Apri Kanso" / "Esci"); with the *hide to tray on launch* setting the window disappears into the tray when a game starts
  - `tauri-plugin-single-instance`: starting Kanso again while it sits in the tray just brings the running window back
- **`.github/workflows/build-windows.yml`** — GitHub Actions CI/CD workflow building `.exe` and `.msi` Windows artifacts on `windows-latest`.

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
