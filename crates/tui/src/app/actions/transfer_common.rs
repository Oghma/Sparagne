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
) -> std::result::Result<i64, String> {
    let amount = match Money::parse_major(amount_str.trim(), currency) {
        Ok(money) => money.minor().abs(),
        Err(_) => return Err("Importo non valido.".to_string()),
    };
    if amount <= 0 {
        return Err("Importo deve essere > 0.".to_string());
    }
    Ok(amount)
}

/// Validates that two IDs are different
pub(crate) fn validate_different_ids<T: Eq>(from: T, to: T) -> std::result::Result<(), String> {
    if from == to {
        Err("Scegli due elementi diversi.".to_string())
    } else {
        Ok(())
    }
}

/// Validates minimum count of items
pub(crate) fn validate_minimum_count(
    count: usize,
    transfer_type: TransferType,
) -> std::result::Result<(), String> {
    if count < 2 {
        let entity = match transfer_type {
            TransferType::Wallet => "wallet",
            TransferType::Flow => "flow",
        };
        Err(format!("Servono almeno 2 {}.", entity))
    } else {
        Ok(())
    }
}
