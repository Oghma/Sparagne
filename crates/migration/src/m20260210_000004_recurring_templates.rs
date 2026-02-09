//! Create `recurring_templates` table for recurring transaction templates.

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[derive(Iden)]
enum RecurringTemplates {
    Table,
    Id,
    VaultId,
    Kind,
    AmountMinor,
    WalletId,
    FlowId,
    CategoryId,
    Note,
    CreatedBy,
    Frequency,
    DayOfPeriod,
    StartDate,
    EndDate,
    Enabled,
    LastExecutedDate,
    CreatedAt,
    ArchivedAt,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(RecurringTemplates::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(RecurringTemplates::Id)
                            .blob()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(RecurringTemplates::VaultId)
                            .blob()
                            .not_null(),
                    )
                    .col(ColumnDef::new(RecurringTemplates::Kind).text().not_null())
                    .col(
                        ColumnDef::new(RecurringTemplates::AmountMinor)
                            .big_integer()
                            .not_null(),
                    )
                    .col(ColumnDef::new(RecurringTemplates::WalletId).blob())
                    .col(ColumnDef::new(RecurringTemplates::FlowId).blob())
                    .col(
                        ColumnDef::new(RecurringTemplates::CategoryId)
                            .blob()
                            .not_null(),
                    )
                    .col(ColumnDef::new(RecurringTemplates::Note).text())
                    .col(
                        ColumnDef::new(RecurringTemplates::CreatedBy)
                            .text()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(RecurringTemplates::Frequency)
                            .text()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(RecurringTemplates::DayOfPeriod)
                            .integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(RecurringTemplates::StartDate)
                            .text()
                            .not_null(),
                    )
                    .col(ColumnDef::new(RecurringTemplates::EndDate).text())
                    .col(
                        ColumnDef::new(RecurringTemplates::Enabled)
                            .boolean()
                            .not_null()
                            .default(true),
                    )
                    .col(ColumnDef::new(RecurringTemplates::LastExecutedDate).text())
                    .col(
                        ColumnDef::new(RecurringTemplates::CreatedAt)
                            .text()
                            .not_null(),
                    )
                    .col(ColumnDef::new(RecurringTemplates::ArchivedAt).text())
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(RecurringTemplates::Table).to_owned())
            .await
    }
}
