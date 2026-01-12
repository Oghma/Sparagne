# Running Migrator CLI

This crate exposes a small custom migrator CLI (not the upstream
`sea-orm-migration` CLI). It keeps the workflow simple and stable under
Rust 1.92; any future switch back to the official CLI is a post‑1.0 decision.

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
