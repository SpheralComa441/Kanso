# Account degli store: che cosa fa Kanso con il tuo accesso / Store accounts: what Kanso does with your sign-in

Questa pagina dice, senza giri di parole, che cosa succede quando accedi a uno store da Kanso:
che cosa Kanso vede, che cosa conserva, dove, a chi lo invia e come lo cancelli. Ogni
affermazione rimanda al file del codice in cui la puoi verificare.
*This page says plainly what happens when you sign in to a store from Kanso: what Kanso sees,
what it keeps, where, whom it sends it to and how you delete it. Every statement points to the
source file where you can check it. The English version follows the Italian one.*

---

## In breve

- **La password non passa mai da Kanso.** La scrivi sulla pagina dello store (nel tuo browser, o
  in una finestra che mostra la pagina dello store) e arriva solo allo store. Kanso non la
  legge, non la salva, non la invia a nessuno. Vale anche per i codici a due fattori.
- **Kanso non ha server.** Non esiste un servizio di Kanso a cui possa arrivare qualcosa: niente
  telemetria, niente statistiche, niente copia "nel cloud". Tutto resta sul tuo PC.
- **Sul PC resta solo la sessione** che lo store rilascia dopo l'accesso (un token, oppure i
  cookie del sito dello store). Serve a una cosa sola: chiedere *a quello stesso store* l'elenco
  dei tuoi giochi. Non viene mai inviata ad altri.
- **Accedere è facoltativo.** Kanso funziona senza: l'accesso serve solo a vedere in libreria i
  giochi che possiedi e non hai installato.
- **«Esci» cancella tutto** ciò che riguarda quell'account: la voce in `accounts.json` e, per
  gli store con finestra di accesso, il profilo del browser di quella finestra. I giochi già in
  libreria restano.

## Store per store

Tutti i file stanno nella cartella dati di Kanso, `%LOCALAPPDATA%\com.kanso.launcher`.

