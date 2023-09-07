use sea_orm_migration::prelude::*;

use super::m20230531_190127_vaults::Vaults;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(CashFlows::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(CashFlows::Name)
                            .string()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(CashFlows::Balance).double().not_null())
                    .col(ColumnDef::new(CashFlows::MaxBalance).double())
                    .col(ColumnDef::new(CashFlows::IncomeBalance).double())
                    .col(ColumnDef::new(CashFlows::Archived).boolean().not_null())
                    .col(ColumnDef::new(CashFlows::VaultId).uuid())
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-cashflows-vault_id")
                            .from(CashFlows::Table, CashFlows::VaultId)
                            .to(Vaults::Table, Vaults::Id),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(CashFlows::Table).to_owned())
            .await
    }
}

/// Learn more at https://docs.rs/sea-query#iden
#[derive(Iden)]
pub enum CashFlows {
    Table,
    Name,
    Balance,
    MaxBalance,
    IncomeBalance,
    Archived,
    VaultId,
}
