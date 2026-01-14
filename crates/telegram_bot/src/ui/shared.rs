use api_types::transaction::{TransactionKind, TransactionView};
use engine::{Currency as EngineCurrency, Money};

pub(crate) fn flow_display_name(is_unallocated: bool, name: &str) -> &str {
    if is_unallocated { "Non in flow" } else { name }
}

pub(crate) fn tx_button_label(currency: EngineCurrency, tx: &TransactionView) -> String {
    let amount = Money::new(tx.amount_minor).format(currency);
    let kind = match tx.kind {
        TransactionKind::Income => "+",
        TransactionKind::Expense => "-",
        TransactionKind::Refund => "r",
        TransactionKind::TransferWallet => "tw",
        TransactionKind::TransferFlow => "tf",
    };
    let category = tx.category.as_deref().unwrap_or("-");
    let voided = if tx.voided { " • void" } else { "" };
    format!("{kind} {amount} • {category}{voided}")
}
