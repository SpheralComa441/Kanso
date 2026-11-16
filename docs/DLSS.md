# DLSS: che cosa fa Kanso con le librerie di NVIDIA / DLSS: what Kanso does with NVIDIA's libraries

Questa pagina dice che cosa Kanso legge, che cosa cambia, che cosa scarica e da dove, quando si
tratta di DLSS. Ogni affermazione rimanda al file in cui la puoi verificare.
*This page says what Kanso reads, changes and downloads, and from where, when it comes to
DLSS. Every statement points to the file where you can check it. The English version follows
the Italian one.*

---

## In breve

- **Leggere le versioni non contatta nessuno e non cambia nulla.** Kanso cerca nella cartella
  del gioco tre file — `nvngx_dlss.dll` (Super Resolution), `nvngx_dlssg.dll` (Frame
  Generation), `nvngx_dlssd.dll` (Ray Reconstruction) — e ne legge la versione.
- **Una libreria viene sostituita solo se lo chiedi tu**, gioco per gioco, con un clic.
- **L'originale è sempre conservato** accanto al file nuovo (`<nome>.kanso-original`) e
  *Ripristina l'originale* lo rimette. Un secondo aggiornamento non sovrascrive mai
  l'originale conservato: ciò che torna è sempre ciò con cui il gioco è stato installato.
- **Entra in un gioco solo un file firmato da NVIDIA**: lo conferma Windows, non Kanso.
- **Kanso non contiene e non distribuisce nessuna libreria di NVIDIA.** Non sono nel
  repository né nell'installer.
- **Il download da NVIDIA è spento** finché non lo accendi in *Impostazioni → DLSS*.

## Da dove può arrivare una libreria più recente

| Fonte | Rete | Che cosa succede |
| --- | --- | --- |
| **Un altro tuo gioco** | nessuna | Kanso copia la libreria più recente che c'è già sul tuo PC, nella cartella dell'altro gioco |
| **Un file scelto da te** | nessuna | Indichi una libreria che hai scaricato tu; vale per la parte che il nome del file indica |
| **NVIDIA** (solo con l'impostazione accesa) | sì | Kanso scarica il file dal repository pubblico di NVIDIA |

## Che cosa viene contattato, con l'impostazione accesa

Due indirizzi, senza account né chiave, e senza inviare nulla di te o dei tuoi giochi:

1. `https://github.com/NVIDIA/DLSS.git/info/refs?service=git-upload-pack` — l'elenco delle
   versioni (i tag) del repository pubblico di NVIDIA. È la stessa richiesta che fa `git`
   quando guarda un repository. Kanso la fa all'avvio (al più ogni dodici ore) per dire quanti
   giochi sono indietro, e quando premi *Controlla ora*.
2. `https://raw.githubusercontent.com/NVIDIA/DLSS/v<versione>/lib/Windows_x86_64/rel/<file>` —
   il file della libreria, quando aggiorni un gioco da NVIDIA o premi *Scarica*.

Un file scaricato viene tenuto solo se dichiara la versione chiesta e se Windows conferma la
firma di NVIDIA; altrimenti viene cancellato. Le librerie scaricate stanno nella cartella dati
di Kanso (`dlss/v<versione>/`); quando esce una versione nuova le vecchie vengono rimosse.

Con l'impostazione spenta nessuno dei due indirizzi viene contattato: il comando si rifiuta.

## La licenza

Le librerie sono di NVIDIA e restano sotto la sua licenza, la *NVIDIA RTX SDKs License*
(`LICENSE.txt` nel repository di NVIDIA). È scritta per chi sviluppa programmi che usano DLSS:
concede di «installare e usare l'SDK» e vieta di distribuirlo come prodotto a sé. **Non dice
nulla, né a favore né contro, sulla sostituzione delle librerie in un gioco già pubblicato.**
Per questo Kanso:

- non include le librerie e non le ospita: le scarica dal repository di NVIDIA sul tuo PC, su
  tua richiesta;
- tiene il download spento finché non lo accendi tu;
- offre due strade che non scaricano nulla (un altro tuo gioco, un file tuo).

Usare una libreria diversa da quella con cui un gioco è stato pubblicato è una tua scelta. Nei
giochi online con anti-cheat una libreria diversa può non essere accettata: Kanso lo scrive
accanto al pulsante, e *Ripristina l'originale* rimette tutto com'era.

## Che cosa Kanso non fa

- Non aggiorna nulla da solo: né all'avvio, né in blocco.
- Non tocca DLSS 1 (le librerie più recenti non lo sostituiscono).
- Non sostituisce una copia che è già alla versione offerta o più recente.
- Non lascia file a metà: il file nuovo viene scritto accanto al vecchio e ne prende il posto
  in un solo passo; se la scrittura non riesce (gioco in esecuzione, cartella protetta) la
  cartella resta com'era.
- Non segue collegamenti a cartelle esterne e non cerca oltre nove livelli sotto la cartella
  del gioco.

## Dove verificare

| Che cosa | File |
| --- | --- |
| Ricerca delle librerie, sostituzione, originale conservato, ripristino, download | `src-tauri/crates/kanso-core/src/dlss.rs` |
| Lettura della versione di un file | `src-tauri/crates/kanso-core/src/pe_version.rs` |
| Controllo della firma (di Windows, `WinVerifyTrust`) | `src-tauri/crates/kanso-core/src/authenticode.rs` |
| Comandi dell'app: che cosa è permesso con l'impostazione spenta | `src-tauri/src/dlss.rs` |
| L'impostazione (`dlssUpdates`, spenta in partenza) | `src-tauri/crates/kanso-core/src/config.rs` |

## Non ancora provato su Windows

Dall'ambiente di sviluppo (Linux) non si è potuto provare: il controllo della firma, che usa
funzioni di Windows; il download fatto dall'app stessa (gli indirizzi sono stati provati a
mano, e la libreria scaricata è risultata firmata da NVIDIA); la sostituzione in una cartella
di gioco vera.

