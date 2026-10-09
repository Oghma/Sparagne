//! The allocation plan over sync: two members deciding periods offline, the
//! server's log order settling who wins, and the limits that follow.
//!
//! Same harness as `two_clients.rs`. The projection compared here adds what
//! the allocation commands change beyond the common one: the plan, the
//! decided runs, the pending period and the base the next execution shares
//! out.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use axum::http::StatusCode;
use chrono::NaiveDate;
use common::{Api, Client, Projection, at, basics, flow_cmd, income_cmd, projection};
use serde_json::json;
use sparagne_core::{
    AllocationBase, AllocationLine, AllocationMove, AllocationPlanPatch, AllocationPlanView,
    AllocationRule, AllocationRunView, Command, Core, FlowMode, Frequency, PendingAllocation,
    RunOutcome, Schedule, TransactionKind,
};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn day(y: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, d).unwrap()
}

/// Noon UTC of a day, in seconds.
fn on(date: NaiveDate) -> i64 {
    date.and_hms_opt(12, 0, 0).unwrap().and_utc().timestamp()
}

/// The periods of the fixture's plan: the 1st of January and of February.
fn jan() -> NaiveDate {
    day(2026, 1, 1)
}

fn feb() -> NaiveDate {
    day(2026, 2, 1)
}

/// What the app asks the core on this day.
fn today() -> NaiveDate {
    day(2026, 2, 15)
}

const INCOME: i64 = 100_000;

/// Everything a user can see of a vault, allocation included.
#[derive(Debug, PartialEq)]
struct Full {
    common: Projection,
    plan: Option<AllocationPlanView>,
    runs: Vec<AllocationRunView>,
    pending: Option<PendingAllocation>,
    base: AllocationBase,
}

fn full(core: &Core, vault: Uuid) -> Full {
    Full {
        common: projection(core, vault),
        plan: core.allocation_plan(vault).unwrap(),
        runs: core.allocation_runs(vault, 100).unwrap(),
        pending: core.pending_allocation(vault, today()).unwrap(),
        base: core.allocation_base(vault).unwrap(),
    }
}

/// The server's own view.
fn server_full(api: &Api, vault: Uuid) -> Full {
    full(&api.state.core(), vault)
}

fn unallocated(core: &Core, vault: Uuid) -> i64 {
    core.snapshot(vault)
        .unwrap()
        .flows
        .iter()
        .find(|flow| flow.is_unallocated)
        .expect("Unallocated")
        .balance
}

fn envelope_balance(core: &Core, vault: Uuid, id: Uuid) -> i64 {
    core.snapshot(vault)
        .unwrap()
        .flows
        .iter()
        .find(|flow| flow.id == id)
        .expect("envelope")
        .balance
}

/// The id of the transfer an execution makes to `flow`.
fn move_id(command_id: Uuid, flow: Uuid) -> Uuid {
    Uuid::new_v5(&command_id, format!("allocation:{flow}").as_bytes())
}

fn has_transaction(core: &Core, vault: Uuid, id: Uuid) -> bool {
    core.list_transactions(vault, &common::all_txns(), 100, None)
        .unwrap()
        .items
        .iter()
        .any(|t| t.id == id)
}

fn rejected_codes(client: &Client, vault: Uuid) -> Vec<(Uuid, String)> {
    client
        .core
        .rejected_commands(vault)
        .unwrap()
        .into_iter()
        .map(|r| (r.command_id, r.code))
        .collect()
}

/// The id of the command the client queued last: `Client::exec` answers with
/// the created entity, which an allocation command does not have.
fn last_command(client: &Client, vault: Uuid) -> Uuid {
    client
        .core
        .outbox(vault)
        .unwrap()
        .last()
        .expect("a queued command")
        .envelope
        .id
}

struct Fixture {
    vault: Uuid,
    wallet: Uuid,
    plan: Uuid,
    rent: Uuid,
    food: Uuid,
}

/// Executes `period` on the client's current base, split in two halves
/// between the fixture's envelopes. Returns the command id and the total.
fn execute(client: &mut Client, f: &Fixture, period: NaiveDate) -> (Uuid, i64) {
    let total = client.core.allocation_base(f.vault).unwrap().total;
    assert!(total > 1, "something to share out");
    let half = total / 2;
    client.exec(
        f.vault,
        Command::ExecuteAllocation {
            plan_id: f.plan,
            period_date: period,
            occurred_at: at(on(period)),
            total,
            moves: vec![
                AllocationMove {
                    flow_id: f.rent,
                    amount: half,
                },
                AllocationMove {
                    flow_id: f.food,
                    amount: total - half,
                },
            ],
            note: None,
        },
    );
    (last_command(client, f.vault), total)
}

