//! Whole-database chores and local refusals: `backup_to`, `forget_vault`,
//! `reject_outbox`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use common::{T0, all, entry, wallet_cmd};
use sparagne_core::{
    CategoryView, Command, CommandEnvelope, Core, Currency, SyncReport, SyncState, TransactionView,
    VaultSnapshot,
};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

/// A push or pull limit that is no limit.
const ALL: usize = usize::MAX;

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
            &sparagne_core::sync::PushRequest {
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
