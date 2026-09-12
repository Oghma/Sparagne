//! Acceptance tests for the v1 import.
//!
//! The v1 fixtures are built here by running the v1 DDL (transcribed from
//! `crates/migration/src/*.rs` at tag `v0.93.0`, kept to the tables and
//! columns the importer reads) and inserting rows the way the v1 engine did:
//! balances materialized on wallets and flows, one leg per target, positive
//! amounts on transactions and the sign on the legs.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};

use rusqlite::{Connection, params};
use sparagne_core::{
    Core, DomainError, TransactionFilter, TransactionKind,
    import_v1::{ImportOptions, ImportReport, command_id, import_v1},
};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// The v1 schema
// ---------------------------------------------------------------------------

const V1_SCHEMA: &str = "
CREATE TABLE users (
    username TEXT NOT NULL PRIMARY KEY,
    password TEXT NOT NULL,
    telegram_id TEXT,
    pair_code TEXT
);
CREATE TABLE vaults (
    id BLOB NOT NULL PRIMARY KEY,
    name TEXT NOT NULL,
    user_id TEXT NOT NULL,
    currency TEXT NOT NULL DEFAULT 'EUR'
);
CREATE TABLE wallets (
    id BLOB NOT NULL PRIMARY KEY,
    name TEXT NOT NULL,
    balance BIGINT NOT NULL,
    currency TEXT NOT NULL DEFAULT 'EUR',
    archived BOOLEAN NOT NULL,
    vault_id BLOB NOT NULL
);
CREATE TABLE cash_flows (
    id BLOB NOT NULL PRIMARY KEY,
    name TEXT NOT NULL,
    system_kind TEXT,
    balance BIGINT NOT NULL,
    max_balance BIGINT,
    income_balance BIGINT,
    currency TEXT NOT NULL DEFAULT 'EUR',
    archived BOOLEAN NOT NULL,
    vault_id BLOB NOT NULL,
    allow_negative BOOLEAN NOT NULL DEFAULT 0
);
CREATE TABLE transactions (
    id BLOB NOT NULL PRIMARY KEY,
    vault_id BLOB NOT NULL,
    kind TEXT NOT NULL,
    occurred_at TIMESTAMP NOT NULL,
    amount_minor BIGINT NOT NULL,
    currency TEXT NOT NULL,
    category TEXT,
    note TEXT,
    created_by TEXT NOT NULL,
    voided_at TIMESTAMP,
    voided_by TEXT,
    refunded_transaction_id BLOB,
    idempotency_key TEXT,
    category_id BLOB
);
CREATE TABLE legs (
    id BLOB NOT NULL PRIMARY KEY,
    transaction_id BLOB NOT NULL,
    target_kind TEXT NOT NULL,
    target_id BLOB NOT NULL,
    amount_minor BIGINT NOT NULL,
    currency TEXT NOT NULL,
    attributed_user_id TEXT
);
CREATE TABLE categories (
    id BLOB NOT NULL PRIMARY KEY,
    vault_id BLOB NOT NULL,
    name TEXT NOT NULL,
    name_norm TEXT NOT NULL,
    archived BOOLEAN NOT NULL DEFAULT 0,
    is_system BOOLEAN NOT NULL DEFAULT 0
);
CREATE TABLE category_aliases (
    id BLOB NOT NULL PRIMARY KEY,
    vault_id BLOB NOT NULL,
    category_id BLOB NOT NULL,
    alias TEXT NOT NULL,
    alias_norm TEXT NOT NULL
);
CREATE TABLE vault_memberships (
    vault_id BLOB NOT NULL,
    user_id TEXT NOT NULL,
    role TEXT NOT NULL,
    PRIMARY KEY (vault_id, user_id)
);
CREATE TABLE flow_memberships (
    flow_id BLOB NOT NULL,
    user_id TEXT NOT NULL,
    role TEXT NOT NULL,
    PRIMARY KEY (flow_id, user_id)
);
CREATE TABLE recurring_templates (
    id BLOB NOT NULL PRIMARY KEY,
    vault_id BLOB NOT NULL,
    kind TEXT NOT NULL,
    amount_minor BIGINT NOT NULL,
    wallet_id BLOB,
    flow_id BLOB,
    category_id BLOB NOT NULL,
    note TEXT,
    created_by TEXT NOT NULL,
    frequency TEXT NOT NULL,
    day_of_period INTEGER NOT NULL,
    start_date TEXT NOT NULL,
    end_date TEXT,
    enabled BOOLEAN NOT NULL DEFAULT 1,
    last_executed_date TEXT,
    created_at TEXT NOT NULL,
    archived_at TEXT
);
CREATE TABLE flow_references (
    id BLOB NOT NULL PRIMARY KEY,
    vault_id BLOB NOT NULL,
    target_flow_id BLOB NOT NULL,
    display_name TEXT,
    created_at TEXT NOT NULL
);
";

