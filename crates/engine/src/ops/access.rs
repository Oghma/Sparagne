use sea_orm::{DatabaseTransaction, QueryFilter, prelude::*, sea_query::Expr};
use uuid::Uuid;

use crate::{
    EngineError, ResultEngine, cash_flows, flow_memberships, users, vault, vault_memberships,
    wallets,
};

use super::{Engine, normalize_required_name};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum MembershipRole {
    Owner,
    Editor,
    Viewer,
}

impl MembershipRole {
    pub(super) fn can_write(self) -> bool {
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
            other => Err(EngineError::InvalidRole(format!(
                "invalid membership role: {other}"
            ))),
        }
    }
}

/// Generates `_exists_in_vault` and `require_in_vault` methods for a target
/// entity.
macro_rules! impl_target_in_vault {
    ($exists_fn:ident, $require_fn:ident, $entity:path, $vault_col:expr, $err_msg:literal) => {
        async fn $exists_fn(
            &self,
            db: &DatabaseTransaction,
            vault_id: &str,
            target_id: Uuid,
        ) -> ResultEngine<bool> {
            <$entity>::find_by_id(target_id.to_string())
                .filter($vault_col.eq(vault_id.to_string()))
                .one(db)
                .await
                .map(|model| model.is_some())
                .map_err(Into::into)
        }

        pub(super) async fn $require_fn(
            &self,
            db: &DatabaseTransaction,
            vault_id: &str,
            target_id: Uuid,
        ) -> ResultEngine<()> {
            if !self.$exists_fn(db, vault_id, target_id).await? {
                return Err(EngineError::KeyNotFound($err_msg.to_string()));
            }
            Ok(())
        }
    };
}

impl Engine {
    impl_target_in_vault!(
        flow_exists_in_vault,
        require_flow_in_vault,
        cash_flows::Entity,
        cash_flows::Column::VaultId,
        "cash_flow not exists"
    );

    impl_target_in_vault!(
        wallet_exists_in_vault,
        require_wallet_in_vault,
        wallets::Entity,
        wallets::Column::VaultId,
        "wallet not exists"
    );

    async fn find_vault_by_id(
        &self,
        db: &DatabaseTransaction,
        vault_id: &str,
    ) -> ResultEngine<Option<vault::Model>> {
        vault::Entity::find_by_id(vault_id.to_string())
            .one(db)
            .await
            .map_err(Into::into)
    }

    pub(super) async fn vault_membership_role(
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

    pub(super) async fn require_vault_by_id_write(
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

    pub(super) async fn require_vault_owner(
        &self,
        db: &DatabaseTransaction,
        vault_id: &str,
        user_id: &str,
    ) -> ResultEngine<vault::Model> {
        let model = self
            .find_vault_by_id(db, vault_id)
            .await?
            .ok_or_else(|| EngineError::KeyNotFound("vault not exists".to_string()))?;
        if model.user_id != user_id {
            return Err(EngineError::KeyNotFound("vault not exists".to_string()));
        }
        Ok(model)
    }

    pub(super) async fn require_user_exists(
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

    pub(super) async fn flow_membership_role(
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

    pub(super) async fn has_vault_read_access(
        &self,
        db: &DatabaseTransaction,
        vault_id: &str,
        user_id: &str,
    ) -> ResultEngine<bool> {
        let Some(vault) = self.find_vault_by_id(db, vault_id).await? else {
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

    pub(super) async fn has_vault_write_access(
        &self,
        db: &DatabaseTransaction,
        vault_id: &str,
        user_id: &str,
    ) -> ResultEngine<bool> {
        let Some(vault) = self.find_vault_by_id(db, vault_id).await? else {
            return Ok(false);
        };
        if vault.user_id == user_id {
            return Ok(true);
        }
        let role = self.vault_membership_role(db, vault_id, user_id).await?;
        Ok(role.is_some_and(|r| r.can_write()))
    }

    pub(super) async fn require_flow_read(
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

    pub(super) async fn require_flow_write(
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

    pub(super) async fn require_vault_by_id(
        &self,
        db: &DatabaseTransaction,
        vault_id: &str,
        user_id: &str,
    ) -> ResultEngine<vault::Model> {
        let model = self
            .find_vault_by_id(db, vault_id)
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

    pub(super) async fn require_vault_by_name(
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

    pub(super) async fn unallocated_flow_id(
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
            .map_err(|_| EngineError::InvalidId("invalid cash_flow id".to_string()))
    }

    pub(super) async fn resolve_flow_id(
        &self,
        db: &DatabaseTransaction,
        vault_id: &str,
        flow_id: Option<Uuid>,
    ) -> ResultEngine<Uuid> {
        if let Some(id) = flow_id {
            // Ensure it exists and belongs to the vault.
            self.require_flow_in_vault(db, vault_id, id).await?;
            return Ok(id);
        }
        self.unallocated_flow_id(db, vault_id).await
    }

    pub(super) async fn resolve_wallet_id(
        &self,
        db: &DatabaseTransaction,
        vault_id: &str,
        wallet_id: Option<Uuid>,
    ) -> ResultEngine<Uuid> {
        if let Some(id) = wallet_id {
            self.require_wallet_in_vault(db, vault_id, id).await?;
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
            .map_err(|_| EngineError::InvalidId("invalid wallet id".to_string()))
    }
}
