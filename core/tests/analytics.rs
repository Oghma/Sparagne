//! Tests for the ledger aggregations: people, the envelope x person matrix
//! and its moves between envelopes, the category breakdown, the twelve-month
//! buckets, the top expenses, and the year breakdown, the months x categories
//! grid and the balance breakdown behind the RIEPILOGO.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use chrono::{DateTime, Utc};
use common::*;
use sparagne_core::{
    BucketCategoryTotals, Command, DomainError, FlowMode, TargetKind, TransactionFilter,
};
use uuid::Uuid;

const DAY: i64 = 86_400;

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
    let cash = fx.id(flow_cmd("Cash", FlowMode::Unlimited, true, 0));
    let varie = fx.id(flow_cmd("Varie", FlowMode::Unlimited, true, 0));
    let w = Some(fx.wallet);

    let cmd = Command::Income(entry(900, w, Some(cash), Some("Stipendio"), T0));
    fx.run_as("matteo", cmd);
    let cmd = Command::Expense(entry(30, w, Some(cash), Some("Spesa"), T0 + DAY));
    fx.run_as("elisa", cmd);
    let cmd = Command::Expense(entry(100, w, Some(cash), Some("Casa"), T0 + 2 * DAY));
    fx.run_as("matteo", cmd);
    let cmd = Command::Expense(entry(25, w, Some(varie), Some("Svago"), T0 + 3 * DAY));
    fx.run_as("matteo", cmd);
    let cmd = Command::Refund(entry(10, w, Some(cash), Some("Casa"), T0 + 4 * DAY));
    fx.run_as("matteo", cmd);

    Shared { fx, cash, varie }
}

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
    s.fx.run(Command::VoidTransaction {
        transaction_id: elisa_row[0].id,
    });
    assert_eq!(s.fx.core.people(s.fx.vault).unwrap(), ["matteo"]);
}

#[test]
fn flow_person_totals_splits_by_envelope_and_person_with_net_expense() {
    let s = shared_month();
    let (from, to) = month_range();
    let rows = s.fx.core.flow_person_totals(s.fx.vault, from, to).unwrap();

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
    let (from, to) = month_range();
    let w = Some(s.fx.wallet);
    let vault = s.fx.vault;

    // A flow transfer between the two envelopes must not show up as spending.
    s.fx.run(Command::TransferFlow {
        amount: 50,
        from_flow_id: s.cash,
        to_flow_id: s.varie,
        note: None,
        occurred_at: at(T0 + 5 * DAY),
    });
    // A row well past the window.
    let cmd = Command::Expense(entry(777, w, Some(s.cash), Some("Fuori"), T0 + 90 * DAY));
    s.fx.run_as("elisa", cmd);

    let rows = s.fx.core.flow_person_totals(vault, from, to).unwrap();
    assert_eq!(rows.len(), 3);
    let elisa = rows
        .iter()
        .find(|r| r.person == "elisa")
        .expect("elisa still has her 30");
    assert_eq!(elisa.expense, 30);
}

