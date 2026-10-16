//! Statement import: reading the CSV, the card preset, the preview and the
//! import with its deduplication. Every fixture is synthetic.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use chrono::{Datelike, Timelike};
use common::*;
use sparagne_core::statement::{self, STATEMENT_NAMESPACE};
use sparagne_core::{
    AmountSign, Command, CommandEnvelope, Core, Currency, DomainError, FlowMode, StatementAction,
    StatementDateFormat, StatementDescriptionRule, StatementMapping, StatementOptions,
    StatementPreview, StatementReport, StatementRow, StatementRowOverride, StatementRowStatus,
    StatementTypeRule, TransactionKind,
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
        description_rules: Vec::new(),
    }
}

fn options(wallet: Uuid) -> StatementOptions {
    StatementOptions {
        wallet_id: wallet,
        flow_id: None,
        timezone: "Europe/Rome".to_string(),
        also_check_wallets: Vec::new(),
        since: None,
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
        StatementRowStatus::Skipped { reason, .. } => reason,
        other => panic!("line {} is {other:?}, not skipped", row.line),
    }
}

fn skip_code(row: &StatementRow) -> &str {
    match &row.status {
        StatementRowStatus::Skipped { code, .. } => code,
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

const BANK_HEADER: &str = "Data operazione;Data valuta;Descrizione;Importo;Divisa";

/// A bank statement made of `rows`, header first, for [`bank_mapping`].
fn bank_csv(rows: &[String]) -> String {
    format!("{BANK_HEADER}\n{}\n", rows.join("\n"))
}

/// A bank row on `day` (`16/09/2026`), the amount as the bank writes it
/// (`-3,50` for money out).
fn bank_row(day: &str, description: &str, amount: &str) -> String {
    format!("{day};{day};{description};{amount};EUR")
}

/// An override of `line` that changes nothing yet.
fn user(line: u32) -> StatementRowOverride {
    StatementRowOverride {
        line,
        ..Default::default()
    }
}

fn preview_with(
    fx: &Fx,
    text: &str,
    mapping: &StatementMapping,
    options: &StatementOptions,
    overrides: &[StatementRowOverride],
) -> StatementPreview {
    fx.core
        .preview_statement(fx.vault, text, mapping, options, overrides)
        .unwrap()
}

fn import_with(
    fx: &mut Fx,
    text: &str,
    mapping: &StatementMapping,
    options: &StatementOptions,
    overrides: &[StatementRowOverride],
) -> StatementReport {
    fx.core
        .import_statement(fx.vault, "alice", text, mapping, options, overrides)
        .unwrap()
}

/// A wallet without money, archived.
fn archived_wallet(fx: &mut Fx, name: &str) -> Uuid {
    let id = run(&mut fx.core, fx.vault, wallet_cmd(name, 0))
        .result_id
        .unwrap();
    run(
        &mut fx.core,
        fx.vault,
        Command::ArchiveWallet { wallet_id: id },
    );
    id
}

/// A description rule that only matches.
fn rule(contains: &str) -> StatementDescriptionRule {
    StatementDescriptionRule {
        contains: contains.to_string(),
        action: None,
        category: None,
        note: None,
    }
}

fn new_flow(fx: &mut Fx, name: &str) -> Uuid {
    run(
        &mut fx.core,
        fx.vault,
        flow_cmd(name, FlowMode::Unlimited, true, 0),
    )
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
        .preview_statement(
            fx.vault,
            &text,
            &card_mapping(),
            &options(fx.wallet),
            &no_overrides(),
        )
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
                ..Default::default()
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
        .preview_statement(
            fx.vault,
            CARD,
            &card_mapping(),
            &options(fx.wallet),
            &no_overrides(),
        )
        .unwrap();
    let crlf = fx
        .core
        .preview_statement(
            fx.vault,
            &windows,
            &card_mapping(),
            &options(fx.wallet),
            &no_overrides(),
        )
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
        .preview_statement(fx.vault, BANK, &bank_mapping(), &options, &no_overrides())
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
    assert_eq!(skip_code(row(&preview, 5)), "zero_amount");
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
        .preview_statement(
            fx.vault,
            BANK,
            &bank_mapping(),
            &options(fx.wallet),
            &no_overrides(),
        )
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
        .preview_statement(fx.vault, BANK, &bank_mapping(), &abroad, &no_overrides())
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
        .preview_statement(
            fx.vault,
            &text,
            &card_mapping(),
            &options(fx.wallet),
            &no_overrides(),
        )
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
        .preview_statement(
            fx.vault,
            text,
            &bank_mapping(),
            &options(fx.wallet),
            &no_overrides(),
        )
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
            fx.core.preview_statement(
                fx.vault,
                text,
                &card_mapping(),
                &options(fx.wallet),
                &no_overrides()
            ),
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
        .preview_statement(
            fx.vault,
            CARD,
            &card_mapping(),
            &options(fx.wallet),
            &no_overrides(),
        )
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
        .preview_statement(
            fx.vault,
            &text,
            &card_mapping(),
            &options(fx.wallet),
            &no_overrides(),
        )
        .unwrap();
    assert_eq!(skip_reason(row(&preview, 2)), "status PENDING is skipped");
    assert_eq!(skip_reason(row(&preview, 3)), "status CANCELLED is skipped");
    assert_eq!(skip_reason(row(&preview, 4)), "status declined is skipped");
    for line in 2..=4 {
        assert_eq!(skip_code(row(&preview, line)), "skipped_status");
    }
    assert_eq!(row(&preview, 5).status, StatementRowStatus::New);
    assert_eq!(row(&preview, 5).kind, Some(TransactionKind::Expense));
    assert_eq!((preview.new_rows, preview.skipped), (1, 3));
}

#[test]
fn the_card_fixture_previews_row_by_row() {
    let fx = setup();
    let preview = fx
        .core
        .preview_statement(
            fx.vault,
            CARD,
            &card_mapping(),
            &options(fx.wallet),
            &no_overrides(),
        )
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
        .preview_statement(
            fx.vault,
            CARD,
            &card_mapping(),
            &options(fx.wallet),
            &no_overrides(),
        )
        .unwrap();
    let topup = row(&preview, 8);
    assert_eq!(skip_reason(topup), "choose the wallet the money came from");
    assert_eq!(skip_code(topup), "needs_wallet");
    assert_eq!(topup.kind, Some(TransactionKind::TransferWallet));
    assert_eq!(topup.counter_wallet_id, None);

    let mapping = card_mapping_with_topups_from(bank);
    let preview = fx
        .core
        .preview_statement(
            fx.vault,
            CARD,
            &mapping,
            &options(fx.wallet),
            &no_overrides(),
        )
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
        .preview_statement(
            fx.vault,
            CARD,
            &card_mapping(),
            &options(fx.wallet),
            &no_overrides(),
        )
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
        .preview_statement(
            fx.vault,
            CARD,
            &card_mapping(),
            &options(fx.wallet),
            &no_overrides(),
        )
        .unwrap();
    let deposit = row(&preview, 9);
    assert_eq!(skip_reason(deposit), "type liquid_deposit is skipped");
    assert_eq!(skip_code(deposit), "skipped_by_rule");
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
        .preview_statement(
            fx.vault,
            &text,
            &card_mapping(),
            &options(fx.wallet),
            &no_overrides(),
        )
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
        .preview_statement(
            fx.vault,
            CARD,
            &card_mapping(),
            &options(fx.wallet),
            &no_overrides(),
        )
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
        .preview_statement(
            fx.vault,
            &text,
            &card_mapping(),
            &options(fx.wallet),
            &no_overrides(),
        )
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
        .preview_statement(
            fx.vault,
            &text,
            &card_mapping(),
            &options(fx.wallet),
            &no_overrides(),
        )
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
        .preview_statement(
            fx.vault,
            &text,
            &card_mapping(),
            &options(fx.wallet),
            &no_overrides(),
        )
        .unwrap();
    assert_eq!(invalid_code(row(&preview, 2)), "invalid_date");
    assert_eq!(invalid_code(row(&preview, 3)), "invalid_amount");
}

#[test]
fn rows_before_the_start_day_are_skipped_and_keep_their_ids() {
    let mut fx = setup();
    let plain = preview_with(
        &fx,
        BANK,
        &bank_mapping(),
        &options(fx.wallet),
        &no_overrides(),
    );
    let options = StatementOptions {
        since: Some(day(2026, 9, 15)),
        ..options(fx.wallet)
    };
    let preview = preview_with(&fx, BANK, &bank_mapping(), &options, &no_overrides());
    assert_eq!(row(&preview, 2).status, StatementRowStatus::New);
    assert_eq!(row(&preview, 3).status, StatementRowStatus::New);
    // The zero amount on line 5 is before the start too.
    for line in [4, 5] {
        assert_eq!(skip_code(row(&preview, line)), "before_start");
    }
    assert_eq!(
        skip_reason(row(&preview, 4)),
        "dated before 2026-09-15, the day the import starts"
    );
    for line in 2..=5 {
        assert_eq!(row(&preview, line).command_id, row(&plain, line).command_id);
    }
    assert_eq!((preview.new_rows, preview.skipped), (2, 2));
    let report = import_with(&mut fx, BANK, &bank_mapping(), &options, &no_overrides());
    assert_eq!((report.executed, report.skipped), (2, 2));
}

#[test]
fn the_start_day_is_read_in_the_rows_own_offset_before_its_currency() {
    let fx = setup();
    let text = card_csv(&[
        spend("2026-08-31 23:30:00 UTC", "NIGHT BUS", "CLEARED", "2.00"),
        spend("2026-08-31 21:30:00 UTC", "LATE BAR", "CLEARED", "4.00"),
        spend("31/08/2026 08:00", "BAR", "CLEARED", "1.20"),
        card_row(
            "2026-08-30 10:00:00 UTC",
            "card_spend",
            "DINER",
            "CLEARED",
            "7.45",
            "USD",
        ),
        card_row(
            "2026-09-02 10:00:00 UTC",
            "card_spend",
            "DINER",
            "CLEARED",
            "7.45",
            "USD",
        ),
    ]);
    let options = StatementOptions {
        since: Some(day(2026, 9, 1)),
        ..options(fx.wallet)
    };
    let preview = preview_with(&fx, &text, &card_mapping(), &options, &no_overrides());
    assert_eq!(
        row(&preview, 2).status,
        StatementRowStatus::New,
        "1 September at 01:30 in Rome"
    );
    assert_eq!(skip_code(row(&preview, 3)), "before_start");
    assert_eq!(invalid_code(row(&preview, 4)), "invalid_date");
    assert_eq!(skip_code(row(&preview, 5)), "before_start");
    assert_eq!(invalid_code(row(&preview, 6)), "currency_mismatch");
}

#[test]
fn the_wallets_also_checked_must_be_the_vaults() {
    let mut fx = setup();
    let old = archived_wallet(&mut fx, "Old");
    let checked = |wallets: Vec<Uuid>| StatementOptions {
        also_check_wallets: wallets,
        ..options(fx.wallet)
    };
    let preview = |options: &StatementOptions| {
        fx.core
            .preview_statement(fx.vault, BANK, &bank_mapping(), options, &no_overrides())
    };
    assert!(matches!(
        preview(&checked(vec![old, Uuid::now_v7()])),
        Err(DomainError::NotFound(_))
    ));
    // Archived is fine, the imported wallet itself is ignored.
    let preview = preview(&checked(vec![old, fx.wallet])).unwrap();
    assert_eq!(preview.new_rows, 3);
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
        .preview_statement(
            fx.vault,
            CARD,
            &card_mapping(),
            &options(fx.wallet),
            &no_overrides(),
        )
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
        .preview_statement(
            fx.vault,
            CARD,
            &card_mapping(),
            &options(fx.wallet),
            &no_overrides(),
        )
        .unwrap();
    let overrides = [
        StatementRowOverride {
            line: 3,
            category: Some("Libri".to_string()),
            note: None,
            skip: false,
            ..Default::default()
        },
        StatementRowOverride {
            line: 4,
            category: None,
            note: Some("Flight to Lisbon".to_string()),
            skip: false,
            ..Default::default()
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
        .preview_statement(
            fx.vault,
            CARD,
            &card_mapping(),
            &options(fx.wallet),
            &no_overrides(),
        )
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
                ..Default::default()
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
        .preview_statement(
            fx.vault,
            CARD,
            &card_mapping(),
            &options(fx.wallet),
            &no_overrides(),
        )
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
                ..Default::default()
            }],
        )
        .unwrap();
    assert_eq!(report.executed, 4);
    assert_eq!(report.rejected.len(), 1);
    assert_eq!(report.rejected[0].line, 10);
    assert_eq!(report.rejected[0].code, "invalid_name");
}

// ---------------------------------------------------------------------------
// What the user says a row is
// ---------------------------------------------------------------------------

#[test]
fn an_action_override_changes_what_the_row_is_in_the_preview_and_the_import() {
    let mut fx = setup();
    let savings = run(&mut fx.core, fx.vault, wallet_cmd("Savings", 0))
        .result_id
        .unwrap();
    let overrides = [
        StatementRowOverride {
            action: Some(StatementAction::Refund),
            ..user(2)
        },
        StatementRowOverride {
            action: Some(StatementAction::Skip),
            ..user(3)
        },
        StatementRowOverride {
            action: Some(StatementAction::TransferOut {
                to_wallet_id: Some(savings),
            }),
            ..user(4)
        },
    ];
    let options = options(fx.wallet);
    let plain = preview_with(&fx, BANK, &bank_mapping(), &options, &no_overrides());
    let preview = preview_with(&fx, BANK, &bank_mapping(), &options, &overrides);

    let pos = row(&preview, 2);
    assert_eq!(pos.action, StatementAction::Refund);
    assert_eq!(pos.kind, Some(TransactionKind::Refund));
    assert_eq!(pos.status, StatementRowStatus::New);
    let salary = row(&preview, 3);
    assert_eq!(salary.action, StatementAction::Skip);
    assert_eq!(salary.kind, None);
    assert_eq!(skip_code(salary), "skipped_by_you");
    assert_eq!(skip_reason(salary), "skipped by you");
    let transfer = row(&preview, 4);
    assert_eq!(transfer.kind, Some(TransactionKind::TransferWallet));
    assert_eq!(transfer.counter_wallet_id, Some(savings));
    assert_eq!((preview.new_rows, preview.skipped), (2, 2));
    // What the row is never changes its id.
    for line in 2..=5 {
        assert_eq!(row(&preview, line).command_id, row(&plain, line).command_id);
    }

    let report = import_with(&mut fx, BANK, &bank_mapping(), &options, &overrides);
    assert_eq!((report.executed, report.skipped), (2, 2));
    let refund = fx.core.transaction(fx.vault, pos.command_id).unwrap();
    assert_eq!(refund.kind, TransactionKind::Refund);
    assert_eq!(refund.amount, 350);
    let moved = fx.core.transaction(fx.vault, transfer.command_id).unwrap();
    assert_eq!(
        (moved.kind, moved.from_id, moved.to_id),
        (
            TransactionKind::TransferWallet,
            Some(fx.wallet),
            Some(savings)
        )
    );
    assert_eq!(wallet_balance(&fx.core, fx.vault, fx.wallet), 350 - 12_000);
    assert_eq!(wallet_balance(&fx.core, fx.vault, savings), 12_000);
}

#[test]
fn a_transfer_override_without_a_usable_wallet_waits_for_one() {
    let mut fx = setup();
    let old = archived_wallet(&mut fx, "Old");
    let stranger = Uuid::now_v7();
    let text = bank_csv(&[
        bank_row("16/09/2026", "GIROCONTO", "-10,00"),
        bank_row("15/09/2026", "GIROCONTO", "-11,00"),
        bank_row("14/09/2026", "GIROCONTO", "12,00"),
        bank_row("13/09/2026", "GIROCONTO", "-13,00"),
    ]);
    let overrides = [
        StatementRowOverride {
            action: Some(StatementAction::TransferOut { to_wallet_id: None }),
            ..user(2)
        },
        StatementRowOverride {
            action: Some(StatementAction::TransferOut {
                to_wallet_id: Some(fx.wallet),
            }),
            ..user(3)
        },
        StatementRowOverride {
            action: Some(StatementAction::TransferIn {
                from_wallet_id: Some(old),
            }),
            ..user(4)
        },
        StatementRowOverride {
            action: Some(StatementAction::TransferOut {
                to_wallet_id: Some(stranger),
            }),
            ..user(5)
        },
    ];
    let options = options(fx.wallet);
    let preview = preview_with(&fx, &text, &bank_mapping(), &options, &overrides);
    for line in 2..=5 {
        assert_eq!(
            skip_code(row(&preview, line)),
            "needs_wallet",
            "line {line}"
        );
        assert_eq!(
            row(&preview, line).kind,
            Some(TransactionKind::TransferWallet)
        );
    }
    assert_eq!(
        skip_reason(row(&preview, 2)),
        "choose the wallet the money went to"
    );
    assert_eq!(
        skip_reason(row(&preview, 4)),
        "the wallet the money came from is archived: choose another"
    );
    let report = import_with(&mut fx, &text, &bank_mapping(), &options, &overrides);
    assert_eq!((report.executed, report.skipped), (0, 4));
}

#[test]
fn an_override_can_import_a_row_a_type_rule_skips() {
    let mut fx = setup();
    let text = card_csv(&[card_row(
        "2026-09-11 09:30:00 UTC",
        "liquid_deposit",
        "Liquid deposit",
        "",
        "50.00",
        "EUR",
    )]);
    let options = options(fx.wallet);
    let overrides = [StatementRowOverride {
        action: Some(StatementAction::Income),
        ..user(2)
    }];
    let plain = preview_with(&fx, &text, &card_mapping(), &options, &no_overrides());
    assert_eq!(skip_code(row(&plain, 2)), "skipped_by_rule");
    let preview = preview_with(&fx, &text, &card_mapping(), &options, &overrides);
    assert_eq!(row(&preview, 2).status, StatementRowStatus::New);
    assert_eq!(row(&preview, 2).kind, Some(TransactionKind::Income));
    let report = import_with(&mut fx, &text, &card_mapping(), &options, &overrides);
    assert_eq!(report.executed, 1);
    assert_eq!(wallet_balance(&fx.core, fx.vault, fx.wallet), 5_000);
}

#[test]
fn a_flow_override_picks_the_envelope_and_an_unusable_one_is_ignored() {
    let mut fx = setup();
    let casa = new_flow(&mut fx, "Casa");
    let viaggi = new_flow(&mut fx, "Viaggi");
    let gone = new_flow(&mut fx, "Gone");
    run(
        &mut fx.core,
        fx.vault,
        Command::ArchiveFlow { flow_id: gone },
    );
    let text = bank_csv(&[
        bank_row("16/09/2026", "BAR", "-3,50"),
        bank_row("15/09/2026", "STIPENDIO", "1850,00"),
        bank_row("14/09/2026", "EDICOLA", "-2,00"),
        bank_row("13/09/2026", "CINEMA", "-9,00"),
    ]);
    let overrides = [
        StatementRowOverride {
            flow_id: Some(viaggi),
            ..user(2)
        },
        StatementRowOverride {
            flow_id: Some(fx.unallocated),
            ..user(3)
        },
        StatementRowOverride {
            flow_id: Some(Uuid::now_v7()),
            ..user(4)
        },
        StatementRowOverride {
            flow_id: Some(gone),
            ..user(5)
        },
    ];
    let options = StatementOptions {
        flow_id: Some(casa),
        ..options(fx.wallet)
    };
    let preview = preview_with(&fx, &text, &bank_mapping(), &options, &overrides);
    assert_eq!(preview.new_rows, 4);
    let report = import_with(&mut fx, &text, &bank_mapping(), &options, &overrides);
    assert_eq!(report.executed, 4);
    let flow = |line| {
        fx.core
            .transaction(fx.vault, row(&preview, line).command_id)
            .unwrap()
            .flow_id
    };
    assert_eq!(flow(2), Some(viaggi));
    assert_eq!(flow(3), Some(fx.unallocated));
    assert_eq!(flow(4), Some(casa), "an unknown envelope is ignored");
    assert_eq!(flow(5), Some(casa), "an archived envelope is ignored");
}

#[test]
fn overrides_change_nothing_on_rows_already_imported_or_invalid() {
    let mut fx = setup();
    let savings = run(&mut fx.core, fx.vault, wallet_cmd("Savings", 0))
        .result_id
        .unwrap();
    let text = card_csv(&[
        spend("2026-09-16 08:00:00 UTC", "BAR", "CLEARED", "1.20"),
        spend("16/09/2026 08:00", "BAR", "CLEARED", "1.30"),
        spend("2026-09-16 06:00:00 UTC", "BAR", "CLEARED", "1.2.0"),
        "2026-09-15 08:00:00 UTC,card_spend,TOO SHORT,CLEARED,2.00".to_string(),
    ]);
    let options = options(fx.wallet);
    let first = import_with(&mut fx, &text, &card_mapping(), &options, &no_overrides());
    assert_eq!(first.executed, 1);

    let everything = |line| StatementRowOverride {
        line,
        category: Some("Libri".to_string()),
        note: Some("mine".to_string()),
        skip: true,
        action: Some(StatementAction::TransferOut {
            to_wallet_id: Some(savings),
        }),
        flow_id: Some(fx.unallocated),
        on_match: None,
    };
    let overrides: Vec<_> = (2..=5).map(everything).collect();
    let plain = preview_with(&fx, &text, &card_mapping(), &options, &no_overrides());
    let preview = preview_with(&fx, &text, &card_mapping(), &options, &overrides);
    assert_eq!(preview, plain);
    assert_eq!(row(&preview, 2).status, StatementRowStatus::AlreadyImported);
    assert_eq!(invalid_code(row(&preview, 3)), "invalid_date");
    assert_eq!(invalid_code(row(&preview, 4)), "invalid_amount");
    assert_eq!(invalid_code(row(&preview, 5)), "invalid_row");

    let commands = command_count(&fx.core, fx.vault);
    let again = import_with(&mut fx, &text, &card_mapping(), &options, &overrides);
    assert_eq!((again.executed, again.deduplicated), (0, 1));
    assert_eq!(command_count(&fx.core, fx.vault), commands);
}

#[test]
fn a_row_imported_before_stays_imported_whatever_the_user_now_says() {
    let mut fx = setup();
    let savings = run(&mut fx.core, fx.vault, wallet_cmd("Savings", 0))
        .result_id
        .unwrap();
    let options = options(fx.wallet);
    let first = import_with(&mut fx, BANK, &bank_mapping(), &options, &no_overrides());
    assert_eq!(first.executed, 3);
    let commands = command_count(&fx.core, fx.vault);
    let overrides = [
        StatementRowOverride {
            action: Some(StatementAction::TransferOut {
                to_wallet_id: Some(savings),
            }),
            ..user(4)
        },
        StatementRowOverride {
            category: Some("Bar".to_string()),
            note: Some("coffee".to_string()),
            ..user(2)
        },
    ];
    let again = import_with(&mut fx, BANK, &bank_mapping(), &options, &overrides);
    assert_eq!((again.executed, again.deduplicated), (0, 3));
    assert_eq!(command_count(&fx.core, fx.vault), commands);
}

// ---------------------------------------------------------------------------
// Description rules
// ---------------------------------------------------------------------------

#[test]
fn a_description_rule_matches_whatever_the_case_and_the_spaces() {
    let fx = setup();
    let mapping = StatementMapping {
        description_rules: vec![StatementDescriptionRule {
            category: Some(" Bar ".to_string()),
            ..rule("  bar   CENTRALE  milano ")
        }],
        ..bank_mapping()
    };
    let preview = preview_with(&fx, BANK, &mapping, &options(fx.wallet), &no_overrides());
    let pos = row(&preview, 2);
    assert_eq!(pos.description_rule, Some(0));
    assert_eq!(pos.category.as_deref(), Some("Bar"));
    assert_eq!(pos.matched_category, None);
    assert_eq!(pos.note, pos.payee, "a rule without a note keeps the payee");
    assert_eq!(pos.kind, Some(TransactionKind::Expense));
    assert_eq!(row(&preview, 3).description_rule, None);
    assert_eq!(row(&preview, 3).category, None);
}

#[test]
fn the_first_rule_that_matches_decides_on_its_own_and_a_blank_one_is_none() {
    let fx = setup();
    let mapping = StatementMapping {
        description_rules: vec![
            // Being typed: neither its wallet nor its category fail the
            // preview, and it matches nothing.
            StatementDescriptionRule {
                action: Some(StatementAction::TransferOut {
                    to_wallet_id: Some(fx.wallet),
                }),
                category: Some("!!!".to_string()),
                ..rule("   ")
            },
            StatementDescriptionRule {
                note: Some("Salary".to_string()),
                ..rule("stipendio")
            },
            StatementDescriptionRule {
                action: Some(StatementAction::Expense),
                category: Some("Altro".to_string()),
                note: Some("Something else".to_string()),
                ..rule("accredito")
            },
        ],
        ..bank_mapping()
    };
    let preview = preview_with(&fx, BANK, &mapping, &options(fx.wallet), &no_overrides());
    let salary = row(&preview, 3);
    assert_eq!(salary.description_rule, Some(1));
    assert_eq!(salary.note, "Salary");
    assert_eq!(salary.category, None, "the second rule adds nothing");
    assert_eq!(salary.kind, Some(TransactionKind::Income));
    assert_eq!(salary.action, StatementAction::BySign);
    assert!(
        preview
            .rows
            .iter()
            .all(|row| row.description_rule != Some(0))
    );
}

#[test]
fn a_rule_without_an_action_keeps_the_type_rule_and_cleans_the_note() {
    let mut fx = setup();
    let mut mapping = card_mapping();
    mapping.description_rules = vec![StatementDescriptionRule {
        note: Some("Flight to Lisbon".to_string()),
        ..rule("skyline")
    }];
    let options = options(fx.wallet);
    let preview = preview_with(&fx, CARD, &mapping, &options, &no_overrides());
    let flight = row(&preview, 4);
    assert_eq!(flight.action, StatementAction::Expense);
    assert_eq!(flight.kind, Some(TransactionKind::Expense));
    assert_eq!(flight.note, "Flight to Lisbon");
    assert_eq!(flight.payee, "SKYLINE AIR");
    import_with(&mut fx, CARD, &mapping, &options, &no_overrides());
    let tx = fx.core.transaction(fx.vault, flight.command_id).unwrap();
    assert_eq!(tx.note.as_deref(), Some("Flight to Lisbon (12.34 USD)"));
}

#[test]
fn a_rule_category_is_created_and_the_user_has_the_last_word() {
    let mut fx = setup();
    run(
        &mut fx.core,
        fx.vault,
        Command::CreateCategory {
            name: "Bakeries".to_string(),
        },
    );
    let mut mapping = card_mapping();
    mapping.description_rules = vec![
        StatementDescriptionRule {
            category: Some("Libri".to_string()),
            note: Some("Books".to_string()),
            ..rule("books")
        },
        StatementDescriptionRule {
            category: Some("Pane".to_string()),
            ..rule("forno")
        },
        StatementDescriptionRule {
            category: Some("Frutta".to_string()),
            note: Some("Fruit".to_string()),
            ..rule("grocer")
        },
    ];
    let options = options(fx.wallet);
    let overrides = [StatementRowOverride {
        category: Some("Spesa".to_string()),
        note: Some("Vegetables".to_string()),
        ..user(10)
    }];
    let preview = preview_with(&fx, CARD, &mapping, &options, &overrides);
    let bakery = row(&preview, 5);
    assert_eq!(bakery.matched_category.as_deref(), Some("Bakeries"));
    assert_eq!(bakery.category.as_deref(), Some("Pane"));
    // What the row gets before the user's own.
    assert_eq!(row(&preview, 10).category.as_deref(), Some("Frutta"));
    assert_eq!(row(&preview, 10).note, "Fruit");

    import_with(&mut fx, CARD, &mapping, &options, &overrides);
    let names: Vec<String> = fx
        .core
        .categories(fx.vault, false)
        .unwrap()
        .into_iter()
        .map(|c| c.name)
        .collect();
    assert!(names.contains(&"Libri".to_string()), "{names:?}");
    assert!(!names.contains(&"Frutta".to_string()), "{names:?}");
    let tx = |line| {
        fx.core
            .transaction(fx.vault, row(&preview, line).command_id)
            .unwrap()
    };
    assert_eq!(
        (tx(3).category, tx(3).note),
        ("Libri".to_string(), Some("Books".to_string()))
    );
    assert_eq!(tx(5).category, "Pane");
    assert_eq!(
        (tx(10).category, tx(10).note),
        ("Spesa".to_string(), Some("Vegetables".to_string()))
    );
}

#[test]
fn a_skip_rule_names_its_text_and_the_user_can_still_import_the_row() {
    let mut fx = setup();
    let mapping = StatementMapping {
        description_rules: vec![StatementDescriptionRule {
            action: Some(StatementAction::Skip),
            ..rule(" Bonifico   a ")
        }],
        ..bank_mapping()
    };
    let options = options(fx.wallet);
    let preview = preview_with(&fx, BANK, &mapping, &options, &no_overrides());
    let wire = row(&preview, 4);
    assert_eq!(skip_code(wire), "skipped_by_rule");
    assert_eq!(
        skip_reason(wire),
        "description contains \"Bonifico a\" is skipped"
    );
    assert_eq!(wire.action, StatementAction::Skip);
    assert_eq!(wire.kind, None);

    let overrides = [StatementRowOverride {
        action: Some(StatementAction::Expense),
        ..user(4)
    }];
    let preview = preview_with(&fx, BANK, &mapping, &options, &overrides);
    assert_eq!(row(&preview, 4).status, StatementRowStatus::New);
    let report = import_with(&mut fx, BANK, &mapping, &options, &overrides);
    assert_eq!(report.executed, 3);
}

#[test]
fn a_rule_naming_the_imported_wallet_an_unknown_one_or_a_bad_category_is_refused() {
    let fx = setup();
    let with = |rule: StatementDescriptionRule| StatementMapping {
        description_rules: vec![rule],
        ..bank_mapping()
    };
    let preview = |mapping: &StatementMapping| {
        fx.core.preview_statement(
            fx.vault,
            BANK,
            mapping,
            &options(fx.wallet),
            &no_overrides(),
        )
    };
    let to = |wallet| StatementDescriptionRule {
        action: Some(StatementAction::TransferOut {
            to_wallet_id: Some(wallet),
        }),
        ..rule("giroconto")
    };
    assert!(matches!(
        preview(&with(to(Uuid::now_v7()))),
        Err(DomainError::NotFound(_))
    ));
    assert!(matches!(
        preview(&with(to(fx.wallet))),
        Err(DomainError::InvalidCommand(_))
    ));
    assert!(matches!(
        preview(&with(StatementDescriptionRule {
            category: Some("!!!".to_string()),
            ..rule("giroconto")
        })),
        Err(DomainError::InvalidName(_))
    ));
}

#[test]
fn a_row_imported_before_a_rule_existed_stays_imported_after_it() {
    let mut fx = setup();
    let savings = run(&mut fx.core, fx.vault, wallet_cmd("Savings", 0))
        .result_id
        .unwrap();
    let options = options(fx.wallet);
    let before = preview_with(&fx, BANK, &bank_mapping(), &options, &no_overrides());
    import_with(&mut fx, BANK, &bank_mapping(), &options, &no_overrides());
    let commands = command_count(&fx.core, fx.vault);

    let mapping = StatementMapping {
        description_rules: vec![
            StatementDescriptionRule {
                action: Some(StatementAction::TransferOut {
                    to_wallet_id: Some(savings),
                }),
                note: Some("To savings".to_string()),
                ..rule("bonifico")
            },
            StatementDescriptionRule {
                action: Some(StatementAction::Refund),
                ..rule("bar")
            },
        ],
        ..bank_mapping()
    };
    let after = preview_with(&fx, BANK, &mapping, &options, &no_overrides());
    for line in 2..=5 {
        assert_eq!(
            row(&after, line).command_id,
            row(&before, line).command_id,
            "line {line}"
        );
    }
    assert_eq!(row(&after, 4).status, StatementRowStatus::AlreadyImported);
    assert_eq!(row(&after, 4).kind, Some(TransactionKind::TransferWallet));
    assert_eq!((after.new_rows, after.already_imported), (0, 3));
    let report = import_with(&mut fx, BANK, &mapping, &options, &no_overrides());
    assert_eq!((report.executed, report.deduplicated), (0, 3));
    assert_eq!(command_count(&fx.core, fx.vault), commands);
}

#[test]
fn a_mapping_saved_before_description_rules_reads_back_and_is_written_as_it_was() {
    let saved = r#"{"delimiter":";","date_column":"Data operazione","date_format":{"kind":"day_month_year"},"amount_column":"Importo","amount_sign":"outflow_negative","decimal_comma":true,"description_columns":["Descrizione"],"type_column":null,"type_rules":[],"default_action":{"kind":"by_sign"},"status_column":null,"skip_statuses":[],"currency_column":"Divisa","category_column":null,"original_amount_column":null,"original_currency_column":null}"#;
    assert_eq!(statement::decode_mapping(saved).unwrap(), bank_mapping());
    assert_eq!(statement::encode_mapping(&bank_mapping()), saved);
    assert!(!statement::encode_mapping(&card_mapping()).contains("description_rules"));

    let ruled = StatementMapping {
        description_rules: vec![
            StatementDescriptionRule {
                action: Some(StatementAction::TransferIn {
                    from_wallet_id: Some(Uuid::now_v7()),
                }),
                category: Some("Giroconti".to_string()),
                note: Some("From the bank".to_string()),
                ..rule("giroconto")
            },
            rule("bar"),
        ],
        ..bank_mapping()
    };
    let json = statement::encode_mapping(&ruled);
    assert!(json.contains(r#"{"contains":"bar"}"#), "{json}");
    assert_eq!(statement::decode_mapping(&json).unwrap(), ruled);
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
        .preview_statement(
            fx.vault,
            CARD,
            &card_mapping(),
            &options(fx.wallet),
            &no_overrides(),
        )
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
        .preview_statement(
            fx.vault,
            &second_export,
            &mapping,
            &options(fx.wallet),
            &no_overrides(),
        )
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
        .preview_statement(
            fx.vault,
            &text,
            &card_mapping(),
            &options(fx.wallet),
            &no_overrides(),
        )
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
        .preview_statement(
            vault,
            &text,
            &card_mapping(),
            &options(wallet),
            &no_overrides(),
        )
        .unwrap();
    let key = format!("v1|{vault}|{wallet}|2026-09-16T08:54:40Z|card_spend|320|forno bianco|0");
    let id = row(&preview, 2).command_id;
    assert_eq!(id, Uuid::new_v5(&STATEMENT_NAMESPACE, key.as_bytes()));
    assert_eq!(
        id,
        Uuid::parse_str("90cfcd60-e888-5137-8699-d8a3270bb2e1").unwrap()
    );
}

/// Without a type column the key holds the kind the type rules and the
/// default action give the row (here `BySign`), never what a description rule
/// or the user later makes of it: these ids were handed out before either
/// existed.
#[test]
fn the_key_without_a_type_column_is_frozen() {
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
        command: wallet_cmd("Bank", 0),
    })
    .unwrap();
    let preview = core
        .preview_statement(
            vault,
            BANK,
            &bank_mapping(),
            &options(wallet),
            &no_overrides(),
        )
        .unwrap();
    for (line, key, pinned) in [
        (
            2,
            format!(
                "v1|{vault}|{wallet}|2026-09-16|expense|-350|pagamento pos; bar centrale milano|0"
            ),
            "52b10083-f809-5eb2-9500-d7d80792fff5",
        ),
        (
            3,
            format!("v1|{vault}|{wallet}|2026-09-15|income|185000|accredito stipendio settembre|0"),
            "d2f9394b-a006-5482-97dc-d232c63d83a7",
        ),
    ] {
        let id = row(&preview, line).command_id;
        assert_eq!(
            id,
            Uuid::new_v5(&STATEMENT_NAMESPACE, key.as_bytes()),
            "line {line}"
        );
        assert_eq!(id, Uuid::parse_str(pinned).unwrap(), "line {line}");
    }
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

#[test]
fn preview_writes_nothing() {
    let fx = setup();
    let before = command_count(&fx.core, fx.vault);
    fx.core
        .preview_statement(
            fx.vault,
            CARD,
            &card_mapping(),
            &options(fx.wallet),
            &no_overrides(),
        )
        .unwrap();
    assert_eq!(command_count(&fx.core, fx.vault), before);
}

#[test]
fn unknown_wallets_and_flows_are_not_found() {
    let mut fx = setup();
    let stranger = Uuid::now_v7();
    let preview = |core: &Core, mapping: &StatementMapping, options: &StatementOptions| {
        core.preview_statement(fx.vault, CARD, mapping, options, &no_overrides())
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
        .preview_statement(
            fx.vault,
            CARD,
            &mapping,
            &options(fx.wallet),
            &no_overrides(),
        )
        .unwrap_err();
    assert!(
        matches!(&err, DomainError::InvalidCommand(m) if m.contains("merchant category")),
        "{err:?}"
    );
    let mut mapping = card_mapping();
    mapping.delimiter = ",;".to_string();
    assert!(matches!(
        fx.core.preview_statement(
            fx.vault,
            CARD,
            &mapping,
            &options(fx.wallet),
            &no_overrides()
        ),
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
