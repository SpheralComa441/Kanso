# Temi di Kanso / Kanso themes

Un tema è un singolo file `.json` che ridisegna l'intera interfaccia: colori, font, forma di card,
controlli e icone, disposizione, sfondo, animazioni, la scheda che si apre cliccando un gioco, e
come sono fatti il pannello laterale e le pagine Home, Store e News (che cosa c'è, in che ordine,
a griglia o a elenco).
*A theme is a single `.json` file that restyles the whole interface: colors, fonts, the shape of
cards, controls and icons, layout, background, motion, the sheet that opens when a game is
clicked, and how the side panel and the Home, Store and News pages are made (what is there, in
which order, as a grid or as a list).*

I temi inclusi in Kanso (*Brace*, *Ambra*, *Ciano*, *Viola*, *Menta*, *Inchiostro*) nascono dal
logo per i colori: la stessa ardesia con un colore ciascuno. Hanno forme tonde, copertine senza
didascalia che raccontano il gioco al passaggio del mouse, e animazioni fluide. Sono temi come
gli altri: tutto ciò che fanno lo può fare, o rovesciare, un file importato.
*The themes that ship with Kanso take their colors from its logo: the same slate with one color
each. They have rounded shapes, covers without a caption that tell about the game under the
pointer, and smooth animations. They are themes like any other: whatever they do, an imported
file can do or overturn.*

## Due modi di fare un tema / Two ways to make a theme

**Dall'app (colori e font) / In the app (colors and fonts)** — *Impostazioni → Tema →
Personalizza*: scegli un tema di partenza, cambia colori e font vedendo il risultato dal vivo, poi
*Salva come nuovo tema*. L'editor modifica **solo** colori e font; tutto il resto del tema di
partenza viene conservato com'è.

**Da file (tutto il resto) / From a file (everything else)** — forme, misure, icone, sfondo, font
incorporati e la disposizione di pannello laterale, Home, Store e News si definiscono nel file:

1. *Impostazioni → Tema*: scegli un tema e premi *Esporta...*. Il file esportato elenca **tutti** i
   valori, quindi è un modello completo. In alternativa copia un esempio da [`themes/`](themes/).
2. Modifica il `.json` con un editor di testo.
3. *Importa...* e scegli il file. Se un valore è sbagliato l'importazione viene rifiutata indicando
   il campo da correggere.

*Export any theme to get a complete template, edit the `.json`, import it. A wrong value makes the
import fail with the name of the field to fix.*

Esempi / Examples: [`nord`](themes/nord.kanso-theme.json) (solo colori, formato 1),
[`arcade`](themes/arcade.kanso-theme.json) (angoli tagliati, titoli pixel, icone squadrate, bordi
netti e bagliore al passaggio, riquadri pieni nella scheda del gioco, animazioni vivaci),
[`aurora`](themes/aurora.kanso-theme.json) (sfondo sfumato, forme tonde, riquadri di vetro nella
scheda del gioco sopra la copertina molto sfocata). Entrambi usano i layout attuali: copertine
che raccontano il gioco al passaggio del mouse e scheda a riquadri con le altre schede che scorrono.
Arcade e Aurora sono anche inclusi nell'app: il tema incluso e il file sono la stessa cosa.
Pannello laterale, Store e News hanno in entrambi la disposizione predefinita di Kanso, quella
che avevano prima che un tema potesse cambiarla: chi li usa li ritrova com'erano.

Due altri esempi, [`arcade-rows`](themes/arcade-rows.kanso-theme.json) e
[`aurora-grid`](themes/aurora-grid.kanso-theme.json), hanno lo stesso aspetto di Arcade e Aurora
e in più dispongono il pannello laterale, lo Store e le News, ciascuno a modo suo. Non sono
inclusi nell'app: si importano come ogni altro file. *Two more examples look like Arcade and
Aurora and also arrange the side panel, the Store and the News, each in its own way. They are
not built in: import them like any other file.*

| | Pannello laterale / Side panel | Store | News |
| --- | --- | --- | --- |
| **Arcade Rows** | Pagine in alto con gli Emulatori subito dopo la Home (`home`, `emulators`, `store`, `news`, `stats`); tra i gruppi, le piattaforme prima degli stati | A elenco (`list`), senza la riga delle cifre | A elenco (`list`), con immagine e riassunto |
| **Aurora Grid** | Pagine in fondo (`pagesPosition: bottom`); generi e tag subito dopo la libreria, poi piattaforme e stati | A griglia, con «Per te» come prima scheda: la pagina si apre lì | A griglia uniforme (`grid`), senza la notizia grande in testa |

Nord, che è un file del formato 1, non dice nulla di tutto questo e prende la disposizione
predefinita di Kanso, come Arcade, Aurora e i sei temi inclusi nati dal logo.

## Installare un tema / Installing a theme

- *Impostazioni → Tema → Importa...* e scegli il file, **oppure**
- *Apri cartella temi* e copia lì il file `.json`: comparirà nell'elenco alla riapertura delle
  Impostazioni.

