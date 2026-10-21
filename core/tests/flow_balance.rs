//! Tests for `flow_balance_ranges`: what an envelope holds on a past day and
//! how low and how high it goes afterwards, the figures a transfer dated
//! that day is checked against.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use common::*;
use sparagne_core::{Command, FlowBalanceRange, FlowMode};
use uuid::Uuid;

const DAY: i64 = 86_400;

fn range(fx: &Fx, flow: Uuid, from: i64) -> FlowBalanceRange {
    fx.core
        .flow_balance_ranges(fx.vault, utc(from))
        .unwrap()
        .into_iter()
        .find(|r| r.flow_id == flow)
        .unwrap()
}

fn transfer(fx: &mut Fx, amount: i64, from: Uuid, to: Uuid, secs: i64) {
    fx.run(Command::TransferFlow {
        amount,
        from_flow_id: from,
        to_flow_id: to,
        note: None,
        occurred_at: at(secs),
    });
}

fn spend(fx: &mut Fx, amount: i64, secs: i64) {
    let spesa = entry(amount, None, None, Some("Spesa"), secs);
    fx.run(Command::Expense(spesa));
}

/// Unallocated gets 1.000 on day 1, spends 800 on day 10 and gets 1.800 on
/// day 20: it holds 2.000 today, but only 200 between day 10 and day 20.
fn ups_and_downs() -> Fx {
    let mut fx = setup();
    income_in(&mut fx, 1_000, T0 + DAY);
    spend(&mut fx, 800, T0 + 10 * DAY);
    income_in(&mut fx, 1_800, T0 + 20 * DAY);
    fx
}

#[test]
fn a_past_day_holds_what_came_before_it_and_the_lowest_comes_later() {
    let fx = ups_and_downs();
    assert_eq!(
        range(&fx, fx.unallocated, T0 + 5 * DAY),
        FlowBalanceRange {
            flow_id: fx.unallocated,
            balance: 1_000,
            low: 200,
            low_at: Some(utc(T0 + 10 * DAY)),
            high: 2_000,
        }
    );

    // After the last row, the range is the balance the snapshot shows.
    let today = range(&fx, fx.unallocated, T0 + 25 * DAY);
    assert_eq!(
        (today.balance, today.low, today.low_at, today.high),
        (2_000, 2_000, None, 2_000)
    );

    // Before every row the envelope was empty, and never lower than that.
    let before = range(&fx, fx.unallocated, T0);
    assert_eq!(
        (before.balance, before.low, before.low_at, before.high),
        (0, 0, None, 2_000)
    );
}

#[test]
fn transfers_count_and_every_envelope_comes_back_unallocated_first() {
    let mut fx = ups_and_downs();
    let casa = envelope(&mut fx, "Casa", FlowMode::NetCapped { cap: 1_000 });
    let unallocated = fx.unallocated;
    // Dated day 15, while Unallocated held 200: it goes 100 below zero then.
    transfer(&mut fx, 300, unallocated, casa, T0 + 15 * DAY);
    transfer(&mut fx, 100, casa, unallocated, T0 + 30 * DAY);

    let ranges = fx
        .core
        .flow_balance_ranges(fx.vault, utc(T0 + 5 * DAY))
        .unwrap();
    assert_eq!(
        ranges.iter().map(|r| r.flow_id).collect::<Vec<_>>(),
        [fx.unallocated, casa]
    );

    let source = range(&fx, unallocated, T0 + 5 * DAY);
    assert_eq!((source.balance, source.low), (1_000, -100));
    assert_eq!(source.low_at, Some(utc(T0 + 15 * DAY)));

    let casa_range = range(&fx, casa, T0 + 5 * DAY);
    assert_eq!(
        (
            casa_range.balance,
            casa_range.low,
            casa_range.low_at,
            casa_range.high
        ),
        (0, 0, None, 300)
    );
    assert_eq!(flow_balance(&fx.core, fx.vault, casa), 200);
}

#[test]
fn a_voided_row_is_gone_and_one_instant_is_summed_before_it_counts() {
    let mut fx = ups_and_downs();
    // Spent and earned in the same second: no dip between the two.
    spend(&mut fx, 500, T0 + 30 * DAY);
    income_in(&mut fx, 500, T0 + 30 * DAY);
    let same_second = range(&fx, fx.unallocated, T0 + 25 * DAY);
    assert_eq!((same_second.low, same_second.low_at), (2_000, None));

    let expense = fx
        .core
        .list_transactions(fx.vault, &all(), 100, None)
        .unwrap()
        .items
        .into_iter()
        .find(|t| t.amount == 800)
        .unwrap()
        .id;
    fx.run(Command::VoidTransaction {
        transaction_id: expense,
    });
    let voided = range(&fx, fx.unallocated, T0 + 5 * DAY);
    assert_eq!(
        (voided.balance, voided.low, voided.low_at, voided.high),
        (1_000, 1_000, None, 2_800)
    );
}
