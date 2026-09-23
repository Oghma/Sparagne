//! `Core::execute_batch`: several commands in one local transaction, each
//! keeping its own log row and syncing on its own.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use common::{T0, all, entry, flow_cmd, list, setup, wallet_cmd};
use sparagne_core::{
    Command, CommandEnvelope, Core, Currency, DomainError, FlowMode, SyncReport, sync::PushOutcome,
};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

/// A push or pull limit that is no limit.
const ALL: usize = usize::MAX;

fn category(name: &str) -> Command {
    Command::CreateCategory {
        name: name.to_string(),
    }
}

fn spend(amount: i64, wallet: Uuid, flow: Uuid, category: &str, secs: i64) -> Command {
    Command::Expense(entry(
        amount,
        Some(wallet),
        Some(flow),
        Some(category),
        secs,
    ))
}

fn env(vault: Uuid, author: &str, command: Command) -> CommandEnvelope {
    CommandEnvelope::new(vault, author, command)
}

/// One round against a server core: push the whole outbox, fold the answer
/// back, pull from the watermark, fold that back.
fn sync(client: &mut Core, server: &mut Core, vault: Uuid) -> (SyncReport, SyncReport) {
    let request = client.push_request(vault, ALL).unwrap();
    let response = server.serve_push(vault, &request).unwrap();
    let pushed = client.apply_push_response(vault, &response).unwrap();
    let since = client.sync_state(vault).unwrap().last_server_seq;
    let pulled = client
        .integrate_pull(vault, &server.serve_pull(vault, since, ALL).unwrap())
        .unwrap();
    (pushed, pulled)
}

// ---------------------------------------------------------------------------
// Local atomicity
// ---------------------------------------------------------------------------

#[test]
fn an_empty_batch_writes_nothing() {
    let mut fx = setup();
    let before = fx.core.commands_since(fx.vault, 0).unwrap();
    assert!(fx.core.execute_batch(Vec::new()).unwrap().is_empty());
    assert_eq!(fx.core.commands_since(fx.vault, 0).unwrap(), before);
}

#[test]
fn a_failing_command_rolls_the_whole_batch_back() {
    let mut fx = setup();
    let log_before = fx.core.commands_since(fx.vault, 0).unwrap();
    let outbox_before = fx.core.sync_state(fx.vault).unwrap().outbox;

    let first = env(fx.vault, "alice", category("Casa"));
    let broken = env(
        fx.vault,
        "alice",
        Command::Expense(entry(0, Some(fx.wallet), None, Some("Casa"), T0)),
    );
    let first_id = first.id;
    let err = fx.core.execute_batch(vec![first, broken]).unwrap_err();

    // The error of the failing command comes back as it is.
    assert_eq!(
        err,
        DomainError::InvalidAmount("amount must be > 0".to_string())
    );
    // Nothing of the first command survived: no log row, no category.
    let log_after = fx.core.commands_since(fx.vault, 0).unwrap();
    assert_eq!(log_after, log_before);
    assert!(log_after.iter().all(|r| r.envelope.id != first_id));
    assert!(
        fx.core
            .categories(fx.vault, true)
            .unwrap()
            .iter()
            .all(|c| c.name != "Casa")
    );
    assert_eq!(fx.core.sync_state(fx.vault).unwrap().outbox, outbox_before);

    // The failed batch left no hole: the next one continues the seqs.
    let last = log_before.last().unwrap().seq;
    let receipts = fx
        .core
        .execute_batch(vec![env(fx.vault, "alice", category("Casa"))])
        .unwrap();
    assert_eq!(receipts[0].seq, last + 1);
}

