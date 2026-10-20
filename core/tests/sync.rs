//! Client-side sync: outbox, push results, pull integration, rebase.
//!
//! The server is a second in-memory [`Core`] answering through `serve_push`
//! and `serve_pull`. The real server has its own push loop, with author and
//! membership checks, so the scenarios that go through it are in
//! `server/tests`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use common::{
    T0, all, at, create_vault, entry, flow_cmd,
    sync::{ALL, exec, projection, pull, rename, spend, sync, vault_of},
    wallet_cmd,
};
use sparagne_core::{
    Command, CommandEnvelope, Core, DomainError, Entry, FlowMode, Frequency, Schedule, SyncReport,
    TransactionKind, TransactionPatch,
    sync::{PullResponse, PushOutcome, PushRequest, SyncRecord},
};
use uuid::Uuid;

/// The report of a sync round that changed nothing, carrying the server's
/// last seq as every report now does.
fn quiet(server_last_seq: i64) -> SyncReport {
    SyncReport {
        server_last_seq,
        ..SyncReport::default()
    }
}

fn unallocated(core: &Core, vault: Uuid) -> Uuid {
    core.snapshot(vault).unwrap().unallocated_flow_id
}

/// A vault with a wallet holding 100.00 and a `Vacanze` envelope holding 50.00,
/// created by `alice` and already pushed.
struct Shared {
    server: Core,
    alice: Core,
    vault: Uuid,
    wallet: Uuid,
    vacanze: Uuid,
}

fn shared() -> Shared {
    let mut server = Core::open_in_memory().unwrap();
    let (mut alice, vault) = vault_of("alice", "Casa");
    let wallet = exec(&mut alice, vault, "alice", wallet_cmd("Cash", 10_000));
    let vacanze = exec(
        &mut alice,
        vault,
        "alice",
        flow_cmd("Vacanze", FlowMode::Unlimited, false, 50_00),
    );
    sync(&mut alice, &mut server, vault);
    Shared {
        server,
        alice,
        vault,
        wallet,
        vacanze,
    }
}

/// A second client that has joined the vault by pulling from zero.
fn join(server: &Core, vault: Uuid) -> Core {
    let mut core = Core::open_in_memory().unwrap();
    let report = core.integrate_pull(vault, &pull(server, vault, 0)).unwrap();
    assert!(report.rebased);
    assert!(report.received > 0);
    core
}

/// `(id, name, owner)` of every vault, in the order `vaults()` lists them.
fn listing(core: &Core) -> Vec<(Uuid, String, String)> {
    core.vaults()
        .unwrap()
        .into_iter()
        .map(|v| (v.id, v.name, v.owner))
        .collect()
}

#[test]
fn outbox_holds_the_unconfirmed_commands_in_local_order() {
    let mut server = Core::open_in_memory().unwrap();
    let (mut alice, vault) = vault_of("alice", "Casa");
    let wallet = exec(&mut alice, vault, "alice", wallet_cmd("Cash", 10_000));
    let unallocated = unallocated(&alice, vault);
    exec(
        &mut alice,
        vault,
        "alice",
        spend(10_00, wallet, unallocated, "food", T0),
    );

    let outbox = alice.outbox(vault).unwrap();
    assert_eq!(outbox.len(), 3);
    assert_eq!(
        outbox.iter().map(|r| r.seq).collect::<Vec<_>>(),
        vec![1, 2, 3]
    );
    assert!(outbox.iter().all(|r| r.server_seq.is_none()));
    assert_eq!(
        outbox
            .iter()
            .map(|r| r.envelope.command.kind_name())
            .collect::<Vec<_>>(),
        vec!["create_vault", "create_wallet", "expense"]
    );

    let request = alice.push_request(vault, ALL).unwrap();
    assert_eq!(
        request.commands.iter().map(|e| e.id).collect::<Vec<_>>(),
        outbox.iter().map(|r| r.envelope.id).collect::<Vec<_>>()
    );

    let state = alice.sync_state(vault).unwrap();
    assert_eq!(state.last_server_seq, 0);
    assert_eq!(state.outbox, 3);
    assert_eq!(state.rejected, 0);

    // Once confirmed the outbox drains and the watermark moves.
    let response = server.serve_push(vault, &request).unwrap();
    let report = alice.apply_push_response(vault, &response).unwrap();
    assert_eq!(report.confirmed, 3);
    assert!(!report.rebased);
    assert!(report.rejected.is_empty());
    assert_eq!(alice.sync_state(vault).unwrap().outbox, 0);
    assert_eq!(alice.sync_state(vault).unwrap().last_server_seq, 3);
    assert!(alice.outbox(vault).unwrap().is_empty());
}

