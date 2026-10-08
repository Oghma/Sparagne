# Sparagne v2 — Architettura

> Bozza 1, 2026-09-09. Decisioni prese: branch `v2` in questo repo; UI Swift/SwiftUI solo macOS; server di sync dopo l'app locale. Il dominio è quello del `DISTILLATO_V1.md` §1, salvo dove indicato.

## 1. Idea in una frase

L'app lavora su un SQLite locale attraverso un core Rust in-process; ogni scrittura è un **comando** appeso a un **log per vault**; la condivisione tra utenti è la replica di quel log tramite un server piccolo che applica gli stessi comandi con lo stesso core e fa da arbitro.

## 2. Componenti

```
┌──────────────────────────────┐        ┌──────────────────────────────┐
│  App macOS (Swift/SwiftUI)   │        │  sync                        │
│  tabella, quick-add, dash    │        │  auth · membership           │
│         │ UniFFI (in-process)│  HTTP  │  log per vault · push/pull   │
│  ┌──────▼───────┐            │◄──────►│  ┌──────────────┐            │
│  │ core (Rust)  │            │        │  │ core (Rust)  │ (stesso    │
│  │ dominio+SQLite│           │        │  │              │  crate)    │
│  └──────┬───────┘            │        │  └──────┬───────┘            │
│      SQLite locale           │        │     SQLite/Postgres          │
└──────────────────────────────┘        └──────────────────────────────┘
```

### 2.1 `core` (Rust, sincrono)

- Un solo crate, libreria. Niente async, niente ORM: `rusqlite` con feature `bundled`.
- Contiene: tipi di dominio, `Money`, normalizzazione categorie, `apply_leg_change`, i comandi, la proiezione (tabelle di stato), le query per la UI, il parser quick-add, le aggregazioni del mastro (`analytics.rs`).
- Esposto a Swift con **UniFFI** (Swift Package generato). Swift non fa mai SQL e non conosce le regole: chiama `execute(cmd)` e `query_*`.
- Stato (2026-09-09): i derive UniFFI stanno sui tipi di dominio reali (nessun DTO specchio); `Uuid`, date e date-time viaggiano come stringhe; `CoreHandle` serializza l'accesso al `Core` con un mutex; gli id degli envelope si generano solo in Rust. `apple/build-core.sh` produce `apple/SparagneCore` (XCFramework non versionato più `SparagneCore.swift` generato e versionato). Il crate `core` ammette `unsafe_code` solo per lo scaffolding generato. `DomainError` attraversa l'FFI come *flat error* (un case per variante con il messaggio) e `ErrorCodes.swift` riporta il `code()` stabile a mano; `QuickAddError` invece è strutturato, così `ambiguous_name` porta i candidati e la UI può proporli. `resolve_quick_add` restituisce il comando insieme agli id risolti (`ResolvedQuickAdd`), `TransactionView` espone wallet, busta, sorgente e destinazione oltre alle leg, e gli `Update*` portano un record `*Patch` con default.
- Lo stesso crate gira nel server come dipendenza di axum o come NIF Rustler dentro Elixir. Il linguaggio del server è una decisione rimandata (§9).

### 2.2 App macOS

- SwiftUI, `@Observable` store che incapsula il core. Dal 2026-09-10 la tabella è una griglia SwiftUI scritta a mano (`Views/Ledger/`), non `Table`: l'editing in cella e il look dei mockup lo richiedono. NSTableView è stata valutata e scartata (`UI.md`).
- L'app non ha stato di dominio proprio: ogni vista è una query sul core, ogni azione è un comando.
- Dal 2026-09-12 il core sta su un attore dedicato (`Core/CoreActor.swift`), l'unico posto che tocca `CoreHandle`. `AppStore` resta `@MainActor @Observable` ma ogni punto di ingresso che tocca il core è `async`: attende l'attore e scrive lo stato pubblicato sul main actor al ritorno. `SyncEngine` usa lo stesso attore, così il core non è mai chiamato da due domini di isolamento. Il caricamento del mese è una sola visita (`CoreActor.load`), non una dozzina di salti, e una `reload` più vecchia non sovrascrive un mese più recente. I filtri (`month`, `direction`, `person`, i due interruttori) restano proprietà legabili dalle view: il `didSet` non può attendere, quindi accoda il caricamento e `AppStore.settle()` aspetta che la coda si svuoti.
- Timezone di sistema; i comandi portano `occurred_at` come RFC3339 con offset.

### 2.3 `sync`

