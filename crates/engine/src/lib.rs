use base64::Engine as _;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, future::Future};
use uuid::Uuid;

pub use cash_flows::CashFlow;
pub use commands::{
    ExpenseCmd, IncomeCmd, RefundCmd, TransferFlowCmd, TransferWalletCmd, TxMeta,
    UpdateTransactionCmd,
};
pub use currency::Currency;
pub use error::EngineError;
pub use legs::{Leg, LegTarget};
pub use money::Money;
use sea_orm::{
    ActiveValue, Condition, DatabaseTransaction, JoinType, QueryFilter, QueryOrder, QuerySelect,
    Statement, TransactionTrait, prelude::*, sea_query::Expr,
};
pub use transactions::{Transaction, TransactionKind, TransactionNew};
use util::{ensure_vault_currency, validate_flow_mode_fields};
pub use vault::Vault;
pub use wallets::Wallet;

mod cash_flows;
mod commands;
mod currency;
mod error;
mod flow_memberships;
mod legs;
mod money;
mod transactions;
mod users;
mod util;
mod vault;
mod vault_memberships;
mod wallets;

type ResultEngine<T> = Result<T, EngineError>;

fn normalize_required_name(value: &str, label: &str) -> ResultEngine<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(EngineError::InvalidAmount(format!(
            "{label} name must not be empty"
        )));
    }
    Ok(trimmed.to_string())
}

fn normalize_required_flow_name(value: &str) -> ResultEngine<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(EngineError::InvalidFlow(
            "flow name must not be empty".to_string(),
        ));
    }
    Ok(trimmed.to_string())
}

fn normalize_optional_text(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToString::to_string)
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MembershipRole {
    Owner,
    Editor,
    Viewer,
}

impl MembershipRole {
    fn can_write(self) -> bool {
        matches!(self, Self::Owner | Self::Editor)
    }
}

impl TryFrom<&str> for MembershipRole {
    type Error = EngineError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "owner" => Ok(Self::Owner),
            "editor" => Ok(Self::Editor),
            "viewer" => Ok(Self::Viewer),
            other => Err(EngineError::InvalidAmount(format!(
                "invalid membership role: {other}"
            ))),
        }
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

#[derive(Debug)]
pub struct Engine {
    database: DatabaseConnection,
}

impl Engine {
    /// Return a builder for `Engine`. Help to build the struct.
    pub fn builder() -> EngineBuilder {
        EngineBuilder::default()
    }

