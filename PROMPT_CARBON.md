# Specifica Tecnica: Minimalist Windows Game Launcher

## 1. Contesto e Ambiente di Sviluppo
- **Workflow:** Sviluppo tramite agente remoto cloud su repository GitHub. Non hai accesso diretto al filesystem Windows dell'utente finale.
- **Obiettivo:** Creare un'applicazione desktop per Windows leggera, veloce e minimalista che unifichi tutti i videogiochi installati sul PC in un'unica libreria, scaricando automaticamente copertine e metadata.
- **Stack Tecnologico:**
  - **Framework:** Tauri v2 (Frontend in React + TypeScript + Tailwind CSS, Backend in Rust).
  - **Database locale:** SQLite gestito interamente dal backend Rust (`rusqlite`, file `library.db`) nella directory AppData locale di Windows dell'utente. È l'unica fonte di verità della libreria: nessuna copia in JSON o lato frontend. *(Step 4: sostituisce `tauri-plugin-sql` + `library.json`.)*
  - **Build & CI/CD:** GitHub Action (`.github/workflows/build-windows.yml`) con due job: `checks` su `ubuntu-latest` (test del core, clippy, build frontend: feedback in pochi minuti) e `build-windows` su `windows-latest`, che compila il binario Windows (`.exe` / `.msi`) ad ogni push su `main` e lo carica come Artifact scaricabile.

## 2. Gestione Sicura della API Key
- **Non utilizzare file `.env` committati nel repository.**
- Crea una schermata/modale **Impostazioni** accessibile dall'header dell'app in cui l'utente può incollare la propria **SteamGridDB API Key** (salvata localmente in `config.json` sul PC dell'utente, con scrittura atomica).
- Se la chiave non è ancora stata inserita, mostra un banner discreto per configurarla, permettendo comunque la scansione e l'avvio dei giochi.

## 3. Funzionalità Core (Priorità)

### A. Auto-Discovery dei Giochi (Backend Rust)
Implementa uno scanner per Windows che rilevi i giochi installati leggendo:
1. **Steam:** Leggi il registro di sistema (`HKCU\Software\Valve\Steam` -> `SteamPath`) e `libraryfolders.vdf` per trovare tutte le librerie, poi parsa i file `appmanifest_*.acf` per estrarre `appid`, `name` e `installdir`. Se `name` è il segnaposto `appid_<id>` che Steam scrive quando non ha ancora in cache le info dell'app, usa provvisoriamente il nome della cartella e risolvi il nome ufficiale durante la sync dei metadata.
2. **Epic Games:** Leggi i file `.item` in `C:\ProgramData\Epic\EpicGamesLauncher\Data\Manifests` estraendo `DisplayName`, `InstallLocation` e `LaunchExecutable`.
3. **GOG:** `HKLM\SOFTWARE\WOW6432Node\GOG.com\Games\<gameID>` (`gameName`, `path`, `exe`; salta i DLC, che hanno `dependsOn`).
4. **Ubisoft Connect:** chiavi `Uninstall\Uplay Install <id>` (`DisplayName`, `InstallLocation`) più `Ubisoft\Launcher\Installs`; avvio con `uplay://launch/<id>/0`.
5. **EA app:** voci `Uninstall` con publisher Electronic Arts installate dall'installer EA (cartella `__Installer` o disinstallatore `EAInstaller`); eseguibile da `__Installer\installerdata.xml`.
6. **Battle.net:** voci `Uninstall` di Blizzard/Activision, escluso il client.
7. **Xbox app / PC Game Pass:** `<drive>:\XboxGames\<Gioco>\Content\MicrosoftGame.config` (cartella indicata da `.GamingRoot`); avvio tramite `gamelaunchhelper.exe` quando presente.
8. **Cartelle Custom / Standalone:** Permetti all'utente di aggiungere manualmente cartelle dalle Impostazioni. Scansiona gli `.exe` escludendo quelli che contengono `unins`, `crash`, `redist`, `dxsetup`, `vc_redist`, `launcher`.

Un gioco compare una sola volta: se uno store successivo segnala una cartella già rilevata da uno precedente (es. un titolo EA o Ubisoft comprato su Steam), la seconda voce viene scartata.

*Nota per il testing nel cloud:* Scrivi unit test in Rust con file `.acf`, `.item`, voci di registro e `MicrosoftGame.config` mockati per verificare che i parser funzionino prima della build su GitHub Actions. La logica degli scanner non deve dipendere dal registro reale: riceve voci già lette, così gira su qualsiasi OS. La variabile `KANSO_STEAM_ROOT` permette di puntare lo scanner Steam a una cartella finta per le prove end-to-end.

