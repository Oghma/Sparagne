# Sparagne v2 — UI (Fase 4)

> Deciso il 2026-09-10 sui mockup forniti (libro mastro + riepilogo). Sostituisce
> la griglia di dashboard di `DISTILLATO_V1.md` §3.4, che è stata scartata: le
> formule di §3.5 restano, la disposizione no.

## 1. Idea in una frase

Un foglio di calcolo per le righe, un terminale finanziario per i numeri: una
sola finestra scura, monospazio, con il mese come unità di lettura e gli
aggregati sempre a fianco delle righe.

## 2. Le due viste

Uno switcher in barra titolo: `RIEPILOGO · MASTRO · SETUP`. Il riepilogo è
la vista di apertura. (Fino al 2026-09-11 le viste erano tre, `MASTRO · RIEPILOGO ·
ANNO`: il riepilogo mensile è stato assorbito dal pannello destro del mastro
e ANNO dal nuovo riepilogo.)

### 2.1 MASTRO

```
‹ AGOSTO 2026 ›  mese=8   [TUTTI|ELISA|MATTEO]  [USCITE|ENTRATE]  / cerca…   37 righe
┌───┬────────┬────────┬───────────┬─────────────────┬─────────┬──────────┐┌───────────┐
│ # │ DATA   │ FLOW   │ CATEGORIA │ DESCRIZIONE     │ PERSONA │  IMPORTO ││ RIEPILOGO │
├───┼────────┼────────┼───────────┼─────────────────┼─────────┼──────────┤│  per      │
│002│ 01 ago │ Cash   │ Casa      │ mutuo           │ Matteo  │  €950,00 ││  persona  │
│003│ 02 ago │ Cash   │ Computer  │ Claude          │ Matteo  │   €74,33 ││───────────│
│ … │        │        │           │                 │         │          ││ RISPARMIO │
│   │ 29 ago │ Cash   │ Categoria │ descrizione…    │ Persona │     0,00 ││  TOTALE   │
└───┴────────┴────────┴───────────┴─────────────────┴─────────┴──────────┘│───────────│
 ⇥ campo successivo   ↩ salva riga   esc annulla   ⌘D duplica ultima      │ USCITE PER│
● mese=8 flow=uscite persona=tutti    Σ €5.735,98   37 righe   salvato 12:04│ CATEGORIA │
                                                                           │ 12 MESI   │
```

- `#` è l'ordinale della riga nel mese, non un id: la tabella si legge come un
  foglio. Ordine crescente per data, poi per id.
- L'ultima riga è sempre vuota ed è l'inserimento: si compila da sinistra a
  destra con ⇥ e si salva con ↩. Nessun pulsante "aggiungi".
- Ogni cella delle righe esistenti è editabile in posto; ↩ emette un
  `UpdateTransaction` con i soli campi cambiati, esc ripristina. Lasciare la
  riga (⇥ oltre IMPORTO, click altrove) salva come in un foglio di calcolo.
- Il pannello destro (284 pt) è il riepilogo del mese: tabella per persona,
  card risparmio, uscite per categoria, 12 mesi.
- I trasferimenti non stanno né in USCITE né in ENTRATE: spostano soldi senza
  guadagnarli o spenderli. Il menu Mastro li aggiunge alla lista corrente
  (⌘⇧T), come fa con le annullate (⌘⇧V).
- **Selezione** (Fase 6, 2026-09-23): ⌘-click aggiunge o toglie una riga,
  ⇧-click prende l'intervallo, ⌘A tutte le righe visibili quando nessuna cella
  è in modifica; le righe scelte hanno la tinta dell'accento e un click semplice
  apre ancora la riga in modifica. Con due o più righe compare la barra della
  selezione: **Annulla N righe** (un solo toast, un solo `execute_batch` alla
  scadenza) e **Imposta categoria…** (un solo `execute_batch` di
  `UpdateTransaction`); ⌫/⌦ annullano la selezione, esc la toglie. Righe
  annullate e trasferimenti si possono scegliere ma le azioni li saltano. La
  selezione si svuota cambiando mese, vault o filtro.
