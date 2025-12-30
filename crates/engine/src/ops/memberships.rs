use uuid::Uuid;

use sea_orm::{ActiveValue, QueryFilter, prelude::*};

use crate::{EngineError, ResultEngine, cash_flows, flow_memberships, vault_memberships};

use super::{Engine, access::MembershipRole, parse_vault_uuid};

impl Engine {
    /// Adds or updates a vault member (owner-only).
    pub async fn upsert_vault_member(
        &self,
        vault_id: &str,
        member_username: &str,
        role: &str,
        user_id: &str,
    ) -> ResultEngine<()> {
        let vault_id = vault_id.to_string();
        let member_username = member_username.to_string();
        let role = role.to_string();
        let user_id = user_id.to_string();
        self.with_tx(|engine, db_tx| {
            Box::pin(async move {
                engine
                    .require_vault_owner(db_tx, vault_id.as_str(), user_id.as_str())
                    .await?;
                engine
                    .require_user_exists(db_tx, member_username.as_str())
                    .await?;

                let _role = MembershipRole::try_from(role.as_str())?;
                let vault_uuid = parse_vault_uuid(vault_id.as_str())?;

                let active = vault_memberships::ActiveModel {
                    vault_id: ActiveValue::Set(vault_uuid),
                    user_id: ActiveValue::Set(member_username.clone()),
                    role: ActiveValue::Set(role.clone()),
                };

                // Upsert: insert if missing, otherwise update role.
                match vault_memberships::Entity::find_by_id((vault_uuid, member_username.clone()))
                    .one(db_tx)
                    .await?
                {
                    Some(_) => {
                        active.update(db_tx).await?;
                    }
                    None => {
                        active.insert(db_tx).await?;
                    }
                }

                Ok(())
            })
        })
        .await
    }

    /// Removes a vault member (owner-only).
    pub async fn remove_vault_member(
        &self,
        vault_id: &str,
        member_username: &str,
        user_id: &str,
    ) -> ResultEngine<()> {
        let vault_id = vault_id.to_string();
        let member_username = member_username.to_string();
        let user_id = user_id.to_string();
        self.with_tx(|engine, db_tx| {
            Box::pin(async move {
                let vault = engine
                    .require_vault_owner(db_tx, vault_id.as_str(), user_id.as_str())
                    .await?;
                if member_username == vault.user_id {
                    return Err(EngineError::InvalidAmount(
                        "cannot remove vault owner".to_string(),
                    ));
                }

                let vault_uuid = parse_vault_uuid(vault_id.as_str())?;
                vault_memberships::Entity::delete_by_id((vault_uuid, member_username.clone()))
                    .exec(db_tx)
                    .await?;

                Ok(())
            })
        })
        .await
    }

    /// Lists vault members (owner-only).
    pub async fn list_vault_members(
        &self,
        vault_id: &str,
        user_id: &str,
    ) -> ResultEngine<Vec<(String, String)>> {
        let vault_id = vault_id.to_string();
        let user_id = user_id.to_string();
        self.with_tx(|engine, db_tx| {
            Box::pin(async move {
                engine
                    .require_vault_owner(db_tx, vault_id.as_str(), user_id.as_str())
                    .await?;

                let vault_uuid = parse_vault_uuid(vault_id.as_str())?;
                let rows = vault_memberships::Entity::find()
                    .filter(vault_memberships::Column::VaultId.eq(vault_uuid))
                    .all(db_tx)
                    .await?;
                Ok(rows.into_iter().map(|m| (m.user_id, m.role)).collect())
            })
        })
        .await
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
        let vault_id = vault_id.to_string();
        let member_username = member_username.to_string();
        let role = role.to_string();
        let user_id = user_id.to_string();
        self.with_tx(|engine, db_tx| {
            Box::pin(async move {
                engine
                    .require_vault_owner(db_tx, vault_id.as_str(), user_id.as_str())
                    .await?;
                engine
                    .require_user_exists(db_tx, member_username.as_str())
                    .await?;
                let _role = MembershipRole::try_from(role.as_str())?;

                let vault_uuid = parse_vault_uuid(vault_id.as_str())?;
                let flow = cash_flows::Entity::find_by_id(flow_id)
                    .filter(cash_flows::Column::VaultId.eq(vault_uuid))
                    .one(db_tx)
                    .await?
                    .ok_or_else(|| EngineError::KeyNotFound("cash_flow not exists".to_string()))?;
                if flow.system_kind == Some(cash_flows::SystemFlowKind::Unallocated) {
                    return Err(EngineError::InvalidFlow(
                        "cannot share Unallocated".to_string(),
                    ));
                }

                let active = flow_memberships::ActiveModel {
                    flow_id: ActiveValue::Set(flow_id),
                    user_id: ActiveValue::Set(member_username.clone()),
                    role: ActiveValue::Set(role.clone()),
                };

                match flow_memberships::Entity::find_by_id((flow_id, member_username.clone()))
                    .one(db_tx)
                    .await?
                {
                    Some(_) => {
                        active.update(db_tx).await?;
                    }
                    None => {
                        active.insert(db_tx).await?;
                    }
                }

                Ok(())
            })
        })
        .await
    }

    /// Removes a flow member (owner-only).
    pub async fn remove_flow_member(
        &self,
        vault_id: &str,
        flow_id: Uuid,
        member_username: &str,
        user_id: &str,
    ) -> ResultEngine<()> {
        let vault_id = vault_id.to_string();
        let member_username = member_username.to_string();
        let user_id = user_id.to_string();
        self.with_tx(|engine, db_tx| {
            Box::pin(async move {
                engine
                    .require_vault_owner(db_tx, vault_id.as_str(), user_id.as_str())
                    .await?;

                // Ensure flow exists and belongs to vault.
                engine
                    .require_flow_read(db_tx, vault_id.as_str(), flow_id, user_id.as_str())
                    .await?;

                flow_memberships::Entity::delete_by_id((flow_id, member_username.clone()))
                    .exec(db_tx)
                    .await?;
                Ok(())
            })
        })
        .await
    }

    /// Lists flow members (owner-only).
    pub async fn list_flow_members(
        &self,
        vault_id: &str,
        flow_id: Uuid,
        user_id: &str,
    ) -> ResultEngine<Vec<(String, String)>> {
        let vault_id = vault_id.to_string();
        let user_id = user_id.to_string();
        self.with_tx(|engine, db_tx| {
            Box::pin(async move {
                engine
                    .require_vault_owner(db_tx, vault_id.as_str(), user_id.as_str())
                    .await?;
                engine
                    .require_flow_read(db_tx, vault_id.as_str(), flow_id, user_id.as_str())
                    .await?;

                let rows = flow_memberships::Entity::find()
                    .filter(flow_memberships::Column::FlowId.eq(flow_id))
                    .all(db_tx)
                    .await?;
                Ok(rows.into_iter().map(|m| (m.user_id, m.role)).collect())
            })
        })
        .await
    }
}
