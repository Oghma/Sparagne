# Sparagne v2 — Messa in produzione del server

> 2026-09-12. Riferimenti: `SYNC.md` §2-3 (storage e API), `server/Dockerfile`,
> `server/deploy/`.

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
| `RUST_LOG` | (vuoto, nessun filtro esplicito) | sintassi `tracing-subscriber` env-filter, es. `info` o `sparagne_server=debug,info`. |

Fuori dal server: TLS e rate limiting li fa il reverse proxy, non
l'applicazione.

## 3. Primo avvio

1. `cd server/deploy && cp .env.example .env`, impostare `DOMAIN` e le altre
   variabili.
2. In `Caddyfile`, sostituire `sparagne.example.com` col dominio vero.
3. `docker compose up -d` (builda l'immagine dal `Dockerfile` alla radice del
   repo e avvia `sparagne` + `caddy`).
4. Verificare `curl https://il-tuo-dominio/health` → `{"status":"ok"}`.
5. Con `SPARAGNE_ALLOW_REGISTRATION=true` (default), creare gli account che
   servono da `POST /auth/register` (o dalla schermata di registrazione
   dell'app).
6. Chiudere la registrazione: in `.env` impostare
   `SPARAGNE_ALLOW_REGISTRATION=false`, poi `docker compose up -d` di nuovo
   (ricrea solo il container `sparagne` con la nuova variabile). Da qui in
   poi nuovi account si aggiungono solo condividendo un vault (`PUT
   /vaults/{id}/members`) — la registrazione resta chiusa.

### Bare metal (senza Docker)

`server/deploy/sparagne-server.service` è un'unit systemd pronta all'uso: il
commento in testa al file elenca i comandi (utente dedicato, percorso del
binario, `EnvironmentFile`). In questo caso il reverse proxy (Caddy, nginx,
…) è un servizio a parte sulla stessa macchina o su un'altra.

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

1. `git pull` (o scaricare la nuova immagine se pubblicata), poi:
   - Compose: `docker compose build sparagne && docker compose up -d
     sparagne` (Caddy non serve ricostruirlo).
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

## 7. Log e healthcheck

- Log strutturati su stdout via `tracing`, controllati da `RUST_LOG`
  (`docker compose logs -f sparagne`, oppure `journalctl -u
  sparagne-server -f` su bare metal).
- `GET /health` risponde `{"status":"ok"}` senza autenticazione: è anche
  l'`HEALTHCHECK` dell'immagine Docker (`docker ps` mostra `healthy`).
