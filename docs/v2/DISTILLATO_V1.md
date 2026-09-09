# Sparagne v2 — Distillato della v1

> Bozza iniziale, 2026-09-05. Base: v1 `0.93.0` (HEAD `be41ad0`), 703 commit, nov 2022 → feb 2026.
> Scopo: elencare cosa la v1 ha prodotto di riutilizzabile per la riscrittura (UI nativa macOS, multi-tenant, stile bancario + tabellare), con un verdetto per ogni voce.
> Gli inventari grezzi (engine, server/schema, TUI, bot + storia git) sono in `docs/v2/inventory/`.

Legenda verdetti: **TIENI** (porta com'è, al più rinomina) · **RIPENSA** (l'idea è buona, l'implementazione no) · **BUTTA**.

## 0. La v1 in numeri

| | |
|---|---|
| Crate | engine 11.5k righe, tui 29.5k, telegram_bot 6.4k, più server, api_types, migration, app, admin_cli |
| Test engine | 44 test di integrazione su SQLite in memoria (2 file) |
| i18n TUI | 541 chiavi, IT + EN complete |
| Commit | 703, il 62% negli ultimi 3 mesi (dic 2025 – feb 2026) |
| Tag | v0.90.0 → v0.93.0 (gen–feb 2026) |
| Doc di dominio | `crates/engine/SPEC.md` (v1, dic 2025), `crates/engine/ARCH.md` |

## 1. Il nucleo di dominio (TIENI)

È la parte che vale più di tutto: tre anni di iterazione hanno prodotto un modello stabile e testato, sopravvissuto a due riscritture (2023 e 2025).

### 1.1 Le cinque entità

| Entità | Ruolo | Note v1 |
|---|---|---|
| **Vault** | contenitore per utente: wallet, flow, categorie, transazioni. È il "tenant" de facto. | `currency` (solo EUR), owner, nome unico per owner case-insensitive |
| **Wallet** | dove i soldi *stanno* fisicamente (contante, conto, carta). Può andare negativo. | `WalletKind` (Cash/Bank/CreditCard) è in SPEC ma mai implementato |
| **Flow** (busta) | a cosa i soldi sono *destinati* (vacanze, emergenze). Non scende sotto zero salvo `allow_negative`. Agnostico rispetto al wallet. | `FlowMode`: Unlimited, NetCapped(cap), IncomeCapped(cap) |
| **Unallocated** | flow di sistema, non rinominabile, può andare negativo. Riceve tutto ciò che non è assegnato. | nome interno `unallocated`, colonna `system_kind` |
| **Category** | etichetta per analytics, normalizzata per vault, con alias. | categoria di sistema `Uncategorized` non rimovibile |

L'insight centrale: **Σ wallet = Σ flow (Unallocated incluso)**. Wallet e flow sono due partizioni indipendenti dello stesso denaro. È envelope budgeting sopra un ledger, e ha retto.

### 1.2 Transaction + Leg

Ogni operazione utente è **una Transaction con più Leg firmate** (header + righe). La leg punta a un target polimorfico `Wallet | Flow` con `amount_minor` signed.

| Kind | Legs | Nelle statistiche |
|---|---|---|
| `Income` | wallet +x, flow +x | sì (entrate) |
| `Expense` | wallet −x, flow −x | sì (uscite) |
| `Refund` | wallet +x, flow +x | riduce le uscite, non gonfia le entrate |
| `TransferWallet` | wallet −x, wallet +x | no |
| `TransferFlow` | flow −x, flow +x | no |

Perché tenerlo: è ciò che permette un dashboard "bancario" corretto (i transfer interni non inquinano entrate/uscite) e al tempo stesso una UI tabellare (una transazione = una riga con colonne wallet/flow/importo).

### 1.3 Invarianti "locked" (SPEC §4), da copiare nella SPEC v2

1. Ogni operazione è una Transaction con almeno una Leg.
2. I flow diversi da Unallocated restano ≥ 0 (salvo `allow_negative`).
3. I wallet possono andare negativi.
4. Income/Expense/Refund: esattamente 1 leg wallet + 1 leg flow, stesso importo e segno.
5. Transfer: esattamente 2 leg dello stesso tipo, segni opposti, stesso valore assoluto.
6. Ogni `flow:+x` rispetta il cap del `FlowMode`; per `IncomeCapped` contano anche i transfer in ingresso.
7. Spesa oltre il saldo del flow: errore, mai split silenzioso.
8. Soft delete: `voided_at` esclude da saldi e statistiche; le read escludono le void per default.
9. Idempotency: stessa `(vault, key)` → stesso id, nessun duplicato.
10. Ogni transazione ha una categoria valida; vuoto → `Uncategorized`.
11. Currency coerente su vault, wallet, flow, transazione, leg.

### 1.4 Primitive tecniche che hanno funzionato

- **Importi in `i64` minor units + `Currency` esplicita ovunque.** Mai float, mai stringhe. `Currency` ha un solo variant ma è già su 5 tabelle: la multi-valuta è un'estensione, non un refactor.
- **Engine stateless, DB source of truth** (ARCH §1). Ogni operazione è una transazione DB; niente stato in RAM; `recompute_balances` ricostruisce i saldi dalle leg. Decisione del 2025-12-16, da mantenere.
- **Saldi denormalizzati aggiornati atomicamente** nella stessa transazione DB della scrittura. `preview_apply_leg_updates` simula prima e scrive dopo; create, update e void passano dallo stesso percorso: se un invariante fallisce non cambia nulla.
- **Void come soft delete** (`voided_at`, `voided_by`); le leg restano.
- **Idempotency key nel body** con unique index `(vault_id, key)`. Il bot la derivava dall'evento (`tg:{chat}:{msg}`); la TUI non la usava mai.
- **Cursor pagination keyset** (`occurred_at DESC, id DESC`, cursore base64url opaco); filtri `from/to` con semantica `[from, to)`, `kinds` allow-list, `include_voided`, `include_transfers`.
- **Error envelope** `{error: {code, message, details}}` con 19 codici snake_case stabili e mapping fisso a 400/403/404/409/422/500.
- **Saldo di apertura come transazione reale** (Income/Expense su Unallocated per i wallet, TransferFlow da Unallocated per i flow): niente casi speciali nello storico.

## 2. Feature applicative

### 2.1 Categorie — TIENI

- `name` per la UI + `name_norm` come chiave: trim, collapse spazi, NFKD, rimozione diacritici, lowercase, punteggiatura → spazio. `"Caffè"` → `caffe`, `"Auto-Moto"` → `auto moto`.
- **Alias** per vault (`alias_norm` unico), risolti nella catena di lookup.
- **Catena di risoluzione del testo libero**: id esplicito → vuoto = Uncategorized → match esatto su `name_norm` → match su alias → archiviata = errore → guardia similarità → auto-create.
- **Merge con preview**: restituisce `{ok, conflicts[{kind, value}]}` con kind `same_category`, `source_system`, `target_archived`, `alias_conflict`, `name_conflict`. Il merge ripunta le transazioni, sposta gli alias, aggiunge il vecchio nome come alias del target, archivia la sorgente.
- Il rename propaga il nome denormalizzato sulle transazioni.
- La migrazione `categories` contiene un **algoritmo di clustering** dei nomi storici (Levenshtein, soglia 1 per ≤6 caratteri altrimenti 2): riusabile per un import.

Da correggere: la guardia Levenshtein blocca la creazione senza offrire un "conferma" (il messaggio lo promette, il codice no). In v2: **suggerisci, non bloccare**.

### 2.2 Flow mode e cap — TIENI

`Unlimited | NetCapped{cap} | IncomeCapped{cap}` più il flag `allow_negative`. Storia istruttiva: nato nel 2022 come gerarchia di trait (`Unbounded/Bounded/HardBounded`), collassato in una struct nel 2023, riespresso come dati nel 2025, esteso con `allow_negative` nel 2026. Lezione: **mode come dati, non come tipi**.

Attenzione: il cap viene applicato anche su update e void, quindi un void può fallire con `MaxBalanceReached` o `InsufficientFunds`. Da decidere in v2 se un void debba mai essere bloccato da un invariante; probabilmente no.

### 2.3 Ricorrenze — RIPENSA (concetto ok, modello incompleto)

Buono: **materializzazione esplicita** (`list_pending` → l'utente conferma → `execute`), mai automatica; idempotency key `recurring:{id}:{YYYYMMDD}`; execute e `last_executed_date` nella stessa transazione DB.

Modello v1: `kind` (solo Income/Expense), `amount`, `wallet_id?`, `flow_id?`, `category`, `note`, `frequency ∈ {daily, weekly, monthly, yearly}`, `day_of_period` (weekly 1..7 ISO, monthly 1..28, yearly MMDD con giorno ≤ 28), `start_date`, `end_date?`, `enabled`, `last_executed_date`, `archived_at`.

Lacune: nessuno **skip**; nessun backfill dei periodi persi (solo l'ultimo periodo è mai "pending"); giorni 29-31 impossibili; `period_date` può precedere `start_date`; nessuna timezone (tutto `NaiveDate` e mezzanotte UTC); nessun transfer ricorrente. In v2 vale la pena guardare a RRULE (RFC 5545) o almeno a `interval` + `count`.

### 2.4 Void con undo differito — TIENI (come pattern di UI)

TUI: `d` nasconde subito la riga, toast di 5 secondi con barra countdown, `u` ripristina; alla scadenza (o prima della successiva azione distruttiva) chiama `void`. Stesso pattern per l'archiviazione di wallet e flow. Bot: il messaggio "✅ Salvato" porta i bottoni `[↩ Annulla] [✏️ Modifica]`. È la UX giusta per una tabella: nessun dialog di conferma sul percorso principale.

### 2.5 Sharing multi-utente — RIPENSA

Concetti v1:
- `vault_memberships(vault, user, role ∈ owner|editor|viewer)`.
- `flow_memberships(flow, user, role)`: accesso a un singolo flow senza accesso al vault.
- `flow_references(vault, target_flow, display_name)`: **il flow vive in un solo vault; un riferimento virtuale lo fa apparire in altri vault**, con transazioni cross-vault risolte via `resolve_flow_vault`. Conflitti di nome → `"{nome} ({owner})"`.
- Regole: read = owner o membro; write = owner o editor; statistiche e gestione membri = solo owner; "blind 404" per non rivelare l'esistenza di risorse altrui.
- Protezioni: non si cambia né rimuove il ruolo dell'owner del vault; non si rimuove l'ultimo owner di un flow; Unallocated non è condivisibile.

Perché ripensare: l'inventario engine elenca bug strutturali. Il ruolo di flow membership è bypassato per i flow referenziati (un `viewer` che possiede il proprio vault può svuotare il flow condiviso); `recompute_balances` perde i contributi cross-vault; le transazioni cross-vault non sono aggiornabili; lo storico di un flow condiviso è spezzato per `transactions.vault_id`. Due meccanismi (membership a due livelli + reference) per un solo caso d'uso reale ("dividiamo la busta vacanze") sono troppi. Se in v2 multi-tenant significa "più utenti sullo stesso vault", basta `vault_memberships`; la busta condivisa cross-vault va rifatta da zero o rimandata.

### 2.6 Statistiche — BUTTA lato server, TIENI le formule client

Server v1: un solo endpoint con totali **lifetime** (balance, income, expenses − refunds), **solo owner**. Tutto il resto lo faceva la TUI scaricando 180 giorni di transazioni a pagine di 200. Per un dashboard bancario servono aggregati server-side (o query locali, se local-first). Le formule da conservare sono in §3.5.

### 2.7 Export CSV — TIENI come minimo

Bot: `data,tipo,importo,categoria,nota,annullata`, kind localizzati, void incluse. Banale, ma esisteva. L'import non è mai esistito.

## 3. UX distillata da TUI e bot

### 3.1 Quick-add: la grammatica a una riga — TIENI, unificata

Bot e TUI condividono la grammatica con estensioni diverse. Unione:

```
[+|-|r] importo  [nota…]  [#categoria]  [@wallet]  [>busta]
tw> importo @da @a [nota]        trasferimento tra wallet
tf> importo >da >a [nota]        trasferimento tra buste
```

- Segno: niente o `-` = spesa, `+` = entrata, `r` = rimborso.
- Importo = **primo token**, grammatica di `Money::parse_major` (Appendice A).
- Al massimo un `#`, un `@`, un `>`; i token nudi `#`, `@`, `>` restano nella nota.
- Categoria come stringa: la risolve il server, alias inclusi.
- Wallet e busta risolti case-insensitive tra le entità attive, ordinate default → recenti → resto, con priorità **esatto > prefisso > contiene**; ambiguità mostrata inline, `Ctrl+R` cicla le opzioni.
- Preview live: `▼ 15.00 EUR  pizza │ #food │ >groceries │ @cash │ Today`.

Lezioni: il prefisso `r` impedisce una nota che inizia con "r" come primo token (`rent 50` diventa un rimborso); nessun token data (`occurred_at` è sempre "ora"); in v2 aggiungere `ieri`, `-3d`, `12/03`. In una UI tabellare questa grammatica vive bene in una **cella smart** o in una command bar tipo Spotlight.

### 3.2 Default sticky e recenti — TIENI

- Wallet: scope corrente → default per (utente, vault) → primo recente → primo attivo.
- Busta: scope → default → prima recente → ultima usata → Unallocated.
- Recenti: ultime 5 categorie, wallet e buste da una query sui 90 giorni; picker ordinati default → recenti → resto.
- Bot: se manca il default wallet la bozza viene "parcheggiata", compare il picker e la bozza riprende dopo la scelta.
- Bot: suggerimento categoria da keyword nella nota (`caffè → bar`, `benzina → auto`, `treno → trasporti`, …) quando manca `#`. Buon aggancio per un mapping appreso dallo storico.

### 3.3 Lista transazioni — TIENI le idee, in forma di tabella

- Riga: `HH:MM icona [VOID] importo firmato nota #categoria @wallet >busta`.
- **Grouping** per data, categoria, wallet o busta con totale firmato per gruppo (solo le Expense negate).
- Filtri: intervallo date, kind multi-select, includi transfer, includi void; scope wallet **oppure** busta (mutuamente esclusivi: limite da rimuovere).
- **Visual mode**: multi-select, `d` void di massa con undo, `c` bulk-categorize.
- Repeat: nuova transazione dagli stessi leg, data = ora.
- Ricerca client-side su kind, nota, categoria, importo, data, ma solo sulla pagina caricata (limite noto).
- Template (bot): `nome | 1.50 #bar caffè`, massimo 10, uso a un tap.

### 3.4 Dashboard — TIENI come griglia di partenza

- Card: **Net worth** (somma dei wallet attivi), **Income** ed **Expenses** del mese, sparkline 30 giorni del netto cumulato.
- **Balances**: wallet per saldo decrescente; buste senza Unallocated.
- **Activity feed**: prima gli alert (busta < 0 = critical, 0 ≤ saldo ≤ soglia `low_balance_minor` = warning), poi le transazioni raggruppate Today / Yesterday / data, con una riga "insight" (`Hai risparmiato X (p% delle entrate)`).
- Badge busta: `45.50 / 100.00 EUR` (deciso il 2026-02-13 al posto di `[limitato]`), `[allow neg.]`, `[condiviso da X]`, `[in condivisione]`.
- Gauge spese/entrate con soglie 70% e 90%.

### 3.5 Formule analytics — TIENI, da spostare lato server

- Netto giornaliero: `+|income| −|expense| +|refund|`, transfer ignorati, void escluse.
- Rollup mensile: income, expense, refund; `net_expense = max(expense − refund, 0)`.
- Breakdown categoria mensile con bucket "Other" / "Senza categoria"; `❗` se una categoria pesa ≥ 40%.
- MoM: `(ultimo − precedente) / |precedente|`; badge EXCELLENT / GOOD / STABLE / CAUTION / DECLINING con soglie ±10% e 0, direzione "buona" invertita per le spese.
- Mappa keyword → icona categoria (IT/EN): food/spesa/cibo → 🍴, casa/affitto → 🏠, auto/benzina → 🚗, salute/farmacia → 🏥, …

### 3.6 Vocabolario e i18n — TIENI

- IT default, EN completo. Termini: **Wallet**, **Busta / Budget** (flow), **Categoria**, **Non in flow** (Unallocated), **Vault**.
- Onboarding del bot in tre concetti: "👛 Wallet - dove tieni i soldi · 🎯 Budget - come organizzi le spese · 🏷 Categoria - tag per classificare". Da riusare al primo avvio.
- Formato importo `12.34 EUR`, senza separatore delle migliaia e senza locale: in v2 usare il formatter di sistema.

## 4. Cosa buttare

| Cosa | Perché |
|---|---|
| Bot Telegram, TUI Ratatui | decisione presa; delle due restano solo le idee di §3 |
| HTTP Basic con password in chiaro e match `LIKE '%…%'` su username *e* password | è un bypass di autenticazione, non un debito tecnico |
| Header `telegram-user-id` per impersonare, pairing con codici che non scadono | legati al bot |
| API RPC-over-POST (`POST /vault/get`, body JSON su `GET` e `DELETE`), naming misto (`/cashFlow/get`, `/transferWallet`, `/flow-references`), `vault_id` stringa | contratto da ridisegnare: REST o RPC coerente, ma uno solo |
| Handler non atomici (`wallet_new` = 2 chiamate, `flow_update` = fino a 4) | la composizione va nell'engine |
| Enum duplicati engine ↔ api_types con 5 file di mapping a mano | un solo tipo per il contratto, o schema-first |
| Categoria free-text `"opening"` per i saldi di apertura | usare una categoria di sistema |
| `config.toml` con sezioni server, bot e tui nello stesso file; `tui_state.json` relativo alla CWD | configurazione per componente |
| Statistiche lifetime, solo owner | vedi §2.6 |
| Guardia Levenshtein bloccante | vedi §2.1 |
| Delete hard di un flow con leg orfane; `delete_vault` con SQL raw e ordine manuale | FK con cascade e niente hard delete sui flow |
| Saldi di wallet e flow senza check di integrità | tenere la denormalizzazione, ma con un controllo periodico `Σ wallet = Σ flow` |

## 5. Buchi che la v1 non ha mai coperto

Qui la v2 deve decidere, perché non c'è codice da cui partire.

- **Identità e tenancy.** `user_id` è lo username in chiaro; non esistono sessioni, token, hashing, reset password, email, organizzazioni. Il vault è il tenant di fatto. Per un vero multi-tenant serve prima un modello utente.
- **Concorrenza.** Nessun `updated_at`, `version`, ETag. Una UI tabellare multi-client perde aggiornamenti in silenzio. Servono optimistic locking sulle transazioni e un endpoint batch atomico per il bulk edit.
- **Timezone.** `Europe/Rome` hard-coded nel bot, mix `Local`/UTC nella TUI, "oggi" delle ricorrenze calcolato in UTC dal server. In v2 una timezone per utente (o per vault) usata ovunque.
- **Multi-valuta.** Il tipo c'è, la logica no: nessun tasso, nessuna conversione, un solo variant.
- **Split.** Una spesa su più buste o categorie: il modello a leg lo permette già (n leg flow con somma pari alla leg wallet), la SPEC lo cita solo come opzione futura per Unallocated (§4.8).
- **Import.** Nessun import CSV o bancario. Per un'app "stile bancario" è la feature che porta i dati dentro; il clustering categorie della migrazione è il pezzo riusabile.
- **Audit e revisioni** (SPEC §9.1), **passività e mutui** (§9.2), **WalletKind** (§3.2): mai implementati.
- **Allegati, tag multipli, obiettivi con scadenza**: assenti.
- **Piattaforma.** Build e release solo Linux x86_64; niente macOS o ARM in CI.

## 6. Lezioni dalla storia git

- **Il tipo degli id ha cambiato idea quattro volte** (String → Uuid → String → Uuid → Uuid come BLOB). Decidere subito e non toccare più: UUID v7 o ULID, ordinabili nel tempo.
- **Gerarchia di tipi → dati.** I tre tipi di cash flow del 2022 sono diventati un enum `FlowMode` più un flag. Stesso destino probabile per qualsiasi "kind" in v2.
- **Stateless ha vinto.** L'engine in RAM del 2023 è stato buttato nel 2025 ("This is a big change"); la SPEC ha registrato la scelta come definitiva.
- **La SPEC è arrivata dopo il codice** (scritta a dic 2025, committata a feb 2026) e il codice se n'è già allontanato: nomi dei kind, `WalletKind` assente, statistiche non mensili, `allow_negative`, ricorrenze e alias fuori SPEC. In v2: SPEC prima, test come acceptance.
- **Cluster di bug** (39 `fix:`): segno e forma dei DTO tra client e server nella sprint pairing di gen 2024; risoluzione di vault e flow condivisi (gen–feb 2026); routing input della TUI. I primi due sono conseguenze di contratto duplicato e sharing troppo articolato.
- **La v1 attuale è già una riscrittura**: dopo 18 mesi di pausa (mag 2024 → dic 2025) bot, engine e modello sono stati rifatti in tre mesi. Il modello di dominio è l'unica cosa sopravvissuta a entrambe le vite del progetto: è il segnale più forte su cosa tenere.

## 7. Implicazioni per una UI tabellare

Non è un design: sono i vincoli che il distillato impone.

- **Transazione = riga.** Colonne naturali: data, kind (o icona), importo firmato, wallet, busta, categoria, nota, stato (void). Le leg non vanno mostrate: per i 5 kind la riga le determina univocamente.
- **Transfer = riga con due colonne** `da → a` dello stesso tipo. La tabella deve poter rendere wallet e busta come `@bank → @cash`.
- **Kind derivabile** dal segno dell'importo e dal tipo dei target. In una cella si può scrivere `-12.50` e la grammatica di §3.1 fa il resto.
- **Bulk edit** (categorizzazione, void, cambio busta su N righe) esiste già come UX; serve un endpoint batch atomico più il version check.
- **Void, non delete**: la riga resta, barrata, con toggle "mostra annullate".
- **Aggregati in tempo reale**: totali di gruppo, saldo wallet e busta, mese corrente. O li espone il server, o i dati sono locali.
- **Buste con cap** come barra `speso / cap`: è l'unico widget "budget" necessario nella prima versione.

## 8. Decisioni aperte

1. Multi-tenant da subito (server + auth) o **local-first** con sync dopo? Cambia storage, engine e concorrenza.
2. Tenere l'**envelope model** (wallet × busta) o semplificare a conti + categorie + budget mensili per categoria? La v1 dice che il modello regge; la domanda è se lo usi davvero.
3. Sharing: solo membri di vault, o anche buste cross-vault?
4. Ricorrenze: rifare con RRULE o riportare il modello v1 aggiungendo skip e backfill?
5. Multi-valuta nella prima release o dopo?
6. Engine Rust riusato via FFI da Swift, o core riscritto in Swift?
7. Lingua della SPEC v2 e dei nomi di dominio (IT nel documento, EN nel codice, come in v1?).

## Appendice A — file da copiare quasi tal quali

| File v1 | Cosa contiene |
|---|---|
| `crates/engine/SPEC.md` | glossario, entità, invarianti, casi d'uso: base della SPEC v2, da aggiornare con §1.3 e §2 |
| `crates/engine/src/money.rs` | `Money::parse_major`: `[+|-] cifre [ (.|,) max 2 decimali ]`, niente migliaia, niente simboli, overflow controllato; `format` |
| `crates/engine/src/util.rs` | `normalize_category_key`, `normalize_category_display`, `validate_category_name` |
| `crates/engine/src/cash_flows.rs` | `apply_leg_change(old, new)`: tutte le regole di cap e non-negatività in una funzione |
| `crates/engine/src/ops/recurring.rs` | `compute_current_period_date`, `validate_day_of_period` |
| `crates/engine/src/ops/transactions/list.rs` | cursore keyset e filtri |
| `crates/engine/tests/transactions.rs`, `tests/flow_sharing.rs` | 44 test = acceptance test della v2 (elenco in `inventory/engine.md` §12) |
| `crates/api_types/src/lib.rs` | `ErrorCode`, `FlowMode` taggato, `TransactionView`, `LegTarget`: forma del contratto |
| `crates/migration/src/m20260115_000001_categories.rs` | clustering Levenshtein dei nomi categoria, per l'import |
| `crates/tui/src/quick_add.rs`, `crates/telegram_bot/src/parsing.rs` | parser quick-add e relativi test |
| `crates/tui/src/app/actions/stats.rs` | formule di §3.5 |
| `crates/tui/src/text/it.rs`, `en.rs` | 541 stringhe IT/EN già tradotte |
| `crates/tui/src/app/resolve/defaults.rs` | catena dei default e risoluzione dei nomi |

## Appendice B — inventari grezzi

`docs/v2/inventory/engine.md`, `server.md`, `tui.md`, `bot-and-history.md`: cosa fa il codice oggi, sezione per sezione, con warts e divergenze dalla SPEC. Da consultare quando serve il dettaglio.
