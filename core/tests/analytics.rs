//! Tests for the ledger aggregations (`docs/v2/UI.md` §4): people, the
//! envelope x person matrix, the category breakdown, the twelve-month buckets,
//! the top expenses and the year breakdown behind the RIEPILOGO.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use chrono::{DateTime, TimeZone, Utc};
use common::*;
use sparagne_core::{
    Command, CommandEnvelope, Core, DomainError, FlowMode, Receipt, TransactionFilter,
};
use uuid::Uuid;

const DAY: i64 = 86_400;

fn utc(secs: i64) -> DateTime<Utc> {
    Utc.timestamp_opt(secs, 0).unwrap()
}

/// Runs a command as `author`, so the PERSONA column has more than one value.
fn run_as(core: &mut Core, vault: Uuid, author: &str, cmd: Command) -> Receipt {
    core.execute(CommandEnvelope::new(vault, author, cmd))
        .unwrap()
}

struct Shared {
    fx: Fx,
    cash: Uuid,
    varie: Uuid,
}

/// Two people, two envelopes (both allowed to go negative, so the fixture does
/// not have to pre-fund them), one month: elisa spends 30 on cash, matteo 100
/// on cash and 25 on varie, and matteo earns 900. Matteo also gets a 10 refund
/// on cash.
fn shared_month() -> Shared {
    let mut fx = setup();
    let cash = run(
        &mut fx.core,
        fx.vault,
        flow_cmd("Cash", FlowMode::Unlimited, true, 0),
    )
    .result_id
    .unwrap();
    let varie = run(
        &mut fx.core,
        fx.vault,
        flow_cmd("Varie", FlowMode::Unlimited, true, 0),
    )
    .result_id
    .unwrap();
    let w = Some(fx.wallet);

    run_as(
        &mut fx.core,
        fx.vault,
        "matteo",
        Command::Income(entry(900, w, Some(cash), Some("Stipendio"), T0)),
    );
    run_as(
        &mut fx.core,
        fx.vault,
        "elisa",
        Command::Expense(entry(30, w, Some(cash), Some("Spesa"), T0 + DAY)),
    );
    run_as(
        &mut fx.core,
        fx.vault,
        "matteo",
        Command::Expense(entry(100, w, Some(cash), Some("Casa"), T0 + 2 * DAY)),
    );
    run_as(
        &mut fx.core,
        fx.vault,
        "matteo",
        Command::Expense(entry(25, w, Some(varie), Some("Svago"), T0 + 3 * DAY)),
    );
    run_as(
        &mut fx.core,
        fx.vault,
        "matteo",
        Command::Refund(entry(10, w, Some(cash), Some("Casa"), T0 + 4 * DAY)),
    );

    Shared { fx, cash, varie }
}

// -- people ----------------------------------------------------------------

#[test]
fn people_are_distinct_and_sorted_and_skip_voided_rows() {
    let mut s = shared_month();
    assert_eq!(s.fx.core.people(s.fx.vault).unwrap(), ["elisa", "matteo"]);

    // Voiding elisa's only row drops her from the list.
    let elisa_row = list(
        &s.fx.core,
        s.fx.vault,
        &TransactionFilter {
            person: Some("elisa".to_string()),
            ..Default::default()
        },
    );
    assert_eq!(elisa_row.len(), 1);
    run(
        &mut s.fx.core,
        s.fx.vault,
        Command::VoidTransaction {
            transaction_id: elisa_row[0].id,
        },
    );
    assert_eq!(s.fx.core.people(s.fx.vault).unwrap(), ["matteo"]);
}

// -- flow x person ---------------------------------------------------------

#[test]
fn flow_person_totals_splits_by_envelope_and_author_with_net_expense() {
    let s = shared_month();
    let rows =
        s.fx.core
            .flow_person_totals(s.fx.vault, utc(T0 - DAY), utc(T0 + 30 * DAY))
            .unwrap();

    let find = |flow: Uuid, person: &str| {
        rows.iter()
            .find(|r| r.flow_id == flow && r.person == person)
            .unwrap_or_else(|| panic!("no row for {person}"))
    };

    let elisa = find(s.cash, "elisa");
    assert_eq!((elisa.income, elisa.expense, elisa.refund), (0, 30, 0));
    assert_eq!(elisa.net_expense, 30);

    let matteo_cash = find(s.cash, "matteo");
    assert_eq!(
        (matteo_cash.income, matteo_cash.expense, matteo_cash.refund),
        (900, 100, 10)
    );
    assert_eq!(matteo_cash.net_expense, 90);

    let matteo_varie = find(s.varie, "matteo");
    assert_eq!(matteo_varie.expense, 25);
    assert_eq!(matteo_varie.income, 0);

    // Nothing else moved: three (flow, person) pairs in total.
    assert_eq!(rows.len(), 3);
}