#[test]
fn every_command_gets_its_own_seq_in_order() {
    let mut fx = setup();
    let last = fx.core.last_seq(fx.vault).unwrap();

    let batch = vec![
        env(fx.vault, "alice", category("Casa")),
        env(
            fx.vault,
            "alice",
            Command::Expense(entry(12_00, Some(fx.wallet), None, Some("Casa"), T0)),
        ),
        env(
            fx.vault,
            "alice",
            Command::Income(entry(5_00, Some(fx.wallet), None, None, T0 + 1)),
        ),
    ];
    let ids: Vec<Uuid> = batch.iter().map(|e| e.id).collect();
    let receipts = fx.core.execute_batch(batch).unwrap();

    assert_eq!(
        receipts.iter().map(|r| r.command_id).collect::<Vec<_>>(),
        ids
    );
    assert_eq!(
        receipts.iter().map(|r| r.seq).collect::<Vec<_>>(),
        vec![last + 1, last + 2, last + 3]
    );
    assert!(receipts.iter().all(|r| !r.deduplicated));
    // Every command created its entity, with the id of the command.
    assert_eq!(
        receipts.iter().map(|r| r.result_id).collect::<Vec<_>>(),
        ids.iter().copied().map(Some).collect::<Vec<_>>()
    );

    // Each is its own row of the log and of the outbox, in batch order.
    let outbox: Vec<Uuid> = fx
        .core
        .outbox(fx.vault)
        .unwrap()
        .into_iter()
        .map(|r| r.envelope.id)
        .collect();
    assert_eq!(outbox[outbox.len() - 3..], ids[..]);
    assert_eq!(list(&fx.core, fx.vault, &all()).len(), 2);
}

#[test]
fn a_known_command_id_is_deduplicated_inside_a_batch() {
    let mut fx = setup();
    let known = env(fx.vault, "alice", category("Casa"));
    let original = fx.core.execute(known.clone()).unwrap();

    let fresh = env(fx.vault, "alice", category("Auto"));
    let receipts = fx
        .core
        .execute_batch(vec![known.clone(), fresh.clone(), fresh.clone()])
        .unwrap();

    // The command already in the log answers with its original receipt.
    assert_eq!(receipts[0].seq, original.seq);
    assert_eq!(receipts[0].result_id, original.result_id);
    assert!(receipts[0].deduplicated);
    // A repeat inside the same batch is deduplicated against its first copy.
    assert!(!receipts[1].deduplicated);
    assert_eq!(receipts[1].seq, original.seq + 1);
    assert!(receipts[2].deduplicated);
    assert_eq!(receipts[2].seq, receipts[1].seq);
    assert_eq!(fx.core.last_seq(fx.vault).unwrap(), original.seq + 1);
}

#[test]
fn a_batch_across_vaults_is_refused() {
    let mut fx = setup();
    let other = fx
        .core
        .execute(CommandEnvelope::create_vault(
            "alice",
            "Other",
            Currency::Eur,
        ))
        .unwrap()
        .result_id
        .unwrap();
    let before = fx.core.last_seq(fx.vault).unwrap();

    let err = fx
        .core
        .execute_batch(vec![
            env(fx.vault, "alice", category("Casa")),
            env(other, "alice", category("Casa")),
        ])
        .unwrap_err();
    assert_eq!(err.code(), "invalid_command");
    assert_eq!(fx.core.last_seq(fx.vault).unwrap(), before);
    assert_eq!(fx.core.last_seq(other).unwrap(), 1);
}

#[test]
fn a_batch_can_create_its_own_vault() {
    let mut core = Core::open_in_memory().unwrap();
    let create = CommandEnvelope::create_vault("alice", "Casa", Currency::Eur);
    let vault = create.vault_id;
    let receipts = core
        .execute_batch(vec![
            create,
            env(vault, "alice", wallet_cmd("Cash", 10_00)),
            env(vault, "alice", category("Spesa")),
        ])
        .unwrap();
    assert_eq!(
        receipts.iter().map(|r| r.seq).collect::<Vec<_>>(),
        vec![1, 2, 3]
    );
    assert_eq!(core.vaults().unwrap().len(), 1);
    assert_eq!(core.snapshot(vault).unwrap().wallets[0].balance, 10_00);
}

