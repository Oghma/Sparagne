# Sparagne v2 — Sincronizzazione (Fase 3)

> Bozza 1, 2026-09-10. Server in Rust con axum (deciso il 2026-09-10; Elixir scartato per ora). Riferimenti: `ARCH.md` §3, §5, §6. I tipi di wire stanno in `core/src/sync.rs` e sono gli stessi per client e server.

## 1. Modello

- Il **server** tiene, per ogni vault, il log dei comandi con `seq` assegnato da lui, e la proiezione ottenuta applicandoli con lo stesso crate `core` del client. È l'unico punto in cui le regole valgono tra utenti diversi.
- Il **client** applica i comandi in locale (ottimista) e li tiene in **outbox** finché il server non li conferma. Il log locale ha `seq` (ordine locale) e `server_seq` (NULL finché non confermato). `outbox` = righe applicate con `server_seq IS NULL`, in ordine di `seq`.
- Il **vault è l'unità di replica e di permesso**: `vault_memberships(vault, user, role ∈ owner|editor|viewer)` vive nel server. Lettura = owner o membro; scrittura = owner o editor; gestione membri = owner. Le risorse altrui non esistono ("blind 404").
- L'**autore** di un comando è lo username dell'account. Il server rifiuta un push il cui `author` non coincide con l'utente autenticato (`403 author_mismatch`): così client e server producono la stessa proiezione (`created_by`, `owner_user_id`). Al login il client rietichetta i comandi in outbox con `relabel_outbox` e ricostruisce la proiezione.
- Il client non serializza mai un comando: il core produce e consuma il JSON di push e pull (`push_request_json`, `apply_push_response_json`, `integrate_pull_json`); Swift fa solo HTTP con `URLSession`.

## 2. Storage del server

Due file SQLite in `SPARAGNE_DATA_DIR`:
- `vaults.sqlite`: il `Core` (schema del core, `commands.seq` è il seq del server).
- `server.sqlite`: `users(id, username UNIQUE, password_hash argon2id, created_at)`, `tokens(hash sha256 PK, user_id, created_at, expires_at)`, `vault_memberships(vault_id, user_id, role, created_at, PK(vault_id, user_id))`.

Un `Mutex<Core>` serializza tutti i comandi (SQLite ha un solo scrittore); le chiamate al core stanno in `spawn_blocking`. Configurazione via ambiente: `SPARAGNE_BIND` (default `127.0.0.1:3000`), `SPARAGNE_DATA_DIR` (default `./data`), `SPARAGNE_ALLOW_REGISTRATION` (default `true`), `SPARAGNE_TOKEN_TTL_DAYS` (default `30`), `RUST_LOG`. TLS e rate limiting li fa il reverse proxy.

## 3. API HTTP

JSON ovunque; autenticazione `Authorization: Bearer <token>` tranne dove indicato. Errori a livello di richiesta come `ErrorBody { error: { code, message } }`.

| Metodo e path | Corpo → risposta | Chi | Note |
|---|---|---|---|
| `GET /health` | → `{"status":"ok"}` | nessuno | |
| `POST /auth/register` | `Credentials` → 201 `TokenResponse` | nessuno | `403 registration_disabled`, `409 already_exists`, `400 invalid_request` (username 3-32 `[a-z0-9_.-]`, password ≥ 8) |
| `POST /auth/login` | `Credentials` → 200 `TokenResponse` | nessuno | `401 unauthorized` |
| `POST /auth/logout` | → 204 | bearer | revoca il token |
| `GET /me` | → `{ "username" }` | bearer | |
| `GET /vaults` | → `[VaultSummary]` | bearer | i vault di cui sono owner o membro, con `role` e `last_seq` |
| `POST /vaults` | `CommandEnvelope` (`CreateVault`, `author` = me, `vault_id` = `id`) → 201 `PushResult` | bearer | crea anche la membership `owner`; `409 already_exists` per nome duplicato |
| `POST /vaults/{vault_id}/push` | `PushRequest` → 200 `PushResponse` | owner, editor | viewer `403 forbidden`; non membro `404 not_found`; `vault_id` dell'envelope diverso dal path `400 invalid_request`; `author` diverso `403 author_mismatch` |
| `GET /vaults/{vault_id}/pull?since=0&limit=500` | → 200 `PullResponse` | membro | `since` = ultimo seq noto; `limit` massimo 1000; `last_seq` è l'ultimo seq del vault (per sapere se c'è altro) |
| `GET /vaults/{vault_id}/members` | → `[MemberEntry]` | membro | |
| `PUT /vaults/{vault_id}/members` | `SetMemberRequest` → 204 | owner | upsert; l'owner non si tocca (`403 forbidden`); utente inesistente `404 not_found` |
| `DELETE /vaults/{vault_id}/members/{username}` | → 204 | owner | l'owner non si rimuove |