fn reopen(client: &mut Client, f: &Fixture, period: NaiveDate) -> Uuid {
    client.exec(
        f.vault,
        Command::ReopenAllocation {
            plan_id: f.plan,
            period_date: period,
        },
    );
    last_command(client, f.vault)
}

fn income(client: &mut Client, f: &Fixture, amount: i64, date: NaiveDate) -> Uuid {
    client.exec(
        f.vault,
        income_cmd(amount, f.wallet, None, "stipendio", on(date)),
    )
}

/// alice and bob share a vault with two envelopes, a monthly plan from
/// 2026-01-01 and one income of January in Unallocated; both are synced.
async fn plan_vault(api: &Api) -> (Client, Client, Fixture) {
    let (mut alice, mut bob, vault, wallet) = basics(api).await;
    let rent = alice.exec(vault, flow_cmd("Rent", FlowMode::Unlimited, false, 0));
    let food = alice.exec(vault, flow_cmd("Food", FlowMode::Unlimited, false, 0));
    let plan = alice.exec(
        vault,
        Command::CreateAllocationPlan {
            schedule: Schedule {
                frequency: Frequency::Monthly { day: 1 },
                interval: 1,
                start_date: jan(),
                end_date: None,
            },
            lines: vec![
                AllocationLine {
                    flow_id: rent,
                    rule: AllocationRule::Percent { basis_points: 5000 },
                },
                AllocationLine {
                    flow_id: food,
                    rule: AllocationRule::Percent { basis_points: 5000 },
                },
            ],
        },
    );
    let f = Fixture {
        vault,
        wallet,
        plan,
        rent,
        food,
    };
    income(&mut alice, &f, INCOME, day(2026, 1, 10));
    alice.sync(api, vault).await;
    bob.sync(api, vault).await;
    assert_eq!(full(&alice.core, vault), full(&bob.core, vault));
    assert_eq!(alice.core.allocation_base(vault).unwrap().total, INCOME);
    (alice, bob, f)
}

/// Syncs the clients in turn, twice, until they settle.
async fn settle(api: &Api, vault: Uuid, first: &mut Client, second: &mut Client) {
    first.sync(api, vault).await;
    second.sync(api, vault).await;
    first.sync(api, vault).await;
    second.sync(api, vault).await;
}

// ---------------------------------------------------------------------------
// 1. The same period twice
// ---------------------------------------------------------------------------

/// Both members execute January offline. The first push wins; the second
/// client's execution is refused with `already_exists`, the rest of its batch
/// (an income after it) goes through, and the two end up identical.
#[tokio::test]
async fn the_same_period_executed_twice_is_refused_for_that_command_only() {
    let api = Api::new();
    let (mut alice, mut bob, f) = plan_vault(&api).await;

    let (by_alice, total) = execute(&mut alice, &f, jan());
    let (by_bob, _) = execute(&mut bob, &f, jan());
    let after = income(&mut bob, &f, 7_000, day(2026, 2, 3));

    alice.sync(&api, f.vault).await;
    let reports = bob.sync(&api, f.vault).await;
    let rejected: Vec<_> = reports.iter().flat_map(|r| r.rejected.clone()).collect();
    assert_eq!(rejected.len(), 1, "{rejected:?}");
    assert_eq!(rejected[0].command_id, by_bob);
    assert_eq!(rejected[0].code, "already_exists");
    assert_eq!(
        rejected_codes(&bob, f.vault),
        [(by_bob, "already_exists".to_string())]
    );
    alice.sync(&api, f.vault).await;

    let expected = server_full(&api, f.vault);
    assert_eq!(full(&alice.core, f.vault), expected);
    assert_eq!(full(&bob.core, f.vault), expected);

    // alice's execution stands, bob's left no transfer, his income did land.
    assert_eq!(expected.runs.len(), 1);
    assert_eq!(expected.runs[0].period_date, jan());
    assert_eq!(expected.runs[0].outcome, RunOutcome::Executed);
    assert_eq!(expected.runs[0].created_by, "alice");
    assert_eq!(expected.runs[0].total, total);
    for client in [&alice, &bob] {
        let core = &client.core;
        assert!(has_transaction(core, f.vault, move_id(by_alice, f.rent)));
        assert!(!has_transaction(core, f.vault, move_id(by_bob, f.rent)));
        assert!(has_transaction(core, f.vault, after));
        assert_eq!(envelope_balance(core, f.vault, f.rent), total / 2);
        assert_eq!(core.sync_state(f.vault).unwrap().outbox, 0);
    }
    // The income came after alice's run, so it is the next base.
    assert_eq!(expected.base.total, 7_000);
}

