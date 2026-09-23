//! Statement import: reading the CSV, the card preset, the preview and the
//! import with its deduplication. Every fixture is synthetic.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use chrono::{Datelike, Timelike};
use common::*;
use sparagne_core::statement::{self, STATEMENT_NAMESPACE};
use sparagne_core::{
    AmountSign, Command, CommandEnvelope, Core, Currency, DomainError, FlowMode, StatementAction,
    StatementDateFormat, StatementMapping, StatementOptions, StatementPreview, StatementRow,
    StatementRowOverride, StatementRowStatus, StatementTypeRule, TransactionKind,
};
use uuid::Uuid;

const CARD: &str = include_str!("fixtures/statement/card.csv");
const BANK: &str = include_str!("fixtures/statement/bank.csv");
const CARD_HEADER: &str = "timestamp,type,description,status,amount,currency,card,\
    card holder name,original amount,original currency,cashback earned,cashback currency,\
    category,spending mode";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn card_mapping() -> StatementMapping {
    statement::presets()
        .into_iter()
        .find(|preset| preset.id == "card-transactions")
        .unwrap()
        .mapping
}

/// The card preset with top-ups coming from `wallet`.
fn card_mapping_with_topups_from(wallet: Uuid) -> StatementMapping {
    let mut mapping = card_mapping();
    for rule in &mut mapping.type_rules {
        if rule.value == "topup" {
            rule.action = StatementAction::TransferIn {
                from_wallet_id: Some(wallet),
            };
        }
    }
    mapping
}

/// The `;` export of an Italian bank: day-first dates, decimal commas,
/// spending negative, no type column.
fn bank_mapping() -> StatementMapping {
    StatementMapping {
        delimiter: ";".to_string(),
        date_column: "Data operazione".to_string(),
        date_format: StatementDateFormat::DayMonthYear,
        amount_column: "Importo".to_string(),
        amount_sign: AmountSign::OutflowNegative,
        decimal_comma: true,
        description_columns: vec!["Descrizione".to_string()],
        type_column: None,
        type_rules: Vec::new(),
        default_action: StatementAction::BySign,
        status_column: None,
        skip_statuses: Vec::new(),
        currency_column: Some("Divisa".to_string()),
        category_column: None,
        original_amount_column: None,
        original_currency_column: None,
    }
}

fn options(wallet: Uuid) -> StatementOptions {
    StatementOptions {
        wallet_id: wallet,
        flow_id: None,
        timezone: "Europe/Rome".to_string(),
    }
}

/// A card statement made of `rows`, header first.
fn card_csv(rows: &[String]) -> String {
    format!("{CARD_HEADER}\n{}\n", rows.join("\n"))
}

/// A card row; the columns the tests do not look at are filled in.
fn card_row(
    at: &str,
    kind: &str,
    description: &str,
    status: &str,
    amount: &str,
    currency: &str,
) -> String {
    format!(
        "{at},{kind},{description},{status},{amount},{currency},4821,ALEX RIVERA,\
         {amount},{currency},,,,standard"
    )
}

fn spend(at: &str, description: &str, status: &str, amount: &str) -> String {
    card_row(at, "card_spend", description, status, amount, "EUR")
}

fn row(preview: &StatementPreview, line: u32) -> &StatementRow {
    preview
        .rows
        .iter()
        .find(|row| row.line == line)
        .unwrap_or_else(|| panic!("no row on line {line}"))
}

fn skip_reason(row: &StatementRow) -> &str {
    match &row.status {
        StatementRowStatus::Skipped { reason } => reason,
        other => panic!("line {} is {other:?}, not skipped", row.line),
    }
}

fn invalid_code(row: &StatementRow) -> &str {
    match &row.status {
        StatementRowStatus::Invalid { code, .. } => code,
        other => panic!("line {} is {other:?}, not invalid", row.line),
    }
}

fn no_overrides() -> Vec<StatementRowOverride> {
    Vec::new()
}

fn command_count(core: &Core, vault: Uuid) -> usize {
    core.commands_since(vault, 0).unwrap().len()
}

/// A second wallet with money in it, for top-ups.
fn bank_wallet(fx: &mut Fx) -> Uuid {
    run(&mut fx.core, fx.vault, wallet_cmd("Bank", 100_000))
        .result_id
        .unwrap()
}

// ---------------------------------------------------------------------------
// Reading the file
// ---------------------------------------------------------------------------

