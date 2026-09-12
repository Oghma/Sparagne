//! Client-side sync: outbox, push results, pull integration, rebase.
//!
//! Every test runs against a [`FakeServer`], a third in-memory [`Core`] that
//! plays the part `docs/v2/SYNC.md` §3 gives the real server: apply a push in
//! order with `execute`, hand out the log from a seq.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use common::{T0, all, at, entry, flow_cmd, wallet_cmd};
use sparagne_core::{
    AliasView, CategoryView, Command, CommandEnvelope, Core, Currency, DomainError, Entry,
    FlowMode, Frequency, RecurringView, Schedule, SyncReport, TransactionView, VaultSnapshot,
    sync::{PullResponse, PushOutcome, PushRequest, PushResponse, PushResult, SyncRecord},
};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

/// A push limit that is no limit: the whole outbox in one body.
const ALL: usize = usize::MAX;

/// The report of a sync round that changed nothing, carrying the server's
/// last seq as every report now does.
fn quiet(server_last_seq: i64) -> SyncReport {
    SyncReport {
        server_last_seq,
        ..SyncReport::default()
    }
}

/// The server of `SYNC.md` §3, minus HTTP and permissions.
struct FakeServer {
    core: Core,
}

impl FakeServer {
    fn new() -> Self {
        Self {
            core: Core::open_in_memory().unwrap(),
        }
    }

    /// Applies a batch in order. A refusal does not stop the batch, and a
    /// command already in the log answers with its original seq.
    fn push(&mut self, vault: Uuid, request: PushRequest) -> PushResponse {
        let results = request
            .commands
            .into_iter()
            .map(|envelope| {
                let command_id = envelope.id;
                let outcome = match self.core.execute(envelope) {
                    Ok(receipt) => PushOutcome::Applied {
                        seq: receipt.seq,
                        result_id: receipt.result_id,
                    },
                    Err(err) => PushOutcome::Rejected {
                        code: err.code().to_string(),
                        message: err.to_string(),
                    },
                };
                PushResult {
                    command_id,
                    outcome,
                }
            })
            .collect();
        PushResponse {
            results,
            last_seq: self.core.last_seq(vault).unwrap(),
        }
    }

    /// A capped page, the way `GET /pull?limit=` answers.
    fn pull_page(&self, vault: Uuid, since: i64, limit: usize) -> PullResponse {
        self.core.serve_pull(vault, since, limit).unwrap()
    }

    fn pull(&self, vault: Uuid, since: i64) -> PullResponse {
        PullResponse {
            commands: self
                .core
                .commands_since(vault, since)
                .unwrap()
                .into_iter()
                .map(|record| SyncRecord {
                    envelope: record.envelope,
                    seq: record.seq,
                    result_id: record.result_id,
                    created_at: record.created_at,
                })
                .collect(),
            last_seq: self.core.last_seq(vault).unwrap(),
        }
    }
}

/// One round of what the app does: push, fold the response back, pull from the
/// resulting watermark, fold that back.
fn sync(client: &mut Core, server: &mut FakeServer, vault: Uuid) -> (SyncReport, SyncReport) {
    let request = client.push_request(vault, ALL).unwrap();
    let response = server.push(vault, request);
    let pushed = client.apply_push_response(vault, &response).unwrap();
    let since = client.sync_state(vault).unwrap().last_server_seq;
    let pulled = client
        .integrate_pull(vault, &server.pull(vault, since))
        .unwrap();
    (pushed, pulled)
}

/// Everything a user can see of a vault, in one comparable value.
type Projection = (
    VaultSnapshot,
    Vec<TransactionView>,
    Vec<CategoryView>,
    Vec<AliasView>,
    Vec<RecurringView>,
);

fn projection(core: &Core, vault: Uuid) -> Projection {
    (
        core.snapshot(vault).unwrap(),
        core.list_transactions(vault, &all(), 200, None)
            .unwrap()
            .items,
        core.categories(vault, true).unwrap(),
        core.aliases(vault).unwrap(),
        core.list_recurring(vault, true).unwrap(),
    )
}

