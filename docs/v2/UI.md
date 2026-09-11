# Sparagne v2 — UI (Fase 4)

> Deciso il 2026-09-10 sui mockup forniti (libro mastro + riepilogo). Sostituisce
> la griglia di dashboard di `DISTILLATO_V1.md` §3.4, che è stata scartata: le
> formule di §3.5 restano, la disposizione no.

## 1. Idea in una frase

Un foglio di calcolo per le righe, un terminale finanziario per i numeri: una
sola finestra scura, monospazio, con il mese come unità di lettura e gli
aggregati sempre a fianco delle righe.

## 2. Le due viste

Uno switcher in barra titolo: `RIEPILOGO · MASTRO`. Il riepilogo è la vista
di apertura. (Fino al 2026-09-11 le viste erano tre, `MASTRO · RIEPILOGO ·
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

Il **wallet non è una colonna**: i mockup non lo mostrano. Resta nel modello
(ogni entry ha una leg wallet) e viene risolto con il default sticky; è una
colonna opzionale, nascosta di default.

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
| `esc` | annulla la modifica |
| `⌘D` | duplica l'ultima riga |
| `⌥←` `⌥→` | mese precedente / successivo |
| `⌘F` | fuoco sulla ricerca |
| `⌘K` | riga quick-add sopra la griglia (la grammatica di §3.1 del distillato) |
| `⌘E` | esporta CSV: le righe a schermo, RFC 4180, `Support/LedgerCSV.swift` |
| `⌘⇧M` | gestione: vault, wallet, buste, ricorrenze, condivisione |
| `⌘⇧C` | finestra categorie |
| `⌘⇧V` / `⌘⇧T` | mostra annullate / trasferimenti |

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
scuro. Restano fuori: la palette comandi ⌘K del mockup è per ora solo il
quick-add, e il wallet non ha ancora una colonna opzionale.

Per guardare la UI con dei dati veri c'è una fixture:

```text
cargo run -p sparagne_core --example seed -- <db path> --replace
```
