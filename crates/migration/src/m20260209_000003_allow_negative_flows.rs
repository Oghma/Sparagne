//! Add `allow_negative` boolean column to `cash_flows`.

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[derive(Iden)]
enum CashFlows {
    Table,
    AllowNegative,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(CashFlows::Table)
                    .add_column(
                        ColumnDef::new(CashFlows::AllowNegative)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(CashFlows::Table)
                    .drop_column(CashFlows::AllowNegative)
                    .to_owned(),
            )
            .await
    }
}