#[test]
fn flow_person_totals_ignores_transfers_and_voided_and_rows_outside_the_range() {
    let mut s = shared_month();
    let vault = s.fx.vault;

    // A flow transfer between the two envelopes must not show up as spending.
    run(
        &mut s.fx.core,
        vault,
        Command::TransferFlow {
            amount: 50,
            from_flow_id: s.cash,
            to_flow_id: s.varie,
            note: None,
            occurred_at: at(T0 + 5 * DAY),
        },
    );
    // A row well past the window.
    run_as(
        &mut s.fx.core,
        vault,
        "elisa",
        Command::Expense(entry(
            777,
            Some(s.fx.wallet),
            Some(s.cash),
            Some("Fuori"),
            T0 + 90 * DAY,
        )),
    );

    let rows =
        s.fx.core
            .flow_person_totals(vault, utc(T0 - DAY), utc(T0 + 30 * DAY))
            .unwrap();
    assert_eq!(rows.len(), 3);
    let elisa = rows
        .iter()
        .find(|r| r.person == "elisa")
        .expect("elisa still has her 30");
    assert_eq!(elisa.expense, 30);
}

#[test]
fn flow_person_totals_rejects_an_inverted_range() {
    let s = shared_month();
    let err =
        s.fx.core
            .flow_person_totals(s.fx.vault, utc(T0 + DAY), utc(T0))
            .unwrap_err();
    assert!(matches!(err, DomainError::InvalidCommand(_)));
}

// -- categories ------------------------------------------------------------

#[test]
fn category_totals_are_sorted_by_net_expense_and_can_be_filtered_by_person() {
    let s = shared_month();
    let all =
        s.fx.core
            .category_totals(s.fx.vault, utc(T0 - DAY), utc(T0 + 30 * DAY), None)
            .unwrap();

    // Casa: 100 spent, 10 refunded -> 90 net, heavier than Svago's 25 and
    // Spesa's 30... so Casa, Spesa, Svago, then Stipendio (income only).
    let names: Vec<&str> = all.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["Casa", "Spesa", "Svago", "Stipendio"]);

    let casa = &all[0];
    assert_eq!((casa.expense, casa.refund, casa.net_expense), (100, 10, 90));
    assert_eq!(casa.count, 2);

    let stipendio = &all[3];
    assert_eq!(stipendio.income, 900);
    assert_eq!(stipendio.net_expense, 0);

    let elisa =
        s.fx.core
            .category_totals(
                s.fx.vault,
                utc(T0 - DAY),
                utc(T0 + 30 * DAY),
                Some("elisa".to_string()),
            )
            .unwrap();
    assert_eq!(elisa.len(), 1);
    assert_eq!(elisa[0].name, "Spesa");
    assert_eq!(elisa[0].net_expense, 30);
}

// -- buckets ---------------------------------------------------------------

#[test]
fn bucket_totals_split_the_range_on_the_boundaries() {
    let s = shared_month();
    // Three buckets of two days each: [T0, +2d), [+2d, +4d), [+4d, +6d).
    let bounds = vec![
        utc(T0),
        utc(T0 + 2 * DAY),
        utc(T0 + 4 * DAY),
        utc(T0 + 6 * DAY),
    ];
    let buckets = s.fx.core.bucket_totals(s.fx.vault, bounds, None).unwrap();
    assert_eq!(buckets.len(), 3);

    // Income 900 on T0 and elisa's 30 on +1d.
    assert_eq!(buckets[0].income, 900);
    assert_eq!(buckets[0].expense, 30);
    // Matteo's 100 on +2d and 25 on +3d.
    assert_eq!(buckets[1].expense, 125);
    assert_eq!(buckets[1].income, 0);
    // The 10 refund on +4d: net_expense floors at zero.
    assert_eq!(buckets[2].refund, 10);
    assert_eq!(buckets[2].expense, 0);
    assert_eq!(buckets[2].net_expense, 0);
}

#[test]
fn bucket_totals_can_be_filtered_by_person_and_reject_bad_boundaries() {
    let s = shared_month();
    let bounds = vec![utc(T0), utc(T0 + 30 * DAY)];
    let elisa =
        s.fx.core
            .bucket_totals(s.fx.vault, bounds.clone(), Some("elisa".to_string()))
            .unwrap();
    assert_eq!(elisa.len(), 1);
    assert_eq!((elisa[0].income, elisa[0].expense), (0, 30));

    assert!(matches!(
        s.fx.core.bucket_totals(s.fx.vault, vec![utc(T0)], None),
        Err(DomainError::InvalidCommand(_))
    ));
    assert!(matches!(
        s.fx.core
            .bucket_totals(s.fx.vault, vec![utc(T0 + DAY), utc(T0)], None),
        Err(DomainError::InvalidCommand(_))
    ));
}

