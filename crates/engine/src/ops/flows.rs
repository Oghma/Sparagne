use chrono::Utc;
use uuid::Uuid;

use sea_orm::{ActiveValue, QueryFilter, QueryOrder, Statement, prelude::*, sea_query::Expr};

use crate::{
    CashFlow, EngineError, ResultEngine, TransactionKind, cash_flows, flow_memberships,
    flow_references,
    util::{normalize_required_name, validate_flow_mode_fields},
    vault,
};

use super::{Engine, build_transaction, parse_vault_uuid, transfer_flow_legs};

/// Parameters for creating a new cash flow.
pub struct NewCashFlowParams<'a> {
    pub vault_id: &'a str,
    pub name: &'a str,
    pub balance: i64,
    pub max_balance: Option<i64>,
    pub income_bounded: Option<bool>,
    pub allow_negative: bool,
    pub user_id: &'a str,
}

impl Engine {
    /// Return a [`CashFlow`] (snapshot from DB).
    pub async fn cash_flow(
        &self,
        cash_flow_id: Uuid,
        vault_id: &str,
        user_id: &str,
    ) -> ResultEngine<CashFlow> {
        let vault_id = vault_id.to_string();
        let user_id = user_id.to_string();
        self.with_tx(|engine, db_tx| {
            Box::pin(async move {
                let model = engine
                    .require_flow_read(db_tx, vault_id.as_str(), cash_flow_id, user_id.as_str())
                    .await?;
                let vault_uuid = parse_vault_uuid(vault_id.as_str())?;
                let vault_model = vault::Entity::find_by_id(vault_uuid)
                    .one(db_tx)
                    .await?
                    .ok_or_else(|| {
                        EngineError::KeyNotFound(EngineError::VAULT_NOT_FOUND.to_string())
                    })?;
                let vault_currency = vault_model.currency;
                let flow = CashFlow::try_from((model, vault_currency))?;
                Ok(flow)
            })
        })
        .await
    }

    pub async fn cash_flow_by_name(
        &self,
        name: &str,
        vault_id: &str,
        user_id: &str,
    ) -> ResultEngine<CashFlow> {
        let name = normalize_required_name(name, "flow")?;
        let name_lower = name.to_lowercase();
        let vault_id = vault_id.to_string();
        let user_id = user_id.to_string();
        self.with_tx(|engine, db_tx| {
            Box::pin(async move {
                let vault_uuid = parse_vault_uuid(vault_id.as_str())?;
                let vault_model = vault::Entity::find_by_id(vault_uuid)
                    .one(db_tx)
                    .await?
                    .ok_or_else(|| {
                        EngineError::KeyNotFound(EngineError::VAULT_NOT_FOUND.to_string())
                    })?;
                let vault_currency = vault_model.currency;

                let model = cash_flows::Entity::find()
                    .filter(cash_flows::Column::VaultId.eq(vault_uuid))
                    .filter(Expr::cust("LOWER(name)").eq(name_lower))
                    .one(db_tx)
                    .await?
                    .ok_or_else(|| {
                        EngineError::KeyNotFound(EngineError::FLOW_NOT_FOUND.to_string())
                    })?;

                if !engine
                    .has_vault_read_access(db_tx, vault_id.as_str(), user_id.as_str())
                    .await?
                {
                    let role = engine
                        .flow_membership_role(db_tx, model.id, user_id.as_str())
                        .await?
                        .ok_or_else(|| {
                            EngineError::KeyNotFound(EngineError::FLOW_NOT_FOUND.to_string())
                        })?;
                    let _ = role;
                }

                let flow = CashFlow::try_from((model, vault_currency))?;
                Ok(flow)
            })
        })
        .await
    }