- Autentica (argon2 + token opachi), mantiene il log di ogni vault, espone `push(vault, comandi)` e `pull(vault, since_seq)`.
- Applica ogni comando ricevuto con `core` prima di accettarlo: è l'unico punto dove le regole vengono fatte rispettare tra utenti diversi.
- Serializza i comandi per vault (un lock per vault in Rust, o un processo per vault sulla BEAM).
- Stato (2026-09-10): fatto in axum con un solo `Mutex<Core>` (SQLite ha un solo scrittore) e `spawn_blocking`; protocollo in `SYNC.md`.

## 3. Il log dei comandi

Il log è la verità; le tabelle di stato (wallet, buste, transazioni, leg, saldi) sono una proiezione ricostruibile. È la generalizzazione del `recompute_balances` della v1.

```
commands
  id              BLOB(16) PK   -- UUID v7 generato dal client; è anche la idempotency key
  vault_id        BLOB(16)
  seq             INTEGER       -- ordine totale nel vault; assegnato dal server (locale prima del sync)
  author_user_id  TEXT
  kind            TEXT          -- vedi §4
  payload         TEXT (JSON)
  occurred_at     TEXT          -- RFC3339 con offset, quando l'utente dice che è successo
  created_at      TEXT          -- RFC3339, quando è stato emesso
  status          TEXT          -- applied | rejected
  rejection       TEXT NULL
UNIQUE(vault_id, seq)
```

Stato locale aggiuntivo per il sync: `outbox` (comandi emessi e non ancora confermati dal server) e `vault_sync(vault_id, last_server_seq)`.

**Applicazione.** `execute(cmd)`: valida contro la proiezione corrente, scrive comando e proiezione nella stessa transazione SQLite. Se una regola fallisce, non viene scritto nulla e il chiamante riceve un `DomainError` con codice stabile (stessa famiglia di codici della v1).

**Snapshot.** Ogni N comandi il core salva uno snapshot della proiezione; il replay riparte dall'ultimo snapshot. Con volumi da finanza personale (migliaia di comandi) il replay completo resta comunque nell'ordine dei millisecondi.

## 4. Comandi

Tutto ciò che muta un vault è un comando; niente scritture dirette alle tabelle. Regola degli id: **ogni entità creata da un comando ha un id derivato dal comando** (uguale all'id del comando, oppure UUID v5 dell'id del comando con un ruolo: `opening`, `category`, `unallocated`). Così il replay del log produce gli stessi id senza stato esterno.