Codici HTTP: `invalid_request`/`invalid_*` 400, `unauthorized` 401, `forbidden`/`author_mismatch`/`registration_disabled` 403, `not_found` 404, `already_exists` 409, errori di storage 500.

**Push.** I comandi vengono applicati nell'ordine ricevuto, ognuno con `Core::execute`; per ognuno la risposta dice `applied { seq, result_id }` o `rejected { code, message }`; un rifiuto non ferma il lotto (un comando che dipende da uno rifiutato verrà rifiutato a sua volta). Un comando già noto (stesso id) risponde `applied` con il seq originale: il push è idempotente. I comandi rifiutati non entrano nel log del server.

**Pull.** Restituisce i comandi con `seq > since`, compresi quelli dello stesso client già confermati (il client li riconosce dall'id).

## 4. Algoritmo del client (nel core)

Stato per vault: `last_server_seq = max(server_seq)`, outbox, righe `rejected`.

1. **Push.** `push_request(vault)` = outbox. `apply_push_response`: per ogni risultato `applied` scrive `server_seq`; per ogni `rejected` marca la riga `status = rejected` con il motivo, poi ricostruisce la proiezione senza di essa. Dopo un push il client fa sempre un pull.
2. **Pull.** `integrate_pull(vault, response)`: ignora i record con `seq <= last_server_seq`. Se tutti i record restanti sono comandi locali in outbox e nello stesso ordine relativo → **fast path**: scrive i `server_seq`. Altrimenti → **rebase**: in una sola transazione SQLite cancella proiezione e log del vault, riapplica nell'ordine del server i comandi confermati locali più quelli ricevuti (devono tutti applicarsi: se uno fallisce è una divergenza, errore `storage`), poi riesegue l'outbox rimasta in ordine locale; i comandi che ora falliscono diventano `rejected` e finiscono nel `SyncReport`. Le righe `rejected` precedenti vengono conservate.
3. **Vault condivisi.** Un vault di cui si diventa membri non esiste in locale: `integrate_pull` da `since = 0` lo crea, perché il primo comando del log è `CreateVault`.
4. **Login.** `relabel_outbox(vault, username)` aggiorna l'`author` delle righe in outbox e ricostruisce la proiezione.
5. **Rifiuti.** `rejected_commands(vault)` elenca i comandi rifiutati (id, kind, codice, messaggio) per la UI; `dismiss_rejected` li toglie.

Gli id delle entità derivano dall'id del comando (`ARCH.md` §4), quindi il rebase produce gli stessi id su ogni client e sul server.

## 5. App

- Impostazioni: URL del server, login e registrazione, logout. Lo username del server diventa l'`author` di ogni nuovo comando.
- Sync automatica: all'avvio, dopo ogni comando (con un piccolo debounce) e ogni 60 s: per ogni vault locale push poi pull; per i vault del server non ancora locali pull da 0. Stato nella toolbar (in sync, in attesa, offline, errore); i rifiuti in un alert e in un elenco consultabile.
- Condivisione: foglio "Condividi vault…" (solo owner) con elenco membri, aggiunta per username e ruolo, rimozione.

## 6. Test di accettazione (in `server/tests`)

Due `Core` in memoria come client A e B contro il router in-process: A registra, crea il vault, un wallet e un'entrata e fa push; l'owner aggiunge B come editor; B fa pull da 0 (ottiene il vault), aggiunge una spesa e fa push; A fa pull e ricostruisce (rebase); le proiezioni di A, B e del server coincidono (`snapshot`, `list_transactions`, `categories`). Più: push idempotente; spesa oltre il saldo rifiutata dal server e tolta dalla proiezione di chi l'ha emessa; viewer che non può fare push; non membro che riceve 404; `author` diverso rifiutato; token scaduto o revocato → 401.