    /// Lists flows the user can access within a vault.
    ///
    /// Authorization:
    /// - If the user has vault access, returns all flows for the vault.
    /// - Otherwise, returns only flows explicitly shared via flow memberships.
    pub async fn list_accessible_flows(
        &self,
        vault_id: &str,
        user_id: &str,
        include_archived: bool,
    ) -> ResultEngine<Vec<CashFlow>> {
        let vault_id = vault_id.to_string();
        let user_id = user_id.to_string();
        self.with_tx(|engine, db_tx| {
            Box::pin(async move {
                let vault_uuid = parse_vault_uuid(vault_id.as_str())?;
                let vault_model = vault::Entity::find_by_id(vault_uuid)
                    .one(db_tx)
                    .await?
                    .ok_or_else(|| {
                        EngineError::KeyNotFound(EngineError::VAULT_NOT_FOUND.to_string())
                    })?;
                let vault_currency = vault_model.currency;

                let has_vault_access = if vault_model.user_id == user_id {
                    true
                } else {
                    engine
                        .vault_membership_role(db_tx, vault_uuid, user_id.as_str())
                        .await?
                        .is_some()
                };

                // Get direct flows (owned by this vault)
                let mut direct_flow_models = if has_vault_access {
                    let mut query = cash_flows::Entity::find()
                        .filter(cash_flows::Column::VaultId.eq(vault_uuid));
                    if !include_archived {
                        query = query.filter(cash_flows::Column::Archived.eq(false));
                    }
                    query.all(db_tx).await?
                } else {
                    let rows: Vec<(flow_memberships::Model, Option<cash_flows::Model>)> =
                        flow_memberships::Entity::find()
                            .filter(flow_memberships::Column::UserId.eq(user_id.clone()))
                            .find_also_related(cash_flows::Entity)
                            .all(db_tx)
                            .await?;

                    let mut models = Vec::new();
                    for (_membership, flow) in rows {
                        let Some(flow) = flow else {
                            continue;
                        };
                        if flow.vault_id != vault_uuid {
                            continue;
                        }
                        if !include_archived && flow.archived {
                            continue;
                        }
                        models.push(flow);
                    }
                    models
                };

                // Get referenced flows (from other vaults, appearing here via flow_references)
                let flow_refs = flow_references::Entity::find()
                    .filter(flow_references::Column::VaultId.eq(vault_uuid))
                    .all(db_tx)
                    .await?;

                let referenced_flow_ids: Vec<Uuid> =
                    flow_refs.iter().map(|r| r.target_flow_id).collect();

                let mut referenced_flow_models = if !referenced_flow_ids.is_empty() {
                    let mut query = cash_flows::Entity::find()
                        .filter(cash_flows::Column::Id.is_in(referenced_flow_ids));
                    if !include_archived {
                        query = query.filter(cash_flows::Column::Archived.eq(false));
                    }
                    query.all(db_tx).await?
                } else {
                    vec![]
                };

                // Combine direct and referenced flows
                direct_flow_models.append(&mut referenced_flow_models);
                let flow_models = direct_flow_models;

                let mut flows = Vec::with_capacity(flow_models.len());
                for model in flow_models {
                    flows.push(CashFlow::try_from((model, vault_currency))?);
                }
                Ok(flows)
            })
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
        let vault_id = vault_id.to_string();
        let user_id = user_id.to_string();
        self.with_tx(|engine, db_tx| {
            Box::pin(async move {
                engine
                    .require_vault_by_id_write(db_tx, vault_id.as_str(), user_id.as_str())
                    .await?;

                let vault_uuid = parse_vault_uuid(vault_id.as_str())?;
                let flow_model = cash_flows::Entity::find_by_id(cash_flow_id)
                    .filter(cash_flows::Column::VaultId.eq(vault_uuid))
                    .one(db_tx)
                    .await?
                    .ok_or_else(|| {
                        EngineError::KeyNotFound(EngineError::FLOW_NOT_FOUND.to_string())
                    })?;

                if flow_model.system_kind == Some(cash_flows::SystemFlowKind::Unallocated)
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
                        id: ActiveValue::Set(cash_flow_id),
                        archived: ActiveValue::Set(true),
                        ..Default::default()
                    };
                    flow_model.update(db_tx).await?;
                } else {
                    cash_flows::Entity::delete_by_id(cash_flow_id)
                        .exec(db_tx)
                        .await?;
                }

                Ok(())
            })
        })
        .await
    }