I temi installati stanno in `%LOCALAPPDATA%\com.kanso.launcher\themes\`.

## Condividere un tema / Sharing a theme

Un tema è un file solo e nessuno deve approvarlo: pubblicalo dove vuoi, con la licenza che
scegli.

1. *Impostazioni → Tema → Esporta...*: il file `<nome>.kanso-theme.json` contiene tutto, anche
   i font incorporati.
2. Mettilo in un repository GitHub tuo (o in un gist), con una schermata e una riga su come si
   installa: *Impostazioni → Tema → Importa...*.
3. Aggiungi al repository il topic **`kanso-theme`**: comparirà in
   <https://github.com/topics/kanso-theme>, l'elenco dove chi usa Kanso cerca i temi degli
   altri. Non c'è niente da chiedere a nessuno.
4. Se ti va, mostralo nelle [Discussions](https://github.com/SpheralComa441/Kanso/discussions).

Usa solo font che si possono ridistribuire: il file li porta con sé. Chi importa un tema non
corre rischi, vedi [Sicurezza](#sicurezza--safety).

*A theme is one file and needs nobody's approval. Export it (Settings → Theme → Export...),
put it in a repository or gist of your own with a screenshot, and add the topic
**`kanso-theme`** to the repository: it is then listed at
<https://github.com/topics/kanso-theme>, where people look for themes. Show it in the
[Discussions](https://github.com/SpheralComa441/Kanso/discussions) if you like. The file carries its fonts, so use only fonts that may be
redistributed.*

## Formato / Format

Solo `name` e `colors` sono obbligatori. Tutto ciò che manca prende il valore predefinito di Kanso;
i numeri fuori intervallo vengono riportati al limite più vicino; un valore del tipo sbagliato o un
nome non previsto fanno rifiutare il file. I file con `"kansoTheme": 1` (colori, tre misure e un
font) e `"kansoTheme": 2` restano validi: le sezioni che non conoscevano (`nav`, `categories`,
`store`, `news`) prendono i valori predefiniti, cioè l'aspetto che avevano prima. L'unica differenza tra le
versioni riguarda `home.sections` ed è spiegata [più sotto](#home--la-pagina-home).
*Only `name` and `colors` are required. Missing values take Kanso's default, out-of-range numbers
are clamped, wrong types and unknown names are rejected. Files marked 1 or 2 are still valid:
the sections they did not know (`nav`, `categories`, `store`, `news`) take their defaults, that is the look
they had before. The one difference between versions is about `home.sections`, explained below.*

```json
{
  "kansoTheme": 3,
  "name": "Il mio tema",
  "author": "Chi l'ha fatto",
  "description": "Una riga di descrizione.",
  "colorScheme": "dark",
  "colors": {
    "background": "#17181f",
    "surface": "#1e2029",
    "elevated": "#282a37",
    "border": "#343747",
    "borderStrong": "#5b5f78",
    "text": "#f3f4f8",
    "textSecondary": "#d5d7e1",
    "textMuted": "#a4a8ba",
    "textFaint": "#868aa1",
    "accent": "#ff4500",
    "accentHover": "#ff6a33",
    "onAccent": "#0d0d10",
    "accentSecondary": "#3ecbe0"
  },
  "fontFamily": "Rubik",
  "typography": {
    "headingFont": "Unbounded",
    "headingWeight": 600,
    "headingUppercase": false,
    "headingSpacing": 0,
    "scale": 1,
    "labelFont": null,
    "labelUppercase": true
  },
  "layout": {
    "cardMinWidth": 185,
    "cardRadius": 10,
    "sidebarWidth": 208,
    "gridGap": 16,
    "headerHeight": 52,
    "contentPadding": 24,
    "sidebarSide": "left",
    "dividers": false,
    "scrollbars": "auto",
    "windowControls": "bars",
    "navHeadings": "accent",
    "navPages": "secondary"
  },
  "cards": {
    "shape": "rounded",
    "aspect": "2:3",
    "borderWidth": 1,
    "shadow": "none",
    "hover": "lift",
    "caption": "below"
  },
  "shapes": { "controlRadius": 8, "panelRadius": 12, "chipRadius": 6, "panelShape": "rounded" },
  "icons": { "strokeWidth": 2, "corners": "round", "scale": 1, "fill": 0 },
  "effects": { "backgroundGradient": [], "backgroundAngle": 180 },
  "detail": {
    "layout": "bento",
    "backdrop": "art",
    "backdropBlur": 18,
    "backdropDim": 0.62,
    "tiles": "filled",
    "tileGap": 12,
    "cards": "rail",
    "keyTitles": "accent",
    "cardTitles": "secondary"
  },
  "motion": { "level": "smooth", "sheet": "rise", "view": "rise", "stagger": true },
  "home": {
    "sections": ["resume", "upNext", "mostPlayed", "recommendations", "deals", "plugins"],
    "effects": "seasonal"
  },
  "nav": {
    "pages": ["home", "store", "news", "stats", "emulators"],
    "groups": ["library", "status", "platform", "genres", "tags"],
    "pagesPosition": "top",
    "pagesAlign": "center",
    "tabStyle": "frame",
    "tabRadius": null,
    "tabBorderWidth": 1.5,
    "tabPaddingX": 14,
    "tabPaddingY": 6,
    "tabGap": 6,
    "tabIcons": true,
    "tabLabels": true,
    "tabCounts": true,
    "tabBarBackground": "none",
    "tabColor": null,
    "tabHoverColor": null,
    "tabActiveColor": null,
    "tabFrameColor": null,
    "tabFrameAccent": null,
    "tabActiveBackground": null,
    "tabRestOpacity": 0.32,
    "tabAnimation": "wave",
    "tabAnimationSpeed": 5.5,
    "tabGlow": 0.78,
    "tabTransition": "fade",
    "tabTransitionMs": 380
  },
  "categories": { "platforms": true },
  "store": {
    "tabs": ["all", "picks", "steam", "epic", "gog", "others", "liked"],
    "layout": "grid",
    "figures": true
  },
  "news": { "layout": "magazine", "pictures": true, "summaries": true },
  "fontFiles": []
}
```

I valori qui sopra sono i predefiniti di Kanso. *The values above are Kanso's defaults.*

### Generali / General

| Campo / Field | Significato / Meaning |
| --- | --- |
| `kansoTheme` | Versione del formato: `3` (i file `1` e `2` restano validi). Un numero più alto di quello che Kanso conosce fa rifiutare il file. |
| `name` | Nome mostrato nell'elenco (max 60 caratteri). Obbligatorio. |
| `author`, `description` | Facoltativi. |
| `colorScheme` | `"dark"` o `"light"`: dice a Kanso quali tinte usare per badge e avvisi sopra le tue superfici. |

### `colors` — Colori

Valori esadecimali `#rgb`, `#rrggbb` o `#rrggbbaa`.

