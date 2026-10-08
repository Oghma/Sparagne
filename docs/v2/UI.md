# Sparagne v2 — UI

> Deciso il 2026-09-10 sui mockup forniti (libro mastro + riepilogo). Sostituisce
> la griglia di dashboard di `DISTILLATO_V1.md` §3.4, che è stata scartata: le
> formule di §3.5 restano, la disposizione no.

## 1. Idea in una frase

Un foglio di calcolo per le righe, un terminale finanziario per i numeri: una
sola finestra scura, con il mese come unità di lettura e gli aggregati sempre
a fianco delle righe. Dal 2026-10-07 ha l'aspetto di un terminale finanziario
moderno e non più di un'interfaccia a testo: SF Pro con cifre tabulari al
posto del monospazio, angoli arrotondati, etichette in minuscolo (§5).

## 2. Le viste

La finestra disegna da sé la barra in alto (44 pt, al posto di barra titolo e
toolbar, che non ci sono più), mostra la vista scelta e in fondo ha le schede,
come i fogli di Excel: `Riepilogo · Mastro · Ricorrenze · Setup`, che si
cambiano con un click o con ⌘1–⌘4 (menu Vista). Il riepilogo è la vista di
apertura; `-SparagneTab summary|ledger|recurring|setup` apre la finestra su
un'altra, per un solo avvio. (Fino al 2026-09-11 le viste erano tre,
`MASTRO · RIEPILOGO · ANNO`: il riepilogo mensile è stato assorbito dal
pannello destro del mastro e ANNO dal nuovo riepilogo. Fino al 2026-10-07
uno switcher in barra titolo, `RIEPILOGO · MASTRO · SETUP`, sceglieva la
vista, il titolo diceva "vault — vista — anno" e un banner avvisava delle
ricorrenze dovute: la barra in alto e le schede li hanno sostituiti.)

**La barra in alto** (`Views/Chrome/TopBar.swift`, `App/WindowChrome.swift`),
da sinistra: i semafori della finestra, spostati dentro la barra; il
**selettore del vault** (l'iniziale in un riquadro, il nome, un menu con gli
altri vault e le azioni di §2.4) e, se l'app gira su un database di prova, il
badge **DEMO**; solo in Riepilogo e Mastro lo **stepper del mese**
`‹ Ottobre 2026 ›` con "Oggi" quando il mese a schermo non è quello corrente
(⌥← e ⌥→ lo muovono dal menu Mastro); a destra, solo nel Mastro, la ricerca
"Cerca nel mese" (⌘F), poi la pillola **"N da confermare"** (§2.5), il
bottone **"Aggiungi ⌘K"** e la **pillola di sync**. Le aree vuote della
barra trascinano la finestra.

**La pillola di sync** (`Views/Chrome/SyncPill.swift`,
`Model/SyncPillState.swift`) dice dove sta il sync: sincronizzato, in corso,
N in attesa, offline, errore, N rifiutate, sessione scaduta, solo locale
(nessun account, e anche il database di prova). Vince lo stato più urgente,
nell'ordine rifiutate, sessione scaduta, solo locale, errore, offline, in
corso, in attesa, sincronizzato. Un click apre un popover con la frase dello
stato, per un account il server, l'utente, l'ultimo sync e le modifiche in
attesa, e i bottoni che servono: Sincronizza ora, Account e server… (⌘,),
Rivedi (le modifiche rifiutate), Collega un server….

**Le schede** (`Views/Chrome/SheetTabBar.swift`, 27 pt) sono parole semplici,
la scheda attiva sottolineata in ambra; Ricorrenze porta accanto il numero dei
periodi da confermare, lo stesso della pillola. A destra di ogni scheda sta la
**riga di stato** del suo foglio:

| Scheda | Riga di stato |
|---|---|
| Riepilogo | Risparmio dell'anno · Totale · salvato alle … |
| Mastro | Media · Conteggio · Somma delle righe a schermo, o `Selezione: Media …` quando se ne scelgono due o più; poi salvato alle … |
| Ricorrenze | Attive · Archiviate · Uscite fisse al mese ≈ · Entrate fisse ≈ · salvato alle … |
| Setup | Wallet · Buste · Categorie (le archiviate non contano) · salvato alle … |

### 2.1 MASTRO

```
(●●●) [C] Casa ▾ DEMO │ ‹ Ottobre 2026 › Oggi   Cerca nel mese ⌘F  ● 1 da confermare  + Aggiungi ⌘K  ● Sincronizzato
[Uscite|Entrate] [Tutti|Elisa|Matteo] │ (Trasferimenti) (Eliminate) (Colonna wallet)
 #   Data    Busta   Categoria   Descrizione                 Persona      Importo ┃ ┌ Risparmio di ottobre ┐
 02  01 gio  Cash    Casa        mutuo                       Matteo        950,00 ┃ │ 1.250,00 €           │
 03  02 ven  Cash    Computer    Claude                      Matteo         74,33 ┃ │ 66% delle entrate    │
 …                                                                                ┃ ├ Per persona ─────────┤
 ⟳   05 mar  Cash    Svago       Netflix [da confermare]  Salta Registra   12,99 ┃ ├ Uscite per categoria ┤
     29 ott  Busta   Categoria   descrizione…               Persona               ┃ └ Risparmio · 12 mesi ─┘
 Riepilogo  Mastro  Ricorrenze 1  Setup              Media 47,69 · Conteggio 13 · Somma 619,94 · salvato alle 12:04
```

- `#` è l'ordinale della riga nel mese, non un id: la tabella si legge come un
  foglio. Ordine crescente per data, poi per id.
- L'ultima riga è sempre vuota ed è l'inserimento: si compila da sinistra a
  destra con ⇥ e si salva con ↩. Nessun pulsante "aggiungi".
- Ogni cella delle righe esistenti è editabile in posto; ↩ emette un
  `UpdateTransaction` con i soli campi cambiati, esc ripristina. Lasciare la
  riga (⇥ oltre IMPORTO, click altrove) salva come in un foglio di calcolo.