### B. Metadata & Copertine (SteamGridDB + Steam Store API)
1. Pulisci il titolo rilevato rimuovendo suffissi come `GOTY`, `Game of the Year`, `Definitive Edition`, `Remastered`, `Director's Cut` e simboli `™`/`®`.
2. **Copertine (SteamGridDB API, poi Steam):**
   - Per i giochi Steam usa prima la corrispondenza esatta `GET https://www.steamgriddb.com/api/v2/games/steam/{appid}`; altrimenti (e per gli altri store) `GET https://www.steamgriddb.com/api/v2/search/autocomplete/{titolo_pulito}` con header `Authorization: Bearer {SGDB_API_KEY}`.
   - Prendi il primo `id` e scarica la griglia verticale (`dimensions=600x900`) tramite `GET https://www.steamgriddb.com/api/v2/grids/game/{id}?dimensions=600x900`.
   - Salva l'immagine in locale nella cartella `covers/` dentro AppData per il caricamento offline istantaneo.
   - **Ripiego ufficiale per i giochi Steam** (unica fonte se manca la API key): chiedi a `https://api.steampowered.com/IStoreBrowseService/GetItems/v1` gli `assets` dell'app e scarica `library_capsule` (verticale) o, in mancanza, `header`. È indispensabile per i giochi recenti: le loro immagini stanno sotto percorsi con hash e il vecchio `.../library_600x900.jpg` risponde 404. Una copertina orizzontale viene mostrata intera su uno sfondo sfocato di sé stessa.