#[test]
fn quoted_fields_keep_commas_doubled_quotes_and_newlines() {
    let text = "name,note,amount\n\
                \"Rossi, Mario\",\"he said \"\"ciao\"\"\",1.00\n\
                \"two\nlines\",x,2.00\n\
                last,y,3.00\n";
    let detection = statement::detect(text).unwrap();
    assert_eq!(detection.delimiter, ",");
    assert_eq!(detection.headers, ["name", "note", "amount"]);
    assert_eq!(detection.rows, 3);
    assert_eq!(
        detection.sample[0],
        ["Rossi, Mario", "he said \"ciao\"", "1.00"]
    );
    assert_eq!(detection.sample[1], ["two\nlines", "x", "2.00"]);
    assert_eq!(detection.preset_id, None);
}

#[test]
fn a_newline_inside_quotes_moves_the_next_row_down_a_line() {
    let mut fx = setup();
    let text = card_csv(&[
        spend(
            "2026-09-16 08:00:00 UTC",
            "\"CAFE\nCENTRALE\"",
            "CLEARED",
            "1.20",
        ),
        spend("2026-09-15 08:00:00 UTC", "EDICOLA", "CLEARED", "2.00"),
    ]);
    let preview = fx
        .core
        .preview_statement(fx.vault, &text, &card_mapping(), &options(fx.wallet))
        .unwrap();
    assert_eq!(row(&preview, 2).payee, "CAFE CENTRALE");
    assert_eq!(row(&preview, 4).payee, "EDICOLA");
    let report = fx
        .core
        .import_statement(
            fx.vault,
            "alice",
            &text,
            &card_mapping(),
            &options(fx.wallet),
            &[StatementRowOverride {
                line: 4,
                category: None,
                note: None,
                skip: true,
            }],
        )
        .unwrap();
    assert_eq!((report.executed, report.skipped), (1, 1));
}

#[test]
fn crlf_and_a_bom_read_like_lf() {
    let fx = setup();
    let windows = format!("\u{feff}{}", CARD.replace('\n', "\r\n"));
    let detection = statement::detect(&windows).unwrap();
    assert_eq!(detection.headers[0], "timestamp");
    assert_eq!(detection.preset_id.as_deref(), Some("card-transactions"));
    assert_eq!(detection.rows, 9);

    let lf = fx
        .core
        .preview_statement(fx.vault, CARD, &card_mapping(), &options(fx.wallet))
        .unwrap();
    let crlf = fx
        .core
        .preview_statement(fx.vault, &windows, &card_mapping(), &options(fx.wallet))
        .unwrap();
    assert_eq!(lf, crlf);
}

#[test]
fn a_semicolon_bank_export_with_decimal_commas_and_negative_spending() {
    let mut fx = setup();
    let detection = statement::detect(BANK).unwrap();
    assert_eq!(detection.delimiter, ";");
    assert_eq!(detection.preset_id, None);
    assert_eq!(detection.rows, 4);
    assert_eq!(detection.headers[2], "Descrizione");

    let casa = run(
        &mut fx.core,
        fx.vault,
        flow_cmd("Casa", FlowMode::Unlimited, true, 0),
    )
    .result_id
    .unwrap();
    let options = StatementOptions {
        flow_id: Some(casa),
        ..options(fx.wallet)
    };
    let preview = fx
        .core
        .preview_statement(fx.vault, BANK, &bank_mapping(), &options)
        .unwrap();
    let pos = row(&preview, 2);
    assert_eq!(pos.kind, Some(TransactionKind::Expense));
    assert_eq!(pos.amount, 350);
    assert_eq!(pos.payee, "PAGAMENTO POS; BAR CENTRALE MILANO");
    let salary = row(&preview, 3);
    assert_eq!(salary.kind, Some(TransactionKind::Income));
    assert_eq!(salary.amount, 185_000);
    assert_eq!(row(&preview, 4).payee, "BONIFICO A \"MARIO ROSSI\"");
    assert_eq!(row(&preview, 4).amount, 12_000);
    assert_eq!(skip_reason(row(&preview, 5)), "the amount is zero");
    assert_eq!(
        (preview.new_rows, preview.skipped, preview.invalid),
        (3, 1, 0)
    );

    let report = fx
        .core
        .import_statement(
            fx.vault,
            "alice",
            BANK,
            &bank_mapping(),
            &options,
            &no_overrides(),
        )
        .unwrap();
    assert_eq!(report.executed, 3);
    assert_eq!(
        wallet_balance(&fx.core, fx.vault, fx.wallet),
        185_000 - 350 - 12_000
    );
    assert_eq!(
        flow_balance(&fx.core, fx.vault, casa),
        185_000 - 350 - 12_000
    );
}

#[test]
fn date_only_rows_sit_at_local_noon() {
    let fx = setup();
    let preview = fx
        .core
        .preview_statement(fx.vault, BANK, &bank_mapping(), &options(fx.wallet))
        .unwrap();
    let at = row(&preview, 2).occurred_at.unwrap();
    assert_eq!(at.to_rfc3339(), "2026-09-16T12:00:00+02:00");
}