| Famiglia | Comandi |
|---|---|
| Wallet | `CreateWallet`, `RenameWallet`, `ArchiveWallet`, `RestoreWallet` |
| Busta | `CreateFlow`, `UpdateFlow` (nome, mode, allow_negative), `ArchiveFlow`, `RestoreFlow` |
| Categoria | `CreateCategory`, `RenameCategory`, `ArchiveCategory`, `RestoreCategory`, `AddAlias`, `RemoveAlias`, `MergeCategory` |
| Transazione | `Income`, `Expense`, `Refund`, `TransferWallet`, `TransferFlow`, `UpdateTransaction`, `VoidTransaction` |
| Ricorrenze | `CreateRecurring`, `UpdateRecurring`, `ArchiveRecurring`, `RestoreRecurring`, `ExecuteRecurring`, `SkipRecurring` |
| Vault | `CreateVault` (il suo id è l'id del vault: è il primo comando del log), `RenameVault`, `DeleteVault` |

Convenzioni comuni (fissate il 2026-09-09, vedi `core/src/command.rs`):
- Archiviare e ripristinare sono comandi distinti, così il log si legge senza guardare il payload. Archiviare un wallet o una busta richiede saldo zero; wallet, buste e categorie archiviati rifiutano nuove leg.
- Gli `Update*` portano solo i campi cambiati (`Option`); una stringa vuota azzera la nota o riporta la categoria a `Uncategorized`. Almeno un campo deve essere presente.
- `MergeCategory` ripunta le transazioni, sposta gli alias, aggiunge il nome della sorgente come alias del target e **elimina** la sorgente (non la archivia: la chiave deve tornare libera perché l'alias risolva). `Core::preview_merge` elenca i conflitti prima.
- Il vault (2026-09-15): `RenameVault` cambia solo il nome (anche un editor può rinominare, l'owner resta quello). Il nome è un'etichetta e può ripetersi, anche fra i vault dello stesso owner (dal 2026-09-23, `SYNC.md` §3 "Nomi dei vault"): `CreateVault` e `RenameVault` non guardano gli altri vault, così un replay dà lo stesso risultato su ogni dispositivo. La valuta non si cambia mai, ogni importo del log è in quella. `DeleteVault` è l'unica cancellazione dura del dominio: la riga `vaults` va via e il cascade dello schema porta con sé wallet, buste, categorie, transazioni, leg e ricorrenze. Il **log resta**, così la cancellazione viaggia come ogni altro comando (`SYNC.md` §3) e un replay finisce dove è finito il database. Solo l'owner può cancellare (`forbidden`, un `DomainError` nuovo con lo stesso codice del 403 HTTP); dopo, ogni comando sul vault è `not_found`. `Core::deleted_vaults()` elenca i vault che il log conosce e la proiezione no.
- Le ricorrenze non si materializzano mai da sole: `Core::pending_recurring(vault, oggi)` elenca i periodi dovuti, compresi quelli arretrati, e l'utente li esegue o li salta uno per uno. "Oggi" lo passa l'app con la timezone di sistema. Lo `Schedule` è `frequency` (daily, weekly{weekday}, monthly{day}, yearly{month,day}) più `interval`, `start_date`, `end_date`; i giorni 29-31 si agganciano all'ultimo giorno del mese senza derivare. Per le anteprime (agenda dei prossimi giorni, date del modello in modifica) l'FFI espone `schedule_occurrences(schedule, from, limit)`: le prime `limit` date dal giorno `from`, con la validazione di `CreateRecurring` e `end_date` come limite, senza toccare il database (dal 2026-10-07).
- La persona e il titolare (2026-10-08, schema v4): una transazione ha una **persona**, per chi è la riga, distinta dall'autore (`created_by`, chi l'ha registrata); un modello di ricorrenza ha un **titolare**. Colonne `transactions.person` e `recurring_templates.owner`, `TEXT NOT NULL DEFAULT ''`, riempite dalla migrazione con `created_by`. I comandi li nominano con `Entry.person`, `TransactionPatch.person`, `ExecuteRecurring.person`, `CreateRecurring.owner` e `RecurringPatch.owner`, tutti opzionali e contati da `is_empty`. Il **default si calcola all'applicazione** (persona assente o vuota = autore): un log scritto prima si rigioca identico, che è ciò che fa il backfill, e un payload senza persona non cambia di un byte. `ExecuteRecurring` porta la persona in modo esplicito (l'app manda il titolare): leggerlo dal modello all'applicazione cambierebbe al replay le esecuzioni già registrate. Un valore vuoto in una patch rimette la riga a `created_by`. Trasferimenti e saldi di apertura partono con persona = autore; una patch di persona su un trasferimento è rifiutata. Le letture: `TransactionView.person` accanto a `created_by`, `RecurringView.owner`, `people()` al posto di `authors()`, `TransactionFilter.person`; `scope`, `flow_person_totals` e `year_breakdown` raggruppano per persona. Il server rifiuta, comando per comando, un nome che non è un membro (`SYNC.md` §3).
- Il parser quick-add (§3.1 del distillato, più i token data `oggi`, `ieri`, `-3d`, `12/03`) vive nel core come `quick_add::parse` e `Core::resolve_quick_add`, che risolve i nomi di wallet e busta (esatto > prefisso > contiene) e produce un `Command`. Dal 2026-10-08 `!nome` (un `!` seguito da lettera o cifra) è il marcatore della persona: un `!` da solo resta nella nota, un secondo `!` dà `duplicate_marker`, in un trasferimento `marker_not_allowed`. `resolve_quick_add` prende l'elenco `people` che passa l'app e risolve con gli stessi livelli (esatto > prefisso > contiene); una persona ignota o ambigua dà `UnknownName` o `AmbiguousName`.

`UpdateTransaction` porta solo i campi cambiati: due utenti che modificano campi diversi della stessa riga non confliggono; sullo stesso campo vince l'ultimo in ordine di `seq`.

Regole di dominio: invarianti 1-11 del distillato §1.3. Cambiamenti rispetto alla v1:
- `VoidTransaction` non è mai bloccato da cap o non-negatività: annullare è sempre lecito, il saldo può andare dove va.
- La guardia di similarità sui nomi categoria suggerisce, non blocca.
- I saldi di apertura restano transazioni reali, ma con una categoria di sistema `opening`.
- Split di una spesa su più buste: ammesso dal modello a leg, rimandato alla UI.

## 5. Sincronizzazione e conflitti

1. Il client applica il comando in locale (ottimistico) e lo mette in `outbox`.
2. `push` al server. Il server assegna `seq`, applica con `core`, salva `applied` o `rejected`.
3. `pull(since)` riporta i comandi nuovi, compresi quelli degli altri membri.
4. Se il pull porta comandi che il client non aveva e il client ha ancora comandi in `outbox`, il client fa **rebase per replay**: proiezione = snapshot + log confermato + comandi nuovi + outbox. È il modello di `git push` dopo un `fetch`.
5. Un comando `rejected` (esempio: la busta Vacanze non copre più la spesa perché l'altro utente ha speso prima) viene tolto dall'outbox, la proiezione viene rifatta senza di lui e la UI mostra il motivo.

Perché non un CRDT: i CRDT convergono ma non sanno far rispettare un cap. Nella finanza personale il write contention è quasi nullo, quindi il rifiuto è raro e accettabile.

## 6. Utenti, vault, condivisione

Il **vault è l'unità di replica e di permesso**. `vault_memberships(vault, user, role ∈ owner|editor|viewer)` decide chi può fare pull e push. Modelli d'uso:

| Scenario | Modello |
|---|---|
| Tutto in comune | un vault, entrambi membri |
| Personale più comune | un vault a testa più un vault "Casa" con un wallet cointestato |
| Busta singola condivisa tra vault personali | non supportato in v2.0; si ottiene con il vault comune |

Muovere soldi dal personale al comune è un'uscita nel vault personale e un'entrata nel vault comune (due comandi in due log). Un comando `TransferVault` che li emetta insieme è un'estensione possibile.

Multi-tenant lato server: account utente, vault di proprietà di un account, membership. Il `flow_references` della v1 non viene portato.

## 7. Decisioni tecniche fissate

| Tema | Decisione |
|---|---|
| Id | UUID v7, BLOB(16) come in v1; ordinabili nel tempo |
| Importi | `i64` minor units; una valuta per vault (codice ISO sul vault) |
| Date | `occurred_at` RFC3339 con offset; giorno contabile calcolato con la timezone dell'app |
| Storage locale | un file SQLite per account, `vault_id` su ogni tabella |
| Errori | enum con codici stabili, esposto a Swift tal quale via UniFFI |
| Test | i 44 test dell'engine v1 riportati come acceptance test del core, più test di replay (stato = fold del log) |

## 8. Cronologia

| Data | Cosa |
|---|---|
| 2026-09-09 | crate `core`: modello, comandi, log, proiezione, query, 154 test |
| 2026-09-09 | UniFFI e app solo locale: `apple/SparagneCore` + `apple/Sparagne` (tabella, quick-add, void con undo, inspector, gestione wallet/buste/categorie, ricorrenze) |
| 2026-09-10 | server `sync` in axum (auth, membership, outbox e pull), sync nel core, account e condivisione nell'app; dettagli e rimandi in `SYNC.md` §7 |
| 2026-09-10 | libro mastro: griglia editabile, riepilogo e anno, aggregati nel core (`core/src/analytics.rs`); design in `UI.md` |
| 2026-09-12 e 13 | uso quotidiano: app contro il server vero, protocollo senza casi speciali, import v1, deploy, palette ⌘K e colonna wallet, core fuori dal main actor |
| 2026-09-23 | correzioni dell'audit dello stesso giorno e funzioni mancanti: nomi dei vault come etichette, account e limiti sul server, permessi e sola lettura nell'app, ricorrenze dovute, import di estratti conto, export completo e backup, selezione e operazioni in blocco, annulla/ripeti, completamento e suggerimento della categoria, icona e VoiceOver |
| 2026-10-08 | persona sulle transazioni e titolare sulle ricorrenze: schema v4, default calcolato all'applicazione, `!nome` nel quick-add, `people()` al posto di `authors()`, `relabel_outbox` che riscrive persone e titolari, `not_a_member` sul server; da rilasciare prima il server (`DEPLOY.md` §6.2) |

Rimandato: snapshot periodici della proiezione (§3), comandi `rejected` nel log (oggi un comando rifiutato non viene scritto), firma con un team Apple (oggi ad-hoc), target `x86_64-apple-darwin`. Il core fuori dal main actor è stato fatto il 2026-09-12 (§2.2).

La griglia di dashboard di `DISTILLATO_V1.md` §3.4 è stata scartata il
2026-09-10 a favore dei mockup: il riepilogo sta **accanto** alle righe, il
mese è l'unità di lettura e la tabella si edita in cella. Le formule di §3.5
restano e vivono in `core::analytics`. Design e tastiera in `UI.md`.

## 9. Punti aperti

- **Linguaggio del server: axum** (deciso il 2026-09-10; Elixir/Phoenix con NIF Rustler scartato per ora). Protocollo, API e algoritmo del client in `SYNC.md`.
- **Layout del repo.** Workspace Cargo alla radice con `core/` e `server/`; `apple/` contiene il Swift Package generato e il progetto Xcode. I crate v1 sono stati rimossi il 2026-09-09, chiuso il port dei test dell'engine; restano al tag `v0.93.0`.
- **Erlang/BEAM come core in-process: scartato.** La BEAM non si linka in un processo Swift; servirebbe un runtime da 40-50 MB lanciato come processo figlio e un IPC scritto a mano al posto di UniFFI.