- **Barra dei filtri** (`Views/Ledger/FilterBar.swift`): sopra la griglia,
  due controlli segmentati, la direzione (Uscite / Entrate) e la persona
  (Tutti e un segmento per autore, solo se gli autori sono più d'uno), poi tre
  chip per ciò che di solito è spento: Trasferimenti, Eliminate, Colonna wallet
  (tratteggiato da spento, pieno da acceso). I chip e le voci del menu (⌘⇧T,
  ⌘⇧V, ⌘⇧W) sono lo stesso valore. Mese e ricerca stanno nella barra in alto.
- **La griglia è un foglio a righe**: intestazioni da 26 pt e righe da 24 pt
  con filetti, colonne `#`, Data (giorno e giorno della settimana), Busta,
  Categoria, Descrizione, Wallet se acceso, Persona, Importo, e dopo Importo
  una corsia stretta (28 pt) per il cestino, tenuta su ogni riga anche vuota
  così le colonne non si spostano; etichette in minuscolo, cifre tabulari.
- **Ricorrenze dovute nella griglia**: i periodi dovuti del mese a schermo
  stanno fra le righe, alla loro data, come righe tratteggiate con l'icona
  delle ricorrenze e il tag "da confermare"; **Salta** e **Registra** nella
  cella Descrizione (non in un vault in sola lettura). Non sono righe: non si
  aprono, non si scelgono e non entrano nella riga di stato. Seguono il filtro
  direzione (un modello d'entrata sta sotto Entrate) e la ricerca; non hanno
  autore, quindi la persona non li filtra. Nulla si scrive finché non si
  preme uno dei due bottoni (`Model/LedgerLines.swift`).
- **Riga di stato**: la media, il conteggio e la somma delle righe a schermo,
  o di quelle scelte quando sono due o più, come la barra di stato di un
  foglio di calcolo (`Model/SheetStats.swift`). Righe eliminate e
  trasferimenti non contano, un rimborso toglie. I totali del mese sono il pannello destro.
- **Pannello destro** (300 pt, `Views/Ledger/SummaryPanel.swift`): quattro
  card, la più importante per prima. Il **risparmio del mese** in grande, con
  il tag "in corso" se il mese è quello corrente, la quota sulle entrate, la
  differenza col mese prima (▲ o ▼) e sotto entrate e uscite; **per persona**
  (entrate, uscite per busta, risparmio, una colonna per autore); le **uscite
  per categoria** (le sei maggiori, con quota e barra; la quota è su ciò che
  le categorie hanno speso, così un rimborso del mese scorso non porta la
  somma oltre il 100%); il **risparmio degli ultimi 12 mesi**.
- I trasferimenti non stanno né in USCITE né in ENTRATE: spostano soldi senza
  guadagnarli o spenderli. Il menu Mastro li aggiunge alla lista corrente
  (⌘⇧T), come fa con le eliminate (⌘⇧V).
- **Selezione** (2026-09-23): ⌘-click aggiunge o toglie una riga,
  ⇧-click prende l'intervallo, ⌘A tutte le righe visibili quando nessuna cella
  è in modifica; le righe scelte hanno la tinta dell'accento e un click semplice
  apre ancora la riga in modifica. Con due o più righe compare la barra della
  selezione: **Elimina N righe** (un solo toast, un solo `execute_batch` alla
  scadenza) e **Imposta categoria…** (un solo `execute_batch` di
  `UpdateTransaction`); ⌫/⌦ eliminano la selezione, esc la toglie. Righe
  eliminate e trasferimenti si possono scegliere ma le azioni li saltano. La
  selezione si svuota cambiando mese, vault o filtro.
- **Eliminare una riga** (dal 2026-10-07): la riga sotto il puntatore mostra
  in fondo un cestino in `text3`, rosso sotto il puntatore; un click la
  elimina. Con nessuna riga scelta e nessuna cella in modifica, ⌫ o ⌦
  eliminano la riga sotto il puntatore (`DeleteTarget` in
  `Model/LedgerModel.swift`); con righe scelte eliminano quelle; dentro una
  cella scrivono come sempre. Tutte le strade (cestino, ⌫, menu contestuale
  Elimina, azione VoiceOver) passano dallo stesso toast: per 5 secondi
  **Ripristina** la rimette. Il cestino non compare in sola lettura, sulla
  riga in modifica, sulla riga vuota, sui periodi dovuti né su una riga già
  eliminata; un trasferimento da solo si elimina, come dal menu. Nel registro
  del core niente sparisce: eliminare emette un `VoidTransaction`, la riga
  resta nella cronologia, barrata quando il chip Eliminate è acceso, e fuori
  da ogni totale. "Elimina" ha preso il posto di "Annulla", che era anche
  Annulla dei dialoghi e del menu Modifica.
- **Annulla e ripeti** (⌘Z, ⇧⌘Z, menu Modifica, sull'`UndoManager` della
  finestra, `Model/LedgerHistory.swift`): una modifica di cella (si riscrivono
  i valori grezzi di prima: `""` per "senza categoria" e per la nota vuota),
  una riga aggiunta (dalla griglia o dal quick-add: annullarla la elimina
  subito, ripeterla la riaggiunge con un id nuovo), una categoria impostata in
  blocco (un passo solo) e l'eliminazione in attesa sul toast (⌘Z la ferma).
  Un'eliminazione già scritta non si disfa, perché non esiste un comando
  inverso:
  quando il toast scade il suo passo, e quelli sulle stesse righe, lasciano la
  pila. Cambiare vault svuota la pila. Dentro una cella ⌘Z disfa ancora la
  digitazione.
- **Categoria**: mentre si scrive nella cella CATEGORIA (e in Imposta
  categoria…) sotto la cella compare un elenco: prima i nomi che cominciano
  così, poi gli alias (alias → categoria), poi i nomi che lo contengono; in
  ogni gruppo prima le categorie usate di recente (`recent_usage`, 90 giorni),
  senza distinguere maiuscole e accenti. ↑↓ scorrono, ↩ o ⇥ scelgono, esc chiude
  l'elenco (un secondo esc annulla la modifica). Nella riga vuota, scritta la
  nota con la categoria vuota, la categoria suggerita dallo storico
  (`suggest_categories`) compare come segnaposto e si salva se la cella resta
  vuota; nel quick-add compare come suggerimento sotto la riga e solo ⇥ la
  scrive come `#Categoria` (per un nome di una parola sola).
- **Aggiunta rapida (⌘K)**: un riquadro arrotondato sotto la barra, sopra le
  prime righe. Sotto il campo la riga letta compare a **chip**, uno per campo
  che scriverà (tipo, importo, categoria, busta, wallet, quando), con la parola
  piccola davanti; un valore che la riga non dice e il default riempie
  (Senza categoria) è più chiaro. Chi ha scritto una riga che si legge in modo
  diverso da come voleva lo vede prima di ↩. Con `>` il riquadro elenca i
  comandi (§6). I chip li compone `Model/QuickAddTokens.swift`; il parsing
  resta del core.
- **VoiceOver**: ogni riga è una frase (data, importo col segno, categoria,
  nota, busta, persona, eliminata) con le azioni Modifica, Duplica, Elimina e
  Seleziona e il tratto "selezionata"; celle, intestazioni, riga nuova, mese e
  segmenti dei filtri hanno un nome. Un periodo dovuto si legge come una
  frase, con le azioni Registra e Salta.

### 2.2 RIEPILOGO

> Ridisegnato il 2026-09-11 sul foglio Excel "Riepilogo" fornito
> dall'utente: l'anno fino al mese a schermo, con le definizioni sue.

```
2026  fino a ottobre
┌ Entrate · ottobre ┐ ┌ Uscite · ottobre ┐ ┌ Risparmio · ottobre ┐ ┌ Tasso di risparmio ┐
│ 9.200,00          │ │ 3.100,00         │ │ 6.100,00            │ │ 66%                │
│ 9.000,00 a sett.  │ │ 2.800,00 a sett. │ │ ▲ 180,14 su settem. │ │ 64% da inizio anno │
└───────────────────┘ └──────────────────┘ └─────────────────────┘ └────────────────────┘
Fondi  buste con un tetto
┌ Fondo emergenza ───────────────┐ ┌ Fondo varie ───────────────────┐
│ 29.931,00      di 30.000 · 99,8% │ │ 4.611,00        di 5.000 · 92,2% │   una card con barra
│ ████████████████████████████░ │ │ ██████████████████████░░ │   per busta con tetto
└────────────────────────────────┘ └────────────────────────────────┘
Mese per mese                          Totale = risparmio + fondo cassa − uscite dei fondi
Mese          Entrate  Uscite  Risparmio  Fondo cassa  Uscite fondi   Totale   Elisa   Matteo
Inizio anno                                                       32.221,80 14.275,60 17.946,20
Gennaio        9.200    3.100     6.100    32.221          1.500     36.934   18.100  18.834
…
Ottobre [in corso]
2026 (le somme)
┌ Cash flow ────────────────────────┐ ┌ Fondo cassa · totale a fine mese ┐
│ entrate / uscite / risparmio      │ │ linea del TOTALE, mese per mese  │
└───────────────────────────────────┘ └──────────────────────────────────┘
```

In cima, sotto l'anno, le quattro card del primo riepilogo (richieste di nuovo
il 2026-09-12), ridisegnate come **KPI** il 2026-10-07: entrate, uscite,
risparmio e tasso del mese a schermo, col nome del mese nel titolo perché non
si leggano come valori dell'anno, e sotto ciascuna un confronto, col mese
prima (entrate, uscite, risparmio con ▲ o ▼) o con l'anno finora (il tasso).
Solo il risparmio è colorato: le uscite sono l'andamento normale di un mese,
non un allarme. Un tasso senza entrate è un trattino, mai uno 0% finto.

Definizioni (dell'utente, 2026-09-11), per il mese `m`:

| Colonna | Definizione |
|---|---|
| ENTRATE | entrate del mese, escluse le aperture dei wallet (categoria di sistema `Opening`) |
| USCITE | uscite nette del mese sulle buste **senza tetto** (Cash, Non allocato) |
| RISPARMIO | ENTRATE − USCITE |
| USCITE FONDI | uscite nette del mese sulle buste **con tetto** (i "fondi": casa, varie, emergenza…) |
| FONDO CASSA | TOTALE del mese prima, più le aperture di wallet del mese; a gennaio è tutto ciò che è successo prima dell'anno |
| TOTALE | RISPARMIO + FONDO CASSA − USCITE FONDI, cioè il saldo complessivo dei wallet a fine mese |
| colonne persona | il TOTALE di ciascuna persona, stessa formula sulle sue sole righe |

- Le persone non sono fisse: una colonna per ogni autore del vault, nell'ordine
  del core. Un vault con una persona ha una colonna sola e nessun totale
  ripetuto.
- Il mese a schermo (lo stepper è lo stesso del mastro) decide l'anno e il
  "fino a": i mesi dopo quello a schermo, nell'anno a schermo, sono righe
  vuote. Un anno passato è pieno.
- TOTALE è `positive` se non è sceso rispetto al mese prima, `negative` se è
  sceso. Il mese a schermo è evidenziato.
- "Fondo" è una busta con tetto (`netCapped` o `incomeCapped`), il che rende
  la classificazione un dato del vault e non un nome. Una card per ogni busta
  attiva con tetto, con una **barra**: riempimento = saldo/tetto (net) o
  entrate cumulative/tetto (income), e sotto `di tetto · percentuale` e
  "tetto sul saldo" o "tetto sulle entrate". Senza buste con tetto la fila di
  card non c'è.
- La tabella ha una riga **"Inizio anno"** (il TOTALE e i totali per persona
  di prima dell'anno), una riga per mese col tag
  **"in corso"** sul mese a schermo, e in fondo le somme dell'anno. I mesi
  dopo quello a schermo sono trattini opachi (non ancora), non zeri.
- CASH FLOW e FONDO CASSA sono due grafici a linee (Swift Charts, colori
  della serie di §5): il primo entrate, uscite e risparmio per mese, col
  segmento verso il mese in corso tratteggiato (non è finito) e, passando il
  puntatore, una scheda con le tre cifre del mese; il secondo il TOTALE a fine
  mese dall'inizio dell'anno, coi mesi in cui è sceso segnati. Un anno senza
  movimenti dice "Nessun movimento quest'anno" invece di tabella e grafici.
- Nessun filtro persona nel riepilogo: le persone sono colonne.

### 2.3 SETUP

> Aggiunta il 2026-09-12 su richiesta: "sezioni per aggiungere le categorie
> e gli envelope", l'equivalente del foglio "Categorie e flow" dell'Excel.
> Il 2026-09-23 si aggiunge la tabella dei wallet, che prima si vedevano solo
> nella gestione ⌘⇧M. Il 2026-10-07 la vista diventa una pagina di card, con
> la card del vault in cima.

Una pagina di card sul foglio (`Views/Setup/SetupView.swift`): a sinistra la
**card del vault** sopra i wallet e le buste, a destra le categorie; due
colonne quando la finestra ne ha lo spazio (520 pt l'una), una sotto l'altra
altrimenti. Le tabelle hanno lo stile del mastro (stesse celle, stesse
intestazioni, stessa riga vuota in fondo per aggiungere: ⇥ tra i campi, ↩
salva, esc annulla). La tabella dei wallet è alta quanto le sue righe (sono
pochi, niente scroll) e ha una riga Totale. Nessun mese, quindi niente
intestazione mese. Le archiviate stanno in fondo, in `dim`, con "Ripristina"
nel menu contestuale. Gli errori del core passano dallo stesso alert del
mastro.

La **card del vault** (`Views/Setup/VaultCard.swift`): nome, valuta ("non si
cambia"), membri (l'elenco del server per un vault di un account, altrimenti
"Solo su questo Mac") e, a destra del titolo, **Condividi…** per l'owner con un
account. Nome e valuta si leggono e basta: si rinomina dal menu Vault o dal
selettore in alto (§2.4).

Ultima colonna di ogni tabella: nessuna intestazione, vuota a riposo,
mostra l'icona `archivebox`/`tray.and.arrow.up` solo sulla riga sotto il
puntatore — un click manda lo stesso `ArchiveWallet`/`RestoreWallet`,
`ArchiveFlow`/`RestoreFlow` o `ArchiveCategory`/`RestoreCategory` del menu
contestuale, senza aprirlo. Assente sulle righe di sistema (Non allocato,
Opening, Uncategorized) e durante l'editing della riga
(`Views/Setup/WalletTable.swift`, `Views/Setup/EnvelopeTable.swift`,
`Views/Setup/CategoryTable.swift`).

```
┌ Vault ─────────────────────────────────┐ ┌ Categorie ─────────────────────────────────┐
│ Nome        Casa                Condividi… │ │ Nome        Alias                 Righe 90 gg │
│ Valuta      EUR · €   non si cambia      │ │ Casa        [mutuo] [affitto]       4  [📥] │
│ Membri      Elisa, Matteo                │ │ Spesa       [coop] [esselunga]     21      │
└──────────────────────────────────────────┘ │ Opening     system                  —      │
┌ Wallet ────────────────────────────────┐   │ nome…       Simili: Casa                   │
│ Nome                              Saldo  │   └────────────────────────────────────────────┘
│ Conto                      4.120,50      │
│ nome…                      saldo apertura│
│ Totale                     3.810,50      │
└──────────────────────────────────────────┘
┌ Buste ─ un tetto la rende un fondo ────┐
│ Nome        Tetto      Limite Neg. Saldo │
│ Cash        Nessuno         —   no  3.480,20 │
│ Casa        Sul saldo 150.000   no 31.935,00 │
│                                  ▓▓▓░░ │   barra: quanto del tetto è usato
│ nome…       Nessuno ▾  limite…  no  allocazione │
└──────────────────────────────────────────┘
```

| Tabella | Colonne | In posto | Menu contestuale |
|---|---|---|---|
| WALLET | NOME, SALDO (e una riga Totale) | nome → `RenameWallet`; SALDO segue le transazioni e si scrive solo nella riga vuota, dove è il saldo di apertura (negativo per una carta che parte in rosso), che va in Non allocato → `CreateWallet` | archivia (il core vuole saldo zero) / ripristina |
| BUSTE | NOME, TETTO (nessuno / sul saldo / sulle entrate), LIMITE, NEG (sì/no, un click), SALDO con, per una busta con tetto, una barra sottile di quanto del tetto è usato | nome, tipo, tetto, negativo → `UpdateFlow` con i soli campi cambiati; nella riga vuota SALDO è l'allocazione iniziale da Non allocato → `CreateFlow` | archivia / ripristina |
| CATEGORIE | NOME, ALIAS (a riposo chip, in modifica una lista separata da virgole), RIGHE 90 GG (quante righe ha avuto la categoria negli ultimi 90 giorni: `category_totals`, zero in `dim`, per accorgersi di una categoria che non si usa prima di archiviarla; `Model/AppStore+Usage.swift`) | nome → `RenameCategory`; alias → la differenza fra la lista di prima e quella nuova, un `AddAlias`/`RemoveAlias` per voce; nella riga vuota, sotto il nome, "simili: …" suggerisce e non blocca (`similar_categories`) | unisci in… (anteprima con `preview_merge`), archivia / ripristina |

Non allocato e le categorie di sistema (`Opening`, `Uncategorized`) si vedono
ma non si modificano. ⌘⇧C ("Wallet, buste e categorie…") porta qui, come
⌘4; la finestra Categorie separata non c'è più. Vault, ricorrenze e
condivisione restano anche nella gestione ⌘⇧M, che elenca ancora wallet e
buste.

### 2.4 Il vault

> Aggiunto il 2026-09-15: creazione, rinomina e cancellazione del vault.

Tre azioni, raggiungibili da quattro posti che condividono un'implementazione
sola (notifiche del menu Vault, ricevute da `MainWindow`): il menu **Vault**
(Nuovo vault…, Rinomina vault…, Elimina vault…), la palette `>` (stesse voci),
il menu a tendina del vault nella gestione ⌘⇧M (Nuovo…, Rinomina…,
Condividi…, Elimina…) e, dal 2026-10-07, il **selettore del vault nella barra
in alto** (§2): gli altri vault, poi Nuovo vault…, Rinomina…, Condividi…,
Elimina… ed Esci dal vault…, spenti dalle stesse regole (`VaultPermissions`)
del menu e della palette.

| Azione | Foglio | Comando |
|---|---|---|
| Nuovo | l'onboarding (`OnboardingSheet`): nome, primo wallet, saldo di apertura | `CreateVault`, poi un batch con `CreateWallet` e le categorie di partenza (`CreateCategory`, `AddAlias`) |
| Rinomina | `RenameSheet`, come per wallet e buste | `RenameVault` |
| Elimina | `DeleteVaultSheet`: il nome del vault, una frase che dice che vanno via wallet, buste, transazioni e ricorrenze per ogni membro e su ogni dispositivo, Annulla e un "Elimina" distruttivo che ↩ non attiva | `DeleteVault` |

La valuta non si modifica (`ARCH.md` §4). Elimina non compare per un vault
condiviso di cui non si è owner (`SyncEngine.mayDeleteVault`); se arriva lo
stesso, il core risponde `forbidden` e l'alert dice "Non puoi farlo". Dopo la
cancellazione la finestra passa a un altro vault (quello ricordato, altrimenti
il primo) o torna all'onboarding se era l'ultimo; una riga in attesa sul toast
di eliminazione muore col vault senza mandare nulla. Ai membri la
cancellazione arriva col sync (`SYNC.md` §4 punto 6).

**Dal 2026-09-23.** Una quarta azione, **Esci dal vault…**, per un membro
che non è l'owner (menu Vault, palette, gestione): `LeaveVaultSheet` spiega
che le modifiche in attesa partono prima e che la copia locale sparisce, poi
il sync toglie la membership e `forget_vault` il vault (`SYNC.md` §5). Rinomina
compare solo a chi può scrivere, Elimina solo all'owner, Esci solo a un membro
(`SyncEngine+Permissions.swift`). I nomi sono etichette: un nome che l'owner
usa già si può scegliere, con un avviso sotto il campo, e dove due vault
omonimi stanno nello stesso elenco compare l'owner fra parentesi. Il foglio di
cancellazione dice che il vault sparisce da ogni dispositivo di ogni membro man
mano che sincronizzano e che il server ne tiene la storia per la sync. Un vault
in **sola lettura** (ruolo `viewer`) non offre la riga vuota, le celle non si
modificano, Duplica ed Elimina sono spenti, il cestino non compare, il
quick-add risponde con un
messaggio, SETUP non si edita, e `CoreActor` rifiuta comunque ogni scrittura.
Un vault che non è più condiviso con me resta leggibile e la gestione lo
elenca fra i "Non più condivisi con te" con **Rimuovi da questo Mac**. La riga
in attesa sul toast di eliminazione porta il suo vault: se un pull cancella
quel vault il toast sparisce senza avvisi, e chiudere l'app scrive
l'eliminazione prima di uscire.

**Dal 2026-09-28.** Un vault nuovo nasce con 16 categorie, 14 di uscita e 2
di entrata, ognuna di una parola sola così che il quick-add la raggiunga con
`#Nome`, e con qualche alias (`#pizza` finisce in Ristoranti). La lista è
nell'app (`DefaultCategories.swift`), nella lingua in cui l'app mostra le sue
stringhe, italiano o inglese, e il core non ne sa nulla: l'app la manda come
comandi normali nello stesso batch del primo wallet. Cambiarla cambia solo i
vault creati dopo, e il replay di un log vecchio finisce dove finiva. Mancano
di proposito una categoria "Varie", perché c'è già Senza categoria, una
"Risparmio", perché a quello servono le buste, e una "Rimborsi": un rimborso è
un tipo di transazione (`r30 #Salute`) che si netta sulla categoria della
spesa, e una categoria di entrata con quel nome inviterebbe a registrarlo come
entrata, gonfiando entrate e uscite.

### 2.5 Ricorrenze

> Aggiunto il 2026-09-23: dal ridisegno del 2026-09-10 il banner
> "N ricorrenze da confermare" apriva un pannello che non sapeva eseguirle.
> Dal 2026-10-07 sono una scheda, la terza: banner, `DueRecurringSheet` e
> `RecurringPanel` non ci sono più.

La scheda **Ricorrenze** (`Views/Recurring/RecurringTab.swift`, ⌘3) ha a
sinistra tre card in colonna e a destra l'inspector (340 pt):

- **Da confermare**: ogni periodo dovuto, compresi gli arretrati, in ordine di
  data, con importo, tipo, wallet e busta; per ogni periodo **Salta** o
  **Registra**, e **Registra tutte** che manda ogni periodo in un solo
  `execute_batch` (tutto o niente: se uno non passa, per esempio per fondi
  insufficienti in una busta, non si scrive nulla e l'alert lo dice). Un vault
  in sola lettura li mostra senza bottoni. Gli stessi periodi stanno anche
  nel mese del Mastro, alla loro data (§2.1), e la pillola "N da confermare"
  della barra in alto porta qui. Un modello non scrive mai una transazione da
  solo.
- **Prossimi 30 giorni**: una tessera per ogni periodo da domani in poi
  (giorno, titolo, importo col segno), con le uscite e le entrate previste
  della finestra. Sola lettura: un periodo si registra quando è dovuto, mai
  prima. Oggi sta in "Da confermare", e un giorno non si conta in tutte e due.
  Le date le calcola il core (`schedule_occurrences`, sotto).
- **Modelli**: ogni modello in una riga da 26 pt (Attiva, Descrizione,
  Importo, Cadenza, Prossima, Wallet, Busta, Categoria), le archiviate in
  fondo in `dim` con **Ripristina**, una riga vuota per aggiungerne uno. Un
  click su una riga la apre nell'inspector; l'interruttore Attiva mette in
  pausa o riprende il modello sul posto. Le frasi di cadenza sono intere e al
  plurale giusto ("Ogni 2 settimane il lunedì").

L'**inspector** (`Views/Recurring/RecurringInspector.swift`) modifica un
modello o ne scrive uno nuovo: importo e tipo (uscita o entrata), Dove
(wallet, busta, categoria, nota), Quando (frequenza, ogni N, il giorno della
settimana, del mese o dell'anno, inizio, fine) e **Prossime date**, le
prossime quattro, che seguono ogni tasto e che il core calcola anche per un
modello non ancora salvato. **Salva** manda un `UpdateRecurring` con i soli
campi cambiati, Annulla rimette i valori salvati; un modello archiviato si
ripristina prima di modificarlo. L'inspector non offre ciò che
`RecurringPatch` non sa dire: il tipo non cambia una volta creato il modello,
e un modello con un wallet o una busta non torna a "qualsiasi wallet" né a
Non allocato (quelle voci sono spente).

**Limite noto, non ancora affrontato** (2026-10-08). Un modello non si
modifica del tutto: non si può togliergli il wallet o la busta una volta
scelti, né trasformare un'uscita in un'entrata (o viceversa). Esempi: un
abbonamento creato sul Conto che dovrebbe valere per "qualsiasi wallet"; un
"Rimborso spese lavoro" creato per sbaglio come uscita. La causa sta nel core:
`RecurringPatch` dice "imposta questo campo" ma non "svuotalo", e non ha un
campo per il tipo; cambiarlo tocca il formato dei comandi sincronizzati
(`ARCH.md` §4, `SYNC.md`). Oggi la via è archiviare il modello e crearne uno
nuovo, anche con **Duplica** dal menu "…" dell'inspector; la storia dei
periodi già registrati resta sul modello archiviato.

**Nuova ricorrenza…** (menu Vault e palette; dal 2026-09-29, prima erano tre
passaggi) passa alla scheda e apre l'inspector su un modello nuovo; spenta per
un vault in sola lettura.

**Core** (2026-10-07): `schedule_occurrences(schedule, from, limit)`
(`core/src/recurring.rs`, `core/src/ffi.rs`) restituisce le prime `limit` date
di una pianificazione da `from` in poi, validandola come `CreateRecurring`;
`end_date` la limita. Non tocca il database, e serve all'agenda e all'anteprima
dell'inspector.

### 2.6 Estratti conto, export completo e backup

> Aggiunto il 2026-09-23.

- **File › Importa estratto conto…** (⇧⌘I, anche dalla palette) apre un foglio
  a quattro passi: FILE, COLONNE, ANTEPRIMA, REPORT. Il file è letto in UTF-8
  (poi Windows-1252/Latin-1); `detect_statement` sceglie il separatore e, se
  l'intestazione corrisponde, il preset `card-transactions`; altrimenti si
  riapre la mappatura usata l'ultima volta per lo stesso vault e la stessa
  intestazione (`StatementMappingStore`). COLONNE: data, importo, descrizione,
  tipo, stato, valuta, categoria e importo originale; formato della data, segno
  delle uscite, virgola decimale; per ogni tipo cosa diventa (uscita, entrata,
  rimborso, trasferimento da o verso un altro wallet, salta, secondo il segno);
  stati da saltare; wallet e busta di destinazione. ANTEPRIMA: ogni riga con
  data, descrizione, importo, tipo, esito tradotto dal codice del core (nuova,
  già importata, saltata per stato/regola/da te, senza wallet, importo zero,
  non valida) e categoria; per le nuove una casella per escluderle e una
  categoria modificabile, precompilata da `suggest_categories` sullo storico.
  Reimportare lo stesso file non aggiunge nulla (id stabili per riga). Il foglio
  è più largo degli altri (880 × 640) perché la tabella ha otto colonne.
- **File › Esporta tutte le transazioni…**: un CSV con ogni transazione del
  vault, tutte le date, eliminate e trasferimenti compresi, con tipo, wallet e
  busta (da → a per i trasferimenti), categoria, nota, autore, eliminata (la
  colonna `voided`).
  ⌘E resta l'export delle righe a schermo, ma legge tutte le pagine del mese.
- **File › Backup del database…**: una copia coerente (`VACUUM INTO`) salvata
  dove si sceglie; l'avviso finale spiega come ripristinarla (README).

## 3. Mappa mockup → dominio

(Le etichette in maiuscolo sono quelle dei mockup; dal 2026-10-07 l'app le
scrive in minuscolo, e `FLOW` si legge "Busta".)

| Colonna / etichetta | Dominio |
|---|---|
| `FLOW` (Cash, Casa, Varie, Investimenti, Emergenza) | busta (`flows`) |
| `CATEGORIA` | categoria |
| `DESCRIZIONE` | `note` |
| `PERSONA` (Elisa, Matteo) | `transactions.created_by`, cioè l'autore del comando |
| `IMPORTO` | `amount`, valore assoluto: il segno lo dà il filtro USCITE/ENTRATE |
| `USCITE` / `ENTRATE` | `kinds = [expense, refund]` / `kinds = [income]` |
| `Risparmio` (mastro) | `income − net_expense` |
| `Tasso` | `risparmio / income` |
| `Fondo` (riepilogo) | busta con `cap` non nullo |
| `USCITE` / `USCITE FONDI` (riepilogo) | `net_expense` sulle buste senza / con tetto |
| `FONDO CASSA INIZIALE` | il secchio "prima di gennaio" di `year_breakdown`, per persona |

Il **wallet non è una colonna** di default: i mockup non lo mostrano, e ogni
riga lo prende dal default sticky. È una colonna opzionale (fatta il
2026-09-12), fra DESCRIZIONE e PERSONA, che si accende dal menu Vista
("Mostra colonna wallet", ⌘⇧W) e resta accesa fra un avvio e l'altro
(`UserDefaults`, chiave `showWalletColumn`, condivisa fra il menu e
`AppStore.showWalletColumn`). Quando è visibile la cella si modifica come le
altre — ↩ manda il wallet nuovo in `UpdateTransaction.wallet_id`, risolto per
nome come la cella FLOW — e la riga vuota lascia scegliere il wallet invece di
prendere il default. I trasferimenti fanno eccezione: la cella mostra
`da → a` in `dim` e non si modifica, perché le due gambe stanno in
`from_id`/`to_id`, che la griglia non tocca. L'export CSV (⌘E) porta la
colonna `wallet` solo quando è a schermo.

Decisione sulla persona (2026-09-10): niente campo nuovo sulle transazioni.
`created_by` è già l'utente del vault ed è il senso della colonna nei vault
condivisi. Costo: non si registra una spesa "per conto di" un altro membro.
Da loggati fuori il nome si sceglie nelle Impostazioni ("Nome nel mastro",
2026-09-11); senza scelta vale l'utente macOS, e dopo il login vale il nome
dell'account.

## 4. Contratto delle query (core)

Tutti gli aggregati stanno nel core, come da `ARCH.md` §2.2: l'app somma nulla.
L'app passa gli estremi `[from, to)` in UTC, calcolati con la timezone di
sistema, così il core non conosce fusi né calendari.

| Query | Serve a |
|---|---|
| `authors(vault)` | il segmentato `TUTTI/ELISA/MATTEO` |
| `flow_person_totals(vault, from, to)` | tabella persona × flow, "chi ha speso cosa" |
| `category_totals(vault, from, to, person?)` | uscite per categoria |
| `bucket_totals(vault, bounds[], person?)` | serie 12 mesi (13 estremi → 12 secchi) |
| `top_expenses(vault, from, to, person?, limit)` | top uscite del mese (non più a schermo dal 2026-09-11) |
| `year_breakdown(vault, bounds[])` | il riepilogo: per secchio × persona, `income` (senza aperture), `opening`, `cash_expense`, `fund_expense`; l'app passa 14 estremi (epoca + 13 inizi mese) e legge 13 secchi |
| `TransactionFilter.author` + `ascending` | griglia del mastro |

## 5. Aspetto

Palette fissa scura, non i colori di sistema: la finestra forza il tema scuro
e dipinge il suo fondo (`Support/Palette.swift`, enum `Ink`). I fondi salgono
di luminosità da `bg` a `hi`. L'**ambra** (`accent`) segna solo l'interazione:
selezione, focus, scheda attiva, bottoni, tag "da confermare". Non è mai il
colore di una spesa o di un errore, che hanno il loro, così un numero non
sembra cliccabile per il solo colore.

| Token | Valore | Uso |
|---|---|---|
| `bg` | `#0A0A0B` | barre della finestra (alta, schede, pannello destro) |
| `sheet` | `#0E0E10` | fondo del foglio, un passo sopra le barre |
| `card` | `#131316` | card |
| `raised` | `#1B1B20` | riga sotto il puntatore, chip acceso |
| `hi` | `#2A2A31` | segmento attivo di un controllo segmentato |
| `line` | `#1E1E23` | filetti e bordi delle card |
| `line2` | `#2B2B32` | bordi più forti: controlli, tasti |
| `rowLine` | `#18181C` | filetto fra le righe, più tenue di `line` |
| `text` | `#EDEDEF` | testo |
| `text2` | `#A3A3AB` | testo secondario |
| `text3` | `#80808A` | intestazioni di colonna, segnaposto, zeri |
| `accent` | `#FF9A2E` | ambra: solo interazione |
| `positive` | `#3ECF8E` | entrate, risparmio |
| `negative` | `#FF6B6B` | importi e saldi negativi, errori |
| `warning` | `#E8A33D` | modifica rifiutata dal sync, fascia di mezzo di una barra |
| `muted`, `mutedPositive` | `#4A2A12`, `#1D5340` | metà scura di una barra a due toni |
| `chartIncome` | `#5A8CF0` | serie entrate |
| `chartExpense` | `#D6742A` | serie uscite, barre delle categorie |
| `chartSavings` | `#25A26C` | serie risparmio |

I colori delle serie sono scelti perché si distinguano anche a chi non vede
bene i colori, su `#111113`. Una barra di avanzamento passa da `positive` a
`warning` a `negative` oltre il 70% e il 90%.

Tipografia: SF Pro con cifre tabulari ovunque (`Face.ui`), così le colonne di
numeri stanno in colonna senza trucchi; il monospazio resta solo per i tasti
(`Face.key`, le sigle ⌘K ⌘F). 12 pt nelle righe, 11 pt medium nelle intestazioni
di colonna e di sezione, che sono in minuscolo (sentence case) e in `text3`,
non più maiuscole spaziate; 22 e 24 pt per il numero di una card e per il
risparmio del pannello. Le card hanno angoli di 8 pt e un bordo da 1 pt.
`Metrics`: righe da 24 pt, intestazioni da 26, barra in alto da 44, barra
delle schede da 27.

## 6. Tastiera

| Tasto | Azione |
|---|---|
| `⇥` / `⇧⇥` | campo successivo / precedente |
| `↩` | salva la riga |
| `esc` | chiude l'elenco delle categorie, poi annulla la modifica; senza modifica toglie la selezione |
| `⌘`-click / `⇧`-click / `⌘A` | sceglie righe: una, un intervallo, tutte (§2.1) |
| `⌫` / `⌦` | elimina le righe scelte (col toast); senza righe scelte e senza cella in modifica, la riga sotto il puntatore (dal 2026-10-07) |
| `⌘Z` / `⇧⌘Z` | annulla / ripeti: modifica di cella, riga aggiunta, categoria in blocco, eliminazione in attesa |
| `↑` `↓` `↩` `⇥` | nell'elenco delle categorie: scorre e sceglie; nel quick-add `⇥` scrive la categoria suggerita |
| `⌘D` | duplica l'ultima riga (non un trasferimento; il tipo resta, un rimborso duplicato è un rimborso) |
| `⌥←` `⌥→` | mese precedente / successivo |
| `⌘F` | passa al Mastro e dà il fuoco alla ricerca nella barra in alto |
| `⌘K` | riquadro quick-add sopra la griglia (la grammatica di §3.1 del distillato, con la riga letta a chip, §2.1); con `>` in prima posizione è la palette comandi |
| `⌘E` | esporta CSV: le righe a schermo (tutte le pagine del mese), RFC 4180, `Support/LedgerCSV.swift` |
| `⇧⌘I` | importa un estratto conto (§2.6) |
| menu File | Esporta tutte le transazioni…, Backup del database… (§2.6) |
| `⌘⇧M` | gestione: vault, wallet, buste, ricorrenze, condivisione |
| menu Vault | Nuovo vault…, Rinomina vault…, Elimina vault…, Esci dal vault… (§2.4), senza scorciatoia: rari, e due distruttivi; dal 2026-09-29 anche Nuova ricorrenza… (§2.5) |
| `⌘1` `⌘2` `⌘3` `⌘4` | schede Riepilogo, Mastro, Ricorrenze, Setup (menu Vista) |
| `⌘⇧C` | scheda Setup: wallet, buste e categorie (menu Vault, "Wallet, buste e categorie…") |
| `⌘⇧V` / `⌘⇧T` | mostra eliminate / trasferimenti |
| `⌘⇧W` | mostra / nasconde la colonna WALLET (menu Vista) |

**Palette comandi** (2026-09-12): il campo ⌘K resta il quick-add finché il
testo non comincia con `>`. Allora sotto il campo compare l'elenco delle
azioni, filtrato dal vivo su quello che segue il `>` (senza distinzione di
maiuscole né di accenti: prima il titolo dall'inizio, poi dall'inizio di una
sua parola, poi ovunque, infine le parole chiave nascoste); ↑↓ scorrono con
rientro in fondo, ↩ esegue e chiude, esc chiude. Nessuna finestra nuova:
è lo stesso riquadro. Le azioni sono mese precedente / successivo / corrente,
"Vai a" ciascuna delle quattro schede (Riepilogo, Mastro, Ricorrenze, Setup), un "Vault: nome" per ogni altro vault, Nuovo vault…,
Rinomina vault… ed Elimina vault… (§2.4; le ultime due solo con un vault a
schermo, Elimina solo se lo si può cancellare), "Wallet, buste e categorie…", gestione, esporta CSV,
sincronizza ora, e i tre interruttori di vista (eliminate, trasferimenti,
colonna wallet; fino al 2026-10-07 "annullate"); dal 2026-09-23 anche Esci dal vault… (solo a un membro),
Importa estratto conto…, Esporta tutte le transazioni… e Backup del
database…, e Rinomina vault… solo a chi può scrivere; dal 2026-09-29 Nuova
ricorrenza…, solo a chi può scrivere. Quelle che hanno già una voce di
menu ne mandano la notifica, così le due strade condividono una sola
implementazione; il modello (`Views/Ledger/CommandPalette.swift`) è puro e
testato senza finestra.

## 7. Cronologia

Finestra nuova del 2026-09-10, in due commit: `core::analytics` con il
filtro per autore e l'ordine crescente (`core/src/analytics.rs`, 10 test),
poi palette, shell della finestra, switcher, barra di stato, vista MASTRO
con editing in cella, pannello riepilogo, viste RIEPILOGO e ANNO, gestione
in menu e finestre al posto delle view vecchie (47 test dell'app).
`SidebarView` è diventata `ManagementSheet`, `DetailView` e `InspectorView`
sono state cancellate insieme a `Period` e ad `AppTheme` (la palette li
sostituisce).

Riepilogo ridisegnato il 2026-09-11 (§2.2): `core::analytics::year_breakdown`,
`Model/YearModel.swift` (`YearSummary.build`, aritmetica pura testata),
`Views/Summary/SummaryView.swift` riscritta; `YearView` cancellata insieme
al riepilogo mensile, che vive nel pannello del mastro.

Rifinitura del 2026-09-11 (65 test dell'app): la griglia salva anche quando
si lascia la riga (⇥ oltre IMPORTO, click su un'altra riga o sulla ricerca),
tutta la cella è cliccabile, la riga vuota sopravvive a sync e annullamenti,
la cella DATA parte dal numero del giorno; export CSV (⌘E); il catalogo
stringhe copre tutta la UI del mastro in italiano; le barre dei 12 mesi
hanno una linea di base per i mesi in rosso; il toast di annullamento e il
bottone di sync usano la palette; le finestre secondarie forzano il tema
scuro.

Chiusura del 2026-09-12 (124 test dell'app): i due buchi lasciati aperti
il 2026-09-11 sono chiusi. Il campo ⌘K è anche una palette comandi quando la
riga comincia con `>` (§6), e il wallet ha la sua colonna opzionale, spenta
di default, accesa da ⌘⇧W e ricordata in `UserDefaults` (§3). Codice nuovo:
`Views/Ledger/CommandPalette.swift` (`PaletteAction`, `CommandPaletteModel`,
la lista sotto il campo) e `SparagneTests/CommandPaletteTests.swift`; la
colonna vive in `GridColumn.wallet`, `RowField.wallet`, `RowDraft.wallet`,
`AppStore.resolveWallet` e `LedgerCSV.render(_:wallet:)`.

Per guardare la UI con dei dati veri c'è una fixture:

```text
cargo run -p sparagne_core --example seed -- <db path> --replace
```

**Dal 2026-09-28.** La fixture scrive i dodici mesi che finiscono con quello
corrente, fino a oggi, così l'app apre sempre su un mese pieno. Il vault parte
con le categorie e gli alias di un vault nuovo in italiano, poi ha due wallet
(Conto, Contanti), tre buste (Cash, Casa, e Vacanze con un tetto), gli stipendi
di due persone, la spesa della settimana, bollette, trasferimenti fra wallet e
fra buste, rimborsi, righe senza categoria, una riga annullata e tre
ricorrenze: il mutuo con la rata del mese ancora da confermare, Netflix, e la
palestra archiviata dopo sei mesi. Le righe sono firmate da due persone, che un
server rifiuterebbe da un solo account, per cui il file non va nel database
vero. L'app lo apre a parte con `-SparagneDatabase demo.sqlite`, che legge un
altro file nella stessa cartella e non crea il `SyncEngine`: niente sync,
niente account, e il badge **DEMO** accanto al vault nella barra in alto, col
nome del file nel suggerimento (`LaunchOptions.swift`; fino al 2026-10-07 il
nome del file era il sottotitolo della finestra, che non c'è più). La pillola
di sync dice "Solo locale". Con `-SparagneTab summary|ledger|recurring|setup`
la finestra si apre su una scheda, per guardarla senza guidare la tastiera. Lo schema **Sparagne Demo** di Xcode passa già
l'opzione. La cartella dipende dalla firma: una build da Xcode gira nella
sandbox e legge
`~/Library/Containers/it.oghma.sparagne/Data/Library/Application Support/Sparagne/`,
una build con `CODE_SIGNING_ALLOWED=NO` legge `~/Library/Application Support/Sparagne/`.

**2026-10-07: finestra da terminale finanziario.** L'aspetto cambia in tutta
l'app e le viste di §2 si riorganizzano. Aspetto: `Support/Palette.swift` ha la
palette nuova (fondi a gradini, ambra solo per l'interazione, rosso diverso
dall'ambra, tre colori di serie), `Face` passa a SF Pro con cifre tabulari
(`Face.key` resta l'unico monospazio) e `Metrics` fissa righe da 24 pt,
intestazioni da 26, barra in alto da 44 e schede da 27. Finestra:
`App/WindowChrome.swift` e `Views/Chrome/TopBar.swift` disegnano la barra in
alto (semafori, `VaultSelector` col badge DEMO, `MonthStepper`, ricerca,
`DuePill`, "Aggiungi ⌘K", `SyncPill` con popover da `Model/SyncPillState.swift`),
`Views/Chrome/SheetTabBar.swift` le schede con le righe di stato
(`StatusLine`, ⌘1–⌘4); titolo, sottotitolo demo, toolbar con lo switcher e banner
delle ricorrenze sono tolti. Mastro: `Views/Ledger/FilterBar.swift`, griglia a
foglio, periodi dovuti fra le righe (`Model/LedgerLines.swift`, `PendingRowView`),
`Model/SheetStats.swift` per media, conteggio e somma, pannello destro rifatto,
quick-add a chip (`Model/QuickAddTokens.swift`). Riepilogo: card KPI, fondi a
barre, riga "Inizio anno" e tag "in corso" (`Views/Summary/`,
`Model/YearModel.swift`), grafici con scheda al passaggio del puntatore.
Ricorrenze, scheda nuova: `Views/Recurring/` (`DueRecurringCard`,
`RecurringAgendaCard`, `RecurringTemplateTable`, `RecurringInspector`) con
`Model/RecurringAgenda.swift`, `RecurringDraft.swift`, `RecurringNext.swift` e
`RecurringMonthly.swift`; `RecurringPanel` e `DueRecurringSheet` cancellati; nel
core `schedule_occurrences` (`core/src/recurring.rs`, `core/src/ffi.rs`).
Setup: `Views/Setup/VaultCard.swift`, tabelle ridisegnate, barre di riempimento
delle buste e colonna "Righe 90 gg" (`Model/AppStore+Usage.swift`). Avvio:
`-SparagneTab` (`App/LaunchOptions.swift`).

**Dal 2026-10-07.** Nella UI "annulla" diventa "elimina", in inglese (Void →
Delete) e in italiano (Annulla → Elimina, Annullate → Eliminate), perché
"Annulla" era anche il Cancel dei dialoghi e l'Undo di sistema; nel codice e
nel core il termine resta void (`VoidTransaction`, `voidSelection`). La riga
sotto il puntatore ha un cestino in una corsia fissa dopo IMPORTO
(`RowDeleteButton`, `RowActionTrack`) e ⌫ senza selezione la elimina
(`DeleteTarget`, `SparagneTests/DeleteTargetTests.swift`), §2.1.