- **Annulla e ripeti** (⌘Z, ⇧⌘Z, menu Modifica, sull'`UndoManager` della
  finestra, `Model/LedgerHistory.swift`): una modifica di cella (si riscrivono
  i valori grezzi di prima: `""` per "senza categoria" e per la nota vuota),
  una riga aggiunta (dalla griglia o dal quick-add: annullarla la annulla
  subito, ripeterla la riaggiunge con un id nuovo), una categoria impostata in
  blocco (un passo solo) e l'annullo in attesa sul toast (⌘Z lo ferma). Un
  annullo già scritto non si disfa, perché non esiste un comando inverso:
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
- **VoiceOver**: ogni riga è una frase (data, importo col segno, categoria,
  nota, busta, persona, annullata) con le azioni Modifica, Duplica, Annulla e
  Seleziona e il tratto "selezionata"; celle, intestazioni, riga nuova, mese e
  segmenti dei filtri hanno un nome.

### 2.2 RIEPILOGO

> Ridisegnato il 2026-09-11 sul foglio Excel "Riepilogo" fornito
> dall'utente: l'anno fino al mese a schermo, con le definizioni sue.

```
‹ SETTEMBRE 2026 ›                                    2026 · fino a settembre
┌ FONDO EMERGENZA ─┐ ┌ FONDO VARIE ─────┐ ┌ FONDO CASA ──────┐
│      ◯ 99,8%     │ │      ◯ 92,2%     │ │      ◯ 21,3%     │   una gauge per
│ 29.931 / 30.000  │ │  4.611 / 5.000   │ │ 31.935 / 150.000 │   busta con tetto
└──────────────────┘ └──────────────────┘ └──────────────────┘
FONDO CASSA INIZIALE      ELISA 14.275,60    MATTEO 17.946,20    TOTALE 32.221,80
MESE  ENTRATE  USCITE  RISPARMIO  FONDO CASSA  USCITE FONDI   TOTALE    ELISA   MATTEO
gen   9.200    3.100     6.100      32.221        1.500      36.934   18.100   18.834
feb   …
set   …
ott   (vuoto: mese futuro)
┌ CASH FLOW ────────────────────────┐ ┌ FONDO CASSA ─────────────────────┐
│ linee entrate / uscite / risparmio│ │ linea del TOTALE, mese per mese  │
└───────────────────────────────────┘ └──────────────────────────────────┘
```

In cima, prima dell'intestazione, le quattro card del primo riepilogo
(richieste di nuovo il 2026-09-12): entrate, uscite, risparmio e tasso del
mese a schermo, col nome del mese nel titolo perché non si leggano come
valori dell'anno.

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
  la classificazione un dato del vault e non un nome. Una gauge per ogni busta
  attiva con tetto: riempimento = saldo/tetto (net) o entrate cumulative/tetto
  (income), sotto `saldo / tetto`. Senza buste con tetto la fila di gauge non
  c'è.
- CASH FLOW e FONDO CASSA sono grafici a linee (Swift Charts, palette): il
  primo entrate, uscite e risparmio per mese; il secondo il TOTALE.
- Nessun filtro persona nel riepilogo: le persone sono colonne.

### 2.3 SETUP

> Aggiunta il 2026-09-12 su richiesta: "sezioni per aggiungere le categorie
> e gli envelope", l'equivalente del foglio "Categorie e flow" dell'Excel.
> Il 2026-09-23 si aggiunge la tabella dei wallet, che prima si vedevano solo
> nella gestione ⌘⇧M.

Tre tabelle nello stile del mastro (stesse celle, stesse intestazioni, stessa
riga vuota in fondo per aggiungere: ⇥ tra i campi, ↩ salva, esc annulla):
a sinistra i wallet sopra le buste, a destra le categorie. La tabella dei
wallet è alta quanto le sue righe (sono pochi, niente scroll) e le buste sotto
prendono il resto; le due hanno la stessa larghezza, così i SALDO sono in
colonna. Nessun mese, quindi
niente intestazione mese. Le archiviate stanno in fondo, in `dim`, con
"Ripristina" nel menu contestuale. Gli errori del core passano dallo stesso
alert del mastro.

