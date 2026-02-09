//! Recurring template CRUD, pending detection, and execution.

use chrono::{NaiveDate, NaiveTime, TimeZone, Utc};
use sea_orm::{ActiveValue, QueryFilter, prelude::*};
use uuid::Uuid;

use crate::{
    CreateRecurringCmd, EngineError, ResultEngine, TransactionKind, TxMeta, UpdateRecurringCmd,
    recurring_templates::{
        self, PendingRecurring, RecurringTemplate, compute_current_period_date,
        validate_day_of_period,
    },
};

use super::{Engine, parse_vault_uuid, transactions::write::common::FlowWalletCmd};

impl Engine {
    /// Create a new recurring template.
    pub async fn create_recurring(&self, cmd: CreateRecurringCmd) -> ResultEngine<Uuid> {
        match cmd.kind {
            TransactionKind::Income | TransactionKind::Expense => {}
            _ => {
                return Err(EngineError::InvalidRecurring(
                    "recurring templates only support income or expense".to_string(),
                ));
            }
        }
        if cmd.amount_minor <= 0 {
            return Err(EngineError::InvalidAmount("amount must be > 0".to_string()));
        }
        validate_day_of_period(cmd.frequency, cmd.day_of_period)?;

        let vault_id = cmd.vault_id.clone();
        let user_id = cmd.user_id.clone();
        self.with_tx(|engine, db_tx| {
            Box::pin(async move {
                engine
                    .require_vault_by_id_write(db_tx, &vault_id, &user_id)
                    .await?;

                let category = engine
                    .resolve_category_input(
                        db_tx,
                        &vault_id,
                        cmd.category_id,
                        cmd.category.as_deref(),
                    )
                    .await?;

                let id = Uuid::new_v4();
                let vault_uuid = parse_vault_uuid(&vault_id)?;
                let now = Utc::now().to_rfc3339();

                let template = RecurringTemplate {
                    id,
                    vault_id: vault_uuid,
                    kind: cmd.kind,
                    amount_minor: cmd.amount_minor,
                    wallet_id: cmd.wallet_id,
                    flow_id: cmd.flow_id,
                    category_id: category.id,
                    note: cmd.note,
                    created_by: user_id.clone(),
                    frequency: cmd.frequency,
                    day_of_period: cmd.day_of_period,
                    start_date: cmd.start_date,
                    end_date: cmd.end_date,
                    enabled: true,
                    last_executed_date: None,
                    created_at: now,
                    archived_at: None,
                };

                let active: recurring_templates::ActiveModel = (&template).into();
                active.insert(db_tx).await?;

                Ok(id)
            })
        })
        .await
    }

    /// Update an existing recurring template.
    pub async fn update_recurring(&self, cmd: UpdateRecurringCmd) -> ResultEngine<()> {
        if let Some(amount) = cmd.amount_minor
            && amount <= 0
        {
            return Err(EngineError::InvalidAmount("amount must be > 0".to_string()));
        }
        if let (Some(freq), Some(dop)) = (cmd.frequency, cmd.day_of_period) {
            validate_day_of_period(freq, dop)?;
        }

        let vault_id = cmd.vault_id.clone();
        let user_id = cmd.user_id.clone();
        self.with_tx(|engine, db_tx| {
            Box::pin(async move {
                engine
                    .require_vault_by_id_write(db_tx, &vault_id, &user_id)
                    .await?;

                let vault_uuid = parse_vault_uuid(&vault_id)?;
                let model = recurring_templates::Entity::find_by_id(cmd.template_id)
                    .filter(recurring_templates::Column::VaultId.eq(vault_uuid))
                    .filter(recurring_templates::Column::ArchivedAt.is_null())
                    .one(db_tx)
                    .await?
                    .ok_or_else(|| {
                        EngineError::KeyNotFound("recurring template not found".to_string())
                    })?;

                // If only one of frequency/day_of_period changed, validate against the other.
                let effective_freq = cmd.frequency.unwrap_or(model.frequency);
                let effective_dop = cmd.day_of_period.unwrap_or(model.day_of_period);
                if cmd.frequency.is_some() || cmd.day_of_period.is_some() {
                    validate_day_of_period(effective_freq, effective_dop)?;
                }

                let mut active = recurring_templates::ActiveModel {
                    id: ActiveValue::Set(cmd.template_id),
                    ..Default::default()
                };

                if let Some(v) = cmd.amount_minor {
                    active.amount_minor = ActiveValue::Set(v);
                }
                if let Some(v) = cmd.wallet_id {
                    active.wallet_id = ActiveValue::Set(Some(v));
                }
                if let Some(v) = cmd.flow_id {
                    active.flow_id = ActiveValue::Set(Some(v));
                }
                if cmd.category_id.is_some() || cmd.category.is_some() {
                    let cat = engine
                        .resolve_category_input(
                            db_tx,
                            &vault_id,
                            cmd.category_id,
                            cmd.category.as_deref(),
                        )
                        .await?;
                    active.category_id = ActiveValue::Set(cat.id);
                }
                if let Some(v) = cmd.note {
                    active.note = ActiveValue::Set(Some(v));
                }
                if let Some(v) = cmd.frequency {
                    active.frequency = ActiveValue::Set(v);
                }
                if let Some(v) = cmd.day_of_period {
                    active.day_of_period = ActiveValue::Set(v);
                }
                if let Some(v) = cmd.end_date {
                    active.end_date = ActiveValue::Set(v.map(|d| d.format("%Y-%m-%d").to_string()));
                }
                if let Some(v) = cmd.enabled {
                    active.enabled = ActiveValue::Set(v);
                }

                active.update(db_tx).await?;
                Ok(())
            })
        })
        .await
    }

