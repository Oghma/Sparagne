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

The design is in `docs/v2/ARCH.md`.