// ---------------------------------------------------------------------------
// 2. A reopen travels
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_reopen_reaches_the_other_member() {
    let api = Api::new();
    let (mut alice, mut bob, f) = plan_vault(&api).await;

    let (run, total) = execute(&mut alice, &f, jan());
    alice.sync(&api, f.vault).await;
    bob.sync(&api, f.vault).await;
    assert_eq!(full(&bob.core, f.vault).runs.len(), 1);
    assert_eq!(envelope_balance(&bob.core, f.vault, f.rent), total / 2);

    reopen(&mut alice, &f, jan());
    alice.sync(&api, f.vault).await;
    bob.sync(&api, f.vault).await;

    let expected = server_full(&api, f.vault);
    assert_eq!(full(&alice.core, f.vault), expected);
    assert_eq!(full(&bob.core, f.vault), expected);

    assert!(expected.runs.is_empty());
    assert_eq!(expected.pending.map(|p| p.period_date), Some(feb()));
    assert_eq!(
        expected.base.total, INCOME,
        "the income is back in the base"
    );
    assert_eq!(envelope_balance(&bob.core, f.vault, f.rent), 0);
    assert_eq!(envelope_balance(&bob.core, f.vault, f.food), 0);
    // The transfers are voided, not gone.
    let moved = bob.core.transaction(f.vault, move_id(run, f.rent)).unwrap();
    assert!(moved.voided);
    assert_eq!(moved.kind, TransactionKind::TransferFlow);
}

// ---------------------------------------------------------------------------
// 3. A reopen against the next period
// ---------------------------------------------------------------------------

/// alice executes January, both sync, an income of February arrives; then,
/// offline, alice reopens January while bob executes February. The log order
/// decides:
///
/// - alice first: the reopen lands (January is the latest decided), then
///   February is accepted (it is later than every decided period, and January
///   is undecided). Final: one run, February's, and January is closed by it.
/// - bob first: February is decided, so reopening January is no longer "the
///   latest decided period" and alice's command is refused (`invalid_command`). Final:
///   two runs, February and January, both executed, and a rejection on alice.
///
/// Either way both clients end on the server's projection.
async fn reopen_against_next(alice_first: bool) {
    let api = Api::new();
    let (mut alice, mut bob, f) = plan_vault(&api).await;

    execute(&mut alice, &f, jan());
    income(&mut alice, &f, INCOME, day(2026, 2, 10));
    alice.sync(&api, f.vault).await;
    bob.sync(&api, f.vault).await;

    let reopened = reopen(&mut alice, &f, jan());
    let (february, _) = execute(&mut bob, &f, feb());

    if alice_first {
        settle(&api, f.vault, &mut alice, &mut bob).await;
    } else {
        settle(&api, f.vault, &mut bob, &mut alice).await;
    }

    let expected = server_full(&api, f.vault);
    assert_eq!(full(&alice.core, f.vault), expected);
    assert_eq!(full(&bob.core, f.vault), expected);
    for client in [&alice, &bob] {
        assert_eq!(client.core.sync_state(f.vault).unwrap().outbox, 0);
    }
    assert!(has_transaction(
        &bob.core,
        f.vault,
        move_id(february, f.rent)
    ));

    let periods: Vec<_> = expected.runs.iter().map(|r| r.period_date).collect();
    if alice_first {
        assert_eq!(periods, [feb()]);
        assert!(rejected_codes(&alice, f.vault).is_empty());
        assert!(rejected_codes(&bob, f.vault).is_empty());
        assert_eq!(expected.pending, None);
    } else {
        assert_eq!(periods, [feb(), jan()]);
        assert_eq!(
            rejected_codes(&alice, f.vault),
            [(reopened, "invalid_command".to_string())]
        );
        assert!(rejected_codes(&bob, f.vault).is_empty());
    }
}

