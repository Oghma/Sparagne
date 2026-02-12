use sea_orm::{
    DatabaseTransaction, JoinType, QueryFilter, QuerySelect, RelationTrait, prelude::*,
    sea_query::Expr,
};
use uuid::Uuid;

use crate::{
    EngineError, ResultEngine, cash_flows, flow_memberships, flow_references, users,
    util::normalize_required_name, vault, vault_memberships, wallets,
};

use super::{Engine, parse_vault_uuid};

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

    pub(super) fn is_owner(self) -> bool {
        matches!(self, Self::Owner)
    }

    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Owner => "owner",
            Self::Editor => "editor",
            Self::Viewer => "viewer",
        }
    }
}

/// Vault access level for authorization checks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AccessLevel {
    /// Read access: vault owner or any member (Owner/Editor/Viewer)
    Read,
    /// Write access: vault owner or Editor
    Write,
    /// Owner-only access
    Owner,
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
            let vault_uuid = parse_vault_uuid(vault_id)?;
            <$entity>::find_by_id(target_id)
                .filter($vault_col.eq(vault_uuid))
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
        let vault_uuid = parse_vault_uuid(vault_id)?;
        vault::Entity::find_by_id(vault_uuid)
            .one(db)
            .await
            .map_err(Into::into)
    }

    pub(super) async fn vault_membership_role(
        &self,
        db: &DatabaseTransaction,
        vault_id: Uuid,
        user_id: &str,
    ) -> ResultEngine<Option<MembershipRole>> {
        let row = vault_memberships::Entity::find_by_id((vault_id, user_id.to_string()))
            .one(db)
            .await?;
        row.as_ref()
            .map(|m| MembershipRole::try_from(m.role.as_str()))
            .transpose()
    }

    /// Core authorization helper: checks vault access for a given level.
    ///
    /// Returns the vault model if authorized, or KeyNotFound error otherwise.
    async fn check_vault_access(
        &self,
        db: &DatabaseTransaction,
        model: &vault::Model,
        user_id: &str,
        level: AccessLevel,
    ) -> ResultEngine<()> {
        // Vault owner always has full access
        if model.user_id == user_id {
            return Ok(());
        }

        // Check membership for non-owners
        let role = self
            .vault_membership_role(db, model.id, user_id)
            .await?
            .ok_or_else(|| EngineError::KeyNotFound(EngineError::VAULT_NOT_FOUND.to_string()))?;

        let authorized = match level {
            AccessLevel::Read => true, // Any member can read
            AccessLevel::Write => role.can_write(),
            AccessLevel::Owner => role.is_owner(),
        };

        if !authorized {
            return Err(EngineError::KeyNotFound(
                EngineError::VAULT_NOT_FOUND.to_string(),
            ));
        }

        Ok(())
    }

    pub(super) async fn require_vault_by_id_write(
        &self,
        db: &DatabaseTransaction,
        vault_id: &str,
        user_id: &str,
    ) -> ResultEngine<vault::Model> {
        let model = self
            .find_vault_by_id(db, vault_id)
            .await?
            .ok_or_else(|| EngineError::KeyNotFound(EngineError::VAULT_NOT_FOUND.to_string()))?;
        self.check_vault_access(db, &model, user_id, AccessLevel::Write)
            .await?;
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
            .ok_or_else(|| EngineError::KeyNotFound(EngineError::VAULT_NOT_FOUND.to_string()))?;
        self.check_vault_access(db, &model, user_id, AccessLevel::Owner)
            .await?;
        Ok(model)
    }

    pub(super) async fn require_user_exists(
        &self,
        db: &DatabaseTransaction,
        username: &str,
    ) -> ResultEngine<()> {
        if users::Entity::find_by_id(username.to_string())
            .one(db)
            .await?
            .is_none()
        {
            return Err(EngineError::KeyNotFound(
                EngineError::USER_NOT_FOUND.to_string(),
            ));
        }
        Ok(())
    }

    pub(super) async fn flow_membership_role(
        &self,
        db: &DatabaseTransaction,
        flow_id: Uuid,
        user_id: &str,
    ) -> ResultEngine<Option<MembershipRole>> {
        let row = flow_memberships::Entity::find_by_id((flow_id, user_id.to_string()))
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
            .vault_membership_role(db, vault.id, user_id)
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
        let role = self.vault_membership_role(db, vault.id, user_id).await?;
        Ok(role.is_some_and(|r| r.can_write()))
    }

    pub(super) async fn require_flow_read(
        &self,
        db: &DatabaseTransaction,
        vault_id: &str,
        flow_id: Uuid,
        user_id: &str,
    ) -> ResultEngine<cash_flows::Model> {
        let vault_uuid = parse_vault_uuid(vault_id)?;

        // Try to find flow directly in this vault
        let model = cash_flows::Entity::find_by_id(flow_id)
            .filter(cash_flows::Column::VaultId.eq(vault_uuid))
            .one(db)
            .await?;

        if let Some(model) = model {
            // Flow is direct in this vault - check vault access or flow membership
            if self.has_vault_read_access(db, vault_id, user_id).await? {
                return Ok(model);
            }
            self.flow_membership_role(db, model.id, user_id)
                .await?
                .ok_or_else(|| EngineError::KeyNotFound("cash_flow not exists".to_string()))?;
            return Ok(model);
        }

        // Flow not direct - check if accessed via flow_reference
        let flow_ref = flow_references::Entity::find()
            .filter(flow_references::Column::VaultId.eq(vault_uuid))
            .filter(flow_references::Column::TargetFlowId.eq(flow_id))
            .one(db)
            .await?;

        if flow_ref.is_none() {
            return Err(EngineError::KeyNotFound("cash_flow not exists".to_string()));
        }

        // Flow is referenced - load the actual flow and check membership
        let model = cash_flows::Entity::find_by_id(flow_id)
            .one(db)
            .await?
            .ok_or_else(|| EngineError::KeyNotFound("cash_flow not exists".to_string()))?;

        // For referenced flows, user must have flow_membership (cannot rely on vault access alone)
        self.flow_membership_role(db, model.id, user_id)
            .await?
            .ok_or_else(|| EngineError::KeyNotFound("cash_flow not exists".to_string()))?;

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
            .flow_membership_role(db, model.id, user_id)
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
            .ok_or_else(|| EngineError::KeyNotFound(EngineError::VAULT_NOT_FOUND.to_string()))?;
        self.check_vault_access(db, &model, user_id, AccessLevel::Read)
            .await?;
        Ok(model)
    }

    /// Checks if a user is the vault owner or has a vault membership.
    async fn is_owner_or_member(
        &self,
        db: &DatabaseTransaction,
        model: &vault::Model,
        user_id: &str,
    ) -> ResultEngine<bool> {
        if model.user_id == user_id {
            return Ok(true);
        }
        Ok(self
            .vault_membership_role(db, model.id, user_id)
            .await?
            .is_some())
    }

    pub(super) async fn require_vault_by_name(
        &self,
        db: &DatabaseTransaction,
        vault_name: &str,
        user_id: &str,
    ) -> ResultEngine<vault::Model> {
        let vault_name = normalize_required_name(vault_name, "vault")?;
        let owner_hint = parse_vault_name_owner(&vault_name);
        let vault_name_lower = vault_name.to_lowercase();
        let models: Vec<vault::Model> = vault::Entity::find()
            .filter(Expr::cust("LOWER(name)").eq(vault_name_lower))
            .all(db)
            .await?;

        let mut allowed = Vec::new();
        for model in models {
            if self.is_owner_or_member(db, &model, user_id).await? {
                allowed.push(model);
            }
        }

        if allowed.is_empty() {
            if let Some((base, owner)) = owner_hint.as_ref()
                && let Some(model) = vault::Entity::find()
                    .filter(Expr::cust("LOWER(name)").eq(base.to_lowercase()))
                    .filter(vault::Column::UserId.eq(owner.as_str()))
                    .one(db)
                    .await?
                && self.is_owner_or_member(db, &model, user_id).await?
            {
                return Ok(model);
            }
            return Err(EngineError::KeyNotFound(
                EngineError::VAULT_NOT_FOUND.to_string(),
            ));
        }

        if allowed.len() > 1 {
            if let Some(pos) = allowed.iter().position(|model| model.user_id == user_id) {
                return Ok(allowed.remove(pos));
            }
            if let Some((base, owner)) = owner_hint
                && let Some(model) = vault::Entity::find()
                    .filter(Expr::cust("LOWER(name)").eq(base.to_lowercase()))
                    .filter(vault::Column::UserId.eq(owner.as_str()))
                    .one(db)
                    .await?
                && self.is_owner_or_member(db, &model, user_id).await?
            {
                return Ok(model);
            }
            return Err(EngineError::InvalidAmount(
                "ambiguous vault name".to_string(),
            ));
        }

        Ok(allowed.remove(0))
    }

    pub(super) async fn has_flow_membership_in_vault(
        &self,
        db: &DatabaseTransaction,
        vault_id: Uuid,
        user_id: &str,
    ) -> ResultEngine<bool> {
        let count = flow_memberships::Entity::find()
            .filter(flow_memberships::Column::UserId.eq(user_id.to_string()))
            .join(
                JoinType::InnerJoin,
                flow_memberships::Relation::CashFlows.def(),
            )
            .filter(cash_flows::Column::VaultId.eq(vault_id))
            .count(db)
            .await?;
        Ok(count > 0)
    }

    /// Checks if a user has any access to a vault (owner, member, or flow
    /// member).
    async fn has_vault_or_flow_access(
        &self,
        db: &DatabaseTransaction,
        vault_id: Uuid,
        vault_owner_id: &str,
        user_id: &str,
    ) -> ResultEngine<bool> {
        Ok(vault_owner_id == user_id
            || self
                .vault_membership_role(db, vault_id, user_id)
                .await?
                .is_some()
            || self
                .has_flow_membership_in_vault(db, vault_id, user_id)
                .await?)
    }

    pub(super) async fn require_vault_header_by_id(
        &self,
        db: &DatabaseTransaction,
        vault_id: &str,
        user_id: &str,
    ) -> ResultEngine<vault::Model> {
        let model = self
            .find_vault_by_id(db, vault_id)
            .await?
            .ok_or_else(|| EngineError::KeyNotFound(EngineError::VAULT_NOT_FOUND.to_string()))?;
        let has_access = self
            .has_vault_or_flow_access(db, model.id, &model.user_id, user_id)
            .await?;
        if !has_access {
            return Err(EngineError::KeyNotFound(
                EngineError::VAULT_NOT_FOUND.to_string(),
            ));
        }
        Ok(model)
    }

    pub(super) async fn require_vault_header_by_name(
        &self,
        db: &DatabaseTransaction,
        vault_name: &str,
        user_id: &str,
    ) -> ResultEngine<vault::Model> {
        let vault_name = normalize_required_name(vault_name, "vault")?;
        let owner_hint = parse_vault_name_owner(&vault_name);
        let vault_name_lower = vault_name.to_lowercase();
        let models: Vec<vault::Model> = vault::Entity::find()
            .filter(Expr::cust("LOWER(name)").eq(vault_name_lower))
            .all(db)
            .await?;

        let mut allowed_vaults = Vec::new();
        for model in models {
            let has_access = self
                .has_vault_or_flow_access(db, model.id, &model.user_id, user_id)
                .await?;
            if has_access {
                allowed_vaults.push(model);
            }
        }

        if allowed_vaults.is_empty() {
            if let Some((base, owner)) = owner_hint
                && let Some(model) = self
                    .resolve_vault_by_name_owner(db, base.as_str(), owner.as_str(), user_id)
                    .await?
            {
                return Ok(model);
            }
            return Err(EngineError::KeyNotFound(
                EngineError::VAULT_NOT_FOUND.to_string(),
            ));
        }

        if allowed_vaults.len() > 1 {
            if let Some(pos) = allowed_vaults
                .iter()
                .position(|model| model.user_id == user_id)
            {
                return Ok(allowed_vaults.remove(pos));
            }
            if let Some((base, owner)) = owner_hint
                && let Some(model) = self
                    .resolve_vault_by_name_owner(db, base.as_str(), owner.as_str(), user_id)
                    .await?
            {
                return Ok(model);
            }
            return Err(EngineError::InvalidAmount(
                "ambiguous vault name".to_string(),
            ));
        }

        Ok(allowed_vaults.remove(0))
    }

    async fn resolve_vault_by_name_owner(
        &self,
        db: &DatabaseTransaction,
        vault_name: &str,
        owner: &str,
        user_id: &str,
    ) -> ResultEngine<Option<vault::Model>> {
        if vault_name.is_empty() || owner.is_empty() {
            return Ok(None);
        }
        let vault_name = normalize_required_name(vault_name, "vault")?;
        let vault_name_lower = vault_name.to_lowercase();
        let model = vault::Entity::find()
            .filter(Expr::cust("LOWER(name)").eq(vault_name_lower))
            .filter(vault::Column::UserId.eq(owner))
            .one(db)
            .await?;

        let Some(model) = model else {
            return Ok(None);
        };

        let allowed = self
            .has_vault_or_flow_access(db, model.id, &model.user_id, user_id)
            .await?;

        Ok(if allowed { Some(model) } else { None })
    }

    pub(super) async fn unallocated_flow_id(
        &self,
        db: &DatabaseTransaction,
        vault_id: &str,
    ) -> ResultEngine<Uuid> {
        let vault_uuid = parse_vault_uuid(vault_id)?;
        let model = cash_flows::Entity::find()
            .filter(cash_flows::Column::VaultId.eq(vault_uuid))
            .filter(
                cash_flows::Column::SystemKind.eq(Some(cash_flows::SystemFlowKind::Unallocated)),
            )
            .one(db)
            .await?
            .ok_or_else(|| EngineError::InvalidFlow("missing Unallocated flow".to_string()))?;
        Ok(model.id)
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

        let vault_uuid = parse_vault_uuid(vault_id)?;
        let wallet_models: Vec<wallets::Model> = wallets::Entity::find()
            .filter(wallets::Column::VaultId.eq(vault_uuid))
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
        Ok(first.id)
    }
}

fn parse_vault_name_owner(value: &str) -> Option<(String, String)> {
    let trimmed = value.trim();
    if !trimmed.ends_with(')') {
        return None;
    }
    let (base, owner) = trimmed.rsplit_once(" (")?;
    let owner = owner.trim_end_matches(')').trim();
    let base = base.trim();
    if base.is_empty() || owner.is_empty() {
        return None;
    }
    Some((base.to_string(), owner.to_string()))
}