---

## In short

- **Reading the versions contacts nobody and changes nothing.** Kanso looks in the game's
  folder for three files — `nvngx_dlss.dll` (Super Resolution), `nvngx_dlssg.dll` (Frame
  Generation), `nvngx_dlssd.dll` (Ray Reconstruction) — and reads each one's version.
- **A library is replaced only when you ask**, game by game, with a click.
- **The original is always kept** beside the new file (`<name>.kanso-original`) and *Restore
  the original* puts it back. A second update never overwrites the original kept: what comes
  back is always what the game was installed with.
- **Only a file signed by NVIDIA goes into a game**: Windows confirms it, not Kanso.
- **Kanso neither contains nor distributes any NVIDIA library.** They are not in the
  repository or in the installer.
- **Downloading from NVIDIA is off** until you turn it on in *Settings → DLSS*.

## Where a newer library can come from

| Source | Network | What happens |
| --- | --- | --- |
| **Another of your games** | none | Kanso copies the newest library already on your PC, from the other game's folder |
| **A file you chose** | none | You point at a library you downloaded; it stands in for the part its file name says |
| **NVIDIA** (only with the setting on) | yes | Kanso downloads the file from NVIDIA's public repository |

## What is contacted, with the setting on

Two addresses, with no account and no key, and sending nothing about you or your games:

1. `https://github.com/NVIDIA/DLSS.git/info/refs?service=git-upload-pack` — the list of the
   versions (tags) of NVIDIA's public repository, the same request `git` makes when it looks
   at a repository. Kanso makes it at start (at most every twelve hours) to say how many games
   are behind, and when you press *Check now*.
2. `https://raw.githubusercontent.com/NVIDIA/DLSS/v<version>/lib/Windows_x86_64/rel/<file>` —
   the library file, when you update a game from NVIDIA or press *Download*.

A downloaded file is kept only if it declares the version asked for and Windows confirms
NVIDIA's signature; otherwise it is deleted. Downloads live in Kanso's data folder
(`dlss/v<version>/`); older ones are removed when a newer version comes.

With the setting off neither address is contacted: the command refuses.

## The licence

The libraries are NVIDIA's and stay under its licence, the *NVIDIA RTX SDKs License*
(`LICENSE.txt` in NVIDIA's repository). It is written for developers of programs that use
DLSS: it grants the right to "install and use the SDK" and forbids distributing it as a
stand-alone product. **It says nothing, for or against, about replacing the libraries in a
game already published.** That is why Kanso does not include or host the libraries, keeps the
download off until you turn it on, and offers two ways that download nothing.

Using a library other than the one a game shipped with is your choice. In online games with
anti-cheat a different library may not be accepted: Kanso says so next to the button, and
*Restore the original* puts everything back.

## What Kanso does not do

It updates nothing on its own; it leaves DLSS 1 alone; it does not replace a copy already at
the version offered or newer; it leaves no half-written file (if writing fails the folder
stays as it was); it follows no links to folders elsewhere and looks no deeper than nine
levels below the game's folder.

## Not yet tried on Windows

From the development environment (Linux) these could not be tried: the signature check, which
uses Windows functions; the download made by the app itself (the addresses were tried by
hand, and the library downloaded was signed by NVIDIA); a replacement in a real game folder.
