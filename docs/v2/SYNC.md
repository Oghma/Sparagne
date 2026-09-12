# Sparagne v2 — Sincronizzazione (Fase 3)

> Bozza 1, 2026-09-10. Server in Rust con axum (deciso il 2026-09-10; Elixir scartato per ora). Riferimenti: `ARCH.md` §3, §5, §6. I tipi di wire stanno in `core/src/sync.rs` e sono gli stessi per client e server.

## 1. Modello

- Il **server** tiene, per ogni vault, il log dei comandi con `seq` assegnato da lui, e la proiezione ottenuta applicandoli con lo stesso crate `core` del client. È l'unico punto in cui le regole valgono tra utenti diversi.
- Il **client** applica i comandi in locale (ottimista) e li tiene in **outbox** finché il server non li conferma. Il log locale ha `seq` (ordine locale) e `server_seq` (NULL finché non confermato). `outbox` = righe applicate con `server_seq IS NULL`, in ordine di `seq`.
- Il **vault è l'unità di replica e di permesso**: `vault_memberships(vault, user, role ∈ owner|editor|viewer)` vive nel server. Lettura = owner o membro; scrittura = owner o editor; gestione membri = owner. Le risorse altrui non esistono ("blind 404").
- L'**autore** di un comando è lo username dell'account. Il server rifiuta un push il cui `author` non coincide con l'utente autenticato (`403 author_mismatch`): così client e server producono la stessa proiezione (`created_by`, `owner_user_id`). Al login il client rietichetta i comandi in outbox con `relabel_outbox` e ricostruisce la proiezione.
- Il core espone anche il lato server del protocollo (`serve_push`, `serve_pull`, e le varianti JSON via FFI): il server axum ha la propria implementazione con le stesse regole, e l'app li usa per un finto server nei test. Il client non serializza mai un comando: il core produce e consuma il JSON di push e pull (`push_request_json`, `apply_push_response_json`, `integrate_pull_json`); Swift fa solo HTTP con `URLSession`.

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
| `POST /vaults/{vault_id}/push` | `PushRequest` → 200 `PushResponse` | owner, editor, o chiunque crei il vault | viewer `403 forbidden`; non membro `404 not_found`; `vault_id` dell'envelope diverso dal path `400 invalid_request`; `author` diverso `403 author_mismatch`; nome del vault duplicato `409 already_exists` |
| `GET /vaults/{vault_id}/pull?since=0&limit=500` | → 200 `PullResponse` | membro | `since` = ultimo seq noto; `limit` massimo 1000; `last_seq` è l'ultimo seq del vault (per sapere se c'è altro) |
| `GET /vaults/{vault_id}/members` | → `[MemberEntry]` | membro | |
| `PUT /vaults/{vault_id}/members` | `SetMemberRequest` → 204 | owner | upsert di editor o viewer; `role = owner` è `400 invalid_request`; cambiare il ruolo dell'owner esistente `403 forbidden`; utente inesistente `404 not_found` |
| `DELETE /vaults/{vault_id}/members/{username}` | → 204 | owner | l'owner non si rimuove |

Codici HTTP: `invalid_request`/`invalid_*` 400, `unauthorized` 401, `forbidden`/`author_mismatch`/`registration_disabled` 403, `not_found` 404, `already_exists` 409, errori di storage 500.

**Creazione del vault.** Non esiste una rotta per creare un vault: lo crea il suo stesso primo push. Se il `vault_id` del path è ignoto al server e il primo comando del lotto è il `CreateVault` che lo conia (`id` = `vault_id`), il server crea il vault nel core e la membership `owner` di chi chiama, nella stessa richiesta; il `PushResult` di quel comando sta in testa alla risposta come tutti gli altri. Un vault ignoto il cui primo comando non è quel `CreateVault` è `404 not_found`, esattamente come un vault che esiste ma di cui non si è membri: chi chiama non deve poter distinguere i due casi. Un nome di vault che chi chiama ha già usato è `409 already_exists`. Il push resta idempotente: rifarlo risponde gli stessi seq e non crea nulla di nuovo.