// -- top expenses ----------------------------------------------------------

#[test]
fn top_expenses_are_largest_first_limited_and_filterable() {
    let s = shared_month();
    let top =
        s.fx.core
            .top_expenses(s.fx.vault, utc(T0 - DAY), utc(T0 + 30 * DAY), None, 2)
            .unwrap();
    assert_eq!(top.len(), 2);
    assert_eq!((top[0].amount, top[0].category.as_str()), (100, "Casa"));
    assert_eq!(top[0].person, "matteo");
    assert_eq!((top[1].amount, top[1].category.as_str()), (30, "Spesa"));

    let elisa =
        s.fx.core
            .top_expenses(
                s.fx.vault,
                utc(T0 - DAY),
                utc(T0 + 30 * DAY),
                Some("elisa".to_string()),
                10,
            )
            .unwrap();
    assert_eq!(elisa.len(), 1);
    assert_eq!(elisa[0].amount, 30);
}

// -- year breakdown --------------------------------------------------------

struct YearFx {
    fx: Fx,
    /// The capped envelope, for the tests that add a row of their own.
    fondo: Uuid,
}

/// Opens a wallet with a balance at `secs`, which posts an `Opening` row.
fn open_wallet(core: &mut Core, vault: Uuid, author: &str, name: &str, opening: i64, secs: i64) {
    run_as(
        core,
        vault,
        author,
        Command::CreateWallet {
            name: name.to_string(),
            opening_balance: opening,
            occurred_at: at(secs),
        },
    );
}

/// Three buckets: everything before `T0`, then two stretches of ten days.
fn year_bounds() -> Vec<DateTime<Utc>> {
    vec![utc(0), utc(T0), utc(T0 + 10 * DAY), utc(T0 + 20 * DAY)]
}

/// One unlimited envelope and one capped one (a "fondo"), wallets opened
/// before and inside the year, and two people moving money.
fn year_fx() -> YearFx {
    let mut fx = setup();
    let cash = run(
        &mut fx.core,
        fx.vault,
        flow_cmd("Cash", FlowMode::Unlimited, true, 0),
    )
    .result_id
    .unwrap();
    let fondo = run(
        &mut fx.core,
        fx.vault,
        flow_cmd("Fondo casa", FlowMode::NetCapped { cap: 100_000 }, true, 0),
    )
    .result_id
    .unwrap();
    let w = Some(fx.wallet);

    // Before the year: alice's savings account.
    open_wallet(
        &mut fx.core,
        fx.vault,
        "alice",
        "Risparmi",
        5_000,
        T0 - 5 * DAY,
    );

    // First bucket.
    run_as(
        &mut fx.core,
        fx.vault,
        "alice",
        Command::Income(entry(900, w, Some(cash), Some("Stipendio"), T0 + DAY)),
    );
    run_as(
        &mut fx.core,
        fx.vault,
        "elisa",
        Command::Expense(entry(100, w, Some(cash), Some("Spesa"), T0 + 2 * DAY)),
    );
    run_as(
        &mut fx.core,
        fx.vault,
        "elisa",
        Command::Expense(entry(80, w, Some(fondo), Some("Casa"), T0 + 3 * DAY)),
    );
    run_as(
        &mut fx.core,
        fx.vault,
        "elisa",
        Command::Refund(entry(30, w, Some(fondo), Some("Casa"), T0 + 4 * DAY)),
    );
    open_wallet(&mut fx.core, fx.vault, "alice", "Conto", 200, T0 + 5 * DAY);
    open_wallet(&mut fx.core, fx.vault, "alice", "Debito", -50, T0 + 6 * DAY);

    // Second bucket: elisa alone.
    run_as(
        &mut fx.core,
        fx.vault,
        "elisa",
        Command::Expense(entry(40, w, Some(cash), Some("Spesa"), T0 + 11 * DAY)),
    );

    YearFx { fx, fondo }
}

#[test]
fn year_breakdown_puts_wallet_openings_in_their_own_column() {
    let s = year_fx();
    let rows = s.fx.core.year_breakdown(s.fx.vault, year_bounds()).unwrap();

    // Everything before the year is bucket 0: only alice's 5.000 opening,
    // and it is an opening, never income.
    let before: Vec<_> = rows.iter().filter(|r| r.bucket == 0).collect();
    assert_eq!(before.len(), 1);
    assert_eq!(before[0].person, "alice");
    assert_eq!(before[0].opening, 5_000);
    assert_eq!(before[0].income, 0);

    // Inside the year the openings land in their own bucket, signed: 200 in,
    // 50 out, and the negative one is not an expense.
    let alice = rows
        .iter()
        .find(|r| r.bucket == 1 && r.person == "alice")
        .unwrap();
    assert_eq!(alice.opening, 150);
    assert_eq!(alice.income, 900);
    assert_eq!((alice.cash_expense, alice.fund_expense), (0, 0));
}

