//! Whole-database chores and local refusals: `backup_to`, `forget_vault`,
//! `reject_outbox`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::path::{Path, PathBuf};

use common::{T0, all, entry, wallet_cmd};
use sparagne_core::{
    CategoryView, Command, CommandEnvelope, Core, Currency, DomainError, SyncReport, SyncState,
    TransactionView, VaultSnapshot, sync::PushRequest,
};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

/// A push or pull limit that is no limit.
const ALL: usize = usize::MAX;

/// A database path in the temp directory, removed with its WAL files on drop.
struct TempDb(PathBuf);

impl TempDb {
    fn new(label: &str) -> Self {
        Self(std::env::temp_dir().join(format!("sparagne-{label}-{}.sqlite", Uuid::now_v7())))
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDb {
    fn drop(&mut self) {
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{suffix}", self.0.display()));
        }
    }
}

fn exec(core: &mut Core, vault: Uuid, author: &str, command: Command) -> Uuid {
    core.execute(CommandEnvelope::new(vault, author, command))
        .unwrap()
        .result_id
        .unwrap_or(vault)
}

fn create_vault(core: &mut Core, name: &str) -> Uuid {
    core.execute(CommandEnvelope::create_vault("alice", name, Currency::Eur))
        .unwrap()
        .result_id
        .unwrap()
}

fn spend(amount: i64, wallet: Uuid, category: &str, secs: i64) -> Command {
    Command::Expense(entry(amount, Some(wallet), None, Some(category), secs))
}

fn category(name: &str) -> Command {
    Command::CreateCategory {
        name: name.to_string(),
    }
}

/// Push the whole outbox to `server`, fold the answer back, pull, fold back.
fn sync(client: &mut Core, server: &mut Core, vault: Uuid) {
    let request = client.push_request(vault, ALL).unwrap();
    let response = server.serve_push(vault, &request).unwrap();
    client.apply_push_response(vault, &response).unwrap();
    let since = client.sync_state(vault).unwrap().last_server_seq;
    client
        .integrate_pull(vault, &server.serve_pull(vault, since, ALL).unwrap())
        .unwrap();
}

type Projection = (VaultSnapshot, Vec<TransactionView>, Vec<CategoryView>);

fn projection(core: &Core, vault: Uuid) -> Projection {
    (
        core.snapshot(vault).unwrap(),
        core.list_transactions(vault, &all(), 200, None)
            .unwrap()
            .items,
        core.categories(vault, true).unwrap(),
    )
}

fn zeros() -> SyncState {
    SyncState {
        last_server_seq: 0,
        outbox: 0,
        rejected: 0,
    }
}

/// Alice's vault with a wallet and one spend, pushed to `server`.
fn synced() -> (Core, Core, Uuid, Uuid) {
    let mut server = Core::open_in_memory().unwrap();
    let mut alice = Core::open_in_memory().unwrap();
    let vault = create_vault(&mut alice, "Casa");
    let wallet = exec(&mut alice, vault, "alice", wallet_cmd("Cash", 10_000));
    exec(
        &mut alice,
        vault,
        "alice",
        spend(12_00, wallet, "Spesa", T0),
    );
    sync(&mut alice, &mut server, vault);
    assert_eq!(alice.sync_state(vault).unwrap().outbox, 0);
    (server, alice, vault, wallet)
}

// ---------------------------------------------------------------------------
// backup_to
// ---------------------------------------------------------------------------

#[test]
fn a_backup_opens_with_the_same_vaults_and_transactions() {
    let source = TempDb::new("source");
    let copy = TempDb::new("backup");
    let mut core = Core::open(source.path()).unwrap();
    let casa = create_vault(&mut core, "Casa");
    let wallet = exec(&mut core, casa, "alice", wallet_cmd("Cash", 10_000));
    exec(&mut core, casa, "alice", spend(12_00, wallet, "Spesa", T0));
    let viaggi = create_vault(&mut core, "Viaggi");
    let casa_before = projection(&core, casa);

    core.backup_to(copy.path()).unwrap();
    // Writing to the source afterwards does not reach the copy.
    exec(&mut core, casa, "alice", spend(1_00, wallet, "Bar", T0 + 1));

    let restored = Core::open(copy.path()).unwrap();
    assert_eq!(restored.vaults().unwrap(), core.vaults().unwrap());
    assert_eq!(projection(&restored, casa), casa_before);
    assert_eq!(
        restored.commands_since(casa, 0).unwrap(),
        core.commands_since(casa, 0).unwrap()[..3]
    );
    assert_eq!(
        restored.snapshot(viaggi).unwrap(),
        core.snapshot(viaggi).unwrap()
    );
    assert_eq!(
        restored.sync_state(casa).unwrap(),
        SyncState {
            outbox: 3,
            ..zeros()
        }
    );
}

#[test]
fn a_backup_never_overwrites_a_file() {
    let core = Core::open_in_memory().unwrap();
    let taken = TempDb::new("taken");
    std::fs::write(taken.path(), b"keep me").unwrap();
    let err = core.backup_to(taken.path()).unwrap_err();
    assert_eq!(err.code(), "already_exists");
    assert_eq!(std::fs::read(taken.path()).unwrap(), b"keep me");

    // Even an empty file, which SQLite itself would happily fill.
    let empty = TempDb::new("empty");
    std::fs::write(empty.path(), b"").unwrap();
    assert!(matches!(
        core.backup_to(empty.path()),
        Err(DomainError::AlreadyExists(_))
    ));

    // A directory that does not exist is a storage failure.
    let nowhere = std::env::temp_dir()
        .join(format!("sparagne-missing-{}", Uuid::now_v7()))
        .join("backup.sqlite");
    assert_eq!(
        core.backup_to(&nowhere).unwrap_err().code(),
        "storage_error"
    );
}

// ---------------------------------------------------------------------------
// forget_vault
// ---------------------------------------------------------------------------

#[test]
fn forgetting_a_live_vault_drops_projection_log_outbox_and_rejections() {
    let (_server, mut alice, vault, wallet) = synced();
    let other = create_vault(&mut alice, "Altro");
    let other_before = projection(&alice, other);

    // One refused command and two still waiting in the outbox.
    exec(
        &mut alice,
        vault,
        "alice",
        spend(3_00, wallet, "Bar", T0 + 1),
    );
    alice
        .reject_outbox(vault, "forbidden", "read only")
        .unwrap();
    exec(
        &mut alice,
        vault,
        "alice",
        spend(4_00, wallet, "Bar", T0 + 2),
    );
    exec(&mut alice, vault, "alice", category("Casa"));
    assert_eq!(alice.sync_state(vault).unwrap().rejected, 1);

    assert_eq!(alice.forget_vault(vault).unwrap(), 2);

    assert!(alice.vaults().unwrap().iter().all(|v| v.id != vault));
    assert!(alice.deleted_vaults().unwrap().is_empty());
    assert_eq!(alice.sync_state(vault).unwrap(), zeros());
    assert!(alice.commands_since(vault, 0).unwrap().is_empty());
    assert!(alice.rejected_commands(vault).unwrap().is_empty());
    assert!(alice.categories(vault, true).unwrap().is_empty());
    assert_eq!(alice.snapshot(vault).unwrap_err().code(), "not_found");
    // The other vault is untouched.
    assert_eq!(projection(&alice, other), other_before);
    assert_eq!(alice.sync_state(other).unwrap().outbox, 1);
}

#[test]
fn forgetting_a_deleted_vault_drops_its_pending_deletion() {
    let (_server, mut alice, vault, _) = synced();
    exec(&mut alice, vault, "alice", Command::DeleteVault);
    assert_eq!(alice.deleted_vaults().unwrap(), vec![vault]);

    assert_eq!(alice.forget_vault(vault).unwrap(), 1);
    assert!(alice.deleted_vaults().unwrap().is_empty());
    assert!(alice.vaults().unwrap().is_empty());
    assert_eq!(alice.sync_state(vault).unwrap(), zeros());
}

#[test]
fn forgetting_an_unknown_vault_is_quiet() {
    let (_server, mut alice, vault, _) = synced();
    let before = projection(&alice, vault);
    assert_eq!(alice.forget_vault(Uuid::now_v7()).unwrap(), 0);
    assert_eq!(projection(&alice, vault), before);
    // A fully pushed vault has nothing to throw away, and forgetting it twice
    // is quiet.
    assert_eq!(alice.forget_vault(vault).unwrap(), 0);
    assert!(alice.vaults().unwrap().is_empty());
    assert_eq!(alice.forget_vault(vault).unwrap(), 0);
}

#[test]
fn a_forgotten_vault_joins_again_with_a_pull_from_zero() {
    let (server, mut alice, vault, wallet) = synced();
    // An unpushed spend is lost with the vault: the server never saw it.
    let lost = exec(
        &mut alice,
        vault,
        "alice",
        spend(5_00, wallet, "Bar", T0 + 1),
    );
    assert_eq!(alice.forget_vault(vault).unwrap(), 1);

    let report = alice
        .integrate_pull(vault, &server.serve_pull(vault, 0, ALL).unwrap())
        .unwrap();
    assert!(report.rebased);
    assert_eq!(report.received, 3);
    assert!(!report.has_more);
    assert_eq!(projection(&alice, vault), projection(&server, vault));
    assert!(
        alice
            .list_transactions(vault, &all(), 50, None)
            .unwrap()
            .items
            .iter()
            .all(|t| t.id != lost)
    );
    assert_eq!(
        alice.sync_state(vault).unwrap(),
        SyncState {
            last_server_seq: server.last_seq(vault).unwrap(),
            ..zeros()
        }
    );
}

// ---------------------------------------------------------------------------
// reject_outbox
// ---------------------------------------------------------------------------

#[test]
fn reject_outbox_turns_the_whole_outbox_into_rejections() {
    let (mut server, mut alice, vault, wallet) = synced();
    let before = projection(&alice, vault);

    // A viewer's local writes: the spend depends on the category before it.
    let first = exec(&mut alice, vault, "alice", category("Casa"));
    let second = exec(
        &mut alice,
        vault,
        "alice",
        spend(2_00, wallet, "Casa", T0 + 1),
    );
    assert_eq!(alice.sync_state(vault).unwrap().outbox, 2);

    let report = alice
        .reject_outbox(vault, "forbidden", "viewers cannot write")
        .unwrap();
    assert!(report.rebased);
    assert_eq!(report.confirmed, 0);
    assert_eq!(report.received, 0);
    assert_eq!(
        report
            .rejected
            .iter()
            .map(|r| (
                r.command_id,
                r.kind.as_str(),
                r.code.as_str(),
                r.message.as_str()
            ))
            .collect::<Vec<_>>(),
        vec![
            (
                first,
                "create_category",
                "forbidden",
                "viewers cannot write"
            ),
            (second, "expense", "forbidden", "viewers cannot write"),
        ]
    );
    assert_eq!(report.server_last_seq, server.last_seq(vault).unwrap());
    assert!(!report.has_more);

    // The projection is rebuilt without them, and they wait for the UI.
    assert_eq!(projection(&alice, vault), before);
    assert_eq!(
        alice.sync_state(vault).unwrap(),
        SyncState {
            last_server_seq: server.last_seq(vault).unwrap(),
            outbox: 0,
            rejected: 2,
        }
    );
    assert_eq!(alice.rejected_commands(vault).unwrap(), report.rejected);
    assert!(alice.push_request(vault, ALL).unwrap().commands.is_empty());

    // The vault keeps pulling what the others write.
    let bob = CommandEnvelope::new(vault, "bob", spend(7_00, wallet, "Bar", T0 + 2));
    server
        .serve_push(
            vault,
            &PushRequest {
                commands: vec![bob],
            },
        )
        .unwrap();
    sync(&mut alice, &mut server, vault);
    assert_eq!(projection(&alice, vault), projection(&server, vault));
    assert_eq!(alice.sync_state(vault).unwrap().rejected, 2);
}

#[test]
fn reject_outbox_with_nothing_pending_changes_nothing() {
    let (server, mut alice, vault, _) = synced();
    let before = projection(&alice, vault);
    let report = alice
        .reject_outbox(vault, "forbidden", "read only")
        .unwrap();
    assert_eq!(
        report,
        SyncReport {
            server_last_seq: server.last_seq(vault).unwrap(),
            ..SyncReport::default()
        }
    );
    assert_eq!(projection(&alice, vault), before);
    assert_eq!(alice.sync_state(vault).unwrap().rejected, 0);

    // A vault never seen: nothing to refuse, nothing known.
    assert_eq!(
        alice
            .reject_outbox(Uuid::now_v7(), "forbidden", "x")
            .unwrap(),
        SyncReport::default()
    );
}
