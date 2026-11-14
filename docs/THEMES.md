# Temi di Kanso / Kanso themes

Un tema è un singolo file `.json` che cambia colori, misure e font dell'intera interfaccia.
*A theme is a single `.json` file that restyles the whole interface: colors, sizes and font.*

## Creare un tema / Making a theme

**Senza toccare file / Without touching files** — *Impostazioni → Tema → Personalizza*: scegli un
tema di partenza, modifica colori e misure vedendo il risultato dal vivo, poi *Salva come nuovo
tema*. Con *Esporta...* ottieni il file da condividere.

**A mano / By hand** — copia [`themes/nord.kanso-theme.json`](themes/nord.kanso-theme.json),
modificalo e importalo. *Copy the example, edit it, import it.*

## Installare un tema / Installing a theme

- *Impostazioni → Tema → Importa...* e scegli il file, **oppure**
- *Apri cartella temi* e copia lì il file `.json`: comparirà nell'elenco alla riapertura delle
  Impostazioni.

I temi installati stanno in `%LOCALAPPDATA%\com.kanso.launcher\themes\`.

## Formato / Format

```json
{
  "kansoTheme": 1,
  "name": "Nord",
  "author": "Chi l'ha fatto",
  "description": "Una riga di descrizione.",
  "colorScheme": "dark",
  "colors": {
    "background": "#2e3440",
    "surface": "#3b4252",
    "elevated": "#434c5e",
    "border": "#4c566a",
    "borderStrong": "#6b7890",
    "text": "#eceff4",
    "textSecondary": "#d8dee9",
    "textMuted": "#aab4c5",
    "textFaint": "#8a94a8",
    "accent": "#88c0d0",
    "accentHover": "#a3d4e2",
    "onAccent": "#2e3440"
  },
  "layout": { "cardMinWidth": 185, "cardRadius": 10, "sidebarWidth": 208 },
  "fontFamily": "Segoe UI, sans-serif"
}
```

Solo `name` e `colors` sono obbligatori; tutto ciò che manca prende il valore predefinito di
Kanso per lo schema scelto. *Only `name` and `colors` are required; anything missing falls back
to Kanso's default for the chosen scheme.*

| Campo / Field | Significato / Meaning |
| --- | --- |
| `kansoTheme` | Versione del formato, attualmente `1`. |
| `name` | Nome mostrato nell'elenco (max 60 caratteri). |
| `author`, `description` | Facoltativi. |
| `colorScheme` | `"dark"` o `"light"`: dice a Kanso quali tinte usare per badge e avvisi sopra le tue superfici. |
| `fontFamily` | Elenco di font **installati sul PC**; `null` per quello di Kanso. Solo lettere, cifre, spazi, virgole, trattini e apici. |

### Colori / Colors

Valori esadecimali `#rgb`, `#rrggbb` o `#rrggbbaa`.

| Chiave / Key | Dove si vede / Where it shows |
| --- | --- |
| `background` | Sfondo della finestra |
| `surface` | Card, pannelli, finestre di dialogo |
| `elevated` | Righe selezionate o al passaggio del mouse, controlli in rilievo |
| `border` | Linee sottili tra le superfici |
| `borderStrong` | Bordi dei controlli al passaggio del mouse o a fuoco |
| `text` | Titoli e testo principale |
| `textSecondary` | Testo normale |
| `textMuted` | Etichette e informazioni secondarie |
| `textFaint` | Suggerimenti, contatori, segnaposto |
| `accent` | Pulsanti principali, evidenziazioni, elementi attivi |
| `accentHover` | L'accento al passaggio del mouse |
| `onAccent` | Testo e icone sopra il colore d'accento |

### Misure / Layout

Valori in pixel; quelli fuori intervallo vengono riportati al limite più vicino.

| Chiave / Key | Intervallo / Range | Significato / Meaning |
| --- | --- | --- |
| `cardMinWidth` | 120 – 320 | Larghezza minima di una copertina nella griglia |
| `cardRadius` | 0 – 28 | Raggio degli angoli delle card |
| `sidebarWidth` | 160 – 320 | Larghezza del pannello laterale |

## Sicurezza / Safety

Un tema può impostare **solo** i valori elencati sopra, e ognuno viene validato prima di essere
applicato: un colore deve essere un esadecimale, una misura un numero, un font un semplice elenco
di nomi. Un file di tema non può quindi contenere CSS arbitrario, script o risorse remote, e
importarne uno ricevuto da altri è sicuro. Se un valore è sbagliato l'importazione viene rifiutata
indicando il campo da correggere.

*A theme can only set the values listed above and each one is validated before use, so a theme
file cannot carry arbitrary CSS, scripts or remote resources. A wrong value makes the import fail
with the name of the field to fix.*