#[test]
fn a_pull_of_ones_own_commands_takes_the_fast_path() {
    let mut server = Core::open_in_memory().unwrap();
    let (mut alice, vault) = vault_of("alice", "Casa");
    exec(&mut alice, vault, "alice", wallet_cmd("Cash", 10_000));

    // The push goes through but its response never comes back, so the pull is
    // what confirms the commands.
    let request = alice.push_request(vault, ALL).unwrap();
    let _lost = server.serve_push(vault, &request).unwrap();

    let before = projection(&alice, vault);
    let report = alice
        .integrate_pull(vault, &pull(&server, vault, 0))
        .unwrap();
    assert_eq!(report.confirmed, 2);
    assert_eq!(report.received, 0);
    assert!(!report.rebased, "own commands must not force a rebase");
    assert!(report.rejected.is_empty());

    assert_eq!(projection(&alice, vault), before);
    assert_eq!(
        alice
            .commands_since(vault, 0)
            .unwrap()
            .iter()
            .map(|r| r.server_seq)
            .collect::<Vec<_>>(),
        vec![Some(1), Some(2)]
    );
    assert_eq!(alice.sync_state(vault).unwrap().last_server_seq, 2);
}

#[test]
fn integrate_pull_ignores_records_at_or_below_the_watermark() {
    let Shared {
        mut server,
        mut alice,
        vault,
        ..
    } = shared();
    let before = projection(&alice, vault);
    assert_eq!(alice.sync_state(vault).unwrap().last_server_seq, 3);

    // A full pull from zero: every record is already known.
    let report = alice
        .integrate_pull(vault, &pull(&server, vault, 0))
        .unwrap();
    assert_eq!(report, quiet(3));
    assert_eq!(projection(&alice, vault), before);

    // And so is a pull from the watermark.
    let report = alice
        .integrate_pull(vault, &pull(&server, vault, 3))
        .unwrap();
    assert_eq!(report, quiet(3));
    assert_eq!(projection(&alice, vault), before);

    // Nothing reached the server either.
    server
        .serve_push(vault, &PushRequest { commands: vec![] })
        .unwrap();
    assert_eq!(server.last_seq(vault).unwrap(), 3);
}