#[test]
fn a_bank_export_imported_from_another_timezone_is_already_imported() {
    let mut fx = setup();
    fx.core
        .import_statement(
            fx.vault,
            "alice",
            BANK,
            &bank_mapping(),
            &options(fx.wallet),
            &no_overrides(),
        )
        .unwrap();
    let abroad = StatementOptions {
        timezone: "America/New_York".to_string(),
        ..options(fx.wallet)
    };
    let preview = fx
        .core
        .preview_statement(fx.vault, BANK, &bank_mapping(), &abroad)
        .unwrap();
    assert_eq!((preview.new_rows, preview.already_imported), (0, 3));
    assert_eq!(
        row(&preview, 2).occurred_at.unwrap().to_rfc3339(),
        "2026-09-16T12:00:00-04:00"
    );
}

#[test]
fn a_ragged_row_is_invalid_and_the_others_are_read() {
    let fx = setup();
    let text = card_csv(&[
        spend("2026-09-16 08:00:00 UTC", "BAR", "CLEARED", "1.20"),
        "2026-09-15 08:00:00 UTC,card_spend,TOO SHORT,CLEARED,2.00".to_string(),
        spend("2026-09-14 08:00:00 UTC", "EDICOLA", "CLEARED", "2.00"),
    ]);
    let preview = fx
        .core
        .preview_statement(fx.vault, &text, &card_mapping(), &options(fx.wallet))
        .unwrap();
    assert_eq!(invalid_code(row(&preview, 3)), "invalid_row");
    assert_eq!(row(&preview, 3).command_id, Uuid::nil());
    assert_eq!((preview.new_rows, preview.invalid), (2, 1));
}

#[test]
fn a_trailing_delimiter_is_not_a_ragged_row_but_an_extra_value_is() {
    let fx = setup();
    let text = "Data operazione;Descrizione;Importo;Divisa\n\
                16/09/2026;BAR;-1,20;EUR;\n\
                15/09/2026;EDICOLA;-2,00;EUR;;\n\
                14/09/2026;CINEMA;-9,00;EUR;extra\n";
    let preview = fx
        .core
        .preview_statement(fx.vault, text, &bank_mapping(), &options(fx.wallet))
        .unwrap();
    assert_eq!(row(&preview, 2).status, StatementRowStatus::New);
    assert_eq!(row(&preview, 3).status, StatementRowStatus::New);
    assert_eq!(invalid_code(row(&preview, 4)), "invalid_row");
}

#[test]
fn an_empty_statement_is_refused() {
    let fx = setup();
    for text in ["", "\u{feff}", "\n\n", ",,,\r\n"] {
        assert!(matches!(
            statement::detect(text),
            Err(DomainError::InvalidCommand(_))
        ));
        assert!(matches!(
            fx.core
                .preview_statement(fx.vault, text, &card_mapping(), &options(fx.wallet)),
            Err(DomainError::InvalidCommand(_))
        ));
    }
}

// ---------------------------------------------------------------------------
// The card preset
// ---------------------------------------------------------------------------

#[test]
fn the_card_header_is_recognised_as_the_preset() {
    let detection = statement::detect(CARD).unwrap();
    assert_eq!(detection.delimiter, ",");
    assert_eq!(detection.preset_id.as_deref(), Some("card-transactions"));
    assert_eq!(detection.headers.len(), 14);
    assert_eq!(detection.rows, 9);
    assert_eq!(detection.sample.len(), 5);
    assert_eq!(detection.sample[0][2], "TRATTORIA DA MARIO");

    let presets = statement::presets();
    assert_eq!(presets.len(), 1);
    assert_eq!(presets[0].id, "card-transactions");
}

#[test]
fn padded_card_descriptions_are_trimmed() {
    let fx = setup();
    let preview = fx
        .core
        .preview_statement(fx.vault, CARD, &card_mapping(), &options(fx.wallet))
        .unwrap();
    assert_eq!(row(&preview, 2).payee, "TRATTORIA DA MARIO");
    assert_eq!(row(&preview, 3).payee, "BOOKS, MAPS & MORE");
    assert_eq!(row(&preview, 4).payee, "SKYLINE AIR");
}

