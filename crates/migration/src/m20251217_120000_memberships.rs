use sea_orm_migration::prelude::*;

use crate::{m20230531_190127_vaults::Vaults, m20230828_064600_users::Users};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[derive(Iden)]
enum VaultMemberships {
    Table,
    VaultId,
    UserId,
    Role,
}

#[derive(Iden)]
enum FlowMemberships {
    Table,
    FlowId,
    UserId,
    Role,
}

#[derive(Iden)]
enum CashFlows {
    Table,
    Id,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(VaultMemberships::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(VaultMemberships::VaultId).string().not_null())
                    .col(ColumnDef::new(VaultMemberships::UserId).string().not_null())
                    .col(ColumnDef::new(VaultMemberships::Role).string().not_null())
                    .primary_key(
                        Index::create()
                            .col(VaultMemberships::VaultId)
                            .col(VaultMemberships::UserId),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-vault_memberships-vault_id")
                            .from(VaultMemberships::Table, VaultMemberships::VaultId)
                            .to(Vaults::Table, Vaults::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-vault_memberships-user_id")
                            .from(VaultMemberships::Table, VaultMemberships::UserId)
                            .to(Users::Table, Users::Username)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx-vault_memberships-user_id")
                    .table(VaultMemberships::Table)
                    .col(VaultMemberships::UserId)
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(FlowMemberships::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(FlowMemberships::FlowId).string().not_null())
                    .col(ColumnDef::new(FlowMemberships::UserId).string().not_null())
                    .col(ColumnDef::new(FlowMemberships::Role).string().not_null())
                    .primary_key(
                        Index::create()
                            .col(FlowMemberships::FlowId)
                            .col(FlowMemberships::UserId),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-flow_memberships-flow_id")
                            .from(FlowMemberships::Table, FlowMemberships::FlowId)
                            .to(CashFlows::Table, CashFlows::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-flow_memberships-user_id")
                            .from(FlowMemberships::Table, FlowMemberships::UserId)
                            .to(Users::Table, Users::Username)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx-flow_memberships-user_id")
                    .table(FlowMemberships::Table)
                    .col(FlowMemberships::UserId)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(FlowMemberships::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(VaultMemberships::Table).to_owned())
            .await?;
        Ok(())
    }
}
