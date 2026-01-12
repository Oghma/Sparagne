# Sparagne

A budget tracker for personal finance. Feature-freeze in progress for v1.0.

Sparagne (in italian "risparmiare") is a furlan word that means "savings". Think of
it as your tiny, stubborn accountant who lives in the terminal and judges your bar
coffee at 7:12 AM.

Sparagne is made of:
- engine (budgeting domain + persistence)
- HTTP server (API)
- Telegram bot (quick entries)
- TUI (your control room)

## Quick start

### Docker

Build locally:

```sh
docker build -t sparagne .
```

Run with persistent data and config:

```sh
docker run -it --rm \
  -p 3000:3000 \
  -v "$(pwd)/config:/sparagne/config" \
  -v "$(pwd)/data:/sparagne/data" \
  sparagne
```

### From source

```sh
git clone git@github.com:Oghma/Sparagne.git
cd Sparagne
cargo run -p sparagne --release
```

Run the TUI in another terminal:

```sh
cargo run -p sparagne_tui --release
```

## Configuration

Main config file: `config/config.toml`.

Config lookup order:
1) `SPARAGNE_CONFIG`
2) `$XDG_CONFIG_HOME/sparagne/config.toml` (or `~/.config/sparagne/config.toml`)
3) `config/config.toml`

Minimal example:

```toml
[app]
level = "info"

[server]
bind = "0.0.0.0"
port = 3000
database = { Sqlite = "data/sparagne.sqlite3" }

[tui]
base_url = "http://127.0.0.1:3000"
username = ""
vault = "Main"
timezone = "Europe/Rome"

# [telegram]
# token = "..."
# server = "http://127.0.0.1:3000"
# username = "service_bot"
# password = "secret"
```

Notes:
- `server.database` is a `Database` enum (`Memory` or `{ Sqlite = "path" }`).
- `SPARAGNE_SERVER=host:port` overrides `server.bind` and `server.port`.
- Telegram bot requires a dedicated service user.
- If multiple vaults share the same name, your own vault is preferred. To access a shared one, use `tui.vault = "Main (owner)"` or `tui.vault = "id:<uuid>"`.

## Data and storage

- Default SQLite path for the server is `data/sparagne.sqlite3`.
- When running in Docker, mount `/sparagne/data` as a volume.

## Telegram bot

The bot starts automatically when `[telegram]` is present in `config/config.toml`.

## Developer guide

See `docs/DEVELOPMENT.md` for workspace structure, build/test/lint commands, and
notes about configuration and migrations.
