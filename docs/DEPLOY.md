# Sparagne v2 — Messa in produzione del server

> 2026-09-12, aggiornato il 2026-09-23 (limiti ai tentativi, account da
> riga di comando) e il 2026-10-08 (immagine pubblicata su GHCR, §3.2).
> Riferimenti: `server/Dockerfile`, `server/deploy/`,
> `.github/workflows/release.yml`.

## 1. TLS obbligatorio

L'app macOS usa App Transport Security, che rifiuta `http://` verso
qualunque host che non sia `localhost`. Il server quindi **sta sempre dietro
un reverse proxy con certificato**: `server/deploy/compose.yml` include
Caddy, che ottiene e rinnova il certificato Let's Encrypt da solo (bastano
una porta 80/443 raggiungibile e un DNS che punta al dominio). L'URL che si
digita nelle impostazioni dell'app è `https://il-tuo-dominio`, mai un IP o
`http://`.

## 2. Variabili d'ambiente

| Variabile | Default (nel container) | Significato |
|---|---|---|
| `SPARAGNE_BIND` | `0.0.0.0:3000` | indirizzo:porta di ascolto. Nell'immagine Docker è già `0.0.0.0:3000` (Caddy fa da front); su bare metal si preferisce `127.0.0.1:3000` col proxy sulla stessa macchina. |
| `SPARAGNE_DATA_DIR` | `/data` | cartella con `vaults.sqlite` e `server.sqlite`. |
| `SPARAGNE_ALLOW_REGISTRATION` | `true` | se `false`, `POST /auth/register` risponde `403 registration_disabled`. |
| `SPARAGNE_TOKEN_TTL_DAYS` | `30` | validità di un token di login. |
| `SPARAGNE_TRUST_PROXY` | `false` (`true` in `compose.yml`) | se `true`, l'indirizzo del client è l'ultima voce di `X-Forwarded-For` invece del peer TCP (§2.1). |
| `SPARAGNE_LOGIN_MAX_FAILURES` | `5` | login falliti per uno username dentro la finestra, dopo i quali lo username è bloccato per un'altra finestra; `0` toglie il limite. |
| `SPARAGNE_LOGIN_WINDOW_SECS` | `900` | la finestra dei due limiti sul login, in secondi. |
| `SPARAGNE_IP_MAX_FAILURES` | `30` | login falliti da un indirizzo dentro la finestra; `0` toglie il limite. |
| `RUST_LOG` | (vuoto, nessun filtro esplicito) | sintassi `tracing-subscriber` env-filter, es. `info` o `sparagne_server=debug,info`. |

Il TLS lo fa il reverse proxy; i limiti ai tentativi li fa il server stesso
(§2.1), il proxy non ne ha.

### 2.1 Limiti ai tentativi

Il server frena chi prova a indovinare una password, in memoria (un riavvio
li azzera) e prima di calcolare qualsiasi hash:

- **Per username**: 5 login falliti in 15 minuti bloccano lo username per 15
  minuti. Mentre è bloccato, anche la password giusta riceve `429
  too_many_requests` con l'header `Retry-After` (secondi). Un login riuscito
  azzera il conteggio. Uno username che non esiste si blocca allo stesso
  modo, così il blocco non rivela quali account ci sono; anche la password
  attuale sbagliata in `POST /auth/password` conta come un login fallito.
- **Per indirizzo**: 30 login falliti in 15 minuti dallo stesso indirizzo
  (per IPv6, dalla stessa /64) → `429`, qualunque sia lo username.
- **Registrazione**: al massimo 10 tentativi l'ora per indirizzo, validi o no.

**Quale indirizzo.** Senza proxy è il peer della connessione TCP. Dietro un
proxy il peer è sempre il proxy, e tutti i client finirebbero nello stesso
conteggio: con `SPARAGNE_TRUST_PROXY=true` il server usa invece l'**ultima**
voce di `X-Forwarded-For`, quella che il proxy aggiunge per conto suo e che
il client non può falsificare (le voci più a sinistra le può scrivere
chiunque). `compose.yml` lo accende perché la porta 3000 non è pubblicata e
solo Caddy raggiunge il container; Caddy imposta `X-Forwarded-For` da solo.
Su bare metal va acceso solo se il server ascolta su `127.0.0.1` (o su una
porta che il firewall apre al solo proxy) e il proxy aggiunge l'indirizzo
del client: Caddy lo fa di default, nginx con `proxy_set_header
X-Forwarded-For $proxy_add_x_forwarded_for;`. **Mai** con la porta del
server raggiungibile direttamente: chiunque sceglierebbe il proprio
indirizzo e il limite per indirizzo non varrebbe più nulla (quello per
username resta).

