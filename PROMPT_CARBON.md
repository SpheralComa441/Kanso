# Specifica Tecnica: Minimalist Windows Game Launcher

## 1. Contesto e Ambiente di Sviluppo
- **Workflow:** Sviluppo tramite agente remoto cloud su repository GitHub. Non hai accesso diretto al filesystem Windows dell'utente finale.
- **Obiettivo:** Creare un'applicazione desktop per Windows leggera, veloce e minimalista che unifichi tutti i videogiochi installati sul PC in un'unica libreria, scaricando automaticamente copertine e metadata.
- **Stack Tecnologico:**
  - **Framework:** Tauri v2 (Frontend in React + TypeScript + Tailwind CSS, Backend in Rust).
  - **Database locale:** SQLite (`tauri-plugin-sql`) salvato nella directory AppData locale di Windows dell'utente.
  - **Build & CI/CD:** Configura fin dallo Step 1 una GitHub Action (`.github/workflows/build-windows.yml`) su runner `windows-latest` che compili il binario Windows (`.exe` / `.msi`) ad ogni push su `main` e lo carichi come Artifact scaricabile.

## 2. Gestione Sicura della API Key
- **Non utilizzare file `.env` committati nel repository.**
- Crea una schermata/modale **Impostazioni** accessibile dall'header dell'app in cui l'utente può incollare la propria **SteamGridDB API Key** (salvata localmente nel database SQLite / `config.json` sul PC dell'utente).
- Se la chiave non è ancora stata inserita, mostra un banner discreto per configurarla, permettendo comunque la scansione e l'avvio dei giochi.

## 3. Funzionalità Core (Priorità)

### A. Auto-Discovery dei Giochi (Backend Rust)
Implementa uno scanner per Windows che rilevi i giochi installati leggendo:
1. **Steam:** Leggi il registro di sistema (`HKCU\Software\Valve\Steam` -> `SteamPath`) e `libraryfolders.vdf` per trovare tutte le librerie, poi parsa i file `appmanifest_*.acf` per estrarre `appid`, `name` e `installdir`.
2. **Epic Games:** Leggi i file `.item` in `C:\ProgramData\Epic\EpicGamesLauncher\Data\Manifests` estraendo `DisplayName`, `InstallLocation` e `LaunchExecutable`.
3. **Cartelle Custom / Standalone:** Permetti all'utente di aggiungere manualmente cartelle dalle Impostazioni. Scansiona gli `.exe` escludendo quelli che contengono `unins`, `crash`, `redist`, `dxsetup`, `vc_redist`, `launcher`.
*Nota per il testing nel cloud:* Scrivi unit test in Rust con file `.acf` e `.item` mockati per verificare che il parser funzioni prima della build su GitHub Actions.

### B. Metadata & Copertine (SteamGridDB + Steam Store API)
1. Pulisci il titolo rilevato rimuovendo suffissi come `GOTY`, `Game of the Year`, `Definitive Edition`, `Remastered`, `Director's Cut` e simboli `™`/`®`.
2. **Copertine (SteamGridDB API):**
   - Usa l'endpoint `GET https://www.steamgriddb.com/api/v2/search/autocomplete/{titolo_pulito}` con header `Authorization: Bearer {SGDB_API_KEY}`.
   - Prendi il primo `id` e scarica la griglia verticale (`dimensions=600x900`) tramite `GET https://www.steamgriddb.com/api/v2/grids/game/{id}?dimensions=600x900`.
   - Salva l'immagine in locale nella cartella `covers/` dentro AppData per il caricamento offline istantaneo.
3. **Descrizioni e Categorie (Steam Store API pubblico - nessuna key richiesta):**
   - Se è un gioco Steam usa il suo `appid`; altrimenti cerca su `https://steamcommunity.com/actions/SearchApps/{titolo_pulito}` per ottenere l'`appid`.
   - Interroga `https://store.steampowered.com/api/appdetails?appids={appid}&l=italian` per estrarre `short_description` e `genres` (es. Azione, GDR, Indie).
4. **Modifica Manuale:** Permetti all'utente di modificare titolo, genere, percorso `.exe` o sostituire la copertina con un file immagine locale.

### C. Organizzazione e Avvio
- Filtri rapidi per **Genere** (generati automaticamente dai metadata), **Piattaforma** (Steam, Epic, Locale) e **Preferiti**.
- Avvio tramite `steam://rungameid/{appid}` per Steam, URI Epic o esecuzione diretta dell'`.exe` per i giochi locali, con opzione di minimizzare l'app nella system tray all'avvio di un gioco.

## 4. Design & UI/UX
- **Estetica:** Tema scuro profondo (`#0f0f11`) ottimizzato per schermi OLED, zero menu superflui, tipografia pulita.
- **Layout:** Griglia di copertine verticali (aspect ratio `2:3`), barra di ricerca istantanea in alto, sidebar minimalista a scomparsa per i filtri.
- **Personalizzazione:** Esponi colori principali, raggio bordi e dimensione minima delle card in un file `src/theme.css` tramite variabili CSS (`--bg-primary`, `--accent-color`, `--card-min-width`).

## 5. Piano di Esecuzione a Step
Procedi un passo alla volta e fermati alla fine di ogni step per attendere la mia conferma:
- **Step 1:** Inizializza il progetto Tauri v2 + React + Tailwind, configura il workflow GitHub Actions (`build-windows.yml`), implementa il parser Rust dei giochi (con unit test su dati mock) e una prima UI grezza che mostri l'elenco rilevato + la schermata Impostazioni per salvare la API key.
- **Step 2:** Integra il client per SteamGridDB e Steam Store API, il salvataggio locale delle immagini in AppData e la cache su SQLite.
- **Step 3:** Rifinisci la UI finale (griglia OLED, vista dettaglio gioco, filtri per categoria, avvio eseguibili e system tray).

Inizia ora dallo **Step 1**.