#[test]
fn two_clients_converge_on_the_server_projection() {
    let Shared {
        mut server,
        mut alice,
        vault,
        wallet,
        vacanze,
    } = shared();

    // Alice fills the vault out: a second wallet, a transfer, a category with
    // an alias and a spend.
    let bank = exec(&mut alice, vault, "alice", wallet_cmd("Banca", 0));
    exec(
        &mut alice,
        vault,
        "alice",
        Command::TransferWallet {
            amount: 20_00,
            from_wallet_id: wallet,
            to_wallet_id: bank,
            note: Some("giroconto".to_string()),
            occurred_at: at(T0 + 60),
        },
    );
    let travel = exec(
        &mut alice,
        vault,
        "alice",
        Command::CreateCategory {
            name: "Viaggi".to_string(),
        },
    );
    exec(
        &mut alice,
        vault,
        "alice",
        Command::AddAlias {
            category_id: travel,
            alias: "trip".to_string(),
        },
    );
    let food = exec(
        &mut alice,
        vault,
        "alice",
        spend(10_00, wallet, vacanze, "cibo", T0 + 120),
    );
    sync(&mut alice, &mut server, vault);

    // Bob joins by pulling the whole log.
    let mut bob = join(&server, vault);
    assert_eq!(projection(&bob, vault), projection(&alice, vault));
    assert_eq!(bob.sync_state(vault).unwrap().last_server_seq, 8);

    // Both write before either syncs.
    exec(
        &mut bob,
        vault,
        "bob",
        spend(5_00, wallet, vacanze, "hotel", T0 + 180),
    );
    exec(
        &mut bob,
        vault,
        "bob",
        Command::VoidTransaction {
            transaction_id: food,
        },
    );
    exec(
        &mut bob,
        vault,
        "bob",
        Command::CreateRecurring {
            transaction_kind: TransactionKind::Expense,
            amount: 49_00,
            wallet_id: Some(wallet),
            flow_id: None,
            category: Some("affitto".to_string()),
            note: Some("casa".to_string()),
            schedule: Schedule {
                frequency: Frequency::Monthly { day: 5 },
                interval: 1,
                start_date: at(T0).date_naive(),
                end_date: None,
            },
            owner: None,
        },
    );
    exec(
        &mut alice,
        vault,
        "alice",
        spend(7_00, wallet, vacanze, "taxi", T0 + 240),
    );

    // Bob pushes first, so Alice's command lands after his on the server.
    let (bob_push, bob_pull) = sync(&mut bob, &mut server, vault);
    assert_eq!(bob_push.confirmed, 3);
    assert_eq!(bob_pull, quiet(server.last_seq(vault).unwrap()));

    let (alice_push, alice_pull) = sync(&mut alice, &mut server, vault);
    assert_eq!(alice_push.confirmed, 1);
    assert!(!alice_push.rebased);
    // The push confirmed seq 12 while 9-11 are missing, so the watermark stays
    // at 8 and the pull brings the hole back and rebases.
    assert!(alice_pull.rebased, "Alice must rebase under Bob's commands");
    assert_eq!(alice_pull.received, 3);
    assert!(alice_pull.rejected.is_empty());

    // Bob pulls Alice's command and rebases in turn.
    let (_, second) = sync(&mut bob, &mut server, vault);
    assert!(second.rebased);
    assert_eq!(second.received, 1);

    // Entity ids derive from the command ids, so the views (ids included)
    // are equal on every core.
    let expected = projection(&server, vault);
    assert_eq!(projection(&alice, vault), expected);
    assert_eq!(projection(&bob, vault), expected);

    // The rebase really moved money: 50.00 allocated, 10.00 voided back,
    // 5.00 and 7.00 spent.
    let vacanze_balance = expected
        .0
        .flows
        .iter()
        .find(|flow| flow.id == vacanze)
        .unwrap()
        .balance;
    assert_eq!(vacanze_balance, 50_00 - 5_00 - 7_00);
    assert!(
        expected.1.iter().any(|t| t.voided && t.id == food),
        "the voided expense is still listed"
    );
    assert!(expected.1.iter().any(|t| t.kind.is_transfer()));
    assert_eq!(expected.4.len(), 1, "Bob's template reached everyone");
    assert_eq!(expected.3.len(), 1, "the alias survived the rebase");

    // No outbox and no gap left anywhere.
    for core in [&alice, &bob] {
        let state = core.sync_state(vault).unwrap();
        assert_eq!(state.outbox, 0);
        assert_eq!(state.rejected, 0);
        assert_eq!(state.last_server_seq, server.last_seq(vault).unwrap());
    }
}

