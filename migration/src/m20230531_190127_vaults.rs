use sea_orm_migration::prelude::*;

use super::m20230828_064600_users::Users;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Vaults::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(Vaults::Id).uuid().not_null().primary_key())
                    .col(ColumnDef::new(Vaults::Name).string().not_null())
                    .col(ColumnDef::new(Vaults::UserId).string().not_null())
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-vaults-users_id")
                            .from(Vaults::Table, Vaults::Id)
                            .to(Users::Table, Users::Username),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Replace the sample below with your own migration scripts
        manager
            .drop_table(Table::drop().table(Vaults::Table).to_owned())
            .await
    }
}

/// Learn more at https://docs.rs/sea-query#iden
#[derive(Iden)]
pub enum Vaults {
    Table,
    Id,
    Name,
    UserId,
}
