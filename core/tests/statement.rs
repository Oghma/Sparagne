//! Statement import: reading the CSV, the card preset, the preview and the
//! import with its deduplication. Every fixture is synthetic.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use chrono::{DateTime, Datelike, FixedOffset, TimeZone, Timelike};
use common::*;
use sparagne_core::statement::{self, STATEMENT_NAMESPACE};
use sparagne_core::{
    AmountSign, Command, CommandEnvelope, Core, Currency, DomainError, Entry, FlowMode,
    StatementAction, StatementDateFormat, StatementDescriptionRule, StatementMapping,
    StatementMatchChoice, StatementOptions, StatementPreview, StatementReport, StatementRow,
    StatementRowOverride, StatementRowStatus, StatementTypeRule, TransactionKind,
};
use uuid::Uuid;

const CARD: &str = include_str!("fixtures/statement/card.csv");
const BANK: &str = include_str!("fixtures/statement/bank.csv");
/// A bank's export holding a transfer out to another bank of the user's.
const GIROCONTO_OUT: &str = include_str!("fixtures/statement/giroconto_out.csv");
/// The other bank's export, holding the same money coming in two days later.
const GIROCONTO_IN: &str = include_str!("fixtures/statement/giroconto_in.csv");
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

/// Noon in Rome on `day` September 2026, where the bank rows sit.
fn september(day: u32) -> DateTime<FixedOffset> {
    noon_in_rome(9, day)
}

/// Noon in Rome (summer time) on `day` of `month` 2026.
fn noon_in_rome(month: u32, day: u32) -> DateTime<FixedOffset> {
    FixedOffset::east_opt(2 * 3600)
        .unwrap()
        .with_ymd_and_hms(2026, month, day, 12, 0, 0)
        .unwrap()
}

/// An entry typed by hand into `wallet` on `day` September.
fn by_hand(fx: &mut Fx, kind: TransactionKind, wallet: Uuid, amount: i64, day: u32) -> Uuid {
    typed_on(fx, kind, wallet, amount, (9, day))
}

/// An entry typed by hand into `wallet` on `(month, day)` 2026.
fn typed_on(
    fx: &mut Fx,
    kind: TransactionKind,
    wallet: Uuid,
    amount: i64,
    (month, day): (u32, u32),
) -> Uuid {
    let entry = Entry {
        amount,
        wallet_id: Some(wallet),
        flow_id: None,
        category: None,
        note: Some(format!("typed on the {day}")),
        occurred_at: noon_in_rome(month, day),
        person: None,
    };
    let command = match kind {
        TransactionKind::Income => Command::Income(entry),
        TransactionKind::Refund => Command::Refund(entry),
        _ => Command::Expense(entry),
    };
    run(&mut fx.core, fx.vault, command).result_id.unwrap()
}

/// A transfer typed by hand on `day` September.
fn transfer_by_hand(fx: &mut Fx, from: Uuid, to: Uuid, amount: i64, day: u32) -> Uuid {
    transfer_on(fx, (from, to), amount, (9, day))
}

/// A transfer typed by hand on `(month, day)` 2026.
fn transfer_on(
    fx: &mut Fx,
    (from, to): (Uuid, Uuid),
    amount: i64,
    (month, day): (u32, u32),
) -> Uuid {
    run(
        &mut fx.core,
        fx.vault,
        Command::TransferWallet {
            amount,
            from_wallet_id: from,
            to_wallet_id: to,
            note: None,
            occurred_at: noon_in_rome(month, day),
        },
    )
    .result_id
    .unwrap()
}

/// Two bank wallets without money.
fn two_banks(fx: &mut Fx) -> (Uuid, Uuid) {
    let mut open = |name| {
        run(&mut fx.core, fx.vault, wallet_cmd(name, 0))
            .result_id
            .unwrap()
    };
    (open("First Bank"), open("Second Bank"))
}

/// [`bank_mapping`] with its `giroconto` rows moving money out to `to`.
fn giroconto_to(to: Uuid) -> StatementMapping {
    StatementMapping {
        description_rules: vec![StatementDescriptionRule {
            action: Some(StatementAction::TransferOut {
                to_wallet_id: Some(to),
            }),
            ..rule("giroconto")
        }],
        ..bank_mapping()
    }
}

/// [`bank_mapping`] with its `giroconto` rows bringing money in from `from`.
fn giroconto_from(from: Uuid) -> StatementMapping {
    StatementMapping {
        description_rules: vec![StatementDescriptionRule {
            action: Some(StatementAction::TransferIn {
                from_wallet_id: Some(from),
            }),
            ..rule("giroconto")
        }],
        ..bank_mapping()
    }
}

