use sea_orm::{ConnectionTrait, DbBackend, DbErr, Statement};
use sea_orm_migration::prelude::*;
use uuid::Uuid;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[derive(Iden)]
enum CashFlows {
    Table,
    Id,
    Name,
    Balance,
    MaxBalance,
    IncomeBalance,
    Currency,
    Archived,
    VaultId,
    SystemKind,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        let backend = db.get_database_backend();

        // 1) Add `system_kind` column to cash_flows.
        manager
            .alter_table(
                Table::alter()
                    .table(CashFlows::Table)
                    .add_column(ColumnDef::new(CashFlows::SystemKind).string())
                    .to_owned(),
            )
            .await?;

        // 2) Detect duplicates: more than one "unallocated" (case-insensitive) within
        //    the same vault.
        let duplicates = db
            .query_all(Statement::from_string(
                backend,
                "SELECT vault_id FROM cash_flows WHERE lower(name) = 'unallocated' GROUP BY vault_id HAVING COUNT(*) > 1;".to_string(),
            ))
            .await?;
        if let Some(row) = duplicates.first() {
            let vault_id: String = row.try_get("", "vault_id")?;
            return Err(DbErr::Custom(format!(
                "cannot migrate system flow: vault '{vault_id}' has multiple cash_flows named 'unallocated' (case-insensitive)"
            )));
        }

        // 3) Mark existing "unallocated" flows and normalize name.
        db.execute(Statement::from_string(
            backend,
            "UPDATE cash_flows SET system_kind = 'unallocated' WHERE lower(name) = 'unallocated';"
                .to_string(),
        ))
        .await?;
        db.execute(Statement::from_string(
            backend,
            "UPDATE cash_flows SET name = 'unallocated' WHERE system_kind = 'unallocated';"
                .to_string(),
        ))
        .await?;

        // 4) Ensure every vault has a system flow Unallocated.
        let vault_rows = db
            .query_all(Statement::from_string(
                backend,
                "SELECT id, currency FROM vaults;".to_string(),
            ))
            .await?;

        for row in vault_rows {
            let vault_id: String = row.try_get("", "id")?;
            let currency: String = row.try_get("", "currency")?;

            let exists = db
                .query_one(Statement::from_string(
                    backend,
                    format!(
                        "SELECT 1 FROM cash_flows WHERE vault_id = '{}' AND system_kind = 'unallocated' LIMIT 1;",
                        vault_id.replace('\'', "''")
                    ),
                ))
                .await?
                .is_some();

            if exists {
                continue;
            }

            let id = Uuid::new_v4().to_string();
            let stmt = Query::insert()
                .into_table(CashFlows::Table)
                .columns([
                    CashFlows::Id,
                    CashFlows::Name,
                    CashFlows::Balance,
                    CashFlows::MaxBalance,
                    CashFlows::IncomeBalance,
                    CashFlows::Currency,
                    CashFlows::Archived,
                    CashFlows::VaultId,
                    CashFlows::SystemKind,
                ])
                .values_panic([
                    id.into(),
                    "unallocated".into(),
                    0i64.into(),
                    None::<i64>.into(),
                    None::<i64>.into(),
                    currency.into(),
                    false.into(),
                    vault_id.into(),
                    "unallocated".into(),
                ])
                .to_owned();

            db.execute(backend.build(&stmt)).await?;
        }

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Safe-ish rollback: drop the column (SQLite requires table rebuild, so only
        // implement for backends that support it).
        let db = manager.get_connection();
        if db.get_database_backend() == DbBackend::Sqlite {
            return Err(DbErr::Custom(
                "m20251215_090000_system_flows is irreversible on SQLite".to_string(),
            ));
        }

        manager
            .alter_table(
                Table::alter()
                    .table(CashFlows::Table)
                    .drop_column(CashFlows::SystemKind)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }
}
