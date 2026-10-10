# Sparagne v2 — Running the server

> 2026-09-12, updated 2026-09-23 (login limits, accounts from the command
> line) and 2026-10-08 (image on GHCR, §3.2; release binary, §3.3).
> See also: `server/Dockerfile`, `server/deploy/`,
> `.github/workflows/release.yml`.

## 1. TLS is required

The macOS app uses App Transport Security, which refuses `http://` to any
host other than `localhost`. The server therefore **always sits behind a
reverse proxy with a certificate**: `server/deploy/compose.yml` includes
Caddy, which obtains and renews a Let's Encrypt certificate on its own (all
it needs is a reachable port 80/443 and a DNS record pointing at the
domain). The URL typed in the app's settings is `https://your-domain`, never
`http://`; on a local network without a domain, `https://<address>` with a
certificate from Caddy's internal CA (§3.3).

## 2. Environment variables

| Variable | Default (in the container) | Meaning |
|---|---|---|
| `SPARAGNE_BIND` | `0.0.0.0:3000` | address:port to listen on. The Docker image already sets `0.0.0.0:3000` (Caddy is in front); without Docker, prefer `127.0.0.1:3000` with the proxy on the same machine. |
| `SPARAGNE_DATA_DIR` | `/data` | directory holding `vaults.sqlite` and `server.sqlite`. |
| `SPARAGNE_ALLOW_REGISTRATION` | `true` | when `false`, `POST /auth/register` answers `403 registration_disabled`. |
| `SPARAGNE_TOKEN_TTL_DAYS` | `30` | how long a login token stays valid. |
| `SPARAGNE_TRUST_PROXY` | `false` (`true` in `compose.yml`) | when `true`, the client's address is the last entry of `X-Forwarded-For` instead of the TCP peer (§2.1). |
| `SPARAGNE_LOGIN_MAX_FAILURES` | `5` | failed logins for one username within the window, after which the username is locked for another window; `0` removes the limit. |
| `SPARAGNE_LOGIN_WINDOW_SECS` | `900` | the window of the two login limits, in seconds. |
| `SPARAGNE_IP_MAX_FAILURES` | `30` | failed logins from one address within the window; `0` removes the limit. |
| `RUST_LOG` | (empty, no explicit filter) | `tracing-subscriber` env-filter syntax, e.g. `info` or `sparagne_server=debug,info`. |

TLS is the reverse proxy's job; the login limits are the server's own
(§2.1), the proxy has none.

### 2.1 Login limits

The server slows down anyone guessing a password, in memory (a restart
resets the counts) and before computing any hash:

- **Per username**: 5 failed logins in 15 minutes lock the username for 15
  minutes. While it is locked, even the right password gets `429
  too_many_requests` with a `Retry-After` header (seconds). A successful
  login resets the count. A username that does not exist locks the same
  way, so the lock does not reveal which accounts exist; a wrong current
  password in `POST /auth/password` also counts as a failed login.
- **Per address**: 30 failed logins in 15 minutes from the same address
  (for IPv6, the same /64) → `429`, whatever the username.
- **Registration**: at most 10 attempts an hour per address, valid or not.

**Which address.** Without a proxy it is the TCP peer. Behind a proxy the
peer is always the proxy, and every client would share one count: with
`SPARAGNE_TRUST_PROXY=true` the server uses the **last** entry of
`X-Forwarded-For` instead, the one the proxy appends itself and the client
cannot forge (anyone can write the entries further left). `compose.yml`
turns it on because port 3000 is not published and only Caddy reaches the
container; Caddy sets `X-Forwarded-For` by itself. Without Docker, turn it
on only when the server listens on `127.0.0.1` (or on a port the firewall
opens to the proxy alone) and the proxy appends the client's address: Caddy
does by default, nginx with `proxy_set_header X-Forwarded-For
$proxy_add_x_forwarded_for;`. **Never** with the server's port reachable
directly: anyone could pick their own address and the per-address limit
would be worthless (the per-username one stays).

## 3. First start

1. `cd server/deploy && cp .env.example .env`, then set `DOMAIN`,
   `SPARAGNE_VERSION` (the release to run, §3.2) and the other variables.
2. In `Caddyfile`, replace `sparagne.example.com` with the real domain.
3. `docker compose up -d` (pulls `ghcr.io/oghma/sparagne-server` at the
   version in `.env` and starts `sparagne` + `caddy`). The host only needs
   `server/deploy/` and Docker: no sources, no Rust.
4. Check `curl https://your-domain/health` → `{"status":"ok"}`.
5. Create the accounts you need: with `SPARAGNE_ALLOW_REGISTRATION=true`
   (the default) from the app's sign-up screen or `POST /auth/register`; in
   any case with the CLI (§3.1).
