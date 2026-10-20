//! Integration tests for quick-add resolution against a vault.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use chrono::{DateTime, FixedOffset, NaiveDate, TimeZone, Utc};
use common::*;
use sparagne_core::quick_add::{
    self, DateSpec, QuickAdd, QuickAddDefaults, QuickAddError, ResolvedQuickAdd,
};
use sparagne_core::{Command, Currency, DomainError, Entry, FlowMode, TransactionKind};
use uuid::Uuid;

/// A vault with wallets Cash, Bank, Bancoposta and flows Vacanze, Spesa.
struct Fixture {
    fx: Fx,
    cash: Uuid,
    bank: Uuid,
    bancoposta: Uuid,
    vacanze: Uuid,
    spesa: Uuid,
}

fn build_fixture() -> Fixture {
    let mut fx = setup();
    let bank = fx.id(wallet_cmd("Bank", 0));
    let bancoposta = fx.id(wallet_cmd("Bancoposta", 0));
    let vacanze = fx.id(flow_cmd("Vacanze", FlowMode::Unlimited, false, 5000));
    let spesa = fx.id(flow_cmd("Spesa", FlowMode::Unlimited, false, 0));
    let cash = fx.wallet;
    Fixture {
        fx,
        cash,
        bank,
        bancoposta,
        vacanze,
        spesa,
    }
}

impl Fixture {
    /// `line` parsed and resolved at `now` without defaults.
    fn resolve_at(
        &self,
        line: &str,
        now: DateTime<FixedOffset>,
        people: &[String],
    ) -> Result<ResolvedQuickAdd, QuickAddError> {
        let parsed = quick_add::parse(line, Currency::Eur).unwrap();
        let defaults = QuickAddDefaults::default();
        self.fx
            .core
            .resolve_quick_add(self.fx.vault, &parsed, now, &defaults, people)
    }

    fn resolve(&self, line: &str) -> Result<ResolvedQuickAdd, QuickAddError> {
        self.resolve_at(line, at(T0), &[])
    }
}

fn expense(command: &Command) -> &Entry {
    match command {
        Command::Expense(entry) => entry,
        other => panic!("unexpected {other:?}"),
    }
}

fn unknown(kind: &str, name: &str) -> QuickAddError {
    QuickAddError::UnknownName {
        kind: kind.to_string(),
        name: name.to_string(),
    }
}

