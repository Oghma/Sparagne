# Development

## Project structure

Workspace root (`Cargo.toml`) defines a multi-crate Rust project:
- `crates/engine/`: core budgeting domain + persistence layer
- `crates/server/`: Axum HTTP API that wraps the engine
- `crates/telegram_bot/`: Teloxide bot that uses the engine
- `crates/migration/`: SeaORM migrations and CLI runner
- `crates/api_types/`: shared HTTP DTOs used by server and bot
- `crates/app/`: launcher binary for running server/bot together
- `crates/tui/`: terminal UI client

## Build and run

```sh
cargo build --workspace
cargo run -p sparagne
cargo run -p server
cargo run -p telegram_bot
cargo run -p sparagne_tui
```

## Tests and linting

```sh
cargo test --workspace
cargo +nightly fmt --all
cargo +nightly fmt --all --check
cargo clippy --workspace --all-targets
```

## Configuration

Config lookup order (server/app):
1) `SPARAGNE_CONFIG`
2) `$XDG_CONFIG_HOME/sparagne/config.toml` (or `~/.config/sparagne/config.toml`)
3) `config/config.toml`

Server env override:
- `SPARAGNE_SERVER=host:port` (overrides bind and port)

TUI config resolution:
1) `--config <path>` or `SPARAGNE_CONFIG`
2) `$XDG_CONFIG_HOME/sparagne/config.toml` (or `~/.config/sparagne/config.toml`)
3) `config/config.toml`

TUI env overrides:
- `SPARAGNE_URL_SERVER`
- `SPARAGNE_USERNAME`
- `SPARAGNE_VAULT`
- `SPARAGNE_TIMEZONE`

## Migrations

Uses `DATABASE_URL` (defaults to `sqlite:./sparagne.db?mode=rwc`).

```sh
cargo run -p migration -- up
cargo run -p migration -- down
```
