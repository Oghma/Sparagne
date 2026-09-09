# Sparagne v2 — Architettura

> Bozza 1, 2026-09-09. Decisioni prese: branch `v2` in questo repo; UI Swift/SwiftUI solo macOS; server di sync in una fase successiva. Il dominio è quello del `DISTILLATO_V1.md` §1, salvo dove indicato.

## 1. Idea in una frase

L'app lavora su un SQLite locale attraverso un core Rust in-process; ogni scrittura è un **comando** appeso a un **log per vault**; la condivisione tra utenti è la replica di quel log tramite un server piccolo che applica gli stessi comandi con lo stesso core e fa da arbitro.

## 2. Componenti

```
┌──────────────────────────────┐        ┌──────────────────────────────┐
│  App macOS (Swift/SwiftUI)   │        │  sync (Fase 3)               │
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
- Contiene: tipi di dominio, `Money`, normalizzazione categorie, `apply_leg_change`, i comandi, la proiezione (tabelle di stato), le query per la UI, il parser quick-add, le aggregazioni per dashboard.
- Esposto a Swift con **UniFFI** (Swift Package generato). Swift non fa mai SQL e non conosce le regole: chiama `execute(cmd)` e `query_*`.
- Lo stesso crate gira nel server (Fase 3) come dipendenza di axum o come NIF Rustler dentro Elixir. Il linguaggio del server è una decisione rimandata (§9).

### 2.2 App macOS

- SwiftUI, `@Observable` store che incapsula il core. `Table` per la vista transazioni; NSTableView via `NSViewRepresentable` solo se l'editing in cella lo richiede.
- L'app non ha stato di dominio proprio: ogni vista è una query sul core, ogni azione è un comando.
- Timezone di sistema; i comandi portano `occurred_at` come RFC3339 con offset.

### 2.3 `sync` (Fase 3)

- Autentica (argon2 + token opachi), mantiene il log di ogni vault, espone `push(vault, comandi)` e `pull(vault, since_seq)`.
- Applica ogni comando ricevuto con `core` prima di accettarlo: è l'unico punto dove le regole vengono fatte rispettare tra utenti diversi.
- Serializza i comandi per vault (un lock per vault in Rust, o un processo per vault sulla BEAM).

## 3. Il log dei comandi

Il log è la verità; le tabelle di stato (wallet, buste, transazioni, leg, saldi) sono una proiezione ricostruibile. È la generalizzazione del `recompute_balances` della v1.

```
commands
  id              BLOB(16) PK   -- UUID v7 generato dal client; è anche la idempotency key
  vault_id        BLOB(16)
  seq             INTEGER       -- ordine totale nel vault; assegnato dal server (Fase 1: locale)
  author_user_id  TEXT
  kind            TEXT          -- vedi §4
  payload         TEXT (JSON)
  occurred_at     TEXT          -- RFC3339 con offset, quando l'utente dice che è successo
  created_at      TEXT          -- RFC3339, quando è stato emesso
  status          TEXT          -- applied | rejected
  rejection       TEXT NULL
UNIQUE(vault_id, seq)
```

Stato locale aggiuntivo (Fase 3): `outbox` (comandi emessi e non ancora confermati dal server) e `vault_sync(vault_id, last_server_seq)`.

**Applicazione.** `execute(cmd)`: valida contro la proiezione corrente, scrive comando e proiezione nella stessa transazione SQLite. Se una regola fallisce, non viene scritto nulla e il chiamante riceve un `DomainError` con codice stabile (stessa famiglia di codici della v1).

**Snapshot.** Ogni N comandi il core salva uno snapshot della proiezione; il replay riparte dall'ultimo snapshot. Con volumi da finanza personale (migliaia di comandi) il replay completo resta comunque nell'ordine dei millisecondi.

## 4. Comandi

Tutto ciò che muta un vault è un comando; niente scritture dirette alle tabelle. Regola degli id: **ogni entità creata da un comando ha un id derivato dal comando** (uguale all'id del comando, oppure UUID v5 dell'id del comando con un ruolo: `opening`, `category`, `unallocated`). Così il replay del log produce gli stessi id senza stato esterno.

| Famiglia | Comandi |
|---|---|
| Wallet | `CreateWallet`, `RenameWallet`, `ArchiveWallet` |
| Busta | `CreateFlow`, `UpdateFlow` (nome, mode, cap, allow_negative), `ArchiveFlow` |
| Categoria | `CreateCategory`, `UpdateCategory`, `AddAlias`, `RemoveAlias`, `MergeCategory` |
| Transazione | `Income`, `Expense`, `Refund`, `TransferWallet`, `TransferFlow`, `UpdateTransaction`, `VoidTransaction` |
| Ricorrenze | `CreateRecurring`, `UpdateRecurring`, `ArchiveRecurring`, `ExecuteRecurring`, `SkipRecurring` |
| Vault | `CreateVault` (unico comando fuori dal log del vault; vive nel log dell'account) |

`UpdateTransaction` porta solo i campi cambiati: due utenti che modificano campi diversi della stessa riga non confliggono; sullo stesso campo vince l'ultimo in ordine di `seq`.

Regole di dominio: invarianti 1-11 del distillato §1.3. Cambiamenti rispetto alla v1:
- `VoidTransaction` non è mai bloccato da cap o non-negatività: annullare è sempre lecito, il saldo può andare dove va.
- La guardia di similarità sui nomi categoria suggerisce, non blocca.
- I saldi di apertura restano transazioni reali, ma con una categoria di sistema `opening`.
- Split di una spesa su più buste: ammesso dal modello a leg, rimandato alla UI.

## 5. Sincronizzazione e conflitti (Fase 3)

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

## 8. Fasi

| Fase | Cosa | Esito |
|---|---|---|
| 0 | SPEC v2 breve (dominio v1 + §3-§4 di questo documento) | documento |
| 1 | crate `core`: modello, comandi, log, proiezione, query, test | libreria testata, nessuna UI |
| 2 | UniFFI, Swift Package, app con tabella transazioni e quick-add, solo locale | app usabile da un utente |
| 3 | server `sync`, auth, membership, outbox e pull | secondo utente |
| 4 | dashboard e analytics con query sul SQLite locale | stile bancario |

La Fase 1 già scrive il log con i campi di §3, anche se `seq` è locale e `outbox` non esiste.

## 9. Punti aperti

- **Linguaggio del server.** axum con `core` come dipendenza, oppure Elixir/Phoenix con `core` come NIF Rustler (processo per vault, canali per il push). Non tocca l'app: si decide in Fase 3.
- **Layout del repo.** `core/` (Rust, workspace a sé: i crate v1 pinnano una `libsqlite3-sys` più vecchia tramite sea-orm e Cargo ammette un solo link nativo a `sqlite3` per workspace), `apple/` (Xcode + Swift Package generato), `server/` (Fase 3). I crate v1 restano in `crates/`, esclusi dal build del core, finché il core non copre i 44 test, poi vengono rimossi.
- **Erlang/BEAM come core in-process: scartato.** La BEAM non si linka in un processo Swift; servirebbe un runtime da 40-50 MB lanciato come processo figlio e un IPC scritto a mano al posto di UniFFI.
