# Sparagne v2 — UI (Fase 4)

> Deciso il 2026-09-10 sui mockup forniti (libro mastro + riepilogo). Sostituisce
> la griglia di dashboard di `DISTILLATO_V1.md` §3.4, che è stata scartata: le
> formule di §3.5 restano, la disposizione no.

## 1. Idea in una frase

Un foglio di calcolo per le righe, un terminale finanziario per i numeri: una
sola finestra scura, monospazio, con il mese come unità di lettura e gli
aggregati sempre a fianco delle righe.

## 2. Le tre viste

Uno switcher in barra titolo: `MASTRO · RIEPILOGO · ANNO`.

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
  `UpdateTransaction` con i soli campi cambiati, esc ripristina.
- Il pannello destro (284 pt) è lo stesso riepilogo della vista RIEPILOGO,
  ridotto: tabella per persona, card risparmio, uscite per categoria, 12 mesi.
- I trasferimenti non stanno né in USCITE né in ENTRATE: spostano soldi senza
  guadagnarli o spenderli. Il menu Mastro li aggiunge alla lista corrente
  (⌘⇧T), come fa con le annullate (⌘⇧V).

### 2.2 RIEPILOGO

Quattro card in alto (entrate, uscite, risparmio, tasso di risparmio), a
sinistra la tabella persona × flow più "chi ha speso cosa", a destra la serie
entrate/uscite dei 12 mesi dell'anno e le top uscite del mese.

### 2.3 ANNO

La stessa tabella del riepilogo con i 12 mesi come colonne.

## 3. Mappa mockup → dominio

| Colonna / etichetta | Dominio |
|---|---|
| `FLOW` (Cash, Casa, Varie, Investimenti, Emergenza) | busta (`flows`) |
| `CATEGORIA` | categoria |
| `DESCRIZIONE` | `note` |
| `PERSONA` (Elisa, Matteo) | `transactions.created_by`, cioè l'autore del comando |
| `IMPORTO` | `amount`, valore assoluto: il segno lo dà il filtro USCITE/ENTRATE |
| `USCITE` / `ENTRATE` | `kinds = [expense, refund]` / `kinds = [income]` |
| `Risparmio` | `income − net_expense` |
| `Tasso` | `risparmio / income` |

Il **wallet non è una colonna**: i mockup non lo mostrano. Resta nel modello
(ogni entry ha una leg wallet) e viene risolto con il default sticky; è una
colonna opzionale, nascosta di default.

Decisione sulla persona (2026-09-10): niente campo nuovo sulle transazioni.
`created_by` è già l'utente del vault ed è il senso della colonna nei vault
condivisi. Costo: non si registra una spesa "per conto di" un altro membro.

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
| `top_expenses(vault, from, to, person?, limit)` | top uscite del mese |
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
| `⌘E` | esporta CSV (non ancora implementato) |
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
insieme a `Period` e ad `AppTheme` (la palette li sostituisce). Restano fuori:
export CSV (⌘E), la palette comandi ⌘K del mockup è per ora solo il quick-add,
e il wallet non ha ancora una colonna opzionale.

Per guardare la UI con dei dati veri c'è una fixture:

```text
cargo run -p sparagne_core --example seed -- <db path> --replace
```