    /// Soft-delete a recurring template.
    pub async fn archive_recurring(
        &self,
        vault_id: &str,
        template_id: Uuid,
        user_id: &str,
    ) -> ResultEngine<()> {
        let vault_id = vault_id.to_string();
        let user_id = user_id.to_string();
        self.with_tx(|engine, db_tx| {
            Box::pin(async move {
                engine
                    .require_vault_by_id_write(db_tx, &vault_id, &user_id)
                    .await?;

                let vault_uuid = parse_vault_uuid(&vault_id)?;
                let _model = recurring_templates::Entity::find_by_id(template_id)
                    .filter(recurring_templates::Column::VaultId.eq(vault_uuid))
                    .filter(recurring_templates::Column::ArchivedAt.is_null())
                    .one(db_tx)
                    .await?
                    .ok_or_else(|| {
                        EngineError::KeyNotFound("recurring template not found".to_string())
                    })?;

                let now = Utc::now().to_rfc3339();
                let active = recurring_templates::ActiveModel {
                    id: ActiveValue::Set(template_id),
                    archived_at: ActiveValue::Set(Some(now)),
                    ..Default::default()
                };
                active.update(db_tx).await?;
                Ok(())
            })
        })
        .await
    }

    /// List recurring templates for a vault.
    pub async fn list_recurring(
        &self,
        vault_id: &str,
        user_id: &str,
        include_archived: bool,
    ) -> ResultEngine<Vec<RecurringTemplate>> {
        let vault_id = vault_id.to_string();
        let user_id = user_id.to_string();
        self.with_tx(|engine, db_tx| {
            Box::pin(async move {
                engine
                    .require_vault_by_id_write(db_tx, &vault_id, &user_id)
                    .await?;

                let vault_uuid = parse_vault_uuid(&vault_id)?;
                let mut query = recurring_templates::Entity::find()
                    .filter(recurring_templates::Column::VaultId.eq(vault_uuid));
                if !include_archived {
                    query = query.filter(recurring_templates::Column::ArchivedAt.is_null());
                }
                let models = query.all(db_tx).await?;

                let mut templates = Vec::with_capacity(models.len());
                for model in models {
                    templates.push(RecurringTemplate::try_from(model)?);
                }
                Ok(templates)
            })
        })
        .await
    }

    /// Get a single recurring template.
    pub async fn get_recurring(
        &self,
        vault_id: &str,
        template_id: Uuid,
        user_id: &str,
    ) -> ResultEngine<RecurringTemplate> {
        let vault_id = vault_id.to_string();
        let user_id = user_id.to_string();
        self.with_tx(|engine, db_tx| {
            Box::pin(async move {
                engine
                    .require_vault_by_id_write(db_tx, &vault_id, &user_id)
                    .await?;

                let vault_uuid = parse_vault_uuid(&vault_id)?;
                let model = recurring_templates::Entity::find_by_id(template_id)
                    .filter(recurring_templates::Column::VaultId.eq(vault_uuid))
                    .one(db_tx)
                    .await?
                    .ok_or_else(|| {
                        EngineError::KeyNotFound("recurring template not found".to_string())
                    })?;

                RecurringTemplate::try_from(model)
            })
        })
        .await
    }