6. Close registration: set `SPARAGNE_ALLOW_REGISTRATION=false` in `.env`,
   then `docker compose up -d` again (it recreates only the `sparagne`
   container with the new variable). From then on, new accounts are created
   with the CLI only (§3.1). **Sharing a vault does not create accounts**:
   `PUT /vaults/{id}/members` with a username that does not exist answers
   `404 not_found`, so the account has to exist first.

### 3.1 Accounts from the command line

The same binary manages the accounts: `sparagne-server` (or
`sparagne-server serve`) runs the server, `sparagne-server user …` works on
the accounts and exits. It opens only `server.sqlite` in the data directory
(`--data-dir`, else `SPARAGNE_DATA_DIR`, else `./data`) and refuses a
directory that does not hold one instead of creating an empty one. It is
meant for a running server: SQLite in WAL mode with a busy timeout copes
with the two processes, and a revoked token stops working on its next
request. It ignores `SPARAGNE_ALLOW_REGISTRATION`: it is precisely the way
to create accounts with registration closed.

| Command | Effect |
|---|---|
| `user add <name>` | creates the account; password from the first line of stdin |
| `user passwd <name>` | new password from the first line of stdin, and revokes every token of the account (it has to log in again everywhere) |
| `user list` | one account per line: username, tab, creation date (UTC) |
| `user revoke <name>` | revokes every token of the account, the password stays |

Usernames and passwords follow the registration rules (username 3-32
characters of `[a-z0-9_.-]`, lowercased; password at least 8 characters).
The password is never an argument, so it ends up neither in the shell's
history nor in `ps`. With Docker Compose, from `server/deploy/` (`-T` lets
`exec` pass stdin through):

```sh
read -rs PW    # typed without echo
printf '%s\n' "$PW" | docker compose exec -T sparagne sparagne-server user add alice
printf '%s\n' "$PW" | docker compose exec -T sparagne sparagne-server user passwd alice
docker compose exec sparagne sparagne-server user list
docker compose exec sparagne sparagne-server user revoke alice
```

Without Docker, as the service's user (so SQLite's `-wal`/`-shm` files stay
its own); `runuser` needs root and is there even where `sudo` is not:

```sh
printf '%s\n' "$PW" | runuser -u sparagne -- env SPARAGNE_DATA_DIR=/var/lib/sparagne \
    /usr/local/bin/sparagne-server user add alice
```

On an error the command prints the reason on stderr and exits with 1 (2
for a malformed command). From the app, a user changes their own password
with `POST /auth/password`, which asks for the current one.

### 3.2 The image and where it comes from

Every version tag (`v2.0.0`) publishes the image for linux/amd64 and
linux/arm64 (`.github/workflows/release.yml`), tagged `2.0.0`, `2.0` and
`latest`; a pre-release (`v2.1.0-beta.1`) only under its own tag.
`compose.yml` takes it by version number, never `latest`, so an upgrade is
a choice made after a backup (§6).

The workflow attaches to the image a provenance attestation signed by
GitHub: it says the image was built by that workflow, from that commit of
the repository. Before using it (or upgrading):

```sh
gh attestation verify oci://ghcr.io/oghma/sparagne-server:2.0.0 --repo Oghma/Sparagne
```

To build it from the sources instead (a change not released yet, a host
that must not download anything): from a checkout of the repository,
`docker compose -f compose.yml -f compose.build.yml up -d --build`
(`server/deploy/compose.build.yml`).

### 3.3 Without Docker: the release binary (LXC, VM, bare metal)

Every release also carries the server as a static Linux binary (musl, with
SQLite built in), for amd64 and arm64, so it runs on any distribution:
`sparagne-server-<version>-x86_64-unknown-linux-musl.tar.gz` (or
`aarch64-…`), holding the binary, the systemd unit, `.env.example`,
`backup.sh` (§4) and the two scripts below, plus its `.sha256`.

**With the scripts.** On a fresh Debian host (a Proxmox LXC with Debian 13,
a VM), as root:

```sh
curl -fLO https://github.com/Oghma/Sparagne/releases/latest/download/install.sh
sh install.sh                 # --pre for pre-releases, or a version: sh install.sh 2.1.0
```

`install.sh` installs the packages it needs, fetches the archive for the
host's architecture and checks it (its checksum, and its provenance too
when `gh` is logged in), then creates the `sparagne` user, the data and
backup directories and `/etc/sparagne.env` (registration closed unless
`--allow-registration`), and installs the unit, a daily backup timer (03:30,
kept 30 days) and the update script. By default it puts Caddy in front with
HTTPS from Caddy's internal CA on the host's address (`--address` picks
another); `--http` leaves Caddy out and serves plain HTTP on port 3000. It
ends with the address to type in the app and what is left to do: trusting
the CA's root on the Macs, the accounts. It refuses a host where Sparagne
is already installed.