Ultima colonna di entrambe le tabelle: nessuna intestazione, vuota a riposo,
mostra l'icona `archivebox`/`tray.and.arrow.up` solo sulla riga sotto il
puntatore — un click manda lo stesso `ArchiveWallet`/`RestoreWallet`,
`ArchiveFlow`/`RestoreFlow` o `ArchiveCategory`/`RestoreCategory` del menu
contestuale, senza aprirlo. Assente sulle righe di sistema (Non allocato,
Opening, Uncategorized) e durante l'editing della riga
(`Views/Setup/WalletTable.swift`, `Views/Setup/EnvelopeTable.swift`,
`Views/Setup/CategoryTable.swift`).

```
┌ WALLET ────────────────────────────────────────────┐ ┌ CATEGORIE ───────────────────────────┐
│ NOME                                   SALDO     ⎘ │ │ NOME         ALIAS                  ⎘ │
│ Conto                               4.120,50 [📥] │ │ Casa         mutuo, affitto      [📥] │
│ Carta                                 -310,00     │ │ Spesa        coop, esselunga         │
│ nome…                                   0,00      │ │ Opening      (sistema)                │
│ Il saldo di apertura di un wallet nuovo va in     │ │ nome…        simili: Casa             │
│ Non allocato                                      │ └────────────────────────────────────────┘
└──────────────────────────────────────────────────┘
┌ BUSTE ─────────────────────────────────────────────┐
│ NOME        TIPO      TETTO     NEG   SALDO      ⎘ │
│ Non alloc.  —         —         —     1.250,00     │
│ Cash        nessuno   —         no    3.480,20 [📥] │
│ Casa        netto     150.000   no   31.935,00     │
│ Emergenza   entrate   30.000    no   29.931,00     │
│ nome…       nessuno ▾ tetto…    no   apertura…     │
└──────────────────────────────────────────────────┘
```

| Tabella | Colonne | In posto | Menu contestuale |
|---|---|---|---|
| WALLET | NOME, SALDO | nome → `RenameWallet`; SALDO segue le transazioni e si scrive solo nella riga vuota, dove è il saldo di apertura (negativo per una carta che parte in rosso), che va in Non allocato → `CreateWallet` | archivia (il core vuole saldo zero) / ripristina |
| BUSTE | NOME, TIPO (nessuno / netto / entrate), TETTO, NEG (sì/no, un click), SALDO | nome, tipo, tetto, negativo → `UpdateFlow` con i soli campi cambiati; nella riga vuota SALDO è l'allocazione iniziale da Non allocato → `CreateFlow` | archivia / ripristina |
| CATEGORIE | NOME, ALIAS (lista separata da virgole) | nome → `RenameCategory`; alias → la differenza fra la lista di prima e quella nuova, un `AddAlias`/`RemoveAlias` per voce; nella riga vuota, sotto il nome, "simili: …" suggerisce e non blocca (`similar_categories`) | unisci in… (anteprima con `preview_merge`), archivia / ripristina |

Non allocato e le categorie di sistema (`Opening`, `Uncategorized`) si vedono
ma non si modificano. ⌘⇧C ("Wallet, buste e categorie…") porta qui; la
finestra Categorie separata non c'è più. Vault, ricorrenze e condivisione
restano nella gestione ⌘⇧M, che elenca ancora anche wallet e buste.

### 2.4 Il vault

> Aggiunto il 2026-09-15: creazione, rinomina e cancellazione del vault.

Tre azioni, raggiungibili da tre posti che condividono un'implementazione
sola (notifiche del menu Vault, ricevute da `MainWindow`): il menu **Vault**
(Nuovo vault…, Rinomina vault…, Elimina vault…), la palette `>` (stesse voci)
e il menu a tendina del vault nella gestione ⌘⇧M (Nuovo…, Rinomina…,
Condividi…, Elimina…).