    /// List templates that are due for execution.
    pub async fn list_pending_recurring(
        &self,
        vault_id: &str,
        user_id: &str,
        as_of_date: NaiveDate,
    ) -> ResultEngine<Vec<PendingRecurring>> {
        let vault_id = vault_id.to_string();
        let user_id = user_id.to_string();
        self.with_tx(|engine, db_tx| {
            Box::pin(async move {
                engine
                    .require_vault_by_id_write(db_tx, &vault_id, &user_id)
                    .await?;

                let vault_uuid = parse_vault_uuid(&vault_id)?;
                let models = recurring_templates::Entity::find()
                    .filter(recurring_templates::Column::VaultId.eq(vault_uuid))
                    .filter(recurring_templates::Column::Enabled.eq(true))
                    .filter(recurring_templates::Column::ArchivedAt.is_null())
                    .all(db_tx)
                    .await?;

                let as_of_str = as_of_date.format("%Y-%m-%d").to_string();
                let mut pending = Vec::new();
                for model in models {
                    if model.start_date > as_of_str {
                        continue;
                    }
                    if let Some(ref end) = model.end_date
                        && end.as_str() < as_of_str.as_str()
                    {
                        continue;
                    }

                    let template = RecurringTemplate::try_from(model)?;
                    let period_date = compute_current_period_date(
                        template.frequency,
                        template.day_of_period,
                        as_of_date,
                    );

                    let is_due = match template.last_executed_date {
                        None => true,
                        Some(last) => last < period_date,
                    };

                    if is_due {
                        pending.push(PendingRecurring {
                            template,
                            period_date,
                        });
                    }
                }

                Ok(pending)
            })
        })
        .await
    }

    /// Execute a pending recurring template (user-approved).
    ///
    /// Creates the actual income/expense transaction and updates
    /// `last_executed_date`.
    pub async fn execute_recurring(
        &self,
        vault_id: &str,
        template_id: Uuid,
        user_id: &str,
        as_of_date: NaiveDate,
    ) -> ResultEngine<Uuid> {
        let vault_id = vault_id.to_string();
        let user_id = user_id.to_string();
        self.with_tx(|engine, db_tx| {
            Box::pin(async move {
                engine
                    .require_vault_by_id_write(db_tx, &vault_id, &user_id)
                    .await?;

                let vault_uuid = parse_vault_uuid(&vault_id)?;
                let model = recurring_templates::Entity::find_by_id(template_id)
                    .filter(recurring_templates::Column::VaultId.eq(vault_uuid))
                    .filter(recurring_templates::Column::ArchivedAt.is_null())
                    .filter(recurring_templates::Column::Enabled.eq(true))
                    .one(db_tx)
                    .await?
                    .ok_or_else(|| {
                        EngineError::KeyNotFound("recurring template not found".to_string())
                    })?;

                let template = RecurringTemplate::try_from(model)?;

                let period_date = compute_current_period_date(
                    template.frequency,
                    template.day_of_period,
                    as_of_date,
                );

                if let Some(last) = template.last_executed_date
                    && last >= period_date
                {
                    return Err(EngineError::InvalidRecurring(
                        "template already executed for this period".to_string(),
                    ));
                }

                let midnight = NaiveTime::from_hms_opt(0, 0, 0).ok_or_else(|| {
                    EngineError::InvalidRecurring("failed to build midnight time".to_string())
                })?;
                let occurred_at = Utc.from_utc_datetime(&period_date.and_time(midnight));

                let idempotency_key =
                    format!("recurring:{}:{}", template_id, period_date.format("%Y%m%d"));

                let mut meta = TxMeta::new(occurred_at)
                    .category_id(template.category_id)
                    .idempotency_key(idempotency_key);
                if let Some(ref note) = template.note {
                    meta = meta.note(note.clone());
                }

                let cmd = FlowWalletCmd {
                    vault_id: vault_id.clone(),
                    amount_minor: template.amount_minor,
                    flow_id: template.flow_id,
                    wallet_id: template.wallet_id,
                    meta,
                    user_id: user_id.clone(),
                    kind: template.kind,
                };

                let tx_id = engine
                    .create_flow_wallet_transaction_in_tx(db_tx, cmd)
                    .await?;

                let active = recurring_templates::ActiveModel {
                    id: ActiveValue::Set(template_id),
                    last_executed_date: ActiveValue::Set(Some(
                        period_date.format("%Y-%m-%d").to_string(),
                    )),
                    ..Default::default()
                };
                active.update(db_tx).await?;

                Ok(tx_id)
            })
        })
        .await
    }
}