## 3. Primo avvio

1. `cd server/deploy && cp .env.example .env`, impostare `DOMAIN`,
   `SPARAGNE_VERSION` (la release da usare, §3.2) e le altre variabili.
2. In `Caddyfile`, sostituire `sparagne.example.com` col dominio vero.
3. `docker compose up -d` (scarica `ghcr.io/oghma/sparagne-server` alla
   versione di `.env` e avvia `sparagne` + `caddy`). Sull'host bastano
   `server/deploy/` e Docker: niente sorgenti né Rust.
4. Verificare `curl https://il-tuo-dominio/health` → `{"status":"ok"}`.
5. Creare gli account che servono: con `SPARAGNE_ALLOW_REGISTRATION=true`
   (default) dalla schermata di registrazione dell'app o da `POST
   /auth/register`; in ogni caso con la CLI (§3.1).
6. Chiudere la registrazione: in `.env` impostare
   `SPARAGNE_ALLOW_REGISTRATION=false`, poi `docker compose up -d` di nuovo
   (ricrea solo il container `sparagne` con la nuova variabile). Da qui in
   poi i nuovi account si creano solo con la CLI (§3.1). **Condividere un
   vault non crea account**: `PUT /vaults/{id}/members` con uno username che
   non esiste risponde `404 not_found`, quindi l'account va creato prima.

### 3.1 Account da riga di comando

Lo stesso binario gestisce gli account: `sparagne-server` (o
`sparagne-server serve`) avvia il server, `sparagne-server user …` lavora
sugli account e esce. Apre solo `server.sqlite` nella cartella dei dati
(`--data-dir`, altrimenti `SPARAGNE_DATA_DIR`, altrimenti `./data`) e rifiuta
una cartella che non lo contiene invece di crearne uno vuoto. Si usa col
server acceso: SQLite in WAL con un busy timeout regge i due processi, e un
token revocato smette di funzionare alla richiesta successiva. Ignora
`SPARAGNE_ALLOW_REGISTRATION`: è proprio il modo di creare account a
registrazione chiusa.

| Comando | Effetto |
|---|---|
| `user add <nome>` | crea l'account; password dalla prima riga di stdin |
| `user passwd <nome>` | nuova password dalla prima riga di stdin, e revoca tutti i token dell'account (va rifatto il login ovunque) |
| `user list` | un account per riga: username, tab, data di creazione (UTC) |
| `user revoke <nome>` | revoca tutti i token dell'account, la password resta |

Username e password seguono le regole della registrazione (username 3-32
caratteri `[a-z0-9_.-]`, portato in minuscolo; password di almeno 8
caratteri). La password non è mai un argomento, così non finisce nella
history della shell né in `ps`. Con Docker Compose, da `server/deploy/`
(`-T` serve a passare stdin a `exec`):

```sh
read -rs PW    # digitata senza eco
printf '%s\n' "$PW" | docker compose exec -T sparagne sparagne-server user add alice
printf '%s\n' "$PW" | docker compose exec -T sparagne sparagne-server user passwd alice
docker compose exec sparagne sparagne-server user list
docker compose exec sparagne sparagne-server user revoke alice
```

Su bare metal, come utente del servizio (così i file `-wal`/`-shm` di SQLite
restano suoi):

```sh
printf '%s\n' "$PW" | sudo -u sparagne env SPARAGNE_DATA_DIR=/var/lib/sparagne \
    /usr/local/bin/sparagne-server user add alice
