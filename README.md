# Sparagne

A budget tracker for personal finance. Feature-freeze in progress for **v1.0**.

Sparagne (in italian "risparmiare") is a furlan word that means "savings". Think of
it as your tiny, stubborn accountant who lives in the terminal and judges your bar
coffee at 7:12 AM.

The app consists of:
- an engine that manages expenses, cash flows, wallets, etc
- a server that exposes the API
- a Telegram bot for quick entries
- a TUI as the main “control room”

## HTTP API (Server)

- Base URL: `http://127.0.0.1:3000`
- Auth: Basic auth (`Authorization: Basic base64(username:password)`).
- Telegram bot requests may also include `telegram-user-id` header.
- JSON request bodies are used for read and write endpoints (POST everywhere for bodies).

Core endpoints:
- `POST /vault/new` (`api_types::vault::VaultNew`) → `api_types::vault::Vault`
- `POST /vault/get` (`api_types::vault::Vault`) → `api_types::vault::Vault`
- `POST /vault/snapshot` (`api_types::vault::Vault`) → `api_types::vault::VaultSnapshot`
- `DELETE /vault/{id}` → `204 No Content`
- `POST /cashFlow/get` (`api_types::cash_flow::CashFlowGet`) → `engine::CashFlow`
- `POST /stats/get` (`api_types::vault::Vault`) → `api_types::stats::Statistic`

Transactions:
- `POST /transactions` (`api_types::transaction::TransactionList`) → `TransactionListResponse`
- `POST /transactions/get` (`TransactionGet`) → `TransactionDetailResponse`
- `POST /income` (`IncomeNew`) → `TransactionCreated`
- `POST /expense` (`ExpenseNew`) → `TransactionCreated`
- `POST /refund` (`Refund`) → `TransactionCreated`
- `POST /transferWallet` (`TransferWalletNew`) → `TransactionCreated`
- `POST /transferFlow` (`TransferFlowNew`) → `TransactionCreated`
- `PATCH /transactions/{id}` (`TransactionUpdate`) → `200 OK`
- `POST /transactions/{id}/void` (`TransactionVoid`) → `200 OK`

Sharing/memberships:
- `GET /vault/{vault_id}/members` / `POST /vault/{vault_id}/members` / `DELETE /vault/{vault_id}/members/{username}`
- `GET /vault/{vault_id}/flows/{flow_id}/members` / `POST /vault/{vault_id}/flows/{flow_id}/members` / `DELETE /vault/{vault_id}/flows/{flow_id}/members/{username}`
- `POST /flows/shared` (`api_types::flow::FlowSharedList`) → `FlowSharedListResponse`

Categories:
- `POST /categories/list` (`CategoryList`) → `CategoryListResponse`
- `POST /categories` (`CategoryCreate`) → `CategoryCreated`
- `PATCH /categories/{category_id}` (`CategoryUpdate`) → `CategoryView`
- `POST /categories/{category_id}/aliases/list` (`CategoryAliasList`) → `CategoryAliasListResponse`
- `POST /categories/{category_id}/aliases` (`CategoryAliasCreate`) → `CategoryAliasCreated`
- `DELETE /categories/{category_id}/aliases/{alias_id}` (`CategoryAliasDelete`) → `204 No Content`
- `POST /categories/{category_id}/merge/preview` (`CategoryMergePreview`) → `CategoryMergePreviewResponse`
- `POST /categories/{category_id}/merge` (`CategoryMerge`) → `CategoryCreated`

## Installation

### Option 1: From Docker

Pull the image from Docker Hub:

```sh
docker pull oghma/sparagne
```

Prepare `config/config.toml` (see [Settings](#Settings)), then run:

```sh
docker run -it --rm \
  -p 3000:3000 \
  -v "$(pwd)/config:/sparagne/config" \
  oghma/sparagne
```


### Option 2: From Source

Clone the `sparagne` repository and navigate to the root directory

``` sh
git clone git@github.com:Oghma/Sparagne.git
cd sparagne
```

Open `config/config.toml` and change the settings. See [Settings](#Settings).
Save the settings and run `Sparagne`:

``` sh
cargo run -p sparagne --release
```

### TUI

Run the TUI in a separate terminal:

```sh
cargo run -p sparagne_tui --release
```

Config resolution (in order):
1) `--config <path>` or `SPARAGNE_CONFIG`
2) `$XDG_CONFIG_HOME/sparagne/config.toml` (or `~/.config/sparagne/config.toml`)
3) `config/config.toml`

CLI flags override config values. Env overrides:
- `SPARAGNE_URL_SERVER`
- `SPARAGNE_USERNAME`
- `SPARAGNE_VAULT`
- `SPARAGNE_TIMEZONE`

### Telegram bot

The bot is started by `sparagne` if `[telegram]` is present in
`config/config.toml`.

### Database

Sparagne requires a database to store users and their entries. At the moment
only `Sqlite3` is supported.

NOTE: Telegram bot requires its account for the authentication.

To bootstrap users and vaults, use the admin CLI (it runs migrations on startup
and uses `DATABASE_URL`, defaulting to `sqlite:./sparagne.db?mode=rwc`):

```sh
# Create a user
cargo run -p sparagne_admin -- user create --username alice

# Create a vault (also creates Unallocated + default wallet)
cargo run -p sparagne_admin -- vault create --owner alice --name Main --currency EUR
```

## Settings

`config/config.toml` is loaded by `crates/app`. Config lookup order:
1) `SPARAGNE_CONFIG`
2) `$XDG_CONFIG_HOME/sparagne/config.toml` (or `~/.config/sparagne/config.toml`)
3) `config/config.toml`

Minimal structure:

```toml
[app]
level = "info"

[server]
bind = "127.0.0.1"
port = 3000
database = { Sqlite = "./sparagne.db" }

[tui]
base_url = "http://127.0.0.1:3000"
username = "matteo"
vault = "Main"
timezone = "Europe/Rome"

[telegram]
token = "..."
server = "http://127.0.0.1:3000"
username = "service_bot"
password = "secret"
```

Notes:
- `server.database` is a `Database` enum (`Memory` or `{ Sqlite = "path" }`).
- Telegram bot requires a dedicated service user.
- `SPARAGNE_SERVER=host:port` overrides `server.bind`/`server.port`.
