use chrono::Utc;
use uuid::Uuid;

use sea_orm::{
    ActiveValue, QueryFilter, Statement, TransactionTrait, prelude::*, sea_query::Expr,
};

use crate::{
    cash_flows, vault, CashFlow, Currency, EngineError, Leg, LegTarget, ResultEngine,
    TransactionKind, TransactionNew,
};
use crate::util::validate_flow_mode_fields;

use super::{normalize_required_flow_name, with_tx, Engine};

impl Engine {
    /// Return a [`CashFlow`] (snapshot from DB).
    pub async fn cash_flow(
        &self,
        cash_flow_id: Uuid,
        vault_id: &str,
        user_id: &str,
    ) -> ResultEngine<CashFlow> {
        with_tx!(self, |db_tx| {
            let model = self
                .require_flow_read(&db_tx, vault_id, cash_flow_id, user_id)
                .await?;
            let vault_model = vault::Entity::find_by_id(vault_id.to_string())
                .one(&db_tx)
                .await?
                .ok_or_else(|| EngineError::KeyNotFound("vault not exists".to_string()))?;
            let vault_currency =
                Currency::try_from(vault_model.currency.as_str()).unwrap_or_default();
            let flow = CashFlow::try_from((model, vault_currency))?;
            Ok(flow)
        })
    }

    pub async fn cash_flow_by_name(
        &self,
        name: &str,
        vault_id: &str,
        user_id: &str,
    ) -> ResultEngine<CashFlow> {
        let name = normalize_required_flow_name(name)?;
        let name_lower = name.to_lowercase();
        with_tx!(self, |db_tx| {
            let vault_model = vault::Entity::find_by_id(vault_id.to_string())
                .one(&db_tx)
                .await?
                .ok_or_else(|| EngineError::KeyNotFound("vault not exists".to_string()))?;
            let vault_currency =
                Currency::try_from(vault_model.currency.as_str()).unwrap_or_default();

            let model = cash_flows::Entity::find()
                .filter(cash_flows::Column::VaultId.eq(vault_id.to_string()))
                .filter(Expr::cust("LOWER(name)").eq(name_lower))
                .one(&db_tx)
                .await?
                .ok_or_else(|| EngineError::KeyNotFound("cash_flow not exists".to_string()))?;

            if !self
                .has_vault_read_access(&db_tx, vault_id, user_id)
                .await?
            {
                let role = self
                    .flow_membership_role(&db_tx, &model.id, user_id)
                    .await?
                    .ok_or_else(|| EngineError::KeyNotFound("cash_flow not exists".to_string()))?;
                let _ = role;
            }

            let flow = CashFlow::try_from((model, vault_currency))?;
            Ok(flow)
        })
    }

    /// Delete a cash flow contained by a vault.
    pub async fn delete_cash_flow(
        &self,
        vault_id: &str,
        cash_flow_id: Uuid,
        archive: bool,
        user_id: &str,
    ) -> ResultEngine<()> {
        with_tx!(self, |db_tx| {
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

            Ok(())
        })
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
        with_tx!(self, |db_tx| {
            let vault_model = self
                .require_vault_by_id_write(&db_tx, vault_id, user_id)
                .await?;
            let vault_currency =
                Currency::try_from(vault_model.currency.as_str()).unwrap_or_default();

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
                let tx = crate::Transaction::new(TransactionNew {
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
                    Leg::new(
                        tx.id,
                        LegTarget::Flow { flow_id },
                        balance,
                        vault_currency,
                    ),
                ];
                self.create_transaction_with_legs(&db_tx, vault_id, vault_currency, &tx, &legs)
                    .await?;
            }

            Ok(flow_id)
        })
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
        with_tx!(self, |db_tx| {
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
            Ok(())
        })
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
        with_tx!(self, |db_tx| {
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
            Ok(())
        })
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
        with_tx!(self, |db_tx| {
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
                    let income_total_minor =
                        row.and_then(|r| r.try_get("", "sum").ok()).unwrap_or(0);
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
            Ok(())
        })
    }
}