#[test]
fn exact_beats_prefix_but_ambiguous_prefix_errors() {
    let f = build_fixture();
    // "@bank" is an exact match for Bank, even though it is also a prefix of
    // Bancoposta.
    for (line, wallet) in [
        ("15 pizza @bank", f.bank),
        ("15 pizza @bancoposta", f.bancoposta),
        ("15 pizza @cash", f.cash),
    ] {
        let resolved = f.resolve(line).unwrap();
        assert_eq!(resolved.wallet_id, Some(wallet), "{line}");
        assert_eq!(expense(&resolved.command).wallet_id, Some(wallet), "{line}");
    }

    // "@ban" matches Bank and Bancoposta at the prefix tier: ambiguous.
    match f.resolve("15 pizza @ban").unwrap_err() {
        QuickAddError::AmbiguousName {
            name,
            mut candidates,
        } => {
            assert_eq!(name, "ban");
            candidates.sort();
            assert_eq!(candidates, ["Bancoposta", "Bank"]);
        }
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn unknown_wallet_name_is_an_error() {
    let f = build_fixture();
    assert_eq!(
        f.resolve("15 pizza @revolut").unwrap_err(),
        unknown("wallet", "revolut")
    );
}

#[test]
fn defaults_apply_when_no_marker_present() {
    let f = build_fixture();
    let defaults = QuickAddDefaults {
        wallet_id: Some(f.bank),
        flow_id: Some(f.vacanze),
    };
    let parsed = quick_add::parse("15 pizza", Currency::Eur).unwrap();
    let resolved =
        f.fx.core
            .resolve_quick_add(f.fx.vault, &parsed, at(T0), &defaults, &[])
            .unwrap();
    // The resolution reports the ids it settled on, so the app can keep them
    // as the next defaults without taking the command apart.
    assert_eq!(resolved.wallet_id, Some(f.bank));
    assert_eq!(resolved.flow_id, Some(f.vacanze));
    assert_eq!(resolved.from_id, None);
    assert_eq!(resolved.to_id, None);
    let entry = expense(&resolved.command);
    assert_eq!(entry.wallet_id, Some(f.bank));
    assert_eq!(entry.flow_id, Some(f.vacanze));
}

#[test]
fn unallocated_flow_resolves_by_its_display_name() {
    let f = build_fixture();
    let resolved = f.resolve("15 pizza >unallocated").unwrap();
    assert_eq!(expense(&resolved.command).flow_id, Some(f.fx.unallocated));
}

#[test]
fn yesterday_shifts_occurred_at_by_one_day_keeping_time_and_offset() {
    let f = build_fixture();
    let offset = FixedOffset::east_opt(3600).unwrap();
    let now = offset.from_utc_datetime(&Utc.timestamp_opt(T0, 0).unwrap().naive_utc());
    let resolved = f.resolve_at("15 pizza ieri", now, &[]).unwrap();
    let occurred_at = expense(&resolved.command).occurred_at;
    assert_eq!(
        occurred_at.date_naive(),
        now.date_naive().pred_opt().unwrap()
    );
    assert_eq!(occurred_at.time(), now.time());
    assert_eq!(occurred_at.offset(), now.offset());
}

#[test]
fn archived_wallets_are_not_matched() {
    let mut f = build_fixture();
    f.fx.run(Command::ArchiveWallet { wallet_id: f.bank });
    assert_eq!(
        f.resolve("15 pizza @bank").unwrap_err(),
        unknown("wallet", "bank")
    );
}

#[test]
fn transfer_flow_execute_produces_expected_transaction() {
    let mut f = build_fixture();
    let resolved = f
        .resolve_at("tf>25 >vacanze >spesa weekend", at(T0 + 10), &[])
        .unwrap();
    match &resolved.command {
        Command::TransferFlow {
            from_flow_id,
            to_flow_id,
            amount,
            ..
        } => {
            assert_eq!(*from_flow_id, f.vacanze);
            assert_eq!(*to_flow_id, f.spesa);
            assert_eq!(*amount, 2500);
        }
        other => panic!("unexpected {other:?}"),
    }

    assert_eq!(resolved.from_id, Some(f.vacanze));
    assert_eq!(resolved.to_id, Some(f.spesa));
    assert_eq!(resolved.wallet_id, None);
    assert_eq!(resolved.flow_id, None);

    f.fx.run(resolved.command);

    let txns = list(&f.fx.core, f.fx.vault, &all());
    let transfer = txns
        .iter()
        .find(|t| t.kind == TransactionKind::TransferFlow)
        .expect("transfer transaction present");
    assert_eq!(transfer.amount, 2500);
    assert_eq!(transfer.note.as_deref(), Some("weekend"));
}

#[test]
fn same_target_transfer_is_rejected() {
    let f = build_fixture();
    assert_eq!(
        f.resolve("tw>10 @cash @cash").unwrap_err(),
        QuickAddError::same_target()
    );
}

#[test]
fn resolve_quick_add_error_converts_to_domain_error() {
    let f = build_fixture();
    let domain: DomainError = f.resolve("15 pizza @unknown").unwrap_err().into();
    assert_eq!(domain.code(), "invalid_command");
}

/// Who the app lets a row be for in these tests.
fn people() -> Vec<String> {
    names(&["elisa", "matteo", "marta", "ann", "anna"])
}

fn names(names: &[&str]) -> Vec<String> {
    names.iter().map(|n| (*n).to_string()).collect()
}

fn resolve_person(f: &Fixture, line: &str, people: &[String]) -> Result<String, QuickAddError> {
    let resolved = f.resolve_at(line, at(T0), people)?;
    Ok(expense(&resolved.command)
        .person
        .clone()
        .expect("a person on the entry"))
}

#[test]
fn a_person_resolves_through_the_same_tiers_as_a_wallet() {
    let f = build_fixture();
    let people = people();
    for (line, person) in [
        // Exact, whatever the case, and the command carries the full name.
        ("24 cena !ELISA", "elisa"),
        // A prefix of one name only.
        ("24 cena !eli", "elisa"),
        // Inside one name only.
        ("24 cena !tte", "matteo"),
        // "ann" is exact for ann even though it is a prefix of anna.
        ("24 cena !ann", "ann"),
    ] {
        assert_eq!(resolve_person(&f, line, &people).unwrap(), person, "{line}");
    }
}

#[test]
fn an_unknown_or_ambiguous_person_is_an_error() {
    let f = build_fixture();
    let people = people();
    assert_eq!(
        resolve_person(&f, "24 cena !giorgio", &people).unwrap_err(),
        unknown("person", "giorgio")
    );
    match resolve_person(&f, "24 cena !ma", &people).unwrap_err() {
        QuickAddError::AmbiguousName {
            name,
            mut candidates,
        } => {
            assert_eq!(name, "ma");
            candidates.sort();
            assert_eq!(candidates, ["marta", "matteo"]);
        }
        other => panic!("unexpected {other:?}"),
    }
    // With nobody to choose from every name is unknown.
    assert_eq!(
        resolve_person(&f, "24 cena !elisa", &[]).unwrap_err(),
        unknown("person", "elisa")
    );
}

#[test]
fn repeated_or_blank_people_do_not_make_a_name_ambiguous() {
    let f = build_fixture();
    let people = names(&["elisa", " ", "elisa", "matteo"]);
    assert_eq!(
        resolve_person(&f, "24 cena !eli", &people).unwrap(),
        "elisa"
    );
}

#[test]
fn one_person_under_two_spellings_is_never_ambiguous_with_itself() {
    let f = build_fixture();
    // The local name of a logged-out window and the username its synced
    // rows carry, plus someone else sharing their first letters.
    let people = names(&["Matteo", "matteo", "marta"]);
    for (line, person) in [
        // The name as typed wins, whichever spelling it is.
        ("24 cena !matteo", "matteo"),
        ("24 cena !Matteo", "Matteo"),
        // Neither as typed: the lowercase spelling, the username.
        ("24 cena !MATTEO", "matteo"),
        ("24 cena !matt", "matteo"),
    ] {
        assert_eq!(resolve_person(&f, line, &people).unwrap(), person, "{line}");
    }
    // Still ambiguous with someone else, offering the person once.
    match resolve_person(&f, "24 cena !ma", &people).unwrap_err() {
        QuickAddError::AmbiguousName { mut candidates, .. } => {
            candidates.sort();
            assert_eq!(candidates, ["marta", "matteo"]);
        }
        other => panic!("unexpected {other:?}"),
    }
    // With no lowercase spelling, the first one listed.
    assert_eq!(
        resolve_person(&f, "24 cena !eli", &names(&["Elisa", "ELISA"])).unwrap(),
        "Elisa"
    );
}

#[test]
fn a_row_is_for_the_person_on_the_line_or_else_its_author() {
    for (line, named, person) in [
        ("24 cena @cash", None, "alice"),
        ("24 cena !eli @cash", Some("elisa"), "elisa"),
    ] {
        let mut f = build_fixture();
        let resolved = f.resolve_at(line, at(T0), &people()).unwrap();
        assert_eq!(expense(&resolved.command).person.as_deref(), named);
        let id = f.fx.id(resolved.command);
        let row = f.fx.core.transaction(f.fx.vault, id).unwrap();
        assert_eq!(row.person, person, "{line}");
        assert_eq!(row.created_by, "alice");
        assert_eq!(row.note.as_deref(), Some("cena"));
    }
}

fn day_month(day: u8, month: u8, today: NaiveDate) -> Result<NaiveDate, QuickAddError> {
    DateSpec::DayMonth { day, month }.resolve(today)
}

#[test]
fn a_day_and_month_is_the_latest_occurrence_up_to_a_month_ahead() {
    let today = day(2026, 9, 23);
    // Earlier this year stays in this year, however far back.
    assert_eq!(day_month(12, 3, today), Ok(day(2026, 3, 12)));
    assert_eq!(day_month(23, 9, today), Ok(day(2026, 9, 23)));
    // A little ahead is still this year...
    assert_eq!(day_month(24, 9, today), Ok(day(2026, 9, 24)));
    assert_eq!(day_month(20, 10, today), Ok(day(2026, 10, 20)));
    assert_eq!(day_month(24, 10, today), Ok(day(2026, 10, 24)));
    // ...but past 31 days ahead it is last year.
    assert_eq!(day_month(25, 10, today), Ok(day(2025, 10, 25)));
    assert_eq!(day_month(30, 10, today), Ok(day(2025, 10, 30)));
}

#[test]
fn a_day_and_month_crosses_the_new_year_both_ways() {
    // Early January looks back at the December just gone.
    assert_eq!(day_month(31, 12, day(2027, 1, 2)), Ok(day(2026, 12, 31)));
    // Late December looks ahead at the January to come.
    assert_eq!(day_month(2, 1, day(2026, 12, 31)), Ok(day(2027, 1, 2)));
}

#[test]
fn a_year_without_the_date_is_skipped() {
    // No 29 February in 2029 or 2030: the latest one is 2028's.
    assert_eq!(day_month(29, 2, day(2029, 3, 10)), Ok(day(2028, 2, 29)));
    assert_eq!(day_month(29, 2, day(2028, 3, 5)), Ok(day(2028, 2, 29)));
    // Within the month ahead, next year's leap day counts.
    assert_eq!(day_month(29, 2, day(2028, 2, 1)), Ok(day(2028, 2, 29)));
    // The only 29 February around is too far ahead, the others do not exist;
    // a day the month never has is invalid whatever the year.
    for (d, m, today) in [
        (29, 2, day(2027, 1, 10)),
        (31, 4, day(2026, 9, 23)),
        (1, 13, day(2026, 9, 23)),
    ] {
        assert!(matches!(
            day_month(d, m, today),
            Err(QuickAddError::InvalidDate { .. })
        ));
    }
}

#[test]
fn a_two_digit_year_is_this_century_and_other_lengths_are_invalid() {
    for (token, date) in [
        ("12/03/24", day(2024, 3, 12)),
        ("1/3/99", day(2099, 3, 1)),
        ("12/03/2024", day(2024, 3, 12)),
    ] {
        match quick_add::parse(&format!("10 pizza {token}"), Currency::Eur).unwrap() {
            QuickAdd::Entry { date: parsed, .. } => {
                assert_eq!(parsed, Some(DateSpec::Date(date)), "{token}");
            }
            other => panic!("unexpected {other:?}"),
        }
    }
    for token in ["12/03/024", "12/03/2", "12/03/20245"] {
        assert!(
            matches!(
                quick_add::parse(&format!("10 pizza {token}"), Currency::Eur),
                Err(QuickAddError::InvalidDate { .. })
            ),
            "{token}"
        );
    }
}

#[test]
fn a_day_and_month_on_a_line_looks_back_across_the_new_year() {
    let f = build_fixture();
    let now = FixedOffset::east_opt(3600)
        .unwrap()
        .with_ymd_and_hms(2026, 1, 2, 9, 30, 0)
        .unwrap();
    let resolved = f.resolve_at("15 cenone 31/12", now, &[]).unwrap();
    let occurred_at = expense(&resolved.command).occurred_at;
    assert_eq!(occurred_at.date_naive(), day(2025, 12, 31));
    assert_eq!(occurred_at.time(), now.time());
}

#[test]
fn an_entry_built_as_a_transfer_is_refused_instead_of_panicking() {
    let f = build_fixture();
    for kind in [
        TransactionKind::TransferWallet,
        TransactionKind::TransferFlow,
    ] {
        let parsed = QuickAdd::Entry {
            kind,
            amount: 100,
            note: None,
            category: None,
            wallet: None,
            flow: None,
            date: None,
            person: None,
        };
        let defaults = QuickAddDefaults::default();
        let err =
            f.fx.core
                .resolve_quick_add(f.fx.vault, &parsed, at(T0), &defaults, &[])
                .unwrap_err();
        assert!(
            matches!(err, QuickAddError::Domain { ref code, .. } if code == "invalid_command"),
            "{err:?}"
        );
    }
}
