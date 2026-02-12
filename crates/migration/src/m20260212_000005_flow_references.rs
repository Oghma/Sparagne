//! Create `flow_references` table for cross-vault flow sharing.
//!
//! This migration enables flows to appear in multiple vaults via virtual references,
//! supporting the cross-vault sharing use case (e.g., family budgets).

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[derive(Iden)]
enum FlowReferences {
    Table,
    Id,
    VaultId,
    TargetFlowId,
    DisplayName,
    CreatedAt,
}

#[derive(Iden)]
enum Vaults {
    Table,
    Id,
}

#[derive(Iden)]
enum CashFlows {
    Table,
    Id,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Create flow_references table
        manager
            .create_table(
                Table::create()
                    .table(FlowReferences::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(FlowReferences::Id)
                            .blob()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(FlowReferences::VaultId)
                            .blob()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(FlowReferences::TargetFlowId)
                            .blob()
                            .not_null(),
                    )
                    .col(ColumnDef::new(FlowReferences::DisplayName).text())
                    .col(
                        ColumnDef::new(FlowReferences::CreatedAt)
                            .text()
                            .not_null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_flow_references_vault_id")
                            .from(FlowReferences::Table, FlowReferences::VaultId)
                            .to(Vaults::Table, Vaults::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_flow_references_target_flow_id")
                            .from(FlowReferences::Table, FlowReferences::TargetFlowId)
                            .to(CashFlows::Table, CashFlows::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        // Create unique constraint on (vault_id, target_flow_id)
        manager
            .create_index(
                Index::create()
                    .name("idx_flow_references_vault_target_unique")
                    .table(FlowReferences::Table)
                    .col(FlowReferences::VaultId)
                    .col(FlowReferences::TargetFlowId)
                    .unique()
                    .to_owned(),
            )
            .await?;

        // Create index on vault_id for fast lookups
        manager
            .create_index(
                Index::create()
                    .name("idx_flow_references_vault_id")
                    .table(FlowReferences::Table)
                    .col(FlowReferences::VaultId)
                    .to_owned(),
            )
            .await?;

        // Create index on target_flow_id for reverse lookups
        manager
            .create_index(
                Index::create()
                    .name("idx_flow_references_target_flow_id")
                    .table(FlowReferences::Table)
                    .col(FlowReferences::TargetFlowId)
                    .to_owned(),
            )
            .await?;

        // Note: Existing flow_memberships will be migrated to flow_references
        // via a separate data migration script or manual process.
        // This ensures the table structure is in place first.

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(FlowReferences::Table).to_owned())
            .await
    }
}
