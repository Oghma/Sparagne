use uuid::Uuid;

use api_types::transaction::TransactionDetailResponse;

use crate::{
    error::AppError,
    text::{Locale, TextKey, t},
};

/// Extracts wallet and flow IDs from a transaction's legs.
///
/// Returns (wallet_id, flow_id) where either may be None if not present in the
/// transaction.
pub(crate) fn extract_wallet_flow(
    detail: &TransactionDetailResponse,
) -> (Option<Uuid>, Option<Uuid>) {
    let mut wallet_id = None;
    let mut flow_id = None;
    for leg in &detail.legs {
        match leg.target {
            api_types::transaction::LegTarget::Wallet { wallet_id: id } => {
                wallet_id = Some(id);
            }
            api_types::transaction::LegTarget::Flow { flow_id: id } => {
                flow_id = Some(id);
            }
        }
    }
    (wallet_id, flow_id)
}

/// Extracts source and destination wallet IDs from a wallet transfer
/// transaction.
///
/// Returns (from_wallet_id, to_wallet_id) or an error if the transfer
/// structure is invalid.
pub(crate) fn extract_wallet_transfer(
    detail: &TransactionDetailResponse,
    locale: Locale,
) -> Result<(Uuid, Uuid), AppError> {
    let mut from_wallet = None;
    let mut to_wallet = None;
    for leg in &detail.legs {
        if let api_types::transaction::LegTarget::Wallet { wallet_id } = leg.target {
            if leg.amount_minor < 0 {
                from_wallet = Some(wallet_id);
            } else if leg.amount_minor > 0 {
                to_wallet = Some(wallet_id);
            }
        }
    }
    match (from_wallet, to_wallet) {
        (Some(from), Some(to)) => Ok((from, to)),
        _ => Err(AppError::Terminal(
            t(locale, TextKey::StateCannotDetermineWalletTransfer).to_string(),
        )),
    }
}

/// Extracts source and destination flow IDs from a flow transfer transaction.
///
/// Returns (from_flow_id, to_flow_id) or an error if the transfer structure is
/// invalid.
pub(crate) fn extract_flow_transfer(
    detail: &TransactionDetailResponse,
    locale: Locale,
) -> Result<(Uuid, Uuid), AppError> {
    let mut from_flow = None;
    let mut to_flow = None;
    for leg in &detail.legs {
        if let api_types::transaction::LegTarget::Flow { flow_id } = leg.target {
            if leg.amount_minor < 0 {
                from_flow = Some(flow_id);
            } else if leg.amount_minor > 0 {
                to_flow = Some(flow_id);
            }
        }
    }
    match (from_flow, to_flow) {
        (Some(from), Some(to)) => Ok((from, to)),
        _ => Err(AppError::Terminal(
            t(locale, TextKey::StateCannotDetermineFlowTransfer).to_string(),
        )),
    }
}
