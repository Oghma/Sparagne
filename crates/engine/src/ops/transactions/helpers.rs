use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::{
    Currency, EngineError, Leg, LegTarget, ResultEngine, TransactionKind, TxMeta,
};

use super::super::{flow_wallet_signed_amount, normalize_optional_text};

pub(super) fn normalize_tx_meta(meta: &TxMeta) -> (Option<String>, Option<String>) {
    (
        normalize_optional_text(meta.category.as_deref()),
        normalize_optional_text(meta.note.as_deref()),
    )
}

pub(super) fn apply_optional_text_patch(
    existing: Option<String>,
    patch: Option<&str>,
) -> Option<String> {
    match patch {
        None => existing,
        Some(value) => normalize_optional_text(Some(value)),
    }
}

pub(super) fn apply_optional_datetime_patch(
    existing: DateTime<Utc>,
    patch: Option<DateTime<Utc>>,
) -> DateTime<Utc> {
    patch.unwrap_or(existing)
}

pub(super) fn parse_leg_id(raw: &str) -> ResultEngine<Uuid> {
    Uuid::parse_str(raw)
        .map_err(|_| EngineError::InvalidId("invalid leg id".to_string()))
}

pub(super) fn validate_transfer_legs<T, F>(
    legs: &[Leg],
    amount_minor: i64,
    length_label: &str,
    kind_label: &str,
    target_label: &str,
    extract_target: F,
) -> ResultEngine<()>
where
    T: Copy + Eq,
    F: Fn(&LegTarget) -> Option<T>,
{
    if legs.len() != 2 {
        return Err(EngineError::InvalidAmount(format!(
            "invalid {length_label}: expected 2 legs"
        )));
    }

    let mut targets: Vec<T> = Vec::with_capacity(legs.len());
    let mut has_neg = false;
    let mut has_pos = false;

    for leg in legs {
        let target = extract_target(&leg.target).ok_or_else(|| {
            EngineError::InvalidAmount(format!(
                "invalid {kind_label}: expected {target_label} legs"
            ))
        })?;
        targets.push(target);

        if leg.amount_minor == -amount_minor {
            has_neg = true;
        } else if leg.amount_minor == amount_minor {
            has_pos = true;
        } else {
            return Err(EngineError::InvalidAmount(format!(
                "invalid {kind_label}: unexpected leg amount"
            )));
        }
    }

    if !has_neg || !has_pos {
        return Err(EngineError::InvalidAmount(format!(
            "invalid {kind_label}: missing positive/negative leg"
        )));
    }
    if targets.len() == 2 && targets[0] == targets[1] {
        return Err(EngineError::InvalidAmount(format!(
            "invalid {kind_label}: from/to must differ"
        )));
    }

    Ok(())
}

pub(super) struct TransferLegInfo<T> {
    pub(super) from_target: T,
    pub(super) to_target: T,
    pub(super) from_leg_id: Uuid,
    pub(super) to_leg_id: Uuid,
}

pub(super) fn parse_transfer_leg_pairs<T, F>(
    leg_pairs: &[(crate::legs::Model, Leg)],
    kind_label: &str,
    target_label: &str,
    extract_target: F,
) -> ResultEngine<TransferLegInfo<T>>
where
    T: Copy + Eq,
    F: Fn(&LegTarget) -> Option<T>,
{
    if leg_pairs.len() != 2 {
        return Err(EngineError::InvalidAmount(format!(
            "invalid {kind_label}: expected 2 legs"
        )));
    }

    let mut from_target: Option<T> = None;
    let mut to_target: Option<T> = None;
    let mut from_leg_id: Option<Uuid> = None;
    let mut to_leg_id: Option<Uuid> = None;

    for (model, leg) in leg_pairs {
        let target = extract_target(&leg.target).ok_or_else(|| {
            EngineError::InvalidAmount(format!(
                "invalid {kind_label}: expected {target_label} legs"
            ))
        })?;
        if leg.amount_minor < 0 {
            from_target = Some(target);
            from_leg_id = Some(parse_leg_id(&model.id)?);
        } else if leg.amount_minor > 0 {
            to_target = Some(target);
            to_leg_id = Some(parse_leg_id(&model.id)?);
        }
    }

    let from_target = from_target.ok_or_else(|| {
        EngineError::InvalidAmount(format!(
            "invalid {kind_label}: missing negative leg"
        ))
    })?;
    let to_target = to_target.ok_or_else(|| {
        EngineError::InvalidAmount(format!(
            "invalid {kind_label}: missing positive leg"
        ))
    })?;
    let from_leg_id = from_leg_id.ok_or_else(|| {
        EngineError::InvalidAmount(format!("invalid {kind_label}: missing leg id"))
    })?;
    let to_leg_id = to_leg_id.ok_or_else(|| {
        EngineError::InvalidAmount(format!("invalid {kind_label}: missing leg id"))
    })?;

    Ok(TransferLegInfo {
        from_target,
        to_target,
        from_leg_id,
        to_leg_id,
    })
}

