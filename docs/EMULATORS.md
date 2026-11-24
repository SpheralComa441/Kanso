# Emulatori

Kanso ha una sezione **Emulatori**: una scheda per ogni emulatore presente sul PC e, aprendola,
l'elenco dei giochi di quell'emulatore. Un gioco emulato si comporta come ogni altro (copertina,
scheda, preferito, stato, tempo di gioco) e si avvia nel suo emulatore, ma resta nella sua
sezione: non compare in *Tutti i giochi* o nei tag della libreria, mentre compare tra i *Preferiti*,
negli stati, in *Nascosti* e nella scheda *Emulatori* di *Generi*.

## Che cosa fa Kanso, e che cosa no

- **Avvia l'emulatore passandogli il file del gioco.** Nient'altro: gli argomenti sono passati
  al programma uno per uno, senza una shell in mezzo, quindi il nome di un file non può essere
  letto come un comando.
- **Legge le impostazioni dell'emulatore, non le scrive mai.** Da lì prende solo l'elenco delle
  cartelle dei giochi.
- **Non scarica nulla**: né giochi, né BIOS, né core, né firmware. Non contatta nessun sito per
  conto degli emulatori. I file dei giochi sono quelli che hai già sul disco.
- Copertine e descrizioni arrivano come per gli altri giochi (SteamGridDB con la tua chiave,
  pagina di Steam con lo stesso titolo): il titolo cercato è ricavato dal nome del file.

## Come vengono trovati gli emulatori

Gli emulatori quasi mai si installano: sono archivi scompattati dove capita, in cartelle con il
nome che ciascuno preferisce (`D:\pcsx2 PS2 Emulator`). Kanso quindi non li aspetta in un posto
preciso: raccoglie candidati da più parti e tiene gli eseguibili il cui nome di file è quello di
un emulatore che conosce.

1. **Aggiunti da te** in *Impostazioni → Emulatori → Aggiungi emulatore…*: hanno la precedenza.
2. **Programmi installati**: le cartelle che gli installer usano e quelle dichiarate
   nell'elenco *App installate* di Windows.
3. **Programmi avviati**: Windows tiene nel registro dell'utente due elenchi dei programmi che
   sono stati avviati (`MuiCache` e `Compatibility Assistant\Store`). Kanso ne legge solo i
   percorsi e tiene quelli che sono emulatori noti: il resto dell'elenco non viene conservato
   né inviato da nessuna parte.
4. **Ricerca sui dischi**: i dischi fissi e le cartelle Desktop, Documenti e Download, fino a
   quattro livelli sotto la radice, un livello alla volta (prima ciò che è vicino alla radice
   di ogni disco, poi più in profondità) e aprendo al massimo 12.000 cartelle. Sono saltate le
   cartelle di sistema, le librerie degli store (`steamapps`, `XboxGames`, `WindowsApps`) e
   quelle nascoste; `Program Files` è guardata solo per due livelli. Una cartella che contiene
   un emulatore non viene esplorata oltre.
5. **Nelle tue cartelle di giochi** (*Impostazioni → Cartelle giochi personalizzate*): un
   emulatore trovato lì è un emulatore, non un gioco.
6. **Ciò che le ricerche precedenti hanno trovato**, ricordato in `emulators.json` nella
   cartella dati di Kanso (solo un elenco di percorsi).

La ricerca sui dischi è quella lenta: parte alla prima scansione, quando premi *Scansiona* e
con *Cerca sul PC* nella sezione Emulatori, non a ogni avvio. In tutti i casi Kanso legge solo
nomi di file e cartelle: non apre, non modifica e non invia nulla.

## Come vengono trovati i giochi

Per gli emulatori che elencano le proprie cartelle di giochi, Kanso legge quell'elenco. In più
puoi aggiungere cartelle da *Impostazioni → Emulatori*. Nelle cartelle (fino a quattro livelli)
conta i file con le estensioni dell'emulatore, una voce per gioco:

- un gioco su più dischi è la sua playlist `.m3u` se c'è, altrimenti il primo disco;
- un'immagine descritta da un foglio (`.cue`, `.gdi`, `.ccd`, `.mds`) è il foglio, non i file
  `.bin`/`.img` accanto;
- file e cartelle nascosti sono ignorati; al massimo 5000 giochi per emulatore.