#[test]
fn an_outbox_command_that_no_longer_applies_is_rejected_by_the_rebase() {
    let Shared {
        mut server,
        mut alice,
        vault,
        wallet,
        vacanze,
    } = shared();
    let mut bob = join(&server, vault);

    // Alice empties Vacanze and pushes; Bob's unpushed spend cannot survive
    // being replayed after hers.
    exec(
        &mut alice,
        vault,
        "alice",
        spend(50_00, wallet, vacanze, "volo", T0 + 60),
    );
    sync(&mut alice, &mut server, vault);

    let doomed = exec(
        &mut bob,
        vault,
        "bob",
        spend(30_00, wallet, vacanze, "hotel", T0 + 120),
    );
    // Bob pulls before pushing, so the rebase is what refuses the command.
    let since = bob.sync_state(vault).unwrap().last_server_seq;
    let report = bob
        .integrate_pull(vault, &pull(&server, vault, since))
        .unwrap();

    assert!(report.rebased);
    assert_eq!(report.received, 1);
    assert_eq!(report.confirmed, 0);
    assert_eq!(report.rejected.len(), 1);
    let rejection = &report.rejected[0];
    assert_eq!(rejection.command_id, doomed);
    assert_eq!(rejection.kind, "expense");
    assert_eq!(rejection.code, "insufficient_funds");
    assert!(rejection.message.contains("Vacanze"));

    assert_eq!(bob.sync_state(vault).unwrap().outbox, 0);
    assert!(bob.push_request(vault, ALL).unwrap().commands.is_empty());
    assert_eq!(projection(&bob, vault), projection(&server, vault));
    // A rejected row is out of the log the server would ever see, and waits
    // for the UI until it is dismissed.
    assert!(
        bob.commands_since(vault, 0)
            .unwrap()
            .iter()
            .all(|r| r.envelope.id != doomed)
    );
    assert_eq!(bob.sync_state(vault).unwrap().rejected, 1);
    assert_eq!(bob.rejected_commands(vault).unwrap(), report.rejected);
    bob.dismiss_rejected(vault, doomed).unwrap();
    assert!(bob.rejected_commands(vault).unwrap().is_empty());
    assert_eq!(bob.sync_state(vault).unwrap().rejected, 0);
    // Dismissing twice is quiet.
    bob.dismiss_rejected(vault, doomed).unwrap();
}

#[test]
fn a_confirmed_command_that_cannot_be_replayed_is_a_divergence() {
    let Shared {
        server,
        mut alice,
        vault,
        wallet,
        vacanze,
    } = shared();
    let before = projection(&alice, vault);
    let log_before = alice.commands_since(vault, 0).unwrap();

    // A forged record: an expense on an envelope that cannot fund it, handed
    // over as if the server had accepted it.
    let forged = CommandEnvelope::new(
        vault,
        "mallory",
        Command::Expense(Entry {
            amount: 50_000,
            wallet_id: Some(wallet),
            flow_id: Some(vacanze),
            category: Some("truffa".to_string()),
            note: None,
            occurred_at: at(T0 + 60),
            person: None,
        }),
    );
    let response = PullResponse {
        commands: vec![SyncRecord {
            envelope: forged,
            seq: 4,
            result_id: None,
            created_at: T0,
        }],
        last_seq: 4,
    };

    let err = alice.integrate_pull(vault, &response).unwrap_err();
    assert_eq!(err.code(), "storage_error");
    assert!(
        matches!(&err, DomainError::Storage(message) if message.starts_with("sync divergence:")),
        "unexpected error: {err}"
    );

    // Nothing moved: the whole integration is one transaction.
    assert_eq!(projection(&alice, vault), before);
    assert_eq!(alice.commands_since(vault, 0).unwrap(), log_before);
    assert_eq!(alice.sync_state(vault).unwrap().last_server_seq, 3);
    assert_eq!(server.last_seq(vault).unwrap(), 3);
}

#[test]
fn a_pull_for_another_vault_is_refused() {
    let Shared {
        server,
        mut alice,
        vault,
        ..
    } = shared();
    let response = PullResponse {
        commands: pull(&server, vault, 0)
            .commands
            .into_iter()
            .map(|mut record| {
                record.envelope.vault_id = Uuid::now_v7();
                record.seq += 100;
                record
            })
            .collect(),
        last_seq: 103,
    };
    let err = alice.integrate_pull(vault, &response).unwrap_err();
    assert_eq!(err.code(), "invalid_command");
}

