use base64::Engine as _;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

use sea_orm::{
    ActiveValue, Condition, DatabaseTransaction, QueryFilter, QueryOrder, QuerySelect,
    TransactionTrait, prelude::*,
};

use crate::{
    cash_flows, legs, transactions, vault, vault_memberships, wallets, Currency, EngineError,
    ExpenseCmd, IncomeCmd, Leg, LegTarget, RefundCmd, ResultEngine, Transaction, TransactionKind,
    TransferFlowCmd, TransferWalletCmd, TxMeta, UpdateTransactionCmd,
};
use crate::util::{ensure_vault_currency, model_currency, validate_flow_mode_fields};

use super::{
    build_transaction, flow_wallet_legs, flow_wallet_signed_amount, normalize_optional_text,
    parse_vault_currency, transfer_flow_legs, transfer_wallet_legs, with_tx, Engine,
};

/// Filters for listing transactions.
///
/// `from` is inclusive and `to` is exclusive (`[from, to)`), both in UTC.
#[derive(Clone, Debug, Default)]
pub struct TransactionListFilter {
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    /// If present, acts as an allow-list of kinds to return.
    pub kinds: Option<Vec<TransactionKind>>,
    /// If true, includes voided transactions (default: false).
    pub include_voided: bool,
    /// If true, includes internal transfers (default: false).
    pub include_transfers: bool,
}

fn validate_list_filter(filter: &TransactionListFilter) -> ResultEngine<()> {
    if let (Some(from), Some(to)) = (filter.from, filter.to)
        && from >= to
    {
        return Err(EngineError::InvalidAmount(
            "invalid range: from must be < to".to_string(),
        ));
    }
    if filter.kinds.as_ref().is_some_and(|k| k.is_empty()) {
        return Err(EngineError::InvalidAmount(
            "kinds must not be empty".to_string(),
        ));
    }
    Ok(())
}

fn parse_leg_id(raw: &str) -> ResultEngine<Uuid> {
    Uuid::parse_str(raw)
        .map_err(|_| EngineError::InvalidAmount("invalid leg id".to_string()))
}