// ---------------------------------------------------------------------------
// Fixture ids and helpers
// ---------------------------------------------------------------------------

const USER: &str = "matteo";

const CASA: Uuid = Uuid::from_u128(1);
const LAVORO: Uuid = Uuid::from_u128(2);
const CONTO: Uuid = Uuid::from_u128(10);
const CONTANTI: Uuid = Uuid::from_u128(11);
const VECCHIO: Uuid = Uuid::from_u128(12);
const BANCA: Uuid = Uuid::from_u128(13);
const UNALLOCATED: Uuid = Uuid::from_u128(20);
const SPESA: Uuid = Uuid::from_u128(21);
const VACANZE: Uuid = Uuid::from_u128(22);
const VECCHIA: Uuid = Uuid::from_u128(23);
const UNALLOCATED_LAVORO: Uuid = Uuid::from_u128(24);
const UNCATEGORIZED: Uuid = Uuid::from_u128(30);
const CAT_SPESA: Uuid = Uuid::from_u128(31);
const CAT_STIPENDIO: Uuid = Uuid::from_u128(32);
const CAT_ARCHIVIATA: Uuid = Uuid::from_u128(33);
const UNCATEGORIZED_LAVORO: Uuid = Uuid::from_u128(34);
const ALIAS: Uuid = Uuid::from_u128(40);
const TX_INCOME: Uuid = Uuid::from_u128(50);
const TX_TRANSFER_FLOW: Uuid = Uuid::from_u128(51);
const TX_EXPENSE: Uuid = Uuid::from_u128(52);
const TX_REFUND: Uuid = Uuid::from_u128(53);
const TX_TRANSFER_WALLET: Uuid = Uuid::from_u128(54);
const TX_VOIDED: Uuid = Uuid::from_u128(55);
const RECURRING: Uuid = Uuid::from_u128(80);
const FLOW_REFERENCE: Uuid = Uuid::from_u128(90);

/// A v1 file that deletes itself with the test.
struct TempDb {
    path: PathBuf,
}

impl TempDb {
    fn new(tag: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("sparagne-v1-{tag}-{}.sqlite", Uuid::now_v7()));
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn connect(&self) -> Connection {
        let conn = Connection::open(&self.path).unwrap();
        conn.execute_batch(V1_SCHEMA).unwrap();
        conn.execute(
            "INSERT INTO users (username, password) VALUES (?1, 'x')",
            params![USER],
        )
        .unwrap();
        conn
    }
}

impl Drop for TempDb {
    fn drop(&mut self) {
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{suffix}", self.path.display()));
        }
    }
}

fn b(id: Uuid) -> Vec<u8> {
    id.as_bytes().to_vec()
}

fn vault(conn: &Connection, id: Uuid, name: &str) {
    conn.execute(
        "INSERT INTO vaults (id, name, user_id, currency) VALUES (?1, ?2, ?3, 'EUR')",
        params![b(id), name, USER],
    )
    .unwrap();
}

fn wallet(conn: &Connection, id: Uuid, vault_id: Uuid, name: &str, balance: i64, archived: bool) {
    conn.execute(
        "INSERT INTO wallets (id, name, balance, archived, vault_id) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![b(id), name, balance, archived, b(vault_id)],
    )
    .unwrap();
}

struct FlowRow<'a> {
    id: Uuid,
    vault_id: Uuid,
    name: &'a str,
    system_kind: Option<&'a str>,
    balance: i64,
    max_balance: Option<i64>,
    income_balance: Option<i64>,
    archived: bool,
    allow_negative: bool,
}

fn flow(conn: &Connection, row: FlowRow<'_>) {
    conn.execute(
        "INSERT INTO cash_flows
            (id, name, system_kind, balance, max_balance, income_balance, archived, vault_id,
             allow_negative)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            b(row.id),
            row.name,
            row.system_kind,
            row.balance,
            row.max_balance,
            row.income_balance,
            row.archived,
            b(row.vault_id),
            row.allow_negative,
        ],
    )
    .unwrap();
}

fn category(
    conn: &Connection,
    id: Uuid,
    vault_id: Uuid,
    name: &str,
    norm: &str,
    archived: bool,
    system: bool,
) {
    conn.execute(
        "INSERT INTO categories (id, vault_id, name, name_norm, archived, is_system)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![b(id), b(vault_id), name, norm, archived, system],
    )
    .unwrap();
}