#[test]
fn relabel_outbox_rewrites_the_author_of_what_is_not_yet_pushed() {
    let mut server = Core::open_in_memory().unwrap();
    // The app starts logged out: commands carry a local placeholder author.
    let (mut core, vault) = vault_of("local", "Casa");
    let wallet = exec(&mut core, vault, "local", wallet_cmd("Cash", 10_000));
    let unallocated = unallocated(&core, vault);
    exec(
        &mut core,
        vault,
        "local",
        spend(10_00, wallet, unallocated, "cibo", T0),
    );
    let before = core
        .list_transactions(vault, &all(), 50, None)
        .unwrap()
        .items;
    assert!(before.iter().all(|t| t.created_by == "local"));
    assert_eq!(core.vaults().unwrap()[0].owner, "local");

    core.relabel_outbox(vault, "alice").unwrap();

    let after = core
        .list_transactions(vault, &all(), 50, None)
        .unwrap()
        .items;
    assert!(after.iter().all(|t| t.created_by == "alice"));
    assert_eq!(
        after.iter().map(|t| t.id).collect::<Vec<_>>(),
        before.iter().map(|t| t.id).collect::<Vec<_>>(),
        "ids come from the command, so relabelling does not move them"
    );
    assert_eq!(core.vaults().unwrap()[0].owner, "alice");
    assert!(
        core.outbox(vault)
            .unwrap()
            .iter()
            .all(|r| r.envelope.author == "alice")
    );

    // The relabelled log is what the server gets, and both agree.
    sync(&mut core, &mut server, vault);
    assert_eq!(projection(&core, vault), projection(&server, vault));

    // Confirmed commands are not touched by a later relabel.
    core.relabel_outbox(vault, "bruno").unwrap();
    assert!(
        core.list_transactions(vault, &all(), 50, None)
            .unwrap()
            .items
            .iter()
            .all(|t| t.created_by == "alice")
    );
}

#[test]
fn relabelling_renames_the_people_the_outbox_named_by_the_local_name() {
    let mut server = Core::open_in_memory().unwrap();
    let (mut core, vault) = vault_of("local", "Casa");
    let wallet = exec(&mut core, vault, "local", wallet_cmd("Cash", 10_000));
    let flow = unallocated(&core, vault);
    let for_whom = |person: &str, amount: i64| {
        Command::Expense(Entry {
            person: Some(person.to_string()),
            ..entry(amount, Some(wallet), Some(flow), Some("cibo"), T0)
        })
    };
    // Logged out, the app offered the local name and somebody else's.
    let mine = exec(&mut core, vault, "local", for_whom("local", 10_00));
    let theirs = exec(&mut core, vault, "local", for_whom("elisa", 20_00));
    // A patch that only moves the person to the local name: it must keep
    // something to carry once relabelled.
    exec(
        &mut core,
        vault,
        "local",
        Command::UpdateTransaction {
            transaction_id: theirs,
            patch: TransactionPatch {
                person: Some(" local ".to_string()),
                ..TransactionPatch::default()
            },
        },
    );
    let template = exec(
        &mut core,
        vault,
        "local",
        Command::CreateRecurring {
            transaction_kind: TransactionKind::Expense,
            amount: 5_00,
            wallet_id: Some(wallet),
            flow_id: None,
            category: None,
            note: None,
            schedule: Schedule {
                frequency: Frequency::Monthly { day: 1 },
                interval: 1,
                start_date: chrono::NaiveDate::from_ymd_opt(2023, 11, 1).unwrap(),
                end_date: None,
            },
            owner: Some("local".to_string()),
        },
    );
    let paid = exec(
        &mut core,
        vault,
        "local",
        Command::ExecuteRecurring {
            recurring_id: template,
            period_date: chrono::NaiveDate::from_ymd_opt(2023, 11, 1).unwrap(),
            occurred_at: at(T0),
            person: Some("local".to_string()),
        },
    );

    core.relabel_outbox(vault, "alice").unwrap();

    let outbox = core.outbox(vault).unwrap();
    assert!(outbox.iter().all(|r| r.envelope.author == "alice"));
    let named: Vec<&str> = outbox
        .iter()
        .flat_map(|r| r.envelope.command.named_people())
        .collect();
    assert_eq!(named, ["alice", "elisa", "alice", "alice", "alice"]);
    let patch = outbox
        .iter()
        .find_map(|r| match &r.envelope.command {
            Command::UpdateTransaction { patch, .. } => Some(patch.clone()),
            _ => None,
        })
        .unwrap();
    assert!(!patch.is_empty());
    assert_eq!(patch.person.as_deref(), Some("alice"));

    let person = |id: Uuid| core.transaction(vault, id).unwrap().person;
    assert_eq!(person(mine), "alice");
    assert_eq!(person(theirs), "alice");
    assert_eq!(person(paid), "alice");
    assert_eq!(core.list_recurring(vault, true).unwrap()[0].owner, "alice");

    // The server takes the relabelled log and both sides agree.
    sync(&mut core, &mut server, vault);
    assert_eq!(core.sync_state(vault).unwrap().rejected, 0);
    assert_eq!(projection(&core, vault), projection(&server, vault));
}