#[test]
fn pending_and_cancelled_rows_are_skipped_and_cleared_ones_are_new() {
    let fx = setup();
    let text = card_csv(&[
        spend("2026-09-16 08:00:00 UTC", "BAR", "PENDING", "1.20"),
        spend("2026-09-16 07:00:00 UTC", "BAR", "CANCELLED", "1.30"),
        spend("2026-09-16 06:00:00 UTC", "BAR", "declined", "1.40"),
        spend("2026-09-16 05:00:00 UTC", "BAR", "CLEARED", "1.50"),
    ]);
    let preview = fx
        .core
        .preview_statement(fx.vault, &text, &card_mapping(), &options(fx.wallet))
        .unwrap();
    assert_eq!(skip_reason(row(&preview, 2)), "status PENDING is skipped");
    assert_eq!(skip_reason(row(&preview, 3)), "status CANCELLED is skipped");
    assert_eq!(skip_reason(row(&preview, 4)), "status declined is skipped");
    assert_eq!(row(&preview, 5).status, StatementRowStatus::New);
    assert_eq!(row(&preview, 5).kind, Some(TransactionKind::Expense));
    assert_eq!((preview.new_rows, preview.skipped), (1, 3));
}

#[test]
fn the_card_fixture_previews_row_by_row() {
    let fx = setup();
    let preview = fx
        .core
        .preview_statement(fx.vault, CARD, &card_mapping(), &options(fx.wallet))
        .unwrap();
    let kinds: Vec<(u32, Option<TransactionKind>, i64)> = preview
        .rows
        .iter()
        .filter(|r| r.status == StatementRowStatus::New)
        .map(|r| (r.line, r.kind, r.amount))
        .collect();
    assert_eq!(
        kinds,
        [
            (3, Some(TransactionKind::Expense), 1890),
            (4, Some(TransactionKind::Expense), 1137),
            (5, Some(TransactionKind::Expense), 320),
            (6, Some(TransactionKind::Refund), 2499),
            (10, Some(TransactionKind::Expense), 1475),
        ]
    );
    assert_eq!(
        (
            preview.new_rows,
            preview.already_imported,
            preview.skipped,
            preview.invalid,
            preview.rounded
        ),
        (5, 0, 4, 0, 0)
    );
}

#[test]
fn a_topup_waits_for_its_source_wallet_then_becomes_a_transfer() {
    let mut fx = setup();
    let bank = bank_wallet(&mut fx);
    let preview = fx
        .core
        .preview_statement(fx.vault, CARD, &card_mapping(), &options(fx.wallet))
        .unwrap();
    let topup = row(&preview, 8);
    assert_eq!(skip_reason(topup), "choose the wallet the money came from");
    assert_eq!(topup.kind, Some(TransactionKind::TransferWallet));
    assert_eq!(topup.counter_wallet_id, None);

    let mapping = card_mapping_with_topups_from(bank);
    let preview = fx
        .core
        .preview_statement(fx.vault, CARD, &mapping, &options(fx.wallet))
        .unwrap();
    let topup = row(&preview, 8);
    assert_eq!(topup.status, StatementRowStatus::New);
    assert_eq!(topup.counter_wallet_id, Some(bank));
    assert_eq!(preview.rounded, 1);

    let report = fx
        .core
        .import_statement(
            fx.vault,
            "alice",
            CARD,
            &mapping,
            &options(fx.wallet),
            &no_overrides(),
        )
        .unwrap();
    assert_eq!((report.executed, report.skipped, report.rounded), (6, 3, 1));
    let transfer = fx.core.transaction(fx.vault, topup.command_id).unwrap();
    assert_eq!(transfer.kind, TransactionKind::TransferWallet);
    assert_eq!(transfer.from_id, Some(bank));
    assert_eq!(transfer.to_id, Some(fx.wallet));
    assert_eq!(transfer.amount, 12_602);
    assert_eq!(wallet_balance(&fx.core, fx.vault, bank), 100_000 - 12_602);
    assert_eq!(
        wallet_balance(&fx.core, fx.vault, fx.wallet),
        12_602 + 2499 - 1890 - 1137 - 320 - 1475
    );
}

#[test]
fn extra_decimals_are_rounded_and_flagged() {
    let fx = setup();
    let preview = fx
        .core
        .preview_statement(fx.vault, CARD, &card_mapping(), &options(fx.wallet))
        .unwrap();
    let topup = row(&preview, 8);
    assert_eq!(topup.amount, 12_602);
    assert!(topup.rounded);
    assert!(!row(&preview, 3).rounded);
}

#[test]
fn liquid_deposits_are_skipped() {
    let fx = setup();
    let preview = fx
        .core
        .preview_statement(fx.vault, CARD, &card_mapping(), &options(fx.wallet))
        .unwrap();
    let deposit = row(&preview, 9);
    assert_eq!(skip_reason(deposit), "type liquid_deposit is skipped");
    assert_eq!(deposit.kind, None);
}