Il titolo viene dal nome del file: spariscono regione, revisione e sigle tra parentesi, l'articolo
spostato in fondo torna davanti, il trattino diventa due punti. `Legend of Zelda, The - Ocarina of
Time (USA) (Rev 2) [!].z64` diventa *The Legend of Zelda: Ocarina of Time*. Dalla scheda del
gioco puoi cambiare titolo e copertina come per ogni altro.

Alcuni giochi non hanno un file con il loro nome: ogni gioco PlayStation è un `eboot.bin`, ogni
gioco Xbox 360 estratto un `default.xex`. Per i file di avvio (`eboot`, `default`, `boot`, `main`)
il titolo è quello scritto nel file dei titoli del gioco (`sce_sys\param.sfo` o `PARAM.SFO`: un
file di pochi kilobyte, da cui Kanso legge solo il nome) oppure, se manca, il nome della cartella.

Se una scansione dà a un gioco emulato un titolo diverso da prima, copertina e descrizione trovate
con il vecchio titolo vengono cercate di nuovo: erano di un altro gioco. Un gioco che hai
modificato a mano resta com'è.

## Emulatori riconosciuti

File di impostazioni, sezioni, chiavi e argomenti sono stati controllati sul codice sorgente di
ogni emulatore e sulle definizioni di Playnite ed EmulationStation-DE. «Portatile» indica quando
l'emulatore tiene le impostazioni accanto all'eseguibile.