| Azione | Foglio | Comando |
|---|---|---|
| Nuovo | l'onboarding (`OnboardingSheet`): nome, primo wallet, saldo di apertura | `CreateVault` + `CreateWallet` |
| Rinomina | `RenameSheet`, come per wallet e buste | `RenameVault` |
| Elimina | `DeleteVaultSheet`: il nome del vault, una frase che dice che vanno via wallet, buste, transazioni e ricorrenze per ogni membro e su ogni dispositivo, Annulla e un "Elimina" distruttivo che ↩ non attiva | `DeleteVault` |

La valuta non si modifica (`ARCH.md` §4). Elimina non compare per un vault
condiviso di cui non si è owner (`SyncEngine.mayDeleteVault`); se arriva lo
stesso, il core risponde `forbidden` e l'alert dice "Non puoi farlo". Dopo la
cancellazione la finestra passa a un altro vault (quello ricordato, altrimenti
il primo) o torna all'onboarding se era l'ultimo; una riga in attesa sul toast
di annullamento muore col vault senza mandare nulla. Ai membri la
cancellazione arriva col sync (`SYNC.md` §4 punto 6).

**Fase 6 (2026-09-23).** Una quarta azione, **Esci dal vault…**, per un membro
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
modificano, Duplica e Annulla sono spenti, il quick-add risponde con un
messaggio, SETUP non si edita, e `CoreActor` rifiuta comunque ogni scrittura.
Un vault che non è più condiviso con me resta leggibile e la gestione lo
elenca fra i "Non più condivisi con te" con **Rimuovi da questo Mac**. La riga
in attesa sul toast di annullamento porta il suo vault: se un pull cancella
quel vault il toast sparisce senza avvisi, e chiudere l'app scrive l'annullo
prima di uscire.

### 2.5 Ricorrenze dovute

> Aggiunto il 2026-09-23 (Fase 6): dal ridisegno del 2026-09-10 il banner
> "N ricorrenze da confermare" apriva un pannello che non sapeva eseguirle.