#[test]
fn a_row_in_another_currency_is_invalid() {
    let fx = setup();
    let text = card_csv(&[card_row(
        "2026-09-16 08:00:00 UTC",
        "card_spend",
        "DINER",
        "CLEARED",
        "7.45",
        "USD",
    )]);
    let preview = fx
        .core
        .preview_statement(fx.vault, &text, &card_mapping(), &options(fx.wallet))
        .unwrap();
    let diner = row(&preview, 2);
    assert_eq!(invalid_code(diner), "currency_mismatch");
    assert_eq!(preview.invalid, 1);
}

#[test]
fn the_original_amount_goes_into_the_note() {
    let mut fx = setup();
    let preview = fx
        .core
        .preview_statement(fx.vault, CARD, &card_mapping(), &options(fx.wallet))
        .unwrap();
    let flight = row(&preview, 4);
    assert_eq!(flight.original.as_deref(), Some("12.34 USD"));
    assert_eq!(
        row(&preview, 3).original,
        None,
        "same amount, same currency"
    );
    assert_eq!(
        row(&preview, 8).original.as_deref(),
        Some("126.017164395008 EURC")
    );

    fx.core
        .import_statement(
            fx.vault,
            "alice",
            CARD,
            &card_mapping(),
            &options(fx.wallet),
            &no_overrides(),
        )
        .unwrap();
    let tx = fx.core.transaction(fx.vault, flight.command_id).unwrap();
    assert_eq!(tx.note.as_deref(), Some("SKYLINE AIR (12.34 USD)"));
    let books = fx
        .core
        .transaction(fx.vault, row(&preview, 3).command_id)
        .unwrap();
    assert_eq!(books.note.as_deref(), Some("BOOKS, MAPS & MORE"));
}

#[test]
fn a_sign_that_contradicts_the_action_keeps_the_action() {
    let fx = setup();
    let text = card_csv(&[card_row(
        "2026-09-16 08:00:00 UTC",
        "card_refund",
        "SHOEBOX",
        "CLEARED",
        "24.99",
        "EUR",
    )]);
    let preview = fx
        .core
        .preview_statement(fx.vault, &text, &card_mapping(), &options(fx.wallet))
        .unwrap();
    let refund = row(&preview, 2);
    assert_eq!(refund.kind, Some(TransactionKind::Refund));
    assert_eq!(refund.amount, 2499);
    assert_eq!(refund.status, StatementRowStatus::New);
}

// ---------------------------------------------------------------------------
// Dates
// ---------------------------------------------------------------------------

#[test]
fn a_utc_row_late_at_night_lands_on_the_next_local_day() {
    let mut fx = setup();
    let text = card_csv(&[spend(
        "2026-03-31 23:30:00 UTC",
        "NIGHT BUS",
        "CLEARED",
        "2.00",
    )]);
    let preview = fx
        .core
        .preview_statement(fx.vault, &text, &card_mapping(), &options(fx.wallet))
        .unwrap();
    let at = row(&preview, 2).occurred_at.unwrap();
    assert_eq!((at.month(), at.day(), at.hour()), (4, 1, 1));
    assert_eq!(at.offset().local_minus_utc(), 2 * 3600);

    fx.core
        .import_statement(
            fx.vault,
            "alice",
            &text,
            &card_mapping(),
            &options(fx.wallet),
            &no_overrides(),
        )
        .unwrap();
    let tx = fx
        .core
        .transaction(fx.vault, row(&preview, 2).command_id)
        .unwrap();
    assert_eq!(tx.occurred_at, at);
}

#[test]
fn a_bad_date_or_amount_makes_the_row_invalid() {
    let fx = setup();
    let text = card_csv(&[
        spend("16/09/2026 08:00", "BAR", "CLEARED", "1.20"),
        spend("2026-09-16 08:00:00 UTC", "BAR", "CLEARED", "1.2.0"),
    ]);
    let preview = fx
        .core
        .preview_statement(fx.vault, &text, &card_mapping(), &options(fx.wallet))
        .unwrap();
    assert_eq!(invalid_code(row(&preview, 2)), "invalid_date");
    assert_eq!(invalid_code(row(&preview, 3)), "invalid_amount");
}

// ---------------------------------------------------------------------------
// Categories and overrides
// ---------------------------------------------------------------------------