#[test]
fn year_breakdown_splits_expenses_by_cap_and_nets_refunds_inside_the_group() {
    let s = year_fx();
    let rows = s.fx.core.year_breakdown(s.fx.vault, year_bounds()).unwrap();
    let elisa = rows
        .iter()
        .find(|r| r.bucket == 1 && r.person == "elisa")
        .unwrap();

    // 100 on the unlimited envelope, 80 - 30 on the capped one: the refund
    // nets off the fondo only.
    assert_eq!(elisa.cash_expense, 100);
    assert_eq!(elisa.fund_expense, 50);
    assert_eq!((elisa.income, elisa.opening), (0, 0));

    // A refund alone floors its group at zero instead of going negative.
    let mut s = s;
    run_as(
        &mut s.fx.core,
        s.fx.vault,
        "elisa",
        Command::Refund(entry(
            70,
            Some(s.fx.wallet),
            Some(s.fondo),
            Some("Casa"),
            T0 + 12 * DAY,
        )),
    );
    let rows = s.fx.core.year_breakdown(s.fx.vault, year_bounds()).unwrap();
    let late = rows
        .iter()
        .find(|r| r.bucket == 2 && r.person == "elisa")
        .unwrap();
    assert_eq!((late.cash_expense, late.fund_expense), (40, 0));
}

#[test]
fn year_breakdown_returns_one_ordered_row_per_person_with_movement() {
    let s = year_fx();
    let rows = s.fx.core.year_breakdown(s.fx.vault, year_bounds()).unwrap();

    let pairs: Vec<(u32, &str)> = rows.iter().map(|r| (r.bucket, r.person.as_str())).collect();
    // Bucket order first, then the person case-insensitively; alice has no
    // row in the last bucket because she moved nothing there.
    assert_eq!(
        pairs,
        [(0, "alice"), (1, "alice"), (1, "elisa"), (2, "elisa")]
    );
}

#[test]
fn year_breakdown_rejects_bad_boundaries() {
    let s = year_fx();
    assert!(matches!(
        s.fx.core.year_breakdown(s.fx.vault, vec![utc(T0)]),
        Err(DomainError::InvalidCommand(_))
    ));
    assert!(matches!(
        s.fx.core.year_breakdown(s.fx.vault, vec![]),
        Err(DomainError::InvalidCommand(_))
    ));
    assert!(matches!(
        s.fx.core
            .year_breakdown(s.fx.vault, vec![utc(T0 + DAY), utc(T0)]),
        Err(DomainError::InvalidCommand(_))
    ));
}

// -- filter additions ------------------------------------------------------

#[test]
fn the_filter_selects_one_person_and_can_read_oldest_first() {
    let s = shared_month();
    let matteo = list(
        &s.fx.core,
        s.fx.vault,
        &TransactionFilter {
            person: Some("matteo".to_string()),
            ascending: true,
            ..Default::default()
        },
    );
    assert_eq!(matteo.len(), 4);
    let times: Vec<i64> = matteo.iter().map(|t| t.occurred_at.timestamp()).collect();
    assert!(times.windows(2).all(|w| w[0] <= w[1]), "{times:?}");
    assert!(matteo.iter().all(|t| t.created_by == "matteo"));
}

#[test]
fn ascending_pages_walk_forward_through_the_month() {
    let s = shared_month();
    let filter = TransactionFilter {
        ascending: true,
        ..Default::default()
    };
    let first =
        s.fx.core
            .list_transactions(s.fx.vault, &filter, 2, None)
            .unwrap();
    assert_eq!(first.items.len(), 2);
    let cursor = first.next_cursor.expect("more rows after the first two");

    let second =
        s.fx.core
            .list_transactions(s.fx.vault, &filter, 10, Some(&cursor))
            .unwrap();
    assert_eq!(second.items.len(), 3);
    assert!(second.next_cursor.is_none());

    // The two pages together are the whole month, still in order.
    let times: Vec<i64> = first
        .items
        .iter()
        .chain(second.items.iter())
        .map(|t| t.occurred_at.timestamp())
        .collect();
    assert_eq!(times.len(), 5);
    assert!(times.windows(2).all(|w| w[0] <= w[1]), "{times:?}");
}
