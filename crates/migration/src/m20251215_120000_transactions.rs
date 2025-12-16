use sea_orm_migration::prelude::*;

use crate::m20230531_190127_vaults::Vaults;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[derive(Iden)]
enum Transactions {
    Table,
    Id,
    VaultId,
    Kind,
    OccurredAt,
    AmountMinor,
    Currency,
    Category,
    Note,
    CreatedBy,
    VoidedAt,
    VoidedBy,
    RefundedTransactionId,
}

#[derive(Iden)]
enum Legs {
    Table,
    Id,
    TransactionId,
    TargetKind,
    TargetId,
    AmountMinor,
    Currency,
    AttributedUserId,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Transactions::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Transactions::Id)
                            .string()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Transactions::VaultId).string().not_null())
                    .col(ColumnDef::new(Transactions::Kind).string().not_null())
                    .col(
                        ColumnDef::new(Transactions::OccurredAt)
                            .timestamp()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Transactions::AmountMinor)
                            .big_integer()
                            .not_null(),
                    )
                    .col(ColumnDef::new(Transactions::Currency).string().not_null())
                    .col(ColumnDef::new(Transactions::Category).string())
                    .col(ColumnDef::new(Transactions::Note).string())
                    .col(ColumnDef::new(Transactions::CreatedBy).string().not_null())
                    .col(ColumnDef::new(Transactions::VoidedAt).timestamp())
                    .col(ColumnDef::new(Transactions::VoidedBy).string())
                    .col(ColumnDef::new(Transactions::RefundedTransactionId).string())
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-transactions-vault_id")
                            .from(Transactions::Table, Transactions::VaultId)
                            .to(Vaults::Table, Vaults::Id),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx-transactions-vault_id-occurred_at")
                    .table(Transactions::Table)
                    .col(Transactions::VaultId)
                    .col(Transactions::OccurredAt)
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(Legs::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(Legs::Id).string().not_null().primary_key())
                    .col(ColumnDef::new(Legs::TransactionId).string().not_null())
                    .col(ColumnDef::new(Legs::TargetKind).string().not_null())
                    .col(ColumnDef::new(Legs::TargetId).string().not_null())
                    .col(ColumnDef::new(Legs::AmountMinor).big_integer().not_null())
                    .col(ColumnDef::new(Legs::Currency).string().not_null())
                    .col(ColumnDef::new(Legs::AttributedUserId).string())
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-legs-transaction_id")
                            .from(Legs::Table, Legs::TransactionId)
                            .to(Transactions::Table, Transactions::Id),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx-legs-transaction_id")
                    .table(Legs::Table)
                    .col(Legs::TransactionId)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx-legs-target")
                    .table(Legs::Table)
                    .col(Legs::TargetKind)
                    .col(Legs::TargetId)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Legs::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(Transactions::Table).to_owned())
            .await?;
        Ok(())
    }
}