// ---------------------------------------------------------------------------
// Sync: the batch goes up command by command
// ---------------------------------------------------------------------------

#[test]
fn batch_commands_are_pushed_and_rejected_one_by_one() {
    // Alice and Bob share a vault whose Vacanze envelope holds 50.00.
    let mut server = Core::open_in_memory().unwrap();
    let mut alice = Core::open_in_memory().unwrap();
    let vault = alice
        .execute(CommandEnvelope::create_vault(
            "alice",
            "Casa",
            Currency::Eur,
        ))
        .unwrap()
        .result_id
        .unwrap();
    let wallet = alice
        .execute(env(vault, "alice", wallet_cmd("Cash", 10_000)))
        .unwrap()
        .result_id
        .unwrap();
    let vacanze = alice
        .execute(env(
            vault,
            "alice",
            flow_cmd("Vacanze", FlowMode::Unlimited, false, 50_00),
        ))
        .unwrap()
        .result_id
        .unwrap();
    sync(&mut alice, &mut server, vault);
    let mut bob = Core::open_in_memory().unwrap();
    bob.integrate_pull(vault, &server.serve_pull(vault, 0, ALL).unwrap())
        .unwrap();

    // Bob spends 30.00 and pushes first.
    bob.execute(env(
        vault,
        "bob",
        spend(30_00, wallet, vacanze, "hotel", T0 + 60),
    ))
    .unwrap();
    sync(&mut bob, &mut server, vault);

    // Alice, still offline, files two spends in one batch: 40.00 fits her
    // local 50.00, but only the first fits what is left on the server.
    let fits = env(
        vault,
        "alice",
        spend(10_00, wallet, vacanze, "treno", T0 + 90),
    );
    let too_much = env(
        vault,
        "alice",
        spend(30_00, wallet, vacanze, "volo", T0 + 120),
    );
    let (fits_id, too_much_id) = (fits.id, too_much.id);
    alice.execute_batch(vec![fits, too_much]).unwrap();
    assert_eq!(alice.sync_state(vault).unwrap().outbox, 2);

    // The push body carries them as two commands, each answered on its own.
    let request = alice.push_request(vault, ALL).unwrap();
    assert_eq!(
        request.commands.iter().map(|e| e.id).collect::<Vec<_>>(),
        vec![fits_id, too_much_id]
    );
    let response = server.serve_push(vault, &request).unwrap();
    assert!(matches!(
        response.results[0].outcome,
        PushOutcome::Applied { .. }
    ));
    assert!(matches!(
        &response.results[1].outcome,
        PushOutcome::Rejected { code, .. } if code == "insufficient_funds"
    ));

    let report = alice.apply_push_response(vault, &response).unwrap();
    assert_eq!(report.confirmed, 1);
    assert!(report.rebased);
    assert_eq!(report.rejected.len(), 1);
    assert_eq!(report.rejected[0].command_id, too_much_id);
    assert_eq!(alice.sync_state(vault).unwrap().outbox, 0);
    assert_eq!(alice.sync_state(vault).unwrap().rejected, 1);

    // Once Bob's spend is pulled, Alice matches the server.
    let since = alice.sync_state(vault).unwrap().last_server_seq;
    alice
        .integrate_pull(vault, &server.serve_pull(vault, since, ALL).unwrap())
        .unwrap();
    let ids = |core: &Core| {
        let mut ids: Vec<Uuid> = list(core, vault, &all()).iter().map(|t| t.id).collect();
        ids.sort();
        ids
    };
    assert_eq!(ids(&alice), ids(&server));
    assert!(ids(&alice).contains(&fits_id));
    assert!(!ids(&alice).contains(&too_much_id));
    assert_eq!(
        alice.snapshot(vault).unwrap(),
        server.snapshot(vault).unwrap()
    );
}
