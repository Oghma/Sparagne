-- Sparagne v2 core schema, version 1.
-- UUIDs are 16-byte BLOBs (v7 for entities, v5-derived for entities created
-- inside a command). Timestamps are unix seconds (UTC); `occurred_offset` keeps
-- the user's UTC offset in seconds.

CREATE TABLE vaults (
    id            BLOB PRIMARY KEY,
    name          TEXT NOT NULL,
    currency      TEXT NOT NULL,
    owner_user_id TEXT NOT NULL,
    created_at    INTEGER NOT NULL
);
CREATE UNIQUE INDEX ux_vaults_owner_name ON vaults(owner_user_id, lower(name));

CREATE TABLE wallets (
    id         BLOB PRIMARY KEY,
    vault_id   BLOB NOT NULL REFERENCES vaults(id) ON DELETE CASCADE,
    name       TEXT NOT NULL,
    balance    INTEGER NOT NULL DEFAULT 0,
    archived   INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL
);
CREATE UNIQUE INDEX ux_wallets_vault_name ON wallets(vault_id, lower(name));

CREATE TABLE flows (
    id             BLOB PRIMARY KEY,
    vault_id       BLOB NOT NULL REFERENCES vaults(id) ON DELETE CASCADE,
    name           TEXT NOT NULL,
    system_kind    TEXT,                 -- 'unallocated' or NULL
    balance        INTEGER NOT NULL DEFAULT 0,
    cap            INTEGER,              -- NULL = unlimited
    income_total   INTEGER,              -- non-NULL only for income-capped flows
    allow_negative INTEGER NOT NULL DEFAULT 0,
    archived       INTEGER NOT NULL DEFAULT 0,
    created_at     INTEGER NOT NULL
);
CREATE UNIQUE INDEX ux_flows_vault_name ON flows(vault_id, lower(name));

CREATE TABLE categories (
    id        BLOB PRIMARY KEY,
    vault_id  BLOB NOT NULL REFERENCES vaults(id) ON DELETE CASCADE,
    name      TEXT NOT NULL,
    name_norm TEXT NOT NULL,
    is_system INTEGER NOT NULL DEFAULT 0,
    archived  INTEGER NOT NULL DEFAULT 0
);
CREATE UNIQUE INDEX ux_categories_vault_norm ON categories(vault_id, name_norm);

CREATE TABLE category_aliases (
    id          BLOB PRIMARY KEY,
    vault_id    BLOB NOT NULL REFERENCES vaults(id) ON DELETE CASCADE,
    category_id BLOB NOT NULL REFERENCES categories(id) ON DELETE CASCADE,
    alias       TEXT NOT NULL,
    alias_norm  TEXT NOT NULL
);
CREATE UNIQUE INDEX ux_aliases_vault_norm ON category_aliases(vault_id, alias_norm);

CREATE TABLE transactions (
    id              BLOB PRIMARY KEY,
    vault_id        BLOB NOT NULL REFERENCES vaults(id) ON DELETE CASCADE,
    kind            TEXT NOT NULL,       -- income|expense|refund|transfer_wallet|transfer_flow
    occurred_at     INTEGER NOT NULL,    -- unix seconds UTC
    occurred_offset INTEGER NOT NULL,    -- seconds east of UTC
    amount          INTEGER NOT NULL,    -- absolute value; sign lives in legs
    category_id     BLOB NOT NULL REFERENCES categories(id),
    note            TEXT,
    created_by      TEXT NOT NULL,
    voided_at       INTEGER,
    voided_by       TEXT,
    command_id      BLOB NOT NULL
);
CREATE INDEX ix_transactions_vault_time ON transactions(vault_id, occurred_at DESC, id DESC);

CREATE TABLE legs (
    transaction_id BLOB NOT NULL REFERENCES transactions(id) ON DELETE CASCADE,
    ordinal        INTEGER NOT NULL,
    target_kind    TEXT NOT NULL,        -- wallet|flow
    target_id      BLOB NOT NULL,
    amount         INTEGER NOT NULL,     -- signed
    PRIMARY KEY (transaction_id, ordinal)
);
CREATE INDEX ix_legs_target ON legs(target_kind, target_id);

-- The log. Truth for the vault; everything above is a projection of it.
CREATE TABLE commands (
    id          BLOB PRIMARY KEY,        -- client-generated, doubles as idempotency key
    vault_id    BLOB NOT NULL,
    seq         INTEGER NOT NULL,        -- total order inside the vault
    author      TEXT NOT NULL,
    kind        TEXT NOT NULL,
    payload     TEXT NOT NULL,           -- JSON of the command
    occurred_at INTEGER,                 -- for transaction commands
    created_at  INTEGER NOT NULL,
    status      TEXT NOT NULL,           -- applied|rejected
    rejection   TEXT,                    -- '<code>: <message>' for rejected rows
    result_id   BLOB,                    -- id of the entity the command created
    server_seq  INTEGER                  -- seq the server gave it; NULL = outbox
);
CREATE UNIQUE INDEX ux_commands_vault_seq ON commands(vault_id, seq);
-- NULLs are distinct in SQLite, so the outbox is free to hold many rows.
CREATE UNIQUE INDEX ux_commands_vault_server_seq ON commands(vault_id, server_seq);

-- Recurring templates. Never materialized automatically: the app lists the
-- due periods and the user executes or skips each one with a command.
CREATE TABLE recurring_templates (
    id          BLOB PRIMARY KEY,
    vault_id    BLOB NOT NULL REFERENCES vaults(id) ON DELETE CASCADE,
    kind        TEXT NOT NULL,       -- income|expense
    amount      INTEGER NOT NULL,    -- absolute, > 0
    wallet_id   BLOB,                -- NULL = the only active wallet at execution time
    flow_id     BLOB,                -- NULL = Unallocated
    category    TEXT,                -- free text, resolved at execution time
    note        TEXT,
    schedule    TEXT NOT NULL,       -- JSON of Schedule
    enabled     INTEGER NOT NULL DEFAULT 1,
    archived_at INTEGER,
    created_by  TEXT NOT NULL,
    created_at  INTEGER NOT NULL
);
CREATE INDEX ix_recurring_vault ON recurring_templates(vault_id);

-- One row per period the user has decided on; its absence is what makes a
-- period pending.
CREATE TABLE recurring_runs (
    recurring_id   BLOB NOT NULL REFERENCES recurring_templates(id) ON DELETE CASCADE,
    period_date    TEXT NOT NULL,    -- ISO yyyy-mm-dd
    outcome        TEXT NOT NULL,    -- executed|skipped
    transaction_id BLOB,             -- non-NULL only when executed
    command_id     BLOB NOT NULL,
    created_at     INTEGER NOT NULL,
    PRIMARY KEY (recurring_id, period_date)
);