#[test]
fn bank_categories_match_only_existing_categories() {
    let mut fx = setup();
    run(
        &mut fx.core,
        fx.vault,
        Command::CreateCategory {
            name: "Bakeries".to_string(),
        },
    );
    let groceries = run(
        &mut fx.core,
        fx.vault,
        Command::CreateCategory {
            name: "Spesa".to_string(),
        },
    )
    .result_id
    .unwrap();
    run(
        &mut fx.core,
        fx.vault,
        Command::AddAlias {
            category_id: groceries,
            alias: "Grocery Stores, Supermarkets".to_string(),
        },
    );
    let before = fx.core.categories(fx.vault, true).unwrap().len();

    let preview = fx
        .core
        .preview_statement(fx.vault, CARD, &card_mapping(), &options(fx.wallet))
        .unwrap();
    let bakery = row(&preview, 5);
    assert_eq!(bakery.bank_category.as_deref(), Some("5462 - Bakeries"));
    assert_eq!(bakery.matched_category.as_deref(), Some("Bakeries"));
    assert_eq!(row(&preview, 10).matched_category.as_deref(), Some("Spesa"));
    let books = row(&preview, 3);
    assert_eq!(books.bank_category.as_deref(), Some("Book Stores"));
    assert_eq!(books.matched_category, None);

    fx.core
        .import_statement(
            fx.vault,
            "alice",
            CARD,
            &card_mapping(),
            &options(fx.wallet),
            &no_overrides(),
        )
        .unwrap();
    assert_eq!(fx.core.categories(fx.vault, true).unwrap().len(), before);
    let category = |line| {
        fx.core
            .transaction(fx.vault, row(&preview, line).command_id)
            .unwrap()
            .category
    };
    assert_eq!(category(5), "Bakeries");
    assert_eq!(category(10), "Spesa");
    assert_eq!(category(3), "Uncategorized");
}

#[test]
fn an_override_category_is_created_and_an_override_note_replaces_the_payee() {
    let mut fx = setup();
    let preview = fx
        .core
        .preview_statement(fx.vault, CARD, &card_mapping(), &options(fx.wallet))
        .unwrap();
    let overrides = [
        StatementRowOverride {
            line: 3,
            category: Some("Libri".to_string()),
            note: None,
            skip: false,
        },
        StatementRowOverride {
            line: 4,
            category: None,
            note: Some("Flight to Lisbon".to_string()),
            skip: false,
        },
    ];
    fx.core
        .import_statement(
            fx.vault,
            "alice",
            CARD,
            &card_mapping(),
            &options(fx.wallet),
            &overrides,
        )
        .unwrap();
    assert!(
        fx.core
            .categories(fx.vault, false)
            .unwrap()
            .iter()
            .any(|c| c.name == "Libri")
    );
    let books = fx
        .core
        .transaction(fx.vault, row(&preview, 3).command_id)
        .unwrap();
    assert_eq!(books.category, "Libri");
    let flight = fx
        .core
        .transaction(fx.vault, row(&preview, 4).command_id)
        .unwrap();
    assert_eq!(flight.note.as_deref(), Some("Flight to Lisbon (12.34 USD)"));
}

#[test]
fn an_override_skip_leaves_the_row_out() {
    let mut fx = setup();
    let preview = fx
        .core
        .preview_statement(fx.vault, CARD, &card_mapping(), &options(fx.wallet))
        .unwrap();
    let report = fx
        .core
        .import_statement(
            fx.vault,
            "alice",
            CARD,
            &card_mapping(),
            &options(fx.wallet),
            &[StatementRowOverride {
                line: 5,
                category: None,
                note: None,
                skip: true,
            }],
        )
        .unwrap();
    assert_eq!((report.executed, report.skipped), (4, 5));
    assert!(
        fx.core
            .transaction(fx.vault, row(&preview, 5).command_id)
            .is_err()
    );
    // Skipped by the user, so still new the next time.
    let again = fx
        .core
        .preview_statement(fx.vault, CARD, &card_mapping(), &options(fx.wallet))
        .unwrap();
    assert_eq!(row(&again, 5).status, StatementRowStatus::New);
    assert_eq!(row(&again, 3).status, StatementRowStatus::AlreadyImported);
}

#[test]
fn a_refused_row_is_reported_and_the_others_import() {
    let mut fx = setup();
    let report = fx
        .core
        .import_statement(
            fx.vault,
            "alice",
            CARD,
            &card_mapping(),
            &options(fx.wallet),
            &[StatementRowOverride {
                line: 10,
                category: Some("!!!".to_string()),
                note: None,
                skip: false,
            }],
        )
        .unwrap();
    assert_eq!(report.executed, 4);
    assert_eq!(report.rejected.len(), 1);
    assert_eq!(report.rejected[0].line, 10);
    assert_eq!(report.rejected[0].code, "invalid_name");
}

// ---------------------------------------------------------------------------
// Deduplication and ids
// ---------------------------------------------------------------------------