#[test]
fn the_json_entry_points_round_trip_through_the_wire_types() {
    let mut server = Core::open_in_memory().unwrap();
    let (mut alice, vault) = vault_of("alice", "Casa");
    exec(&mut alice, vault, "alice", wallet_cmd("Cash", 10_000));

    let body = alice.push_request_json(vault, ALL).unwrap();
    let request: PushRequest = serde_json::from_str(&body).unwrap();
    assert_eq!(request.commands.len(), 2);

    let response = serde_json::to_string(&server.serve_push(vault, &request).unwrap()).unwrap();
    let report = alice.apply_push_response_json(vault, &response).unwrap();
    assert_eq!(report.confirmed, 2);

    let pulled = serde_json::to_string(&pull(&server, vault, 0)).unwrap();
    assert_eq!(alice.integrate_pull_json(vault, &pulled).unwrap(), quiet(2));

    for bad in ["", "{", "{\"nope\":1}"] {
        assert_eq!(
            alice
                .apply_push_response_json(vault, bad)
                .unwrap_err()
                .code(),
            "invalid_command"
        );
        assert_eq!(
            alice.integrate_pull_json(vault, bad).unwrap_err().code(),
            "invalid_command"
        );
    }
}

#[test]
fn an_unknown_vault_reports_an_empty_sync_state() {
    let core = Core::open_in_memory().unwrap();
    let state = core.sync_state(Uuid::now_v7()).unwrap();
    assert_eq!(state.last_server_seq, 0);
    assert_eq!(state.outbox, 0);
    assert_eq!(state.rejected, 0);
    assert_eq!(core.last_seq(Uuid::now_v7()).unwrap(), 0);
}

#[test]
fn a_long_outbox_goes_up_in_batches_and_converges() {
    const BATCH: usize = 500;
    const TOTAL: i64 = 1_200;

    let mut server = Core::open_in_memory().unwrap();
    let (mut alice, vault) = vault_of("alice", "Casa");
    let wallet = exec(&mut alice, vault, "alice", wallet_cmd("Cash", 1_000_000));
    let unallocated = unallocated(&alice, vault);
    // Two commands are already in: fill the outbox up to TOTAL.
    for i in 0..(TOTAL - 2) {
        exec(
            &mut alice,
            vault,
            "alice",
            spend(1_00, wallet, unallocated, "food", T0 + i),
        );
    }
    assert_eq!(
        alice.sync_state(vault).unwrap().outbox,
        u32::try_from(TOTAL).unwrap()
    );

    // What the app does: push a batch, fold the answer back, repeat while the
    // outbox is not empty.
    let mut rounds = 0;
    while alice.sync_state(vault).unwrap().outbox > 0 {
        rounds += 1;
        assert!(rounds <= 10, "the push loop must converge");
        let request = alice.push_request(vault, BATCH).unwrap();
        assert!(request.commands.len() <= BATCH);
        let response = server.serve_push(vault, &request).unwrap();
        alice.apply_push_response(vault, &response).unwrap();
    }

    assert_eq!(rounds, 3, "1200 commands in batches of 500");
    assert_eq!(server.last_seq(vault).unwrap(), TOTAL);
    assert_eq!(alice.sync_state(vault).unwrap().last_server_seq, TOTAL);
    assert_eq!(projection(&alice, vault), projection(&server, vault));
}