#[tokio::test]
async fn a_reopen_then_the_next_period_applies_both() {
    reopen_against_next(true).await;
}

#[tokio::test]
async fn the_next_period_then_a_reopen_refuses_the_reopen() {
    reopen_against_next(false).await;
}

// ---------------------------------------------------------------------------
// 4. The accepted limit
// ---------------------------------------------------------------------------

/// KNOWN, ACCEPTED LIMIT. `ExecuteAllocation` carries the moves the app
/// worked out from its own base, and the server only checks them against the
/// plan, never against the incomes it holds. So an income that reaches the
/// server BEFORE an offline execution, from a member who had not seen that
/// execution, is not in the execution's moves; and since the base counts only
/// incomes recorded after the latest run in log order, it precedes the run and
/// is not counted afterwards either. It stays in Unallocated until the user
/// moves it by hand. Nothing is lost or double counted, the money is just not
/// shared out by the plan.
#[tokio::test]
async fn an_income_landing_before_an_offline_execution_stays_in_unallocated() {
    let api = Api::new();
    let (mut alice, mut bob, f) = plan_vault(&api).await;

    // alice shares out January on what she knows, offline.
    let (run, total) = execute(&mut alice, &f, jan());
    assert_eq!(total, INCOME);
    let unallocated_after_run = unallocated(&alice.core, f.vault);

    // bob, who has not seen it, records another income of January, and his
    // push reaches the server first.
    let late = 25_000;
    let late_id = income(&mut bob, &f, late, day(2026, 1, 20));
    bob.sync(&api, f.vault).await;
    alice.sync(&api, f.vault).await;
    bob.sync(&api, f.vault).await;

    let expected = server_full(&api, f.vault);
    assert_eq!(full(&alice.core, f.vault), expected);
    assert_eq!(full(&bob.core, f.vault), expected);
    assert!(
        rejected_codes(&alice, f.vault).is_empty(),
        "the run is accepted"
    );

    assert_eq!(expected.runs.len(), 1);
    assert_eq!(
        expected.runs[0].total, INCOME,
        "the run never saw the income"
    );
    assert!(has_transaction(&alice.core, f.vault, move_id(run, f.rent)));
    assert!(expected.base.incomes.iter().all(|t| t.id != late_id));
    assert_eq!(
        expected.base.total, 0,
        "and the next base does not count it"
    );
    assert_eq!(
        unallocated(&bob.core, f.vault),
        unallocated_after_run + late,
        "it stays in Unallocated"
    );
}

// ---------------------------------------------------------------------------
// 5. Viewers
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_viewer_cannot_push_allocation_commands() {
    let api = Api::new();
    let (alice, _bob, f) = plan_vault(&api).await;

    let mut carol = Client::register(&api, "carol").await;
    let put = api
        .put(
            &format!("/vaults/{}/members", f.vault),
            &alice.token,
            json!({ "username": "carol", "role": "viewer" }),
        )
        .await;
    assert_eq!(put.status, StatusCode::NO_CONTENT, "{:?}", put.body);
    carol.join(&api, f.vault).await;
    assert_eq!(full(&carol.core, f.vault), server_full(&api, f.vault));

    execute(&mut carol, &f, jan());
    carol.exec(
        f.vault,
        Command::SkipAllocation {
            plan_id: f.plan,
            period_date: feb(),
        },
    );

    carol.exec(
        f.vault,
        Command::UpdateAllocationPlan {
            plan_id: f.plan,
            patch: AllocationPlanPatch {
                enabled: Some(false),
                ..AllocationPlanPatch::default()
            },
        },
    );
    let before = api.state.core().last_seq(f.vault).unwrap();
    let request = carol.core.push_request(f.vault, common::BATCH).unwrap();
    assert_eq!(request.commands.len(), 3);
    let res = api
        .post(
            &format!("/vaults/{}/push", f.vault),
            &carol.token,
            json!(request),
        )
        .await;
    assert_eq!(res.status, StatusCode::FORBIDDEN);
    assert_eq!(res.code(), "forbidden");
    assert_eq!(api.state.core().last_seq(f.vault).unwrap(), before);
    assert_eq!(carol.core.sync_state(f.vault).unwrap().outbox, 3);

    let server = server_full(&api, f.vault);
    assert!(server.runs.is_empty());
    assert!(server.plan.unwrap().enabled);
}
