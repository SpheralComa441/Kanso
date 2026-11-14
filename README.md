# Kanso

Minimalist, fast Windows game launcher built with **Tauri v2**, **React**, **TypeScript**, **Tailwind CSS**, and **Rust**.

## Architecture

- **`src/`** — React + TypeScript + Tailwind CSS frontend with customizable OLED dark theme tokens in `src/theme.css`.
- **`src-tauri/crates/kanso-core/`** — Pure Rust core library containing:
  - Steam scanner (`HKCU\Software\Valve\Steam`, `libraryfolders.vdf`, `appmanifest_*.acf`)
  - Epic Games scanner (`C:\ProgramData\Epic\EpicGamesLauncher\Data\Manifests\*.item`)
  - Custom / Standalone `.exe` folder scanner with service binary exclusion filters
  - Title cleaner (`GOTY`, `Definitive Edition`, `Remastered`, `Director's Cut`, `™`/`®`)
  - Local `AppData` configuration manager (`config.json` — no `.env` files committed)
- **`src-tauri/`** — Tauri v2 desktop shell, IPC commands, `tauri-plugin-sql` (`sqlite:kanso.db`), `tauri-plugin-dialog`, and `tauri-plugin-opener`.
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