fn exec(core: &mut Core, vault: Uuid, author: &str, command: Command) -> Uuid {
    core.execute(CommandEnvelope::new(vault, author, command))
        .unwrap()
        .result_id
        .unwrap_or(vault)
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

fn unallocated(core: &Core, vault: Uuid) -> Uuid {
    core.snapshot(vault).unwrap().unallocated_flow_id
}

/// A vault with a wallet holding 100.00 and a `Vacanze` envelope holding 50.00,
/// created by `alice` and already pushed.
struct Shared {
    server: FakeServer,
    alice: Core,
    vault: Uuid,
    wallet: Uuid,
    vacanze: Uuid,
}

fn shared() -> Shared {
    let mut server = FakeServer::new();
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
fn join(server: &FakeServer, vault: Uuid) -> Core {
    let mut core = Core::open_in_memory().unwrap();
    let report = core.integrate_pull(vault, &server.pull(vault, 0)).unwrap();
    assert!(report.rebased);
    assert!(report.received > 0);
    core
}

// ---------------------------------------------------------------------------
// Outbox and push
// ---------------------------------------------------------------------------

#[test]
fn outbox_holds_the_unconfirmed_commands_in_local_order() {
    let mut server = FakeServer::new();
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
    let response = server.push(vault, request);
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
    let mut server = FakeServer::new();
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
    exec(&mut alice, vault, "alice", wallet_cmd("Cash", 10_000));

    // The push goes through but its response never comes back, so the pull is
    // what confirms the commands.
    let request = alice.push_request(vault, ALL).unwrap();
    let _lost = server.push(vault, request);

    let before = projection(&alice, vault);
    let report = alice.integrate_pull(vault, &server.pull(vault, 0)).unwrap();
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
fn pushing_twice_is_idempotent() {
    let mut server = FakeServer::new();
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
    exec(&mut alice, vault, "alice", wallet_cmd("Cash", 10_000));

    let request = alice.push_request(vault, ALL).unwrap();
    let first = server.push(vault, request.clone());
    let second = server.push(vault, request);
    assert_eq!(first, second, "a replayed push answers the same seqs");

    let report = alice.apply_push_response(vault, &first).unwrap();
    assert_eq!(report.confirmed, 2);
    // The second response confirms nothing more and changes nothing.
    let again = alice.apply_push_response(vault, &second).unwrap();
    assert_eq!(again, quiet(2));
    assert_eq!(alice.sync_state(vault).unwrap().last_server_seq, 2);
    assert_eq!(server.core.last_seq(vault).unwrap(), 2);
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
    let report = alice.integrate_pull(vault, &server.pull(vault, 0)).unwrap();
    assert_eq!(report, quiet(3));
    assert_eq!(projection(&alice, vault), before);

    // And so is a pull from the watermark.
    let report = alice.integrate_pull(vault, &server.pull(vault, 3)).unwrap();
    assert_eq!(report, quiet(3));
    assert_eq!(projection(&alice, vault), before);

    // Nothing reached the server either.
    let _ = server.push(vault, PushRequest { commands: vec![] });
    assert_eq!(server.core.last_seq(vault).unwrap(), 3);
}

// ---------------------------------------------------------------------------
// Two clients
// ---------------------------------------------------------------------------

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
            transaction_kind: sparagne_core::TransactionKind::Expense,
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
    assert_eq!(bob_pull, quiet(server.core.last_seq(vault).unwrap()));

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

    let expected = projection(&server.core, vault);
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
        assert_eq!(state.last_server_seq, server.core.last_seq(vault).unwrap());
    }
}

#[test]
fn entity_ids_are_the_same_on_every_core() {
    let Shared {
        mut server,
        mut alice,
        vault,
        wallet,
        vacanze,
    } = shared();
    let bob_core = join(&server, vault);
    let mut bob = bob_core;
    exec(
        &mut bob,
        vault,
        "bob",
        spend(5_00, wallet, vacanze, "hotel", T0 + 60),
    );
    exec(
        &mut alice,
        vault,
        "alice",
        spend(7_00, wallet, vacanze, "taxi", T0 + 120),
    );
    sync(&mut bob, &mut server, vault);
    sync(&mut alice, &mut server, vault);
    sync(&mut bob, &mut server, vault);

    let ids = |core: &Core| {
        let snapshot = core.snapshot(vault).unwrap();
        let mut out: Vec<Uuid> = snapshot.wallets.iter().map(|w| w.id).collect();
        out.extend(snapshot.flows.iter().map(|f| f.id));
        out.extend(core.categories(vault, true).unwrap().iter().map(|c| c.id));
        out.extend(
            core.list_transactions(vault, &all(), 200, None)
                .unwrap()
                .items
                .iter()
                .map(|t| t.id),
        );
        out
    };
    let expected = ids(&server.core);
    assert!(!expected.is_empty());
    assert_eq!(ids(&alice), expected);
    assert_eq!(ids(&bob), expected);
}

// ---------------------------------------------------------------------------
// Rejections
// ---------------------------------------------------------------------------

#[test]
fn a_server_rejection_leaves_the_projection_and_is_dismissable() {
    let Shared {
        mut server,
        mut alice,
        vault,
        wallet,
        vacanze,
    } = shared();
    let mut bob = join(&server, vault);

    // Vacanze holds 50.00. Alice takes 45.00 first; Bob's 40.00 fits locally
    // but not once Alice's spend is on the server.
    exec(
        &mut alice,
        vault,
        "alice",
        spend(45_00, wallet, vacanze, "volo", T0 + 60),
    );
    sync(&mut alice, &mut server, vault);

    let refused = exec(
        &mut bob,
        vault,
        "bob",
        spend(40_00, wallet, vacanze, "hotel", T0 + 120),
    );
    assert!(
        bob.list_transactions(vault, &all(), 50, None)
            .unwrap()
            .items
            .iter()
            .any(|t| t.id == refused),
        "the optimistic spend is in Bob's projection"
    );

    let request = bob.push_request(vault, ALL).unwrap();
    let response = server.push(vault, request);
    let report = bob.apply_push_response(vault, &response).unwrap();

    assert_eq!(report.confirmed, 0);
    assert!(report.rebased);
    assert_eq!(report.rejected.len(), 1);
    let rejection = &report.rejected[0];
    assert_eq!(rejection.command_id, refused);
    assert_eq!(rejection.kind, "expense");
    assert_eq!(rejection.code, "insufficient_funds");
    assert!(rejection.message.contains("Vacanze"));

    assert!(
        !bob.list_transactions(vault, &all(), 50, None)
            .unwrap()
            .items
            .iter()
            .any(|t| t.id == refused),
        "the rebuild dropped it"
    );
    assert_eq!(bob.sync_state(vault).unwrap().outbox, 0);
    assert_eq!(bob.sync_state(vault).unwrap().rejected, 1);
    assert_eq!(bob.rejected_commands(vault).unwrap(), report.rejected);
    // A rejected row is out of the log the server would ever see.
    assert!(
        bob.commands_since(vault, 0)
            .unwrap()
            .iter()
            .all(|r| r.envelope.id != refused)
    );

    // Bob catches up and matches the server.
    let (_, pulled) = sync(&mut bob, &mut server, vault);
    assert_eq!(pulled.received, 1);
    assert_eq!(projection(&bob, vault), projection(&server.core, vault));
    // The rejection outlives the rebase until it is dismissed.
    assert_eq!(bob.rejected_commands(vault).unwrap().len(), 1);

    bob.dismiss_rejected(vault, refused).unwrap();
    assert!(bob.rejected_commands(vault).unwrap().is_empty());
    assert_eq!(bob.sync_state(vault).unwrap().rejected, 0);
    // Dismissing twice is quiet.
    bob.dismiss_rejected(vault, refused).unwrap();
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
        .integrate_pull(vault, &server.pull(vault, since))
        .unwrap();

    assert!(report.rebased);
    assert_eq!(report.received, 1);
    assert_eq!(report.confirmed, 0);
    assert_eq!(report.rejected.len(), 1);
    assert_eq!(report.rejected[0].command_id, doomed);
    assert_eq!(report.rejected[0].code, "insufficient_funds");

    assert_eq!(bob.sync_state(vault).unwrap().outbox, 0);
    assert!(bob.push_request(vault, ALL).unwrap().commands.is_empty());
    assert_eq!(projection(&bob, vault), projection(&server.core, vault));
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
    assert_eq!(server.core.last_seq(vault).unwrap(), 3);
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
        commands: server
            .pull(vault, 0)
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

// ---------------------------------------------------------------------------
// Login
// ---------------------------------------------------------------------------

#[test]
fn relabel_outbox_rewrites_the_author_of_what_is_not_yet_pushed() {
    let mut server = FakeServer::new();
    let mut core = Core::open_in_memory().unwrap();
    // The app starts logged out: commands carry a local placeholder author.
    let vault = core
        .execute(CommandEnvelope::create_vault(
            "local",
            "Casa",
            Currency::Eur,
        ))
        .unwrap()
        .result_id
        .unwrap();
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
    assert_eq!(projection(&core, vault), projection(&server.core, vault));

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

// ---------------------------------------------------------------------------
// JSON entry points
// ---------------------------------------------------------------------------

#[test]
fn the_json_entry_points_round_trip_through_the_wire_types() {
    let mut server = FakeServer::new();
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
    exec(&mut alice, vault, "alice", wallet_cmd("Cash", 10_000));

    let body = alice.push_request_json(vault, ALL).unwrap();
    let request: PushRequest = serde_json::from_str(&body).unwrap();
    assert_eq!(request.commands.len(), 2);

    let response = serde_json::to_string(&server.push(vault, request)).unwrap();
    let report = alice.apply_push_response_json(vault, &response).unwrap();
    assert_eq!(report.confirmed, 2);

    let pull = serde_json::to_string(&server.pull(vault, 0)).unwrap();
    assert_eq!(alice.integrate_pull_json(vault, &pull).unwrap(), quiet(2));

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

// ---------------------------------------------------------------------------
// Batches and server-side reads
// ---------------------------------------------------------------------------

#[test]
fn a_long_outbox_goes_up_in_batches_and_converges() {
    const BATCH: usize = 500;
    const TOTAL: i64 = 1_200;

    let mut server = FakeServer::new();
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
        let response = server.core.serve_push(vault, &request).unwrap();
        alice.apply_push_response(vault, &response).unwrap();
    }

    assert_eq!(rounds, 3, "1200 commands in batches of 500");
    assert_eq!(server.core.last_seq(vault).unwrap(), TOTAL);
    assert_eq!(alice.sync_state(vault).unwrap().last_server_seq, TOTAL);
    assert_eq!(projection(&alice, vault), projection(&server.core, vault));
}

#[test]
fn a_report_carries_the_servers_last_seq_and_whether_more_is_waiting() {
    let Shared { server, vault, .. } = shared();
    let mut bob = Core::open_in_memory().unwrap();

    // One record per page: the first two leave something behind.
    for since in 0..2 {
        let report = bob
            .integrate_pull(vault, &server.pull_page(vault, since, 1))
            .unwrap();
        assert_eq!(report.server_last_seq, 3);
        assert!(report.has_more, "page from {since} is not the last");
    }
    let last = bob
        .integrate_pull(vault, &server.pull_page(vault, 2, 1))
        .unwrap();
    assert_eq!(last.server_last_seq, 3);
    assert!(!last.has_more);
    assert_eq!(projection(&bob, vault), projection(&server.core, vault));
}

#[test]
fn the_server_reads_one_vault_and_every_last_seq_at_once() {
    let Shared {
        mut server, vault, ..
    } = shared();
    let casa = server.core.vault(vault).unwrap().expect("Casa");
    assert_eq!(casa.id, vault);
    assert_eq!(casa.name, "Casa");
    assert_eq!(casa.owner, "alice");
    assert_eq!(server.core.vault(Uuid::now_v7()).unwrap(), None);

    // A second vault, with a log of its own.
    let other = server
        .core
        .execute(CommandEnvelope::create_vault(
            "bob",
            "Ufficio",
            Currency::Eur,
        ))
        .unwrap()
        .result_id
        .unwrap();
    let mut seqs = server.core.last_seqs().unwrap();
    seqs.sort_by_key(|(_, seq)| *seq);
    assert_eq!(seqs, vec![(other, 1), (vault, 3)]);
}