struct TxRow<'a> {
    id: Uuid,
    vault_id: Uuid,
    kind: &'a str,
    occurred_at: &'a str,
    amount: i64,
    category_id: Uuid,
    note: Option<&'a str>,
    created_by: &'a str,
    voided_at: Option<&'a str>,
    idempotency_key: Option<&'a str>,
    refunded: Option<Uuid>,
    /// `(target_kind, target_id, signed amount, attributed user)`.
    legs: &'a [(&'a str, Uuid, i64, Option<&'a str>)],
}

fn transaction(conn: &Connection, row: TxRow<'_>) {
    conn.execute(
        "INSERT INTO transactions
            (id, vault_id, kind, occurred_at, amount_minor, currency, category, note, created_by,
             voided_at, voided_by, refunded_transaction_id, idempotency_key, category_id)
         VALUES (?1, ?2, ?3, ?4, ?5, 'EUR', NULL, ?6, ?7, ?8, NULL, ?9, ?10, ?11)",
        params![
            b(row.id),
            b(row.vault_id),
            row.kind,
            row.occurred_at,
            row.amount,
            row.note,
            row.created_by,
            row.voided_at,
            row.refunded.map(b),
            row.idempotency_key,
            b(row.category_id),
        ],
    )
    .unwrap();
    for (ordinal, (kind, target, amount, user)) in row.legs.iter().enumerate() {
        let leg_id = Uuid::new_v5(&row.id, ordinal.to_string().as_bytes());
        conn.execute(
            "INSERT INTO legs
                (id, transaction_id, target_kind, target_id, amount_minor, currency,
                 attributed_user_id)
             VALUES (?1, ?2, ?3, ?4, ?5, 'EUR', ?6)",
            params![b(leg_id), b(row.id), kind, b(*target), amount, user],
        )
        .unwrap();
    }
}

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/// Two vaults of one user. In `Casa`: three wallets (one archived), three
/// flows besides Unallocated (one capped, one archived), four categories (one
/// system, one archived) with an alias, the five transaction kinds, a voided
/// expense, a monthly template already run three times, a `flow_reference` and
/// a `flow_membership`. Every balance is exactly the sum of the legs.
fn full_fixture() -> TempDb {
    let db = TempDb::new("full");
    let conn = db.connect();

    vault(&conn, CASA, "Casa");
    vault(&conn, LAVORO, "Lavoro");

    // 500000 - 30000 + 10000 - 50000
    wallet(&conn, CONTO, CASA, "Conto", 430_000, false);
    // 50000; the voided expense of 5000 does not count
    wallet(&conn, CONTANTI, CASA, "Contanti", 50_000, false);
    wallet(&conn, VECCHIO, CASA, "Vecchio", 0, true);
    wallet(&conn, BANCA, LAVORO, "Banca", 0, false);

    flow(
        &conn,
        FlowRow {
            id: UNALLOCATED,
            vault_id: CASA,
            name: "unallocated",
            system_kind: Some("unallocated"),
            balance: 300_000,
            max_balance: None,
            income_balance: None,
            archived: false,
            allow_negative: true,
        },
    );
    flow(
        &conn,
        FlowRow {
            id: SPESA,
            vault_id: CASA,
            name: "Spesa",
            system_kind: None,
            balance: 180_000,
            max_balance: None,
            income_balance: None,
            archived: false,
            allow_negative: false,
        },
    );
    flow(
        &conn,
        FlowRow {
            id: VACANZE,
            vault_id: CASA,
            name: "Vacanze",
            system_kind: None,
            balance: 0,
            max_balance: Some(1_000_000),
            income_balance: None,
            archived: false,
            allow_negative: false,
        },
    );
    flow(
        &conn,
        FlowRow {
            id: VECCHIA,
            vault_id: CASA,
            name: "Vecchia",
            system_kind: None,
            balance: 0,
            max_balance: None,
            income_balance: None,
            archived: true,
            allow_negative: false,
        },
    );
    flow(
        &conn,
        FlowRow {
            id: UNALLOCATED_LAVORO,
            vault_id: LAVORO,
            name: "unallocated",
            system_kind: Some("unallocated"),
            balance: 0,
            max_balance: None,
            income_balance: None,
            archived: false,
            allow_negative: true,
        },
    );

    category(
        &conn,
        UNCATEGORIZED,
        CASA,
        "Uncategorized",
        "uncategorized",
        false,
        true,
    );
    category(&conn, CAT_SPESA, CASA, "Spesa", "spesa", false, false);
    category(
        &conn,
        CAT_STIPENDIO,
        CASA,
        "Stipendio",
        "stipendio",
        false,
        false,
    );
    category(
        &conn,
        CAT_ARCHIVIATA,
        CASA,
        "Archiviata",
        "archiviata",
        true,
        false,
    );
    category(
        &conn,
        UNCATEGORIZED_LAVORO,
        LAVORO,
        "Uncategorized",
        "uncategorized",
        false,
        true,
    );
    conn.execute(
        "INSERT INTO category_aliases (id, vault_id, category_id, alias, alias_norm)
         VALUES (?1, ?2, ?3, 'Supermercato', 'supermercato')",
        params![b(ALIAS), b(CASA), b(CAT_SPESA)],
    )
    .unwrap();

    // The formats differ on purpose: v1 wrote SeaORM timestamps, whose text
    // shape changed with the driver.
    transaction(
        &conn,
        TxRow {
            id: TX_INCOME,
            vault_id: CASA,
            kind: "income",
            occurred_at: "2026-01-02T08:00:00Z",
            amount: 500_000,
            category_id: CAT_STIPENDIO,
            note: Some("stipendio di gennaio"),
            created_by: USER,
            voided_at: None,
            idempotency_key: Some("bot:1"),
            refunded: None,
            legs: &[
                ("wallet", CONTO, 500_000, None),
                ("flow", UNALLOCATED, 500_000, Some(USER)),
            ],
        },
    );
    transaction(
        &conn,
        TxRow {
            id: TX_TRANSFER_FLOW,
            vault_id: CASA,
            kind: "transfer_flow",
            occurred_at: "2026-01-03 09:00:00+00:00",
            amount: 200_000,
            category_id: UNCATEGORIZED,
            note: None,
            created_by: USER,
            voided_at: None,
            idempotency_key: None,
            refunded: None,
            legs: &[
                ("flow", UNALLOCATED, -200_000, None),
                ("flow", SPESA, 200_000, None),
            ],
        },
    );
    transaction(
        &conn,
        TxRow {
            id: TX_EXPENSE,
            vault_id: CASA,
            kind: "expense",
            occurred_at: "2026-01-04 10:00:00",
            amount: 30_000,
            category_id: CAT_SPESA,
            note: Some("supermercato"),
            created_by: "altro",
            voided_at: None,
            idempotency_key: None,
            refunded: None,
            legs: &[
                ("wallet", CONTO, -30_000, None),
                ("flow", SPESA, -30_000, None),
            ],
        },
    );
    transaction(
        &conn,
        TxRow {
            id: TX_REFUND,
            vault_id: CASA,
            kind: "refund",
            occurred_at: "2026-01-05T11:00:00Z",
            amount: 10_000,
            category_id: CAT_SPESA,
            note: None,
            created_by: USER,
            voided_at: None,
            idempotency_key: None,
            refunded: Some(TX_EXPENSE),
            legs: &[
                ("wallet", CONTO, 10_000, None),
                ("flow", SPESA, 10_000, None),
            ],
        },
    );
    transaction(
        &conn,
        TxRow {
            id: TX_TRANSFER_WALLET,
            vault_id: CASA,
            kind: "transfer_wallet",
            occurred_at: "2026-01-06T12:00:00Z",
            amount: 50_000,
            category_id: UNCATEGORIZED,
            note: Some("prelievo"),
            created_by: USER,
            voided_at: None,
            idempotency_key: None,
            refunded: None,
            legs: &[
                ("wallet", CONTO, -50_000, None),
                ("wallet", CONTANTI, 50_000, None),
            ],
        },
    );
    transaction(
        &conn,
        TxRow {
            id: TX_VOIDED,
            vault_id: CASA,
            kind: "expense",
            occurred_at: "2026-01-07T13:00:00Z",
            amount: 5_000,
            category_id: CAT_SPESA,
            note: None,
            created_by: USER,
            voided_at: Some("2026-01-08T09:00:00Z"),
            idempotency_key: None,
            refunded: None,
            legs: &[
                ("wallet", CONTANTI, -5_000, None),
                ("flow", SPESA, -5_000, None),
            ],
        },
    );

    conn.execute(
        "INSERT INTO recurring_templates
            (id, vault_id, kind, amount_minor, wallet_id, flow_id, category_id, note, created_by,
             frequency, day_of_period, start_date, end_date, enabled, last_executed_date,
             created_at, archived_at)
         VALUES (?1, ?2, 'income', 500000, ?3, ?4, ?5, 'stipendio', ?6,
                 'monthly', 27, '2026-01-01', NULL, 1, '2026-03-27',
                 '2026-01-01T00:00:00Z', NULL)",
        params![
            b(RECURRING),
            b(CASA),
            b(CONTO),
            b(UNALLOCATED),
            b(CAT_STIPENDIO),
            USER
        ],
    )
    .unwrap();

    conn.execute(
        "INSERT INTO flow_references (id, vault_id, target_flow_id, display_name, created_at)
         VALUES (?1, ?2, ?3, 'Spesa condivisa', '2026-01-01T00:00:00Z')",
        params![b(FLOW_REFERENCE), b(CASA), b(SPESA)],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO flow_memberships (flow_id, user_id, role) VALUES (?1, ?2, 'editor')",
        params![b(SPESA), USER],
    )
    .unwrap();

    db
}