pub(super) fn resolve_transfer_targets<T: Copy + Eq>(
    info: &TransferLegInfo<T>,
    from_override: Option<T>,
    to_override: Option<T>,
    error_message: &str,
) -> ResultEngine<(T, T)> {
    let new_from = from_override.unwrap_or(info.from_target);
    let new_to = to_override.unwrap_or(info.to_target);
    if new_from == new_to {
        return Err(EngineError::InvalidAmount(error_message.to_string()));
    }
    Ok((new_from, new_to))
}

pub(super) fn apply_transfer_leg_updates<T, F>(
    leg_pairs: &[(crate::legs::Model, Leg)],
    kind_label: &str,
    vault_currency: Currency,
    from_leg_id: Uuid,
    to_leg_id: Uuid,
    new_from: T,
    new_to: T,
    new_amount_minor: i64,
    make_target: F,
    balance_updates: &mut Vec<(LegTarget, i64, i64)>,
    leg_updates: &mut Vec<(String, LegTarget, i64)>,
) -> ResultEngine<()>
where
    T: Copy + Eq,
    F: Fn(T) -> LegTarget,
{
    for (model, leg) in leg_pairs {
        if leg.currency != vault_currency {
            return Err(EngineError::CurrencyMismatch(format!(
                "vault currency is {}, got {}",
                vault_currency.code(),
                leg.currency.code()
            )));
        }
        let id = parse_leg_id(&model.id)?;
        let (new_target, new_amount) = if id == from_leg_id {
            (make_target(new_from), -new_amount_minor)
        } else if id == to_leg_id {
            (make_target(new_to), new_amount_minor)
        } else {
            return Err(EngineError::InvalidAmount(format!(
                "invalid {kind_label}: unexpected legs"
            )));
        };

        if leg.target == new_target {
            balance_updates.push((leg.target, leg.amount_minor, new_amount));
        } else {
            balance_updates.push((leg.target, leg.amount_minor, 0));
            balance_updates.push((new_target, 0, new_amount));
        }
        leg_updates.push((model.id.clone(), new_target, new_amount));
    }

    Ok(())
}

pub(super) fn validate_update_fields(
    kind: TransactionKind,
    wallet_id: Option<Uuid>,
    flow_id: Option<Uuid>,
    from_wallet_id: Option<Uuid>,
    to_wallet_id: Option<Uuid>,
    from_flow_id: Option<Uuid>,
    to_flow_id: Option<Uuid>,
) -> ResultEngine<()> {
    match kind {
        TransactionKind::Income | TransactionKind::Expense | TransactionKind::Refund => {
            if from_wallet_id.is_some()
                || to_wallet_id.is_some()
                || from_flow_id.is_some()
                || to_flow_id.is_some()
            {
                return Err(EngineError::InvalidAmount(
                    "invalid update: unexpected transfer fields".to_string(),
                ));
            }
        }
        TransactionKind::TransferWallet => {
            if wallet_id.is_some() || flow_id.is_some() || from_flow_id.is_some() || to_flow_id.is_some()
            {
                return Err(EngineError::InvalidAmount(
                    "invalid update: unexpected wallet/flow fields".to_string(),
                ));
            }
        }
        TransactionKind::TransferFlow => {
            if wallet_id.is_some()
                || flow_id.is_some()
                || from_wallet_id.is_some()
                || to_wallet_id.is_some()
            {
                return Err(EngineError::InvalidAmount(
                    "invalid update: unexpected wallet fields".to_string(),
                ));
            }
        }
    }

    Ok(())
}

