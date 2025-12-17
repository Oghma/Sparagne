use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[derive(Iden)]
enum Transactions {
    Table,
    VaultId,
    CreatedBy,
    IdempotencyKey,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Transactions::Table)
                    .add_column(ColumnDef::new(Transactions::IdempotencyKey).string())
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("uidx-transactions-vault_id-created_by-idempotency_key")
                    .table(Transactions::Table)
                    .col(Transactions::VaultId)
                    .col(Transactions::CreatedBy)
                    .col(Transactions::IdempotencyKey)
                    .unique()
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                Index::drop()
                    .name("uidx-transactions-vault_id-created_by-idempotency_key")
                    .table(Transactions::Table)
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Transactions::Table)
                    .drop_column(Transactions::IdempotencyKey)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }
}