#[test]
fn flow_transfer_totals_count_a_move_in_on_one_envelope_and_out_on_the_other() {
    let mut s = shared_month();
    let (from, to) = month_range();
    let vault = s.fx.vault;
    let move_between =
        |from_flow: Uuid, to_flow: Uuid, amount: i64, secs: i64| Command::TransferFlow {
            amount,
            from_flow_id: from_flow,
            to_flow_id: to_flow,
            note: None,
            occurred_at: at(secs),
        };
    s.fx.run_as("elisa", move_between(s.cash, s.varie, 50, T0 + 5 * DAY));
    s.fx.run_as("elisa", move_between(s.varie, s.cash, 20, T0 + 6 * DAY));
    s.fx.run_as("matteo", move_between(s.cash, s.varie, 7, T0 + 7 * DAY));
    // A voided move and one past the range are left out, and so are the
    // fixture's entries.
    let voided = s.fx.id(move_between(s.cash, s.varie, 1_000, T0 + 8 * DAY));
    s.fx.run(Command::VoidTransaction {
        transaction_id: voided,
    });
    s.fx.run(move_between(s.cash, s.varie, 2_000, T0 + 90 * DAY));

    let rows = s.fx.core.flow_transfer_totals(vault, from, to).unwrap();
    let cell = |flow: Uuid, person: &str| {
        rows.iter()
            .find(|r| r.flow_id == flow && r.person == person)
            .map(|r| (r.moved_in, r.moved_out))
    };
    assert_eq!(cell(s.cash, "elisa"), Some((20, 50)));
    assert_eq!(cell(s.varie, "elisa"), Some((50, 20)));
    assert_eq!(cell(s.cash, "matteo"), Some((0, 7)));
    assert_eq!(cell(s.varie, "matteo"), Some((7, 0)));
    assert_eq!(
        rows.iter().map(|r| r.person.as_str()).collect::<Vec<_>>(),
        ["elisa", "elisa", "matteo", "matteo"]
    );

    let err = s.fx.core.flow_transfer_totals(vault, to, from).unwrap_err();
    assert!(matches!(err, DomainError::InvalidCommand(_)));
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

#[test]
fn every_aggregation_follows_the_person_not_who_recorded_the_row() {
    let mut s = shared_month();
    let w = Some(s.fx.wallet);
    let vault = s.fx.vault;
    // matteo records elisa's dinner on varie: hers in every view.
    let mut dinner = entry(40, w, Some(s.varie), Some("Ristoranti"), T0 + 5 * DAY);
    dinner.person = Some("elisa".to_string());
    s.fx.run_as("matteo", Command::Expense(dinner));
    let (from, to) = month_range();
    let elisa = || Some("elisa".to_string());
    let core = &s.fx.core;

    let matrix = core.flow_person_totals(vault, from, to).unwrap();
    let cell = |flow: Uuid, person: &str| {
        matrix
            .iter()
            .find(|r| r.flow_id == flow && r.person == person)
            .map(|r| r.expense)
    };
    assert_eq!(cell(s.varie, "elisa"), Some(40));
    assert_eq!(cell(s.varie, "matteo"), Some(25));
    assert_eq!(cell(s.cash, "elisa"), Some(30));

    let categories = core.category_totals(vault, from, to, elisa()).unwrap();
    assert_eq!(
        categories
            .iter()
            .map(|c| (c.name.as_str(), c.expense))
            .collect::<Vec<_>>(),
        [("Ristoranti", 40), ("Spesa", 30)]
    );
    let top = core.top_expenses(vault, from, to, elisa(), 10).unwrap();
    assert_eq!(
        top.iter()
            .map(|t| (t.amount, t.person.as_str()))
            .collect::<Vec<_>>(),
        [(40, "elisa"), (30, "elisa")]
    );
    let buckets = core.bucket_totals(vault, vec![from, to], elisa()).unwrap();
    assert_eq!(buckets[0].expense, 70);

    let year = core.year_breakdown(vault, vec![from, to]).unwrap();
    let elisa_year = year.iter().find(|r| r.person == "elisa").unwrap();
    assert_eq!(elisa_year.expense, 70);
    let matteo_year = year.iter().find(|r| r.person == "matteo").unwrap();
    assert_eq!(matteo_year.expense, 90 + 25);

    let grid = core
        .category_bucket_totals(vault, vec![from, to], elisa())
        .unwrap();
    assert_eq!(
        grid.iter()
            .map(|c| (c.name.as_str(), c.net_expense))
            .collect::<Vec<_>>(),
        [("Ristoranti", 40), ("Spesa", 30)]
    );
}

#[test]
fn category_totals_are_sorted_by_net_expense_and_can_be_filtered_by_person() {
    let s = shared_month();
    let (from, to) = month_range();
    let all =
        s.fx.core
            .category_totals(s.fx.vault, from, to, None)
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
            .category_totals(s.fx.vault, from, to, Some("elisa".to_string()))
            .unwrap();
    assert_eq!(elisa.len(), 1);
    assert_eq!(elisa[0].name, "Spesa");
    assert_eq!(elisa[0].net_expense, 30);
}

#[test]
fn bucket_totals_split_the_range_on_the_boundaries() {
    let s = shared_month();
    let buckets =
        s.fx.core
            .bucket_totals(s.fx.vault, two_day_bounds(), None)
            .unwrap();
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

#[test]
fn top_expenses_are_largest_first_limited_and_filterable() {
    let s = shared_month();
    let (from, to) = month_range();
    let top =
        s.fx.core
            .top_expenses(s.fx.vault, from, to, None, 2)
            .unwrap();
    assert_eq!(top.len(), 2);
    assert_eq!((top[0].amount, top[0].category.as_str()), (100, "Casa"));
    assert_eq!(top[0].person, "matteo");
    assert_eq!((top[1].amount, top[1].category.as_str()), (30, "Spesa"));

    let elisa =
        s.fx.core
            .top_expenses(s.fx.vault, from, to, Some("elisa".to_string()), 10)
            .unwrap();
    assert_eq!(elisa.len(), 1);
    assert_eq!(elisa[0].amount, 30);
}

struct YearFx {
    fx: Fx,
    /// The capped envelope, for the tests that add a row of their own.
    fondo: Uuid,
}

/// Opens a wallet with a balance at `secs`, which posts an `Opening` row.
fn open_wallet(fx: &mut Fx, author: &str, name: &str, opening: i64, secs: i64) {
    fx.run_as(
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
    let cash = fx.id(flow_cmd("Cash", FlowMode::Unlimited, true, 0));
    let cmd = flow_cmd("Fondo casa", FlowMode::NetCapped { cap: 100_000 }, true, 0);
    let fondo = fx.id(cmd);
    let w = Some(fx.wallet);

    // Before the year: alice's savings account.
    open_wallet(&mut fx, "alice", "Risparmi", 5_000, T0 - 5 * DAY);

    // First bucket.
    let cmd = Command::Income(entry(900, w, Some(cash), Some("Stipendio"), T0 + DAY));
    fx.run_as("alice", cmd);
    let cmd = Command::Expense(entry(100, w, Some(cash), Some("Spesa"), T0 + 2 * DAY));
    fx.run_as("elisa", cmd);
    let cmd = Command::Expense(entry(80, w, Some(fondo), Some("Casa"), T0 + 3 * DAY));
    fx.run_as("elisa", cmd);
    let cmd = Command::Refund(entry(30, w, Some(fondo), Some("Casa"), T0 + 4 * DAY));
    fx.run_as("elisa", cmd);
    open_wallet(&mut fx, "alice", "Conto", 200, T0 + 5 * DAY);
    open_wallet(&mut fx, "alice", "Debito", -50, T0 + 6 * DAY);

    // Second bucket: elisa alone.
    let cmd = Command::Expense(entry(40, w, Some(cash), Some("Spesa"), T0 + 11 * DAY));
    fx.run_as("elisa", cmd);

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
    assert_eq!(alice.expense, 0);
}

#[test]
fn year_breakdown_sums_every_envelope_and_nets_refunds() {
    let s = year_fx();
    let w = Some(s.fx.wallet);
    let rows = s.fx.core.year_breakdown(s.fx.vault, year_bounds()).unwrap();
    let elisa = rows
        .iter()
        .find(|r| r.bucket == 1 && r.person == "elisa")
        .unwrap();

    // 100 on the unlimited envelope and 80 - 30 on the capped one: a cap
    // does not take an expense out of the month.
    assert_eq!(elisa.expense, 100 + 50);
    assert_eq!((elisa.income, elisa.opening), (0, 0));

    // A refund larger than the month's expenses floors it at zero instead of
    // going negative.
    let mut s = s;
    let cmd = Command::Refund(entry(70, w, Some(s.fondo), Some("Casa"), T0 + 12 * DAY));
    s.fx.run_as("elisa", cmd);
    let rows = s.fx.core.year_breakdown(s.fx.vault, year_bounds()).unwrap();
    let late = rows
        .iter()
        .find(|r| r.bucket == 2 && r.person == "elisa")
        .unwrap();
    assert_eq!(late.expense, 0);
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

/// `(bucket, name, net_expense, count)` in the order the query returns them.
fn cells(rows: &[BucketCategoryTotals]) -> Vec<(u32, &str, i64, u32)> {
    rows.iter()
        .map(|r| (r.bucket, r.name.as_str(), r.net_expense, r.count))
        .collect()
}

/// Three buckets of two days each: [T0, +2d), [+2d, +4d), [+4d, +6d).
fn two_day_bounds() -> Vec<DateTime<Utc>> {
    vec![
        utc(T0),
        utc(T0 + 2 * DAY),
        utc(T0 + 4 * DAY),
        utc(T0 + 6 * DAY),
    ]
}

#[test]
fn category_bucket_totals_put_a_row_on_a_boundary_in_the_later_bucket() {
    let s = shared_month();
    let rows =
        s.fx.core
            .category_bucket_totals(s.fx.vault, two_day_bounds(), None)
            .unwrap();

    // Matteo's 100 on Casa sits exactly on +2d and his 10 refund exactly on
    // +4d: each opens the later bucket. The refund is alone there, so Casa
    // reads zero in bucket 2. The 900 of income is no expense at all.
    assert_eq!(
        cells(&rows),
        [
            (0, "Spesa", 30, 1),
            (1, "Casa", 100, 1),
            (1, "Svago", 25, 1),
            (2, "Casa", 0, 1),
        ]
    );
    assert!(rows.iter().all(|r| !r.is_system));
}

#[test]
fn category_bucket_totals_net_refunds_inside_the_bucket_and_floor_at_zero() {
    let mut s = shared_month();
    let w = Some(s.fx.wallet);
    let bounds = vec![utc(T0), utc(T0 + 30 * DAY)];
    let rows =
        s.fx.core
            .category_bucket_totals(s.fx.vault, bounds.clone(), None)
            .unwrap();
    // Casa: 100 spent, 10 back, two rows behind it.
    assert_eq!(
        cells(&rows),
        [(0, "Casa", 90, 2), (0, "Spesa", 30, 1), (0, "Svago", 25, 1)]
    );

    // A refund larger than what Svago spent floors it at zero, still counted.
    let cmd = Command::Refund(entry(40, w, Some(s.varie), Some("Svago"), T0 + 5 * DAY));
    s.fx.run_as("matteo", cmd);
    let rows =
        s.fx.core
            .category_bucket_totals(s.fx.vault, bounds, None)
            .unwrap();
    assert_eq!(
        cells(&rows),
        [(0, "Casa", 90, 2), (0, "Spesa", 30, 1), (0, "Svago", 0, 2)]
    );
}

#[test]
fn category_bucket_totals_leave_voided_rows_out() {
    let mut s = shared_month();
    let vault = s.fx.vault;
    let w = Some(s.fx.wallet);
    let trip = Command::Expense(entry(500, w, Some(s.cash), Some("Viaggi"), T0 + 5 * DAY));
    let refund = Command::Refund(entry(30, w, Some(s.cash), Some("Casa"), T0 + 5 * DAY));
    let ids = [trip, refund].map(|cmd| s.fx.run_as("matteo", cmd).result_id.unwrap());
    for id in ids {
        s.fx.run(Command::VoidTransaction { transaction_id: id });
    }

    let rows =
        s.fx.core
            .category_bucket_totals(vault, vec![utc(T0), utc(T0 + 30 * DAY)], None)
            .unwrap();
    // No Viaggi at all, and Casa is back to 100 - 10 over its two live rows.
    assert_eq!(
        cells(&rows),
        [(0, "Casa", 90, 2), (0, "Spesa", 30, 1), (0, "Svago", 25, 1)]
    );
}

#[test]
fn category_bucket_totals_can_be_filtered_by_person() {
    let s = shared_month();
    let bounds = vec![utc(T0), utc(T0 + 30 * DAY)];
    let for_person = |person: &str| {
        s.fx.core
            .category_bucket_totals(s.fx.vault, bounds.clone(), Some(person.to_string()))
            .unwrap()
    };

    assert_eq!(cells(&for_person("elisa")), [(0, "Spesa", 30, 1)]);
    assert_eq!(
        cells(&for_person("matteo")),
        [(0, "Casa", 90, 2), (0, "Svago", 25, 1)]
    );
    assert!(for_person("nobody").is_empty());
}

#[test]
fn category_bucket_totals_order_by_bucket_then_heaviest_then_name() {
    let mut s = shared_month();
    let vault = s.fx.vault;
    let w = Some(s.fx.wallet);
    // Two more rows in bucket 1: Affitto would come first by name and comes
    // last by weight; bollette ties with Svago and goes first because the
    // name compares case-insensitively.
    let cmd = Command::Expense(entry(5, w, Some(s.varie), Some("Affitto"), T0 + 3 * DAY));
    s.fx.run_as("matteo", cmd);
    let cmd = Command::Expense(entry(25, w, Some(s.varie), Some("bollette"), T0 + 3 * DAY));
    s.fx.run_as("matteo", cmd);

    let rows =
        s.fx.core
            .category_bucket_totals(vault, two_day_bounds(), None)
            .unwrap();
    // Bucket 0's 30 comes before bucket 1's 100: the bucket orders first.
    assert_eq!(
        cells(&rows),
        [
            (0, "Spesa", 30, 1),
            (1, "Casa", 100, 1),
            (1, "bollette", 25, 1),
            (1, "Svago", 25, 1),
            (1, "Affitto", 5, 1),
            (2, "Casa", 0, 1),
        ]
    );
}

#[test]
fn category_bucket_totals_reject_bad_boundaries() {
    let s = shared_month();
    for bounds in [
        vec![],
        vec![utc(T0)],
        vec![utc(T0 + DAY), utc(T0)],
        vec![utc(T0), utc(T0), utc(T0 + DAY)],
    ] {
        assert!(matches!(
            s.fx.core.category_bucket_totals(s.fx.vault, bounds, None),
            Err(DomainError::InvalidCommand(_))
        ));
    }
}

/// `(bucket, target, delta)` in the order the query returns them.
fn deltas(fx: &Fx, bounds: Vec<DateTime<Utc>>, target: TargetKind) -> Vec<(u32, Uuid, i64)> {
    fx.core
        .balance_breakdown(fx.vault, bounds, target)
        .unwrap()
        .into_iter()
        .map(|r| (r.bucket, r.target_id, r.delta))
        .collect()
}

/// The epoch, then two stretches of thirty days from the day after `T0`:
/// what the app passes, with months of a fixed length.
fn month_bounds() -> Vec<DateTime<Utc>> {
    vec![
        utc(0),
        utc(T0 + DAY),
        utc(T0 + 31 * DAY),
        utc(T0 + 61 * DAY),
    ]
}

#[test]
fn balance_breakdown_moves_a_wallet_transfer_from_one_wallet_to_the_other() {
    let mut fx = setup();
    let conto = fx.id(wallet_cmd("Conto", 0));
    let e = entry(500, Some(fx.wallet), None, Some("Stipendio"), T0 + 2 * DAY);
    fx.run(Command::Income(e));
    fx.run(Command::TransferWallet {
        amount: 200,
        from_wallet_id: fx.wallet,
        to_wallet_id: conto,
        note: None,
        occurred_at: at(T0 + 3 * DAY),
    });

    // The transfer opens a bucket of its own: [T0 + 3d, T0 + 31d).
    let bounds = vec![utc(0), utc(T0 + DAY), utc(T0 + 3 * DAY), utc(T0 + 31 * DAY)];
    let wallets = deltas(&fx, bounds.clone(), TargetKind::Wallet);
    assert_eq!(wallets[0], (1, fx.wallet, 500));
    let transfer: Vec<(Uuid, i64)> = wallets
        .iter()
        .filter(|(bucket, ..)| *bucket == 2)
        .map(|&(_, id, delta)| (id, delta))
        .collect();
    assert_eq!(transfer.len(), 2);
    assert!(transfer.contains(&(fx.wallet, -200)));
    assert!(transfer.contains(&(conto, 200)));
    assert_eq!(transfer.iter().map(|(_, delta)| delta).sum::<i64>(), 0);
    // Bucket first, then the id byte by byte.
    assert!(
        wallets
            .windows(2)
            .all(|w| (w[0].0, w[0].1) < (w[1].0, w[1].1))
    );

    // A wallet transfer has no envelope legs: the envelopes only see the
    // income.
    assert_eq!(
        deltas(&fx, bounds, TargetKind::Flow),
        [(1, fx.unallocated, 500)]
    );
}

#[test]
fn balance_breakdown_puts_an_opening_before_the_first_month_in_bucket_zero() {
    let mut fx = setup();
    // Opened at T0, before the first month boundary at T0 + 1d.
    let risparmi = fx.id(wallet_cmd("Risparmi", 5_000));
    let r = Some(risparmi);
    let cmd = Command::Expense(entry(300, r, None, Some("Casa"), T0 + 2 * DAY));
    fx.run(cmd);
    let cmd = Command::Income(entry(1_200, r, None, Some("Stipendio"), T0 + 40 * DAY));
    fx.run(cmd);

    // The fixture's Cash wallet opened at zero, so it never moved.
    let wallets = deltas(&fx, month_bounds(), TargetKind::Wallet);
    assert_eq!(
        wallets,
        [
            (0, risparmi, 5_000),
            (1, risparmi, -300),
            (2, risparmi, 1_200)
        ]
    );
    // Summed bucket by bucket, the deltas are the balance at the end of each.
    let running: Vec<i64> = wallets
        .iter()
        .scan(0, |balance, &(_, _, delta)| {
            *balance += delta;
            Some(*balance)
        })
        .collect();
    assert_eq!(running, [5_000, 4_700, 5_900]);
    assert_eq!(wallet_balance(&fx.core, fx.vault, risparmi), 5_900);

    // The opening lands on Unallocated on the envelope side.
    let flows = deltas(&fx, month_bounds(), TargetKind::Flow);
    assert_eq!(flows[0], (0, fx.unallocated, 5_000));
}

#[test]
fn balance_breakdown_of_the_envelopes_matches_the_year_breakdown_bucket_by_bucket() {
    let mut fx = setup();
    let vault = fx.vault;
    let w = Some(fx.wallet);
    let spesa = fx.id(flow_cmd("Spesa", FlowMode::Unlimited, true, 0));
    let cmd = flow_cmd("Fondo", FlowMode::NetCapped { cap: 100_000 }, true, 0);
    let fondo = fx.id(cmd);

    // Bucket 0: an opening and an expense before the first month.
    open_wallet(&mut fx, "alice", "Risparmi", 700, T0);
    let cmd = Command::Expense(entry(50, w, Some(spesa), Some("Spesa"), T0));
    fx.run_as("elisa", cmd);
    // Bucket 1: income, a share of it moved to Spesa, expenses on both
    // envelopes, a refund on the capped one and another opening.
    let cmd = Command::Income(entry(1_000, w, None, Some("Stipendio"), T0 + 2 * DAY));
    fx.run_as("alice", cmd);
    let move_to = |from: Uuid, to: Uuid, amount: i64, secs: i64| Command::TransferFlow {
        amount,
        from_flow_id: from,
        to_flow_id: to,
        note: None,
        occurred_at: at(secs),
    };
    fx.run(move_to(fx.unallocated, spesa, 400, T0 + 3 * DAY));
    let cmd = Command::Expense(entry(150, w, Some(spesa), Some("Spesa"), T0 + 4 * DAY));
    fx.run_as("elisa", cmd);
    let cmd = Command::Expense(entry(80, w, Some(fondo), Some("Casa"), T0 + 5 * DAY));
    fx.run_as("matteo", cmd);
    let cmd = Command::Refund(entry(20, w, Some(fondo), Some("Casa"), T0 + 6 * DAY));
    fx.run_as("matteo", cmd);
    open_wallet(&mut fx, "alice", "Conto", 250, T0 + 7 * DAY);
    // Bucket 2: a transfer between envelopes and nothing else.
    fx.run(move_to(spesa, fondo, 30, T0 + 40 * DAY));

    let flows = deltas(&fx, month_bounds(), TargetKind::Flow);
    let year = fx.core.year_breakdown(vault, month_bounds()).unwrap();
    let moved = |bucket: u32| -> i64 {
        flows
            .iter()
            .filter(|(b, ..)| *b == bucket)
            .map(|(_, _, delta)| delta)
            .sum()
    };
    let earned = |bucket: u32| -> i64 {
        year.iter()
            .filter(|r| r.bucket == bucket)
            .map(|r| r.income + r.opening - r.expense)
            .sum()
    };
    // 700 - 50; 1.000 + 250 - 150 - (80 - 20); the transfer nets to zero.
    for (bucket, expected) in [(0, 650), (1, 1_040), (2, 0)] {
        assert_eq!(moved(bucket), expected, "envelopes in bucket {bucket}");
        assert_eq!(earned(bucket), expected, "year in bucket {bucket}");
    }

    // The transfer still shows as movement on both envelopes.
    let last: Vec<(Uuid, i64)> = flows
        .iter()
        .filter(|(b, ..)| *b == 2)
        .map(|&(_, id, delta)| (id, delta))
        .collect();
    assert_eq!(last.len(), 2);
    assert!(last.contains(&(spesa, -30)) && last.contains(&(fondo, 30)));
    // Inside bucket 1 the transfer is part of each envelope's delta.
    let in_month = |id: Uuid| {
        flows
            .iter()
            .find(|&&(b, target, _)| b == 1 && target == id)
            .map(|&(_, _, delta)| delta)
    };
    assert_eq!(in_month(fx.unallocated), Some(1_000 - 400 + 250));
    assert_eq!(in_month(spesa), Some(400 - 150));
    assert_eq!(in_month(fondo), Some(-80 + 20));
}

#[test]
fn balance_breakdown_keeps_an_archived_wallet_with_movement_in_the_range() {
    let mut fx = setup();
    let vecchio = fx.id(wallet_cmd("Vecchio", 300));
    fx.run(Command::TransferWallet {
        amount: 300,
        from_wallet_id: vecchio,
        to_wallet_id: fx.wallet,
        note: None,
        occurred_at: at(T0 + 5 * DAY),
    });
    fx.run(Command::ArchiveWallet { wallet_id: vecchio });

    let wallets = deltas(&fx, month_bounds(), TargetKind::Wallet);
    assert_eq!(wallets.len(), 3);
    assert!(wallets.contains(&(0, vecchio, 300)));
    assert!(wallets.contains(&(1, vecchio, -300)));
    assert!(wallets.contains(&(1, fx.wallet, 300)));
}

#[test]
fn balance_breakdown_leaves_voided_rows_and_rows_past_the_range_out() {
    let mut fx = setup();
    let w = Some(fx.wallet);
    let cmd = Command::Income(entry(500, w, None, Some("Stipendio"), T0 + 2 * DAY));
    let income = fx.id(cmd);
    let cmd = Command::Expense(entry(100, w, None, Some("Spesa"), T0 + 3 * DAY));
    fx.run(cmd);
    // Past the last boundary.
    let cmd = Command::Expense(entry(70, w, None, Some("Spesa"), T0 + 90 * DAY));
    fx.run(cmd);
    fx.run(Command::VoidTransaction {
        transaction_id: income,
    });

    assert_eq!(
        deltas(&fx, month_bounds(), TargetKind::Wallet),
        [(1, fx.wallet, -100)]
    );
    assert_eq!(
        deltas(&fx, month_bounds(), TargetKind::Flow),
        [(1, fx.unallocated, -100)]
    );
}

#[test]
fn balance_breakdown_rejects_bad_boundaries() {
    let fx = setup();
    for target in [TargetKind::Wallet, TargetKind::Flow] {
        for bounds in [
            vec![],
            vec![utc(T0)],
            vec![utc(T0 + DAY), utc(T0)],
            vec![utc(T0), utc(T0), utc(T0 + DAY)],
        ] {
            assert!(matches!(
                fx.core.balance_breakdown(fx.vault, bounds, target),
                Err(DomainError::InvalidCommand(_))
            ));
        }
    }
}

/// The shared month plus two wallets opened inside it: matteo's account at
/// 10.000, posted as an income on Unallocated, and elisa's credit card at
/// -1.500, posted as an expense, both with the system category `Opening`.
/// Next to the card, an ordinary 45 of Spesa for elisa.
fn shared_month_with_openings() -> Shared {
    let mut s = shared_month();
    open_wallet(&mut s.fx, "matteo", "Conto", 10_000, T0 + 5 * DAY);
    open_wallet(&mut s.fx, "elisa", "Carta", -1_500, T0 + 6 * DAY);
    let w = Some(s.fx.wallet);
    let cmd = Command::Expense(entry(45, w, Some(s.cash), Some("Spesa"), T0 + 6 * DAY));
    s.fx.run_as("elisa", cmd);
    s
}

fn month_range() -> (DateTime<Utc>, DateTime<Utc>) {
    (utc(T0 - DAY), utc(T0 + 30 * DAY))
}

#[test]
fn openings_are_read_by_the_year_breakdown_alone() {
    let s = shared_month_with_openings();
    let (from, to) = month_range();
    let year =
        s.fx.core
            .year_breakdown(s.fx.vault, vec![from, to])
            .unwrap();
    let opening = |person: &str| year.iter().find(|r| r.person == person).unwrap().opening;
    // Both openings are in the ledger, and FONDO CASSA reads them signed.
    assert_eq!(opening("matteo"), 10_000);
    assert_eq!(opening("elisa"), -1_500);

    // The month's income and expense leave them out, or the panel's hero
    // would disagree with its by-person and category figures.
    let buckets =
        s.fx.core
            .bucket_totals(s.fx.vault, vec![from, to], None)
            .unwrap();
    assert_eq!(buckets[0].income, 900);
    assert_eq!(buckets[0].expense, 30 + 100 + 25 + 45);
}

#[test]
fn bucket_totals_leave_openings_out() {
    let s = shared_month_with_openings();
    // Both openings and elisa's ordinary 45 fall in the second bucket.
    let bounds = vec![utc(T0), utc(T0 + 5 * DAY), utc(T0 + 30 * DAY)];
    let sums = |person: Option<&str>| {
        s.fx.core
            .bucket_totals(s.fx.vault, bounds.clone(), person.map(str::to_string))
            .unwrap()
            .into_iter()
            .map(|b| (b.income, b.expense, b.refund, b.net_expense))
            .collect::<Vec<_>>()
    };

    assert_eq!(sums(None), [(900, 155, 10, 145), (0, 45, 0, 45)]);
    // Elisa's -1.500 card is no expense, her 45 beside it is.
    assert_eq!(sums(Some("elisa")), [(0, 30, 0, 30), (0, 45, 0, 45)]);
    // Matteo's 10.000 account is no income: nothing of his in bucket 1.
    assert_eq!(sums(Some("matteo")), [(900, 125, 10, 115), (0, 0, 0, 0)]);
}

#[test]
fn the_month_panel_figures_agree_with_wallets_opened_in_the_month() {
    let s = shared_month_with_openings();
    let (from, to) = month_range();
    let core = &s.fx.core;
    let vault = s.fx.vault;

    // The hero: income and net expense of the month.
    let hero = &core.bucket_totals(vault, vec![from, to], None).unwrap()[0];
    assert_eq!((hero.income, hero.net_expense), (900, 190));

    // The by-person card: every person's income less net expense over the
    // envelopes adds up to the hero's savings, person by person too.
    let matrix = core.flow_person_totals(vault, from, to).unwrap();
    let saved = |person: &str| -> i64 {
        matrix
            .iter()
            .filter(|r| r.person == person)
            .map(|r| r.income - r.net_expense)
            .sum()
    };
    let people = core.people(vault).unwrap();
    assert_eq!(
        people.iter().map(|p| saved(p)).sum::<i64>(),
        hero.income - hero.net_expense
    );
    for person in &people {
        let own = &core
            .bucket_totals(vault, vec![from, to], Some(person.clone()))
            .unwrap()[0];
        assert_eq!(saved(person), own.income - own.net_expense, "{person}");
    }

    // The category list: no refund here exceeds its category's expenses,
    // so the categories' net expenses add up to the hero's.
    let categories = core.category_totals(vault, from, to, None).unwrap();
    assert_eq!(
        categories.iter().map(|c| c.net_expense).sum::<i64>(),
        hero.net_expense
    );

    // A month whose only row is a wallet opened with 10.000 has nothing to
    // show anywhere: no income, no savings, no activity by person.
    let mut fx = setup();
    open_wallet(&mut fx, "alice", "Conto", 10_000, T0 + DAY);
    let only = &fx
        .core
        .bucket_totals(fx.vault, vec![from, to], None)
        .unwrap()[0];
    assert_eq!((only.income, only.net_expense), (0, 0));
    assert!(
        fx.core
            .flow_person_totals(fx.vault, from, to)
            .unwrap()
            .is_empty()
    );
    assert!(
        fx.core
            .category_totals(fx.vault, from, to, None)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn flow_person_totals_leave_openings_out() {
    let s = shared_month_with_openings();
    let (from, to) = month_range();
    let rows = s.fx.core.flow_person_totals(s.fx.vault, from, to).unwrap();

    // Neither opening shows on Unallocated, for matteo or for elisa.
    assert!(
        rows.iter().all(|r| r.flow_id != s.fx.unallocated),
        "{rows:?}"
    );
    let cell = |flow: Uuid, person: &str| {
        rows.iter()
            .find(|r| r.flow_id == flow && r.person == person)
            .map(|r| (r.income, r.expense, r.refund))
    };
    // The ordinary rows beside them all count, elisa's 45 included.
    assert_eq!(cell(s.cash, "elisa"), Some((0, 30 + 45, 0)));
    assert_eq!(cell(s.cash, "matteo"), Some((900, 100, 10)));
    assert_eq!(cell(s.varie, "matteo"), Some((0, 25, 0)));
    assert_eq!(rows.len(), 3);
}

#[test]
fn category_totals_leave_openings_out() {
    let s = shared_month_with_openings();
    let (from, to) = month_range();
    let all =
        s.fx.core
            .category_totals(s.fx.vault, from, to, None)
            .unwrap();
    assert_eq!(
        all.iter()
            .map(|c| (c.name.as_str(), c.income, c.net_expense))
            .collect::<Vec<_>>(),
        [
            ("Casa", 0, 90),
            ("Spesa", 0, 75),
            ("Svago", 0, 25),
            ("Stipendio", 900, 0),
        ]
    );
    assert!(all.iter().all(|c| !c.is_system));

    let elisa =
        s.fx.core
            .category_totals(s.fx.vault, from, to, Some("elisa".to_string()))
            .unwrap();
    assert_eq!(
        elisa
            .iter()
            .map(|c| (c.name.as_str(), c.net_expense, c.count))
            .collect::<Vec<_>>(),
        [("Spesa", 75, 2)]
    );
}

#[test]
fn category_bucket_totals_leave_openings_out() {
    let s = shared_month_with_openings();
    // The openings and elisa's 45 fall in the second bucket.
    let bounds = vec![utc(T0), utc(T0 + 5 * DAY), utc(T0 + 30 * DAY)];
    let rows =
        s.fx.core
            .category_bucket_totals(s.fx.vault, bounds.clone(), None)
            .unwrap();
    assert_eq!(
        cells(&rows),
        [
            (0, "Casa", 90, 2),
            (0, "Spesa", 30, 1),
            (0, "Svago", 25, 1),
            (1, "Spesa", 45, 1),
        ]
    );
    assert!(rows.iter().all(|r| !r.is_system));

    let elisa =
        s.fx.core
            .category_bucket_totals(s.fx.vault, bounds, Some("elisa".to_string()))
            .unwrap();
    assert_eq!(cells(&elisa), [(0, "Spesa", 30, 1), (1, "Spesa", 45, 1)]);
}

#[test]
fn top_expenses_leave_openings_out() {
    let s = shared_month_with_openings();
    let (from, to) = month_range();
    // The card's 1.500 would lead the list.
    let top =
        s.fx.core
            .top_expenses(s.fx.vault, from, to, None, 10)
            .unwrap();
    assert_eq!(
        top.iter().map(|t| t.amount).collect::<Vec<_>>(),
        [100, 45, 30, 25]
    );
    assert!(top.iter().all(|t| !t.category_is_system));

    let elisa =
        s.fx.core
            .top_expenses(s.fx.vault, from, to, Some("elisa".to_string()), 10)
            .unwrap();
    assert_eq!(elisa.iter().map(|t| t.amount).collect::<Vec<_>>(), [45, 30]);
}

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
