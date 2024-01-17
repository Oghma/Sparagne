use sea_orm_migration::prelude::*;

use crate::m20230531_190127_vaults::Vaults;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Wallets::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Wallets::Name)
                            .string()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Wallets::Balance).double().not_null())
                    .col(ColumnDef::new(Wallets::Archived).boolean().not_null())
                    .col(ColumnDef::new(Wallets::VaultId).string())
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-wallets-vault_id")
                            .from(Wallets::Table, Wallets::VaultId)
                            .to(Vaults::Table, Vaults::Id),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Wallets::Table).to_owned())
            .await
    }
}

/// Learn more at https://docs.rs/sea-query#iden
#[derive(Iden)]
pub enum Wallets {
    Table,
    Name,
    Balance,
    Archived,
    VaultId,
}