#[test]
fn a_report_carries_the_servers_last_seq_and_whether_more_is_waiting() {
    let Shared { server, vault, .. } = shared();
    let mut bob = Core::open_in_memory().unwrap();

    // One record per page: the first two leave something behind.
    for since in 0..2 {
        let report = bob
            .integrate_pull(vault, &server.serve_pull(vault, since, 1).unwrap())
            .unwrap();
        assert_eq!(report.server_last_seq, 3);
        assert!(report.has_more, "page from {since} is not the last");
    }
    let last = bob
        .integrate_pull(vault, &server.serve_pull(vault, 2, 1).unwrap())
        .unwrap();
    assert_eq!(last.server_last_seq, 3);
    assert!(!last.has_more);
    assert_eq!(projection(&bob, vault), projection(&server, vault));
}

#[test]
fn the_server_reads_one_vault_and_every_last_seq_at_once() {
    let Shared {
        mut server, vault, ..
    } = shared();
    let casa = server.vault(vault).unwrap().expect("Casa");
    assert_eq!(casa.id, vault);
    assert_eq!(casa.name, "Casa");
    assert_eq!(casa.owner, "alice");
    assert_eq!(server.vault(Uuid::now_v7()).unwrap(), None);

    // A second vault, with a log of its own.
    let other = create_vault(&mut server, "bob", "Ufficio");
    let mut seqs = server.last_seqs().unwrap();
    seqs.sort_by_key(|(_, seq)| *seq);
    assert_eq!(seqs, vec![(other, 1), (vault, 3)]);
}

#[test]
fn a_deleted_vault_still_pushes_and_the_server_forgets_it() {
    let Shared {
        mut server,
        mut alice,
        vault,
        ..
    } = shared();

    exec(&mut alice, vault, "alice", Command::DeleteVault);
    assert!(alice.vaults().unwrap().is_empty());
    assert_eq!(alice.deleted_vaults().unwrap(), vec![vault]);
    assert_eq!(alice.sync_state(vault).unwrap().outbox, 1);
    // The push body is built from the log, which the deletion left intact.
    assert_eq!(alice.push_request(vault, ALL).unwrap().commands.len(), 1);

    let (pushed, pulled) = sync(&mut alice, &mut server, vault);
    assert_eq!(pushed.confirmed, 1);
    assert!(pushed.rejected.is_empty());
    assert!(!pulled.has_more);
    assert_eq!(alice.sync_state(vault).unwrap().outbox, 0);
    assert_eq!(alice.sync_state(vault).unwrap().last_server_seq, 4);

    // The server applied the same command with the same outcome.
    assert_eq!(server.vault(vault).unwrap(), None);
    assert_eq!(server.last_seq(vault).unwrap(), 4);
    assert_eq!(server.deleted_vaults().unwrap(), vec![vault]);
    // And refuses whatever comes after.
    let late = server
        .serve_push(
            vault,
            &PushRequest {
                commands: vec![CommandEnvelope::new(vault, "alice", wallet_cmd("Late", 0))],
            },
        )
        .unwrap();
    assert!(matches!(
        &late.results[0].outcome,
        PushOutcome::Rejected { code, .. } if code == "not_found"
    ));
}

