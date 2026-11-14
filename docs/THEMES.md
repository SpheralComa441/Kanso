# Temi di Kanso / Kanso themes

Un tema è un singolo file `.json` che ridisegna l'intera interfaccia: colori, font, forma di card,
controlli e icone, disposizione e sfondo.
*A theme is a single `.json` file that restyles the whole interface: colors, fonts, the shape of
cards, controls and icons, layout and background.*

## Due modi di fare un tema / Two ways to make a theme

**Dall'app (colori e font) / In the app (colors and fonts)** — *Impostazioni → Tema →
Personalizza*: scegli un tema di partenza, cambia colori e font vedendo il risultato dal vivo, poi
*Salva come nuovo tema*. L'editor modifica **solo** colori e font; tutto il resto del tema di
partenza viene conservato com'è.

**Da file (tutto il resto) / From a file (everything else)** — forme, misure, icone, sfondo e font
incorporati si definiscono nel file:

1. *Impostazioni → Tema*: scegli un tema e premi *Esporta...*. Il file esportato elenca **tutti** i
   valori, quindi è un modello completo. In alternativa copia un esempio da [`themes/`](themes/).
2. Modifica il `.json` con un editor di testo.
3. *Importa...* e scegli il file. Se un valore è sbagliato l'importazione viene rifiutata indicando
   il campo da correggere.

*Export any theme to get a complete template, edit the `.json`, import it. A wrong value makes the
import fail with the name of the field to fix.*

Esempi / Examples: [`nord`](themes/nord.kanso-theme.json) (solo colori, formato 1),
[`arcade`](themes/arcade.kanso-theme.json) (angoli tagliati, titoli pixel, icone squadrate),
[`aurora`](themes/aurora.kanso-theme.json) (sfondo sfumato, forme tonde, titoli sulle copertine).
Arcade e Aurora sono anche inclusi nell'app.

## Installare un tema / Installing a theme

- *Impostazioni → Tema → Importa...* e scegli il file, **oppure**
- *Apri cartella temi* e copia lì il file `.json`: comparirà nell'elenco alla riapertura delle
  Impostazioni.