| Store | Dove accedi | Che cosa resta sul PC | Dove | A chi viene inviato |
|---|---|---|---|---|
| **Epic Games** | Nel tuo browser, sulla pagina di Epic; alla fine incolli in Kanso il codice mostrato | Il token di rinnovo della sessione | `accounts.json`, cifrato | Solo ai servizi di Epic (`*.ol.epicgames.com`, `*.on.epicgames.com`) |
| **GOG** | Nel tuo browser, sulla pagina di GOG; alla fine incolli l'indirizzo della pagina | Il token di rinnovo della sessione | `accounts.json`, cifrato | Solo a GOG (`auth.gog.com`, `embed.gog.com`) |
| **Ubisoft Connect** | In una finestra di Kanso che mostra la pagina di Ubisoft | I ticket di sessione rilasciati da Ubisoft | `accounts.json`, cifrato | Solo a Ubisoft (`public-ubiservices.ubi.com` per la sessione e l'elenco dei giochi; `api-uplayplusvault.ubi.com` per i giochi dell'abbonamento Ubisoft+) |
| **EA app** | In una finestra di Kanso che mostra la pagina di EA | I cookie del sito di EA | `signin\ea\` (profilo del browser di quella finestra) | Solo a EA: i cookie li usa la finestra con `accounts.ea.com`; il token temporaneo che EA rilascia va a `service-aggregation-layer.juno.ea.com` e non viene salvato |
| **Battle.net** | In una finestra di Kanso che mostra la pagina di Battle.net | I cookie del sito di Battle.net | `signin\battlenet\` (profilo del browser di quella finestra) | Solo a Blizzard: l'elenco dei giochi viene letto nella finestra stessa, da `account.battle.net` |

- **Cifrato** significa protetto con la *Data Protection API* di Windows per il tuo utente: il
  file non serve a nulla su un altro PC o a un altro utente dello stesso PC
  ([`secrets.rs`](../src-tauri/crates/kanso-core/src/secrets.rs)).
- In `accounts.json` c'è anche il nome dell'account, per mostrare «Collegato come ...».
- Per EA e Battle.net il file contiene solo l'annotazione che qualcuno ha effettuato l'accesso:
  questi store non rilasciano nulla da conservare
  ([`accounts.rs`](../src-tauri/crates/kanso-core/src/accounts.rs)).
- Steam e Xbox non hanno accesso in Kanso: la libreria di Steam viene letta dai file locali del
  client, senza credenziali.

**Che cosa viene letto dall'account.** Solo l'elenco dei giochi: titolo, identificativo, e la
copertina dove lo store la dà. Per Ubisoft anche le piattaforme su cui possiedi ogni gioco: un
gioco entra in libreria solo se lo possiedi per PC, e quelli che hai soltanto su console
(PlayStation, Xbox, Switch) restano fuori. Più i giochi dell'abbonamento Ubisoft+ che hai in
libreria. Sotto ogni account collegato le Impostazioni dicono quanti giochi sono stati letti,
quanti vengono da un abbonamento e quanti sono stati lasciati fuori. Nient'altro viene chiesto:
né amici, né acquisti, né dati di pagamento.

## Che cosa Kanso legge dai client installati, senza alcun accesso

Oltre agli account, Kanso legge dal disco ciò che i client degli store già sanno dei tuoi
giochi: l'elenco di Steam, il catalogo di Epic, il database di GOG Galaxy e la cache di
Ubisoft Connect. Sono letture locali: **nessuna richiesta di rete, nessuna credenziale, nulla
viene scritto o modificato** in quei file.

**La cache di Ubisoft Connect merita due righe in più**, perché è l'unico posto in cui si trova
l'elenco completo dei giochi che puoi installare su questo PC, compresi quelli che ti arrivano
da Ubisoft+ o da Xbox Game Pass (l'elenco dell'account su internet non li contiene).

- I client recenti tengono quell'elenco in `configurations.vdat`, cifrato (AES-256-GCM). La
  chiave sta accanto, in `%LOCALAPPDATA%\Ubisoft Game Launcher\store\`, ed è a sua volta
  protetta **da Windows per il tuo utente** (DPAPI: la stessa protezione che Kanso usa per i
  propri token). Windows la consegna a qualunque programma avviato da te, e a nessun altro.
- Kanso chiede a Windows quella chiave, apre l'elenco **in memoria** e ne legge i giochi: nome,
  numero del gioco nel launcher, nome del file della copertina. In quel file non ci sono
  password, sessioni o dati di pagamento.
- La chiave viene usata una volta e scartata: **non viene salvata, mostrata né inviata**. Kanso
  non scrive nulla nella cartella di Ubisoft Connect e non aggira alcuna protezione dei giochi:
  legge un elenco che è tuo, sul tuo PC, con la chiave che Windows custodisce per te.
- Se preferisci che Kanso non lo faccia, il file sorgente è uno solo
  ([`ubisoft_cache.rs`](../src-tauri/crates/kanso-core/src/scanner/ubisoft_cache.rs)) e la
  lettura è indipendente dall'accesso: senza Ubisoft Connect installato non succede nulla.

**Xbox Game Pass su Battle.net: lo dici tu.** Il sito di Battle.net non dice se un account ha
Game Pass e non elenca i giochi che Game Pass rende installabili finché non li avvii una volta.
Kanso non accede a Xbox. Per questo nelle Impostazioni c'è la spunta *«Ho Xbox Game Pass
collegato a Battle.net»*: se la metti, Kanso aggiunge come «non installati» i giochi di un
elenco incluso nel programma ([`battlenet_account.rs`](../src-tauri/crates/kanso-core/src/battlenet_account.rs)),
preso dalla pagina di supporto di Blizzard. È solo una tua dichiarazione salvata in
`config.json`: **non parte alcuna richiesta e nessuno viene informato**. Se un gioco non è
compreso nel tuo piano, Battle.net semplicemente non te lo farà installare.

## La finestra di accesso (Ubisoft, EA, Battle.net)

Questi tre store non hanno un accesso "con codice da incollare": la sessione nasce e resta
dentro la loro pagina. Per questo Kanso apre **la pagina dello store** in una finestra a parte.
Che cosa fa e che cosa non fa quella finestra è scritto, come contratto, in
[`signin.rs`](../src-tauri/crates/kanso-core/src/signin.rs); il codice che la apre è
[`signin_window.rs`](../src-tauri/src/signin_window.rs). In concreto:

1. **Mostra la pagina di accesso dello store, e basta.** Kanso non esegue nulla sul modulo di
   accesso: nessuno script suo, nessun tasto osservato, nessun contenuto letto, nessuna
   richiesta della pagina intercettata. Password e codici vanno dalla pagina allo store, come
   in un normale browser. Per completezza, due cose che invece accadono:
   - Kanso **vede l'indirizzo** di ogni pagina che la finestra apre: gli serve per capire
     quando lo store ha finito e per rifiutare ciò che non è `https`. Gli indirizzi non vengono
     salvati; in un messaggio d'errore possono comparire solo sito e percorso (mai la parte
     dopo il `?`, che può contenere codici);
   - **Tauri**, il framework con cui Kanso è costruito, aggiunge a ogni finestra il proprio
     codice di avvio. Non legge la pagina, e i comandi che servirebbe a chiamare sono rifiutati
     a questa finestra (punto 3).
2. **Dopo che lo store ti ha fatto accedere**, Kanso esegue *uno* di due script, entrambi di
   una riga e scritti per esteso in `signin.rs` (`SCRIPTS`). Mai mentre stai accedendo, e mai
   su una pagina in cui hai scritto qualcosa:
   - per Ubisoft, lo script legge le tre voci in cui il sito stesso tiene la sessione appena
     ottenuta. Non succede nulla finché Ubisoft non manda la finestra al suo indirizzo
     `connect.ubisoft.com/ready`, cosa che fa solo ad accesso completato (lì non c'è niente da
     leggere: oggi risponde «pagina non trovata»). A quel punto Kanso porta la finestra su una
     pagina vuota dello stesso sito, `connect.ubisoft.com/change_domain/`, e legge lì. Solo se
     quella pagina non è leggibile, ricarica da capo l'indirizzo di accesso (una pagina nuova,
     in cui non è stato scritto nulla) e legge **una volta sola**, nell'istante in cui compare;
   - per EA e Battle.net, su un indirizzo che contiene solo la risposta dello store in formato
     JSON (il token temporaneo di EA; l'elenco dei giochi di Battle.net): legge il testo della
     pagina.

   Nessun altro script può essere eseguito: la finestra rifiuta tutto ciò che non è in quell'elenco,
   e un test controlla che quei due script non tocchino moduli, campi, cookie o rete
   (`the_scripts_only_read_what_the_store_handed_out`).
3. **È isolata dal resto di Kanso.** Ha un profilo del browser tutto suo, uno per store
   (`signin\<store>\`): i suoi cookie non sono quelli dell'app né di un altro store. Può aprire
   solo indirizzi `https` (anche nelle finestrelle che la pagina dello store apre da sé, per
   esempio «accedi con...»), non scarica file, e non può chiamare nessun comando di Kanso: i
   comandi sono concessi alla sola finestra principale
   ([`capabilities/default.json`](../src-tauri/capabilities/default.json)) e Tauri li rifiuta
   alle pagine che non sono dell'app. La pagina dello store non può quindi leggere impostazioni,
   chiavi API, libreria o file.
   Per **Battle.net**, se nessuno ha effettuato l'accesso il sito non mostra da solo il modulo:
   Kanso porta la finestra all'indirizzo con cui il sito avvia l'accesso e, solo se nemmeno
   quello mostra il modulo, all'indirizzo di uscita del sito (che al modulo conduce; è la strada
   usata da Playnite). Questo avviene soltanto nella finestra in cui stai accedendo, mai in
   quella nascosta e mai dopo che il modulo è comparso; riguarda comunque solo il profilo di
   quella finestra, non il tuo browser né l'app Battle.net.
4. **Per Ubisoft il profilo del browser viene cancellato subito dopo l'accesso**: la sessione
   sta già, cifrata, in `accounts.json`. Per EA e Battle.net il profilo resta, perché *è* la
   sessione; viene cancellato quando esci.
5. **Rinnovo senza disturbare.** Per EA e Battle.net, quando Kanso aggiorna la libreria riapre
   la stessa finestra senza mostrarla, giusto il tempo di chiedere allo store la risposta di cui
   sopra. Se lo store chiede di nuovo le credenziali, Kanso non fa nulla: segna la sessione come
   scaduta e le Impostazioni ti invitano ad accedere di nuovo.

**Restare collegati.** I cookie di EA e Battle.net sopravvivono alla chiusura di Kanso solo se
sulla pagina dello store spunti l'opzione per restare collegato; il ticket di Ubisoft si
rinnova solo con «Remember me». Senza, i giochi già importati restano in libreria, ma per
vedere i nuovi acquisti dovrai accedere di nuovo.

## Con che identità Kanso parla agli store

Nessuno di questi store offre un'API pubblica per la libreria di un utente. Come gli altri
launcher liberi che la mostrano (Heroic, Legendary, Lutris, Playnite), Kanso usa i servizi che
usano i client e i siti degli store stessi, presentandosi con gli identificativi di
applicazione che quei progetti pubblicano nel loro codice (l'id e il segreto del launcher di
Epic e di GOG Galaxy, l'id del sito "Club" di Ubisoft, l'id del sito di EA). **Identificano il
programma, non te**: nel codice di Kanso non c'è nulla che riguardi un utente.

Questi servizi non sono documentati e possono cambiare: se uno store cambia qualcosa,
l'accesso a quello store smette di funzionare finché Kanso non viene aggiornato. In quel caso
Kanso lo dice (indicando su quali pagine si è fermato, senza mai mostrare codici o token) e non
tenta altre strade.

## Come cancellare tutto

- **Un account:** Impostazioni → *Account degli store* → **Esci**.
- **Tutto a mano:** chiudi Kanso e cancella `accounts.json` e la cartella `signin` in
  `%LOCALAPPDATA%\com.kanso.launcher`.
- **Dal lato dello store:** ogni store ha una pagina in cui vedere e revocare le sessioni
  attive; revocarle lì rende inutile qualunque cosa sia rimasta sul PC.

## Come verificare da te

| Che cosa | Dove guardare |
|---|---|
| Il contratto della finestra e i due script | [`kanso-core/src/signin.rs`](../src-tauri/crates/kanso-core/src/signin.rs) |
| La finestra: profilo separato, solo `https`, nessun download | [`src-tauri/src/signin_window.rs`](../src-tauri/src/signin_window.rs) |
| I passi di ogni store, e i test che li percorrono | `ubisoft_account.rs`, `ea_account.rs`, `battlenet_account.rs` in [`kanso-core/src/`](../src-tauri/crates/kanso-core/src/) |
| Accesso con codice (Epic, GOG) | `epic_account.rs`, `gog_account.rs` |
| Che cosa finisce in `accounts.json` e come | [`accounts.rs`](../src-tauri/crates/kanso-core/src/accounts.rs), [`secrets.rs`](../src-tauri/crates/kanso-core/src/secrets.rs) |
| I comandi dell'app (accedi, esci, aggiorna) | [`src-tauri/src/accounts.rs`](../src-tauri/src/accounts.rs) |
| I permessi delle finestre | [`src-tauri/capabilities/default.json`](../src-tauri/capabilities/default.json) |

Gli indirizzi contattati per gli account sono tutti scritti come costanti in quei file: non ce
ne sono altri.

---

## In short (English)

- **Your password never passes through Kanso.** You type it on the store's page (in your
  browser, or in a window showing the store's page) and it reaches the store only. Kanso does
  not read it, save it or send it to anyone. The same goes for two-factor codes.
- **Kanso has no server.** There is no Kanso service anything could be sent to: no telemetry,
  no statistics, no "cloud" copy. Everything stays on your PC.
- **Only the session stays on the PC**: what the store issues after the sign-in (a token, or
  the cookies of the store's site). It is used for one thing: asking *that same store* for the
  list of your games. It is never sent to anyone else.
- **Signing in is optional.** Kanso works without it: it only adds to the library the games you
  own and have not installed.
- **"Sign out" deletes everything** about that account: its entry in `accounts.json` and, for
  the stores with a sign-in window, that window's browser profile. Games already in the library
  stay.

## Store by store

All files are in Kanso's data folder, `%LOCALAPPDATA%\com.kanso.launcher`.

| Store | Where you sign in | What stays on the PC | Where | Whom it is sent to |
|---|---|---|---|---|
| **Epic Games** | In your browser, on Epic's page; you paste the code shown at the end | The token that renews the session | `accounts.json`, encrypted | Epic's services only (`*.ol.epicgames.com`, `*.on.epicgames.com`) |
| **GOG** | In your browser, on GOG's page; you paste the address of the last page | The token that renews the session | `accounts.json`, encrypted | GOG only (`auth.gog.com`, `embed.gog.com`) |
| **Ubisoft Connect** | In a window of Kanso showing Ubisoft's page | The session tickets Ubisoft issued | `accounts.json`, encrypted | Ubisoft only (`public-ubiservices.ubi.com` for the session and the list of games; `api-uplayplusvault.ubi.com` for the games of the Ubisoft+ subscription) |
| **EA app** | In a window of Kanso showing EA's page | The cookies of EA's site | `signin\ea\` (that window's browser profile) | EA only: the window uses the cookies with `accounts.ea.com`; the short-lived token EA issues goes to `service-aggregation-layer.juno.ea.com` and is never saved |
| **Battle.net** | In a window of Kanso showing Battle.net's page | The cookies of Battle.net's site | `signin\battlenet\` (that window's browser profile) | Blizzard only: the list of games is read in the window itself, from `account.battle.net` |

- **Encrypted** means protected with the Windows *Data Protection API* for your user: the file
  is useless on another PC or to another user of the same PC (`secrets.rs`).
- `accounts.json` also holds the account's name, to show "Signed in as ...".
- For EA and Battle.net the file only notes that someone signed in: these stores issue nothing
  to keep.
- Steam and Xbox have no sign-in in Kanso: the Steam library is read from the client's local
  files, with no credentials.

**What is read from the account.** Only the list of games: title, identifier, and the cover
where the store gives one. For Ubisoft also the platforms you have each game on: a game joins
the library only if you own it for PC, and the ones you have on a console only (PlayStation,
Xbox, Switch) are left out. Plus the games of the Ubisoft+ subscription that are in your
library. Under each signed-in account, Settings say how many games were read, how many come
from a subscription and how many were left out. Nothing else is asked for: no friends, no
purchases, no payment data.

## What Kanso reads from the installed clients, with no sign-in at all

Besides the accounts, Kanso reads from disk what the stores' clients already know about your
games: Steam's list, Epic's catalog, GOG Galaxy's database and Ubisoft Connect's cache. These
are local reads: **no network request, no credentials, nothing is written or changed** in
those files.

**Ubisoft Connect's cache deserves a few more lines**, because it is the only place that holds
the full list of the games you can install on this PC, including the ones Ubisoft+ or Xbox
Game Pass give you (the account's list on the internet does not have them).

- Recent clients keep that list in `configurations.vdat`, encrypted (AES-256-GCM). The key is
  next to it, in `%LOCALAPPDATA%\Ubisoft Game Launcher\store\`, itself protected **by
  Windows for your user** (DPAPI: the very protection Kanso gives its own tokens). Windows
  hands it to any program you start, and to nobody else.
- Kanso asks Windows for that key, opens the list **in memory** and reads the games from it:
  name, the game's number in the launcher, the cover's file name. There are no passwords,
  sessions or payment data in that file.
- The key is used once and dropped: **it is never stored, shown or sent**. Kanso writes
  nothing in Ubisoft Connect's folder and gets around no protection of the games: it reads a
  list that is yours, on your PC, with the key Windows keeps for you.
- If you would rather Kanso did not, it is a single source file
  ([`ubisoft_cache.rs`](../src-tauri/crates/kanso-core/src/scanner/ubisoft_cache.rs)) and the
  read does not depend on signing in: without Ubisoft Connect installed nothing happens.

**Xbox Game Pass on Battle.net: you say so.** Battle.net's site does not say whether an account
has Game Pass, and does not list the games Game Pass makes installable until they have been
started once. Kanso does not sign in to Xbox. Hence the *"I have Xbox Game Pass linked to
Battle.net"* tick in Settings: with it, Kanso adds as "not installed" the games of a list that
ships with the program ([`battlenet_account.rs`](../src-tauri/crates/kanso-core/src/battlenet_account.rs)),
taken from Blizzard's support page. It is only your own statement, saved in `config.json`: **no
request is made and nobody is told**. If your plan does not include a game, Battle.net will
simply not let you install it.

## The sign-in window (Ubisoft, EA, Battle.net)

These three stores have no sign-in that ends on a code to paste: the session is born and stays
inside their page. So Kanso opens **the store's own page** in a separate window. What that
window may and may not do is written down, as a contract, in `kanso-core/src/signin.rs`; the
code that opens it is `src-tauri/src/signin_window.rs`.

1. **It shows the store's sign-in page, and that is all.** Kanso runs nothing on the sign-in
   form: no script of its own, no key press observed, no content read, no request of the page
   intercepted. Passwords and codes go from the page to the store, as in any browser. For
   completeness, two things that do happen:
   - Kanso **sees the address** of each page the window opens: it needs it to tell when the
     store is done and to refuse anything that is not `https`. Addresses are not saved; an
     error message may show the site and path only (never the part after the `?`, which may
     carry codes);
   - **Tauri**, the framework Kanso is built with, adds its own startup code to every window.
     It does not read the page, and the commands it would be used to call are refused to this
     window (point 3).
2. **After the store has signed you in**, Kanso runs *one* of two scripts, both one line long
   and written out in `signin.rs` (`SCRIPTS`). Never while you are signing in, and never on a
   page you have typed into:
   - for Ubisoft, the script reads the three entries in which the site itself keeps the
     session it just obtained. Nothing happens until Ubisoft sends the window to its
     `connect.ubisoft.com/ready` address, which it does only once the sign-in is complete
     (there is nothing to read there: today it answers "page not found"). Kanso then takes the
     window to a blank page of the same site, `connect.ubisoft.com/change_domain/`, and reads
     there. Only if that page cannot be read does it load the sign-in address afresh (a new
     page, in which nothing has been typed) and read **once**, the moment it shows;
   - for EA and Battle.net, on an address that holds nothing but the store's answer in JSON
     (EA's short-lived token; Battle.net's list of games): it reads the text of the page.

   No other script can run: the window refuses anything not on that list, and a test checks
   that those two scripts touch no form, field, cookie or network call.
3. **It is cut off from the rest of Kanso.** It has a browser profile of its own, one per store
   (`signin\<store>\`): its cookies are neither the app's nor another store's. It may open
   `https` addresses only (also in the small windows the store's page opens by itself, such as
   "sign in with..."), downloads nothing, and cannot call any command of Kanso: commands
   are granted to the main window alone (`capabilities/default.json`) and Tauri refuses them to
   pages that are not the app's own. The store's page therefore cannot read settings, API keys,
   the library or files.
   For **Battle.net**, when nobody is signed in the site does not show its form by itself:
   Kanso takes the window to the address the site starts a sign-in from and, only if that
   shows no form either, to the site's sign-out address (which leads to the form; it is the
   way Playnite takes). This happens only in the window you are signing in through, never in
   the unseen one and never after the form has shown; in any case it concerns that window's
   profile alone, not your browser nor the Battle.net app.
4. **For Ubisoft the browser profile is deleted right after the sign-in**: the session is
   already in `accounts.json`, encrypted. For EA and Battle.net the profile stays, because it
   *is* the session; it is deleted when you sign out.
5. **Renewal without getting in the way.** For EA and Battle.net, when Kanso refreshes the
   library it reopens the same window without showing it, just long enough to ask the store for
   the answer described above. If the store asks for credentials again, Kanso does nothing: it
   marks the session as expired and Settings invites you to sign in again.

**Staying signed in.** The cookies of EA and Battle.net outlive Kanso only if you tick the
option to stay signed in on the store's page; Ubisoft's ticket renews only with "Remember me".
Without it, the games already imported stay in the library, but to see new purchases you will
have to sign in again.

## The identity Kanso uses with the stores

None of these stores offers a public API for a user's library. Like the other free launchers
that show it (Heroic, Legendary, Lutris, Playnite), Kanso uses the services the stores' own
clients and sites use, with the application identifiers those projects publish in their code
(the id and secret of Epic's launcher and of GOG Galaxy, the id of Ubisoft's "Club" site, the
id of EA's site). **They name the program, not you**: nothing in Kanso's code concerns a user.

These services are undocumented and may change: if a store changes something, signing in to
that store stops working until Kanso is updated. Kanso then says so (naming the pages it
stopped on, never showing codes or tokens) and tries nothing else.

## Deleting everything

- **One account:** Settings → *Store accounts* → **Sign out**.
- **By hand:** close Kanso and delete `accounts.json` and the `signin` folder in
  `%LOCALAPPDATA%\com.kanso.launcher`.
- **On the store's side:** every store has a page to see and revoke active sessions; revoking
  them there makes whatever is left on the PC useless.
