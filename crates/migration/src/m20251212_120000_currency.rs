use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Vaults::Table)
                    .add_column(
                        ColumnDef::new(Vaults::Currency)
                            .string()
                            .not_null()
                            .default("EUR"),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Wallets::Table)
                    .add_column(
                        ColumnDef::new(Wallets::Currency)
                            .string()
                            .not_null()
                            .default("EUR"),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(CashFlows::Table)
                    .add_column(
                        ColumnDef::new(CashFlows::Currency)
                            .string()
                            .not_null()
                            .default("EUR"),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Entries::Table)
                    .add_column(
                        ColumnDef::new(Entries::Currency)
                            .string()
                            .not_null()
                            .default("EUR"),
                    )
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Entries::Table)
                    .drop_column(Entries::Currency)
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(CashFlows::Table)
                    .drop_column(CashFlows::Currency)
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Wallets::Table)
                    .drop_column(Wallets::Currency)
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Vaults::Table)
                    .drop_column(Vaults::Currency)
                    .to_owned(),
            )
            .await?;
        Ok(())
    }
}

#[derive(Iden)]
enum Vaults {
    Table,
    Currency,
}

#[derive(Iden)]
enum Wallets {
    Table,
    Currency,
}

#[derive(Iden)]
enum CashFlows {
    Table,
    Currency,
}

#[derive(Iden)]
enum Entries {
    Table,
    Currency,
}
