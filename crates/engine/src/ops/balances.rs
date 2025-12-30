use std::collections::HashMap;

use uuid::Uuid;

use sea_orm::{ActiveValue, JoinType, QueryFilter, QueryOrder, QuerySelect, prelude::*};

use crate::{
    CashFlow, EngineError, Leg, LegTarget, ResultEngine, Wallet, cash_flows, legs, transactions,
    util::ensure_vault_currency, wallets,
};

use super::{Engine, parse_vault_uuid};

impl Engine {
    /// Recomputes denormalized balances for wallets and flows from the ledger
    /// (`transactions` + `legs`).
    ///
    /// - Ignores voided transactions.
    /// - Validates flow invariants while replaying legs in chronological order.
    /// - Refreshes the in-memory vault state from DB models post-commit.
    pub async fn recompute_balances(&self, vault_id: &str, user_id: &str) -> ResultEngine<()> {
        let vault_id = vault_id.to_string();
        let user_id = user_id.to_string();
        self.with_tx(|engine, db_tx| {
            Box::pin(async move {
                let vault_model = engine
                    .require_vault_by_id_write(db_tx, vault_id.as_str(), user_id.as_str())
                    .await?;
                let currency = vault_model.currency;
                let vault_uuid = parse_vault_uuid(vault_id.as_str())?;

                // Load all wallets/flows from DB (including archived) to avoid stale RAM
                // issues.
                let wallet_models: Vec<wallets::Model> = wallets::Entity::find()
                    .filter(wallets::Column::VaultId.eq(vault_uuid))
                    .all(db_tx)
                    .await?;
                let flow_models: Vec<cash_flows::Model> = cash_flows::Entity::find()
                    .filter(cash_flows::Column::VaultId.eq(vault_uuid))
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
                    .filter(transactions::Column::VaultId.eq(vault_uuid))
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
                        id: ActiveValue::Set(*wallet_id),
                        balance: ActiveValue::Set(wallet.balance),
                        ..Default::default()
                    };
                    wallet_model.update(db_tx).await?;
                }

                for (flow_id, flow) in &flows {
                    let flow_model = cash_flows::ActiveModel {
                        id: ActiveValue::Set(*flow_id),
                        balance: ActiveValue::Set(flow.balance),
                        income_balance: ActiveValue::Set(flow.income_balance),
                        ..Default::default()
                    };
                    flow_model.update(db_tx).await?;
                }

                Ok(())
            })
        })
        .await
    }
}