**Push.** I comandi vengono applicati nell'ordine ricevuto, ognuno con `Core::execute`; per ognuno la risposta dice `applied { seq, result_id }` o `rejected { code, message }`; un rifiuto non ferma il lotto (un comando che dipende da uno rifiutato verrà rifiutato a sua volta). Un comando già noto (stesso id) risponde `applied` con il seq originale: il push è idempotente. I comandi rifiutati non entrano nel log del server. Il client spezza l'outbox in lotti da 500 comandi (`push_request(vault, limit)`) e ripete finché l'outbox non è vuota.

**Pull.** Restituisce i comandi con `seq > since`, compresi quelli dello stesso client già confermati (il client li riconosce dall'id).

## 4. Algoritmo del client (nel core)

Stato per vault: `last_server_seq` = **watermark contiguo** (il più alto `server_seq` tale che tutti i precedenti sono presenti in locale, non il massimo: dopo un push confermato oltre un buco lasciato da un altro membro, il pull deve ripartire dal buco), outbox, righe `rejected`.

1. **Push.** `push_request(vault, limit)` = i primi `limit` comandi dell'outbox, in ordine locale; il client spinge a lotti (500) finché `sync_state(vault).outbox` non è zero, fermandosi se un lotto non accorcia l'outbox. `apply_push_response`: per ogni risultato `applied` scrive `server_seq`; per ogni `rejected` marca la riga `status = rejected` con il motivo, poi ricostruisce la proiezione senza di essa. Sia le conferme sia i rifiuti tolgono comandi dall'outbox, quindi il ciclo termina. Dopo un push il client fa sempre un pull.
2. **Pull.** `integrate_pull(vault, response)`: ignora i record con `seq <= last_server_seq`. Ogni `SyncReport` (di push o di pull) porta `server_last_seq`, l'ultimo seq che il server ha dichiarato nel corpo, e `has_more`, vero quando quel seq sta oltre il watermark locale dopo l'integrazione: è così che l'app sa se chiedere un'altra pagina, senza mai guardare dentro il JSON. Se i record restanti sono un **prefisso dell'outbox** (stessi id, stesso ordine, confrontati a coppie) → **fast path**: scrive i `server_seq`. Altrimenti → **rebase**: in una sola transazione SQLite cancella proiezione e log del vault, riapplica nell'ordine del server i comandi confermati locali più quelli ricevuti (devono tutti applicarsi: se uno fallisce è una divergenza, errore `storage`), poi riesegue l'outbox rimasta in ordine locale; i comandi che ora falliscono diventano `rejected` e finiscono nel `SyncReport`. Le righe `rejected` precedenti vengono conservate e reinserite in coda al log rinumerato; se un record in arrivo ha l'id di una riga `rejected` locale, vince il server e la riga viene tolta. I comandi riapplicati conservano il `created_at` originale (quello del server per i confermati), così i timestamp convergono.
3. **Vault condivisi.** Un vault di cui si diventa membri non esiste in locale: `integrate_pull` da `since = 0` lo crea, perché il primo comando del log è `CreateVault`. `sync_state` di un vault sconosciuto **risponde zeri invece di errore**, ed è voluto: il join legge lo stato prima che il primo pull crei il vault, e `last_server_seq = 0` è esattamente il punto da cui partire. Lo stesso vale per `last_seq`.
4. **Login.** `relabel_outbox(vault, username)` aggiorna l'`author` delle righe in outbox e ricostruisce la proiezione.
5. **Rifiuti.** `rejected_commands(vault)` elenca i comandi rifiutati (id, kind, codice, messaggio) per la UI; `dismiss_rejected` li toglie.

Gli id delle entità derivano dall'id del comando (`ARCH.md` §4), quindi il rebase produce gli stessi id su ogni client e sul server.

## 5. App

- Impostazioni: URL del server, login e registrazione, logout. Lo username del server diventa l'`author` di ogni nuovo comando.
- Sync automatica: all'avvio, dopo ogni comando (con un piccolo debounce) e ogni 60 s: per ogni vault locale push poi pull; per i vault del server non ancora locali pull da 0. Stato nella toolbar (in sync, in attesa, offline, errore); i rifiuti in un alert e in un elenco consultabile.
- Condivisione: foglio "Condividi vault…" (solo owner) con elenco membri, aggiunta per username e ruolo, rimozione.

## 6. Test di accettazione

**Server** (`server/tests`). Due `Core` in memoria come client A e B contro il router in-process: A registra, crea il vault, un wallet e un'entrata e li manda con un solo push, che è anche ciò che crea il vault sul server; B registra e l'owner lo aggiunge come editor (l'utente deve esistere prima); B fa pull da 0 (ottiene il vault), aggiunge una spesa e fa push; A fa pull e ricostruisce (rebase); le proiezioni di A, B e del server coincidono (`snapshot`, `list_transactions`, `categories`). Più: push idempotente; spesa oltre il saldo rifiutata dal server e tolta dalla proiezione di chi l'ha emessa; viewer che non può fare push; non membro che riceve 404; `author` diverso rifiutato; token scaduto o revocato → 401.

**L'app contro il server vero** (`apple/Sparagne/SparagneTests/ServerE2ETests.swift`).
Lo stesso scenario con l'app intera e nessun finto server: due `AppStore` su
due file SQLite temporanei, ognuno col suo `SyncEngine` e il transport vero
(`URLSessionTransport`), contro un `sparagne-server` in ascolto su una porta
libera. A registra, crea vault, wallet, busta e un'entrata dall'API normale
dell'app e sincronizza (il push conia il vault); aggiunge B come editor; B
sincronizza da zero e riceve il vault, scrive una spesa e risincronizza; A
riconverge. Si confrontano snapshot, elenco completo delle transazioni e
`GET /vaults` (id, owner, ruolo) fra i due. Un secondo test ripete la
condivisione con un `viewer`: il push torna `403 forbidden`, l'engine lo
riporta come stato di errore e il comando resta in outbox.

La suite gira solo se `SPARAGNE_E2E_SERVER` è impostata, altrimenti si salta.
La imposta `scripts/e2e.sh`, che compila il server, lo avvia su una porta
libera con `SPARAGNE_DATA_DIR` temporanea e `SPARAGNE_ALLOW_REGISTRATION=true`,
aspetta `GET /health`, lancia `xcodebuild ... -only-testing:SparagneTests/ServerE2ETests`
passando l'indirizzo come `TEST_RUNNER_SPARAGNE_E2E_SERVER` (xcodebuild
inoltra al processo di test le variabili d'ambiente con quel prefisso,
togliendolo) e ferma il server uscendo. `DERIVED_DATA` diventa
`-derivedDataPath`, per non litigare con un'altra build. Gli username sono
casuali a ogni esecuzione, così lo stesso server si può riusare. In CI è
l'ultimo passo del job `apple`.

## 7. Stato e punti rimandati

Fase 3 completata il 2026-09-10. Protocollo ripulito dai casi speciali il
2026-09-12 (pacchetto P2 di `ROADMAP.md`): `POST /vaults` non esiste più, il
push accetta `CreateVault` come primo comando di un vault ignoto, l'outbox sale
a lotti, il `SyncReport` porta `server_last_seq` e `has_more`, e il core espone
`vault(id)` e `last_seqs()` per `GET /vaults`. Swift non legge più dentro
nessun corpo JSON. Copertura: `server/` 42 test (4 end-to-end a due client),
sync lato client nel core 16 test più il finto server, app 102 test su un finto
server fatto da un secondo `CoreHandle`, di cui i 2 end-to-end di §6 girano
solo contro un server vero.

Il percorso HTTP vero fra app e server (`URLSession` verso axum) è esercitato
dal 2026-09-12 (pacchetto P1): `URLSessionTransport`, `ServerAPI` e le rotte
del server si sono trovati d'accordo al primo colpo, nessuna divergenza da
correggere. App Transport Security non ha richiesto nulla, perché il loopback
è esente: `project.yml` resta senza blocco `info`. Verso un server non locale
in chiaro ATS bloccherebbe invece la chiamata, ed è la ragione per cui
`DEPLOY.md` mette il TLS fra i requisiti.

Ancora da fare quando servirà:

- `RejectedCommand.kind` è il nome snake_case del comando; l'app lo mostra tal quale accanto al messaggio localizzato.
- Il Keychain con firma ad-hoc può rifiutare `SecItemAdd`: il token resta in memoria per la sessione. Con un team Apple il problema sparisce.
- Il server non tiene i comandi rifiutati, solo la risposta al push.