/// One vault, one wallet whose balance the legs do not explain by 12345.
fn opening_fixture() -> TempDb {
    let db = TempDb::new("opening");
    let conn = db.connect();
    vault(&conn, CASA, "Casa");
    wallet(&conn, CONTO, CASA, "Conto", 112_345, false);
    flow(
        &conn,
        FlowRow {
            id: UNALLOCATED,
            vault_id: CASA,
            name: "unallocated",
            system_kind: Some("unallocated"),
            balance: 100_000,
            max_balance: None,
            income_balance: None,
            archived: false,
            allow_negative: true,
        },
    );
    category(
        &conn,
        UNCATEGORIZED,
        CASA,
        "Uncategorized",
        "uncategorized",
        false,
        true,
    );
    transaction(
        &conn,
        TxRow {
            id: TX_INCOME,
            vault_id: CASA,
            kind: "income",
            occurred_at: "2026-02-01T08:00:00Z",
            amount: 100_000,
            category_id: UNCATEGORIZED,
            note: None,
            created_by: USER,
            voided_at: None,
            idempotency_key: None,
            refunded: None,
            legs: &[
                ("wallet", CONTO, 100_000, None),
                ("flow", UNALLOCATED, 100_000, None),
            ],
        },
    );
    db
}

/// One vault whose history the v2 rules refuse in part. v1 wrote the expense
/// after the income that funded it but backdated it, and the importer replays
/// by `occurred_at`: the expense then lands on an empty flow that cannot go
/// negative. Every v1 balance is still the sum of its legs.
fn rejection_fixture() -> TempDb {
    let db = TempDb::new("rejection");
    let conn = db.connect();
    vault(&conn, CASA, "Casa");
    wallet(&conn, CONTO, CASA, "Conto", 51_000, false);
    flow(
        &conn,
        FlowRow {
            id: UNALLOCATED,
            vault_id: CASA,
            name: "unallocated",
            system_kind: Some("unallocated"),
            balance: 0,
            max_balance: None,
            income_balance: None,
            archived: false,
            allow_negative: true,
        },
    );
    flow(
        &conn,
        FlowRow {
            id: VACANZE,
            vault_id: CASA,
            name: "Vacanze",
            system_kind: None,
            balance: 51_000,
            max_balance: None,
            income_balance: None,
            archived: false,
            allow_negative: false,
        },
    );
    category(
        &conn,
        UNCATEGORIZED,
        CASA,
        "Uncategorized",
        "uncategorized",
        false,
        true,
    );
    transaction(
        &conn,
        TxRow {
            id: TX_EXPENSE,
            vault_id: CASA,
            kind: "expense",
            occurred_at: "2026-02-01T08:00:00Z",
            amount: 50_000,
            category_id: UNCATEGORIZED,
            note: None,
            created_by: USER,
            voided_at: None,
            idempotency_key: None,
            refunded: None,
            legs: &[
                ("wallet", CONTO, -50_000, None),
                ("flow", VACANZE, -50_000, None),
            ],
        },
    );
    transaction(
        &conn,
        TxRow {
            id: TX_INCOME,
            vault_id: CASA,
            kind: "income",
            occurred_at: "2026-02-02T08:00:00Z",
            amount: 100_000,
            category_id: UNCATEGORIZED,
            note: None,
            created_by: USER,
            voided_at: None,
            idempotency_key: None,
            refunded: None,
            legs: &[
                ("wallet", CONTO, 100_000, None),
                ("flow", VACANZE, 100_000, None),
            ],
        },
    );
    transaction(
        &conn,
        TxRow {
            id: TX_REFUND,
            vault_id: CASA,
            kind: "refund",
            occurred_at: "2026-02-03T08:00:00Z",
            amount: 1_000,
            category_id: UNCATEGORIZED,
            note: None,
            created_by: USER,
            voided_at: None,
            idempotency_key: None,
            refunded: None,
            legs: &[
                ("wallet", CONTO, 1_000, None),
                ("flow", VACANZE, 1_000, None),
            ],
        },
    );
    db
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn options() -> ImportOptions {
    ImportOptions::new(USER)
}

fn run(db: &TempDb, options: &ImportOptions) -> (Core, ImportReport) {
    let mut core = Core::open_in_memory().unwrap();
    let report = import_v1(&mut core, db.path(), options).unwrap();
    (core, report)
}

fn v2_vault(v1: Uuid) -> Uuid {
    command_id("vault", &v1.to_string())
}

fn wallet_balance(core: &Core, vault: Uuid, name: &str) -> i64 {
    core.snapshot(vault)
        .unwrap()
        .wallets
        .iter()
        .find(|w| w.name == name)
        .unwrap()
        .balance
}

fn flow_balance(core: &Core, vault: Uuid, name: &str) -> i64 {
    core.snapshot(vault)
        .unwrap()
        .flows
        .iter()
        .find(|f| f.name == name)
        .unwrap()
        .balance
}

fn date(y: i32, m: u32, d: u32) -> chrono::NaiveDate {
    chrono::NaiveDate::from_ymd_opt(y, m, d).unwrap()
}

fn ledger(core: &Core, vault: Uuid) -> Vec<(TransactionKind, i64)> {
    let filter = TransactionFilter {
        include_voided: true,
        include_transfers: true,
        ascending: true,
        ..Default::default()
    };
    core.list_transactions(vault, &filter, 100, None)
        .unwrap()
        .items
        .into_iter()
        .map(|t| (t.kind, t.amount))
        .collect()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn the_report_counts_every_entity() {
    let db = full_fixture();
    let (_core, report) = run(&db, &options());

    assert_eq!(report.vaults, 2);
    assert_eq!(report.wallets, 4);
    assert_eq!(report.wallet_openings, 0);
    assert_eq!(report.flows, 3);
    assert_eq!(report.categories, 3);
    assert_eq!(report.aliases, 1);
    assert_eq!(report.transactions.income, 1);
    assert_eq!(report.transactions.expense, 2);
    assert_eq!(report.transactions.refund, 1);
    assert_eq!(report.transactions.transfer_wallet, 1);
    assert_eq!(report.transactions.transfer_flow, 1);
    assert_eq!(report.transactions.total(), 6);
    assert_eq!(report.voids, 1);
    assert_eq!(report.recurring, 1);
    assert_eq!(report.recurring_periods_skipped, 3);
    assert_eq!(report.archived_wallets, 1);
    assert_eq!(report.archived_flows, 1);
    assert_eq!(report.archived_categories, 1);
    assert_eq!(report.commands_executed, 27);
    assert_eq!(report.commands_deduplicated, 0);
    assert!(report.rejected.is_empty(), "{:?}", report.rejected);
    assert!(
        report.flow_balance_mismatches.is_empty(),
        "{:?}",
        report.flow_balance_mismatches
    );
}

#[test]
fn the_snapshot_balances_are_the_v1_balances() {
    let db = full_fixture();
    let (core, _report) = run(&db, &options());
    let casa = v2_vault(CASA);

    assert_eq!(wallet_balance(&core, casa, "Conto"), 430_000);
    assert_eq!(wallet_balance(&core, casa, "Contanti"), 50_000);
    assert_eq!(wallet_balance(&core, casa, "Vecchio"), 0);
    assert_eq!(flow_balance(&core, casa, "unallocated"), 300_000);
    assert_eq!(flow_balance(&core, casa, "Spesa"), 180_000);
    assert_eq!(flow_balance(&core, casa, "Vacanze"), 0);
    assert_eq!(flow_balance(&core, casa, "Vecchia"), 0);
    assert_eq!(wallet_balance(&core, v2_vault(LAVORO), "Banca"), 0);
}

#[test]
fn the_ledger_has_every_kind_with_its_amount() {
    let db = full_fixture();
    let (core, _report) = run(&db, &options());

    assert_eq!(
        ledger(&core, v2_vault(CASA)),
        vec![
            (TransactionKind::Income, 500_000),
            (TransactionKind::TransferFlow, 200_000),
            (TransactionKind::Expense, 30_000),
            (TransactionKind::Refund, 10_000),
            (TransactionKind::TransferWallet, 50_000),
            (TransactionKind::Expense, 5_000),
        ]
    );
    assert!(ledger(&core, v2_vault(LAVORO)).is_empty());
}

#[test]
fn a_second_import_adds_no_command() {
    let db = full_fixture();
    let mut core = Core::open_in_memory().unwrap();
    let first = import_v1(&mut core, db.path(), &options()).unwrap();
    let before: usize = [CASA, LAVORO]
        .iter()
        .map(|v| core.commands_since(v2_vault(*v), 0).unwrap().len())
        .sum();

    let second = import_v1(&mut core, db.path(), &options()).unwrap();
    let after: usize = [CASA, LAVORO]
        .iter()
        .map(|v| core.commands_since(v2_vault(*v), 0).unwrap().len())
        .sum();

    assert_eq!(before, first.commands_executed);
    assert_eq!(after, before);
    assert_eq!(second.commands_executed, 0);
    assert_eq!(second.commands_deduplicated, first.commands_executed);
    assert_eq!(
        ledger(&core, v2_vault(CASA)).len(),
        first.transactions.total()
    );
}

#[test]
fn the_vault_filter_imports_only_that_vault() {
    let db = full_fixture();
    let picked = ImportOptions {
        vault: Some("Casa".to_string()),
        ..options()
    };
    let (core, report) = run(&db, &picked);

    assert_eq!(report.vaults, 1);
    assert_eq!(report.wallets, 3);
    assert!(core.snapshot(v2_vault(CASA)).is_ok());
    assert!(matches!(
        core.snapshot(v2_vault(LAVORO)),
        Err(DomainError::NotFound(_))
    ));
}

#[test]
fn an_unknown_vault_name_is_not_found() {
    let db = full_fixture();
    let mut core = Core::open_in_memory().unwrap();
    let missing = ImportOptions {
        vault: Some("Inesistente".to_string()),
        ..options()
    };
    assert!(matches!(
        import_v1(&mut core, db.path(), &missing),
        Err(DomainError::NotFound(_))
    ));
}

#[test]
fn a_rejected_command_does_not_stop_the_import() {
    let db = rejection_fixture();
    let (core, report) = run(&db, &options());

    assert_eq!(report.rejected.len(), 1, "{:?}", report.rejected);
    let rejected = &report.rejected[0];
    assert_eq!(rejected.kind, "expense");
    assert_eq!(rejected.code, "insufficient_funds");
    assert_eq!(
        rejected.command_id,
        command_id("tx", &TX_EXPENSE.to_string())
    );

    // The transactions on either side of it went through.
    assert_eq!(report.transactions.income, 1);
    assert_eq!(report.transactions.refund, 1);
    assert_eq!(report.transactions.expense, 0);
    assert_eq!(report.wallet_openings, 0);
    assert_eq!(
        ledger(&core, v2_vault(CASA)),
        vec![
            (TransactionKind::Income, 100_000),
            (TransactionKind::Refund, 1_000),
        ]
    );
    assert_eq!(wallet_balance(&core, v2_vault(CASA), "Conto"), 101_000);
}

#[test]
fn a_wallet_balance_the_legs_do_not_explain_becomes_an_opening() {
    let db = opening_fixture();
    let (core, report) = run(&db, &options());
    let casa = v2_vault(CASA);

    assert_eq!(report.wallet_openings, 1);
    // The wallet matches v1 exactly; the opening entry funded it from
    // Unallocated, which is 12345 above the v1 value on purpose.
    assert_eq!(wallet_balance(&core, casa, "Conto"), 112_345);
    assert_eq!(flow_balance(&core, casa, "unallocated"), 112_345);
    let kinds = ledger(&core, casa);
    assert_eq!(kinds.len(), 2);
    assert!(kinds.contains(&(TransactionKind::Income, 12_345)));
}

#[test]
fn flow_references_and_memberships_are_counted_but_not_imported() {
    let db = full_fixture();
    let (core, report) = run(&db, &options());

    assert_eq!(report.dropped.flow_references, 1);
    assert_eq!(report.dropped.flow_memberships, 1);
    let flows = core.snapshot(v2_vault(CASA)).unwrap().flows;
    assert_eq!(flows.len(), 4, "{flows:?}");
    assert!(!flows.iter().any(|f| f.name == "Spesa condivisa"));
}

#[test]
fn the_dropped_v1_fields_are_counted() {
    let db = full_fixture();
    let (_core, report) = run(&db, &options());

    assert_eq!(report.dropped.attributed_user_legs, 1);
    assert_eq!(report.dropped.idempotency_keys, 1);
    assert_eq!(report.dropped.refund_links, 1);
    assert_eq!(report.dropped.transaction_authors, 1);
    assert_eq!(report.dropped.void_timestamps, 1);
    assert_eq!(report.dropped.categories_mapped_to_system, 2);
}

#[test]
fn the_voided_transaction_is_voided_and_moves_no_balance() {
    let db = full_fixture();
    let (core, _report) = run(&db, &options());
    let casa = v2_vault(CASA);

    let visible = core
        .list_transactions(
            casa,
            &TransactionFilter {
                include_transfers: true,
                ..Default::default()
            },
            100,
            None,
        )
        .unwrap()
        .items;
    assert_eq!(visible.len(), 5);
    assert_eq!(ledger(&core, casa).len(), 6);
    // Contanti got 50000 and lost nothing: the 5000 expense is void.
    assert_eq!(wallet_balance(&core, casa, "Contanti"), 50_000);
}

#[test]
fn the_recurring_template_keeps_its_schedule_and_its_past_periods() {
    let db = full_fixture();
    let (core, report) = run(&db, &options());
    let casa = v2_vault(CASA);

    let templates = core.list_recurring(casa, true).unwrap();
    assert_eq!(templates.len(), 1);
    let template = &templates[0];
    assert_eq!(template.amount, 500_000);
    assert_eq!(template.kind, TransactionKind::Income);
    assert_eq!(template.note.as_deref(), Some("stipendio"));
    assert!(template.enabled);
    assert!(!template.archived);
    assert_eq!(template.schedule.interval, 1);
    assert_eq!(
        template.schedule.frequency,
        sparagne_core::Frequency::Monthly { day: 27 }
    );
    assert_eq!(report.recurring_periods_skipped, 3);
    // The three periods up to last_executed_date are handled, so the first
    // period still pending is April's.
    let pending = core.pending_recurring(casa, date(2026, 6, 30)).unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(
        pending[0].due,
        vec![date(2026, 4, 27), date(2026, 5, 27), date(2026, 6, 27)]
    );
}

#[test]
fn the_same_v1_file_always_gives_the_same_command_ids() {
    let db = full_fixture();
    let (first, _) = run(&db, &options());
    let (second, _) = run(&db, &options());

    let ids = |core: &Core| -> Vec<Uuid> {
        core.commands_since(v2_vault(CASA), 0)
            .unwrap()
            .into_iter()
            .map(|record| record.envelope.id)
            .collect()
    };
    assert_eq!(ids(&first), ids(&second));
    assert!(ids(&first).contains(&command_id("tx", &TX_INCOME.to_string())));
}

#[test]
fn an_unknown_timezone_is_refused() {
    let db = full_fixture();
    let mut core = Core::open_in_memory().unwrap();
    let bad = ImportOptions {
        timezone: "Mars/Olympus".to_string(),
        ..options()
    };
    assert!(matches!(
        import_v1(&mut core, db.path(), &bad),
        Err(DomainError::InvalidName(_))
    ));
}

#[test]
fn the_timezone_gives_the_accounting_day_its_offset() {
    let db = full_fixture();
    let winter = ImportOptions {
        timezone: "Europe/Rome".to_string(),
        ..options()
    };
    let (core, _) = run(&db, &winter);
    let filter = TransactionFilter {
        include_transfers: true,
        ascending: true,
        ..Default::default()
    };
    let first = core
        .list_transactions(v2_vault(CASA), &filter, 1, None)
        .unwrap()
        .items
        .remove(0);
    // 08:00 UTC on 2 January is 09:00 in Rome, one hour east.
    assert_eq!(first.occurred_at.offset().local_minus_utc(), 3600);
    assert_eq!(
        first.occurred_at.format("%Y-%m-%d %H:%M").to_string(),
        "2026-01-02 09:00"
    );

    let (utc_core, _) = run(
        &db,
        &ImportOptions {
            timezone: "UTC".to_string(),
            ..options()
        },
    );
    let same = utc_core
        .list_transactions(v2_vault(CASA), &filter, 1, None)
        .unwrap()
        .items
        .remove(0);
    assert_eq!(same.occurred_at.offset().local_minus_utc(), 0);
    // Same instant, different offset.
    assert_eq!(same.occurred_at.timestamp(), first.occurred_at.timestamp());
}

#[test]
fn an_alias_resolves_to_its_category() {
    let db = full_fixture();
    let (core, _) = run(&db, &options());
    let casa = v2_vault(CASA);

    let aliases = core.aliases(casa).unwrap();
    assert_eq!(aliases.len(), 1);
    assert_eq!(aliases[0].alias, "Supermercato");
    let categories = core.categories(casa, true).unwrap();
    let spesa = categories.iter().find(|c| c.name == "Spesa").unwrap();
    assert_eq!(aliases[0].category_id, spesa.id);
    // Archived in v1, archived in v2; the system ones came with the vault.
    let archiviata = categories.iter().find(|c| c.name == "Archiviata").unwrap();
    assert!(archiviata.archived);
    assert!(
        categories
            .iter()
            .any(|c| c.is_system && c.name == "Opening")
    );
}