I temi installati stanno in `%LOCALAPPDATA%\com.kanso.launcher\themes\`.

## Formato / Format

Solo `name` e `colors` sono obbligatori. Tutto ciò che manca prende il valore predefinito di Kanso;
i numeri fuori intervallo vengono riportati al limite più vicino; un valore del tipo sbagliato o un
nome non previsto fanno rifiutare il file. I file con `"kansoTheme": 1` (colori, tre misure e un
font) restano validi.
*Only `name` and `colors` are required. Missing values take Kanso's default, out-of-range numbers
are clamped, wrong types and unknown names are rejected. Version 1 files are still valid.*

```json
{
  "kansoTheme": 2,
  "name": "Il mio tema",
  "author": "Chi l'ha fatto",
  "description": "Una riga di descrizione.",
  "colorScheme": "dark",
  "colors": {
    "background": "#0f0f11",
    "surface": "#16161a",
    "elevated": "#1e1e24",
    "border": "#272730",
    "borderStrong": "#52525b",
    "text": "#f4f4f5",
    "textSecondary": "#d4d4d8",
    "textMuted": "#a1a1aa",
    "textFaint": "#71717a",
    "accent": "#e5a93c",
    "accentHover": "#f0bc59",
    "onAccent": "#09090b"
  },
  "fontFamily": "Rubik",
  "typography": {
    "headingFont": "Unbounded",
    "headingWeight": 600,
    "headingUppercase": false,
    "headingSpacing": 0,
    "scale": 1
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
    "windowControls": "bars"
  },
  "cards": {
    "shape": "rounded",
    "aspect": "2:3",
    "borderWidth": 1,
    "shadow": "none",
    "hover": "lift",
    "caption": "below"
  },
  "shapes": { "controlRadius": 8, "panelRadius": 12, "chipRadius": 6 },
  "icons": { "strokeWidth": 2, "corners": "round", "scale": 1, "fill": 0 },
  "effects": { "backgroundGradient": [], "backgroundAngle": 180 },
  "fontFiles": []
}
```

I valori qui sopra sono i predefiniti di Kanso. *The values above are Kanso's defaults.*

### Generali / General

| Campo / Field | Significato / Meaning |
| --- | --- |
| `kansoTheme` | Versione del formato: `2` (i file `1` restano validi). |
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

Le misure sono in pixel. *Sizes are in pixels.*

### `cards` — Card dei giochi

| Chiave / Key | Valori / Values | Significato / Meaning |
| --- | --- | --- |
| `shape` | `rounded`, `cut`, `slant` | Angoli arrotondati; tutti e quattro tagliati a 45°; tagliati solo in alto a destra e in basso a sinistra. La misura è `layout.cardRadius` |
| `aspect` | `2:3`, `3:4`, `4:5`, `1:1` | Proporzioni della copertina (le immagini sono 2:3: le altre proporzioni le ritagliano) |
| `borderWidth` | 0 – 4 | Spessore del bordo |
| `shadow` | `none`, `soft`, `strong`, `glow` | Ombra a riposo; `glow` è un alone nel colore d'accento. Con `cut` e `slant` l'ombra non è disponibile |
| `hover` | `lift`, `zoom`, `glow`, `none` | Al passaggio del mouse: la card si solleva; la copertina si ingrandisce; bordo e alone d'accento; nessun effetto |
| `caption` | `below`, `overlay`, `none` | Titolo e generi sotto la copertina; sopra la parte bassa della copertina; solo copertina |

### `shapes` — Controlli e pannelli

| Chiave / Key | Intervallo / Range | Significato / Meaning |
| --- | --- | --- |
| `controlRadius` | 0 – 24 | Pulsanti, campi di testo, righe degli elenchi |
| `panelRadius` | 0 – 32 | Finestre di dialogo e pannelli grandi |
| `chipRadius` | 0 – 16 | Badge, etichette e voci del pannello laterale |

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

## Accessibilità e temi / Accessibility and themes

Le preferenze di *Impostazioni → Accessibilità* prevalgono sul tema: la dimensione del testo
moltiplica `typography.scale`, il contrasto elevato rende più forti i colori di testo e bordi, un
font ad alta leggibilità sostituisce `fontFamily` e `headingFont`, le barre di scorrimento possono
essere forzate visibili. Un tema non può disattivarle. *The user's accessibility preferences win
over the theme and cannot be switched off by it.*

## Sicurezza / Safety

Un tema può impostare **solo** i campi elencati sopra, e ognuno viene validato dal backend prima di
arrivare all'interfaccia: un colore deve essere un esadecimale, una misura un numero, una forma uno
dei nomi previsti, un font un elenco di nomi semplici, un file di font un vero font (viene
controllata la firma del file) e solo dalla cartella del tema. Un tema non può quindi contenere CSS
arbitrario, script, percorsi di file o risorse remote: importarne uno ricevuto da altri è sicuro.

*A theme can only set the fields listed above and each one is validated by the backend before it
reaches the interface. A theme file cannot carry arbitrary CSS, scripts, file paths or remote
resources, so importing one from a stranger is safe.*

## Per chi sviluppa / For developers

- Validazione e formato: `src-tauri/crates/kanso-core/src/theme.rs` (con i test).
- Applicazione dei valori all'interfaccia: `src/lib/themes.ts` (`applyTheme`) e le regole in
  `src/index.css`; i valori predefiniti sono in `src/theme.css`.
- Font inclusi: elenco in `scripts/generate-fonts-css.mjs` (`npm run fonts`), che rigenera
  `src/fonts.css` e `public/font-licenses.txt` (le licenze SIL OFL, distribuite con l'app).