fn validate_transfer_legs<T, F>(
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

struct TransferLegInfo<T> {
    from_target: T,
    to_target: T,
    from_leg_id: Uuid,
    to_leg_id: Uuid,
}

fn parse_transfer_leg_pairs<T, F>(
    leg_pairs: &[(legs::Model, Leg)],
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

fn apply_transfer_leg_updates<T, F>(
    leg_pairs: &[(legs::Model, Leg)],
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

fn validate_update_fields(
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
            if wallet_id.is_some()
                || flow_id.is_some()
                || from_flow_id.is_some()
                || to_flow_id.is_some()
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

fn validate_flow_wallet_legs(
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

fn extract_flow_wallet_targets(
    leg_pairs: &[(legs::Model, Leg)],
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

fn apply_flow_wallet_leg_updates(
    leg_pairs: &[(legs::Model, Leg)],
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

trait ApplyTxFilters: QueryFilter + Sized {
    fn apply_tx_filters(self, filter: &TransactionListFilter) -> Self;
}

impl<T> ApplyTxFilters for T
where
    T: QueryFilter + Sized,
{
    fn apply_tx_filters(mut self, filter: &TransactionListFilter) -> Self {
        if let Some(from) = filter.from {
            self = self.filter(transactions::Column::OccurredAt.gte(from));
        }
        if let Some(to) = filter.to {
            self = self.filter(transactions::Column::OccurredAt.lt(to));
        }

        if !filter.include_voided {
            self = self.filter(transactions::Column::VoidedAt.is_null());
        }
        if let Some(kinds) = &filter.kinds {
            let kinds: Vec<String> = kinds.iter().map(|k| k.as_str().to_string()).collect();
            self = self.filter(transactions::Column::Kind.is_in(kinds));
        } else if !filter.include_transfers {
            self = self.filter(transactions::Column::Kind.is_not_in([
                TransactionKind::TransferWallet.as_str(),
                TransactionKind::TransferFlow.as_str(),
            ]));
        }

        self
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct TransactionsCursor {
    occurred_at: DateTime<Utc>,
    transaction_id: String,
}

impl TransactionsCursor {
    fn encode(&self) -> ResultEngine<String> {
        let bytes = serde_json::to_vec(self)
            .map_err(|_| EngineError::InvalidAmount("invalid transactions cursor".to_string()))?;
        Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes))
    }

    fn decode(input: &str) -> ResultEngine<Self> {
        let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(input.as_bytes())
            .map_err(|_| EngineError::InvalidAmount("invalid transactions cursor".to_string()))?;
        serde_json::from_slice::<Self>(&bytes)
            .map_err(|_| EngineError::InvalidAmount("invalid transactions cursor".to_string()))
    }
}

fn normalize_tx_meta(meta: &TxMeta) -> (Option<String>, Option<String>) {
    (
        normalize_optional_text(meta.category.as_deref()),
        normalize_optional_text(meta.note.as_deref()),
    )
}

fn apply_optional_text_patch(existing: Option<String>, patch: Option<&str>) -> Option<String> {
    match patch {
        None => existing,
        Some(value) => normalize_optional_text(Some(value)),
    }
}

fn apply_optional_datetime_patch(
    existing: DateTime<Utc>,
    patch: Option<DateTime<Utc>>,
) -> DateTime<Utc> {
    patch.unwrap_or(existing)
}

impl Engine {
    async fn create_flow_wallet_transaction(
        &self,
        db_tx: &DatabaseTransaction,
        vault_id: &str,
        user_id: &str,
        flow_id: Option<Uuid>,
        wallet_id: Option<Uuid>,
        amount_minor: i64,
        kind: TransactionKind,
        meta: TxMeta,
    ) -> ResultEngine<Uuid> {
        let (category, note) = normalize_tx_meta(&meta);
        let vault_model = self
            .require_vault_by_id_write(db_tx, vault_id, user_id)
            .await?;
        let currency = parse_vault_currency(vault_model.currency.as_str())?;
        let resolved_flow_id = self.resolve_flow_id(db_tx, vault_id, flow_id).await?;
        let resolved_wallet_id = self.resolve_wallet_id(db_tx, vault_id, wallet_id).await?;
        let leg_amount_minor = flow_wallet_signed_amount(kind, amount_minor)?;

        let tx = build_transaction(
            vault_id,
            kind,
            meta.occurred_at,
            amount_minor,
            currency,
            category,
            note,
            user_id,
            meta.idempotency_key,
            None,
        )?;
        let legs = flow_wallet_legs(
            tx.id,
            resolved_wallet_id,
            resolved_flow_id,
            leg_amount_minor,
            currency,
        );

        self.create_transaction_with_legs(db_tx, vault_id, currency, &tx, &legs)
            .await
    }

    async fn apply_wallet_delta(
        &self,
        db_tx: &DatabaseTransaction,
        vault_id: &str,
        vault_currency: Currency,
        wallet_new_balances: &mut HashMap<Uuid, i64>,
        wallet_id: Uuid,
        delta_minor: i64,
    ) -> ResultEngine<()> {
        let wallet_model = wallets::Entity::find_by_id(wallet_id.to_string())
            .filter(wallets::Column::VaultId.eq(vault_id.to_string()))
            .one(db_tx)
            .await?
            .ok_or_else(|| EngineError::KeyNotFound("wallet not exists".to_string()))?;
        let wallet_currency = model_currency(wallet_model.currency.as_str())?;
        ensure_vault_currency(vault_currency, wallet_currency)?;

        let entry = wallet_new_balances
            .entry(wallet_id)
            .or_insert(wallet_model.balance);
        *entry += delta_minor;
        Ok(())
    }

    async fn apply_flow_change(
        &self,
        db_tx: &DatabaseTransaction,
        vault_id: &str,
        vault_currency: Currency,
        flow_previews: &mut HashMap<Uuid, crate::CashFlow>,
        flow_id: Uuid,
        old_amount_minor: i64,
        new_amount_minor: i64,
    ) -> ResultEngine<()> {
        let flow_model = cash_flows::Entity::find_by_id(flow_id.to_string())
            .filter(cash_flows::Column::VaultId.eq(vault_id.to_string()))
            .one(db_tx)
            .await?
            .ok_or_else(|| EngineError::KeyNotFound("cash_flow not exists".to_string()))?;
        validate_flow_mode_fields(
            &flow_model.name,
            flow_model.max_balance,
            flow_model.income_balance,
        )?;
        let flow_currency = model_currency(flow_model.currency.as_str())?;
        ensure_vault_currency(vault_currency, flow_currency)?;
        let system_kind = flow_model
            .system_kind
            .as_deref()
            .and_then(|k| cash_flows::SystemFlowKind::try_from(k).ok());
        let entry = flow_previews.entry(flow_id).or_insert_with(|| crate::CashFlow {
            id: flow_id,
            name: flow_model.name.clone(),
            system_kind,
            balance: flow_model.balance,
            max_balance: flow_model.max_balance,
            income_balance: flow_model.income_balance,
            currency: flow_currency,
            archived: flow_model.archived,
        });
        entry.apply_leg_change(old_amount_minor, new_amount_minor)?;
        Ok(())
    }

    pub(super) async fn create_transaction_with_legs(
        &self,
        db_tx: &DatabaseTransaction,
        vault_id: &str,
        vault_currency: Currency,
        tx: &Transaction,
        legs: &[Leg],
    ) -> ResultEngine<Uuid> {
        if legs.is_empty() {
            return Err(EngineError::InvalidAmount(
                "transaction must have at least one leg".to_string(),
            ));
        }
        for leg in legs {
            if leg.transaction_id != tx.id {
                return Err(EngineError::InvalidAmount(
                    "invalid leg: transaction_id mismatch".to_string(),
                ));
            }
            if leg.amount_minor == 0 {
                return Err(EngineError::InvalidAmount(
                    "invalid leg: amount_minor must not be 0".to_string(),
                ));
            }
        }

        // Validate kind-specific invariants (kept strict for now).
        match tx.kind {
            TransactionKind::Income | TransactionKind::Expense | TransactionKind::Refund => {
                validate_flow_wallet_legs(tx.kind, tx.amount_minor, legs)?;
            }
            TransactionKind::TransferWallet => {
                validate_transfer_legs(
                    legs,
                    tx.amount_minor,
                    "transfer",
                    "transfer_wallet",
                    "wallet",
                    |target| match target {
                        LegTarget::Wallet { wallet_id } => Some(*wallet_id),
                        _ => None,
                    },
                )?;
            }
            TransactionKind::TransferFlow => {
                validate_transfer_legs(
                    legs,
                    tx.amount_minor,
                    "transfer",
                    "transfer_flow",
                    "flow",
                    |target| match target {
                        LegTarget::Flow { flow_id } => Some(*flow_id),
                        _ => None,
                    },
                )?;
            }
        }

        if tx.currency != vault_currency {
            return Err(EngineError::CurrencyMismatch(format!(
                "vault currency is {}, got {}",
                vault_currency.code(),
                tx.currency.code()
            )));
        }

        if let Some(key) = tx.idempotency_key.as_deref() {
            let existing = transactions::Entity::find()
                .filter(transactions::Column::VaultId.eq(vault_id.to_string()))
                .filter(transactions::Column::CreatedBy.eq(tx.created_by.clone()))
                .filter(transactions::Column::IdempotencyKey.eq(key.to_string()))
                .one(db_tx)
                .await?;
            if let Some(existing) = existing {
                return Uuid::parse_str(&existing.id)
                    .map_err(|_| EngineError::InvalidAmount("invalid transaction id".to_string()));
            }
        }

        // Validate currency and domain invariants by simulating balance changes, while
        // also computing the resulting denormalized balances to persist.
        let mut wallet_new_balances: HashMap<Uuid, i64> = HashMap::new();
        let mut flow_previews: HashMap<Uuid, crate::CashFlow> = HashMap::new();

        for leg in legs {
            if leg.currency != vault_currency {
                return Err(EngineError::CurrencyMismatch(format!(
                    "vault currency is {}, got {}",
                    vault_currency.code(),
                    leg.currency.code()
                )));
            }
            match leg.target {
                LegTarget::Wallet { wallet_id } => {
                    self.apply_wallet_delta(
                        db_tx,
                        vault_id,
                        vault_currency,
                        &mut wallet_new_balances,
                        wallet_id,
                        leg.amount_minor,
                    )
                    .await?;
                }
                LegTarget::Flow { flow_id } => {
                    self.apply_flow_change(
                        db_tx,
                        vault_id,
                        vault_currency,
                        &mut flow_previews,
                        flow_id,
                        0,
                        leg.amount_minor,
                    )
                    .await?;
                }
            }
        }

        if let Err(err) = transactions::ActiveModel::from(tx).insert(db_tx).await {
            if tx.idempotency_key.is_some() {
                let key = tx.idempotency_key.as_deref().unwrap_or_default();
                let existing = transactions::Entity::find()
                    .filter(transactions::Column::VaultId.eq(vault_id.to_string()))
                    .filter(transactions::Column::CreatedBy.eq(tx.created_by.clone()))
                    .filter(transactions::Column::IdempotencyKey.eq(key.to_string()))
                    .one(db_tx)
                    .await?;
                if let Some(existing) = existing {
                    return Uuid::parse_str(&existing.id).map_err(|_| {
                        EngineError::InvalidAmount("invalid transaction id".to_string())
                    });
                }
            }
            return Err(err.into());
        }
        for leg in legs {
            legs::ActiveModel::from(leg).insert(db_tx).await?;
        }

        self.persist_targets(db_tx, wallet_new_balances, flow_previews)
            .await?;

        Ok(tx.id)
    }

    async fn preview_apply_leg_updates(
        &self,
        db_tx: &DatabaseTransaction,
        vault_id: &str,
        vault_currency: Currency,
        updates: &[(LegTarget, i64, i64)],
    ) -> ResultEngine<(HashMap<Uuid, i64>, HashMap<Uuid, crate::CashFlow>)> {
        let mut wallet_new_balances: HashMap<Uuid, i64> = HashMap::new();
        let mut flow_previews: HashMap<Uuid, crate::CashFlow> = HashMap::new();

        for (target, old_amount_minor, new_amount_minor) in updates {
            match *target {
                LegTarget::Wallet { wallet_id } => {
                    let delta_minor = *new_amount_minor - *old_amount_minor;
                    self.apply_wallet_delta(
                        db_tx,
                        vault_id,
                        vault_currency,
                        &mut wallet_new_balances,
                        wallet_id,
                        delta_minor,
                    )
                    .await?;
                }
                LegTarget::Flow { flow_id } => {
                    self.apply_flow_change(
                        db_tx,
                        vault_id,
                        vault_currency,
                        &mut flow_previews,
                        flow_id,
                        *old_amount_minor,
                        *new_amount_minor,
                    )
                    .await?;
                }
            }
        }

        Ok((wallet_new_balances, flow_previews))
    }

    async fn persist_targets(
        &self,
        db_tx: &DatabaseTransaction,
        wallet_new_balances: HashMap<Uuid, i64>,
        flow_previews: HashMap<Uuid, crate::CashFlow>,
    ) -> ResultEngine<()> {
        for (wallet_id, new_balance) in wallet_new_balances {
            let wallet_model = wallets::ActiveModel {
                id: ActiveValue::Set(wallet_id.to_string()),
                balance: ActiveValue::Set(new_balance),
                ..Default::default()
            };
            wallet_model.update(db_tx).await?;
        }

        for (flow_id, flow) in flow_previews {
            let flow_model = cash_flows::ActiveModel {
                id: ActiveValue::Set(flow_id.to_string()),
                balance: ActiveValue::Set(flow.balance),
                income_balance: ActiveValue::Set(flow.income_balance),
                ..Default::default()
            };
            flow_model.update(db_tx).await?;
        }

        Ok(())
    }

    /// Create an income transaction (increases both wallet and flow).
    pub async fn income(&self, cmd: IncomeCmd) -> ResultEngine<Uuid> {
        let IncomeCmd {
            vault_id,
            amount_minor,
            flow_id,
            wallet_id,
            meta,
            user_id,
        } = cmd;
        with_tx!(self, |db_tx| {
            let id = self
                .create_flow_wallet_transaction(
                    &db_tx,
                    &vault_id,
                    &user_id,
                    flow_id,
                    wallet_id,
                    amount_minor,
                    TransactionKind::Income,
                    meta,
                )
                .await?;
            Ok(id)
        })
    }

    /// Create an expense transaction (decreases both wallet and flow).
    pub async fn expense(&self, cmd: ExpenseCmd) -> ResultEngine<Uuid> {
        let ExpenseCmd {
            vault_id,
            amount_minor,
            flow_id,
            wallet_id,
            meta,
            user_id,
        } = cmd;
        with_tx!(self, |db_tx| {
            let id = self
                .create_flow_wallet_transaction(
                    &db_tx,
                    &vault_id,
                    &user_id,
                    flow_id,
                    wallet_id,
                    amount_minor,
                    TransactionKind::Expense,
                    meta,
                )
                .await?;
            Ok(id)
        })
    }

    /// Create a refund transaction (increases both wallet and flow).
    ///
    /// A refund is modeled as its own `TransactionKind::Refund` instead of a
    /// negative expense, to keep reporting correct and explicit.
    pub async fn refund(&self, cmd: RefundCmd) -> ResultEngine<Uuid> {
        let RefundCmd {
            vault_id,
            amount_minor,
            flow_id,
            wallet_id,
            meta,
            user_id,
        } = cmd;
        with_tx!(self, |db_tx| {
            let id = self
                .create_flow_wallet_transaction(
                    &db_tx,
                    &vault_id,
                    &user_id,
                    flow_id,
                    wallet_id,
                    amount_minor,
                    TransactionKind::Refund,
                    meta,
                )
                .await?;
            Ok(id)
        })
    }

    pub async fn transfer_wallet(&self, cmd: TransferWalletCmd) -> ResultEngine<Uuid> {
        if cmd.from_wallet_id == cmd.to_wallet_id {
            return Err(EngineError::InvalidAmount(
                "from_wallet_id and to_wallet_id must differ".to_string(),
            ));
        }
        let TransferWalletCmd {
            vault_id,
            amount_minor,
            from_wallet_id,
            to_wallet_id,
            note,
            idempotency_key,
            occurred_at,
            user_id,
        } = cmd;
        let note = normalize_optional_text(note.as_deref());
        with_tx!(self, |db_tx| {
            let vault_model = self
                .require_vault_by_id_write(&db_tx, &vault_id, &user_id)
                .await?;
            let currency = parse_vault_currency(vault_model.currency.as_str())?;
            // Ensure wallets belong to the vault.
            self.resolve_wallet_id(&db_tx, &vault_id, Some(from_wallet_id))
                .await?;
            self.resolve_wallet_id(&db_tx, &vault_id, Some(to_wallet_id))
                .await?;

            let tx = build_transaction(
                &vault_id,
                TransactionKind::TransferWallet,
                occurred_at,
                amount_minor,
                currency,
                None,
                note,
                &user_id,
                idempotency_key,
                None,
            )?;
            let legs = transfer_wallet_legs(
                tx.id,
                from_wallet_id,
                to_wallet_id,
                amount_minor,
                currency,
            );

            let id = self
                .create_transaction_with_legs(&db_tx, &vault_id, currency, &tx, &legs)
                .await?;
            Ok(id)
        })
    }

    pub async fn transfer_flow(&self, cmd: TransferFlowCmd) -> ResultEngine<Uuid> {
        if cmd.from_flow_id == cmd.to_flow_id {
            return Err(EngineError::InvalidAmount(
                "from_flow_id and to_flow_id must differ".to_string(),
            ));
        }
        let TransferFlowCmd {
            vault_id,
            amount_minor,
            from_flow_id,
            to_flow_id,
            note,
            idempotency_key,
            occurred_at,
            user_id,
        } = cmd;
        let note = normalize_optional_text(note.as_deref());
        with_tx!(self, |db_tx| {
            let vault_model = vault::Entity::find_by_id(vault_id.to_string())
                .one(&db_tx)
                .await?
                .ok_or_else(|| EngineError::KeyNotFound("vault not exists".to_string()))?;
            let currency = parse_vault_currency(vault_model.currency.as_str())?;
            // AuthZ:
            // - Vault owner/editor can transfer between any flows in the vault.
            // - Otherwise, user must be editor/owner on both flows (via flow_memberships).
            if self
                .has_vault_write_access(&db_tx, &vault_id, &user_id)
                .await?
            {
                self.resolve_flow_id(&db_tx, &vault_id, Some(from_flow_id))
                    .await?;
                self.resolve_flow_id(&db_tx, &vault_id, Some(to_flow_id))
                    .await?;
            } else {
                self.require_flow_write(&db_tx, &vault_id, from_flow_id, &user_id)
                    .await?;
                self.require_flow_write(&db_tx, &vault_id, to_flow_id, &user_id)
                    .await?;
            }

            let tx = build_transaction(
                &vault_id,
                TransactionKind::TransferFlow,
                occurred_at,
                amount_minor,
                currency,
                None,
                note,
                &user_id,
                idempotency_key,
                None,
            )?;
            let legs = transfer_flow_legs(tx.id, from_flow_id, to_flow_id, amount_minor, currency);

            let id = self
                .create_transaction_with_legs(&db_tx, &vault_id, currency, &tx, &legs)
                .await?;
            Ok(id)
        })
    }

    /// Voids a transaction (soft delete).
    ///
    /// This:
    /// - sets `voided_at`/`voided_by` on the transaction row
    /// - reverts all legs effects on wallet/flow balances
    ///
    /// Voided transactions are hidden by default in lists/reports.
    pub async fn void_transaction(
        &self,
        vault_id: &str,
        transaction_id: Uuid,
        user_id: &str,
        voided_at: DateTime<Utc>,
    ) -> ResultEngine<()> {
        with_tx!(self, |db_tx| {
            let vault_model = self
                .require_vault_by_id_write(&db_tx, vault_id, user_id)
                .await?;
            let vault_currency = parse_vault_currency(vault_model.currency.as_str())?;

            let tx_model = transactions::Entity::find_by_id(transaction_id.to_string())
                .one(&db_tx)
                .await?
                .ok_or_else(|| EngineError::KeyNotFound("transaction not exists".to_string()))?;
            if tx_model.vault_id != vault_id {
                return Err(EngineError::KeyNotFound(
                    "transaction not exists".to_string(),
                ));
            }
            if tx_model.voided_at.is_some() {
                return Err(EngineError::InvalidAmount(
                    "transaction already voided".to_string(),
                ));
            }

            let leg_models = legs::Entity::find()
                .filter(legs::Column::TransactionId.eq(transaction_id.to_string()))
                .all(&db_tx)
                .await?;

            let mut updates: Vec<(LegTarget, i64, i64)> = Vec::with_capacity(leg_models.len());
            for leg_model in leg_models {
                let leg = Leg::try_from(leg_model)?;
                updates.push((leg.target, leg.amount_minor, 0));
            }

            let (wallet_new_balances, flow_previews) = self
                .preview_apply_leg_updates(&db_tx, vault_id, vault_currency, &updates)
                .await?;

            let tx_active = transactions::ActiveModel {
                id: ActiveValue::Set(transaction_id.to_string()),
                voided_at: ActiveValue::Set(Some(voided_at)),
                voided_by: ActiveValue::Set(Some(user_id.to_string())),
                ..Default::default()
            };
            tx_active.update(&db_tx).await?;

            self.persist_targets(&db_tx, wallet_new_balances, flow_previews)
                .await?;

            Ok(())
        })
    }

    /// Updates an existing transaction (amount, metadata, and/or targets).
    ///
    /// The allowed target edits depend on the transaction kind:
    /// - `Income`/`Expense`/`Refund`: wallet and/or flow can be changed
    /// - `TransferWallet`: from/to wallets can be changed
    /// - `TransferFlow`: from/to flows can be changed
    pub async fn update_transaction(&self, cmd: UpdateTransactionCmd) -> ResultEngine<()> {
        let vault_id = cmd.vault_id;
        let vault_id = vault_id.as_str();
        let transaction_id = cmd.transaction_id;
        let user_id = cmd.user_id;
        let user_id = user_id.as_str();
        let amount_minor = cmd.amount_minor;
        let wallet_id = cmd.wallet_id;
        let flow_id = cmd.flow_id;
        let from_wallet_id = cmd.from_wallet_id;
        let to_wallet_id = cmd.to_wallet_id;
        let from_flow_id = cmd.from_flow_id;
        let to_flow_id = cmd.to_flow_id;
        let category = cmd.category.as_deref();
        let note = cmd.note.as_deref();
        let occurred_at = cmd.occurred_at;
        with_tx!(self, |db_tx| {
            let vault_model = self
                .require_vault_by_id_write(&db_tx, vault_id, user_id)
                .await?;
            let vault_currency = parse_vault_currency(vault_model.currency.as_str())?;

            let tx_model = transactions::Entity::find_by_id(transaction_id.to_string())
                .one(&db_tx)
                .await?
                .ok_or_else(|| EngineError::KeyNotFound("transaction not exists".to_string()))?;
            if tx_model.vault_id != vault_id {
                return Err(EngineError::KeyNotFound(
                    "transaction not exists".to_string(),
                ));
            }
            if tx_model.voided_at.is_some() {
                return Err(EngineError::InvalidAmount(
                    "cannot update a voided transaction".to_string(),
                ));
            }

            let kind = TransactionKind::try_from(tx_model.kind.as_str())?;
            let new_amount_minor = amount_minor.unwrap_or(tx_model.amount_minor);
            if new_amount_minor <= 0 {
                return Err(EngineError::InvalidAmount(
                    "amount_minor must be > 0".to_string(),
                ));
            }

            let new_occurred_at = apply_optional_datetime_patch(tx_model.occurred_at, occurred_at);
            let new_category = apply_optional_text_patch(tx_model.category.clone(), category);
            let new_note = apply_optional_text_patch(tx_model.note.clone(), note);

            let leg_models = legs::Entity::find()
                .filter(legs::Column::TransactionId.eq(transaction_id.to_string()))
                .all(&db_tx)
                .await?;

            let mut leg_pairs: Vec<(legs::Model, Leg)> = Vec::with_capacity(leg_models.len());
            for leg_model in leg_models {
                let leg = Leg::try_from(leg_model.clone())?;
                leg_pairs.push((leg_model, leg));
            }

            let mut balance_updates: Vec<(LegTarget, i64, i64)> = Vec::new();
            let mut leg_updates: Vec<(String, LegTarget, i64)> = Vec::new();

            match kind {
                TransactionKind::Income | TransactionKind::Expense | TransactionKind::Refund => {
                    let (existing_wallet_id, existing_flow_id) =
                        extract_flow_wallet_targets(&leg_pairs)?;
                    let new_wallet_id = wallet_id.unwrap_or(existing_wallet_id);
                    let new_flow_id = flow_id.unwrap_or(existing_flow_id);
                    self.require_wallet_in_vault(&db_tx, vault_id, new_wallet_id)
                        .await?;
                    self.require_flow_in_vault(&db_tx, vault_id, new_flow_id)
                        .await?;

                    let new_signed_amount = flow_wallet_signed_amount(kind, new_amount_minor)?;

                    apply_flow_wallet_leg_updates(
                        &leg_pairs,
                        vault_currency,
                        new_wallet_id,
                        new_flow_id,
                        new_signed_amount,
                        &mut balance_updates,
                        &mut leg_updates,
                    )?;
                }
                TransactionKind::TransferWallet => {
                    let info = parse_transfer_leg_pairs(
                        &leg_pairs,
                        "transfer_wallet",
                        "wallet",
                        |target| match target {
                            LegTarget::Wallet { wallet_id } => Some(*wallet_id),
                            _ => None,
                        },
                    )?;

                    let new_from = from_wallet_id.unwrap_or(info.from_target);
                    let new_to = to_wallet_id.unwrap_or(info.to_target);
                    if new_from == new_to {
                        return Err(EngineError::InvalidAmount(
                            "from_wallet_id and to_wallet_id must differ".to_string(),
                        ));
                    }
                    self.require_wallet_in_vault(&db_tx, vault_id, new_from)
                        .await?;
                    self.require_wallet_in_vault(&db_tx, vault_id, new_to)
                        .await?;

                    apply_transfer_leg_updates(
                        &leg_pairs,
                        "transfer_wallet",
                        vault_currency,
                        info.from_leg_id,
                        info.to_leg_id,
                        new_from,
                        new_to,
                        new_amount_minor,
                        |wallet_id| LegTarget::Wallet { wallet_id },
                        &mut balance_updates,
                        &mut leg_updates,
                    )?;
                }
                TransactionKind::TransferFlow => {
                    let info = parse_transfer_leg_pairs(
                        &leg_pairs,
                        "transfer_flow",
                        "flow",
                        |target| match target {
                            LegTarget::Flow { flow_id } => Some(*flow_id),
                            _ => None,
                        },
                    )?;

                    let new_from = from_flow_id.unwrap_or(info.from_target);
                    let new_to = to_flow_id.unwrap_or(info.to_target);
                    if new_from == new_to {
                        return Err(EngineError::InvalidAmount(
                            "from_flow_id and to_flow_id must differ".to_string(),
                        ));
                    }
                    self.require_flow_in_vault(&db_tx, vault_id, new_from)
                        .await?;
                    self.require_flow_in_vault(&db_tx, vault_id, new_to).await?;

                    apply_transfer_leg_updates(
                        &leg_pairs,
                        "transfer_flow",
                        vault_currency,
                        info.from_leg_id,
                        info.to_leg_id,
                        new_from,
                        new_to,
                        new_amount_minor,
                        |flow_id| LegTarget::Flow { flow_id },
                        &mut balance_updates,
                        &mut leg_updates,
                    )?;
                }
            }

            // Reject unexpected target fields for this kind (avoid silent no-ops).
            validate_update_fields(
                kind,
                wallet_id,
                flow_id,
                from_wallet_id,
                to_wallet_id,
                from_flow_id,
                to_flow_id,
            )?;

            let (wallet_new_balances, flow_previews) = self
                .preview_apply_leg_updates(&db_tx, vault_id, vault_currency, &balance_updates)
                .await?;

            let tx_active = transactions::ActiveModel {
                id: ActiveValue::Set(transaction_id.to_string()),
                amount_minor: ActiveValue::Set(new_amount_minor),
                category: ActiveValue::Set(new_category),
                note: ActiveValue::Set(new_note),
                occurred_at: ActiveValue::Set(new_occurred_at),
                ..Default::default()
            };
            tx_active.update(&db_tx).await?;

            for (leg_id, new_target, new_amount_minor) in leg_updates {
                let (target_kind, target_id) = match new_target {
                    LegTarget::Wallet { wallet_id } => ("wallet".to_string(), wallet_id.to_string()),
                    LegTarget::Flow { flow_id } => ("flow".to_string(), flow_id.to_string()),
                };
                let leg_active = legs::ActiveModel {
                    id: ActiveValue::Set(leg_id),
                    target_kind: ActiveValue::Set(target_kind),
                    target_id: ActiveValue::Set(target_id),
                    amount_minor: ActiveValue::Set(new_amount_minor),
                    ..Default::default()
                };
                leg_active.update(&db_tx).await?;
            }

            self.persist_targets(&db_tx, wallet_new_balances, flow_previews)
                .await?;

            Ok(())
        })
    }

    /// Lists recent transactions that affect a given flow.
    ///
    /// Returns `(transaction, signed_amount_minor)` where `signed_amount_minor`
    /// is the leg amount for that flow.
    pub async fn list_transactions_for_flow(
        &self,
        vault_id: &str,
        flow_id: Uuid,
        user_id: &str,
        limit: u64,
        filter: &TransactionListFilter,
    ) -> ResultEngine<Vec<(Transaction, i64)>> {
        let (items, _next) = self
            .list_transactions_for_flow_page(vault_id, flow_id, user_id, limit, None, filter)
            .await?;
        Ok(items)
    }

    /// Lists recent transactions that affect a given flow, with cursor-based
    /// pagination.
    ///
    /// Pagination is newest → older by `(occurred_at DESC, transaction_id
    /// DESC)`.
    pub async fn list_transactions_for_flow_page(
        &self,
        vault_id: &str,
        flow_id: Uuid,
        user_id: &str,
        limit: u64,
        cursor: Option<&str>,
        filter: &TransactionListFilter,
    ) -> ResultEngine<(Vec<(Transaction, i64)>, Option<String>)> {
        with_tx!(self, |db_tx| {
            self.require_flow_read(&db_tx, vault_id, flow_id, user_id)
                .await?;
            validate_list_filter(filter)?;

            let limit_plus_one = limit.saturating_add(1);
            let mut query = legs::Entity::find()
                .filter(legs::Column::TargetKind.eq(crate::legs::LegTargetKind::Flow.as_str()))
                .filter(legs::Column::TargetId.eq(flow_id.to_string()))
                .find_also_related(transactions::Entity)
                .filter(transactions::Column::VaultId.eq(vault_id.to_string()))
                .order_by_desc(transactions::Column::OccurredAt)
                .order_by_desc(transactions::Column::Id)
                .limit(limit_plus_one);

            if let Some(cursor) = cursor {
                let cursor = TransactionsCursor::decode(cursor)?;
                query = query.filter(
                    Condition::any()
                        .add(transactions::Column::OccurredAt.lt(cursor.occurred_at))
                        .add(
                            Condition::all()
                                .add(transactions::Column::OccurredAt.eq(cursor.occurred_at))
                                .add(transactions::Column::Id.lt(cursor.transaction_id)),
                        ),
                );
            }
            query = query.apply_tx_filters(filter);

            let rows: Vec<(legs::Model, Option<transactions::Model>)> = query.all(&db_tx).await?;
            let has_more = rows.len() > limit as usize;

            let mut out: Vec<(Transaction, i64)> =
                Vec::with_capacity(rows.len().min(limit as usize));
            for (leg_model, tx_model) in rows.into_iter().take(limit as usize) {
                let Some(tx_model) = tx_model else {
                    continue;
                };
                let tx = Transaction::try_from(tx_model)?;
                out.push((tx, leg_model.amount_minor));
            }

            let next_cursor = out.last().map(|(tx, _)| TransactionsCursor {
                occurred_at: tx.occurred_at,
                transaction_id: tx.id.to_string(),
            });
            let next_cursor = if has_more {
                next_cursor.map(|c| c.encode()).transpose()?
            } else {
                None
            };

            Ok((out, next_cursor))
        })
    }

    /// Lists recent transactions affecting the whole vault, with cursor-based
    /// pagination.
    ///
    /// Pagination is newest → older by `(occurred_at DESC, transaction_id
    /// DESC)`.
    pub async fn list_transactions_for_vault_page(
        &self,
        vault_id: &str,
        user_id: &str,
        limit: u64,
        cursor: Option<&str>,
        filter: &TransactionListFilter,
    ) -> ResultEngine<(Vec<Transaction>, Option<String>)> {
        with_tx!(self, |db_tx| {
            self.require_vault_by_id(&db_tx, vault_id, user_id).await?;
            validate_list_filter(filter)?;

            let limit_plus_one = limit.saturating_add(1);
            let mut query = transactions::Entity::find()
                .filter(transactions::Column::VaultId.eq(vault_id.to_string()))
                .order_by_desc(transactions::Column::OccurredAt)
                .order_by_desc(transactions::Column::Id)
                .limit(limit_plus_one);

            if let Some(cursor) = cursor {
                let cursor = TransactionsCursor::decode(cursor)?;
                query = query.filter(
                    Condition::any()
                        .add(transactions::Column::OccurredAt.lt(cursor.occurred_at))
                        .add(
                            Condition::all()
                                .add(transactions::Column::OccurredAt.eq(cursor.occurred_at))
                                .add(transactions::Column::Id.lt(cursor.transaction_id)),
                        ),
                );
            }
            query = query.apply_tx_filters(filter);

            let rows: Vec<transactions::Model> = query.all(&db_tx).await?;
            let has_more = rows.len() > limit as usize;

            let mut out: Vec<Transaction> = Vec::with_capacity(rows.len().min(limit as usize));
            for tx_model in rows.into_iter().take(limit as usize) {
                out.push(Transaction::try_from(tx_model)?);
            }

            let next_cursor = out.last().map(|tx| TransactionsCursor {
                occurred_at: tx.occurred_at,
                transaction_id: tx.id.to_string(),
            });
            let next_cursor = if has_more {
                next_cursor.map(|c| c.encode()).transpose()?
            } else {
                None
            };

            Ok((out, next_cursor))
        })
    }

    /// Lists recent transactions that affect a given wallet.
    ///
    /// Returns `(transaction, signed_amount_minor)` where `signed_amount_minor`
    /// is the leg amount for that wallet.
    pub async fn list_transactions_for_wallet(
        &self,
        vault_id: &str,
        wallet_id: Uuid,
        user_id: &str,
        limit: u64,
        filter: &TransactionListFilter,
    ) -> ResultEngine<Vec<(Transaction, i64)>> {
        let (items, _next) = self
            .list_transactions_for_wallet_page(vault_id, wallet_id, user_id, limit, None, filter)
            .await?;
        Ok(items)
    }

    /// Lists recent transactions that affect a given wallet, with cursor-based
    /// pagination.
    ///
    /// Pagination is newest → older by `(occurred_at DESC, transaction_id
    /// DESC)`.
    pub async fn list_transactions_for_wallet_page(
        &self,
        vault_id: &str,
        wallet_id: Uuid,
        user_id: &str,
        limit: u64,
        cursor: Option<&str>,
        filter: &TransactionListFilter,
    ) -> ResultEngine<(Vec<(Transaction, i64)>, Option<String>)> {
        with_tx!(self, |db_tx| {
            self.require_vault_by_id(&db_tx, vault_id, user_id).await?;
            validate_list_filter(filter)?;

            let limit_plus_one = limit.saturating_add(1);
            let mut query = legs::Entity::find()
                .filter(legs::Column::TargetKind.eq(crate::legs::LegTargetKind::Wallet.as_str()))
                .filter(legs::Column::TargetId.eq(wallet_id.to_string()))
                .find_also_related(transactions::Entity)
                .filter(transactions::Column::VaultId.eq(vault_id.to_string()))
                .order_by_desc(transactions::Column::OccurredAt)
                .order_by_desc(transactions::Column::Id)
                .limit(limit_plus_one);

            if let Some(cursor) = cursor {
                let cursor = TransactionsCursor::decode(cursor)?;
                query = query.filter(
                    Condition::any()
                        .add(transactions::Column::OccurredAt.lt(cursor.occurred_at))
                        .add(
                            Condition::all()
                                .add(transactions::Column::OccurredAt.eq(cursor.occurred_at))
                                .add(transactions::Column::Id.lt(cursor.transaction_id)),
                        ),
                );
            }
            query = query.apply_tx_filters(filter);

            let rows: Vec<(legs::Model, Option<transactions::Model>)> = query.all(&db_tx).await?;
            let has_more = rows.len() > limit as usize;

            let mut out: Vec<(Transaction, i64)> =
                Vec::with_capacity(rows.len().min(limit as usize));
            for (leg_model, tx_model) in rows.into_iter().take(limit as usize) {
                let Some(tx_model) = tx_model else {
                    continue;
                };
                let tx = Transaction::try_from(tx_model)?;
                out.push((tx, leg_model.amount_minor));
            }

            let next_cursor = out.last().map(|(tx, _)| TransactionsCursor {
                occurred_at: tx.occurred_at,
                transaction_id: tx.id.to_string(),
            });
            let next_cursor = if has_more {
                next_cursor.map(|c| c.encode()).transpose()?
            } else {
                None
            };

            Ok((out, next_cursor))
        })
    }

    /// Returns a single transaction with all its legs (detail view).
    ///
    /// Authorization: requires vault read access.
    pub async fn transaction_with_legs(
        &self,
        vault_id: &str,
        transaction_id: Uuid,
        user_id: &str,
    ) -> ResultEngine<Transaction> {
        with_tx!(self, |db_tx| {
            let vault_model = vault::Entity::find_by_id(vault_id.to_string())
                .one(&db_tx)
                .await?
                .ok_or_else(|| EngineError::KeyNotFound("vault not exists".to_string()))?;
            if vault_model.user_id != user_id {
                let member = vault_memberships::Entity::find_by_id((
                    vault_id.to_string(),
                    user_id.to_string(),
                ))
                .one(&db_tx)
                .await?;
                if member.is_none() {
                    return Err(EngineError::Forbidden("forbidden".to_string()));
                }
            }

            let tx_model = transactions::Entity::find_by_id(transaction_id.to_string())
                .one(&db_tx)
                .await?
                .ok_or_else(|| EngineError::KeyNotFound("transaction not exists".to_string()))?;
            if tx_model.vault_id != vault_id {
                return Err(EngineError::KeyNotFound(
                    "transaction not exists".to_string(),
                ));
            }

            let mut tx = Transaction::try_from(tx_model)?;

            let leg_models: Vec<legs::Model> = legs::Entity::find()
                .filter(legs::Column::TransactionId.eq(transaction_id.to_string()))
                .order_by_asc(legs::Column::Id)
                .all(&db_tx)
                .await?;
            let mut out = Vec::with_capacity(leg_models.len());
            for leg_model in leg_models {
                out.push(Leg::try_from(leg_model)?);
            }
            tx.legs = out;

            Ok(tx)
        })
    }
}