"Rivedi" sul banner apre `DueRecurringSheet`: ogni modello con i suoi periodi
dovuti in ordine di data, importo, tipo, wallet e busta; per ogni periodo
**Esegui** o **Salta**, e **Esegui tutte** che manda ogni periodo in un solo
`execute_batch` (tutto o niente: se uno non passa, per esempio per fondi
insufficienti in una busta, non si scrive nulla e l'alert lo dice). Il pannello
delle ricorrenze mostra anche le archiviate e le **ripristina**. Le frasi di
cadenza sono intere e al plurale giusto ("Ogni 2 settimane il lunedì").

### 2.6 Estratti conto, export completo e backup

> Aggiunto il 2026-09-23 (Fase 6).

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
  vault, tutte le date, annullate e trasferimenti compresi, con tipo, wallet e
  busta (da → a per i trasferimenti), categoria, nota, autore, annullata.
  ⌘E resta l'export delle righe a schermo, ma legge tutte le pagine del mese.
- **File › Backup del database…**: una copia coerente (`VACUUM INTO`) salvata
  dove si sceglie; l'avviso finale spiega come ripristinarla (README).

## 3. Mappa mockup → dominio

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

Palette fissa scura, non i colori di sistema (`AppTheme` viene sostituito).

| Token | Valore | Uso |
|---|---|---|
| `bg` | `#0D0D0D` | fondo finestra |
| `panel` | `#141414` | card e pannello destro |
| `line` | `#242424` | separatori |
| `text` | `#E6E6E6` | testo |
| `dim` | `#7A7A7A` | intestazioni, placeholder, zeri |
| `accent` | `#FF5A3C` | selezione, filtri attivi, barre uscite |
| `positive` | `#3ECF8E` | entrate, risparmio |
| `negative` | `#FF5A3C` | uscite |

Tipografia: SF Mono ovunque, 11-12 pt nelle tabelle, cifre tabulari. Le
intestazioni di colonna e di sezione sono maiuscole, 10 pt, `dim`, spaziate.

## 6. Tastiera

| Tasto | Azione |
|---|---|
| `⇥` / `⇧⇥` | campo successivo / precedente |
| `↩` | salva la riga |
| `esc` | chiude l'elenco delle categorie, poi annulla la modifica; senza modifica toglie la selezione |
| `⌘`-click / `⇧`-click / `⌘A` | sceglie righe: una, un intervallo, tutte (§2.1) |
| `⌫` / `⌦` | annulla le righe scelte (col toast) |
| `⌘Z` / `⇧⌘Z` | annulla / ripeti: modifica di cella, riga aggiunta, categoria in blocco, annullo in attesa |
| `↑` `↓` `↩` `⇥` | nell'elenco delle categorie: scorre e sceglie; nel quick-add `⇥` scrive la categoria suggerita |
| `⌘D` | duplica l'ultima riga (non un trasferimento; il tipo resta, un rimborso duplicato è un rimborso) |
| `⌥←` `⌥→` | mese precedente / successivo |
| `⌘F` | fuoco sulla ricerca |
| `⌘K` | riga quick-add sopra la griglia (la grammatica di §3.1 del distillato); con `>` in prima posizione è la palette comandi |
| `⌘E` | esporta CSV: le righe a schermo (tutte le pagine del mese), RFC 4180, `Support/LedgerCSV.swift` |
| `⇧⌘I` | importa un estratto conto (§2.6) |
| menu File | Esporta tutte le transazioni…, Backup del database… (§2.6) |
| `⌘⇧M` | gestione: vault, wallet, buste, ricorrenze, condivisione |
| menu Vault | Nuovo vault…, Rinomina vault…, Elimina vault…, Esci dal vault… (§2.4), senza scorciatoia: rari, e due distruttivi |
| `⌘⇧C` | vista SETUP: wallet, buste e categorie |
| `⌘⇧V` / `⌘⇧T` | mostra annullate / trasferimenti |
| `⌘⇧W` | mostra / nasconde la colonna WALLET (menu Vista) |

**Palette comandi** (2026-09-12): il campo ⌘K resta il quick-add finché il
testo non comincia con `>`. Allora sotto il campo compare l'elenco delle
azioni, filtrato dal vivo su quello che segue il `>` (senza distinzione di
maiuscole né di accenti: prima il titolo dall'inizio, poi dall'inizio di una
sua parola, poi ovunque, infine le parole chiave nascoste); ↑↓ scorrono con
rientro in fondo, ↩ esegue e chiude, esc chiude. Nessuna finestra nuova:
è lo stesso riquadro. Le azioni sono mese precedente / successivo / corrente,
vai a RIEPILOGO o MASTRO, un "Vault: nome" per ogni altro vault, Nuovo vault…,
Rinomina vault… ed Elimina vault… (§2.4; le ultime due solo con un vault a
schermo, Elimina solo se lo si può cancellare), SETUP, gestione, esporta CSV,
sincronizza ora, e i tre interruttori di vista (annullate, trasferimenti,
colonna wallet); dal 2026-09-23 anche Esci dal vault… (solo a un membro),
Importa estratto conto…, Esporta tutte le transazioni… e Backup del
database…, e Rinomina vault… solo a chi può scrivere. Quelle che hanno già una voce di
menu ne mandano la notifica, così le due strade condividono una sola
implementazione; il modello (`Views/Ledger/CommandPalette.swift`) è puro e
testato senza finestra.

## 7. Tappe

| # | Cosa | Esito |
|---|---|---|
| A | `core::analytics` + filtro per autore e ordine crescente | query testate |
| B | palette, shell della finestra, switcher, barra di stato | finestra nuova, contenuto vecchio |
| C | vista MASTRO: intestazione mese, griglia, editing in cella | il primo mockup |
| D | pannello riepilogo a destra | il primo mockup completo |
| E | viste RIEPILOGO e ANNO | il secondo mockup |
| F | gestione (vault, wallet, buste, categorie, ricorrenze, sync) in menu e finestre; rimozione delle view vecchie | fine fase 4 |

Stato al 2026-09-10: A-F fatte in due commit (`core/src/analytics.rs` con 10
test; la finestra nuova con 47 test dell'app). `SidebarView` è diventata
`ManagementSheet`, `DetailView` e `InspectorView` sono state cancellate
insieme a `Period` e ad `AppTheme` (la palette li sostituisce).

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

Chiusura del 2026-09-12 (P3, 124 test dell'app): i due buchi lasciati aperti
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