```

In caso di errore il comando scrive il motivo su stderr ed esce con 1 (2 per
un comando scritto male). Dall'app un utente cambia la propria password con
`POST /auth/password`, che chiede quella attuale.

### 3.2 L'immagine e da dove viene

Ogni tag di versione (`v2.0.0`) pubblica l'immagine per linux/amd64 e
linux/arm64 (`.github/workflows/release.yml`), con i tag `2.0.0`, `2.0` e
`latest`; una pre-release (`v2.1.0-beta.1`) solo col proprio. `compose.yml`
la prende per numero di versione, mai `latest`, così un aggiornamento è
una scelta fatta dopo un backup (§6).

Il workflow allega all'immagine un'attestazione di provenienza firmata da
GitHub: dice che è stata costruita da quel workflow, su quel commit del
repository. Prima di usarla (o di aggiornare):

```sh
gh attestation verify oci://ghcr.io/oghma/sparagne-server:2.0.0 --repo Oghma/Sparagne
```

Per costruirla invece dai sorgenti (una modifica non ancora rilasciata, un
host che non deve scaricare nulla): dal checkout del repository,
`docker compose -f compose.yml -f compose.build.yml up -d --build`
(`server/deploy/compose.build.yml`).

### Bare metal (senza Docker)

`server/deploy/sparagne-server.service` è un'unit systemd pronta all'uso: il
commento in testa al file elenca i comandi (utente dedicato, percorso del
binario, `EnvironmentFile`). In questo caso il reverse proxy (Caddy, nginx,
…) è un servizio a parte sulla stessa macchina o su un'altra; per
`SPARAGNE_TRUST_PROXY` vedi §2.1.

## 4. Backup

`server/deploy/backup.sh` fa un backup online (nessun downtime) di
`vaults.sqlite` e `server.sqlite` con `sqlite3 <db> ".backup '<dest>'"`,
verifica l'integrità del dump con `PRAGMA integrity_check`, e applica una
retention in giorni.

- **Docker Compose**: lo script e `sqlite3` sono già dentro l'immagine
  (`server/Dockerfile`); `compose.yml` monta `./backups` sull'host su
  `/backups` nel container. Da `server/deploy/`:

  ```sh
  docker compose exec sparagne backup.sh
  ```

  Il dump compare direttamente in `server/deploy/backups/<timestamp>/`
  sull'host. Per la retention: `docker compose exec sparagne env
  RETENTION_DAYS=30 backup.sh`.

- **Bare metal**:

  ```sh
  DATA_DIR=/var/lib/sparagne BACKUP_DIR=/var/backups/sparagne \
      server/deploy/backup.sh
  ```

Schedularlo con cron o un timer systemd, secondo la piattaforma.

## 5. Ripristino

1. Fermare il server (`docker compose stop sparagne`, oppure `systemctl stop
   sparagne-server` su bare metal): i due file SQLite non vanno toccati
   mentre il processo scrive.
2. Copiare `vaults.sqlite` e `server.sqlite` dal backup scelto sopra i file
   in `SPARAGNE_DATA_DIR`, sovrascrivendoli (togliere anche eventuali
   `-wal`/`-shm` residui dei vecchi file, così SQLite riparte da uno stato
   pulito).
3. Riavviare il server.

Un dump di `.backup` è un file SQLite completo e coerente: nessun replay o
migrazione manuale serve per usarlo, a parte l'avvio normale (§6).

## 6. Aggiornamento

1. Backup (§4), poi:
   - Compose: in `.env` portare `SPARAGNE_VERSION` alla release nuova
     (verificandola, §3.2), poi `docker compose pull sparagne && docker
     compose up -d sparagne` (Caddy resta com'è). Da un'installazione che
     costruiva l'immagine dai sorgenti, prima della 2.0.0: aggiornare anche
     `compose.yml` e aggiungere `SPARAGNE_VERSION` a `.env` (`.env.example`).
   - Compose dai sorgenti: `git pull`, poi `docker compose -f compose.yml -f
     compose.build.yml up -d --build sparagne`.
   - Bare metal: ricompilare (`cargo build --release -p sparagne_server`),
     sostituire il binario, `systemctl restart sparagne-server`.
2. Lo schema di `vaults.sqlite` **si aggiorna da solo all'avvio**: `Core::open`
   (`core/src/store.rs`) legge `PRAGMA user_version`, applica la migrazione
   mancante se la versione sul disco è più vecchia della versione del codice
   e aggiorna `user_version` di conseguenza, nella stessa apertura di
   connessione. Non c'è un comando di migrazione separato da lanciare: basta
   avviare il binario nuovo sui file esistenti. Se `user_version` sul disco
   fosse più recente della versione che il binario conosce (upgrade poi
   downgrade), l'apertura fallisce con un errore esplicito invece di
   corrompere i dati: in quel caso ripristinare un binario aggiornato o un
   backup precedente.
3. Fare comunque un backup (§4) prima di un aggiornamento importante.

### 6.1 Passaggio allo schema v3

La versione del 2026-09-23 porta lo schema del core alla versione 3 (i nomi
dei vault diventano etichette e possono ripetersi) e aggiunge all'API il
cambio password, l'uscita da un vault e i `429` dei limiti (§2.1). L'ordine
conta:

1. **Backup del server** (§4): `docker compose exec sparagne backup.sh`, o
   `backup.sh` su bare metal.
2. **Prima tutte le app, poi il server.** Aggiornare l'app su ogni Mac che
   sincronizza, e solo dopo il server (§6). Al contrario, un'app vecchia che
   riceve col pull due vault con lo stesso nome (il server nuovo li accetta)
   diverge dal server.
3. **Il ritorno indietro è un ripristino.** Un database v3 non si apre con un
   binario o un'app della versione precedente (l'apertura fallisce, §6 punto
   2): tornare indietro vuol dire rimettere il binario vecchio **e**
   ripristinare (§5) il backup del punto 1, perdendo quel che è arrivato al
   server nel frattempo. Lo stesso vale per il database locale di un'app già
   aggiornata.

### 6.2 Passaggio allo schema v4 (persona e titolare)

La versione del 2026-10-08 porta lo schema del core alla versione 4: una
transazione ha una **persona** distinta dal suo autore (`transactions.person`)
e un modello di ricorrenza ha un **titolare** (`recurring_templates.owner`).
Le righe esistenti si riempiono da sole con il loro autore. Il server inoltre
rifiuta, comando per comando, un comando che nomina come persona o titolare
qualcuno che non è membro del vault (`not_a_member`). L'ordine
conta, ed è **l'inverso di §6.1**:

1. **Backup del server** (§4): `docker compose exec sparagne backup.sh`, o
   `backup.sh` su bare metal.
2. **Prima il server, poi le app.** Un server vecchio, quando risponde a un
   pull, riserializza i comandi e **scarta in silenzio** i campi nuovi: la
   persona e il titolare spariscono dal log che gli altri Mac scaricano. Rifiuta
   inoltre una modifica che cambia solo la persona (per lui è una patch vuota).
3. **Poi ogni app, subito.** Un'app vecchia non conosce i campi nuovi: non
   li mostra e, scaricandoli, li perde. Aggiornare tutti i Mac che
   sincronizzano appena il server è su.
4. **Nessuno registra "per conto di" finché ogni Mac non è aggiornato.**
   Finché ne resta uno vecchio, una persona diversa dall'autore o un titolare
   scelto lì non arriva a quel Mac, e lì la riga risulta dell'autore.
5. **Il ritorno indietro è un ripristino.** Un database v4 non si apre con un
   binario o un'app della versione precedente (§6 punto 2): tornare indietro
   vuol dire rimettere il binario vecchio **e** ripristinare (§5) il backup
   del punto 1, perdendo quel che è arrivato al server nel frattempo. Lo
   stesso vale per il database locale di un'app già aggiornata.

## 7. Log e healthcheck

- Log strutturati su stdout via `tracing`, controllati da `RUST_LOG`
  (`docker compose logs -f sparagne`, oppure `journalctl -u
  sparagne-server -f` su bare metal).
- `GET /health` risponde `{"status":"ok"}` senza autenticazione: è anche
  l'`HEALTHCHECK` dell'immagine Docker (`docker ps` mostra `healthy`).
- All'avvio la riga `listening` riporta la configurazione effettiva, limiti
  e `trust_proxy` compresi: è il posto dove controllare che il proxy sia
  considerato come ci si aspetta.
