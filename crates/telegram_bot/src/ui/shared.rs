use api_types::transaction::{TransactionKind, TransactionView};
use engine::{Currency as EngineCurrency, Money};

use crate::i18n::{self, TextKey};

pub(crate) fn flow_display_name(locale: i18n::Locale, is_unallocated: bool, name: &str) -> &str {
    if is_unallocated {
        i18n::t(locale, TextKey::UnallocatedFlow)
    } else {
        name
    }
}

pub(crate) fn tx_button_label(
    locale: i18n::Locale,
    currency: EngineCurrency,
    tx: &TransactionView,
) -> String {
    let amount = Money::new(tx.amount_minor).format(currency);
    let kind = match tx.kind {
        TransactionKind::Income => "+",
        TransactionKind::Expense => "-",
        TransactionKind::Refund => "r",
        TransactionKind::TransferWallet => i18n::t(locale, TextKey::TxKindTransferWallet),
        TransactionKind::TransferFlow => i18n::t(locale, TextKey::TxKindTransferFlow),
    };
    let category = tx.category.as_deref().unwrap_or("-");
    let voided = if tx.voided {
        i18n::t(locale, TextKey::TxVoidedSuffix)
    } else {
        ""
    };
    format!("{kind} {amount} • {category}{voided}")
}