fn on_match(line: u32, choice: StatementMatchChoice) -> StatementRowOverride {
    StatementRowOverride {
        on_match: Some(choice),
        ..user(line)
    }
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
// Rows typed by hand
// ---------------------------------------------------------------------------

#[test]
fn a_row_typed_by_hand_two_days_away_flags_the_statement_row() {
    let mut fx = setup();
    let cash = fx.wallet;
    let typed = by_hand(&mut fx, TransactionKind::Expense, cash, 350, 14);
    let options = options(fx.wallet);
    let preview = preview_with(&fx, BANK, &bank_mapping(), &options, &no_overrides());
    let pos = row(&preview, 2);
    assert_eq!(pos.status, StatementRowStatus::PossibleDuplicate);
    let existing = pos.existing.as_ref().unwrap();
    assert_eq!(existing.id, typed);
    assert_eq!(existing.note.as_deref(), Some("typed on the 14"));
    assert_eq!(existing.wallet_id, Some(fx.wallet));
    assert_eq!(pos.match_choice, Some(StatementMatchChoice::Skip));
    assert_eq!(pos.kind, Some(TransactionKind::Expense));
    assert_eq!(
        (
            preview.new_rows,
            preview.possible_duplicates,
            preview.moves,
            preview.skipped
        ),
        (2, 1, 0, 1)
    );
    assert_eq!(row(&preview, 3).existing, None);
    assert_eq!(row(&preview, 3).match_choice, None);

    // Imported anyway, the row is new with its match beside it.
    let anyway = [on_match(2, StatementMatchChoice::ImportAnyway)];
    let shown = preview_with(&fx, BANK, &bank_mapping(), &options, &anyway);
    let again = row(&shown, 2);
    assert_eq!(again.status, StatementRowStatus::New);
    assert_eq!(again.existing.as_ref().map(|tx| tx.id), Some(typed));
    assert_eq!(again.match_choice, Some(StatementMatchChoice::ImportAnyway));
    assert_eq!((shown.new_rows, shown.possible_duplicates), (3, 0));

    let report = import_with(&mut fx, BANK, &bank_mapping(), &options, &no_overrides());
    assert_eq!(
        (report.executed, report.possible_duplicates, report.skipped),
        (2, 1, 1)
    );
    assert!(fx.core.transaction(fx.vault, pos.command_id).is_err());
    assert_eq!(
        wallet_balance(&fx.core, fx.vault, fx.wallet),
        -350 + 185_000 - 12_000
    );
}

#[test]
fn a_row_imported_anyway_is_imported_beside_its_match() {
    let mut fx = setup();
    let cash = fx.wallet;
    by_hand(&mut fx, TransactionKind::Expense, cash, 350, 14);
    let options = options(cash);
    let anyway = [on_match(2, StatementMatchChoice::ImportAnyway)];
    let commands = command_count(&fx.core, fx.vault);
    let report = import_with(&mut fx, BANK, &bank_mapping(), &options, &anyway);
    assert_eq!((report.executed, report.possible_duplicates), (3, 0));
    // Three rows, and no claim.
    assert_eq!(command_count(&fx.core, fx.vault), commands + 3);
    assert_eq!(
        wallet_balance(&fx.core, fx.vault, cash),
        -700 + 185_000 - 12_000
    );
}

#[test]
fn only_an_entry_of_the_same_kind_amount_and_wallet_within_three_days_is_a_duplicate() {
    let mut fx = setup();
    let cash = fx.wallet;
    let other = run(&mut fx.core, fx.vault, wallet_cmd("Other", 0))
        .result_id
        .unwrap();
    // Line 2, an expense of 3,50 on the 16th: none of these.
    by_hand(&mut fx, TransactionKind::Expense, other, 350, 16);
    by_hand(&mut fx, TransactionKind::Refund, cash, 350, 16);
    by_hand(&mut fx, TransactionKind::Expense, cash, 351, 16);
    // Line 4, an expense of 120,00 on the 12th: four days.
    by_hand(&mut fx, TransactionKind::Expense, cash, 12_000, 8);
    // Line 3, an income of 1850,00 on the 15th: three days.
    let salary = by_hand(&mut fx, TransactionKind::Income, cash, 185_000, 18);
    let preview = preview_with(
        &fx,
        BANK,
        &bank_mapping(),
        &options(fx.wallet),
        &no_overrides(),
    );
    assert_eq!(row(&preview, 2).status, StatementRowStatus::New);
    assert_eq!(row(&preview, 4).status, StatementRowStatus::New);
    assert_eq!(
        row(&preview, 3).status,
        StatementRowStatus::PossibleDuplicate
    );
    assert_eq!(
        row(&preview, 3).existing.as_ref().map(|tx| tx.id),
        Some(salary)
    );
}

#[test]
fn rows_and_typed_entries_pair_one_to_one_closest_first() {
    let mut fx = setup();
    let cash = fx.wallet;
    let eleventh = by_hand(&mut fx, TransactionKind::Expense, cash, 500, 11);
    let twelfth = by_hand(&mut fx, TransactionKind::Expense, cash, 500, 12);
    let text = bank_csv(&[
        bank_row("10/09/2026", "BAR", "-5,00"),
        bank_row("11/09/2026", "BAR", "-5,00"),
        bank_row("12/09/2026", "BAR", "-5,00"),
    ]);
    let options = options(fx.wallet);
    let preview = preview_with(&fx, &text, &bank_mapping(), &options, &no_overrides());
    let matched = |line| row(&preview, line).existing.as_ref().map(|tx| tx.id);
    assert_eq!(row(&preview, 2).status, StatementRowStatus::New);
    assert_eq!(matched(2), None);
    assert_eq!(matched(3), Some(eleventh));
    assert_eq!(matched(4), Some(twelfth));
    assert_eq!(
        preview,
        preview_with(&fx, &text, &bank_mapping(), &options, &no_overrides())
    );
}

#[test]
fn voided_rows_and_opening_balances_never_match() {
    let mut fx = setup();
    let bank = run(
        &mut fx.core,
        fx.vault,
        Command::CreateWallet {
            name: "Bank".to_string(),
            opening_balance: 185_000,
            occurred_at: september(15),
        },
    )
    .result_id
    .unwrap();
    let typed = by_hand(&mut fx, TransactionKind::Expense, bank, 350, 16);
    run(
        &mut fx.core,
        fx.vault,
        Command::VoidTransaction {
            transaction_id: typed,
        },
    );
    let preview = preview_with(&fx, BANK, &bank_mapping(), &options(bank), &no_overrides());
    assert_eq!((preview.new_rows, preview.possible_duplicates), (3, 0));
}

#[test]
fn a_row_imported_before_is_never_its_own_duplicate_nor_its_neighbours() {
    let mut fx = setup();
    let options = options(fx.wallet);
    let first = card_csv(&[spend("2026-09-16 08:00:00 UTC", "CAFE", "CLEARED", "1.20")]);
    import_with(&mut fx, &first, &card_mapping(), &options, &no_overrides());
    let overlapping = card_csv(&[
        spend("2026-09-17 08:00:00 UTC", "CAFE", "CLEARED", "1.20"),
        spend("2026-09-16 08:00:00 UTC", "CAFE", "CLEARED", "1.20"),
    ]);
    let preview = preview_with(
        &fx,
        &overlapping,
        &card_mapping(),
        &options,
        &no_overrides(),
    );
    assert_eq!(row(&preview, 2).status, StatementRowStatus::New);
    assert_eq!(row(&preview, 2).existing, None);
    assert_eq!(row(&preview, 3).status, StatementRowStatus::AlreadyImported);
    assert_eq!(row(&preview, 3).existing, None);
}

#[test]
fn a_match_imported_anyway_stays_claimed() {
    let mut fx = setup();
    let cash = fx.wallet;
    let typed = by_hand(&mut fx, TransactionKind::Expense, cash, 500, 11);
    let text = bank_csv(&[
        bank_row("11/09/2026", "BAR", "-5,00"),
        bank_row("12/09/2026", "BAR", "-5,00"),
    ]);
    let options = options(fx.wallet);
    let plain = preview_with(&fx, &text, &bank_mapping(), &options, &no_overrides());
    assert_eq!(row(&plain, 2).status, StatementRowStatus::PossibleDuplicate);
    assert_eq!(row(&plain, 3).status, StatementRowStatus::New);
    let anyway = [on_match(2, StatementMatchChoice::ImportAnyway)];
    let preview = preview_with(&fx, &text, &bank_mapping(), &options, &anyway);
    assert_eq!(row(&preview, 2).status, StatementRowStatus::New);
    assert_eq!(
        row(&preview, 2).existing.as_ref().map(|tx| tx.id),
        Some(typed)
    );
    assert_eq!(row(&preview, 3).status, StatementRowStatus::New);
    assert_eq!(row(&preview, 3).existing, None);
}

#[test]
fn a_matched_row_the_user_leaves_out_is_skipped_and_records_nothing() {
    let mut fx = setup();
    let cash = fx.wallet;
    let (first, second) = two_banks(&mut fx);
    // A duplicate typed into the bank, a transfer, and a row near the file's
    // edge next to an entry imported for another export.
    by_hand(&mut fx, TransactionKind::Expense, first, 350, 15);
    transfer_by_hand(&mut fx, second, first, 30_000, 10);
    let edge = bank_csv(&[bank_row("11/09/2026", "EDICOLA", "-2,00")]);
    import_with(
        &mut fx,
        &edge,
        &bank_mapping(),
        &options(first),
        &no_overrides(),
    );
    let text = bank_csv(&[
        bank_row("16/09/2026", "BAR", "-3,50"),
        bank_row("10/09/2026", "GIROCONTO", "300,00"),
        bank_row("10/09/2026", "EDICOLA DUE", "-2,00"),
        bank_row("30/09/2026", "AFFITTO", "-500,00"),
    ]);
    let options = checking(first, cash);
    let plain = preview_with(&fx, &text, &bank_mapping(), &options, &no_overrides());
    assert_eq!(row(&plain, 2).status, StatementRowStatus::PossibleDuplicate);
    assert_eq!(row(&plain, 3).status, StatementRowStatus::MatchedTransfer);
    assert_eq!(
        row(&plain, 4).match_choice,
        Some(StatementMatchChoice::ImportAnyway)
    );
    let skip: Vec<StatementRowOverride> = (2..=4)
        .map(|line| StatementRowOverride {
            skip: true,
            ..user(line)
        })
        .collect();
    let preview = preview_with(&fx, &text, &bank_mapping(), &options, &skip);
    for line in 2..=4 {
        let left_out = row(&preview, line);
        assert_eq!(skip_code(left_out), "skipped_by_you", "line {line}");
        assert!(left_out.existing.is_some(), "line {line}");
        assert_eq!(left_out.match_choice, row(&plain, line).match_choice);
    }
    let commands = command_count(&fx.core, fx.vault);
    let report = import_with(&mut fx, &text, &bank_mapping(), &options, &skip);
    assert_eq!((report.executed, report.skipped), (1, 3));
    // The rent alone: nothing claimed for the rows left out.
    assert_eq!(command_count(&fx.core, fx.vault), commands + 1);
    let again = preview_with(&fx, &text, &bank_mapping(), &options, &no_overrides());
    assert_eq!(row(&again, 2).status, StatementRowStatus::PossibleDuplicate);
    assert_eq!(row(&again, 3).status, StatementRowStatus::MatchedTransfer);
}

// ---------------------------------------------------------------------------
// Rows typed into another wallet, moved into the statement's
// ---------------------------------------------------------------------------

/// The coffee of 3,50 the user typed into `wallet` on the 15th, in `flow`,
/// filed under Bar: the bank's line 2, a day later.
fn coffee_typed_into(fx: &mut Fx, wallet: Uuid, flow: Option<Uuid>) -> Uuid {
    let entry = Entry {
        amount: 350,
        wallet_id: Some(wallet),
        flow_id: flow,
        category: Some("Bar".to_string()),
        note: Some("caffè al bar".to_string()),
        occurred_at: september(15),
        person: None,
    };
    run(&mut fx.core, fx.vault, Command::Expense(entry))
        .result_id
        .unwrap()
}

/// The bank's options, also checking `cash`.
fn checking(bank: Uuid, cash: Uuid) -> StatementOptions {
    StatementOptions {
        also_check_wallets: vec![cash],
        ..options(bank)
    }
}

#[test]
fn a_row_typed_into_cash_is_moved_into_the_bank_as_it_is() {
    let mut fx = setup();
    let cash = fx.wallet;
    let bank = run(&mut fx.core, fx.vault, wallet_cmd("Bank", 0))
        .result_id
        .unwrap();
    let casa = new_flow(&mut fx, "Casa");
    let typed = coffee_typed_into(&mut fx, cash, Some(casa));
    let options = checking(bank, cash);

    let preview = preview_with(&fx, BANK, &bank_mapping(), &options, &no_overrides());
    let pos = row(&preview, 2);
    assert_eq!(pos.status, StatementRowStatus::PossibleDuplicate);
    assert_eq!(pos.existing.as_ref().map(|tx| tx.id), Some(typed));
    assert_eq!(pos.existing.as_ref().unwrap().wallet_id, Some(cash));
    assert_eq!(pos.match_choice, Some(StatementMatchChoice::MoveExisting));
    assert_eq!(
        (preview.new_rows, preview.possible_duplicates, preview.moves),
        (2, 1, 1)
    );

    let report = import_with(&mut fx, BANK, &bank_mapping(), &options, &no_overrides());
    assert_eq!(
        (report.executed, report.possible_duplicates, report.moved),
        (2, 1, 1)
    );
    assert!(report.rejected.is_empty());
    let moved = fx.core.transaction(fx.vault, typed).unwrap();
    assert_eq!(moved.wallet_id, Some(bank));
    assert_eq!(moved.flow_id, Some(casa));
    assert_eq!(moved.category, "Bar");
    assert_eq!(moved.note.as_deref(), Some("caffè al bar"));
    assert_eq!(moved.occurred_at, september(15));
    assert_eq!(wallet_balance(&fx.core, fx.vault, cash), 0);
    assert_eq!(
        wallet_balance(&fx.core, fx.vault, bank),
        -350 + 185_000 - 12_000
    );
    assert_eq!(flow_balance(&fx.core, fx.vault, casa), -350);

    // Its move in the log, the row was imported before: nothing moves again.
    let commands = command_count(&fx.core, fx.vault);
    let preview = preview_with(&fx, BANK, &bank_mapping(), &options, &no_overrides());
    assert_eq!(row(&preview, 2).status, StatementRowStatus::AlreadyImported);
    assert_eq!(row(&preview, 2).existing, None);
    assert_eq!(
        (
            preview.already_imported,
            preview.possible_duplicates,
            preview.moves
        ),
        (3, 0, 0)
    );
    let again = import_with(&mut fx, BANK, &bank_mapping(), &options, &no_overrides());
    assert_eq!(
        (
            again.executed,
            again.moved,
            again.deduplicated,
            again.possible_duplicates
        ),
        (0, 0, 3, 0)
    );
    assert_eq!(command_count(&fx.core, fx.vault), commands);

    // Nor does the coffee it moved stand for a new coffee in a later export.
    let later = bank_csv(&[
        bank_row(
            "16/09/2026",
            "\"PAGAMENTO POS; BAR CENTRALE  MILANO\"",
            "-3,50",
        ),
        bank_row("17/09/2026", "BAR", "-3,50"),
    ]);
    let preview = preview_with(&fx, &later, &bank_mapping(), &options, &no_overrides());
    assert_eq!(row(&preview, 2).status, StatementRowStatus::AlreadyImported);
    assert_eq!(row(&preview, 3).status, StatementRowStatus::New);
    assert_eq!(row(&preview, 3).existing, None);
}

#[test]
fn keeping_a_row_typed_into_cash_reads_as_moving_it_and_import_anyway_leaves_it() {
    for (choice, effective, wallet_after) in [
        (
            StatementMatchChoice::Skip,
            StatementMatchChoice::MoveExisting,
            true,
        ),
        (
            StatementMatchChoice::ImportAnyway,
            StatementMatchChoice::ImportAnyway,
            false,
        ),
    ] {
        let mut fx = setup();
        let cash = fx.wallet;
        let bank = new_wallet(&mut fx, "Bank");
        let typed = coffee_typed_into(&mut fx, cash, None);
        let options = checking(bank, cash);
        let asked = [on_match(2, choice)];
        let preview = preview_with(&fx, BANK, &bank_mapping(), &options, &asked);
        let pos = row(&preview, 2);
        assert_eq!(pos.match_choice, Some(effective), "{choice:?}");
        assert_eq!(pos.default_choice, Some(StatementMatchChoice::MoveExisting));
        import_with(&mut fx, BANK, &bank_mapping(), &options, &asked);
        let moved_to = if wallet_after { bank } else { cash };
        assert_eq!(
            fx.core.transaction(fx.vault, typed).unwrap().wallet_id,
            Some(moved_to),
            "{choice:?}"
        );
    }
}

#[test]
fn moving_reads_as_skipping_but_for_a_duplicate_in_another_wallet() {
    let mut fx = setup();
    let cash = fx.wallet;
    let (first, second) = two_banks(&mut fx);
    // A duplicate in the statement's own wallet, and a transfer.
    coffee_typed_into(&mut fx, first, None);
    transfer_by_hand(&mut fx, first, second, 30_000, 10);
    let text = bank_csv(&[
        bank_row("16/09/2026", "BAR", "-3,50"),
        bank_row("10/09/2026", "GIROCONTO", "-300,00"),
    ]);
    let moving = [
        on_match(2, StatementMatchChoice::MoveExisting),
        on_match(3, StatementMatchChoice::MoveExisting),
    ];
    let options = checking(first, cash);
    let preview = preview_with(&fx, &text, &bank_mapping(), &options, &moving);
    assert_eq!(
        row(&preview, 2).status,
        StatementRowStatus::PossibleDuplicate
    );
    assert_eq!(row(&preview, 3).status, StatementRowStatus::MatchedTransfer);
    for line in [2, 3] {
        assert_eq!(
            row(&preview, line).match_choice,
            Some(StatementMatchChoice::Skip)
        );
    }
    assert_eq!(preview.moves, 0);
    let report = import_with(&mut fx, &text, &bank_mapping(), &options, &moving);
    assert_eq!((report.executed, report.moved), (0, 0));
}

#[test]
fn a_refused_move_is_reported_on_its_line() {
    let mut fx = setup();
    let cash = fx.wallet;
    let bank = run(&mut fx.core, fx.vault, wallet_cmd("Bank", 0))
        .result_id
        .unwrap();
    let gone = new_flow(&mut fx, "Gone");
    coffee_typed_into(&mut fx, cash, Some(gone));
    // Back to zero, so the envelope can be archived.
    let refund = Entry {
        amount: 350,
        wallet_id: Some(cash),
        flow_id: Some(gone),
        category: None,
        note: None,
        occurred_at: september(1),
        person: None,
    };
    run(&mut fx.core, fx.vault, Command::Refund(refund));
    run(
        &mut fx.core,
        fx.vault,
        Command::ArchiveFlow { flow_id: gone },
    );
    let report = import_with(
        &mut fx,
        BANK,
        &bank_mapping(),
        &checking(bank, cash),
        &no_overrides(),
    );
    assert_eq!((report.executed, report.moved), (2, 0));
    assert_eq!(report.rejected.len(), 1);
    assert_eq!(report.rejected[0].line, 2);
    assert_eq!(report.rejected[0].code, "invalid_command");
}

#[test]
fn a_row_typed_into_an_archived_wallet_still_moves() {
    let mut fx = setup();
    let cash = fx.wallet;
    let bank = run(&mut fx.core, fx.vault, wallet_cmd("Bank", 0))
        .result_id
        .unwrap();
    let typed = coffee_typed_into(&mut fx, cash, None);
    by_hand(&mut fx, TransactionKind::Income, cash, 350, 1);
    run(
        &mut fx.core,
        fx.vault,
        Command::ArchiveWallet { wallet_id: cash },
    );
    let report = import_with(
        &mut fx,
        BANK,
        &bank_mapping(),
        &checking(bank, cash),
        &no_overrides(),
    );
    assert_eq!(report.moved, 1);
    assert_eq!(
        fx.core.transaction(fx.vault, typed).unwrap().wallet_id,
        Some(bank)
    );
}

// ---------------------------------------------------------------------------
// Rows an earlier import wrote
// ---------------------------------------------------------------------------

#[test]
fn last_months_coffee_is_not_a_duplicate_of_this_months() {
    let mut fx = setup();
    let bank = run(&mut fx.core, fx.vault, wallet_cmd("Bank", 0))
        .result_id
        .unwrap();
    let options = options(bank);
    let september = bank_csv(&[
        bank_row("30/09/2026", "BAR", "-1,20"),
        bank_row("29/09/2026", "BAR", "-1,20"),
    ]);
    let first = import_with(
        &mut fx,
        &september,
        &bank_mapping(),
        &options,
        &no_overrides(),
    );
    assert_eq!(first.executed, 2);
    let october = bank_csv(&[
        bank_row("01/10/2026", "BAR", "-1,20"),
        bank_row("02/10/2026", "BAR", "-1,20"),
    ]);
    let preview = preview_with(&fx, &october, &bank_mapping(), &options, &no_overrides());
    for line in [2, 3] {
        assert_eq!(row(&preview, line).status, StatementRowStatus::New);
        assert_eq!(row(&preview, line).existing, None);
    }
    assert_eq!((preview.new_rows, preview.possible_duplicates), (2, 0));
    let report = import_with(
        &mut fx,
        &october,
        &bank_mapping(),
        &options,
        &no_overrides(),
    );
    assert_eq!(report.executed, 2);
    assert_eq!(wallet_balance(&fx.core, fx.vault, bank), -480);
}

#[test]
fn a_row_typed_into_the_bank_still_flags_next_to_an_imported_one() {
    let mut fx = setup();
    let bank = run(&mut fx.core, fx.vault, wallet_cmd("Bank", 0))
        .result_id
        .unwrap();
    let options = options(bank);
    let september = bank_csv(&[bank_row("30/09/2026", "BAR", "-1,20")]);
    import_with(
        &mut fx,
        &september,
        &bank_mapping(),
        &options,
        &no_overrides(),
    );
    let entry = Entry {
        amount: 120,
        wallet_id: Some(bank),
        flow_id: None,
        category: None,
        note: Some("caffè".to_string()),
        occurred_at: noon_in_rome(10, 2),
        person: None,
    };
    let typed = run(&mut fx.core, fx.vault, Command::Expense(entry))
        .result_id
        .unwrap();
    assert_eq!(typed.get_version_num(), 7, "the app mints ids as v7");
    // A day from the imported row on the 30th and from the typed one.
    let october = bank_csv(&[bank_row("01/10/2026", "BAR", "-1,20")]);
    let preview = preview_with(&fx, &october, &bank_mapping(), &options, &no_overrides());
    let coffee = row(&preview, 2);
    assert_eq!(coffee.status, StatementRowStatus::PossibleDuplicate);
    assert_eq!(coffee.existing.as_ref().map(|tx| tx.id), Some(typed));
}

#[test]
fn a_recurring_row_recorded_into_the_bank_still_flags_its_bank_row() {
    let mut fx = setup();
    let bank = run(&mut fx.core, fx.vault, wallet_cmd("Bank", 0))
        .result_id
        .unwrap();
    let mortgage = run(
        &mut fx.core,
        fx.vault,
        Command::CreateRecurring {
            transaction_kind: TransactionKind::Expense,
            amount: 85_000,
            wallet_id: Some(bank),
            flow_id: None,
            category: Some("Mutuo".to_string()),
            note: Some("rata mutuo".to_string()),
            schedule: monthly_from(5, day(2026, 1, 5)),
            owner: None,
        },
    )
    .result_id
    .unwrap();
    let recorded = run(
        &mut fx.core,
        fx.vault,
        Command::ExecuteRecurring {
            recurring_id: mortgage,
            period_date: day(2026, 9, 5),
            occurred_at: september(5),
            person: None,
        },
    )
    .result_id
    .unwrap();
    let text = bank_csv(&[bank_row("07/09/2026", "ADDEBITO RATA MUTUO", "-850,00")]);
    let preview = preview_with(&fx, &text, &bank_mapping(), &options(bank), &no_overrides());
    let rata = row(&preview, 2);
    assert_eq!(rata.status, StatementRowStatus::PossibleDuplicate);
    assert_eq!(rata.existing.as_ref().map(|tx| tx.id), Some(recorded));
    assert_eq!(rata.match_choice, Some(StatementMatchChoice::Skip));
}

#[test]
fn a_row_once_imported_into_cash_is_still_found_moved_and_then_imported() {
    let mut fx = setup();
    let cash = fx.wallet;
    let bank = run(&mut fx.core, fx.vault, wallet_cmd("Bank", 0))
        .result_id
        .unwrap();
    // The user's own sheet of cash spending, imported into Cash once.
    let sheet = bank_csv(&[bank_row("15/09/2026", "CAFFE AL BAR", "-3,50")]);
    let cash_options = options(cash);
    let earlier = row(
        &preview_with(&fx, &sheet, &bank_mapping(), &cash_options, &no_overrides()),
        2,
    )
    .command_id;
    import_with(
        &mut fx,
        &sheet,
        &bank_mapping(),
        &cash_options,
        &no_overrides(),
    );

    let options = checking(bank, cash);
    let preview = preview_with(&fx, BANK, &bank_mapping(), &options, &no_overrides());
    let pos = row(&preview, 2);
    assert_eq!(pos.status, StatementRowStatus::PossibleDuplicate);
    assert_eq!(pos.existing.as_ref().map(|tx| tx.id), Some(earlier));
    assert_eq!(pos.match_choice, Some(StatementMatchChoice::MoveExisting));
    let report = import_with(&mut fx, BANK, &bank_mapping(), &options, &no_overrides());
    assert_eq!((report.executed, report.moved), (2, 1));
    assert_eq!(
        fx.core.transaction(fx.vault, earlier).unwrap().wallet_id,
        Some(bank)
    );

    // In the bank now, an imported row there: the bank's row reads imported
    // through its move.
    let commands = command_count(&fx.core, fx.vault);
    let preview = preview_with(&fx, BANK, &bank_mapping(), &options, &no_overrides());
    assert_eq!(row(&preview, 2).status, StatementRowStatus::AlreadyImported);
    let again = import_with(&mut fx, BANK, &bank_mapping(), &options, &no_overrides());
    assert_eq!((again.executed, again.moved, again.deduplicated), (0, 0, 3));
    assert_eq!(command_count(&fx.core, fx.vault), commands);
}

// ---------------------------------------------------------------------------
// What an import leaves in the log: moves and claims
// ---------------------------------------------------------------------------

fn new_wallet(fx: &mut Fx, name: &str) -> Uuid {
    run(&mut fx.core, fx.vault, wallet_cmd(name, 0))
        .result_id
        .unwrap()
}

/// The command of the log whose id is `id`.
fn logged(fx: &Fx, id: Uuid) -> Option<Command> {
    fx.core
        .commands_since(fx.vault, 0)
        .unwrap()
        .into_iter()
        .find(|record| record.envelope.id == id)
        .map(|record| record.envelope.command)
}

/// The claim an import writes for the row `row_id`.
fn claim_of(row_id: Uuid) -> Uuid {
    Uuid::new_v5(&row_id, b"claim")
}

fn claim_cmd(transaction_id: Uuid, patch: sparagne_core::TransactionPatch) -> Command {
    Command::UpdateTransaction {
        transaction_id,
        patch,
    }
}

#[test]
fn a_coffee_moved_in_for_september_is_no_duplicate_of_octobers() {
    let mut fx = setup();
    let cash = fx.wallet;
    let bank = new_wallet(&mut fx, "Bank");
    typed_on(&mut fx, TransactionKind::Expense, cash, 120, (9, 30));
    let options = checking(bank, cash);
    let september = bank_csv(&[bank_row("30/09/2026", "BAR", "-1,20")]);
    let first = import_with(
        &mut fx,
        &september,
        &bank_mapping(),
        &options,
        &no_overrides(),
    );
    assert_eq!((first.executed, first.moved), (0, 1));

    let october = bank_csv(&[bank_row("01/10/2026", "BAR", "-1,20")]);
    let preview = preview_with(&fx, &october, &bank_mapping(), &options, &no_overrides());
    assert_eq!(row(&preview, 2).status, StatementRowStatus::New);
    assert_eq!(row(&preview, 2).existing, None);
    let report = import_with(
        &mut fx,
        &october,
        &bank_mapping(),
        &options,
        &no_overrides(),
    );
    assert_eq!(report.executed, 1);
    assert_eq!(wallet_balance(&fx.core, fx.vault, bank), -240);
}

#[test]
fn an_overlapping_export_pairs_a_moved_row_with_no_other() {
    let mut fx = setup();
    let cash = fx.wallet;
    let bank = new_wallet(&mut fx, "Bank");
    let typed = typed_on(&mut fx, TransactionKind::Expense, cash, 350, (9, 14));
    let options = checking(bank, cash);
    let first = bank_csv(&[
        bank_row("15/09/2026", "BAR", "-3,50"),
        bank_row("20/09/2026", "CINEMA", "-9,00"),
    ]);
    import_with(&mut fx, &first, &bank_mapping(), &options, &no_overrides());
    // The next export starts the day after the moved coffee's row, and
    // opens with another coffee two days from the one typed.
    let overlapping = bank_csv(&[
        bank_row("16/09/2026", "BAR", "-3,50"),
        bank_row("20/09/2026", "CINEMA", "-9,00"),
    ]);
    let preview = preview_with(
        &fx,
        &overlapping,
        &bank_mapping(),
        &options,
        &no_overrides(),
    );
    assert_eq!(row(&preview, 3).status, StatementRowStatus::AlreadyImported);
    assert_eq!(row(&preview, 2).status, StatementRowStatus::New);
    assert_eq!(row(&preview, 2).existing, None);
    // One that holds the coffee's row again pairs the moved one with it.
    let again = bank_csv(&[
        bank_row("13/09/2026", "BAR", "-3,50"),
        bank_row("15/09/2026", "BAR", "-3,50"),
    ]);
    let preview = preview_with(&fx, &again, &bank_mapping(), &options, &no_overrides());
    assert_eq!(row(&preview, 3).status, StatementRowStatus::AlreadyImported);
    assert_eq!(row(&preview, 2).status, StatementRowStatus::New);
    assert_eq!(row(&preview, 2).existing, None);

    let report = import_with(
        &mut fx,
        &overlapping,
        &bank_mapping(),
        &options,
        &no_overrides(),
    );
    assert_eq!((report.executed, report.moved), (1, 0));
    assert_eq!(
        fx.core.transaction(fx.vault, typed).unwrap().wallet_id,
        Some(bank)
    );
    assert_eq!(wallet_balance(&fx.core, fx.vault, bank), -700 - 900);
}

#[test]
fn a_row_left_in_its_place_is_claimed_and_pairs_with_no_other() {
    let mut fx = setup();
    let bank = new_wallet(&mut fx, "Bank");
    let typed = typed_on(&mut fx, TransactionKind::Expense, bank, 10_000, (9, 29));
    let options = options(bank);
    let september = bank_csv(&[bank_row("29/09/2026", "BONIFICO", "-100,00")]);
    let first = import_with(
        &mut fx,
        &september,
        &bank_mapping(),
        &options,
        &no_overrides(),
    );
    assert_eq!((first.executed, first.possible_duplicates), (0, 1));

    // A month later, three days on: another payment.
    let october = bank_csv(&[bank_row("02/10/2026", "BONIFICO", "-100,00")]);
    let preview = preview_with(&fx, &october, &bank_mapping(), &options, &no_overrides());
    assert_eq!(row(&preview, 2).status, StatementRowStatus::New);
    // An overlapping export: the row again, and another a day from the
    // typed one.
    let overlapping = bank_csv(&[
        bank_row("30/09/2026", "BONIFICO", "-100,00"),
        bank_row("29/09/2026", "BONIFICO", "-100,00"),
    ]);
    let preview = preview_with(
        &fx,
        &overlapping,
        &bank_mapping(),
        &options,
        &no_overrides(),
    );
    assert_eq!(row(&preview, 3).status, StatementRowStatus::AlreadyImported);
    assert_eq!(row(&preview, 2).status, StatementRowStatus::New);
    assert_eq!(row(&preview, 2).existing, None);
    let report = import_with(
        &mut fx,
        &overlapping,
        &bank_mapping(),
        &options,
        &no_overrides(),
    );
    assert_eq!(report.executed, 1);
    assert_eq!(
        fx.core.transaction(fx.vault, typed).unwrap().wallet_id,
        Some(bank)
    );
}

#[test]
fn a_row_moved_into_one_bank_is_not_moved_again_into_another() {
    let mut fx = setup();
    let cash = fx.wallet;
    let (revolut, ing) = two_banks(&mut fx);
    let typed = coffee_typed_into(&mut fx, cash, None);
    let report = import_with(
        &mut fx,
        BANK,
        &bank_mapping(),
        &checking(revolut, cash),
        &no_overrides(),
    );
    assert_eq!(report.moved, 1);
    let ing_options = StatementOptions {
        also_check_wallets: vec![revolut, cash],
        ..options(ing)
    };
    let text = bank_csv(&[bank_row("16/09/2026", "BAR", "-3,50")]);
    let preview = preview_with(&fx, &text, &bank_mapping(), &ing_options, &no_overrides());
    assert_eq!(row(&preview, 2).status, StatementRowStatus::New);
    import_with(
        &mut fx,
        &text,
        &bank_mapping(),
        &ing_options,
        &no_overrides(),
    );
    assert_eq!(
        fx.core.transaction(fx.vault, typed).unwrap().wallet_id,
        Some(revolut)
    );
}

#[test]
fn a_transfer_claimed_for_septembers_row_is_not_octobers() {
    let mut fx = setup();
    let (first, second) = two_banks(&mut fx);
    let mapping = giroconto_to(second);
    let options = options(first);
    let september = bank_csv(&[bank_row("29/09/2026", "GIROCONTO", "-100,00")]);
    assert_eq!(
        import_with(&mut fx, &september, &mapping, &options, &no_overrides()).executed,
        1
    );
    let october = bank_csv(&[bank_row("02/10/2026", "GIROCONTO", "-100,00")]);
    let preview = preview_with(&fx, &october, &mapping, &options, &no_overrides());
    assert_eq!(row(&preview, 2).status, StatementRowStatus::New);
    assert_eq!(row(&preview, 2).existing, None);
    let report = import_with(&mut fx, &october, &mapping, &options, &no_overrides());
    assert_eq!(report.executed, 1);
    assert_eq!(wallet_balance(&fx.core, fx.vault, first), -20_000);
    assert_eq!(wallet_balance(&fx.core, fx.vault, second), 20_000);
}

#[test]
fn a_row_typed_on_the_same_day_beats_a_transfer_three_days_away() {
    let mut fx = setup();
    let cash = fx.wallet;
    let (first, second) = two_banks(&mut fx);
    let typed = typed_on(&mut fx, TransactionKind::Expense, cash, 30_000, (9, 10));
    let transfer = transfer_by_hand(&mut fx, first, second, 30_000, 13);
    let text = bank_csv(&[bank_row("10/09/2026", "PAGAMENTO", "-300,00")]);
    let preview = preview_with(
        &fx,
        &text,
        &bank_mapping(),
        &checking(first, cash),
        &no_overrides(),
    );
    let paid = row(&preview, 2);
    assert_eq!(paid.status, StatementRowStatus::PossibleDuplicate);
    assert_eq!(paid.existing.as_ref().map(|tx| tx.id), Some(typed));
    assert_ne!(paid.existing.as_ref().map(|tx| tx.id), Some(transfer));
}

/// A month of one bank: lines 2 and 6 on its first and last days, 3 to 5
/// well inside it, the 15th a transfer out when a rule says so.
fn september_statement() -> String {
    bank_csv(&[
        bank_row("01/09/2026", "STIPENDIO", "1850,00"),
        bank_row("10/09/2026", "BAR", "-3,50"),
        bank_row("15/09/2026", "GIROCONTO", "-300,00"),
        bank_row("20/09/2026", "CINEMA", "-9,00"),
        bank_row("30/09/2026", "AFFITTO", "-500,00"),
    ])
}

/// [`bank_mapping`] reading one more column into the description: every
/// row gets another id.
fn wider(mapping: StatementMapping) -> StatementMapping {
    StatementMapping {
        description_columns: vec!["Descrizione".to_string(), "Data valuta".to_string()],
        ..mapping
    }
}

#[test]
fn the_same_file_read_with_another_mapping_flags_every_row() {
    let mut fx = setup();
    let bank = new_wallet(&mut fx, "Bank");
    let options = options(bank);
    let text = september_statement();
    let before = preview_with(&fx, &text, &bank_mapping(), &options, &no_overrides());
    import_with(&mut fx, &text, &bank_mapping(), &options, &no_overrides());
    let preview = preview_with(
        &fx,
        &text,
        &wider(bank_mapping()),
        &options,
        &no_overrides(),
    );
    for line in 2..=6 {
        let again = row(&preview, line);
        assert_ne!(again.command_id, row(&before, line).command_id);
        assert_eq!(
            again.existing.as_ref().map(|tx| tx.id),
            Some(row(&before, line).command_id),
            "line {line}"
        );
    }
    // Inside the month the row is left out; on its first and last days it
    // may as well be the neighbour of another export, and is imported.
    for line in 3..=5 {
        assert_eq!(
            row(&preview, line).status,
            StatementRowStatus::PossibleDuplicate
        );
        assert_eq!(
            row(&preview, line).match_choice,
            Some(StatementMatchChoice::Skip)
        );
    }
    for line in [2, 6] {
        assert_eq!(row(&preview, line).status, StatementRowStatus::New);
        assert_eq!(
            row(&preview, line).match_choice,
            Some(StatementMatchChoice::ImportAnyway)
        );
    }
    let skip_the_edges = [
        on_match(2, StatementMatchChoice::Skip),
        on_match(6, StatementMatchChoice::Skip),
    ];
    let balance = wallet_balance(&fx.core, fx.vault, bank);
    let report = import_with(
        &mut fx,
        &text,
        &wider(bank_mapping()),
        &options,
        &skip_the_edges,
    );
    assert_eq!((report.executed, report.possible_duplicates), (0, 5));
    assert_eq!(wallet_balance(&fx.core, fx.vault, bank), balance);
    let after = preview_with(
        &fx,
        &text,
        &wider(bank_mapping()),
        &options,
        &no_overrides(),
    );
    assert_eq!((after.already_imported, after.possible_duplicates), (5, 0));
}

#[test]
fn the_same_file_read_with_another_mapping_finds_its_transfers_again() {
    let mut fx = setup();
    let (first, second) = two_banks(&mut fx);
    let options = options(first);
    let mapping = giroconto_to(second);
    let text = september_statement();
    let before = preview_with(&fx, &text, &mapping, &options, &no_overrides());
    import_with(&mut fx, &text, &mapping, &options, &no_overrides());
    let preview = preview_with(
        &fx,
        &text,
        &wider(mapping.clone()),
        &options,
        &no_overrides(),
    );
    let giroconto = row(&preview, 4);
    assert_eq!(giroconto.status, StatementRowStatus::MatchedTransfer);
    assert_eq!(giroconto.match_choice, Some(StatementMatchChoice::Skip));
    assert_eq!(
        giroconto.existing.as_ref().map(|tx| tx.id),
        Some(row(&before, 4).command_id)
    );
    let report = import_with(&mut fx, &text, &wider(mapping), &options, &no_overrides());
    assert_eq!(report.matched_transfers, 1);
    assert_eq!(wallet_balance(&fx.core, fx.vault, second), 30_000);
}

#[test]
fn as_many_rows_pair_as_can_be() {
    let mut fx = setup();
    let cash = fx.wallet;
    let bank = new_wallet(&mut fx, "Bank");
    let eleventh = typed_on(&mut fx, TransactionKind::Expense, cash, 700, (9, 11));
    let eighth = typed_on(&mut fx, TransactionKind::Expense, cash, 700, (9, 8));
    let text = bank_csv(&[
        bank_row("10/09/2026", "BAR", "-7,00"),
        bank_row("13/09/2026", "BAR", "-7,00"),
    ]);
    let options = checking(bank, cash);
    let preview = preview_with(&fx, &text, &bank_mapping(), &options, &no_overrides());
    let matched = |line| row(&preview, line).existing.as_ref().map(|tx| tx.id);
    assert_eq!(matched(2), Some(eighth));
    assert_eq!(matched(3), Some(eleventh));
    let report = import_with(&mut fx, &text, &bank_mapping(), &options, &no_overrides());
    assert_eq!((report.executed, report.moved), (0, 2));
    assert_eq!(wallet_balance(&fx.core, fx.vault, cash), 0);
    assert_eq!(wallet_balance(&fx.core, fx.vault, bank), -1_400);
}

#[test]
fn money_coming_in_matches_a_typed_income_or_refund_and_keeps_its_kind() {
    let mut fx = setup();
    let cash = fx.wallet;
    let bank = new_wallet(&mut fx, "Bank");
    let refund = typed_on(&mut fx, TransactionKind::Refund, cash, 1_500, (9, 14));
    let income = typed_on(&mut fx, TransactionKind::Income, cash, 2_000, (9, 14));
    typed_on(&mut fx, TransactionKind::Expense, cash, 2_500, (9, 14));
    let text = bank_csv(&[
        bank_row("15/09/2026", "RIMBORSO", "15,00"),
        bank_row("15/09/2026", "BONIFICO", "20,00"),
        bank_row("15/09/2026", "ACCREDITO", "25,00"),
    ]);
    let options = checking(bank, cash);
    let preview = preview_with(&fx, &text, &bank_mapping(), &options, &no_overrides());
    let matched = |line| row(&preview, line).existing.as_ref().map(|tx| tx.id);
    assert_eq!(matched(2), Some(refund));
    assert_eq!(row(&preview, 2).kind, Some(TransactionKind::Income));
    assert_eq!(matched(3), Some(income));
    assert_eq!(matched(4), None, "an expense is money going out");
    import_with(&mut fx, &text, &bank_mapping(), &options, &no_overrides());
    let moved = fx.core.transaction(fx.vault, refund).unwrap();
    assert_eq!(
        (moved.kind, moved.wallet_id),
        (TransactionKind::Refund, Some(bank))
    );
}

#[test]
fn a_pizza_in_the_cash_sheet_just_before_the_start_day_is_moved() {
    let mut fx = setup();
    let cash = fx.wallet;
    let bank = new_wallet(&mut fx, "Bank");
    // The user's own sheet, imported into Cash once: a pizza paid by card on
    // the 30th of August, booked by the bank on the 1st of September.
    let sheet = bank_csv(&[bank_row("30/08/2026", "PIZZA", "-15,00")]);
    let pizza = row(
        &preview_with(
            &fx,
            &sheet,
            &bank_mapping(),
            &options(cash),
            &no_overrides(),
        ),
        2,
    )
    .command_id;
    import_with(
        &mut fx,
        &sheet,
        &bank_mapping(),
        &options(cash),
        &no_overrides(),
    );
    let options = StatementOptions {
        since: Some(day(2026, 9, 1)),
        ..checking(bank, cash)
    };
    let text = bank_csv(&[bank_row("01/09/2026", "PIZZERIA DA GINO", "-15,00")]);
    let preview = preview_with(&fx, &text, &bank_mapping(), &options, &no_overrides());
    let booked = row(&preview, 2);
    assert_eq!(booked.status, StatementRowStatus::PossibleDuplicate);
    assert_eq!(booked.existing.as_ref().map(|tx| tx.id), Some(pizza));
    assert_eq!(
        booked.match_choice,
        Some(StatementMatchChoice::MoveExisting)
    );
    let report = import_with(&mut fx, &text, &bank_mapping(), &options, &no_overrides());
    assert_eq!((report.executed, report.moved), (0, 1));
    assert_eq!(
        fx.core.transaction(fx.vault, pizza).unwrap().wallet_id,
        Some(bank)
    );
    // Four days before the start, it is the user's August.
    let mut fx = setup();
    let cash = fx.wallet;
    let bank = new_wallet(&mut fx, "Bank");
    typed_on(&mut fx, TransactionKind::Expense, cash, 1_500, (8, 28));
    let options = StatementOptions {
        since: Some(day(2026, 9, 1)),
        ..checking(bank, cash)
    };
    let text = bank_csv(&[bank_row("01/09/2026", "PIZZERIA", "-15,00")]);
    let preview = preview_with(&fx, &text, &bank_mapping(), &options, &no_overrides());
    assert_eq!(row(&preview, 2).status, StatementRowStatus::New);
}

#[test]
fn claims_are_written_once_and_read_back() {
    let mut fx = setup();
    let (first, second) = two_banks(&mut fx);
    let typed = typed_on(&mut fx, TransactionKind::Expense, first, 350, (9, 15));
    let transfer = transfer_by_hand(&mut fx, second, first, 30_000, 10);
    let text = bank_csv(&[
        bank_row("16/09/2026", "BAR", "-3,50"),
        bank_row("10/09/2026", "GIROCONTO", "300,00"),
        bank_row("17/09/2026", "EDICOLA", "-2,00"),
    ]);
    let options = options(first);
    let preview = preview_with(&fx, &text, &bank_mapping(), &options, &no_overrides());
    assert_eq!(
        (preview.possible_duplicates, preview.matched_transfers),
        (1, 1)
    );
    let commands = command_count(&fx.core, fx.vault);
    let report = import_with(&mut fx, &text, &bank_mapping(), &options, &no_overrides());
    assert_eq!(report.executed, 1);
    assert!(report.rejected.is_empty(), "{:?}", report.rejected);
    // The new row and two claims, which change nothing.
    assert_eq!(command_count(&fx.core, fx.vault), commands + 3);
    assert_eq!(
        logged(&fx, claim_of(row(&preview, 2).command_id)),
        Some(claim_cmd(
            typed,
            sparagne_core::TransactionPatch {
                wallet_id: Some(first),
                ..Default::default()
            }
        ))
    );
    assert_eq!(
        logged(&fx, claim_of(row(&preview, 3).command_id)),
        Some(claim_cmd(
            transfer,
            sparagne_core::TransactionPatch {
                to_id: Some(first),
                ..Default::default()
            }
        ))
    );
    assert_eq!(
        wallet_balance(&fx.core, fx.vault, first),
        -350 + 30_000 - 200
    );

    let commands = command_count(&fx.core, fx.vault);
    let preview = preview_with(&fx, &text, &bank_mapping(), &options, &no_overrides());
    assert_eq!(preview.already_imported, 3);
    let again = import_with(&mut fx, &text, &bank_mapping(), &options, &no_overrides());
    assert_eq!((again.executed, again.deduplicated), (0, 3));
    assert_eq!(command_count(&fx.core, fx.vault), commands);
}

#[test]
fn both_sides_of_a_transfer_are_claimed_once_both_banks_are_in() {
    let mut fx = setup();
    let (first, second) = two_banks(&mut fx);
    let sent = preview_with(
        &fx,
        GIROCONTO_OUT,
        &giroconto_to(second),
        &options(first),
        &no_overrides(),
    );
    import_with(
        &mut fx,
        GIROCONTO_OUT,
        &giroconto_to(second),
        &options(first),
        &no_overrides(),
    );
    let received = preview_with(
        &fx,
        GIROCONTO_IN,
        &bank_mapping(),
        &options(second),
        &no_overrides(),
    );
    import_with(
        &mut fx,
        GIROCONTO_IN,
        &bank_mapping(),
        &options(second),
        &no_overrides(),
    );
    let transfer = row(&sent, 4).command_id;
    let side = |patch| Some(claim_cmd(transfer, patch));
    assert_eq!(
        logged(&fx, claim_of(transfer)),
        side(sparagne_core::TransactionPatch {
            from_id: Some(first),
            ..Default::default()
        })
    );
    assert_eq!(
        logged(&fx, claim_of(row(&received, 3).command_id)),
        side(sparagne_core::TransactionPatch {
            to_id: Some(second),
            ..Default::default()
        })
    );
    // Another 300 two days on, from either bank, is another transfer.
    let out = bank_csv(&[bank_row("12/09/2026", "GIROCONTO", "-300,00")]);
    let preview = preview_with(&fx, &out, &bank_mapping(), &options(first), &no_overrides());
    assert_eq!(row(&preview, 2).status, StatementRowStatus::New);
    let incoming = bank_csv(&[bank_row("14/09/2026", "GIROCONTO", "300,00")]);
    let preview = preview_with(
        &fx,
        &incoming,
        &bank_mapping(),
        &options(second),
        &no_overrides(),
    );
    assert_eq!(row(&preview, 2).status, StatementRowStatus::New);
}

#[test]
fn a_claim_the_engine_refuses_is_reported_on_its_line() {
    let mut fx = setup();
    let bank = new_wallet(&mut fx, "Bank");
    let gone = new_flow(&mut fx, "Gone");
    coffee_typed_into(&mut fx, bank, Some(gone));
    // Back to zero, so the envelope can be archived.
    let refund = Entry {
        amount: 350,
        wallet_id: Some(bank),
        flow_id: Some(gone),
        category: None,
        note: None,
        occurred_at: september(1),
        person: None,
    };
    run(&mut fx.core, fx.vault, Command::Refund(refund));
    run(
        &mut fx.core,
        fx.vault,
        Command::ArchiveFlow { flow_id: gone },
    );
    let report = import_with(
        &mut fx,
        BANK,
        &bank_mapping(),
        &options(bank),
        &no_overrides(),
    );
    assert_eq!((report.executed, report.possible_duplicates), (2, 1));
    assert_eq!(report.rejected.len(), 1);
    assert_eq!(report.rejected[0].line, 2);
    assert_eq!(report.rejected[0].code, "invalid_command");
}

// ---------------------------------------------------------------------------
// Near the first and last days of a file
// ---------------------------------------------------------------------------

/// October in a bank, the 2nd's line given: a salary on the 1st, an
/// apartment's rent on the 31st.
fn october_with(second: String) -> String {
    bank_csv(&[
        bank_row("01/10/2026", "STIPENDIO", "1850,00"),
        second,
        bank_row("31/10/2026", "AFFITTO", "-500,00"),
    ])
}

#[test]
fn a_row_typed_into_the_bank_and_claimed_for_september_is_imported_beside_octobers() {
    let mut fx = setup();
    let bank = new_wallet(&mut fx, "Bank");
    let typed = typed_on(&mut fx, TransactionKind::Expense, bank, 350, (10, 2));
    let options = options(bank);
    let september = bank_csv(&[bank_row("30/09/2026", "BAR", "-3,50")]);
    let first = import_with(
        &mut fx,
        &september,
        &bank_mapping(),
        &options,
        &no_overrides(),
    );
    assert_eq!(first.possible_duplicates, 1);

    let october = october_with(bank_row("02/10/2026", "BAR", "-3,50"));
    let preview = preview_with(&fx, &october, &bank_mapping(), &options, &no_overrides());
    let coffee = row(&preview, 3);
    assert_eq!(coffee.status, StatementRowStatus::New);
    assert_eq!(coffee.existing.as_ref().map(|tx| tx.id), Some(typed));
    assert_eq!(
        coffee.match_choice,
        Some(StatementMatchChoice::ImportAnyway)
    );
    let report = import_with(
        &mut fx,
        &october,
        &bank_mapping(),
        &options,
        &no_overrides(),
    );
    assert_eq!(report.executed, 3);
    assert_eq!(
        wallet_balance(&fx.core, fx.vault, bank),
        -350 - 350 + 185_000 - 50_000
    );
}

#[test]
fn a_row_moved_in_for_september_is_imported_beside_octobers() {
    let mut fx = setup();
    let cash = fx.wallet;
    let bank = new_wallet(&mut fx, "Bank");
    let typed = typed_on(&mut fx, TransactionKind::Expense, cash, 350, (10, 2));
    let options = checking(bank, cash);
    let september = bank_csv(&[bank_row("30/09/2026", "BAR", "-3,50")]);
    let first = import_with(
        &mut fx,
        &september,
        &bank_mapping(),
        &options,
        &no_overrides(),
    );
    assert_eq!(first.moved, 1);

    let october = october_with(bank_row("02/10/2026", "BAR", "-3,50"));
    let preview = preview_with(&fx, &october, &bank_mapping(), &options, &no_overrides());
    let coffee = row(&preview, 3);
    assert_eq!(coffee.status, StatementRowStatus::New);
    assert_eq!(coffee.existing.as_ref().map(|tx| tx.id), Some(typed));
    assert_eq!(
        coffee.match_choice,
        Some(StatementMatchChoice::ImportAnyway)
    );
    let report = import_with(
        &mut fx,
        &october,
        &bank_mapping(),
        &options,
        &no_overrides(),
    );
    assert_eq!((report.executed, report.moved), (3, 0));
}

#[test]
fn a_payment_next_to_a_transfer_claimed_for_september_is_imported() {
    let mut fx = setup();
    let (ing, revolut) = two_banks(&mut fx);
    let transfer = transfer_on(&mut fx, (ing, revolut), 20_000, (10, 1));
    let options = options(ing);
    let september = bank_csv(&[bank_row("30/09/2026", "GIROCONTO", "-200,00")]);
    let first = import_with(
        &mut fx,
        &september,
        &giroconto_to(revolut),
        &options,
        &no_overrides(),
    );
    assert_eq!(first.matched_transfers, 1);

    let october = october_with(bank_row("02/10/2026", "BONIFICO A MARIO ROSSI", "-200,00"));
    let preview = preview_with(&fx, &october, &bank_mapping(), &options, &no_overrides());
    let mario = row(&preview, 3);
    assert_eq!(mario.status, StatementRowStatus::New);
    assert_eq!(mario.kind, Some(TransactionKind::Expense));
    assert_eq!(mario.existing.as_ref().map(|tx| tx.id), Some(transfer));
    assert_eq!(mario.match_choice, Some(StatementMatchChoice::ImportAnyway));
    let report = import_with(
        &mut fx,
        &october,
        &bank_mapping(),
        &options,
        &no_overrides(),
    );
    assert_eq!(report.executed, 3);
    let paid = fx.core.transaction(fx.vault, mario.command_id).unwrap();
    assert_eq!(paid.kind, TransactionKind::Expense);
}

// ---------------------------------------------------------------------------
// The start day and the pools
// ---------------------------------------------------------------------------

#[test]
fn a_transfer_from_cash_before_the_start_day_matches_nothing() {
    let mut fx = setup();
    let cash = fx.wallet;
    let bank = new_wallet(&mut fx, "Bank");
    // The bank funded from Cash the day before the user starts importing.
    transfer_on(&mut fx, (cash, bank), 50_000, (8, 31));
    let text = bank_csv(&[bank_row("02/09/2026", "VERSAMENTO", "500,00")]);
    let plain = preview_with(
        &fx,
        &text,
        &bank_mapping(),
        &checking(bank, cash),
        &no_overrides(),
    );
    assert_eq!(row(&plain, 2).status, StatementRowStatus::MatchedTransfer);
    let options = StatementOptions {
        since: Some(day(2026, 9, 1)),
        ..checking(bank, cash)
    };
    let preview = preview_with(&fx, &text, &bank_mapping(), &options, &no_overrides());
    assert_eq!(row(&preview, 2).status, StatementRowStatus::New);
    assert_eq!(row(&preview, 2).existing, None);
}

#[test]
fn the_other_banks_side_the_day_before_the_start_day_still_matches() {
    let mut fx = setup();
    let (ing, revolut) = two_banks(&mut fx);
    // Revolut's statement started earlier and holds the transfer.
    let revolut_text = bank_csv(&[bank_row("30/09/2026", "BONIFICO DA ING", "200,00")]);
    import_with(
        &mut fx,
        &revolut_text,
        &from_ing(ing),
        &options(revolut),
        &no_overrides(),
    );
    let transfer = live_transfers(&fx)[0].id;
    let options = StatementOptions {
        since: Some(day(2026, 10, 1)),
        ..options(ing)
    };
    let text = bank_csv(&[bank_row("01/10/2026", "BONIFICO A REVOLUT", "-200,00")]);
    let preview = preview_with(&fx, &text, &bank_mapping(), &options, &no_overrides());
    let bonifico = row(&preview, 2);
    assert_eq!(bonifico.status, StatementRowStatus::MatchedTransfer);
    assert_eq!(bonifico.existing.as_ref().map(|tx| tx.id), Some(transfer));
    let report = import_with(&mut fx, &text, &bank_mapping(), &options, &no_overrides());
    assert_eq!((report.executed, report.matched_transfers), (0, 1));
    assert_eq!(wallet_balance(&fx.core, fx.vault, ing), -20_000);
}

#[test]
fn the_files_own_days_leave_out_the_rows_before_the_start_day() {
    let mut fx = setup();
    let bank = new_wallet(&mut fx, "Bank");
    let options = StatementOptions {
        since: Some(day(2026, 9, 1)),
        ..options(bank)
    };
    let earlier = bank_csv(&[bank_row("02/09/2026", "BAR", "-3,50")]);
    import_with(
        &mut fx,
        &earlier,
        &bank_mapping(),
        &options,
        &no_overrides(),
    );
    // The 28th of August is before the start: the file's days are the 5th
    // alone, and the coffee imported on the 2nd is outside them.
    let text = bank_csv(&[
        bank_row("28/08/2026", "BAR", "-3,50"),
        bank_row("05/09/2026", "BAR", "-3,50"),
    ]);
    let preview = preview_with(&fx, &text, &bank_mapping(), &options, &no_overrides());
    assert_eq!(skip_code(row(&preview, 2)), "before_start");
    assert_eq!(row(&preview, 3).status, StatementRowStatus::New);
    assert_eq!(row(&preview, 3).existing, None);
}

// ---------------------------------------------------------------------------
// A transfer already in as an income, an expense or a refund
// ---------------------------------------------------------------------------

/// The first bank's export: a bonifico to the second on 1 October.
fn bonifico_out() -> String {
    bank_csv(&[bank_row("01/10/2026", "BONIFICO A REVOLUT", "-200,00")])
}

/// The second bank's export: the same bonifico coming in.
fn bonifico_in() -> String {
    bank_csv(&[bank_row("01/10/2026", "BONIFICO DA ING", "200,00")])
}

/// [`bank_mapping`] taking the rows holding `contains` for `action`.
fn rule_for(contains: &str, action: StatementAction) -> StatementMapping {
    StatementMapping {
        description_rules: vec![StatementDescriptionRule {
            action: Some(action),
            ..rule(contains)
        }],
        ..bank_mapping()
    }
}

fn to_revolut(revolut: Uuid) -> StatementMapping {
    rule_for(
        "bonifico a revolut",
        StatementAction::TransferOut {
            to_wallet_id: Some(revolut),
        },
    )
}

fn from_ing(ing: Uuid) -> StatementMapping {
    rule_for(
        "bonifico da ing",
        StatementAction::TransferIn {
            from_wallet_id: Some(ing),
        },
    )
}

/// The live rows of `kind`.
fn live(fx: &Fx, kind: TransactionKind) -> Vec<sparagne_core::TransactionView> {
    list(
        &fx.core,
        fx.vault,
        &sparagne_core::TransactionFilter {
            kinds: Some(vec![kind]),
            ..Default::default()
        },
    )
}

fn live_transfers(fx: &Fx) -> Vec<sparagne_core::TransactionView> {
    list(
        &fx.core,
        fx.vault,
        &sparagne_core::TransactionFilter {
            kinds: Some(vec![TransactionKind::TransferWallet]),
            include_transfers: true,
            ..Default::default()
        },
    )
}

#[test]
fn a_transfer_whose_other_side_came_in_as_an_income_replaces_it() {
    let mut fx = setup();
    let (ing, revolut) = two_banks(&mut fx);
    let income = row(
        &preview_with(
            &fx,
            &bonifico_in(),
            &bank_mapping(),
            &options(revolut),
            &no_overrides(),
        ),
        2,
    )
    .command_id;
    import_with(
        &mut fx,
        &bonifico_in(),
        &bank_mapping(),
        &options(revolut),
        &no_overrides(),
    );
    assert_eq!(live(&fx, TransactionKind::Income).len(), 1);

    let preview = preview_with(
        &fx,
        &bonifico_out(),
        &to_revolut(revolut),
        &options(ing),
        &no_overrides(),
    );
    let bonifico = row(&preview, 2);
    assert_eq!(bonifico.status, StatementRowStatus::PossibleDuplicate);
    assert_eq!(bonifico.existing.as_ref().map(|tx| tx.id), Some(income));
    assert_eq!(
        bonifico.match_choice,
        Some(StatementMatchChoice::ReplaceExisting)
    );
    assert_eq!(bonifico.kind, Some(TransactionKind::TransferWallet));
    assert_eq!(bonifico.counter_wallet_id, Some(revolut));
    assert_eq!((preview.new_rows, preview.possible_duplicates), (0, 1));

    let report = import_with(
        &mut fx,
        &bonifico_out(),
        &to_revolut(revolut),
        &options(ing),
        &no_overrides(),
    );
    assert!(report.rejected.is_empty(), "{:?}", report.rejected);
    assert_eq!((report.executed, report.possible_duplicates), (1, 1));
    assert!(fx.core.transaction(fx.vault, income).unwrap().voided);
    let transfer = fx.core.transaction(fx.vault, bonifico.command_id).unwrap();
    assert_eq!(
        (transfer.kind, transfer.from_id, transfer.to_id),
        (TransactionKind::TransferWallet, Some(ing), Some(revolut))
    );
    assert_eq!(wallet_balance(&fx.core, fx.vault, ing), -20_000);
    assert_eq!(wallet_balance(&fx.core, fx.vault, revolut), 20_000);
    assert!(live(&fx, TransactionKind::Income).is_empty());
    // The transfer stands for the second bank's row now, on its side.
    assert_eq!(
        logged(&fx, claim_of(income)),
        Some(claim_cmd(
            bonifico.command_id,
            sparagne_core::TransactionPatch {
                to_id: Some(revolut),
                ..Default::default()
            }
        ))
    );

    let commands = command_count(&fx.core, fx.vault);
    let again = import_with(
        &mut fx,
        &bonifico_out(),
        &to_revolut(revolut),
        &options(ing),
        &no_overrides(),
    );
    assert_eq!((again.executed, again.deduplicated), (0, 1));
    let other = import_with(
        &mut fx,
        &bonifico_in(),
        &bank_mapping(),
        &options(revolut),
        &no_overrides(),
    );
    assert_eq!((other.executed, other.deduplicated), (0, 1));
    assert_eq!(command_count(&fx.core, fx.vault), commands);
}

#[test]
fn a_transfer_whose_other_side_went_out_as_an_expense_replaces_it() {
    let mut fx = setup();
    let (ing, revolut) = two_banks(&mut fx);
    let expense = row(
        &preview_with(
            &fx,
            &bonifico_out(),
            &bank_mapping(),
            &options(ing),
            &no_overrides(),
        ),
        2,
    )
    .command_id;
    import_with(
        &mut fx,
        &bonifico_out(),
        &bank_mapping(),
        &options(ing),
        &no_overrides(),
    );
    let preview = preview_with(
        &fx,
        &bonifico_in(),
        &from_ing(ing),
        &options(revolut),
        &no_overrides(),
    );
    let bonifico = row(&preview, 2);
    assert_eq!(bonifico.status, StatementRowStatus::PossibleDuplicate);
    assert_eq!(bonifico.existing.as_ref().map(|tx| tx.id), Some(expense));
    assert_eq!(
        bonifico.match_choice,
        Some(StatementMatchChoice::ReplaceExisting)
    );

    let report = import_with(
        &mut fx,
        &bonifico_in(),
        &from_ing(ing),
        &options(revolut),
        &no_overrides(),
    );
    assert_eq!((report.executed, report.possible_duplicates), (1, 1));
    assert!(fx.core.transaction(fx.vault, expense).unwrap().voided);
    assert_eq!(wallet_balance(&fx.core, fx.vault, ing), -20_000);
    assert_eq!(wallet_balance(&fx.core, fx.vault, revolut), 20_000);
    assert!(live(&fx, TransactionKind::Expense).is_empty());
    assert!(live(&fx, TransactionKind::Income).is_empty());
    assert_eq!(
        logged(&fx, claim_of(expense)),
        Some(claim_cmd(
            bonifico.command_id,
            sparagne_core::TransactionPatch {
                from_id: Some(ing),
                ..Default::default()
            }
        ))
    );

    let commands = command_count(&fx.core, fx.vault);
    for (text, mapping, wallet) in [
        (bonifico_in(), from_ing(ing), revolut),
        (bonifico_out(), bank_mapping(), ing),
    ] {
        let again = import_with(&mut fx, &text, &mapping, &options(wallet), &no_overrides());
        assert_eq!(again.executed, 0);
    }
    assert_eq!(command_count(&fx.core, fx.vault), commands);
}

#[test]
fn a_bonifico_typed_into_cash_is_shown_and_replaced_when_the_user_says_so() {
    let mut fx = setup();
    let cash = fx.wallet;
    let (ing, revolut) = two_banks(&mut fx);
    let typed = typed_on(&mut fx, TransactionKind::Expense, cash, 20_000, (10, 1));
    let options = checking(ing, cash);
    let preview = preview_with(
        &fx,
        &bonifico_out(),
        &to_revolut(revolut),
        &options,
        &no_overrides(),
    );
    let bonifico = row(&preview, 2);
    assert_eq!(bonifico.existing.as_ref().map(|tx| tx.id), Some(typed));
    // Cash is the user's sheet: never replaced by default.
    assert_eq!(
        bonifico.match_choice,
        Some(StatementMatchChoice::ImportAnyway)
    );
    let replace = [on_match(2, StatementMatchChoice::ReplaceExisting)];
    let preview = preview_with(
        &fx,
        &bonifico_out(),
        &to_revolut(revolut),
        &options,
        &replace,
    );
    assert_eq!(
        row(&preview, 2).match_choice,
        Some(StatementMatchChoice::ReplaceExisting)
    );
    assert_eq!(
        row(&preview, 2).default_choice,
        Some(StatementMatchChoice::ImportAnyway)
    );
    let report = import_with(
        &mut fx,
        &bonifico_out(),
        &to_revolut(revolut),
        &options,
        &replace,
    );
    assert_eq!(report.executed, 1);
    assert!(fx.core.transaction(fx.vault, typed).unwrap().voided);
    assert_eq!(wallet_balance(&fx.core, fx.vault, cash), 0);
    assert_eq!(wallet_balance(&fx.core, fx.vault, ing), -20_000);
    assert_eq!(wallet_balance(&fx.core, fx.vault, revolut), 20_000);
}

#[test]
fn an_entry_standing_for_a_transfer_is_replaced_or_imported_beside_never_kept() {
    // Keeping the entry would leave the transfer out: Skip and Move import
    // the row beside it, as Import anyway does.
    for choice in [
        StatementMatchChoice::Skip,
        StatementMatchChoice::MoveExisting,
        StatementMatchChoice::ImportAnyway,
    ] {
        let mut fx = setup();
        let (ing, revolut) = two_banks(&mut fx);
        import_with(
            &mut fx,
            &bonifico_in(),
            &bank_mapping(),
            &options(revolut),
            &no_overrides(),
        );
        let income = live(&fx, TransactionKind::Income)[0].id;
        let asked = [on_match(2, choice)];
        let preview = preview_with(
            &fx,
            &bonifico_out(),
            &to_revolut(revolut),
            &options(ing),
            &asked,
        );
        let bonifico = row(&preview, 2);
        assert_eq!(bonifico.status, StatementRowStatus::New, "{choice:?}");
        assert_eq!(
            bonifico.match_choice,
            Some(StatementMatchChoice::ImportAnyway),
            "{choice:?}"
        );
        let report = import_with(
            &mut fx,
            &bonifico_out(),
            &to_revolut(revolut),
            &options(ing),
            &asked,
        );
        assert_eq!(report.executed, 1, "{choice:?}");
        assert!(!fx.core.transaction(fx.vault, income).unwrap().voided);
        assert_eq!(wallet_balance(&fx.core, fx.vault, ing), -20_000);
        assert_eq!(wallet_balance(&fx.core, fx.vault, revolut), 40_000);
        assert_eq!(logged(&fx, claim_of(income)), None, "{choice:?}");
    }
}

#[test]
fn an_unrelated_cash_income_two_days_from_a_withdrawal_is_imported_beside() {
    let mut fx = setup();
    let cash = fx.wallet;
    let bank = new_wallet(&mut fx, "Bank");
    let income = typed_on(&mut fx, TransactionKind::Income, cash, 5_000, (10, 2));
    let mapping = rule_for(
        "prelievo",
        StatementAction::TransferOut {
            to_wallet_id: Some(cash),
        },
    );
    let text = bank_csv(&[bank_row("30/09/2026", "PRELIEVO BANCOMAT", "-50,00")]);
    let preview = preview_with(&fx, &text, &mapping, &options(bank), &no_overrides());
    let atm = row(&preview, 2);
    assert_eq!(atm.status, StatementRowStatus::New);
    assert_eq!(atm.existing.as_ref().map(|tx| tx.id), Some(income));
    assert_eq!(atm.match_choice, Some(StatementMatchChoice::ImportAnyway));
    let report = import_with(&mut fx, &text, &mapping, &options(bank), &no_overrides());
    assert_eq!(report.executed, 1);
    assert!(!fx.core.transaction(fx.vault, income).unwrap().voided);
    assert_eq!(wallet_balance(&fx.core, fx.vault, cash), 10_000);
    assert_eq!(wallet_balance(&fx.core, fx.vault, bank), -5_000);
}

#[test]
fn an_income_the_other_banks_row_claimed_is_replaced_and_its_next_export_still_imports() {
    let mut fx = setup();
    let (ing, revolut) = two_banks(&mut fx);
    let typed = typed_on(&mut fx, TransactionKind::Income, revolut, 20_000, (10, 1));
    // Revolut's statement finds the income typed and claims it for its row.
    let first = import_with(
        &mut fx,
        &bonifico_in(),
        &bank_mapping(),
        &options(revolut),
        &no_overrides(),
    );
    assert_eq!(first.possible_duplicates, 1);
    // A day on, ING's bonifico; the claim says the income is Revolut's row.
    let text = bank_csv(&[bank_row("02/10/2026", "BONIFICO A REVOLUT", "-200,00")]);
    let preview = preview_with(
        &fx,
        &text,
        &to_revolut(revolut),
        &options(ing),
        &no_overrides(),
    );
    let bonifico = row(&preview, 2);
    assert_eq!(bonifico.existing.as_ref().map(|tx| tx.id), Some(typed));
    assert_eq!(
        bonifico.match_choice,
        Some(StatementMatchChoice::ReplaceExisting)
    );
    let report = import_with(
        &mut fx,
        &text,
        &to_revolut(revolut),
        &options(ing),
        &no_overrides(),
    );
    assert_eq!(report.executed, 1);
    assert!(fx.core.transaction(fx.vault, typed).unwrap().voided);
    assert_eq!(
        logged(&fx, claim_of(typed)),
        Some(claim_cmd(
            bonifico.command_id,
            sparagne_core::TransactionPatch {
                to_id: Some(revolut),
                ..Default::default()
            }
        ))
    );

    // Revolut's next export holds the bonifico again and a top-up of 200.
    let next = bank_csv(&[
        bank_row("01/10/2026", "BONIFICO DA ING", "200,00"),
        bank_row("04/10/2026", "RICARICA", "200,00"),
    ]);
    let preview = preview_with(
        &fx,
        &next,
        &bank_mapping(),
        &options(revolut),
        &no_overrides(),
    );
    assert_eq!(row(&preview, 2).status, StatementRowStatus::AlreadyImported);
    assert_eq!(row(&preview, 3).status, StatementRowStatus::New);
    assert_eq!(row(&preview, 3).existing, None);
    let report = import_with(
        &mut fx,
        &next,
        &bank_mapping(),
        &options(revolut),
        &no_overrides(),
    );
    assert_eq!(report.executed, 1);
    assert_eq!(wallet_balance(&fx.core, fx.vault, ing), -20_000);
    assert_eq!(wallet_balance(&fx.core, fx.vault, revolut), 40_000);
}

#[test]
fn a_refused_replacement_writes_nothing() {
    let mut fx = setup();
    let (ing, revolut) = two_banks(&mut fx);
    import_with(
        &mut fx,
        &bonifico_in(),
        &bank_mapping(),
        &options(revolut),
        &no_overrides(),
    );
    let income = live(&fx, TransactionKind::Income)[0].id;
    // An archived statement wallet refuses the transfer.
    run(
        &mut fx.core,
        fx.vault,
        Command::ArchiveWallet { wallet_id: ing },
    );
    let preview = preview_with(
        &fx,
        &bonifico_out(),
        &to_revolut(revolut),
        &options(ing),
        &no_overrides(),
    );
    assert_eq!(
        row(&preview, 2).match_choice,
        Some(StatementMatchChoice::ReplaceExisting)
    );
    let commands = command_count(&fx.core, fx.vault);
    let report = import_with(
        &mut fx,
        &bonifico_out(),
        &to_revolut(revolut),
        &options(ing),
        &no_overrides(),
    );
    assert_eq!(report.executed, 0);
    assert_eq!(report.rejected.len(), 1);
    assert_eq!(report.rejected[0].line, 2);
    assert_eq!(command_count(&fx.core, fx.vault), commands);
    assert!(!fx.core.transaction(fx.vault, income).unwrap().voided);
    assert!(live_transfers(&fx).is_empty());
    assert_eq!(wallet_balance(&fx.core, fx.vault, revolut), 20_000);
}

#[test]
fn a_replacing_row_counts_its_rounding_in_the_preview_and_the_report() {
    let mut fx = setup();
    let (ing, revolut) = two_banks(&mut fx);
    let incoming = bank_csv(&[bank_row("01/10/2026", "BONIFICO DA ING", "200,01")]);
    import_with(
        &mut fx,
        &incoming,
        &bank_mapping(),
        &options(revolut),
        &no_overrides(),
    );
    let outgoing = bank_csv(&[bank_row("01/10/2026", "BONIFICO A REVOLUT", "-200,005")]);
    let preview = preview_with(
        &fx,
        &outgoing,
        &to_revolut(revolut),
        &options(ing),
        &no_overrides(),
    );
    let bonifico = row(&preview, 2);
    assert!(bonifico.rounded);
    assert_eq!(bonifico.amount, 20_001);
    assert_eq!(
        bonifico.match_choice,
        Some(StatementMatchChoice::ReplaceExisting)
    );
    assert_eq!(preview.rounded, 1);
    let report = import_with(
        &mut fx,
        &outgoing,
        &to_revolut(revolut),
        &options(ing),
        &no_overrides(),
    );
    assert_eq!((report.executed, report.rounded), (1, 1));
}

#[test]
fn a_replacing_row_the_user_leaves_out_writes_nothing() {
    let mut fx = setup();
    let (ing, revolut) = two_banks(&mut fx);
    import_with(
        &mut fx,
        &bonifico_in(),
        &bank_mapping(),
        &options(revolut),
        &no_overrides(),
    );
    let skip = [StatementRowOverride {
        skip: true,
        ..user(2)
    }];
    let preview = preview_with(
        &fx,
        &bonifico_out(),
        &to_revolut(revolut),
        &options(ing),
        &skip,
    );
    assert_eq!(skip_code(row(&preview, 2)), "skipped_by_you");
    assert!(row(&preview, 2).existing.is_some());
    let commands = command_count(&fx.core, fx.vault);
    let report = import_with(
        &mut fx,
        &bonifico_out(),
        &to_revolut(revolut),
        &options(ing),
        &skip,
    );
    assert_eq!((report.executed, report.skipped), (0, 1));
    assert_eq!(command_count(&fx.core, fx.vault), commands);
    assert_eq!(wallet_balance(&fx.core, fx.vault, ing), 0);
    assert_eq!(live(&fx, TransactionKind::Income).len(), 1);
}

#[test]
fn replacing_reads_as_the_default_on_any_other_match() {
    let mut fx = setup();
    let cash = fx.wallet;
    let bank = new_wallet(&mut fx, "Bank");
    by_hand(&mut fx, TransactionKind::Expense, bank, 350, 15);
    by_hand(&mut fx, TransactionKind::Income, cash, 185_000, 15);
    let asked = [
        on_match(2, StatementMatchChoice::ReplaceExisting),
        on_match(3, StatementMatchChoice::ReplaceExisting),
    ];
    let preview = preview_with(&fx, BANK, &bank_mapping(), &checking(bank, cash), &asked);
    assert_eq!(
        row(&preview, 2).match_choice,
        Some(StatementMatchChoice::Skip)
    );
    assert_eq!(
        row(&preview, 3).match_choice,
        Some(StatementMatchChoice::MoveExisting)
    );
}

// ---------------------------------------------------------------------------
// The user's own sheet in Cash, and what a row says of itself
// ---------------------------------------------------------------------------

/// The user's own sheet of cash spending, imported into `cash` once: the
/// ids of its rows' entries.
fn sheet_into(fx: &mut Fx, cash: Uuid, rows: &[String]) -> Vec<Uuid> {
    let text = bank_csv(rows);
    let preview = preview_with(fx, &text, &bank_mapping(), &options(cash), &no_overrides());
    import_with(fx, &text, &bank_mapping(), &options(cash), &no_overrides());
    preview.rows.iter().map(|row| row.command_id).collect()
}

fn atm_to(cash: Uuid) -> StatementMapping {
    rule_for(
        "prelievo",
        StatementAction::TransferOut {
            to_wallet_id: Some(cash),
        },
    )
}

#[test]
fn an_atm_withdrawal_never_replaces_a_sheet_row_two_days_away() {
    for ticked in [false, true] {
        let mut fx = setup();
        let cash = fx.wallet;
        let ing = new_wallet(&mut fx, "ING");
        let dinner = sheet_into(
            &mut fx,
            cash,
            &[bank_row("08/10/2026", "RIMBORSO CENA LUCA", "50,00")],
        )[0];
        let options = if ticked {
            checking(ing, cash)
        } else {
            options(ing)
        };
        let text = bank_csv(&[bank_row("06/10/2026", "PRELIEVO BANCOMAT", "-50,00")]);
        let preview = preview_with(&fx, &text, &atm_to(cash), &options, &no_overrides());
        let atm = row(&preview, 2);
        assert_eq!(
            atm.existing.as_ref().map(|tx| tx.id),
            Some(dinner),
            "{ticked}"
        );
        assert_eq!(
            atm.match_choice,
            Some(StatementMatchChoice::ImportAnyway),
            "{ticked}"
        );
        let report = import_with(&mut fx, &text, &atm_to(cash), &options, &no_overrides());
        assert_eq!(report.executed, 1);
        assert!(!fx.core.transaction(fx.vault, dinner).unwrap().voided);
        assert_eq!(wallet_balance(&fx.core, fx.vault, cash), 10_000);
        assert_eq!(wallet_balance(&fx.core, fx.vault, ing), -5_000);
    }
}

#[test]
fn a_sheet_row_a_day_from_a_transfer_is_replaced_by_default_only_when_cash_is_not_checked() {
    for (ticked, expected) in [
        (true, StatementMatchChoice::ImportAnyway),
        (false, StatementMatchChoice::ReplaceExisting),
    ] {
        let mut fx = setup();
        let cash = fx.wallet;
        let ing = new_wallet(&mut fx, "ING");
        sheet_into(
            &mut fx,
            cash,
            &[bank_row("07/10/2026", "CONTANTI", "50,00")],
        );
        let options = if ticked {
            checking(ing, cash)
        } else {
            options(ing)
        };
        let text = bank_csv(&[bank_row("06/10/2026", "PRELIEVO BANCOMAT", "-50,00")]);
        let preview = preview_with(&fx, &text, &atm_to(cash), &options, &no_overrides());
        assert_eq!(row(&preview, 2).match_choice, Some(expected), "{ticked}");
        assert_eq!(row(&preview, 2).default_choice, Some(expected), "{ticked}");
    }
}

#[test]
fn an_income_a_day_from_the_other_banks_row_is_replaced_by_default() {
    let mut fx = setup();
    let (ing, revolut) = two_banks(&mut fx);
    let incoming = bank_csv(&[bank_row("02/10/2026", "BONIFICO DA ING", "200,00")]);
    import_with(
        &mut fx,
        &incoming,
        &bank_mapping(),
        &options(revolut),
        &no_overrides(),
    );
    let preview = preview_with(
        &fx,
        &bonifico_out(),
        &to_revolut(revolut),
        &options(ing),
        &no_overrides(),
    );
    assert_eq!(
        row(&preview, 2).match_choice,
        Some(StatementMatchChoice::ReplaceExisting)
    );
}

#[test]
fn a_row_says_what_it_is_before_the_user_and_what_its_default_is() {
    let mut fx = setup();
    let bank = new_wallet(&mut fx, "Bank");
    by_hand(&mut fx, TransactionKind::Expense, bank, 350, 15);
    let mapping = StatementMapping {
        description_rules: vec![StatementDescriptionRule {
            action: Some(StatementAction::Refund),
            ..rule("bonifico")
        }],
        ..bank_mapping()
    };
    let overrides = [
        on_match(2, StatementMatchChoice::ImportAnyway),
        StatementRowOverride {
            action: Some(StatementAction::Expense),
            ..user(4)
        },
    ];
    let options = options(bank);
    let preview = preview_with(&fx, BANK, &mapping, &options, &overrides);
    let pos = row(&preview, 2);
    assert_eq!(pos.match_choice, Some(StatementMatchChoice::ImportAnyway));
    assert_eq!(pos.default_choice, Some(StatementMatchChoice::Skip));
    assert_eq!(pos.base_action, Some(StatementAction::BySign));
    let salary = row(&preview, 3);
    assert_eq!((salary.match_choice, salary.default_choice), (None, None));
    assert_eq!(salary.base_action, Some(StatementAction::BySign));
    let wire = row(&preview, 4);
    assert_eq!(wire.action, StatementAction::Expense);
    assert_eq!(wire.base_action, Some(StatementAction::Refund));

    // Imported, a row still says what the rules make of it.
    import_with(&mut fx, BANK, &bank_mapping(), &options, &no_overrides());
    let again = preview_with(&fx, BANK, &mapping, &options, &overrides);
    assert_eq!(row(&again, 4).status, StatementRowStatus::AlreadyImported);
    assert_eq!(row(&again, 4).base_action, Some(StatementAction::Refund));

    let ragged = card_csv(&["2026-09-15 08:00:00 UTC,card_spend,TOO SHORT".to_string()]);
    let broken = preview_with(&fx, &ragged, &card_mapping(), &options, &no_overrides());
    assert_eq!(row(&broken, 2).base_action, None);
}

#[test]
fn rows_that_look_like_entries_of_a_wallet_not_checked_are_counted() {
    let mut fx = setup();
    let cash = fx.wallet;
    let bank = new_wallet(&mut fx, "Bank");
    let revolut = new_wallet(&mut fx, "Revolut");
    // Cash, typed by hand: the bar on the 15th, the wire on the 12th.
    by_hand(&mut fx, TransactionKind::Expense, cash, 350, 15);
    by_hand(&mut fx, TransactionKind::Expense, cash, 12_000, 12);
    // Revolut is a statement's own: its transfer side is claimed.
    let revolut_text = bank_csv(&[bank_row("20/09/2026", "GIROCONTO", "-1,00")]);
    import_with(
        &mut fx,
        &revolut_text,
        &giroconto_to(cash),
        &options(revolut),
        &no_overrides(),
    );
    by_hand(&mut fx, TransactionKind::Expense, revolut, 350, 16);

    let preview = preview_with(&fx, BANK, &bank_mapping(), &options(bank), &no_overrides());
    assert_eq!(
        preview.lookalikes,
        [statement::StatementLookalike {
            wallet_id: cash,
            rows: 2
        }]
    );
    // Advice only: the rows are new.
    assert_eq!(row(&preview, 2).status, StatementRowStatus::New);
    assert_eq!(row(&preview, 4).status, StatementRowStatus::New);
    // A row left out is not counted.
    let skip = [StatementRowOverride {
        skip: true,
        ..user(4)
    }];
    let preview = preview_with(&fx, BANK, &bank_mapping(), &options(bank), &skip);
    assert_eq!(preview.lookalikes[0].rows, 1);
    // Checked, Cash gives duplicates and no advice.
    let preview = preview_with(
        &fx,
        BANK,
        &bank_mapping(),
        &checking(bank, cash),
        &no_overrides(),
    );
    assert!(preview.lookalikes.is_empty());
    assert_eq!(preview.possible_duplicates, 2);
}

// ---------------------------------------------------------------------------
// Transfers between the user's banks
// ---------------------------------------------------------------------------

#[test]
fn the_other_side_of_an_imported_transfer_is_matched_and_counted_once() {
    let mut fx = setup();
    let (first, second) = two_banks(&mut fx);
    let out = import_with(
        &mut fx,
        GIROCONTO_OUT,
        &giroconto_to(second),
        &options(first),
        &no_overrides(),
    );
    assert_eq!(out.executed, 3);
    let sent = preview_with(
        &fx,
        GIROCONTO_OUT,
        &giroconto_to(second),
        &options(first),
        &no_overrides(),
    );
    let transfer = row(&sent, 4).command_id;

    let options = options(second);
    let preview = preview_with(
        &fx,
        GIROCONTO_IN,
        &bank_mapping(),
        &options,
        &no_overrides(),
    );
    let incoming = row(&preview, 3);
    assert_eq!(incoming.status, StatementRowStatus::MatchedTransfer);
    assert_eq!(incoming.existing.as_ref().map(|tx| tx.id), Some(transfer));
    assert_eq!(incoming.kind, Some(TransactionKind::TransferWallet));
    assert_eq!(incoming.counter_wallet_id, Some(first));
    assert_eq!(incoming.action, StatementAction::BySign);
    assert_eq!(incoming.match_choice, Some(StatementMatchChoice::Skip));
    assert_eq!((preview.new_rows, preview.matched_transfers), (1, 1));

    let report = import_with(
        &mut fx,
        GIROCONTO_IN,
        &bank_mapping(),
        &options,
        &no_overrides(),
    );
    assert_eq!((report.executed, report.matched_transfers), (1, 1));
    assert_eq!(
        wallet_balance(&fx.core, fx.vault, first),
        185_000 - 1_240 - 30_000
    );
    assert_eq!(wallet_balance(&fx.core, fx.vault, second), 30_000 - 5_000);

    // Claimed for the row, the transfer makes it a row imported before.
    let commands = command_count(&fx.core, fx.vault);
    let again = import_with(
        &mut fx,
        GIROCONTO_IN,
        &bank_mapping(),
        &options,
        &no_overrides(),
    );
    assert_eq!(
        (again.executed, again.deduplicated, again.matched_transfers),
        (0, 2, 0)
    );
    assert_eq!(command_count(&fx.core, fx.vault), commands);
}

#[test]
fn the_transfer_imported_from_the_receiving_bank_first_matches_the_sending_one() {
    let mut fx = setup();
    let (first, second) = two_banks(&mut fx);
    let incoming = import_with(
        &mut fx,
        GIROCONTO_IN,
        &giroconto_from(first),
        &options(second),
        &no_overrides(),
    );
    assert_eq!(incoming.executed, 2);
    let preview = preview_with(
        &fx,
        GIROCONTO_OUT,
        &bank_mapping(),
        &options(first),
        &no_overrides(),
    );
    let outgoing = row(&preview, 4);
    assert_eq!(outgoing.status, StatementRowStatus::MatchedTransfer);
    assert_eq!(outgoing.counter_wallet_id, Some(second));
    let report = import_with(
        &mut fx,
        GIROCONTO_OUT,
        &bank_mapping(),
        &options(first),
        &no_overrides(),
    );
    assert_eq!((report.executed, report.matched_transfers), (2, 1));
    assert_eq!(
        wallet_balance(&fx.core, fx.vault, first),
        185_000 - 1_240 - 30_000
    );
}

#[test]
fn a_transfer_matches_only_the_way_and_the_wallet_the_row_names() {
    let mut fx = setup();
    let (first, second) = two_banks(&mut fx);
    let third = run(&mut fx.core, fx.vault, wallet_cmd("Third", 0))
        .result_id
        .unwrap();
    let typed = transfer_by_hand(&mut fx, first, second, 30_000, 10);
    let options = options(first);
    let as_action = |action| StatementRowOverride {
        action: Some(action),
        ..user(4)
    };
    let status = |action| {
        let preview = preview_with(
            &fx,
            GIROCONTO_OUT,
            &bank_mapping(),
            &options,
            &[as_action(action)],
        );
        let row = row(&preview, 4).clone();
        (
            row.status,
            row.existing.map(|tx| tx.id),
            row.counter_wallet_id,
        )
    };
    assert_eq!(
        status(StatementAction::TransferOut {
            to_wallet_id: Some(third)
        }),
        (StatementRowStatus::New, None, Some(third))
    );
    assert_eq!(
        status(StatementAction::TransferOut {
            to_wallet_id: Some(second)
        }),
        (
            StatementRowStatus::MatchedTransfer,
            Some(typed),
            Some(second)
        )
    );
    // Waiting for its wallet, it finds it in the transfer.
    assert_eq!(
        status(StatementAction::TransferOut { to_wallet_id: None }),
        (
            StatementRowStatus::MatchedTransfer,
            Some(typed),
            Some(second)
        )
    );
    // Money coming in is not money going out.
    assert_eq!(
        status(StatementAction::TransferIn {
            from_wallet_id: None
        })
        .0,
        StatementRowStatus::Skipped {
            code: "needs_wallet".to_string(),
            reason: "choose the wallet the money came from".to_string(),
        }
    );
    // An expense is an expense.
    assert_eq!(
        status(StatementAction::Expense),
        (StatementRowStatus::New, None, None)
    );
    assert_eq!(
        status(StatementAction::BySign),
        (
            StatementRowStatus::MatchedTransfer,
            Some(typed),
            Some(second)
        )
    );
}

#[test]
fn a_topup_waiting_for_its_wallet_matches_the_transfer_instead() {
    let mut fx = setup();
    let cash = fx.wallet;
    let bank = bank_wallet(&mut fx);
    let typed = transfer_by_hand(&mut fx, bank, cash, 12_602, 13);
    let preview = preview_with(
        &fx,
        CARD,
        &card_mapping(),
        &options(fx.wallet),
        &no_overrides(),
    );
    let topup = row(&preview, 8);
    assert_eq!(topup.status, StatementRowStatus::MatchedTransfer);
    assert_eq!(topup.existing.as_ref().map(|tx| tx.id), Some(typed));
    assert_eq!(topup.counter_wallet_id, Some(bank));
    assert_eq!(
        topup.action,
        StatementAction::TransferIn {
            from_wallet_id: None
        }
    );
}

#[test]
fn equal_transfers_pair_one_to_one_and_five_days_is_too_far() {
    let mut fx = setup();
    let (first, second) = two_banks(&mut fx);
    // Both rows of 100 are within reach of both transfers of 100.
    let tenth = transfer_by_hand(&mut fx, first, second, 10_000, 10);
    let twelfth = transfer_by_hand(&mut fx, first, second, 10_000, 12);
    transfer_by_hand(&mut fx, first, second, 20_000, 10);
    transfer_by_hand(&mut fx, first, second, 30_000, 10);
    let text = bank_csv(&[
        bank_row("12/09/2026", "GIROCONTO", "-100,00"),
        bank_row("11/09/2026", "GIROCONTO", "-100,00"),
        bank_row("14/09/2026", "GIROCONTO", "-200,00"),
        bank_row("15/09/2026", "GIROCONTO", "-300,00"),
    ]);
    let options = options(first);
    let mapping = giroconto_to(second);
    let preview = preview_with(&fx, &text, &mapping, &options, &no_overrides());
    let matched = |line| row(&preview, line).existing.as_ref().map(|tx| tx.id);
    assert_eq!(matched(2), Some(twelfth));
    assert_eq!(matched(3), Some(tenth));
    assert_eq!(
        row(&preview, 4).status,
        StatementRowStatus::MatchedTransfer,
        "four days"
    );
    assert_eq!(
        row(&preview, 5).status,
        StatementRowStatus::New,
        "five days"
    );
    let report = import_with(&mut fx, &text, &mapping, &options, &no_overrides());
    assert_eq!((report.executed, report.matched_transfers), (1, 3));
}

#[test]
fn a_matched_transfer_imported_anyway_is_what_its_action_says() {
    let mut fx = setup();
    let (first, second) = two_banks(&mut fx);
    let typed = transfer_by_hand(&mut fx, first, second, 30_000, 10);
    let options = options(first);
    let anyway = [on_match(4, StatementMatchChoice::ImportAnyway)];
    let preview = preview_with(&fx, GIROCONTO_OUT, &bank_mapping(), &options, &anyway);
    let outgoing = row(&preview, 4);
    assert_eq!(outgoing.status, StatementRowStatus::New);
    assert_eq!(outgoing.kind, Some(TransactionKind::Expense));
    assert_eq!(outgoing.counter_wallet_id, None);
    assert_eq!(outgoing.existing.as_ref().map(|tx| tx.id), Some(typed));
    assert_eq!(
        outgoing.match_choice,
        Some(StatementMatchChoice::ImportAnyway)
    );
    let report = import_with(&mut fx, GIROCONTO_OUT, &bank_mapping(), &options, &anyway);
    assert_eq!(report.executed, 3);
    let tx = fx.core.transaction(fx.vault, outgoing.command_id).unwrap();
    assert_eq!(tx.kind, TransactionKind::Expense);
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

/// The second bank's export with every kind of row: the other side of a
/// transfer, a duplicate typed into Cash and one typed into the bank, a row
/// the user skips, one imported anyway, a new one, a zero and an unreadable
/// amount.
struct Busy {
    fx: Fx,
    text: String,
    options: StatementOptions,
    overrides: Vec<StatementRowOverride>,
}

fn busy() -> Busy {
    let mut fx = setup();
    let cash = fx.wallet;
    let (first, second) = two_banks(&mut fx);
    import_with(
        &mut fx,
        GIROCONTO_OUT,
        &giroconto_to(second),
        &options(first),
        &no_overrides(),
    );
    coffee_typed_into(&mut fx, cash, None);
    by_hand(&mut fx, TransactionKind::Expense, second, 5_000, 14);
    by_hand(&mut fx, TransactionKind::Expense, second, 1_500, 19);
    let text = bank_csv(&[
        bank_row("14/09/2026", "PRELIEVO BANCOMAT", "-50,00"),
        bank_row("12/09/2026", "BONIFICO IN ENTRATA GIROCONTO", "300,00"),
        bank_row("16/09/2026", "BAR", "-3,50"),
        bank_row("17/09/2026", "EDICOLA", "-2,00"),
        bank_row("18/09/2026", "CINEMA", "-9,00"),
        bank_row("19/09/2026", "PIZZA", "-15,00"),
        bank_row("20/09/2026", "COMMISSIONI", "0,00"),
        bank_row("21/09/2026", "STORNO", "abc"),
    ]);
    let overrides = vec![
        StatementRowOverride {
            category: Some("Giornali".to_string()),
            ..user(5)
        },
        StatementRowOverride {
            skip: true,
            ..user(6)
        },
        on_match(7, StatementMatchChoice::ImportAnyway),
    ];
    Busy {
        fx,
        text,
        options: checking(second, cash),
        overrides,
    }
}

#[test]
fn the_preview_counts_what_the_import_then_does() {
    let Busy {
        mut fx,
        text,
        options,
        overrides,
    } = busy();
    let preview = preview_with(&fx, &text, &bank_mapping(), &options, &overrides);
    assert_eq!(
        (
            preview.new_rows,
            preview.possible_duplicates,
            preview.matched_transfers,
            preview.moves,
            preview.skipped,
            preview.invalid
        ),
        (2, 2, 1, 1, 2, 1)
    );
    let report = import_with(&mut fx, &text, &bank_mapping(), &options, &overrides);
    assert!(report.rejected.is_empty(), "{:?}", report.rejected);
    assert_eq!(report.executed, preview.new_rows);
    assert_eq!(report.deduplicated, preview.already_imported);
    assert_eq!(report.possible_duplicates, preview.possible_duplicates);
    assert_eq!(report.matched_transfers, preview.matched_transfers);
    assert_eq!(report.moved, preview.moves);
    assert_eq!(report.skipped, preview.skipped + preview.invalid);
    assert_eq!(report.rounded, preview.rounded);
    // Every row the import wrote is the one the preview showed.
    for line in [5, 7] {
        let shown = row(&preview, line);
        let tx = fx.core.transaction(fx.vault, shown.command_id).unwrap();
        assert_eq!((tx.kind, tx.amount), (shown.kind.unwrap(), shown.amount));
    }
}

#[test]
fn a_preview_with_overrides_writes_nothing_either() {
    let Busy {
        fx,
        text,
        options,
        overrides,
    } = busy();
    let commands = command_count(&fx.core, fx.vault);
    let before = balances(&fx.core, fx.vault);
    let preview = preview_with(&fx, &text, &bank_mapping(), &options, &overrides);
    assert_eq!((preview.new_rows, preview.moves), (2, 1));
    assert_eq!(command_count(&fx.core, fx.vault), commands);
    assert_eq!(balances(&fx.core, fx.vault), before);
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