| Chiave / Key | Dove si vede / Where it shows |
| --- | --- |
| `background` | Sfondo della finestra |
| `surface` | Card, pannelli, finestre di dialogo |
| `elevated` | Righe selezionate o al passaggio del mouse, controlli in rilievo |
| `border` | Bordi delle card e dei controlli, linee divisorie |
| `borderStrong` | Bordi dei controlli al passaggio del mouse o a fuoco |
| `text` | Titoli e testo principale |
| `textSecondary` | Testo normale |
| `textMuted` | Etichette e informazioni secondarie |
| `textFaint` | Suggerimenti, contatori, segnaposto, barre di scorrimento |
| `accent` | Pulsanti principali, evidenziazioni, elementi attivi |
| `accentHover` | L'accento al passaggio del mouse |
| `onAccent` | Testo e icone sopra il colore d'accento |
| `accentSecondary` | Un secondo colore, distinto dall'accento: di norma le pagine del pannello laterale che non sono la libreria (Home, Statistiche, Emulatori) |

Un tema che ha un suo `accent` ma non indica `accentSecondary` (come i file del formato 1) ne
riceve uno ricavato dall'accento: lo stesso colore girato di 165° sulla ruota dei colori, con la
stessa saturazione e luminosità. *A theme with its own accent and no second color gets one
drawn from the accent.*

### Font

`fontFamily` è il font dell'interfaccia, `typography.headingFont` quello dei titoli. Si scrivono
come elenco di **nomi semplici** separati da virgole (`"Rubik"`, `"Exo 2, sans-serif"`): solo
lettere, cifre, spazi, virgole, trattini e apici. *Plain font names separated by commas.*

Un nome può indicare:

1. **un font incluso in Kanso** — funziona su ogni PC, anche in cirillico:
   Inter, Manrope, Montserrat, Nunito, Rubik, Exo 2, Tektur, Oswald, Unbounded, Comfortaa, Lora,
   Playfair Display, JetBrains Mono, Pixelify Sans; più due font per la leggibilità, solo per
   alfabeti latini: Atkinson Hyperlegible e OpenDyslexic;
2. **un font incorporato nel tema** con `fontFiles` (vedi sotto);
3. **un font installato sul PC** — attenzione: chi non lo ha vedrà il font predefinito.

| `typography` | Intervallo / Range | Significato / Meaning |
| --- | --- | --- |
| `headingFont` | — | Font dei titoli; `null` = come il testo |
| `headingWeight` | 100 – 900 | Peso dei titoli; `null` = quello previsto per ciascun titolo |
| `headingUppercase` | `true` / `false` | Titoli in maiuscolo |
| `headingSpacing` | −0.05 – 0.3 | Spaziatura tra le lettere dei titoli, in em |
| `scale` | 0.85 – 1.25 | Dimensione di tutta l'interfaccia rispetto al normale |
| `labelFont` | — | Font delle piccole etichette sopra cifre e sezioni («Tempo di gioco», «Obiettivi»); `null` = come il testo |
| `labelUppercase` | `true` / `false` | Piccole etichette in maiuscolo, spaziate |

#### `fontFiles` — Font incorporati / Embedded fonts

Per usare un font che non è tra quelli inclusi, mettilo dentro il tema: funzionerà ovunque.
*To use a font Kanso does not ship, embed it in the theme.*

```json
"fontFamily": "Mio Font",
"fontFiles": [
  { "family": "Mio Font", "weight": "400", "style": "normal", "file": "MioFont.woff2" }
]
```

- `file`: nome di un file **nella stessa cartella del tema** (`.woff2`, `.woff`, `.ttf`, `.otf`).
  All'importazione Kanso lo legge e lo incorpora: il tema installato — e quello che ottieni con
  *Esporta...* — è un unico file che contiene anche il font, pronto da condividere.
- In alternativa `data`: il file già codificato in base64 (è ciò che scrive *Esporta...*).
- `weight`: un peso (`"400"`) o, per i font variabili, un intervallo (`"100 900"`). `style`:
  `"normal"` o `"italic"`.
- Limiti: al massimo 6 file, 2 MB ciascuno. Usa solo font la cui licenza permette la
  ridistribuzione (es. SIL Open Font License).

### `layout` — Disposizione

| Chiave / Key | Valori / Values | Significato / Meaning |
| --- | --- | --- |
| `cardMinWidth` | 120 – 320 | Larghezza minima di una copertina nella griglia |
| `cardRadius` | 0 – 40 | Raggio degli angoli delle card (o misura del taglio, vedi `cards.shape`) |
| `sidebarWidth` | 160 – 320 | Larghezza del pannello laterale |
| `gridGap` | 2 – 40 | Spazio tra le card |
| `headerHeight` | 44 – 72 | Altezza della barra superiore |
| `contentPadding` | 8 – 56 | Margine attorno al contenuto |
| `sidebarSide` | `left`, `right` | Lato del pannello laterale |
| `dividers` | `true` / `false` | Linee sottili sotto la barra superiore e accanto al pannello laterale |
| `scrollbars` | `auto`, `always`, `hidden` | Barre di scorrimento: al passaggio del mouse, sempre visibili, nascoste |
| `windowControls` | `bars`, `dots` | Pulsanti della finestra: classici a tutta altezza, o pallini colorati |
| `navHeadings` | `accent`, `secondary`, `text`, `muted` | Colore delle intestazioni del pannello laterale (Libreria, Stato, Piattaforma, Generi, Tag) |
| `navPages` | `accent`, `secondary`, `text`, `muted` | Colore delle pagine del pannello laterale che non sono la libreria (Home, Statistiche, Emulatori) |

