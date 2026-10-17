//! The built-in mappings, and how a header is recognised as one of them.

use super::{
    AmountSign, StatementAction, StatementDateFormat, StatementMapping, StatementPreset,
    StatementTypeRule,
};

/// Id of the card export preset.
pub(super) const CARD_TRANSACTIONS: &str = "card-transactions";

/// Header of the card export, in file order.
const CARD_HEADERS: [&str; 14] = [
    "timestamp",
    "type",
    "description",
    "status",
    "amount",
    "currency",
    "card",
    "card holder name",
    "original amount",
    "original currency",
    "cashback earned",
    "cashback currency",
    "category",
    "spending mode",
];

/// Id of the BudgetBakers Wallet export preset.
pub(super) const BUDGETBAKERS_WALLET: &str = "budgetbakers-wallet";

/// Header of the BudgetBakers Wallet export, in file order.
const WALLET_HEADERS: [&str; 12] = [
    "account",
    "category",
    "currency",
    "amount",
    "ref_currency_amount",
    "type",
    "payment_type",
    "note",
    "date",
    "transfer",
    "payee",
    "labels",
];

/// Every built-in preset.
pub(super) fn all() -> Vec<StatementPreset> {
    vec![
        StatementPreset {
            id: CARD_TRANSACTIONS.to_string(),
            name: "Card transactions".to_string(),
            mapping: card_transactions(),
        },
        StatementPreset {
            id: BUDGETBAKERS_WALLET.to_string(),
            name: "Wallet (BudgetBakers)".to_string(),
            mapping: budgetbakers_wallet(),
        },
    ]
}

/// The preset whose columns are all in `headers` (case-insensitive, trimmed;
/// extra columns are fine, so an export that grows a column is still
/// recognised).
pub(super) fn matching(headers: &[String]) -> Option<&'static str> {
    let present: Vec<String> = headers.iter().map(|h| h.trim().to_lowercase()).collect();
    let holds = |wanted: &[&str]| {
        wanted
            .iter()
            .all(|wanted| present.iter().any(|h| h == wanted))
    };
    [
        (CARD_TRANSACTIONS, &CARD_HEADERS[..]),
        (BUDGETBAKERS_WALLET, &WALLET_HEADERS[..]),
    ]
    .into_iter()
    .find_map(|(id, headers)| holds(headers).then_some(id))
}

/// BudgetBakers Wallet export: every account the app syncs in one file,
/// `;`-separated, a UTC instant per row, every amount positive with the
/// type saying the way. Only `Expense` and `Income` rows are taken, anything
/// else is skipped, so a type the app adds later never slips in. The sign
/// setting reads no row: the rules decide every action and `BySign` never
/// applies; outflow-positive matches how the file writes amounts (all of
/// them positive), and the sign as written is what a row's id holds either
/// way.
fn budgetbakers_wallet() -> StatementMapping {
    let rule = |value: &str, action: StatementAction| StatementTypeRule {
        value: value.to_string(),
        action,
    };
    StatementMapping {
        delimiter: ";".to_string(),
        date_column: "date".to_string(),
        date_format: StatementDateFormat::DateTimeUtc,
        amount_column: "amount".to_string(),
        amount_sign: AmountSign::OutflowPositive,
        decimal_comma: false,
        description_columns: vec!["note".to_string()],
        type_column: Some("type".to_string()),
        type_rules: vec![
            rule("Expense", StatementAction::Expense),
            rule("Income", StatementAction::Income),
        ],
        default_action: StatementAction::Skip,
        status_column: None,
        skip_statuses: Vec::new(),
        currency_column: Some("currency".to_string()),
        category_column: Some("category".to_string()),
        original_amount_column: None,
        original_currency_column: None,
        description_rules: Vec::new(),
        account_column: Some("account".to_string()),
        transfer_column: Some("transfer".to_string()),
    }
}

/// Card export: newest first, spends positive and refunds negative, a status
/// per card row, top-ups from a wallet the user has to pick.
fn card_transactions() -> StatementMapping {
    let rule = |value: &str, action: StatementAction| StatementTypeRule {
        value: value.to_string(),
        action,
    };
    StatementMapping {
        delimiter: ",".to_string(),
        date_column: "timestamp".to_string(),
        date_format: StatementDateFormat::DateTimeUtc,
        amount_column: "amount".to_string(),
        amount_sign: AmountSign::OutflowPositive,
        decimal_comma: false,
        description_columns: vec!["description".to_string()],
        type_column: Some("type".to_string()),
        type_rules: vec![
            rule("card_spend", StatementAction::Expense),
            rule("card_refund", StatementAction::Refund),
            rule(
                "topup",
                StatementAction::TransferIn {
                    from_wallet_id: None,
                },
            ),
            rule("liquid_deposit", StatementAction::Skip),
        ],
        default_action: StatementAction::BySign,
        status_column: Some("status".to_string()),
        skip_statuses: ["PENDING", "CANCELLED", "DECLINED", "REVERTED"]
            .map(str::to_string)
            .to_vec(),
        currency_column: Some("currency".to_string()),
        category_column: Some("category".to_string()),
        original_amount_column: Some("original amount".to_string()),
        original_currency_column: Some("original currency".to_string()),
        description_rules: Vec::new(),
        account_column: None,
        transfer_column: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers(values: &[&str]) -> Vec<String> {
        values.iter().map(|v| (*v).to_string()).collect()
    }

    #[test]
    fn the_card_header_is_recognised_in_any_case_and_order() {
        assert_eq!(matching(&headers(&CARD_HEADERS)), Some(CARD_TRANSACTIONS));
        let mut shuffled: Vec<String> = CARD_HEADERS
            .iter()
            .rev()
            .map(|h| format!(" {} ", h.to_uppercase()))
            .collect();
        shuffled.push("notes".to_string());
        assert_eq!(matching(&shuffled), Some(CARD_TRANSACTIONS));
    }

    #[test]
    fn a_header_missing_a_card_column_is_not_the_card_preset() {
        assert_eq!(matching(&headers(&CARD_HEADERS[1..])), None);
        assert_eq!(matching(&headers(&["Data", "Importo"])), None);
    }

    #[test]
    fn each_mapping_names_only_its_own_columns() {
        for (mapping, header) in [
            (card_transactions(), &CARD_HEADERS[..]),
            (budgetbakers_wallet(), &WALLET_HEADERS[..]),
        ] {
            let mut named = vec![mapping.date_column.clone(), mapping.amount_column.clone()];
            named.extend(mapping.description_columns.iter().cloned());
            named.extend(
                [
                    &mapping.type_column,
                    &mapping.status_column,
                    &mapping.currency_column,
                    &mapping.category_column,
                    &mapping.original_amount_column,
                    &mapping.original_currency_column,
                    &mapping.account_column,
                    &mapping.transfer_column,
                ]
                .into_iter()
                .flatten()
                .cloned(),
            );
            for column in named {
                assert!(header.contains(&column.as_str()), "{column}");
            }
        }
    }

    #[test]
    fn the_wallet_header_is_its_preset_and_not_the_cards() {
        assert_eq!(
            matching(&headers(&WALLET_HEADERS)),
            Some(BUDGETBAKERS_WALLET)
        );
        assert_eq!(matching(&headers(&WALLET_HEADERS[1..])), None);
    }
}
