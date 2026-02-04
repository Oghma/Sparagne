use crate::text::{Locale, TextKey, t};
use engine::{Currency, Money};

/// Type of transfer (wallet-to-wallet or flow-to-flow)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TransferType {
    Wallet,
    Flow,
}

/// Validates transfer amount (positive, non-zero)
pub(crate) fn validate_transfer_amount(
    amount_str: &str,
    currency: Currency,
    locale: Locale,
) -> std::result::Result<i64, String> {
    let amount = match Money::parse_major(amount_str.trim(), currency) {
        Ok(money) => money.minor().abs(),
        Err(_) => return Err(t(locale, TextKey::ValidationAmountInvalid).to_string()),
    };
    if amount <= 0 {
        return Err(t(locale, TextKey::ValidationAmountPositive).to_string());
    }
    Ok(amount)
}

/// Validates that two IDs are different
pub(crate) fn validate_different_ids<T: Eq>(from: T, to: T, locale: Locale) -> std::result::Result<(), String> {
    if from == to {
        Err(t(locale, TextKey::ValidationTransferSameElements).to_string())
    } else {
        Ok(())
    }
}

/// Validates minimum count of items
pub(crate) fn validate_minimum_count(
    count: usize,
    _transfer_type: TransferType,
    locale: Locale,
) -> std::result::Result<(), String> {
    if count < 2 {
        Err(t(locale, TextKey::ValidationTransferMinimumTwo).to_string())
    } else {
        Ok(())
    }
}