pub(super) fn validate_flow_wallet_legs(
    kind: TransactionKind,
    amount_minor: i64,
    legs: &[Leg],
) -> ResultEngine<()> {
    if legs.len() != 2 {
        return Err(EngineError::InvalidAmount(
            "invalid transaction: expected 2 legs".to_string(),
        ));
    }
    let expected = flow_wallet_signed_amount(kind, amount_minor)?;
    let (mut wallet_legs, mut flow_legs) = (0, 0);
    for leg in legs {
        match leg.target {
            LegTarget::Wallet { .. } => wallet_legs += 1,
            LegTarget::Flow { .. } => flow_legs += 1,
        }
        if leg.amount_minor != expected {
            return Err(EngineError::InvalidAmount(
                "invalid transaction: unexpected leg amount".to_string(),
            ));
        }
    }
    if wallet_legs != 1 || flow_legs != 1 {
        return Err(EngineError::InvalidAmount(
            "invalid transaction: expected one wallet leg and one flow leg".to_string(),
        ));
    }
    Ok(())
}

pub(super) fn extract_flow_wallet_targets(
    leg_pairs: &[(crate::legs::Model, Leg)],
) -> ResultEngine<(Uuid, Uuid)> {
    if leg_pairs.len() != 2 {
        return Err(EngineError::InvalidAmount(
            "invalid transaction: expected 2 legs".to_string(),
        ));
    }

    let mut existing_wallet_id: Option<Uuid> = None;
    let mut existing_flow_id: Option<Uuid> = None;
    for (_, leg) in leg_pairs {
        match leg.target {
            LegTarget::Wallet { wallet_id } => existing_wallet_id = Some(wallet_id),
            LegTarget::Flow { flow_id } => existing_flow_id = Some(flow_id),
        }
    }
    let existing_wallet_id = existing_wallet_id.ok_or_else(|| {
        EngineError::InvalidAmount("invalid transaction: missing wallet leg".to_string())
    })?;
    let existing_flow_id = existing_flow_id.ok_or_else(|| {
        EngineError::InvalidAmount("invalid transaction: missing flow leg".to_string())
    })?;

    Ok((existing_wallet_id, existing_flow_id))
}

pub(super) fn apply_flow_wallet_leg_updates(
    leg_pairs: &[(crate::legs::Model, Leg)],
    vault_currency: Currency,
    new_wallet_id: Uuid,
    new_flow_id: Uuid,
    new_signed_amount: i64,
    balance_updates: &mut Vec<(LegTarget, i64, i64)>,
    leg_updates: &mut Vec<(String, LegTarget, i64)>,
) -> ResultEngine<()> {
    for (model, leg) in leg_pairs {
        if leg.currency != vault_currency {
            return Err(EngineError::CurrencyMismatch(format!(
                "vault currency is {}, got {}",
                vault_currency.code(),
                leg.currency.code()
            )));
        }

        let (new_target, new_amount) = match leg.target {
            LegTarget::Wallet { .. } => (
                LegTarget::Wallet {
                    wallet_id: new_wallet_id,
                },
                new_signed_amount,
            ),
            LegTarget::Flow { .. } => (
                LegTarget::Flow { flow_id: new_flow_id },
                new_signed_amount,
            ),
        };

        if leg.target == new_target {
            balance_updates.push((leg.target, leg.amount_minor, new_amount));
        } else {
            balance_updates.push((leg.target, leg.amount_minor, 0));
            balance_updates.push((new_target, 0, new_amount));
        }
        leg_updates.push((model.id.clone(), new_target, new_amount));
    }

    Ok(())
}
