//! `Core::suggest_categories`: the category a note was filed under before.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use chrono::{DateTime, Utc};
use common::{Fx, T0, at, run, setup};
use sparagne_core::{CategorySuggestion, Command, Entry};
use uuid::Uuid;

/// Files an expense of 1.00 with `note` under `category` at `secs`.
fn file(fx: &mut Fx, note: &str, category: &str, secs: i64) -> Uuid {
    file_as(fx, Command::Expense, note, Some(category), secs)
}

fn file_as(
    fx: &mut Fx,
    kind: fn(Entry) -> Command,
    note: &str,
    category: Option<&str>,
    secs: i64,
) -> Uuid {
    let command = kind(Entry {
        amount: 1_00,
        wallet_id: Some(fx.wallet),
        flow_id: None,
        category: category.map(str::to_string),
        note: Some(note.to_string()),
        occurred_at: at(secs),
    });
    run(&mut fx.core, fx.vault, command).result_id.unwrap()
}

fn since(secs: i64) -> DateTime<Utc> {
    DateTime::from_timestamp(secs, 0).unwrap()
}

fn suggest(fx: &Fx, notes: &[&str]) -> Vec<Option<CategorySuggestion>> {
    let notes: Vec<String> = notes.iter().map(|n| (*n).to_string()).collect();
    fx.core
        .suggest_categories(fx.vault, &notes, since(0))
        .unwrap()
}

fn one(fx: &Fx, note: &str) -> Option<CategorySuggestion> {
    suggest(fx, &[note]).pop().unwrap()
}

fn category_id(fx: &Fx, name: &str) -> Uuid {
    fx.core
        .categories(fx.vault, true)
        .unwrap()
        .into_iter()
        .find(|c| c.name == name)
        .unwrap()
        .id
}

/// `(name, uses, exact)` of a suggestion, for compact asserts.
fn shape(suggestion: Option<CategorySuggestion>) -> Option<(String, u32, bool)> {
    suggestion.map(|s| (s.name, s.uses, s.exact))
}

#[test]
fn an_exact_match_picks_the_most_used_category() {
    let mut fx = setup();
    file(&mut fx, "Pizza", "Ristorante", T0);
    file(&mut fx, "pizza!", "Ristorante", T0 + 10);
    // The most recent use goes elsewhere, but the count wins.
    file(&mut fx, "PIZZA", "Spesa", T0 + 20);

    let suggestion = one(&fx, "pizza").unwrap();
    assert_eq!(suggestion.category_id, category_id(&fx, "Ristorante"));
    assert_eq!(suggestion.name, "Ristorante");
    assert_eq!(suggestion.uses, 2);
    assert!(suggestion.exact);
}

#[test]
fn a_tie_goes_to_the_most_recent_use() {
    let mut fx = setup();
    file(&mut fx, "caffè", "Colazione", T0 + 100);
    file(&mut fx, "Caffe", "Bar", T0);
    assert_eq!(
        shape(one(&fx, "CAFFÈ")),
        Some(("Colazione".to_string(), 1, true))
    );

    // One more use in Bar, older still, breaks the tie by count.
    file(&mut fx, "caffe", "Bar", T0 - 100);
    assert_eq!(shape(one(&fx, "caffè")), Some(("Bar".to_string(), 2, true)));
}

#[test]
fn bare_numbers_do_not_count() {
    let mut fx = setup();
    file(&mut fx, "pizza 2", "Ristorante", T0);
    file(&mut fx, "Pizza, 12.50", "Ristorante", T0 + 1);
    assert_eq!(
        shape(one(&fx, "pizza")),
        Some(("Ristorante".to_string(), 2, true))
    );
    assert_eq!(
        shape(one(&fx, "Pizza 3")),
        Some(("Ristorante".to_string(), 2, true))
    );
    // A note of digits only has no key at all.
    assert_eq!(one(&fx, "12 50"), None);
}

#[test]
fn the_first_word_falls_back_with_enough_evidence() {
    let mut fx = setup();
    file(&mut fx, "benzina eni", "Auto", T0);
    file(&mut fx, "benzina q8", "Auto", T0 + 1);
    file(&mut fx, "benzina tamoil", "Moto", T0 + 2);
    // Three uses of "benzina", two in Auto: 67%.
    assert_eq!(
        shape(one(&fx, "Benzina IP")),
        Some(("Auto".to_string(), 2, false))
    );
    // An exact note still wins over the first word.
    assert_eq!(
        shape(one(&fx, "benzina tamoil")),
        Some(("Moto".to_string(), 1, true))
    );

    // Exactly 60% is enough.
    file(&mut fx, "bar sport", "Bar", T0);
    file(&mut fx, "bar centrale", "Bar", T0 + 1);
    file(&mut fx, "bar stazione", "Bar", T0 + 2);
    file(&mut fx, "bar mario", "Colazione", T0 + 3);
    file(&mut fx, "bar gino", "Colazione", T0 + 4);
    assert_eq!(
        shape(one(&fx, "bar roma")),
        Some(("Bar".to_string(), 3, false))
    );
}

