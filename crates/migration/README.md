# Running Migrator CLI

This crate currently exposes a small custom migrator CLI (not the upstream `sea-orm-migration` CLI).
We introduced it to keep migrations stable under Rust 1.92 and avoid pulling the `sea-orm-cli`
dependency during the pre‑refactor phase. During the engine refactor we will re‑evaluate whether to
keep this mini‑CLI or switch back to the official one.

## Usage

The CLI reads `DATABASE_URL`. If unset, it defaults to a local SQLite DB:

```sh
export DATABASE_URL="sqlite:./sparagne.db?mode=rwc"
```

Commands:

- Apply all pending migrations (default)
  ```sh
  cargo run -p migration
  # or
  cargo run -p migration -- up
  ```
- Rollback last applied migration batch
  ```sh
  cargo run -p migration -- down
  ```
- Drop all tables, then reapply all migrations
  ```sh
  cargo run -p migration -- fresh
  ```
- Print migration status
  ```sh
  cargo run -p migration -- status
  ```