#[test]
fn relabelling_covers_a_deleted_vault_too() {
    // Offline, "oghma" creates a vault and deletes it; then logs in as alice.
    // The dead log must be re-signed like a live one, or the push would be
    // refused with `author_mismatch` for ever.
    let (mut core, vault) = vault_of("oghma", "Vecchio");
    exec(&mut core, vault, "oghma", wallet_cmd("Cash", 1_000));
    exec(&mut core, vault, "oghma", Command::DeleteVault);
    assert!(core.vaults().unwrap().is_empty());

    core.relabel_outbox(vault, "alice").unwrap();

    let outbox = core.outbox(vault).unwrap();
    assert_eq!(outbox.len(), 3);
    assert!(
        outbox
            .iter()
            .all(|record| record.envelope.author == "alice")
    );
    // Relabelling replays the log, and the replay ends where it started:
    // the owner is now alice, so her deletion still applies.
    assert_eq!(core.vault(vault).unwrap(), None);
    assert_eq!(core.deleted_vaults().unwrap(), vec![vault]);
    assert_eq!(core.sync_state(vault).unwrap().rejected, 0);

    let mut server = Core::open_in_memory().unwrap();
    let (pushed, _) = sync(&mut core, &mut server, vault);
    assert_eq!(pushed.confirmed, 3);
    assert_eq!(server.vault(vault).unwrap(), None);
}

// A rebuild replays a vault's whole log, its `CreateVault` included, next to
// whatever other vaults the device holds by then. While names had to be unique
// per owner, a name another vault had taken in the meantime made that replay
// fail for ever.

#[test]
fn two_vaults_swap_names_and_a_member_follows() {
    let Shared {
        mut server,
        mut alice,
        vault: casa,
        ..
    } = shared();
    let mare = create_vault(&mut alice, "alice", "Mare");
    sync(&mut alice, &mut server, mare);
    let mut bob = join(&server, casa);
    bob.integrate_pull(mare, &pull(&server, mare, 0)).unwrap();

    exec(&mut alice, casa, "alice", rename("tmp"));
    exec(&mut alice, mare, "alice", rename("Casa"));
    exec(&mut alice, casa, "alice", rename("Mare"));
    sync(&mut alice, &mut server, casa);
    sync(&mut alice, &mut server, mare);

    // bob follows in the order a listing by name gives: the vault now called
    // Casa first, while his copy of the other one still has that name too.
    for id in [mare, casa] {
        let (_, pulled) = sync(&mut bob, &mut server, id);
        assert!(pulled.rejected.is_empty());
        assert_eq!(projection(&bob, id), projection(&server, id));
    }
    let swapped = vec![
        (mare, "Casa".to_string(), "alice".to_string()),
        (casa, "Mare".to_string(), "alice".to_string()),
    ];
    assert_eq!(listing(&bob), swapped);
    assert_eq!(listing(&alice), swapped);
}

#[test]
fn relabel_keeps_a_renamed_never_pushed_vault() {
    // Logged out, the app creates Casa, renames it Mare, then creates a new
    // Casa; none of it has reached a server.
    let (mut core, old) = vault_of("local", "Casa");
    exec(&mut core, old, "local", wallet_cmd("Cash", 1_000));
    exec(&mut core, old, "local", rename("Mare"));
    let new = create_vault(&mut core, "local", "Casa");

    // At login every vault is relabelled in listing order: the new Casa
    // first, so the old one's replay finds alice already owning a Casa.
    let order: Vec<Uuid> = listing(&core).into_iter().map(|(id, ..)| id).collect();
    assert_eq!(order, vec![new, old]);
    for id in order {
        core.relabel_outbox(id, "alice").unwrap();
    }

    for id in [new, old] {
        assert_eq!(core.sync_state(id).unwrap().rejected, 0);
        assert!(
            core.outbox(id)
                .unwrap()
                .iter()
                .all(|record| record.envelope.author == "alice")
        );
    }
    assert_eq!(
        listing(&core),
        vec![
            (new, "Casa".to_string(), "alice".to_string()),
            (old, "Mare".to_string(), "alice".to_string()),
        ]
    );
    assert_eq!(core.snapshot(old).unwrap().wallets.len(), 1);

    // And the server takes both logs, in the same order.
    let mut server = Core::open_in_memory().unwrap();
    for id in [new, old] {
        let (pushed, _) = sync(&mut core, &mut server, id);
        assert!(pushed.rejected.is_empty());
        assert_eq!(projection(&core, id), projection(&server, id));
    }
}