`navHeadings` e `navPages` non sono colori ma **nomi di colori del tema**: `accent` è
`colors.accent`, `secondary` è `colors.accentSecondary`, `text` è `colors.text`, `muted` è
`colors.textFaint`. Di norma le intestazioni usano l'accento e le pagine il secondo colore.

Le misure sono in pixel. *Sizes are in pixels.*

### `cards` — Card dei giochi

| Chiave / Key | Valori / Values | Significato / Meaning |
| --- | --- | --- |
| `shape` | `rounded`, `cut`, `slant` | Angoli arrotondati; tutti e quattro tagliati a 45°; tagliati solo in alto a destra e in basso a sinistra. La misura è `layout.cardRadius` |
| `aspect` | `2:3`, `3:4`, `4:5`, `1:1` | Proporzioni della copertina (le immagini sono 2:3: le altre proporzioni le ritagliano) |
| `borderWidth` | 0 – 4 | Spessore del bordo |
| `shadow` | `none`, `soft`, `strong`, `glow` | Ombra a riposo; `glow` è un alone nel colore d'accento. Con `cut` e `slant` l'ombra non è disponibile |
| `hover` | `lift`, `zoom`, `glow`, `none` | Al passaggio del mouse: la card si solleva; la copertina si ingrandisce; bordo e alone d'accento; nessun effetto |
| `caption` | `below`, `overlay`, `hover`, `none` | Titolo e generi sotto la copertina; sopra la parte bassa della copertina; solo copertina, che al passaggio del mouse (o con il fuoco da tastiera) si sfoca e mostra titolo, generi, ore giocate e le etichette dei plugin (le righe le sceglie l'utente nelle Impostazioni; vale anche per le copertine degli scaffali della Home, cioè da riprendere, prossimi giochi e più giocati, che tengono il titolo sotto e, dove le righe scelte non entrano tutte nella copertina, lasciano fuori per intero le ultime; non per i giochi proposti dai plugin nella Home, che hanno titolo, nota e pulsanti propri: vedi [PLUGINS.md](PLUGINS.md#a-part-of-home)); solo copertina |

### `shapes` — Controlli e pannelli

| Chiave / Key | Intervallo / Range | Significato / Meaning |
| --- | --- | --- |
| `controlRadius` | 0 – 24 | Pulsanti, campi di testo, righe degli elenchi |
| `panelRadius` | 0 – 32 | Finestre di dialogo e pannelli grandi |
| `chipRadius` | 0 – 16 | Badge, etichette e voci del pannello laterale |
| `panelShape` | `rounded`, `cut` | Angoli dei riquadri della scheda di un gioco: arrotondati di `panelRadius`, oppure tagliati a 45° della stessa misura in alto a destra e in basso a sinistra |

### `icons` — Icone

| Chiave / Key | Valori / Values | Significato / Meaning |
| --- | --- | --- |
| `strokeWidth` | 1 – 3 | Spessore del tratto |
| `corners` | `round`, `sharp` | Estremità e giunzioni arrotondate oppure squadrate |
| `scale` | 0.8 – 1.4 | Dimensione rispetto al normale |
| `fill` | 0 – 0.6 | Opacità di un riempimento nel colore dell'icona (0 = solo contorno) |

### `effects` — Sfondo

| Chiave / Key | Valori / Values | Significato / Meaning |
| --- | --- | --- |
| `backgroundGradient` | 2 o 3 colori | Sfondo sfumato della finestra; `[]` per il colore pieno `colors.background` |
| `backgroundAngle` | 0 – 360 | Direzione della sfumatura in gradi (180 = dall'alto in basso) |

### `detail` — La scheda di un gioco

Ciò che si apre cliccando un gioco. Un tema ne decide la disposizione, che cosa c'è dietro e
come sono fatti i riquadri. *What opens when a game is clicked: its layout, what is behind it and
how its tiles look.*

| Chiave / Key | Valori / Values | Significato / Meaning |
| --- | --- | --- |
| `layout` | `bento`, `sheet` | `bento`: riquadri separati (copertina e azioni a sinistra; a destra, subito in vista, titolo, cifre e obiettivi, poi le altre schede). `sheet`: una scheda unica con la copertina a lato e il resto in colonna |
| `backdrop` | `art`, `dim` | Dietro la scheda: l'immagine del gioco sfocata e velata, oppure un velo scuro sopra la libreria |
| `backdropBlur` | 0 – 48 | Sfocatura dell'immagine, in pixel |
| `backdropDim` | 0.35 – 0.95 | Quanto del colore `background` vela ciò che sta dietro. Non scende sotto 0.35: il testo deve restare leggibile sopra qualunque copertina |
| `tiles` | `filled`, `glass`, `outlined` | Riquadri pieni (`surface`); traslucidi, con lo sfondo che traspare sfocato; del colore di sfondo con bordo marcato |
| `tileGap` | 0 – 28 | Spazio tra i riquadri |
| `cards` | `rail`, `stack` | Le schede dopo le cifre e gli obiettivi (descrizione, attività, ciò che aggiungono i plugin, file). `rail`: affiancate in un riquadro che avanza da solo di una scheda alla volta, dove un clic ne fissa una. `stack`: tutte in vista, una sotto l'altra. Vale per `layout: bento` |
| `keyTitles` | `accent`, `secondary`, `text`, `muted` | Colore dei titoli di ciò che la scheda mostra per primo (cifre, obiettivi) |
| `cardTitles` | `accent`, `secondary`, `text`, `muted` | Colore dei titoli delle altre schede e del bordo di quelle fissate |

Come nel pannello laterale, `keyTitles` e `cardTitles` sono **nomi di colori del tema**, non
colori. Il riquadro che scorre segue le animazioni: quanto resta una scheda (9, 7 o 5 secondi
con animazioni leggere, fluide o vivaci), quanto dura il passo e con quale curva. Con le
animazioni spente non si muove da solo e restano le frecce.

L'immagine dietro la scheda è sempre e solo la copertina del gioco che Kanso ha già in libreria:
un tema sceglie *come* mostrarla, non *quale* immagine. *The image behind the sheet is always the
cover Kanso already has for that game: a theme chooses how to show it, never which image.*

### `home` — La pagina Home

Quali parti della Home ci sono, in che ordine, e che cosa si muove dietro. *Which parts of Home
there are, in which order, and what moves behind it.*

| Chiave / Key | Valori / Values | Significato / Meaning |
| --- | --- | --- |
| `sections` | elenco di `resume`, `upNext`, `mostPlayed`, `recommendations`, `deals`, `plugins` | Le parti mostrate, nell'ordine scritto: da riprendere, prossimi giochi, i più giocati, consigli per genere dallo Store («Per te»), i cinque sconti più alti, e `plugins`: le parti che i plugin attivati dall'utente aggiungono alla Home (per esempio «Il gioco di oggi»), una per plugin, una dopo l'altra nell'ordine dei plugin, ciascuna con il suo titolo, i suoi testi e i giochi che propone; finché nessun plugin ha qualcosa da mostrare non occupa spazio. Ogni nome una volta sola (una ripetizione viene ignorata). Una parte non nominata non viene mostrata; un elenco vuoto lascia la Home vuota |
| `effects` | `seasonal`, `none` | Lo sfondo che scende dall'alto: qualcosa della stagione, di una festa vicina o, se l'utente lo ha chiesto, del meteo; oppure niente |

**`sections` e la versione del file.** `plugins` esiste dal formato 3. Un file segnato
`"kansoTheme": 1` o `2` (o senza numero) che elenca `sections` è stato scritto prima che quella
parte esistesse: leggendolo, Kanso aggiunge `plugins` in fondo al suo elenco, così un tema già
installato non perde ciò che i plugin mostrano. Un file segnato `3` viene preso alla lettera:
se non nomina `plugins`, quella parte non c'è. I temi salvati dall'app e quelli esportati sono
sempre del formato 3, con l'elenco completo.
*`plugins` exists since format 3. A file marked 1 or 2 (or not marked) that lists `sections`
predates it: on reading, `plugins` is appended to its list. A file marked 3 is taken literally:
a part it does not name is not shown. Themes saved or exported by the app are always format 3.*

**Che cosa mostra `plugins`.** Un plugin può aggiungere alla Home una parte sua (vedi
[PLUGINS.md](PLUGINS.md#a-part-of-home)): un titolo, qualche riga di dati e fino a dodici giochi
della libreria, disegnati da Kanso con le intestazioni, le copertine e le animazioni del tema,
come le altre parti. Il tema decide **dove** stanno tutte insieme (il posto di `plugins`
nell'elenco) o che non ci sono (non nominandolo: in quel caso i plugin non vengono nemmeno
interrogati per la Home). Non può scegliere tra un plugin e l'altro: quali parti mostrare lo
decide l'utente, attivando i plugin e, in *Impostazioni → Home*, togliendo quelle che non vuole.
*What `plugins` shows: the parts that the plugins the user switched on add to Home, one per
plugin, drawn by Kanso with the theme's headings, covers and motion. The theme places them all
together, or leaves them out (then the plugins are not even asked); which plugins show is the
user's choice, in Settings → Plugins and Settings → Home.*

Un tema decide ordine e presenza delle parti; l'utente può comunque nasconderne qualunque, e
spegnere lo sfondo, da *Impostazioni → Home*. Lo sfondo prende i suoi colori dal tema (testo,
accento, secondo colore) e non si disegna con le animazioni spente. Un tema non può indicare
immagini, video o indirizzi per lo sfondo: sceglie solo tra i due nomi.

### `nav` — Navigazione, schede delle pagine e pannello laterale

Quali pagine e quali gruppi sono offerti, dove stanno le schede delle pagine (barra superiore,
barra inferiore o pannello laterale), e i loro colori, grafica e animazioni.
*Which pages and groups are offered, where the page tabs sit (top bar, bottom bar, or side panel),
and their colors, graphics and animations.*

| Chiave / Key | Valori / Values | Significato / Meaning |
| --- | --- | --- |
| `pages` | elenco di `home`, `store`, `news`, `stats`, `emulators` | Le pagine accanto alla libreria, nell'ordine scritto: Home, Store, News, Statistiche, Emulatori. Ogni nome una volta sola. Una pagina non nominata non è offerta: non ha la sua voce e nessun collegamento porta lì (per esempio «Vedi tutto nello Store» dalla Home). Fa eccezione `emulators` quando l'utente ha emulatori o giochi emulati (vedi sotto). `[]` lascia la sola libreria, più gli Emulatori per chi ne ha |
| `groups` | elenco di `library`, `status`, `platform`, `genres`, `tags` | I gruppi del pannello laterale, nell'ordine scritto: Libreria (tutti i giochi, preferiti, recenti, non installati, nascosti), Stato, Piattaforma, Generi, Tag. Un gruppo non nominato non viene mostrato, **tranne `library`**: se il tema non la nomina, Kanso la mette per prima |
| `pagesPosition` | `top`, `bottom`, `sidebarTop`, `sidebarBottom` | Dove stanno le schede delle pagine: barra orizzontale in alto sotto il titolo (`top`, predefinito), barra orizzontale in fondo alla finestra (`bottom`), elenco verticale in cima al pannello laterale sopra i gruppi (`sidebarTop`), oppure fissato in fondo al pannello laterale sotto i gruppi (`sidebarBottom`) |
| `pagesAlign` | `left`, `center`, `right` | Allineamento orizzontale delle schede quando `pagesPosition` è `top` o `bottom` |
| `tabStyle` | `frame`, `pill`, `underline`, `filled`, `minimal` | Aspetto della scheda attiva: piccolo riquadro arrotondato (`frame`), capsula (`pill`), linea luminosa sotto o di lato (`underline`), sfondo pieno sfumato (`filled`), oppure solo colore testo/icona senza riquadro (`minimal`) |
| `tabRadius` | `null` o 0 – 24 | Raggio degli angoli delle schede in pixel; `null` segue `shapes.controlRadius` (ignorato da `pill`) |
| `tabBorderWidth` | 0 – 3 | Spessore del riquadro o della linea luminosa, in pixel (predefinito `1.5`) |
| `tabPaddingX` | 6 – 28 | Spazio interno orizzontale di ciascuna scheda, in pixel |
| `tabPaddingY` | 2 – 14 | Spazio interno verticale di ciascuna scheda, in pixel |
| `tabGap` | 0 – 24 | Spazio tra una scheda e l'altra, in pixel |
| `tabIcons` | `true` / `false` | Mostra le icone sulle schede delle pagine |
| `tabLabels` | `true` / `false` | Mostra le etichette di testo sulle schede (con `tabIcons: true` e `tabLabels: false` si ottengono schede con sola icona e tooltip; se entrambi sono `false`, il testo resta visibile) |
| `tabCounts` | `true` / `false` | Mostra i contatori sulle schede (per esempio il numero di emulatori) |
| `tabBarBackground` | `none`, `surface`, `glass`, `pill` | Sfondo della barra orizzontale (`top` / `bottom`): trasparente (`none`), superficie piena (`surface`), vetro traslucido sfocato (`glass`), oppure contenitore a capsula attorno alle schede (`pill`) |
| `tabColor` | `null` o `#hex` | Colore a riposo delle schede non selezionate; `null` ricava un tono opaco dal colore del tema (`layout.navPages`) |
| `tabHoverColor` | `null` o `#hex` | Colore al passaggio del mouse o con il fuoco da tastiera; `null` usa il colore pieno `layout.navPages` |
| `tabActiveColor` | `null` o `#hex` | Colore della scheda selezionata quando il mouse non è sopra; `null` usa una tonalità morbida di `layout.navPages` |
| `tabFrameColor` | `null` o `#hex` | Colore principale del riquadro, della linea o del riempimento attivo; `null` usa `layout.navPages` |
| `tabFrameAccent` | `null` o `#hex` | Secondo colore sfumato nell'onda o nel riflesso; `null` usa `colors.accent` |
| `tabActiveBackground` | `null` o `#hex` | Sfondo della scheda attiva (`#rrggbb` o `#rrggbbaa`); `null` applica una leggera velatura automatica |
| `tabRestOpacity` | 0 – 1 | Intensità del colore del tema sulle schede a riposo quando `tabColor` è `null` (predefinito `0.32`) |
| `tabAnimation` | `wave`, `pulse`, `shimmer`, `none` | Animazione continua sulla scheda selezionata: onda luminosa morbida lungo il perimetro (`wave`), respiro uniforme (`pulse`), riflesso diagonale (`shimmer`), oppure statica (`none`) |
| `tabAnimationSpeed` | 1.5 – 15 | Durata di un ciclo dell'animazione in secondi (predefinito `5.5` per un movimento morbido) |
| `tabGlow` | 0 – 1 | Intensità dell'alone luminoso soffuso attorno alla scheda selezionata (`0` = nessun alone) |
| `tabTransition` | `fade`, `slide`, `none` | Animazione al cambio scheda: dissolvenza in uscita/entrata (`fade`), dissolvenza con leggero scorrimento (`slide`), oppure cambio istantaneo (`none`) |
| `tabTransitionMs` | 0 – 1000 | Durata della transizione al cambio scheda, in millisecondi (predefinito `380`) |

- La libreria c'è sempre: un tema non può togliere all'utente la strada verso i suoi giochi.
  *The library is always there: a theme cannot lock the user out of their games.*
- Per la stessa ragione la pagina degli Emulatori resta finché l'utente ha emulatori o giochi
  emulati: quei giochi non stanno nell'elenco della libreria né nella sua ricerca, e quella
  pagina è l'unica che li elenca tutti. Un tema può spostarla (nominando `emulators` dove
  vuole); se non la nomina, Kanso la mette dopo le pagine del tema. Per chi non ha emulatori né
  giochi emulati vale la scelta del tema: senza `emulators`, niente voce. *For the same reason
  the Emulators page stays while the user has emulators or emulated games: those games are not
  in the library list or its search, and that page is the only one that lists them all. A theme
  can place it; a theme that leaves `emulators` out gets it after its own pages. With no
  emulators and no emulated games the theme's choice stands: no entry.*
- Ciò che l'utente nasconde da *Impostazioni → Generale → Pannello laterale* (Home, Store, News,
  Tag) resta nascosto qualunque cosa dica il tema. *What the user hides in Settings stays hidden
  whatever the theme says.*
- Ogni gruppo mantiene le proprie condizioni: gli stati compaiono quando se ne usa almeno uno, le
  piattaforme sono quelle che hanno giochi, i tag ci sono dopo la prima sincronizzazione.
- Se la pagina aperta non è più offerta (perché si è cambiato tema, o la si è nascosta), Kanso
  torna alla libreria. Lo stesso vale per le schede di generi e tag quando il loro gruppo non c'è.
  Mentre un tema è solo in prova nelle Impostazioni si vede la libreria, ma annullando si torna
  alla pagina di prima.
- Una pagina non offerta non sparisce da Kanso: cambia solo ciò che il pannello propone. Senza
  `store`, per esempio, le offerte restano tra le parti della Home e gli store si scelgono
  ancora da *Impostazioni → Store*; senza `emulators` (per chi non ne ha) gli emulatori si
  aggiungono ancora da *Impostazioni → Emulatori*, e dalla scansione successiva la pagina
  compare da sé. *A page that is not offered is only out of the panel: without `store`, deals
  still show on Home and the stores are still chosen in Settings; without `emulators` (for
  someone who has none) emulators are still added in Settings → Emulators, and from the next
  scan the page shows up by itself.*
- Un filtro di un gruppo che il tema non mostra può comunque essere attivo (un genere scelto
  dalla scheda di un gioco, un filtro messo prima di cambiare tema). In quel caso il gruppo
  compare ridotto al titolo e alla sola voce selezionata, che lo spegne con un clic, e sparisce
  con lui. *A filter of a group the theme leaves out stays in sight: the group shows up with its
  title and that one entry, which switches it off.*
- Un tema sceglie tra le pagine e i gruppi che esistono: non può aggiungerne, rinominarli o
  cambiarne l'icona. I colori delle voci si scelgono con `layout.navHeadings` e
  `layout.navPages`.

### `categories` — Le schede di generi e tag

Che cosa sta sopra le schede di generi e di tag, le pagine che si aprono dai titoli *Generi* e
*Tag* del pannello laterale. *What is drawn above the tiles of genres and of tags, the pages the
side panel's Genres and Tags titles open.*

| Chiave / Key | Valori / Values | Significato / Meaning |
| --- | --- | --- |
| `platforms` | `true` / `false` | La riga delle piattaforme sopra le schede: *Tutte*, poi ogni store e ogni libreria portata da un plugin che ha giochi (e, nella pagina *Generi*, anche *Emulatori* quando ci sono giochi emulati), ciascuno con quanti ne ha. Sceglierne uno conta le schede sui soli giochi di quella piattaforma, e la scelta resta aprendo una scheda nella libreria. Compare quando i giochi che si stanno guardando (con *Solo installati*, quelli sul disco) sono di almeno due piattaforme, quando ci sono giochi emulati, o quando una è già scelta |

- Anche senza la riga, la piattaforma si sceglie dal pannello laterale: con le schede aperte,
  le voci di *Piattaforma* contano le schede invece di aprire la libreria. *Without the row the
  platform is still chosen from the side panel: while the tiles are open, its Platform entries
  count the tiles instead of opening the library.*
- Una piattaforma già scelta resta in vista anche senza la riga: compare come un filtro da
  togliere con un clic accanto al titolo. *A platform already chosen stays in sight without the
  row, as a filter next to the title that one click removes.*

### `store` — La pagina Store

Le schede della pagina, come sono disegnate le offerte e se ci sono le cifre in alto. Vale per la
pagina intera: i consigli e gli sconti che la Home mostra restano a riquadri.
*The tabs of the page, how deals are drawn and whether the figures are there. It applies to the
full page: what Home shows of the Store stays as tiles.*

| Chiave / Key | Valori / Values | Significato / Meaning |
| --- | --- | --- |
| `tabs` | elenco di `all`, `picks`, `steam`, `epic`, `gog`, `others`, `liked` | Le schede, nell'ordine scritto: tutte le offerte, «Per te», Steam, Epic Games, GOG, altri store, «Mi piace». Ogni nome una volta sola. Una scheda non nominata non è offerta (tranne `liked` finché l'utente ha offerte con il cuore, vedi sotto); **la prima è quella su cui si apre la pagina**. Ne serve almeno una: `[]` fa rifiutare il file |
| `layout` | `grid`, `list` | `grid`: riquadri con l'immagine in alto. `list`: righe compatte con immagine piccola, titolo, store, generi, scadenza, sconto e prezzo; le azioni «Mi piace» e «Nascondi» sono le stesse |
| `figures` | `true` / `false` | La riga delle quattro cifre (offerte, gratis ora, sconto più alto, in scadenza) in cima alla scheda `all` |

- Gli store che l'utente ha nascosto da *Impostazioni → Store* restano nascosti qualunque cosa
  dica il tema: la loro scheda non compare, e se era la prima la pagina si apre sulla successiva.
  *Stores the user hid stay hidden whatever the theme says.*
- Se tutte le schede che il tema nomina sono di store nascosti dall'utente, la pagina lo dice e
  rimanda a *Personalizza*, dove si possono mostrare di nuovo. *If every tab the theme lists
  belongs to a store the user hid, the page says so and points to Customize.*
- Le offerte con il cuore sono dell'utente: se il tema non nomina `liked`, la scheda «Mi piace»
  compare comunque, per ultima, finché ce ne sono. È lì che si vedono tutte, che se ne toglie
  una, e dove porta l'avviso di quelle in scadenza. *The deals the user liked are theirs: if the
  theme leaves `liked` out, the tab still shows, last, while there are any.*
- Nelle righe di `list`, dove la finestra è stretta o il testo è ingrandito, sconto, prezzo e
  azioni vanno a capo sotto il titolo, che resta leggibile. *In a narrow window or with larger
  text a row of `list` wraps: discount, price and actions go under the title.*
- `all` elenca le offerte di tutti gli store che l'utente tiene, anche di quelli a cui il tema
  non dà una scheda propria.
- Un tema sceglie tra le schede che esistono: non può aggiungere negozi o indirizzi, né cambiare
  quali store vengono interrogati.

### `news` — La pagina News

Come sono disposte le notizie e che cosa mostra ciascuna. *How the news are laid out and what
each item shows.*

| Chiave / Key | Valori / Values | Significato / Meaning |
| --- | --- | --- |
| `layout` | `magazine`, `grid`, `list` | `magazine`: la prima notizia grande, con l'immagine a lato, poi le altre a griglia. `grid`: tutte della stessa misura. `list`: righe con una piccola immagine a lato |
| `pictures` | `true` / `false` | Le notizie mostrano la loro immagine. Con `false` nessuna immagine viene mostrata (e in `magazine` nessuna notizia fa da apertura) |
| `summaries` | `true` / `false` | Le notizie mostrano il riassunto sotto il titolo |

Le fonti restano quelle scelte dall'utente in *Impostazioni → News*, e le immagini sono quelle
dei feed: un tema decide solo *come* mostrarle, non può indicare siti, feed o immagini.
*Sources stay the user's choice and pictures are the feeds' own: a theme only decides how they
are shown.*

### `motion` — Animazioni

Come si muove l'interfaccia. *How the interface moves.*

| Chiave / Key | Valori / Values | Significato / Meaning |
| --- | --- | --- |
| `level` | `none`, `subtle`, `smooth`, `lively` | Quanto: niente; dissolvenze brevi; dissolvenze con un piccolo movimento; movimenti più lunghi e ampi, con un leggero rimbalzo |
| `sheet` | `rise`, `zoom`, `slide`, `fade` | Come entrano la scheda di un gioco e le finestre di dialogo: dal basso, ingrandendosi, di lato, in dissolvenza |
| `view` | `rise`, `slide`, `fade` | Come cambia il contenuto scegliendo un'altra voce del pannello laterale |
| `stagger` | `true` / `false` | Le card entrano una dopo l'altra quando si apre una vista |

Un tema sceglie tra questi nomi: non può definire animazioni proprie né tempi arbitrari. L'utente
può cambiare `level` da *Impostazioni → Animazioni* (Tema, Nessuna, Leggere, Fluide, Vivaci) senza
toccare il tema, e l'opzione di accessibilità «Riduci le animazioni» le spegne sempre.
*A theme picks among these names: it cannot define animations or timings of its own. The user can
override `level` in Settings without touching the theme, and "Reduce motion" always switches
them off.*

## Accessibilità e temi / Accessibility and themes

Le preferenze di *Impostazioni → Accessibilità* prevalgono sul tema: la dimensione del testo
moltiplica `typography.scale`, il contrasto elevato rende più forti i colori di testo e bordi, un
font ad alta leggibilità sostituisce `fontFamily` e `headingFont`, le barre di scorrimento possono
essere forzate visibili, «Riduci le animazioni» spegne `motion`. Un tema non può disattivarle. *The user's accessibility preferences win
over the theme and cannot be switched off by it.*

Lo stesso vale per le scelte fatte nelle Impostazioni: le pagine e i tag nascosti dal pannello
laterale, le parti nascoste della Home, gli store nascosti e il livello delle animazioni restano
come l'utente li ha voluti con qualunque tema. Un tema può togliere o riordinare, mai rimettere
ciò che l'utente ha tolto. *The same goes for the user's choices in Settings: a theme can leave
out or reorder, never bring back what the user took away.*

## Sicurezza / Safety

Un tema può impostare **solo** i campi elencati sopra, e ognuno viene validato dal backend prima di
arrivare all'interfaccia: un colore deve essere un esadecimale, una misura un numero, una forma uno
dei nomi previsti, un font un elenco di nomi semplici, un file di font un vero font (viene
controllata la firma del file) e solo dalla cartella del tema. Un tema non può quindi contenere CSS
arbitrario, script, percorsi di file o risorse remote: importarne uno ricevuto da altri è sicuro.
Anche la disposizione di pannello laterale, Home, Store e News è fatta solo di nomi presi da
elenchi chiusi (pagine, gruppi, schede, parti, layout) e di sì/no: un tema riordina e sceglie tra
ciò che Kanso sa già disegnare, non può aggiungere pagine, negozi, fonti di notizie o contenuti
propri, né togliere la libreria.

*A theme can only set the fields listed above and each one is validated by the backend before it
reaches the interface. A theme file cannot carry arbitrary CSS, scripts, file paths or remote
resources, so importing one from a stranger is safe.*

## Per chi sviluppa / For developers

- Validazione e formato: `src-tauri/crates/kanso-core/src/theme.rs` (con i test).
- Applicazione dei valori all'interfaccia: `src/lib/themes.ts` (`applyTheme`) e le regole in
  `src/index.css`; i valori predefiniti sono in `src/theme.css`.
- Le sezioni che dispongono l'interfaccia arrivano ai componenti come proprietà del tema attivo,
  da `src/App.tsx`: `home` a `HomeView`, `nav` a `Sidebar`, `store` a `StoreView`, `news` a
  `NewsView`, `detail` a `GameDetail`. Che cosa è offerto nel pannello (tema più scelte
  dell'utente) si calcola in un punto solo: `offeredPages` e `offeredGroups` in
  `src/lib/themes.ts`.
- I temi inclusi Arcade e Aurora sono i file di `docs/themes/`, importati così come sono da
  `src/lib/themes.ts`; il test `the_example_themes_in_the_docs_are_valid` li fa passare dalla
  stessa validazione dei file importati.
- L'esempio completo di questa pagina è letto da un test
  (`the_full_example_in_the_theme_guide_is_valid_and_shows_the_default_arrangement`): se un
  campo cambia nome o viene aggiunto senza comparire qui, il test fallisce.
- Font inclusi: elenco in `scripts/generate-fonts-css.mjs` (`npm run fonts`), che rigenera
  `src/fonts.css` e `public/font-licenses.txt` (le licenze SIL OFL, distribuite con l'app).
