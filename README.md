# Sparagne

Sparagne (in Italian "risparmiare") is a Furlan word that means "savings".

Version 2 is a local-first personal finance app for macOS. A Rust core keeps
the domain rules and a SQLite file where every change is a command appended
to a per-vault log; the state tables are a projection of that log. The macOS
app (SwiftUI) talks to the core in-process through UniFFI and never touches
SQL. A small sync server, built on the same core, shares vaults between
accounts and Macs.

Version 1 (Rust engine, HTTP server, Telegram bot, terminal UI) lives at tag
`v0.93.0`.

## Install

Server and app share one version: a release tag `vX.Y.Z` names both. Upgrade
the server before the apps (`docs/v2/DEPLOY.md` §6).

### Server

The release image, for linux/amd64 and linux/arm64, is
`ghcr.io/oghma/sparagne-server:<version>`. On the host:

```sh
cd server/deploy && cp .env.example .env   # set DOMAIN and SPARAGNE_VERSION
docker compose up -d
```

`gh attestation verify oci://ghcr.io/oghma/sparagne-server:<version> --repo
Oghma/Sparagne` checks that an image was built by this repository's release
workflow. `docs/v2/DEPLOY.md` has the full setup (TLS via Caddy, login
limits, backups, upgrades, building the image from the sources).

Accounts can also be managed from the command line, which is the way to add
them once registration is closed: `sparagne-server user add <name>` and
`user passwd <name>` read the password from the first line of stdin
(`passwd` also logs the account out everywhere), `user list` prints every
account, `user revoke <name>` revokes its tokens. They work on the data
directory of a running server, e.g.
`printf '%s\n' "$PW" | docker compose exec -T sparagne sparagne-server user add alice`.

### App

The app is built from the sources on the Mac that runs it (Apple silicon,
macOS 27). There is no download: without an Apple Developer ID, macOS would
stop a downloaded copy, while one built here needs no exception. It takes
Xcode 27, Rust through rustup (the version comes from `rust-toolchain.toml`)
and xcodegen (`brew install xcodegen`). From a release tag:

```sh
bash scripts/build-app.sh --install   # or without --install: dist/Sparagne.app
```

The build is signed ad hoc with the app sandbox, like a run from Xcode, so
both use the same database (below).

## Layout

| Path | What |
|---|---|
| `core/` | Rust crate: domain, commands, log, projection, queries, quick-add |
| `apple/` | Swift package generated from the core and the macOS app |
| `server/` | sync server |
| `docs/v2/` | architecture, v1 distillation, inventories |

## Build

```sh
cargo test
cargo clippy --workspace --all-targets
```

### Statements, full export and backup

File › Import Statement… (⇧⌘I) reads a bank or card CSV (UTF-8, falling back
to Windows-1252/Latin-1). It recognises the built-in `card-transactions`
preset from the header, or reopens the mapping last used for the same vault
and header. Pick the columns, the date and amount format, what each row type
becomes (expense, income, refund, transfer from or to another wallet, skip, by
sign), the statuses to skip, and the target wallet and envelope. The preview
shows what happens to every row; new rows can be left out or given a category,
prefilled from the vault's history. Importing the same file again adds
nothing.

File › Export All Transactions… writes every transaction of the vault as CSV,
voided rows and transfers included. File › Back Up Database… saves a
consistent copy, `Sparagne <date>.sqlite`. To restore it, quit Sparagne,
delete `sparagne.sqlite-wal` and `sparagne.sqlite-shm` if present, and replace
`sparagne.sqlite` with the backup renamed. The database lives in
`~/Library/Containers/it.oghma.sparagne/Data/Library/Application Support/Sparagne/`
for a signed (sandboxed) build and in `~/Library/Application Support/Sparagne/`
for an unsigned one; `lsof -p $(pgrep -x Sparagne) | grep sqlite` tells which.

### Import from v1

A Sparagne v1 database is replayed into a v2 one as commands, so the history
arrives with the rules of v2 applied to it:

```sh
cargo run -p sparagne_core --example import_v1 -- \
    <v1.sqlite> \
    ~/Library/Containers/it.oghma.sparagne/Data/Library/Application\ Support/Sparagne/sparagne.sqlite \
    --author <username> [--vault <name>] [--tz Europe/Rome]
```

The v1 file is opened read-only; close the app before writing to its database.
The import is idempotent, so running it again changes nothing. It prints a
report with the counts per entity, the v1 fields that have no v2 counterpart
and the commands the core refused. The imported commands sit in the outbox and
reach the server at the first sync.

The design is in `docs/v2/ARCH.md`.
