use uuid::Uuid;

use sea_orm::{ActiveValue, QueryFilter, TransactionTrait, prelude::*};

use crate::{EngineError, ResultEngine, cash_flows, flow_memberships, vault_memberships};

use super::{Engine, access::MembershipRole, with_tx};

impl Engine {
    /// Adds or updates a vault member (owner-only).
    pub async fn upsert_vault_member(
        &self,
        vault_id: &str,
        member_username: &str,
        role: &str,
        user_id: &str,
    ) -> ResultEngine<()> {
        with_tx!(self, |db_tx| {
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

            Ok(())
        })
    }

    /// Removes a vault member (owner-only).
    pub async fn remove_vault_member(
        &self,
        vault_id: &str,
        member_username: &str,
        user_id: &str,
    ) -> ResultEngine<()> {
        with_tx!(self, |db_tx| {
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

            Ok(())
        })
    }

    /// Lists vault members (owner-only).
    pub async fn list_vault_members(
        &self,
        vault_id: &str,
        user_id: &str,
    ) -> ResultEngine<Vec<(String, String)>> {
        with_tx!(self, |db_tx| {
            self.require_vault_owner(&db_tx, vault_id, user_id).await?;

            let rows = vault_memberships::Entity::find()
                .filter(vault_memberships::Column::VaultId.eq(vault_id.to_string()))
                .all(&db_tx)
                .await?;
            Ok(rows.into_iter().map(|m| (m.user_id, m.role)).collect())
        })
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
        with_tx!(self, |db_tx| {
            self.require_vault_owner(&db_tx, vault_id, user_id).await?;
            self.require_user_exists(&db_tx, member_username).await?;
            let _role = MembershipRole::try_from(role)?;

            let flow = cash_flows::Entity::find_by_id(flow_id.to_string())
                .filter(cash_flows::Column::VaultId.eq(vault_id.to_string()))
                .one(&db_tx)
                .await?
                .ok_or_else(|| EngineError::KeyNotFound("cash_flow not exists".to_string()))?;
            if flow.system_kind == Some(cash_flows::SystemFlowKind::Unallocated) {
                return Err(EngineError::InvalidFlow(
                    "cannot share Unallocated".to_string(),
                ));
            }

            let flow_id_str = flow_id.to_string();
            let active = flow_memberships::ActiveModel {
                flow_id: ActiveValue::Set(flow_id_str.clone()),
                user_id: ActiveValue::Set(member_username.to_string()),
                role: ActiveValue::Set(role.to_string()),
            };

            match flow_memberships::Entity::find_by_id((
                flow_id_str.clone(),
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

            Ok(())
        })
    }

    /// Removes a flow member (owner-only).
    pub async fn remove_flow_member(
        &self,
        vault_id: &str,
        flow_id: Uuid,
        member_username: &str,
        user_id: &str,
    ) -> ResultEngine<()> {
        with_tx!(self, |db_tx| {
            self.require_vault_owner(&db_tx, vault_id, user_id).await?;

            // Ensure flow exists and belongs to vault.
            self.require_flow_read(&db_tx, vault_id, flow_id, user_id)
                .await?;

            flow_memberships::Entity::delete_by_id((
                flow_id.to_string(),
                member_username.to_string(),
            ))
            .exec(&db_tx)
            .await?;
            Ok(())
        })
    }

    /// Lists flow members (owner-only).
    pub async fn list_flow_members(
        &self,
        vault_id: &str,
        flow_id: Uuid,
        user_id: &str,
    ) -> ResultEngine<Vec<(String, String)>> {
        with_tx!(self, |db_tx| {
            self.require_vault_owner(&db_tx, vault_id, user_id).await?;
            self.require_flow_read(&db_tx, vault_id, flow_id, user_id)
                .await?;

            let rows = flow_memberships::Entity::find()
                .filter(flow_memberships::Column::FlowId.eq(flow_id.to_string()))
                .all(&db_tx)
                .await?;
            Ok(rows.into_iter().map(|m| (m.user_id, m.role)).collect())
        })
    }
}