`sh /root/update.sh` updates it: the latest release (`--pre` includes
pre-releases, or name a version). It backs up both databases, installs the
new binary, unit and backup script, starts the server and checks
`/health`; a release that does not come up is rolled back, the databases
included, since the new binary may already have migrated them.
`/etc/sparagne.env` is never touched, and settings a new release adds are
listed. A host installed by hand before the scripts existed adopts them
with one run of `update.sh`, downloaded from a release.

**By hand**, the same steps:

```sh
V=2.0.0-beta.1; T=x86_64-unknown-linux-musl
base=https://github.com/Oghma/Sparagne/releases/download/v$V
curl -fLO "$base/sparagne-server-$V-$T.tar.gz"
curl -fLO "$base/sparagne-server-$V-$T.tar.gz.sha256"
sha256sum -c "sparagne-server-$V-$T.tar.gz.sha256"
tar xzf "sparagne-server-$V-$T.tar.gz"
```

The archive has its own provenance attestation too: on a machine with
`gh`, `gh attestation verify sparagne-server-$V-$T.tar.gz --repo
Oghma/Sparagne`.

Then `sparagne-server.service`, in the archive: the comment at its top
lists the commands (dedicated user, data directory, `/etc/sparagne.env`,
start), to run as root (without `sudo` where you already are root, as in a
container). The server listens on `127.0.0.1:3000`; the reverse proxy with
TLS (§1) is a separate service, on the same machine or another one, and
§2.1 applies to `SPARAGNE_TRUST_PROXY`.

**Local network only, no domain.** Caddy on the same machine also issues a
certificate for an IP address, signed by its own internal CA:

```
https://192.168.178.81 {
	reverse_proxy 127.0.0.1:3000
}
```

The root of that CA (with the Debian package:
`/var/lib/caddy/.local/share/caddy/pki/authorities/local/root.crt`) has to
be trusted on every Mac that syncs: `sudo security add-trusted-cert -d -r
trustRoot -k /Library/Keychains/System.keychain root.crt`. The app's URL is
then `https://192.168.178.81`. The rest of this page applies unchanged;
once there is a domain, only the Caddyfile's first line changes.

## 4. Backup