    /// Add a new cash flow inside a vault.
    ///
    /// `balance` represents the initial allocation for the flow and is modeled
    /// as an opening `TransferFlow` from `Unallocated → this flow` (so
    /// transfers do not inflate income/expense stats).
    ///
    /// The opening transfer uses `Utc::now()` as `occurred_at`.
    pub async fn new_cash_flow(&self, params: NewCashFlowParams<'_>) -> ResultEngine<Uuid> {
        let occurred_at = Utc::now();
        let name = normalize_required_name(params.name, "flow")?;
        let vault_id = params.vault_id.to_string();
        let user_id = params.user_id.to_string();
        let balance = params.balance;
        let max_balance = params.max_balance;
        let income_bounded = params.income_bounded;
        let allow_negative = params.allow_negative;

        if balance < 0 && !allow_negative {
            return Err(EngineError::InvalidAmount(
                "flow balance must be >= 0".to_string(),
            ));
        }
        self.with_tx(|engine, db_tx| {
            Box::pin(async move {
                let vault_model = engine
                    .require_vault_by_id_write(db_tx, vault_id.as_str(), user_id.as_str())
                    .await?;
                let vault_currency = vault_model.currency;
                let vault_uuid = vault_model.id;

                if name.eq_ignore_ascii_case(cash_flows::UNALLOCATED_INTERNAL_NAME) {
                    return Err(EngineError::InvalidFlow(
                        "flow name is reserved".to_string(),
                    ));
                }
                let exists = cash_flows::Entity::find()
                    .filter(cash_flows::Column::VaultId.eq(vault_uuid))
                    .filter(Expr::cust("LOWER(name)").eq(name.to_lowercase()))
                    .one(db_tx)
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
                    allow_negative,
                )?;
                let flow_id = flow.id;
                let mut flow_model: cash_flows::ActiveModel = (&flow).into();
                flow_model.vault_id = ActiveValue::Set(vault_uuid);
                flow_model.insert(db_tx).await?;

                if balance > 0 {
                    let unallocated_flow_id =
                        engine.unallocated_flow_id(db_tx, vault_id.as_str()).await?;
                    let category = engine
                        .resolve_category(db_tx, vault_id.as_str(), None)
                        .await?;
                    let tx = build_transaction(super::TransactionBuildInput {
                        vault_id: vault_id.as_str(),
                        kind: TransactionKind::TransferFlow,
                        occurred_at,
                        amount_minor: balance,
                        currency: vault_currency,
                        category_id: category.id,
                        category: category.name,
                        note: Some(format!("opening allocation for flow '{name}'")),
                        created_by: user_id.as_str(),
                        idempotency_key: None,
                        refunded_transaction_id: None,
                    })?;
                    let legs = transfer_flow_legs(
                        tx.id,
                        unallocated_flow_id,
                        flow_id,
                        balance,
                        vault_currency,
                    );
                    engine
                        .create_transaction_with_legs(
                            db_tx,
                            vault_id.as_str(),
                            vault_currency,
                            &tx,
                            &legs,
                        )
                        .await?;
                }

                Ok(flow_id)
            })
        })
        .await
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
        let new_name = normalize_required_name(new_name, "flow")?;
        let vault_id = vault_id.to_string();
        let user_id = user_id.to_string();
        if new_name.eq_ignore_ascii_case(cash_flows::UNALLOCATED_INTERNAL_NAME) {
            return Err(EngineError::InvalidFlow(
                "flow name is reserved".to_string(),
            ));
        }
        self.with_tx(|engine, db_tx| {
            Box::pin(async move {
                let flow_model = engine
                    .require_flow_write(db_tx, vault_id.as_str(), flow_id, user_id.as_str())
                    .await?;
                if flow_model.system_kind.is_some() {
                    return Err(EngineError::InvalidFlow(
                        "cannot rename system flow".to_string(),
                    ));
                }
                let vault_uuid = parse_vault_uuid(vault_id.as_str())?;

                let exists = cash_flows::Entity::find()
                    .filter(cash_flows::Column::VaultId.eq(vault_uuid))
                    .filter(Expr::cust("LOWER(name)").eq(new_name.to_lowercase()))
                    .filter(cash_flows::Column::Id.ne(flow_id))
                    .one(db_tx)
                    .await?
                    .is_some();
                if exists {
                    return Err(EngineError::ExistingKey(new_name.clone()));
                }

                let active = cash_flows::ActiveModel {
                    id: ActiveValue::Set(flow_id),
                    name: ActiveValue::Set(new_name),
                    ..Default::default()
                };
                active.update(db_tx).await?;
                Ok(())
            })
        })
        .await
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
        let vault_id = vault_id.to_string();
        let user_id = user_id.to_string();
        self.with_tx(|engine, db_tx| {
            Box::pin(async move {
                let flow_model = engine
                    .require_flow_write(db_tx, vault_id.as_str(), flow_id, user_id.as_str())
                    .await?;
                if flow_model.system_kind.is_some() {
                    return Err(EngineError::InvalidFlow(
                        "cannot archive system flow".to_string(),
                    ));
                }

                let active = cash_flows::ActiveModel {
                    id: ActiveValue::Set(flow_id),
                    archived: ActiveValue::Set(archived),
                    ..Default::default()
                };
                active.update(db_tx).await?;
                Ok(())
            })
        })
        .await
    }

    /// Sets or clears the `allow_negative` flag on a cash flow.
    ///
    /// Turning off `allow_negative` while the balance is negative is rejected
    /// to avoid violating the non-negativity invariant.
    ///
    /// Authorization: requires flow write access.
    pub async fn set_cash_flow_allow_negative(
        &self,
        vault_id: &str,
        flow_id: Uuid,
        allow_negative: bool,
        user_id: &str,
    ) -> ResultEngine<()> {
        let vault_id = vault_id.to_string();
        let user_id = user_id.to_string();
        self.with_tx(|engine, db_tx| {
            Box::pin(async move {
                let flow_model = engine
                    .require_flow_write(db_tx, vault_id.as_str(), flow_id, user_id.as_str())
                    .await?;
                if flow_model.system_kind.is_some() {
                    return Err(EngineError::InvalidFlow(
                        "cannot change allow_negative for system flow".to_string(),
                    ));
                }
                if !allow_negative && flow_model.balance < 0 {
                    return Err(EngineError::InvalidFlow(
                        "cannot disable allow_negative while balance is negative".to_string(),
                    ));
                }

                let active = cash_flows::ActiveModel {
                    id: ActiveValue::Set(flow_id),
                    allow_negative: ActiveValue::Set(allow_negative),
                    ..Default::default()
                };
                active.update(db_tx).await?;
                Ok(())
            })
        })
        .await
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
        let vault_id = vault_id.to_string();
        let user_id = user_id.to_string();
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
        self.with_tx(|engine, db_tx| {
            Box::pin(async move {
                let flow_model = engine
                    .require_flow_write(db_tx, vault_id.as_str(), flow_id, user_id.as_str())
                    .await?;
                let flow_name = flow_model.name.clone();
                if flow_model.system_kind.is_some() {
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
                        let vault_uuid = parse_vault_uuid(vault_id.as_str())?;
                        let vault_bytes: Vec<u8> = vault_uuid.as_bytes().to_vec();
                        let flow_bytes: Vec<u8> = flow_id.as_bytes().to_vec();
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
                                vault_bytes.into(),
                                crate::legs::LegTargetKind::Flow.as_str().into(),
                                flow_bytes.into(),
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
                    id: ActiveValue::Set(flow_id),
                    max_balance: ActiveValue::Set(max_balance),
                    income_balance: ActiveValue::Set(income_balance),
                    ..Default::default()
                };
                active.update(db_tx).await?;
                Ok(())
            })
        })
        .await
    }

    /// Shares a flow with another user cross-vault.
    ///
    /// This creates:
    /// - A flow_membership for the target user (granting permissions)
    /// - A flow_reference in the target user's vault (making the flow visible
    ///   there)
    ///
    /// If `target_vault_name` is provided, uses that vault; otherwise uses the
    /// target user's primary vault (first vault owned by user).
    ///
    /// If a flow with the same name already exists in the target vault, the
    /// reference will use an override `display_name` with format "{name}
    /// ({owner})".
    ///
    /// Authorization: requires vault owner permission for the source vault.
    pub async fn share_flow_with_user(
        &self,
        vault_id: &str,
        flow_id: Uuid,
        target_user_id: &str,
        target_vault_name: Option<&str>,
        role: &str,
        user_id: &str,
    ) -> ResultEngine<()> {
        let vault_id = vault_id.to_string();
        let target_user_id = target_user_id.to_string();
        let target_vault_name = target_vault_name.map(|s| s.to_string());
        let role = role.to_string();
        let user_id = user_id.to_string();

        self.with_tx(|engine, db_tx| {
            Box::pin(async move {
                // Verify caller is vault owner
                let vault_model = engine
                    .require_vault_owner(db_tx, vault_id.as_str(), user_id.as_str())
                    .await?;
                let vault_uuid = vault_model.id;

                // Verify target user exists
                engine
                    .require_user_exists(db_tx, target_user_id.as_str())
                    .await?;

                // Verify flow exists and belongs to this vault
                let flow_model = cash_flows::Entity::find_by_id(flow_id)
                    .filter(cash_flows::Column::VaultId.eq(vault_uuid))
                    .one(db_tx)
                    .await?
                    .ok_or_else(|| {
                        EngineError::KeyNotFound(EngineError::FLOW_NOT_FOUND.to_string())
                    })?;

                // Cannot share Unallocated
                if flow_model.system_kind == Some(cash_flows::SystemFlowKind::Unallocated) {
                    return Err(EngineError::InvalidFlow(
                        "cannot share Unallocated".to_string(),
                    ));
                }

                // Get or find target vault
                let target_vault_model = if let Some(ref name) = target_vault_name {
                    engine
                        .require_vault_by_name(db_tx, name.as_str(), target_user_id.as_str())
                        .await?
                } else {
                    // Use target user's primary vault (first vault by name)
                    vault::Entity::find()
                        .filter(vault::Column::UserId.eq(target_user_id.clone()))
                        .order_by_asc(vault::Column::Name)
                        .one(db_tx)
                        .await?
                        .ok_or_else(|| {
                            EngineError::KeyNotFound(format!(
                                "no vault found for user '{}'",
                                target_user_id
                            ))
                        })?
                };

                // Create or update flow membership
                let role_validated = super::access::MembershipRole::try_from(role.as_str())?;
                let membership_active = flow_memberships::ActiveModel {
                    flow_id: ActiveValue::Set(flow_id),
                    user_id: ActiveValue::Set(target_user_id.clone()),
                    role: ActiveValue::Set(role_validated.as_str().to_string()),
                };

                match flow_memberships::Entity::find_by_id((flow_id, target_user_id.clone()))
                    .one(db_tx)
                    .await?
                {
                    Some(_) => {
                        membership_active.update(db_tx).await?;
                    }
                    None => {
                        membership_active.insert(db_tx).await?;
                    }
                }

                // Check if flow_reference already exists
                let existing_ref = flow_references::Entity::find()
                    .filter(flow_references::Column::VaultId.eq(target_vault_model.id))
                    .filter(flow_references::Column::TargetFlowId.eq(flow_id))
                    .one(db_tx)
                    .await?;

                if existing_ref.is_some() {
                    // Reference already exists, nothing more to do
                    return Ok(());
                }

                // Check for name conflicts in target vault
                let flow_name_lower = flow_model.name.to_lowercase();
                let name_conflict_exists = cash_flows::Entity::find()
                    .filter(cash_flows::Column::VaultId.eq(target_vault_model.id))
                    .filter(Expr::cust("LOWER(name)").eq(&flow_name_lower))
                    .one(db_tx)
                    .await?
                    .is_some()
                    || flow_references::Entity::find()
                        .filter(flow_references::Column::VaultId.eq(target_vault_model.id))
                        .find_also_related(cash_flows::Entity)
                        .all(db_tx)
                        .await?
                        .iter()
                        .any(|(ref_model, flow_opt)| {
                            if let Some(override_name) = &ref_model.display_name {
                                override_name.to_lowercase() == flow_name_lower
                            } else if let Some(flow) = flow_opt {
                                flow.name.to_lowercase() == flow_name_lower
                            } else {
                                false
                            }
                        });

                let display_name_override = if name_conflict_exists {
                    // Generate "{name} ({owner})" format
                    Some(format!("{} ({})", flow_model.name, vault_model.user_id))
                } else {
                    None
                };

                // Create flow_reference
                let reference_active = flow_references::ActiveModel {
                    id: ActiveValue::Set(Uuid::new_v4()),
                    vault_id: ActiveValue::Set(target_vault_model.id),
                    target_flow_id: ActiveValue::Set(flow_id),
                    display_name: ActiveValue::Set(display_name_override),
                    created_at: ActiveValue::Set(Utc::now()),
                };
                reference_active.insert(db_tx).await?;

                Ok(())
            })
        })
        .await
    }

    /// Removes a flow reference from a vault, making the shared flow no longer
    /// visible in that vault.
    ///
    /// This allows a member to "unshare" a flow from their own vault without
    /// affecting the flow itself or other members' access.
    ///
    /// - Removes the flow_reference entry from the specified vault
    /// - Does NOT remove the flow_membership (user still has permission if
    ///   re-shared)
    /// - Does NOT affect the flow data itself (remains in owner's vault)
    /// - Does NOT affect other users' references to the same flow
    ///
    /// Authorization: user must have write access to the vault (owner or
    /// editor).
    pub async fn remove_flow_reference(
        &self,
        vault_id: &str,
        flow_id: Uuid,
        user_id: &str,
    ) -> ResultEngine<()> {
        let vault_id = vault_id.to_string();
        let user_id = user_id.to_string();

        self.with_tx(|engine, db_tx| {
            Box::pin(async move {
                // Verify user has write access to the vault
                let vault_model = engine
                    .require_vault_by_id_write(db_tx, vault_id.as_str(), user_id.as_str())
                    .await?;

                // Remove flow_reference
                let deleted = flow_references::Entity::delete_many()
                    .filter(flow_references::Column::VaultId.eq(vault_model.id))
                    .filter(flow_references::Column::TargetFlowId.eq(flow_id))
                    .exec(db_tx)
                    .await?;

                if deleted.rows_affected == 0 {
                    return Err(EngineError::KeyNotFound(
                        "flow reference not found in this vault".to_string(),
                    ));
                }

                Ok(())
            })
        })
        .await
    }
}