| Emulatore | Sistemi | Cartelle dei giochi lette da | Avvio |
|---|---|---|---|
| RetroArch | più sistemi | le playlist `playlists\*.lpl` (o la cartella in `playlist_directory` di `retroarch.cfg`) | `-L <core> <gioco>` |
| Dolphin | GameCube, Wii | `Dolphin.ini`, `[General]` `ISOPath0…`, in `%APPDATA%\Dolphin Emulator\Config` (o `Documenti\Dolphin Emulator`; portatile con `portable.txt`: `User\Config`) | `-b -e <gioco>` |
| PCSX2 (Qt) | PlayStation 2 | `inis\PCSX2.ini`, `[GameList]` `Paths` / `RecursivePaths`, in `Documenti\PCSX2` (portatile con `portable.ini` o `portable.txt`) | `-batch -- <gioco>` |
| DuckStation | PlayStation | `settings.ini`, `[GameList]`, in `%LOCALAPPDATA%\DuckStation` (o `Documenti\DuckStation`; portatile con `portable.txt` o `settings.ini`) | `-batch -- <gioco>` |
| RPCS3 | PlayStation 3 | `config\games.yml`: l'elenco dei giochi per sigla; il titolo da `PARAM.SFO` | `--no-gui <EBOOT.BIN>` |
| shadPS4 | PlayStation 4 | `config.json`, `General` → `install_dirs` (le cartelle spente in shadPS4 sono saltate); prima della 0.16 `config.toml`, `[GUI]` `installDirs`. In `user\` accanto all'eseguibile, altrimenti in `%APPDATA%\shadPS4`. Il gioco è ogni cartella con un `eboot.bin`; il titolo da `sce_sys\param.sfo` | `-g <eboot.bin>`; con il launcher `-d -g <eboot.bin>` |
| PPSSPP | PSP | `ppsspp.ini`, `[PinnedPaths]` e `[General] CurrentDirectory`, in `memstick\PSP\SYSTEM` (o dove dice `installed.txt`) | `<gioco>` |
| Cemu | Wii U | `settings.xml`, `<GamePaths>`, in `%APPDATA%\Cemu` (o accanto all'eseguibile, o in `portable\`) | `-g <gioco>` |
| Ryujinx | Switch | `Config.json`, `game_dirs`, in `%APPDATA%\Ryujinx` (o accanto all'eseguibile, o in `portable\`) | `<gioco>` |
| yuzu, suyu, Eden, Citron, Sudachi | Switch | `config\qt-config.ini`, `[UI]` `Paths\gamedirs\N\path`, in `%APPDATA%\<nome>` (portatile: `user\`) | `-g <gioco>` |
| Azahar, Lime3DS, Citra | Nintendo 3DS | come sopra, in `%APPDATA%\Azahar` (anche `Lime3DS`, `Citra`) | `<gioco>` |
| xemu | Xbox | `xemu.toml`, `[general] games_dir` (accanto all'eseguibile o in `%APPDATA%\xemu\xemu`) | `-dvd_path <gioco>` |
| Snes9x | Super Nintendo | `snes9x.conf`, `[Settings\Win\Files]` `Dir:Roms` | `<gioco>` |
| Flycast | Dreamcast, NAOMI | `emu.cfg`, `[config]` `Dreamcast.ContentPath` | `<gioco>` |
| melonDS | Nintendo DS | `melonDS.toml`, `LastROMFolder`: la cartella da cui hai aperto l'ultimo gioco | `<gioco>` |
| mGBA | Game Boy, Game Boy Advance | `qt.ini`, `[mru]`: le cartelle dei giochi aperti di recente (in `%APPDATA%\mGBA`, o accanto all'eseguibile con `portable.ini`) | `<gioco>` |
| Xenia, Xenia Canary | Xbox 360 | — (aggiungi una cartella) | `<gioco>` |
| Mesen | NES, SNES, Game Boy, PC Engine, Master System | — (aggiungi una cartella) | `<gioco>` |

Note:

- **RetroArch**: ogni gioco parte con il *core* che la playlist gli assegna. Finché un gioco non
  è stato avviato una volta da RetroArch la playlist dice `DETECT`: quei giochi non vengono
  elencati, e la scansione avvisa di quanti sono.
- **shadPS4** esiste in due programmi. `shadPS4.exe` è l'emulatore: da qualche versione non ha
  più una finestra propria, e aperto senza un gioco mostra solo un avviso. `shadPS4QtLauncher.exe`
  è il launcher con la finestra, che avvia l'emulatore. Se Kanso li trova entrambi usa il
  launcher, così *Apri shadPS4* apre la finestra; per avviare un gioco gli passa `-d`, che vuol
  dire «la versione dell'emulatore scelta nel launcher»: nel launcher deve essercene una
  selezionata. Le cartelle `…-UPDATE`, `…-patch` e `…-mods` accanto a un gioco sono suoi
  aggiornamenti e mod, non altri giochi. I giochi in archivi `.zar` non sono elencati.
- **Una cartella che è un intero disco** (per esempio l'ultima cartella sfogliata in PPSSPP)
  non viene letta.
- **Argomenti di avvio**: in *Impostazioni → Emulatori* puoi scriverne di tuoi per ogni
  emulatore (per esempio aggiungere lo schermo intero). `{rom}` è il file del gioco, `{core}` il
  core di RetroArch.
- **Tempo di gioco**: un gioco emulato non è un processo a sé. Kanso conta il tempo finché
  resta aperto l'emulatore che ha avviato per quel gioco, e smette quando l'emulatore si chiude.

## Non ancora supportati

Emulatori che non avviano un file ma un nome o un titolo installato, e che quindi richiedono
una lettura a parte: **MAME** (nomi dei set), **ScummVM** (giochi aggiunti nel programma),
**Vita3K** (titoli installati). Si possono aggiungere come emulatore «non riconosciuto» solo
dove basta passare un file.

Un emulatore aggiunto a mano come «non riconosciuto» diventa da solo quello riconosciuto quando
una versione successiva di Kanso impara a conoscerne l'eseguibile: restano le cartelle e gli
argomenti che avevi scritto, nome ed estensioni diventano quelli dell'emulatore.

## Dove sta nel codice

- `src-tauri/crates/kanso-core/src/emulators/mod.rs`: l'elenco degli emulatori, come si trovano
  e come si avvia un gioco.
- `…/emulators/find.rs`: la ricerca degli emulatori sul PC.
- `…/emulators/own_settings.rs`: la lettura delle cartelle dei giochi dalle impostazioni di
  ciascun emulatore.
- `…/emulators/roms.rs`: quali file sono giochi e come si chiamano.
- `…/emulators/special.rs`: le playlist di RetroArch, l'elenco di RPCS3 e le cartelle dei
  giochi PlayStation 4.
- `src-tauri/src/emulators.rs`: i comandi dell'app (elenco, avvio).
- `src/components/EmulatorsView.tsx`, `src/components/EmulatorsSettings.tsx`: l'interfaccia.