    async fn with_tx<T, F, Fut>(&self, f: F) -> ResultEngine<T>
    where
        F: for<'a> FnOnce(&'a DatabaseTransaction) -> Fut,
        Fut: Future<Output = ResultEngine<T>>,
    {
        let db_tx = self.database.begin().await?;
        let result = f(&db_tx).await?;
        db_tx.commit().await?;
        Ok(result)
    }

    async fn vault_membership_role(
        &self,
        db: &DatabaseTransaction,
        vault_id: &str,
        user_id: &str,
    ) -> ResultEngine<Option<MembershipRole>> {
        let row =
            vault_memberships::Entity::find_by_id((vault_id.to_string(), user_id.to_string()))
                .one(db)
                .await?;
        row.as_ref()
            .map(|m| MembershipRole::try_from(m.role.as_str()))
            .transpose()
    }

    async fn require_vault_by_id_write(
        &self,
        db: &DatabaseTransaction,
        vault_id: &str,
        user_id: &str,
    ) -> ResultEngine<vault::Model> {
        let model = self.require_vault_by_id(db, vault_id, user_id).await?;
        if model.user_id == user_id {
            return Ok(model);
        }
        let role = self
            .vault_membership_role(db, vault_id, user_id)
            .await?
            .ok_or_else(|| EngineError::KeyNotFound("vault not exists".to_string()))?;
        if !role.can_write() {
            return Err(EngineError::KeyNotFound("vault not exists".to_string()));
        }
        Ok(model)
    }

    async fn require_vault_owner(
        &self,
        db: &DatabaseTransaction,
        vault_id: &str,
        user_id: &str,
    ) -> ResultEngine<vault::Model> {
        let model = vault::Entity::find_by_id(vault_id.to_string())
            .one(db)
            .await?
            .ok_or_else(|| EngineError::KeyNotFound("vault not exists".to_string()))?;
        if model.user_id != user_id {
            return Err(EngineError::KeyNotFound("vault not exists".to_string()));
        }
        Ok(model)
    }

    async fn require_user_exists(
        &self,
        db: &DatabaseTransaction,
        username: &str,
    ) -> ResultEngine<()> {
        let exists = users::Entity::find_by_id(username.to_string())
            .one(db)
            .await?
            .is_some();
        if !exists {
            return Err(EngineError::KeyNotFound("user not exists".to_string()));
        }
        Ok(())
    }

    async fn flow_membership_role(
        &self,
        db: &DatabaseTransaction,
        flow_id: &str,
        user_id: &str,
    ) -> ResultEngine<Option<MembershipRole>> {
        let row = flow_memberships::Entity::find_by_id((flow_id.to_string(), user_id.to_string()))
            .one(db)
            .await?;
        row.as_ref()
            .map(|m| MembershipRole::try_from(m.role.as_str()))
            .transpose()
    }

    async fn has_vault_read_access(
        &self,
        db: &DatabaseTransaction,
        vault_id: &str,
        user_id: &str,
    ) -> ResultEngine<bool> {
        let Some(vault) = vault::Entity::find_by_id(vault_id.to_string())
            .one(db)
            .await?
        else {
            return Ok(false);
        };
        if vault.user_id == user_id {
            return Ok(true);
        }
        Ok(self
            .vault_membership_role(db, vault_id, user_id)
            .await?
            .is_some())
    }

    async fn has_vault_write_access(
        &self,
        db: &DatabaseTransaction,
        vault_id: &str,
        user_id: &str,
    ) -> ResultEngine<bool> {
        let Some(vault) = vault::Entity::find_by_id(vault_id.to_string())
            .one(db)
            .await?
        else {
            return Ok(false);
        };
        if vault.user_id == user_id {
            return Ok(true);
        }
        let role = self.vault_membership_role(db, vault_id, user_id).await?;
        Ok(role.is_some_and(|r| r.can_write()))
    }

    async fn require_flow_read(
        &self,
        db: &DatabaseTransaction,
        vault_id: &str,
        flow_id: Uuid,
        user_id: &str,
    ) -> ResultEngine<cash_flows::Model> {
        let Some(model) = cash_flows::Entity::find_by_id(flow_id.to_string())
            .filter(cash_flows::Column::VaultId.eq(vault_id.to_string()))
            .one(db)
            .await?
        else {
            return Err(EngineError::KeyNotFound("cash_flow not exists".to_string()));
        };

        if self.has_vault_read_access(db, vault_id, user_id).await? {
            return Ok(model);
        }
        let role = self
            .flow_membership_role(db, &model.id, user_id)
            .await?
            .ok_or_else(|| EngineError::KeyNotFound("cash_flow not exists".to_string()))?;
        let _ = role;
        Ok(model)
    }

    async fn require_flow_write(
        &self,
        db: &DatabaseTransaction,
        vault_id: &str,
        flow_id: Uuid,
        user_id: &str,
    ) -> ResultEngine<cash_flows::Model> {
        let model = self
            .require_flow_read(db, vault_id, flow_id, user_id)
            .await?;
        if self.has_vault_write_access(db, vault_id, user_id).await? {
            return Ok(model);
        }
        let role = self
            .flow_membership_role(db, &model.id, user_id)
            .await?
            .ok_or_else(|| EngineError::KeyNotFound("cash_flow not exists".to_string()))?;
        if !role.can_write() {
            return Err(EngineError::KeyNotFound("cash_flow not exists".to_string()));
        }
        Ok(model)
    }

    async fn require_vault_by_id(
        &self,
        db: &DatabaseTransaction,
        vault_id: &str,
        user_id: &str,
    ) -> ResultEngine<vault::Model> {
        let model = vault::Entity::find_by_id(vault_id.to_string())
            .one(db)
            .await?
            .ok_or_else(|| EngineError::KeyNotFound("vault not exists".to_string()))?;
        if model.user_id != user_id
            && self
                .vault_membership_role(db, vault_id, user_id)
                .await?
                .is_none()
        {
            return Err(EngineError::KeyNotFound("vault not exists".to_string()));
        }
        Ok(model)
    }

    async fn require_vault_by_name(
        &self,
        db: &DatabaseTransaction,
        vault_name: &str,
        user_id: &str,
    ) -> ResultEngine<vault::Model> {
        let vault_name = normalize_required_name(vault_name, "vault")?;
        let vault_name_lower = vault_name.to_lowercase();
        let models: Vec<vault::Model> = vault::Entity::find()
            .filter(Expr::cust("LOWER(name)").eq(vault_name_lower))
            .all(db)
            .await?;

        let mut out: Option<vault::Model> = None;
        for model in models {
            let allowed = if model.user_id == user_id {
                true
            } else {
                self.vault_membership_role(db, &model.id, user_id)
                    .await?
                    .is_some()
            };
            if allowed {
                if out.is_some() {
                    return Err(EngineError::InvalidAmount(
                        "ambiguous vault name".to_string(),
                    ));
                }
                out = Some(model);
            }
        }

        out.ok_or_else(|| EngineError::KeyNotFound("vault not exists".to_string()))
    }

    async fn unallocated_flow_id(
        &self,
        db: &DatabaseTransaction,
        vault_id: &str,
    ) -> ResultEngine<Uuid> {
        let model = cash_flows::Entity::find()
            .filter(cash_flows::Column::VaultId.eq(vault_id.to_string()))
            .filter(cash_flows::Column::SystemKind.eq(Some(
                cash_flows::SystemFlowKind::Unallocated.as_str().to_string(),
            )))
            .one(db)
            .await?
            .ok_or_else(|| EngineError::InvalidFlow("missing Unallocated flow".to_string()))?;
        Uuid::parse_str(&model.id)
            .map_err(|_| EngineError::InvalidAmount("invalid cash_flow id".to_string()))
    }

    async fn resolve_flow_id(
        &self,
        db: &DatabaseTransaction,
        vault_id: &str,
        flow_id: Option<Uuid>,
    ) -> ResultEngine<Uuid> {
        if let Some(id) = flow_id {
            // Ensure it exists and belongs to the vault.
            let exists = cash_flows::Entity::find_by_id(id.to_string())
                .filter(cash_flows::Column::VaultId.eq(vault_id.to_string()))
                .one(db)
                .await?
                .is_some();
            if !exists {
                return Err(EngineError::KeyNotFound("cash_flow not exists".to_string()));
            }
            return Ok(id);
        }
        self.unallocated_flow_id(db, vault_id).await
    }

    async fn resolve_wallet_id(
        &self,
        db: &DatabaseTransaction,
        vault_id: &str,
        wallet_id: Option<Uuid>,
    ) -> ResultEngine<Uuid> {
        if let Some(id) = wallet_id {
            let exists = wallets::Entity::find_by_id(id.to_string())
                .filter(wallets::Column::VaultId.eq(vault_id.to_string()))
                .one(db)
                .await?
                .is_some();
            if !exists {
                return Err(EngineError::KeyNotFound("wallet not exists".to_string()));
            }
            return Ok(id);
        }

        let wallet_models: Vec<wallets::Model> = wallets::Entity::find()
            .filter(wallets::Column::VaultId.eq(vault_id.to_string()))
            .filter(wallets::Column::Archived.eq(false))
            .all(db)
            .await?;

        let mut iter = wallet_models.into_iter();
        let first = iter
            .next()
            .ok_or_else(|| EngineError::KeyNotFound("missing wallet".to_string()))?;
        if iter.next().is_some() {
            return Err(EngineError::InvalidAmount(
                "wallet_id is required when more than one wallet exists".to_string(),
            ));
        }
        Uuid::parse_str(&first.id)
            .map_err(|_| EngineError::InvalidAmount("invalid wallet id".to_string()))
    }

    async fn require_wallet_in_vault(
        &self,
        db: &DatabaseTransaction,
        vault_id: &str,
        wallet_id: Uuid,
    ) -> ResultEngine<()> {
        let exists = wallets::Entity::find_by_id(wallet_id.to_string())
            .filter(wallets::Column::VaultId.eq(vault_id.to_string()))
            .one(db)
            .await?
            .is_some();
        if !exists {
            return Err(EngineError::KeyNotFound("wallet not exists".to_string()));
        }
        Ok(())
    }

    async fn require_flow_in_vault(
        &self,
        db: &DatabaseTransaction,
        vault_id: &str,
        flow_id: Uuid,
    ) -> ResultEngine<()> {
        let exists = cash_flows::Entity::find_by_id(flow_id.to_string())
            .filter(cash_flows::Column::VaultId.eq(vault_id.to_string()))
            .one(db)
            .await?
            .is_some();
        if !exists {
            return Err(EngineError::KeyNotFound("cash_flow not exists".to_string()));
        }
        Ok(())
    }

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
        leg_amount_minor: i64,
    ) -> ResultEngine<Uuid> {
        let category = normalize_optional_text(meta.category.as_deref());
        let note = normalize_optional_text(meta.note.as_deref());
        let vault_model = self
            .require_vault_by_id_write(db_tx, vault_id, user_id)
            .await?;
        let currency = Currency::try_from(vault_model.currency.as_str()).unwrap_or_default();
        let resolved_flow_id = self.resolve_flow_id(db_tx, vault_id, flow_id).await?;
        let resolved_wallet_id = self.resolve_wallet_id(db_tx, vault_id, wallet_id).await?;

        let tx = Transaction::new(TransactionNew {
            vault_id: vault_id.to_string(),
            kind,
            occurred_at: meta.occurred_at,
            amount_minor,
            currency,
            category,
            note,
            created_by: user_id.to_string(),
            idempotency_key: meta.idempotency_key,
            refunded_transaction_id: None,
        })?;
        let legs = vec![
            Leg::new(
                tx.id,
                LegTarget::Wallet {
                    wallet_id: resolved_wallet_id,
                },
                leg_amount_minor,
                currency,
            ),
            Leg::new(
                tx.id,
                LegTarget::Flow {
                    flow_id: resolved_flow_id,
                },
                leg_amount_minor,
                currency,
            ),
        ];

        self.create_transaction_with_legs(db_tx, vault_id, currency, &tx, &legs)
            .await
    }

    async fn create_transaction_with_legs(
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
                if legs.len() != 2 {
                    return Err(EngineError::InvalidAmount(
                        "invalid transaction: expected 2 legs".to_string(),
                    ));
                }
                let expected = match tx.kind {
                    TransactionKind::Income | TransactionKind::Refund => tx.amount_minor,
                    TransactionKind::Expense => -tx.amount_minor,
                    _ => unreachable!(),
                };
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
            }
            TransactionKind::TransferWallet => {
                if legs.len() != 2 {
                    return Err(EngineError::InvalidAmount(
                        "invalid transfer: expected 2 legs".to_string(),
                    ));
                }
                let mut wallet_ids: Vec<Uuid> = Vec::new();
                let mut has_neg = false;
                let mut has_pos = false;
                for leg in legs {
                    let LegTarget::Wallet { wallet_id } = leg.target else {
                        return Err(EngineError::InvalidAmount(
                            "invalid transfer_wallet: expected wallet legs".to_string(),
                        ));
                    };
                    wallet_ids.push(wallet_id);
                    if leg.amount_minor == -tx.amount_minor {
                        has_neg = true;
                    } else if leg.amount_minor == tx.amount_minor {
                        has_pos = true;
                    } else {
                        return Err(EngineError::InvalidAmount(
                            "invalid transfer_wallet: unexpected leg amount".to_string(),
                        ));
                    }
                }
                if !has_neg || !has_pos {
                    return Err(EngineError::InvalidAmount(
                        "invalid transfer_wallet: missing positive/negative leg".to_string(),
                    ));
                }
                if wallet_ids.len() == 2 && wallet_ids[0] == wallet_ids[1] {
                    return Err(EngineError::InvalidAmount(
                        "invalid transfer_wallet: from/to must differ".to_string(),
                    ));
                }
            }
            TransactionKind::TransferFlow => {
                if legs.len() != 2 {
                    return Err(EngineError::InvalidAmount(
                        "invalid transfer: expected 2 legs".to_string(),
                    ));
                }
                let mut flow_ids: Vec<Uuid> = Vec::new();
                let mut has_neg = false;
                let mut has_pos = false;
                for leg in legs {
                    let LegTarget::Flow { flow_id } = leg.target else {
                        return Err(EngineError::InvalidAmount(
                            "invalid transfer_flow: expected flow legs".to_string(),
                        ));
                    };
                    flow_ids.push(flow_id);
                    if leg.amount_minor == -tx.amount_minor {
                        has_neg = true;
                    } else if leg.amount_minor == tx.amount_minor {
                        has_pos = true;
                    } else {
                        return Err(EngineError::InvalidAmount(
                            "invalid transfer_flow: unexpected leg amount".to_string(),
                        ));
                    }
                }
                if !has_neg || !has_pos {
                    return Err(EngineError::InvalidAmount(
                        "invalid transfer_flow: missing positive/negative leg".to_string(),
                    ));
                }
                if flow_ids.len() == 2 && flow_ids[0] == flow_ids[1] {
                    return Err(EngineError::InvalidAmount(
                        "invalid transfer_flow: from/to must differ".to_string(),
                    ));
                }
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
        let mut flow_previews: HashMap<Uuid, CashFlow> = HashMap::new();

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
                    let wallet_model = wallets::Entity::find_by_id(wallet_id.to_string())
                        .filter(wallets::Column::VaultId.eq(vault_id.to_string()))
                        .one(db_tx)
                        .await?
                        .ok_or_else(|| EngineError::KeyNotFound("wallet not exists".to_string()))?;
                    let wallet_currency = Currency::try_from(wallet_model.currency.as_str())
                        .unwrap_or(vault_currency);
                    if wallet_currency != vault_currency {
                        return Err(EngineError::CurrencyMismatch(format!(
                            "wallet currency is {}, got {}",
                            wallet_currency.code(),
                            vault_currency.code()
                        )));
                    }
                    let entry = wallet_new_balances
                        .entry(wallet_id)
                        .or_insert(wallet_model.balance);
                    *entry += leg.amount_minor;
                }
                LegTarget::Flow { flow_id } => {
                    let flow_model = cash_flows::Entity::find_by_id(flow_id.to_string())
                        .filter(cash_flows::Column::VaultId.eq(vault_id.to_string()))
                        .one(db_tx)
                        .await?
                        .ok_or_else(|| {
                            EngineError::KeyNotFound("cash_flow not exists".to_string())
                        })?;
                    validate_flow_mode_fields(
                        &flow_model.name,
                        flow_model.max_balance,
                        flow_model.income_balance,
                    )?;
                    let flow_currency =
                        Currency::try_from(flow_model.currency.as_str()).unwrap_or(vault_currency);
                    if flow_currency != vault_currency {
                        return Err(EngineError::CurrencyMismatch(format!(
                            "flow currency is {}, got {}",
                            flow_currency.code(),
                            vault_currency.code()
                        )));
                    }

                    let id = Uuid::parse_str(&flow_model.id).map_err(|_| {
                        EngineError::InvalidAmount("invalid cash_flow id".to_string())
                    })?;
                    let system_kind = flow_model
                        .system_kind
                        .as_deref()
                        .and_then(|k| cash_flows::SystemFlowKind::try_from(k).ok());
                    let entry = flow_previews.entry(id).or_insert_with(|| CashFlow {
                        id,
                        name: flow_model.name.clone(),
                        system_kind,
                        balance: flow_model.balance,
                        max_balance: flow_model.max_balance,
                        income_balance: flow_model.income_balance,
                        currency: flow_currency,
                        archived: flow_model.archived,
                    });
                    entry.apply_leg_change(0, leg.amount_minor)?;
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

        Ok(tx.id)
    }

    async fn preview_apply_leg_updates(
        &self,
        db_tx: &DatabaseTransaction,
        vault_id: &str,
        vault_currency: Currency,
        updates: &[(LegTarget, i64, i64)],
    ) -> ResultEngine<(HashMap<Uuid, i64>, HashMap<Uuid, CashFlow>)> {
        let mut wallet_new_balances: HashMap<Uuid, i64> = HashMap::new();
        let mut flow_previews: HashMap<Uuid, CashFlow> = HashMap::new();

        for (target, old_amount_minor, new_amount_minor) in updates {
            match *target {
                LegTarget::Wallet { wallet_id } => {
                    let wallet_model = wallets::Entity::find_by_id(wallet_id.to_string())
                        .filter(wallets::Column::VaultId.eq(vault_id.to_string()))
                        .one(db_tx)
                        .await?
                        .ok_or_else(|| EngineError::KeyNotFound("wallet not exists".to_string()))?;
                    let wallet_currency = Currency::try_from(wallet_model.currency.as_str())
                        .unwrap_or(vault_currency);
                    if wallet_currency != vault_currency {
                        return Err(EngineError::CurrencyMismatch(format!(
                            "vault currency is {}, got {}",
                            vault_currency.code(),
                            wallet_currency.code()
                        )));
                    }
                    let entry = wallet_new_balances
                        .entry(wallet_id)
                        .or_insert(wallet_model.balance);
                    *entry = *entry - *old_amount_minor + *new_amount_minor;
                }
                LegTarget::Flow { flow_id } => {
                    let flow_model = cash_flows::Entity::find_by_id(flow_id.to_string())
                        .filter(cash_flows::Column::VaultId.eq(vault_id.to_string()))
                        .one(db_tx)
                        .await?
                        .ok_or_else(|| {
                            EngineError::KeyNotFound("cash_flow not exists".to_string())
                        })?;
                    validate_flow_mode_fields(
                        &flow_model.name,
                        flow_model.max_balance,
                        flow_model.income_balance,
                    )?;
                    let flow_currency =
                        Currency::try_from(flow_model.currency.as_str()).unwrap_or(vault_currency);
                    if flow_currency != vault_currency {
                        return Err(EngineError::CurrencyMismatch(format!(
                            "vault currency is {}, got {}",
                            vault_currency.code(),
                            flow_currency.code()
                        )));
                    }
                    let system_kind = flow_model
                        .system_kind
                        .as_deref()
                        .and_then(|k| cash_flows::SystemFlowKind::try_from(k).ok());
                    let entry = flow_previews.entry(flow_id).or_insert_with(|| CashFlow {
                        id: flow_id,
                        name: flow_model.name.clone(),
                        system_kind,
                        balance: flow_model.balance,
                        max_balance: flow_model.max_balance,
                        income_balance: flow_model.income_balance,
                        currency: flow_currency,
                        archived: flow_model.archived,
                    });
                    entry.apply_leg_change(*old_amount_minor, *new_amount_minor)?;
                }
            }
        }

        Ok((wallet_new_balances, flow_previews))
    }

    async fn persist_targets(
        &self,
        db_tx: &DatabaseTransaction,
        wallet_new_balances: HashMap<Uuid, i64>,
        flow_previews: HashMap<Uuid, CashFlow>,
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
        self.with_tx(|db_tx| async move {
            self.create_flow_wallet_transaction(
                db_tx,
                &vault_id,
                &user_id,
                flow_id,
                wallet_id,
                amount_minor,
                TransactionKind::Income,
                meta,
                amount_minor,
            )
            .await
        })
        .await
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
        self.with_tx(|db_tx| async move {
            self.create_flow_wallet_transaction(
                db_tx,
                &vault_id,
                &user_id,
                flow_id,
                wallet_id,
                amount_minor,
                TransactionKind::Expense,
                meta,
                -amount_minor,
            )
            .await
        })
        .await
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
        self.with_tx(|db_tx| async move {
            self.create_flow_wallet_transaction(
                db_tx,
                &vault_id,
                &user_id,
                flow_id,
                wallet_id,
                amount_minor,
                TransactionKind::Refund,
                meta,
                amount_minor,
            )
            .await
        })
        .await
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
        self.with_tx(|db_tx| async move {
            let vault_model = self
                .require_vault_by_id_write(db_tx, &vault_id, &user_id)
                .await?;
            let currency = Currency::try_from(vault_model.currency.as_str()).unwrap_or_default();
            // Ensure wallets belong to the vault.
            self.resolve_wallet_id(db_tx, &vault_id, Some(from_wallet_id))
                .await?;
            self.resolve_wallet_id(db_tx, &vault_id, Some(to_wallet_id))
                .await?;

            let tx = Transaction::new(TransactionNew {
                vault_id: vault_id.clone(),
                kind: TransactionKind::TransferWallet,
                occurred_at,
                amount_minor,
                currency,
                category: None,
                note,
                created_by: user_id.clone(),
                idempotency_key,
                refunded_transaction_id: None,
            })?;
            let legs = vec![
                Leg::new(
                    tx.id,
                    LegTarget::Wallet {
                        wallet_id: from_wallet_id,
                    },
                    -amount_minor,
                    currency,
                ),
                Leg::new(
                    tx.id,
                    LegTarget::Wallet {
                        wallet_id: to_wallet_id,
                    },
                    amount_minor,
                    currency,
                ),
            ];

            self.create_transaction_with_legs(db_tx, &vault_id, currency, &tx, &legs)
                .await
        })
        .await
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
        self.with_tx(|db_tx| async move {
            let vault_model = vault::Entity::find_by_id(vault_id.to_string())
                .one(db_tx)
                .await?
                .ok_or_else(|| EngineError::KeyNotFound("vault not exists".to_string()))?;
            let currency = Currency::try_from(vault_model.currency.as_str()).unwrap_or_default();
            // AuthZ:
            // - Vault owner/editor can transfer between any flows in the vault.
            // - Otherwise, user must be editor/owner on both flows (via flow_memberships).
            if self
                .has_vault_write_access(db_tx, &vault_id, &user_id)
                .await?
            {
                self.resolve_flow_id(db_tx, &vault_id, Some(from_flow_id))
                    .await?;
                self.resolve_flow_id(db_tx, &vault_id, Some(to_flow_id))
                    .await?;
            } else {
                self.require_flow_write(db_tx, &vault_id, from_flow_id, &user_id)
                    .await?;
                self.require_flow_write(db_tx, &vault_id, to_flow_id, &user_id)
                    .await?;
            }

            let tx = Transaction::new(TransactionNew {
                vault_id: vault_id.clone(),
                kind: TransactionKind::TransferFlow,
                occurred_at,
                amount_minor,
                currency,
                category: None,
                note,
                created_by: user_id.clone(),
                idempotency_key,
                refunded_transaction_id: None,
            })?;
            let legs = vec![
                Leg::new(
                    tx.id,
                    LegTarget::Flow {
                        flow_id: from_flow_id,
                    },
                    -amount_minor,
                    currency,
                ),
                Leg::new(
                    tx.id,
                    LegTarget::Flow {
                        flow_id: to_flow_id,
                    },
                    amount_minor,
                    currency,
                ),
            ];

            self.create_transaction_with_legs(db_tx, &vault_id, currency, &tx, &legs)
                .await
        })
        .await
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
        let db_tx = self.database.begin().await?;
        let vault_model = self
            .require_vault_by_id_write(&db_tx, vault_id, user_id)
            .await?;
        let vault_currency = Currency::try_from(vault_model.currency.as_str()).unwrap_or_default();

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

        db_tx.commit().await?;
        Ok(())
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

        let db_tx = self.database.begin().await?;
        let vault_model = self
            .require_vault_by_id_write(&db_tx, vault_id, user_id)
            .await?;
        let vault_currency = Currency::try_from(vault_model.currency.as_str()).unwrap_or_default();

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
                if leg_pairs.len() != 2 {
                    return Err(EngineError::InvalidAmount(
                        "invalid transaction: expected 2 legs".to_string(),
                    ));
                }

                let mut existing_wallet_id: Option<Uuid> = None;
                let mut existing_flow_id: Option<Uuid> = None;
                for (_, leg) in &leg_pairs {
                    match leg.target {
                        LegTarget::Wallet { wallet_id } => existing_wallet_id = Some(wallet_id),
                        LegTarget::Flow { flow_id } => existing_flow_id = Some(flow_id),
                    }
                }
                let existing_wallet_id = existing_wallet_id.ok_or_else(|| {
                    EngineError::InvalidAmount(
                        "invalid transaction: missing wallet leg".to_string(),
                    )
                })?;
                let existing_flow_id = existing_flow_id.ok_or_else(|| {
                    EngineError::InvalidAmount("invalid transaction: missing flow leg".to_string())
                })?;

                let new_wallet_id = wallet_id.unwrap_or(existing_wallet_id);
                let new_flow_id = flow_id.unwrap_or(existing_flow_id);
                self.require_wallet_in_vault(&db_tx, vault_id, new_wallet_id)
                    .await?;
                self.require_flow_in_vault(&db_tx, vault_id, new_flow_id)
                    .await?;

                let sign = match kind {
                    TransactionKind::Income | TransactionKind::Refund => 1,
                    TransactionKind::Expense => -1,
                    _ => unreachable!(),
                };
                let new_signed_amount = sign * new_amount_minor;

                for (model, leg) in &leg_pairs {
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
                            LegTarget::Flow {
                                flow_id: new_flow_id,
                            },
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
            }
            TransactionKind::TransferWallet => {
                if leg_pairs.len() != 2 {
                    return Err(EngineError::InvalidAmount(
                        "invalid transfer_wallet: expected 2 legs".to_string(),
                    ));
                }

                let mut from_wallet: Option<Uuid> = None;
                let mut to_wallet: Option<Uuid> = None;
                let mut from_leg_id: Option<Uuid> = None;
                let mut to_leg_id: Option<Uuid> = None;

                for (model, leg) in &leg_pairs {
                    let LegTarget::Wallet { wallet_id } = leg.target else {
                        return Err(EngineError::InvalidAmount(
                            "invalid transfer_wallet: expected wallet legs".to_string(),
                        ));
                    };
                    if leg.amount_minor < 0 {
                        from_wallet = Some(wallet_id);
                        from_leg_id = Some(Uuid::parse_str(&model.id).map_err(|_| {
                            EngineError::InvalidAmount("invalid leg id".to_string())
                        })?);
                    } else if leg.amount_minor > 0 {
                        to_wallet = Some(wallet_id);
                        to_leg_id = Some(Uuid::parse_str(&model.id).map_err(|_| {
                            EngineError::InvalidAmount("invalid leg id".to_string())
                        })?);
                    }
                }

                let from_wallet = from_wallet.ok_or_else(|| {
                    EngineError::InvalidAmount(
                        "invalid transfer_wallet: missing negative leg".to_string(),
                    )
                })?;
                let to_wallet = to_wallet.ok_or_else(|| {
                    EngineError::InvalidAmount(
                        "invalid transfer_wallet: missing positive leg".to_string(),
                    )
                })?;

                let new_from = from_wallet_id.unwrap_or(from_wallet);
                let new_to = to_wallet_id.unwrap_or(to_wallet);
                if new_from == new_to {
                    return Err(EngineError::InvalidAmount(
                        "from_wallet_id and to_wallet_id must differ".to_string(),
                    ));
                }
                self.require_wallet_in_vault(&db_tx, vault_id, new_from)
                    .await?;
                self.require_wallet_in_vault(&db_tx, vault_id, new_to)
                    .await?;

                let from_leg_id = from_leg_id.ok_or_else(|| {
                    EngineError::InvalidAmount(
                        "invalid transfer_wallet: missing leg id".to_string(),
                    )
                })?;
                let to_leg_id = to_leg_id.ok_or_else(|| {
                    EngineError::InvalidAmount(
                        "invalid transfer_wallet: missing leg id".to_string(),
                    )
                })?;

                for (model, leg) in &leg_pairs {
                    if leg.currency != vault_currency {
                        return Err(EngineError::CurrencyMismatch(format!(
                            "vault currency is {}, got {}",
                            vault_currency.code(),
                            leg.currency.code()
                        )));
                    }
                    let id = Uuid::parse_str(&model.id)
                        .map_err(|_| EngineError::InvalidAmount("invalid leg id".to_string()))?;
                    let (new_target, new_amount) = if id == from_leg_id {
                        (
                            LegTarget::Wallet {
                                wallet_id: new_from,
                            },
                            -new_amount_minor,
                        )
                    } else if id == to_leg_id {
                        (LegTarget::Wallet { wallet_id: new_to }, new_amount_minor)
                    } else {
                        return Err(EngineError::InvalidAmount(
                            "invalid transfer_wallet: unexpected legs".to_string(),
                        ));
                    };

                    if leg.target == new_target {
                        balance_updates.push((leg.target, leg.amount_minor, new_amount));
                    } else {
                        balance_updates.push((leg.target, leg.amount_minor, 0));
                        balance_updates.push((new_target, 0, new_amount));
                    }
                    leg_updates.push((model.id.clone(), new_target, new_amount));
                }
            }
            TransactionKind::TransferFlow => {
                if leg_pairs.len() != 2 {
                    return Err(EngineError::InvalidAmount(
                        "invalid transfer_flow: expected 2 legs".to_string(),
                    ));
                }

                let mut from_flow: Option<Uuid> = None;
                let mut to_flow: Option<Uuid> = None;
                let mut from_leg_id: Option<Uuid> = None;
                let mut to_leg_id: Option<Uuid> = None;

                for (model, leg) in &leg_pairs {
                    let LegTarget::Flow { flow_id } = leg.target else {
                        return Err(EngineError::InvalidAmount(
                            "invalid transfer_flow: expected flow legs".to_string(),
                        ));
                    };
                    if leg.amount_minor < 0 {
                        from_flow = Some(flow_id);
                        from_leg_id = Some(Uuid::parse_str(&model.id).map_err(|_| {
                            EngineError::InvalidAmount("invalid leg id".to_string())
                        })?);
                    } else if leg.amount_minor > 0 {
                        to_flow = Some(flow_id);
                        to_leg_id = Some(Uuid::parse_str(&model.id).map_err(|_| {
                            EngineError::InvalidAmount("invalid leg id".to_string())
                        })?);
                    }
                }

                let from_flow = from_flow.ok_or_else(|| {
                    EngineError::InvalidAmount(
                        "invalid transfer_flow: missing negative leg".to_string(),
                    )
                })?;
                let to_flow = to_flow.ok_or_else(|| {
                    EngineError::InvalidAmount(
                        "invalid transfer_flow: missing positive leg".to_string(),
                    )
                })?;

                let new_from = from_flow_id.unwrap_or(from_flow);
                let new_to = to_flow_id.unwrap_or(to_flow);
                if new_from == new_to {
                    return Err(EngineError::InvalidAmount(
                        "from_flow_id and to_flow_id must differ".to_string(),
                    ));
                }
                self.require_flow_in_vault(&db_tx, vault_id, new_from)
                    .await?;
                self.require_flow_in_vault(&db_tx, vault_id, new_to).await?;

                let from_leg_id = from_leg_id.ok_or_else(|| {
                    EngineError::InvalidAmount("invalid transfer_flow: missing leg id".to_string())
                })?;
                let to_leg_id = to_leg_id.ok_or_else(|| {
                    EngineError::InvalidAmount("invalid transfer_flow: missing leg id".to_string())
                })?;

                for (model, leg) in &leg_pairs {
                    if leg.currency != vault_currency {
                        return Err(EngineError::CurrencyMismatch(format!(
                            "vault currency is {}, got {}",
                            vault_currency.code(),
                            leg.currency.code()
                        )));
                    }
                    let id = Uuid::parse_str(&model.id)
                        .map_err(|_| EngineError::InvalidAmount("invalid leg id".to_string()))?;
                    let (new_target, new_amount) = if id == from_leg_id {
                        (LegTarget::Flow { flow_id: new_from }, -new_amount_minor)
                    } else if id == to_leg_id {
                        (LegTarget::Flow { flow_id: new_to }, new_amount_minor)
                    } else {
                        return Err(EngineError::InvalidAmount(
                            "invalid transfer_flow: unexpected legs".to_string(),
                        ));
                    };

                    if leg.target == new_target {
                        balance_updates.push((leg.target, leg.amount_minor, new_amount));
                    } else {
                        balance_updates.push((leg.target, leg.amount_minor, 0));
                        balance_updates.push((new_target, 0, new_amount));
                    }
                    leg_updates.push((model.id.clone(), new_target, new_amount));
                }
            }
        }

        // Reject unexpected target fields for this kind (avoid silent no-ops).
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

        db_tx.commit().await?;
        Ok(())
    }

    /// Return a [`CashFlow`] (snapshot from DB).
    pub async fn cash_flow(
        &self,
        cash_flow_id: Uuid,
        vault_id: &str,
        user_id: &str,
    ) -> ResultEngine<CashFlow> {
        self.with_tx(|db_tx| async move {
            let model = self
                .require_flow_read(db_tx, vault_id, cash_flow_id, user_id)
                .await?;
            let vault_model = vault::Entity::find_by_id(vault_id.to_string())
                .one(db_tx)
                .await?
                .ok_or_else(|| EngineError::KeyNotFound("vault not exists".to_string()))?;
            let vault_currency =
                Currency::try_from(vault_model.currency.as_str()).unwrap_or_default();
            CashFlow::try_from((model, vault_currency))
        })
        .await
    }

    pub async fn cash_flow_by_name(
        &self,
        name: &str,
        vault_id: &str,
        user_id: &str,
    ) -> ResultEngine<CashFlow> {
        let name = normalize_required_flow_name(name)?;
        let name_lower = name.to_lowercase();
        self.with_tx(|db_tx| async move {
            let vault_model = vault::Entity::find_by_id(vault_id.to_string())
                .one(db_tx)
                .await?
                .ok_or_else(|| EngineError::KeyNotFound("vault not exists".to_string()))?;
            let vault_currency =
                Currency::try_from(vault_model.currency.as_str()).unwrap_or_default();

            let model = cash_flows::Entity::find()
                .filter(cash_flows::Column::VaultId.eq(vault_id.to_string()))
                .filter(Expr::cust("LOWER(name)").eq(name_lower))
                .one(db_tx)
                .await?
                .ok_or_else(|| EngineError::KeyNotFound("cash_flow not exists".to_string()))?;

            if !self.has_vault_read_access(db_tx, vault_id, user_id).await? {
                let role = self
                    .flow_membership_role(db_tx, &model.id, user_id)
                    .await?
                    .ok_or_else(|| EngineError::KeyNotFound("cash_flow not exists".to_string()))?;
                let _ = role;
            }

            CashFlow::try_from((model, vault_currency))
        })
        .await
    }

    /// Delete a cash flow contained by a vault.
    pub async fn delete_cash_flow(
        &self,
        vault_id: &str,
        cash_flow_id: Uuid,
        archive: bool,
        user_id: &str,
    ) -> ResultEngine<()> {
        let db_tx = self.database.begin().await?;
        self.require_vault_by_id_write(&db_tx, vault_id, user_id)
            .await?;

        let flow_model = cash_flows::Entity::find_by_id(cash_flow_id.to_string())
            .filter(cash_flows::Column::VaultId.eq(vault_id.to_string()))
            .one(&db_tx)
            .await?
            .ok_or_else(|| EngineError::KeyNotFound("cash_flow not exists".to_string()))?;

        if flow_model
            .system_kind
            .as_deref()
            .is_some_and(|k| k == cash_flows::SystemFlowKind::Unallocated.as_str())
            || flow_model
                .name
                .eq_ignore_ascii_case(cash_flows::UNALLOCATED_INTERNAL_NAME)
        {
            return Err(EngineError::InvalidFlow(if archive {
                "cannot archive Unallocated".to_string()
            } else {
                "cannot delete Unallocated".to_string()
            }));
        }

        if archive {
            let flow_model = cash_flows::ActiveModel {
                id: ActiveValue::Set(cash_flow_id.to_string()),
                archived: ActiveValue::Set(true),
                ..Default::default()
            };
            flow_model.update(&db_tx).await?;
        } else {
            cash_flows::Entity::delete_by_id(cash_flow_id.to_string())
                .exec(&db_tx)
                .await?;
        }
        db_tx.commit().await?;

        Ok(())
    }

    /// Delete or archive a vault
    /// TODO: Add `archive`
    pub async fn delete_vault(&self, vault_id: &str, user_id: &str) -> ResultEngine<()> {
        let db_tx = self.database.begin().await?;
        let vault_model = self
            .require_vault_by_id_write(&db_tx, vault_id, user_id)
            .await?;
        let vault_db_id = vault_model.id;

        // Best-effort cascade delete within one DB transaction.
        // (FKs currently don't declare ON DELETE CASCADE everywhere, and some
        // relationships are not FK-backed, so we do it explicitly.)
        let backend = self.database.get_database_backend();

        // 1) legs for transactions in this vault
        db_tx
            .execute(Statement::from_sql_and_values(
                backend,
                "DELETE FROM legs WHERE transaction_id IN (SELECT id FROM transactions WHERE vault_id = ?);",
                vec![vault_db_id.clone().into()],
            ))
            .await?;

        // 2) transactions
        db_tx
            .execute(Statement::from_sql_and_values(
                backend,
                "DELETE FROM transactions WHERE vault_id = ?;",
                vec![vault_db_id.clone().into()],
            ))
            .await?;

        // 3) flows and wallets
        // (Legacy) entries table (kept for now) references wallets/flows via FK.
        db_tx
            .execute(Statement::from_sql_and_values(
                backend,
                "DELETE FROM entries WHERE vault_id = ?;",
                vec![vault_db_id.clone().into()],
            ))
            .await?;

        db_tx
            .execute(Statement::from_sql_and_values(
                backend,
                "DELETE FROM cash_flows WHERE vault_id = ?;",
                vec![vault_db_id.clone().into()],
            ))
            .await?;
        db_tx
            .execute(Statement::from_sql_and_values(
                backend,
                "DELETE FROM wallets WHERE vault_id = ?;",
                vec![vault_db_id.clone().into()],
            ))
            .await?;

        // 4) vault
        db_tx
            .execute(Statement::from_sql_and_values(
                backend,
                "DELETE FROM vaults WHERE id = ?;",
                vec![vault_db_id.clone().into()],
            ))
            .await?;

        db_tx.commit().await?;

        Ok(())
    }

    /// Add a new vault
    pub async fn new_vault(
        &self,
        name: &str,
        user_id: &str,
        currency: Option<Currency>,
    ) -> ResultEngine<String> {
        let name = normalize_required_name(name, "vault")?;

        let mut new_vault = Vault::new(name.clone(), user_id);
        new_vault.currency = currency.unwrap_or_default();
        let new_vault_id = new_vault.id.clone();
        let vault_entry: vault::ActiveModel = (&new_vault).into();

        let db_tx = self.database.begin().await?;

        // Enforce unique vault names per owner (case-insensitive) to avoid
        // ambiguous name lookups.
        let exists = vault::Entity::find()
            .filter(vault::Column::UserId.eq(user_id.to_string()))
            .filter(Expr::cust("LOWER(name)").eq(name.to_lowercase()))
            .one(&db_tx)
            .await?
            .is_some();
        if exists {
            return Err(EngineError::ExistingKey(name));
        }

        vault_entry.insert(&db_tx).await?;

        // Create the system flow "Unallocated".
        let mut unallocated = CashFlow::new(
            cash_flows::UNALLOCATED_INTERNAL_NAME.to_string(),
            0,
            None,
            None,
            new_vault.currency,
        )?;
        unallocated.system_kind = Some(cash_flows::SystemFlowKind::Unallocated);
        let mut unallocated_model: cash_flows::ActiveModel = (&unallocated).into();
        unallocated_model.vault_id = ActiveValue::Set(new_vault_id.clone());
        unallocated_model.insert(&db_tx).await?;

        // Create a default wallet ("Cash") so clients can start immediately.
        let default_wallet = Wallet::new("Cash".to_string(), 0, new_vault.currency);
        let mut default_wallet_model: wallets::ActiveModel = (&default_wallet).into();
        default_wallet_model.vault_id = ActiveValue::Set(new_vault_id.clone());
        default_wallet_model.insert(&db_tx).await?;

        // Scaffolding for future sharing: create the owner membership row.
        let membership = vault_memberships::ActiveModel {
            vault_id: ActiveValue::Set(new_vault_id.clone()),
            user_id: ActiveValue::Set(user_id.to_string()),
            role: ActiveValue::Set("owner".to_string()),
        };
        membership.insert(&db_tx).await?;

        db_tx.commit().await?;
        Ok(new_vault_id)
    }

    /// Adds or updates a vault member (owner-only).
    pub async fn upsert_vault_member(
        &self,
        vault_id: &str,
        member_username: &str,
        role: &str,
        user_id: &str,
    ) -> ResultEngine<()> {
        let db_tx = self.database.begin().await?;
        self.require_vault_owner(&db_tx, vault_id, user_id).await?;
        self.require_user_exists(&db_tx, member_username).await?;

        let _role = MembershipRole::try_from(role)?;

        let active = vault_memberships::ActiveModel {
            vault_id: ActiveValue::Set(vault_id.to_string()),
            user_id: ActiveValue::Set(member_username.to_string()),
            role: ActiveValue::Set(role.to_string()),
        };

        // Upsert: insert if missing, otherwise update role.
        match vault_memberships::Entity::find_by_id((
            vault_id.to_string(),
            member_username.to_string(),
        ))
        .one(&db_tx)
        .await?
        {
            Some(_) => {
                active.update(&db_tx).await?;
            }
            None => {
                active.insert(&db_tx).await?;
            }
        }

        db_tx.commit().await?;
        Ok(())
    }

    /// Removes a vault member (owner-only).
    pub async fn remove_vault_member(
        &self,
        vault_id: &str,
        member_username: &str,
        user_id: &str,
    ) -> ResultEngine<()> {
        let db_tx = self.database.begin().await?;
        let vault = self.require_vault_owner(&db_tx, vault_id, user_id).await?;
        if member_username == vault.user_id {
            return Err(EngineError::InvalidAmount(
                "cannot remove vault owner".to_string(),
            ));
        }

        vault_memberships::Entity::delete_by_id((
            vault_id.to_string(),
            member_username.to_string(),
        ))
        .exec(&db_tx)
        .await?;

        db_tx.commit().await?;
        Ok(())
    }

    /// Lists vault members (owner-only).
    pub async fn list_vault_members(
        &self,
        vault_id: &str,
        user_id: &str,
    ) -> ResultEngine<Vec<(String, String)>> {
        let db_tx = self.database.begin().await?;
        self.require_vault_owner(&db_tx, vault_id, user_id).await?;

        let rows = vault_memberships::Entity::find()
            .filter(vault_memberships::Column::VaultId.eq(vault_id.to_string()))
            .all(&db_tx)
            .await?;
        db_tx.commit().await?;
        Ok(rows.into_iter().map(|m| (m.user_id, m.role)).collect())
    }

    /// Adds or updates a flow member (owner-only, flow belongs to the vault).
    pub async fn upsert_flow_member(
        &self,
        vault_id: &str,
        flow_id: Uuid,
        member_username: &str,
        role: &str,
        user_id: &str,
    ) -> ResultEngine<()> {
        let db_tx = self.database.begin().await?;
        self.require_vault_owner(&db_tx, vault_id, user_id).await?;
        self.require_user_exists(&db_tx, member_username).await?;
        let _role = MembershipRole::try_from(role)?;

        let flow = cash_flows::Entity::find_by_id(flow_id.to_string())
            .filter(cash_flows::Column::VaultId.eq(vault_id.to_string()))
            .one(&db_tx)
            .await?
            .ok_or_else(|| EngineError::KeyNotFound("cash_flow not exists".to_string()))?;
        if flow
            .system_kind
            .as_deref()
            .is_some_and(|k| k == cash_flows::SystemFlowKind::Unallocated.as_str())
        {
            return Err(EngineError::InvalidFlow(
                "cannot share Unallocated".to_string(),
            ));
        }

        let active = flow_memberships::ActiveModel {
            flow_id: ActiveValue::Set(flow_id.to_string()),
            user_id: ActiveValue::Set(member_username.to_string()),
            role: ActiveValue::Set(role.to_string()),
        };

        match flow_memberships::Entity::find_by_id((
            flow_id.to_string(),
            member_username.to_string(),
        ))
        .one(&db_tx)
        .await?
        {
            Some(_) => {
                active.update(&db_tx).await?;
            }
            None => {
                active.insert(&db_tx).await?;
            }
        }

        db_tx.commit().await?;
        Ok(())
    }

    /// Removes a flow member (owner-only).
    pub async fn remove_flow_member(
        &self,
        vault_id: &str,
        flow_id: Uuid,
        member_username: &str,
        user_id: &str,
    ) -> ResultEngine<()> {
        let db_tx = self.database.begin().await?;
        self.require_vault_owner(&db_tx, vault_id, user_id).await?;

        // Ensure flow exists and belongs to vault.
        self.require_flow_read(&db_tx, vault_id, flow_id, user_id)
            .await?;

        flow_memberships::Entity::delete_by_id((flow_id.to_string(), member_username.to_string()))
            .exec(&db_tx)
            .await?;
        db_tx.commit().await?;
        Ok(())
    }

    /// Lists flow members (owner-only).
    pub async fn list_flow_members(
        &self,
        vault_id: &str,
        flow_id: Uuid,
        user_id: &str,
    ) -> ResultEngine<Vec<(String, String)>> {
        let db_tx = self.database.begin().await?;
        self.require_vault_owner(&db_tx, vault_id, user_id).await?;
        self.require_flow_read(&db_tx, vault_id, flow_id, user_id)
            .await?;

        let rows = flow_memberships::Entity::find()
            .filter(flow_memberships::Column::FlowId.eq(flow_id.to_string()))
            .all(&db_tx)
            .await?;
        db_tx.commit().await?;
        Ok(rows.into_iter().map(|m| (m.user_id, m.role)).collect())
    }

    /// Add a new cash flow inside a vault.
    ///
    /// `balance` represents the initial allocation for the flow and is modeled
    /// as an opening `TransferFlow` from `Unallocated → this flow` (so
    /// transfers do not inflate income/expense stats).
    ///
    /// The opening transfer uses `Utc::now()` as `occurred_at`.
    pub async fn new_cash_flow(
        &self,
        vault_id: &str,
        name: &str,
        balance: i64,
        max_balance: Option<i64>,
        income_bounded: Option<bool>,
        user_id: &str,
    ) -> ResultEngine<Uuid> {
        let occurred_at = Utc::now();
        let name = normalize_required_flow_name(name)?;
        if balance < 0 {
            return Err(EngineError::InvalidAmount(
                "flow balance must be >= 0".to_string(),
            ));
        }

        let db_tx = self.database.begin().await?;
        let vault_model = self
            .require_vault_by_id_write(&db_tx, vault_id, user_id)
            .await?;
        let vault_currency = Currency::try_from(vault_model.currency.as_str()).unwrap_or_default();

        if name.eq_ignore_ascii_case(cash_flows::UNALLOCATED_INTERNAL_NAME) {
            return Err(EngineError::InvalidFlow(
                "flow name is reserved".to_string(),
            ));
        }
        let exists = cash_flows::Entity::find()
            .filter(cash_flows::Column::VaultId.eq(vault_id.to_string()))
            .filter(Expr::cust("LOWER(name)").eq(name.to_lowercase()))
            .one(&db_tx)
            .await?
            .is_some();
        if exists {
            return Err(EngineError::ExistingKey(name.to_string()));
        }

        // Create the flow with a 0 balance. If `balance > 0`, we represent it as an
        // opening allocation transfer from Unallocated → new flow.
        let flow = CashFlow::new(
            name.to_string(),
            0,
            max_balance,
            income_bounded,
            vault_currency,
        )?;
        let flow_id = flow.id;
        let mut flow_model: cash_flows::ActiveModel = (&flow).into();
        flow_model.vault_id = ActiveValue::Set(vault_model.id);
        flow_model.insert(&db_tx).await?;

        if balance > 0 {
            let unallocated_flow_id = self.unallocated_flow_id(&db_tx, vault_id).await?;
            let tx = Transaction::new(TransactionNew {
                vault_id: vault_id.to_string(),
                kind: TransactionKind::TransferFlow,
                occurred_at,
                amount_minor: balance,
                currency: vault_currency,
                category: None,
                note: Some(format!("opening allocation for flow '{name}'")),
                created_by: user_id.to_string(),
                idempotency_key: None,
                refunded_transaction_id: None,
            })?;

            let legs = vec![
                Leg::new(
                    tx.id,
                    LegTarget::Flow {
                        flow_id: unallocated_flow_id,
                    },
                    -balance,
                    vault_currency,
                ),
                Leg::new(tx.id, LegTarget::Flow { flow_id }, balance, vault_currency),
            ];
            self.create_transaction_with_legs(&db_tx, vault_id, vault_currency, &tx, &legs)
                .await?;
        }

        db_tx.commit().await?;

        Ok(flow_id)
    }

    /// Add a new wallet inside a vault.
    ///
    /// `balance_minor` is modeled as an opening transaction against the system
    /// flow `Unallocated`:
    /// - if `balance_minor > 0`: an opening `Income`
    /// - if `balance_minor < 0`: an opening `Expense`
    ///
    /// The opening transaction uses `Utc::now()` as `occurred_at`.
    pub async fn new_wallet(
        &self,
        vault_id: &str,
        name: &str,
        balance_minor: i64,
        user_id: &str,
    ) -> ResultEngine<Uuid> {
        let occurred_at = Utc::now();
        let name = normalize_required_name(name, "wallet")?;
        let db_tx = self.database.begin().await?;
        let vault_model = self
            .require_vault_by_id_write(&db_tx, vault_id, user_id)
            .await?;
        let currency = Currency::try_from(vault_model.currency.as_str()).unwrap_or_default();

        let exists = wallets::Entity::find()
            .filter(wallets::Column::VaultId.eq(vault_id.to_string()))
            .filter(Expr::cust("LOWER(name)").eq(name.to_lowercase()))
            .one(&db_tx)
            .await?
            .is_some();
        if exists {
            return Err(EngineError::ExistingKey(name.to_string()));
        }

        // Create the wallet with a 0 balance. If `balance_minor != 0`, we represent it
        // as an opening transaction that affects both the wallet and
        // Unallocated.
        let wallet = Wallet::new(name.to_string(), 0, currency);
        let wallet_id = wallet.id;
        let mut wallet_model: wallets::ActiveModel = (&wallet).into();
        wallet_model.vault_id = ActiveValue::Set(vault_model.id);
        wallet_model.insert(&db_tx).await?;

        if balance_minor != 0 {
            let (kind, signed_amount) = if balance_minor > 0 {
                (TransactionKind::Income, balance_minor)
            } else {
                (TransactionKind::Expense, balance_minor)
            };
            let amount_minor = balance_minor.abs();

            let tx = Transaction::new(TransactionNew {
                vault_id: vault_id.to_string(),
                kind,
                occurred_at,
                amount_minor,
                currency,
                category: Some("opening".to_string()),
                note: Some(format!("opening balance for wallet '{name}'")),
                created_by: user_id.to_string(),
                idempotency_key: None,
                refunded_transaction_id: None,
            })?;

            let unallocated_flow_id = self.unallocated_flow_id(&db_tx, vault_id).await?;
            let legs = vec![
                Leg::new(
                    tx.id,
                    LegTarget::Wallet { wallet_id },
                    signed_amount,
                    currency,
                ),
                Leg::new(
                    tx.id,
                    LegTarget::Flow {
                        flow_id: unallocated_flow_id,
                    },
                    signed_amount,
                    currency,
                ),
            ];
            self.create_transaction_with_legs(&db_tx, vault_id, currency, &tx, &legs)
                .await?;
        }

        db_tx.commit().await?;
        Ok(wallet_id)
    }

    /// Recomputes denormalized balances for wallets and flows from the ledger
    /// (`transactions` + `legs`).
    ///
    /// - Ignores voided transactions.
    /// - Ignores legacy `entries`.
    /// - Validates flow invariants while replaying legs in chronological order.
    /// - Refreshes the in-memory vault state from DB models post-commit.
    pub async fn recompute_balances(&self, vault_id: &str, user_id: &str) -> ResultEngine<()> {
        self.with_tx(|db_tx| async move {
            let vault_model = self
                .require_vault_by_id_write(db_tx, vault_id, user_id)
                .await?;
            let currency = Currency::try_from(vault_model.currency.as_str()).unwrap_or_default();

            // Load all wallets/flows from DB (including archived) to avoid stale RAM
            // issues.
            let wallet_models: Vec<wallets::Model> = wallets::Entity::find()
                .filter(wallets::Column::VaultId.eq(vault_id.to_string()))
                .all(db_tx)
                .await?;
            let flow_models: Vec<cash_flows::Model> = cash_flows::Entity::find()
                .filter(cash_flows::Column::VaultId.eq(vault_id.to_string()))
                .all(db_tx)
                .await?;

            let mut wallets_by_id: HashMap<Uuid, Wallet> = HashMap::new();
            for model in wallet_models {
                let mut wallet = Wallet::try_from((model, currency))?;
                wallet.balance = 0;
                wallets_by_id.insert(wallet.id, wallet);
            }

            let mut flows: HashMap<Uuid, CashFlow> = HashMap::new();
            for model in flow_models {
                let mut flow = CashFlow::try_from((model, currency))?;
                flow.balance = 0;
                if flow.income_balance.is_some() {
                    flow.income_balance = Some(0);
                }
                flows.insert(flow.id, flow);
            }

            // Replay all non-voided legs in chronological order to validate invariants.
            let leg_models: Vec<legs::Model> = legs::Entity::find()
                .join(JoinType::InnerJoin, legs::Relation::Transactions.def())
                .filter(transactions::Column::VaultId.eq(vault_id.to_string()))
                .filter(transactions::Column::VoidedAt.is_null())
                .order_by_asc(transactions::Column::OccurredAt)
                .order_by_asc(legs::Column::Id)
                .all(db_tx)
                .await?;

            for leg_model in leg_models {
                let leg = Leg::try_from(leg_model)?;
                ensure_vault_currency(currency, leg.currency)?;

                match leg.target {
                    LegTarget::Wallet { wallet_id } => {
                        let wallet = wallets_by_id.get_mut(&wallet_id).ok_or_else(|| {
                            EngineError::KeyNotFound("wallet not exists".to_string())
                        })?;
                        wallet.balance += leg.amount_minor;
                    }
                    LegTarget::Flow { flow_id } => {
                        let flow = flows.get_mut(&flow_id).ok_or_else(|| {
                            EngineError::KeyNotFound("cash_flow not exists".to_string())
                        })?;
                        flow.apply_leg_change(0, leg.amount_minor)?;
                    }
                }
            }

            // Persist denormalized balances.
            for (wallet_id, wallet) in &wallets_by_id {
                let wallet_model = wallets::ActiveModel {
                    id: ActiveValue::Set(wallet_id.to_string()),
                    balance: ActiveValue::Set(wallet.balance),
                    ..Default::default()
                };
                wallet_model.update(db_tx).await?;
            }

            for (flow_id, flow) in &flows {
                let flow_model = cash_flows::ActiveModel {
                    id: ActiveValue::Set(flow_id.to_string()),
                    balance: ActiveValue::Set(flow.balance),
                    income_balance: ActiveValue::Set(flow.income_balance),
                    ..Default::default()
                };
                flow_model.update(db_tx).await?;
            }

            Ok(())
        })
        .await
    }

    /// Return a user `Vault`.
    /// Return a vault snapshot from DB, including all wallets and flows.
    pub async fn vault_snapshot(
        &self,
        vault_id: Option<&str>,
        vault_name: Option<String>,
        user_id: &str,
    ) -> ResultEngine<Vault> {
        if vault_id.is_none() && vault_name.is_none() {
            return Err(EngineError::KeyNotFound(
                "missing vault id or name".to_string(),
            ));
        }
        self.with_tx(|db_tx| async move {
            let vault_model = if let Some(id) = vault_id {
                self.require_vault_by_id(db_tx, id, user_id).await?
            } else {
                let name = vault_name.ok_or_else(|| {
                    EngineError::KeyNotFound("missing vault id or name".to_string())
                })?;
                self.require_vault_by_name(db_tx, &name, user_id).await?
            };
            let vault_currency =
                Currency::try_from(vault_model.currency.as_str()).unwrap_or_default();

            let flow_models: Vec<cash_flows::Model> = cash_flows::Entity::find()
                .filter(cash_flows::Column::VaultId.eq(vault_model.id.clone()))
                .all(db_tx)
                .await?;
            let wallet_models: Vec<wallets::Model> = wallets::Entity::find()
                .filter(wallets::Column::VaultId.eq(vault_model.id.clone()))
                .all(db_tx)
                .await?;

            let mut flows = HashMap::new();
            for flow_model in flow_models {
                let flow = CashFlow::try_from((flow_model, vault_currency))?;
                flows.insert(flow.id, flow);
            }

            let mut wallets_map = HashMap::new();
            for wallet_model in wallet_models {
                let wallet = Wallet::try_from((wallet_model, vault_currency))?;
                wallets_map.insert(wallet.id, wallet);
            }

            Ok(Vault {
                id: vault_model.id,
                name: vault_model.name,
                cash_flow: flows,
                wallet: wallets_map,
                user_id: vault_model.user_id,
                currency: vault_currency,
            })
        })
        .await
    }

    /// Return a wallet snapshot from DB.
    pub async fn wallet(
        &self,
        wallet_id: Uuid,
        vault_id: &str,
        user_id: &str,
    ) -> ResultEngine<Wallet> {
        self.with_tx(|db_tx| async move {
            let vault_model = self.require_vault_by_id(db_tx, vault_id, user_id).await?;
            let vault_currency =
                Currency::try_from(vault_model.currency.as_str()).unwrap_or_default();

            let model = wallets::Entity::find_by_id(wallet_id.to_string())
                .filter(wallets::Column::VaultId.eq(vault_id.to_string()))
                .one(db_tx)
                .await?
                .ok_or_else(|| EngineError::KeyNotFound("wallet not exists".to_string()))?;

            Wallet::try_from((model, vault_currency))
        })
        .await
    }

    /// Returns vault totals: `(currency, balance_minor, total_income_minor,
    /// total_expenses_minor)`.
    ///
    /// Transfers are excluded from income/expense totals.
    pub async fn vault_statistics(
        &self,
        vault_id: &str,
        user_id: &str,
        include_voided: bool,
    ) -> ResultEngine<(Currency, i64, i64, i64)> {
        let db_tx = self.database.begin().await?;
        let vault_model = self.require_vault_by_id(&db_tx, vault_id, user_id).await?;
        let currency = Currency::try_from(vault_model.currency.as_str()).unwrap_or_default();

        let backend = self.database.get_database_backend();
        let void_cond = if include_voided {
            ""
        } else {
            " AND voided_at IS NULL"
        };

        let balance_minor: i64 = {
            let stmt = Statement::from_sql_and_values(
                backend,
                "SELECT COALESCE(SUM(balance), 0) AS sum FROM wallets WHERE vault_id = ? AND archived = 0;"
                    .to_string(),
                vec![vault_id.into()],
            );
            let row = db_tx.query_one(stmt).await?;
            row.and_then(|r| r.try_get("", "sum").ok()).unwrap_or(0)
        };

        let total_income_minor: i64 = {
            let stmt = Statement::from_sql_and_values(
                backend,
                format!(
                    "SELECT COALESCE(SUM(amount_minor), 0) AS sum \
                     FROM transactions \
                     WHERE vault_id = ? AND kind = ?{void_cond}"
                ),
                vec![vault_id.into(), TransactionKind::Income.as_str().into()],
            );
            let row = db_tx.query_one(stmt).await?;
            row.and_then(|r| r.try_get("", "sum").ok()).unwrap_or(0)
        };

        let total_expenses_minor: i64 = {
            let stmt = Statement::from_sql_and_values(
                backend,
                format!(
                    "SELECT COALESCE(SUM(amount_minor), 0) AS sum \
                     FROM transactions \
                     WHERE vault_id = ? AND kind = ?{void_cond}"
                ),
                vec![vault_id.into(), TransactionKind::Expense.as_str().into()],
            );
            let row = db_tx.query_one(stmt).await?;
            row.and_then(|r| r.try_get("", "sum").ok()).unwrap_or(0)
        };

        let total_refunds_minor: i64 = {
            let stmt = Statement::from_sql_and_values(
                backend,
                format!(
                    "SELECT COALESCE(SUM(amount_minor), 0) AS sum \
                     FROM transactions \
                     WHERE vault_id = ? AND kind = ?{void_cond}"
                ),
                vec![vault_id.into(), TransactionKind::Refund.as_str().into()],
            );
            let row = db_tx.query_one(stmt).await?;
            row.and_then(|r| r.try_get("", "sum").ok()).unwrap_or(0)
        };

        db_tx.commit().await?;
        Ok((
            currency,
            balance_minor,
            total_income_minor,
            total_expenses_minor - total_refunds_minor,
        ))
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
        let db_tx = self.database.begin().await?;
        self.require_flow_read(&db_tx, vault_id, flow_id, user_id)
            .await?;

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

        let limit_plus_one = limit.saturating_add(1);
        let mut query = legs::Entity::find()
            .filter(legs::Column::TargetKind.eq(crate::legs::LegTargetKind::Flow.as_str()))
            .filter(legs::Column::TargetId.eq(flow_id.to_string()))
            .find_also_related(transactions::Entity)
            .filter(transactions::Column::VaultId.eq(vault_id.to_string()))
            .order_by_desc(transactions::Column::OccurredAt)
            .order_by_desc(transactions::Column::Id)
            .limit(limit_plus_one);

        if let Some(from) = filter.from {
            query = query.filter(transactions::Column::OccurredAt.gte(from));
        }
        if let Some(to) = filter.to {
            query = query.filter(transactions::Column::OccurredAt.lt(to));
        }

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

        if !filter.include_voided {
            query = query.filter(transactions::Column::VoidedAt.is_null());
        }
        if let Some(kinds) = &filter.kinds {
            let kinds: Vec<String> = kinds.iter().map(|k| k.as_str().to_string()).collect();
            query = query.filter(transactions::Column::Kind.is_in(kinds));
        } else if !filter.include_transfers {
            query = query.filter(transactions::Column::Kind.is_not_in([
                TransactionKind::TransferWallet.as_str(),
                TransactionKind::TransferFlow.as_str(),
            ]));
        }

        let rows: Vec<(legs::Model, Option<transactions::Model>)> = query.all(&db_tx).await?;
        let has_more = rows.len() > limit as usize;

        let mut out: Vec<(Transaction, i64)> = Vec::with_capacity(rows.len().min(limit as usize));
        for (leg_model, tx_model) in rows.into_iter().take(limit as usize) {
            let Some(tx_model) = tx_model else { continue };
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

        db_tx.commit().await?;
        Ok((out, next_cursor))
    }

    /// Lists recent transactions in a vault (vault-wide), with cursor-based
    /// pagination.
    ///
    /// Pagination is newest → older by `(occurred_at DESC, transaction_id
    /// DESC)`.
    ///
    /// Authorization: requires vault read access.
    pub async fn list_transactions_for_vault_page(
        &self,
        vault_id: &str,
        user_id: &str,
        limit: u64,
        cursor: Option<&str>,
        filter: &TransactionListFilter,
    ) -> ResultEngine<(Vec<Transaction>, Option<String>)> {
        let db_tx = self.database.begin().await?;
        self.require_vault_by_id(&db_tx, vault_id, user_id).await?;

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

        let limit_plus_one = limit.saturating_add(1);
        let mut query = transactions::Entity::find()
            .filter(transactions::Column::VaultId.eq(vault_id.to_string()))
            .order_by_desc(transactions::Column::OccurredAt)
            .order_by_desc(transactions::Column::Id)
            .limit(limit_plus_one);

        if let Some(from) = filter.from {
            query = query.filter(transactions::Column::OccurredAt.gte(from));
        }
        if let Some(to) = filter.to {
            query = query.filter(transactions::Column::OccurredAt.lt(to));
        }

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

        if !filter.include_voided {
            query = query.filter(transactions::Column::VoidedAt.is_null());
        }
        if let Some(kinds) = &filter.kinds {
            let kinds: Vec<String> = kinds.iter().map(|k| k.as_str().to_string()).collect();
            query = query.filter(transactions::Column::Kind.is_in(kinds));
        } else if !filter.include_transfers {
            query = query.filter(transactions::Column::Kind.is_not_in([
                TransactionKind::TransferWallet.as_str(),
                TransactionKind::TransferFlow.as_str(),
            ]));
        }

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

        db_tx.commit().await?;
        Ok((out, next_cursor))
    }

    /// Renames an existing wallet.
    ///
    /// Authorization: requires vault write access.
    pub async fn rename_wallet(
        &self,
        vault_id: &str,
        wallet_id: Uuid,
        new_name: &str,
        user_id: &str,
    ) -> ResultEngine<()> {
        let new_name = normalize_required_name(new_name, "wallet")?;

        let db_tx = self.database.begin().await?;
        self.require_vault_by_id_write(&db_tx, vault_id, user_id)
            .await?;
        self.require_wallet_in_vault(&db_tx, vault_id, wallet_id)
            .await?;

        let exists = wallets::Entity::find()
            .filter(wallets::Column::VaultId.eq(vault_id.to_string()))
            .filter(Expr::cust("LOWER(name)").eq(new_name.to_lowercase()))
            .filter(wallets::Column::Id.ne(wallet_id.to_string()))
            .one(&db_tx)
            .await?
            .is_some();
        if exists {
            return Err(EngineError::ExistingKey(new_name));
        }

        let active = wallets::ActiveModel {
            id: ActiveValue::Set(wallet_id.to_string()),
            name: ActiveValue::Set(new_name),
            ..Default::default()
        };
        active.update(&db_tx).await?;
        db_tx.commit().await?;
        Ok(())
    }

    /// Archives/unarchives an existing wallet.
    ///
    /// Authorization: requires vault write access.
    pub async fn set_wallet_archived(
        &self,
        vault_id: &str,
        wallet_id: Uuid,
        archived: bool,
        user_id: &str,
    ) -> ResultEngine<()> {
        let db_tx = self.database.begin().await?;
        self.require_vault_by_id_write(&db_tx, vault_id, user_id)
            .await?;
        self.require_wallet_in_vault(&db_tx, vault_id, wallet_id)
            .await?;

        let active = wallets::ActiveModel {
            id: ActiveValue::Set(wallet_id.to_string()),
            archived: ActiveValue::Set(archived),
            ..Default::default()
        };
        active.update(&db_tx).await?;
        db_tx.commit().await?;
        Ok(())
    }

    /// Renames an existing cash flow.
    ///
    /// Authorization: requires flow write access.
    pub async fn rename_cash_flow(
        &self,
        vault_id: &str,
        flow_id: Uuid,
        new_name: &str,
        user_id: &str,
    ) -> ResultEngine<()> {
        let new_name = normalize_required_flow_name(new_name)?;
        if new_name.eq_ignore_ascii_case(cash_flows::UNALLOCATED_INTERNAL_NAME) {
            return Err(EngineError::InvalidFlow(
                "flow name is reserved".to_string(),
            ));
        }

        let db_tx = self.database.begin().await?;
        let flow_model = self
            .require_flow_write(&db_tx, vault_id, flow_id, user_id)
            .await?;
        let system_kind = flow_model
            .system_kind
            .as_deref()
            .and_then(|k| cash_flows::SystemFlowKind::try_from(k).ok());
        if system_kind.is_some() {
            return Err(EngineError::InvalidFlow(
                "cannot rename system flow".to_string(),
            ));
        }

        let exists = cash_flows::Entity::find()
            .filter(cash_flows::Column::VaultId.eq(vault_id.to_string()))
            .filter(Expr::cust("LOWER(name)").eq(new_name.to_lowercase()))
            .filter(cash_flows::Column::Id.ne(flow_id.to_string()))
            .one(&db_tx)
            .await?
            .is_some();
        if exists {
            return Err(EngineError::ExistingKey(new_name));
        }

        let active = cash_flows::ActiveModel {
            id: ActiveValue::Set(flow_id.to_string()),
            name: ActiveValue::Set(new_name),
            ..Default::default()
        };
        active.update(&db_tx).await?;
        db_tx.commit().await?;
        Ok(())
    }

    /// Archives/unarchives an existing cash flow.
    ///
    /// Authorization: requires flow write access.
    pub async fn set_cash_flow_archived(
        &self,
        vault_id: &str,
        flow_id: Uuid,
        archived: bool,
        user_id: &str,
    ) -> ResultEngine<()> {
        let db_tx = self.database.begin().await?;
        let flow_model = self
            .require_flow_write(&db_tx, vault_id, flow_id, user_id)
            .await?;
        let system_kind = flow_model
            .system_kind
            .as_deref()
            .and_then(|k| cash_flows::SystemFlowKind::try_from(k).ok());
        if system_kind.is_some() {
            return Err(EngineError::InvalidFlow(
                "cannot archive system flow".to_string(),
            ));
        }

        let active = cash_flows::ActiveModel {
            id: ActiveValue::Set(flow_id.to_string()),
            archived: ActiveValue::Set(archived),
            ..Default::default()
        };
        active.update(&db_tx).await?;
        db_tx.commit().await?;
        Ok(())
    }

    /// Updates the cap mode for a cash flow.
    ///
    /// `max_balance` defines the cap value:
    /// - `None`: Unlimited
    /// - `Some(cap)`: NetCapped or IncomeCapped, depending on `income_capped`
    ///
    /// If `income_capped` is true, this method sets `income_balance` to the
    /// cumulative sum of positive legs for this flow (ignoring voided
    /// transactions), and validates `income_balance <= cap`.
    ///
    /// Authorization: requires flow write access.
    pub async fn set_cash_flow_mode(
        &self,
        vault_id: &str,
        flow_id: Uuid,
        max_balance: Option<i64>,
        income_capped: bool,
        user_id: &str,
    ) -> ResultEngine<()> {
        if income_capped && max_balance.is_none() {
            return Err(EngineError::InvalidFlow(
                "income-capped flow requires a cap".to_string(),
            ));
        }
        if let Some(cap_minor) = max_balance
            && cap_minor <= 0
        {
            return Err(EngineError::InvalidFlow("cap must be > 0".to_string()));
        }

        let db_tx = self.database.begin().await?;
        let flow_model = self
            .require_flow_write(&db_tx, vault_id, flow_id, user_id)
            .await?;
        let flow_name = flow_model.name.clone();
        let system_kind = flow_model
            .system_kind
            .as_deref()
            .and_then(|k| cash_flows::SystemFlowKind::try_from(k).ok());
        if system_kind.is_some() {
            return Err(EngineError::InvalidFlow(
                "cannot change mode for system flow".to_string(),
            ));
        }

        let (max_balance, income_balance) = match max_balance {
            None => (None, None),
            Some(cap_minor) if !income_capped => {
                if flow_model.balance > cap_minor {
                    return Err(EngineError::MaxBalanceReached(flow_name));
                }
                (Some(cap_minor), None)
            }
            Some(cap_minor) => {
                let stmt = Statement::from_sql_and_values(
                    db_tx.get_database_backend(),
                    "SELECT COALESCE(SUM(l.amount_minor), 0) AS sum \
                     FROM legs l \
                     JOIN transactions t ON t.id = l.transaction_id \
                     WHERE t.vault_id = ? \
                       AND t.voided_at IS NULL \
                       AND l.target_kind = ? \
                       AND l.target_id = ? \
                       AND l.amount_minor > 0",
                    vec![
                        vault_id.into(),
                        crate::legs::LegTargetKind::Flow.as_str().into(),
                        flow_id.to_string().into(),
                    ],
                );
                let row = db_tx.query_one(stmt).await?;
                let income_total_minor = row.and_then(|r| r.try_get("", "sum").ok()).unwrap_or(0);
                if income_total_minor > cap_minor {
                    return Err(EngineError::MaxBalanceReached(flow_name));
                }
                (Some(cap_minor), Some(income_total_minor))
            }
        };

        validate_flow_mode_fields(&flow_name, max_balance, income_balance)?;

        let active = cash_flows::ActiveModel {
            id: ActiveValue::Set(flow_id.to_string()),
            max_balance: ActiveValue::Set(max_balance),
            income_balance: ActiveValue::Set(income_balance),
            ..Default::default()
        };
        active.update(&db_tx).await?;
        db_tx.commit().await?;
        Ok(())
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
        let db_tx = self.database.begin().await?;
        self.require_vault_by_id(&db_tx, vault_id, user_id).await?;

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

        let limit_plus_one = limit.saturating_add(1);
        let mut query = legs::Entity::find()
            .filter(legs::Column::TargetKind.eq(crate::legs::LegTargetKind::Wallet.as_str()))
            .filter(legs::Column::TargetId.eq(wallet_id.to_string()))
            .find_also_related(transactions::Entity)
            .filter(transactions::Column::VaultId.eq(vault_id.to_string()))
            .order_by_desc(transactions::Column::OccurredAt)
            .order_by_desc(transactions::Column::Id)
            .limit(limit_plus_one);

        if let Some(from) = filter.from {
            query = query.filter(transactions::Column::OccurredAt.gte(from));
        }
        if let Some(to) = filter.to {
            query = query.filter(transactions::Column::OccurredAt.lt(to));
        }

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

        if !filter.include_voided {
            query = query.filter(transactions::Column::VoidedAt.is_null());
        }
        if let Some(kinds) = &filter.kinds {
            let kinds: Vec<String> = kinds.iter().map(|k| k.as_str().to_string()).collect();
            query = query.filter(transactions::Column::Kind.is_in(kinds));
        } else if !filter.include_transfers {
            query = query.filter(transactions::Column::Kind.is_not_in([
                TransactionKind::TransferWallet.as_str(),
                TransactionKind::TransferFlow.as_str(),
            ]));
        }

        let rows: Vec<(legs::Model, Option<transactions::Model>)> = query.all(&db_tx).await?;
        let has_more = rows.len() > limit as usize;

        let mut out: Vec<(Transaction, i64)> = Vec::with_capacity(rows.len().min(limit as usize));
        for (leg_model, tx_model) in rows.into_iter().take(limit as usize) {
            let Some(tx_model) = tx_model else { continue };
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

        db_tx.commit().await?;
        Ok((out, next_cursor))
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
        let db_tx = self.database.begin().await?;
        let vault_model = vault::Entity::find_by_id(vault_id.to_string())
            .one(&db_tx)
            .await?
            .ok_or_else(|| EngineError::KeyNotFound("vault not exists".to_string()))?;
        if vault_model.user_id != user_id {
            let member =
                vault_memberships::Entity::find_by_id((vault_id.to_string(), user_id.to_string()))
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

        db_tx.commit().await?;
        Ok(tx)
    }
}

/// The builder for `Engine`
#[derive(Default)]
pub struct EngineBuilder {
    database: DatabaseConnection,
}

impl EngineBuilder {
    /// Pass the required database
    pub fn database(mut self, db: DatabaseConnection) -> EngineBuilder {
        self.database = db;
        self
    }

    /// Construct `Engine`
    pub async fn build(self) -> ResultEngine<Engine> {
        Ok(Engine {
            database: self.database,
        })
    }
}
