# Sparagne

Sparagne (in Italian "risparmiare") is a Furlan word that means "savings".

Version 2 is a rewrite in progress on branch `v2`: a local-first personal
finance app for macOS. A Rust core keeps the domain rules and a SQLite file
where every change is a command appended to a per-vault log; the state tables
are a projection of that log. The macOS app (SwiftUI) talks to the core
in-process through UniFFI and never touches SQL. A small sync server, built on
the same core, comes later.

Version 1 (Rust engine, HTTP server, Telegram bot, terminal UI) lives at tag
`v0.93.0`.

## Layout

| Path | What |
|---|---|
| `core/` | Rust crate: domain, commands, log, projection, queries, quick-add |
| `apple/` | Swift package generated from the core and the macOS app (phase 2) |
| `server/` | sync server (phase 3) |
| `docs/v2/` | architecture, v1 distillation, inventories |

## Build

```sh
cargo test
cargo clippy --workspace --all-targets
```

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

### Server

```sh
cd server/deploy && cp .env.example .env && docker compose up -d
```

See `docs/v2/DEPLOY.md` for the full setup (TLS via Caddy, backups,
upgrades).

The design is in `docs/v2/ARCH.md`.