`backup.sh` (`server/deploy/`, and in the release archive) takes an online
backup (no downtime) of `vaults.sqlite` and `server.sqlite` with `sqlite3
<db> ".backup '<dest>'"`, checks the dump's integrity with `PRAGMA
integrity_check`, and applies a retention in days.

- **Docker Compose**: the script and `sqlite3` are already in the image
  (`server/Dockerfile`); `compose.yml` mounts `./backups` on the host at
  `/backups` in the container. From `server/deploy/`:

  ```sh
  docker compose exec sparagne backup.sh
  ```

  The dump appears directly in `server/deploy/backups/<timestamp>/` on the
  host. For the retention: `docker compose exec sparagne env
  RETENTION_DAYS=30 backup.sh`.

- **Without Docker** (it needs the `sqlite3` command):

  ```sh
  DATA_DIR=/var/lib/sparagne BACKUP_DIR=/var/backups/sparagne \
      backup.sh
  ```

Schedule it with cron or a systemd timer, whichever the platform has.

## 5. Restore

1. Stop the server (`docker compose stop sparagne`, or `systemctl stop
   sparagne-server` without Docker): the two SQLite files must not be
   touched while the process writes.
2. Copy `vaults.sqlite` and `server.sqlite` from the chosen backup over the
   files in `SPARAGNE_DATA_DIR` (also remove any `-wal`/`-shm` left over
   from the old files, so SQLite starts from a clean state).
3. Start the server again.

A `.backup` dump is a complete, consistent SQLite file: it needs no replay
or manual migration, only the normal start (§6).

## 6. Upgrading

1. Back up (§4), then:
   - Compose: in `.env` set `SPARAGNE_VERSION` to the new release (after
     verifying it, §3.2), then `docker compose pull sparagne && docker
     compose up -d sparagne` (Caddy stays as it is). From an install that
     built the image from the sources, before 2.0.0: also update
     `compose.yml` and add `SPARAGNE_VERSION` to `.env` (`.env.example`).
   - Compose from the sources: `git pull`, then `docker compose -f
     compose.yml -f compose.build.yml up -d --build sparagne`.
   - Without Docker: `sh /root/update.sh` (§3.3), which takes this backup
     itself. By hand: download and verify the new release's archive,
     `install -m 755 sparagne-server /usr/local/bin/sparagne-server`,
     `systemctl restart sparagne-server`.
2. The schema of `vaults.sqlite` **upgrades itself at start**: `Core::open`
   (`core/src/store.rs`) reads `PRAGMA user_version`, applies the missing
   migration when the version on disk is older than the code's, and updates
   `user_version` accordingly, in the same opening of the connection. There
   is no separate migration command to run: starting the new binary on the
   existing files is enough. If the `user_version` on disk were newer than
   the one the binary knows (an upgrade, then a downgrade), opening fails
   with an explicit error instead of corrupting the data: in that case put
   back an up-to-date binary or restore an earlier backup.
3. Back up (§4) before any major upgrade regardless.

### 6.1 Moving to schema v3

The 2026-09-23 version takes the core's schema to version 3 (vault names
become labels and may repeat) and adds password changes, leaving a vault
and the login limits' `429` to the API (§2.1). The order matters:

1. **Back up the server** (§4): `docker compose exec sparagne backup.sh`,
   or `backup.sh` without Docker.
2. **Every app first, then the server.** Upgrade the app on every Mac that
   syncs, and only then the server (§6). The other way round, an old app
   that pulls two vaults with the same name (the new server accepts them)
   diverges from the server.
3. **Going back is a restore.** A v3 database does not open with a binary
   or an app of the previous version (opening fails, §6 point 2): going
   back means putting the old binary back **and** restoring (§5) the backup
   of point 1, losing what reached the server in the meantime. The same
   holds for the local database of an app already upgraded.

### 6.2 Moving to schema v4 (person and owner)

The 2026-10-08 version takes the core's schema to version 4: a transaction
has a **person** distinct from its author (`transactions.person`) and a
recurring template has an **owner** (`recurring_templates.owner`). Existing
rows fill themselves in with their author. The server also refuses, command
by command, a command naming as person or owner someone who is not a member
of the vault (`not_a_member`). The order matters, and it is **the reverse
of §6.1**:

1. **Back up the server** (§4): `docker compose exec sparagne backup.sh`,
   or `backup.sh` without Docker.
2. **The server first, then the apps.** An old server, answering a pull,
   re-serializes the commands and **silently drops** the new fields: the
   person and the owner vanish from the log the other Macs download. It also
   refuses an edit that changes only the person (to it, an empty patch).
3. **Then every app, right away.** An old app does not know the new fields:
   it does not show them and, downloading them, loses them. Upgrade every
   Mac that syncs as soon as the server is up.
4. **Nobody records "on behalf of" until every Mac is upgraded.** While an
   old one remains, a person other than the author, or an owner chosen
   there, does not reach that Mac, and there the row shows as the author's.
5. **Going back is a restore.** A v4 database does not open with a binary
   or an app of the previous version (§6 point 2): going back means putting
   the old binary back **and** restoring (§5) the backup of point 1, losing
   what reached the server in the meantime. The same holds for the local
   database of an app already upgraded.

### 6.3 Moving to schema v5 (allocation plan)

This version takes the core's schema to version 5: two new tables,
`allocation_plans` (a vault's plan) and `allocation_runs` (the periods
decided, one row per plan and period), and five new command kinds in the
log: `create_allocation_plan`, `update_allocation_plan`,
`execute_allocation`, `skip_allocation` and `reopen_allocation`. The order
matters, and it is **the same as §6.2**:

1. **Back up the server** (§4): `docker compose exec sparagne backup.sh`,
   or `backup.sh` without Docker.
2. **The server first, then the apps.** An old server refuses with `400`
   the whole push of an app that carries a command kind it does not know,
   so that Mac stops syncing altogether until the server is upgraded. The
   other way round is worse: an old app cannot pull a vault whose log holds
   one of these commands.
3. **Then every app, right away.** Upgrade every Mac that syncs as soon as
   the server is up.
4. **Nobody creates a plan until every Mac of the vault runs the new app.**
   Once the plan is in the log, an app of the previous version stops pulling
   that vault.
5. **Going back is a restore.** A v5 database does not open with a binary
   or an app of the previous version (§6 point 2): going back means putting
   the old binary back **and** restoring (§5) the backup of point 1, losing
   what reached the server in the meantime. The same holds for the local
   database of an app already upgraded.

## 7. Logs and health check

- Structured logs on stdout through `tracing`, controlled by `RUST_LOG`
  (`docker compose logs -f sparagne`, or `journalctl -u sparagne-server -f`
  without Docker).
- `GET /health` answers `{"status":"ok"}` without authentication: it is
  also the Docker image's `HEALTHCHECK` (`docker ps` shows `healthy`).
- At start, the `listening` line reports the effective configuration,
  limits and `trust_proxy` included: that is where to check that the proxy
  is seen the way it should be.