3. **Descrizioni e Categorie (Steam Store API pubblico - nessuna key richiesta):**
   - Se è un gioco Steam usa il suo `appid`; altrimenti cerca su `https://steamcommunity.com/actions/SearchApps/{titolo_pulito}` per ottenere l'`appid`.
   - Interroga `https://store.steampowered.com/api/appdetails?appids={appid}&l={lingua}` (nella lingua dell'interfaccia) per estrarre `name`, `type`, `short_description` e `genres` (es. Azione, GDR, Indie).
4. **Modifica Manuale:** Permetti all'utente di modificare titolo, genere, percorso `.exe` o sostituire la copertina con un file immagine locale.
5. **Cadenza della sync:** un gioco a cui manca ancora qualcosa (copertina, descrizione, nome ufficiale, tipo) viene riprovato in automatico al massimo una volta ogni 24 ore; subito solo se l'utente preme *Scansiona* o inserisce una nuova API key. Un tentativo che non trova nulla non riscrive il gioco né notifica la UI.
6. **Voci che non sono giochi:** le app Steam che lo store classifica come `music`, `dlc`, `video` e simili vengono nascoste automaticamente la prima volta che vengono classificate; l'utente può sempre riportarle in libreria.

### C. Organizzazione e Avvio
- Filtri rapidi combinabili per **Genere** (generati automaticamente dai metadata), **Piattaforma** (solo quelle con giochi installati), **Preferiti**, **Giocati di recente** (ultimo avvio da Kanso, più recenti prima) e **Nascosti**.
- Qualsiasi gioco può essere nascosto o rimostrato dalla vista dettaglio; i giochi nascosti non compaiono in nessun'altra vista né nei conteggi.
- Avvio tramite `steam://rungameid/{appid}` per Steam, URI Epic/Ubisoft o esecuzione diretta dell'`.exe` per gli altri, con opzione di nascondere l'app nella system tray all'avvio di un gioco. Riavviare Kanso mentre è nella tray riporta in primo piano la finestra esistente (istanza singola). Ogni avvio riuscito aggiorna "giocato di recente".

### D. Home con le offerte
- Vista **Home** che elenca le offerte in corso prese dalle API pubbliche (senza chiave) dei negozi: giochi gratis di Epic (`freeGamesPromotions`, attuali e annunciati), sconti di Steam (`featuredcategories`) e sconti di GOG (`catalog`), con prezzi nella valuta del paese dell'utente.
- Clic su un'offerta: apre la pagina del negozio nel browser. Sono ammessi solo indirizzi `https` sui domini dei tre negozi.
- Le offerte Steam di giochi già in libreria sono contrassegnate. Un negozio irraggiungibile non nasconde gli altri; i risultati vengono riusati per 30 minuti.

### E. Lingue
- Interfaccia in **italiano, inglese, spagnolo e russo**, selezionabile dalle Impostazioni; al primo avvio segue la lingua del sistema (inglese se non supportata).
- Tradotti anche i messaggi del backend e il menu della system tray. Descrizioni e generi vengono riscaricati nella nuova lingua quando la si cambia; i testi modificati a mano non vengono toccati.
- Ogni lingua è un dizionario in `src/i18n/`: il compilatore rifiuta un dizionario a cui manca una chiave.

### F. Sicurezza
- Il comando di avvio riceve solo l'id del gioco: percorso ed URI vengono letti dal database, mai dalla webview.
- **CSP** restrittiva (`default-src 'self'`, immagini solo da app/asset/data, nessuno script remoto).
- Il protocollo asset può leggere **solo** la cartella `covers/`.

## 4. Design & UI/UX
- **Estetica:** Tema scuro profondo (`#0f0f11`) ottimizzato per schermi OLED, zero menu superflui, tipografia pulita.
- **Layout:** Griglia di copertine verticali (aspect ratio `2:3`), barra di ricerca istantanea in alto, sidebar minimalista a scomparsa per i filtri, vista dettaglio del gioco.
- **Personalizzazione:** Esponi colori, raggio bordi e dimensione minima delle card in `src/theme.css` tramite variabili CSS (`--bg-primary`, `--accent-color`, `--card-min-width`), inclusi i colori del testo, così che un tema possa essere anche chiaro.
- **Temi della community:** un tema è un file `.json` (formato in `docs/THEMES.md`) con 12 colori, 3 misure e un font. Si crea dall'editor integrato con anteprima dal vivo, si esporta e si importa da file o copiandolo nella cartella `themes/`. Ogni valore viene validato dal backend: un tema non può contenere CSS, script o risorse remote.

## 5. Piano di Esecuzione a Step
Gli Step 1–3 sono il piano originale. Gli Step 4–8 sono stati aggiunti dopo la revisione del progetto a fine Step 3 e approvati in blocco, quindi vengono eseguiti di seguito senza fermarsi tra uno e l'altro.

- [x] **Step 1:** Inizializza il progetto Tauri v2 + React + Tailwind, configura il workflow GitHub Actions (`build-windows.yml`), implementa il parser Rust dei giochi (con unit test su dati mock) e una prima UI grezza che mostri l'elenco rilevato + la schermata Impostazioni per salvare la API key.
- [x] **Step 2:** Integra il client per SteamGridDB e Steam Store API, il salvataggio locale delle immagini in AppData e la cache locale.
- [x] **Step 3:** Rifinisci la UI finale (griglia OLED, vista dettaglio gioco, filtri per categoria, preferiti, avvio eseguibili e system tray). Correzione dei titoli Steam `appid_<id>`.
- [x] **Step 4 — Store SQLite lato Rust:** `library.db` come unica fonte di verità con scritture atomiche; importazione una tantum del vecchio `library.json` (conservato come backup); rimozione di `tauri-plugin-sql` e della copia lato frontend. Un database illeggibile viene messo da parte, mai cancellato.
- [x] **Step 5 — Sync meno insistente:** nuovo tentativo automatico al massimo ogni 24 ore per i giochi incompleti, immediato solo su richiesta dell'utente; nessuna riscrittura quando non cambia nulla.
- [x] **Step 6 — Altri store:** scanner GOG, Ubisoft Connect, EA app e Xbox app, con deduplica tra store per cartella di installazione.
- [x] **Step 7 — Giocati di recente e Nascosti:** storico degli avvii, vista "Giocati di recente", possibilità di nascondere voci, occultamento automatico di colonne sonore e DLC di Steam.
- [x] **Step 8 — Sicurezza e CI:** CSP, protocollo asset limitato alle copertine, job Linux veloce per test e lint prima della build Windows.
- [x] **Step 9 — Copertine ufficiali Steam:** ripiego sulle immagini ufficiali di Steam quando SteamGridDB non ha il gioco o manca la chiave (caso reale: Invokyr e The Old Flesh of Nuraga).
- [x] **Step 10 — Lingue:** italiano, inglese, spagnolo e russo per interfaccia, messaggi del backend, tray e metadata.
- [x] **Step 11 — Temi della community:** temi completi in `.json`, editor con anteprima dal vivo, importazione ed esportazione, temi integrati (OLED, Grafite, Nord, Carta).
- [x] **Step 12 — Home con le offerte:** sconti e giochi gratis da Steam, Epic Games e GOG.

### Da verificare su un PC Windows reale
Non verificabile dall'ambiente cloud, quindi da controllare alla prima prova dell'installer:
- icona nella system tray e ripristino della finestra;
- copertine visibili (CSP + protocollo asset limitato alla cartella `covers/`);
- rilevamento reale di GOG, Ubisoft Connect, EA app e Xbox app: gli scanner sono testati solo su dati simulati;
- nome ufficiale dei giochi `appid_<id>` dallo Steam Store e occultamento automatico delle colonne sonore;
- immagini delle offerte nella Home (arrivano dai CDN dei negozi, ammessi dalla CSP solo come immagini) e apertura della pagina del negozio nel browser;
- finestre di sistema per importare/esportare un tema e apertura della cartella temi.
