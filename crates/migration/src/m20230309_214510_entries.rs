use sea_orm_migration::prelude::*;

use super::{m20230309_180650_cash_flows::CashFlows, m20230528_204409_wallets::Wallets};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Entries::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Entries::Id)
                            .string()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Entries::Amount).big_integer().not_null())
                    .col(ColumnDef::new(Entries::Note).string())
                    .col(ColumnDef::new(Entries::Category).string())
                    .col(ColumnDef::new(Entries::Date).timestamp().not_null())
                    .col(ColumnDef::new(Entries::CashFlowId).string())
                    .col(ColumnDef::new(Entries::WalletId).string())
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-entries-cashflow_id")
                            .from(Entries::Table, Entries::CashFlowId)
                            .to(CashFlows::Table, CashFlows::Name),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-entries-wallet_id")
                            .from(Entries::Table, Entries::WalletId)
                            .to(Wallets::Table, Wallets::Name),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Entries::Table).to_owned())
            .await
    }
}

/// Learn more at https://docs.rs/sea-query#iden
#[derive(Iden)]
enum Entries {
    Table,
    Id,
    Amount,
    Note,
    Category,
    Date,
    CashFlowId,
    WalletId,
}