#[test]
fn reimporting_a_statement_executes_nothing() {
    let mut fx = setup();
    let import = |core: &mut Core| {
        core.import_statement(
            fx.vault,
            "alice",
            CARD,
            &card_mapping(),
            &options(fx.wallet),
            &no_overrides(),
        )
        .unwrap()
    };
    let first = import(&mut fx.core);
    assert_eq!(
        (first.executed, first.deduplicated, first.skipped),
        (5, 0, 4)
    );
    let commands = command_count(&fx.core, fx.vault);

    let second = import(&mut fx.core);
    assert_eq!(
        (second.executed, second.deduplicated, second.skipped),
        (0, 5, 4)
    );
    assert!(second.rejected.is_empty());
    assert_eq!(command_count(&fx.core, fx.vault), commands);

    let preview = fx
        .core
        .preview_statement(fx.vault, CARD, &card_mapping(), &options(fx.wallet))
        .unwrap();
    assert_eq!((preview.new_rows, preview.already_imported), (0, 5));
}

#[test]
fn a_pending_row_that_cleared_is_already_imported() {
    let mut fx = setup();
    let mut mapping = card_mapping();
    mapping.skip_statuses = vec!["CANCELLED".to_string()];
    let first_export = card_csv(&[spend(
        "2026-09-16 08:00:00 UTC",
        "FORNO BIANCO",
        "PENDING",
        "3.20",
    )]);
    let report = fx
        .core
        .import_statement(
            fx.vault,
            "alice",
            &first_export,
            &mapping,
            &options(fx.wallet),
            &no_overrides(),
        )
        .unwrap();
    assert_eq!(report.executed, 1);

    let second_export = card_csv(&[
        spend("2026-09-17 09:00:00 UTC", "EDICOLA", "PENDING", "2.00"),
        spend("2026-09-16 08:00:00 UTC", "FORNO BIANCO", "CLEARED", "3.20"),
    ]);
    let preview = fx
        .core
        .preview_statement(fx.vault, &second_export, &mapping, &options(fx.wallet))
        .unwrap();
    assert_eq!(row(&preview, 2).status, StatementRowStatus::New);
    assert_eq!(row(&preview, 3).status, StatementRowStatus::AlreadyImported);
}

#[test]
fn identical_rows_in_the_same_second_both_import() {
    let mut fx = setup();
    let coffee = spend("2026-09-16 08:00:00 UTC", "CAFE", "CLEARED", "1.20");
    let text = card_csv(&[coffee.clone(), coffee]);
    let preview = fx
        .core
        .preview_statement(fx.vault, &text, &card_mapping(), &options(fx.wallet))
        .unwrap();
    assert_eq!(preview.new_rows, 2);
    assert_ne!(row(&preview, 2).command_id, row(&preview, 3).command_id);

    let import = |core: &mut Core| {
        core.import_statement(
            fx.vault,
            "alice",
            &text,
            &card_mapping(),
            &options(fx.wallet),
            &no_overrides(),
        )
        .unwrap()
    };
    assert_eq!(import(&mut fx.core).executed, 2);
    assert_eq!(wallet_balance(&fx.core, fx.vault, fx.wallet), -240);
    let again = import(&mut fx.core);
    assert_eq!((again.executed, again.deduplicated), (0, 2));
}

#[test]
fn rows_are_imported_oldest_first() {
    let mut fx = setup();
    let since = command_count(&fx.core, fx.vault);
    fx.core
        .import_statement(
            fx.vault,
            "alice",
            CARD,
            &card_mapping(),
            &options(fx.wallet),
            &no_overrides(),
        )
        .unwrap();
    let times: Vec<_> = fx
        .core
        .commands_since(fx.vault, 0)
        .unwrap()
        .into_iter()
        .skip(since)
        .map(|record| record.envelope.command.occurred_at().unwrap())
        .collect();
    assert_eq!(times.len(), 5);
    assert!(times.windows(2).all(|pair| pair[0] <= pair[1]), "{times:?}");
}

