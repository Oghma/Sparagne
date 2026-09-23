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

/// Every built-in preset.
pub(super) fn all() -> Vec<StatementPreset> {
    vec![StatementPreset {
        id: CARD_TRANSACTIONS.to_string(),
        name: "Card transactions".to_string(),
        mapping: card_transactions(),
    }]
}

/// The preset whose columns are all in `headers` (case-insensitive, trimmed;
/// extra columns are fine, so an export that grows a column is still
/// recognised).
pub(super) fn matching(headers: &[String]) -> Option<&'static str> {
    let present: Vec<String> = headers.iter().map(|h| h.trim().to_lowercase()).collect();
    let card = CARD_HEADERS
        .iter()
        .all(|wanted| present.iter().any(|h| h == wanted));
    card.then_some(CARD_TRANSACTIONS)
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
    fn the_card_mapping_names_only_card_columns() {
        let mapping = card_transactions();
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
            ]
            .into_iter()
            .flatten()
            .cloned(),
        );
        for column in named {
            assert!(CARD_HEADERS.contains(&column.as_str()), "{column}");
        }
    }
}