#[test]
fn the_first_word_stays_quiet_without_enough_evidence() {
    let mut fx = setup();
    // One use only.
    file(&mut fx, "farmacia centrale", "Salute", T0);
    assert_eq!(one(&fx, "farmacia comunale"), None);

    // Two uses split 50/50.
    file(&mut fx, "super conad", "Spesa", T0);
    file(&mut fx, "super mario", "Giochi", T0 + 1);
    assert_eq!(one(&fx, "super coop"), None);

    // A first word shorter than three characters never matches.
    file(&mut fx, "da mario", "Ristorante", T0);
    file(&mut fx, "da gino", "Ristorante", T0 + 1);
    assert_eq!(one(&fx, "da luigi"), None);
}

#[test]
fn archived_and_system_categories_are_left_out() {
    let mut fx = setup();
    file(&mut fx, "regalo", "Regali", T0);
    file(&mut fx, "regalo", "Svago", T0 + 1);
    file(&mut fx, "regalo", "Regali", T0 + 2);
    let regali = category_id(&fx, "Regali");
    run(
        &mut fx.core,
        fx.vault,
        Command::ArchiveCategory {
            category_id: regali,
        },
    );
    assert_eq!(
        shape(one(&fx, "regalo")),
        Some(("Svago".to_string(), 1, true))
    );

    // Uncategorized is a system category: it never comes back as a suggestion.
    file_as(&mut fx, Command::Expense, "parcheggio", None, T0);
    file_as(&mut fx, Command::Expense, "parcheggio", None, T0 + 1);
    assert_eq!(one(&fx, "parcheggio"), None);
}

#[test]
fn voided_and_old_transactions_are_left_out() {
    let mut fx = setup();
    let voided = file(&mut fx, "cinema", "Svago", T0);
    file(&mut fx, "cinema", "Uscite", T0 - 1_000);
    run(
        &mut fx.core,
        fx.vault,
        Command::VoidTransaction {
            transaction_id: voided,
        },
    );
    assert_eq!(
        shape(one(&fx, "cinema")),
        Some(("Uscite".to_string(), 1, true))
    );

    // A cutoff after the only live use leaves nothing.
    let notes = vec!["cinema".to_string()];
    assert_eq!(
        fx.core
            .suggest_categories(fx.vault, &notes, since(T0 - 999))
            .unwrap(),
        vec![None]
    );
}

#[test]
fn income_and_refunds_count_too() {
    let mut fx = setup();
    file_as(&mut fx, Command::Income, "stipendio", Some("Lavoro"), T0);
    file_as(
        &mut fx,
        Command::Refund,
        "reso amazon",
        Some("Shopping"),
        T0,
    );
    assert_eq!(
        shape(one(&fx, "Stipendio")),
        Some(("Lavoro".to_string(), 1, true))
    );
    assert_eq!(
        shape(one(&fx, "reso amazon")),
        Some(("Shopping".to_string(), 1, true))
    );
}

#[test]
fn suggestions_keep_the_order_of_the_notes() {
    let mut fx = setup();
    file(&mut fx, "cinema", "Svago", T0);
    file(&mut fx, "pizza", "Ristorante", T0);

    let names: Vec<Option<String>> =
        suggest(&fx, &["", "cinema", "  ", "pizza", "!!!", "boh", "cinema"])
            .into_iter()
            .map(|s| s.map(|s| s.name))
            .collect();
    assert_eq!(
        names,
        vec![
            None,
            Some("Svago".to_string()),
            None,
            Some("Ristorante".to_string()),
            None,
            None,
            Some("Svago".to_string()),
        ]
    );
    assert!(suggest(&fx, &[]).is_empty());
}

#[test]
fn an_unknown_vault_suggests_nothing() {
    let fx = setup();
    let notes = vec!["pizza".to_string()];
    assert_eq!(
        fx.core
            .suggest_categories(Uuid::now_v7(), &notes, since(0))
            .unwrap(),
        vec![None]
    );
}