#[test]
fn the_namespace_and_the_key_format_are_frozen() {
    assert_eq!(
        STATEMENT_NAMESPACE,
        Uuid::parse_str("e21a7afd-0f46-4de7-8a8f-55241a97b9f5").unwrap()
    );

    let mut core = Core::open_in_memory().unwrap();
    let vault = Uuid::parse_str("0199a000-0000-7000-8000-000000000001").unwrap();
    let wallet = Uuid::parse_str("0199a000-0000-7000-8000-000000000002").unwrap();
    core.execute(CommandEnvelope {
        id: vault,
        vault_id: vault,
        author: "alice".to_string(),
        command: Command::CreateVault {
            name: "Pinned".to_string(),
            currency: Currency::Eur,
        },
    })
    .unwrap();
    core.execute(CommandEnvelope {
        id: wallet,
        vault_id: vault,
        author: "alice".to_string(),
        command: wallet_cmd("Card", 0),
    })
    .unwrap();
    let text = card_csv(&[spend(
        "2026-09-16 08:54:40 UTC",
        "FORNO   BIANCO           ",
        "CLEARED",
        "3.20",
    )]);
    let preview = core
        .preview_statement(vault, &text, &card_mapping(), &options(wallet))
        .unwrap();
    let key = format!("v1|{vault}|{wallet}|2026-09-16T08:54:40Z|card_spend|320|forno bianco|0");
    let id = row(&preview, 2).command_id;
    assert_eq!(id, Uuid::new_v5(&STATEMENT_NAMESPACE, key.as_bytes()));
    assert_eq!(
        id,
        Uuid::parse_str("90cfcd60-e888-5137-8699-d8a3270bb2e1").unwrap()
    );
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

#[test]
fn preview_writes_nothing() {
    let fx = setup();
    let before = command_count(&fx.core, fx.vault);
    fx.core
        .preview_statement(fx.vault, CARD, &card_mapping(), &options(fx.wallet))
        .unwrap();
    assert_eq!(command_count(&fx.core, fx.vault), before);
}

#[test]
fn unknown_wallets_and_flows_are_not_found() {
    let mut fx = setup();
    let stranger = Uuid::now_v7();
    let preview = |core: &Core, mapping: &StatementMapping, options: &StatementOptions| {
        core.preview_statement(fx.vault, CARD, mapping, options)
    };
    assert!(matches!(
        preview(&fx.core, &card_mapping(), &options(stranger)),
        Err(DomainError::NotFound(_))
    ));
    let with_flow = StatementOptions {
        flow_id: Some(stranger),
        ..options(fx.wallet)
    };
    assert!(matches!(
        preview(&fx.core, &card_mapping(), &with_flow),
        Err(DomainError::NotFound(_))
    ));
    assert!(matches!(
        preview(
            &fx.core,
            &card_mapping_with_topups_from(stranger),
            &options(fx.wallet)
        ),
        Err(DomainError::NotFound(_))
    ));
    assert!(matches!(
        preview(
            &fx.core,
            &card_mapping_with_topups_from(fx.wallet),
            &options(fx.wallet)
        ),
        Err(DomainError::InvalidCommand(_))
    ));
    assert!(matches!(
        fx.core.import_statement(
            stranger,
            "alice",
            CARD,
            &card_mapping(),
            &options(fx.wallet),
            &no_overrides()
        ),
        Err(DomainError::NotFound(_))
    ));
    let bad_zone = StatementOptions {
        timezone: "Mars/Olympus".to_string(),
        ..options(fx.wallet)
    };
    assert!(matches!(
        preview(&fx.core, &card_mapping(), &bad_zone),
        Err(DomainError::InvalidName(_))
    ));
}

#[test]
fn a_mapping_naming_a_missing_column_is_refused() {
    let fx = setup();
    let mut mapping = card_mapping();
    mapping.category_column = Some("merchant category".to_string());
    let err = fx
        .core
        .preview_statement(fx.vault, CARD, &mapping, &options(fx.wallet))
        .unwrap_err();
    assert!(
        matches!(&err, DomainError::InvalidCommand(m) if m.contains("merchant category")),
        "{err:?}"
    );
    let mut mapping = card_mapping();
    mapping.delimiter = ",;".to_string();
    assert!(matches!(
        fx.core
            .preview_statement(fx.vault, CARD, &mapping, &options(fx.wallet)),
        Err(DomainError::InvalidCommand(_))
    ));
}

#[test]
fn a_mapping_round_trips_through_json() {
    let preset = card_mapping();
    let json = statement::encode_mapping(&preset);
    assert_eq!(statement::decode_mapping(&json).unwrap(), preset);

    let custom = StatementMapping {
        date_format: StatementDateFormat::Custom {
            pattern: "%d %b %Y".to_string(),
        },
        type_column: Some("Tipo".to_string()),
        type_rules: vec![
            StatementTypeRule {
                value: "giroconto".to_string(),
                action: StatementAction::TransferOut {
                    to_wallet_id: Some(Uuid::now_v7()),
                },
            },
            StatementTypeRule {
                value: "ricarica".to_string(),
                action: StatementAction::TransferIn {
                    from_wallet_id: None,
                },
            },
        ],
        ..bank_mapping()
    };
    let json = statement::encode_mapping(&custom);
    assert_eq!(statement::decode_mapping(&json).unwrap(), custom);
    assert!(matches!(
        statement::decode_mapping("{\"delimiter\":"),
        Err(DomainError::InvalidCommand(_))
    ));
}
