use sea_orm::{ConnectionTrait, DbBackend, DbErr, Statement, prelude::DateTimeUtc};
use sea_orm_migration::prelude::*;
use uuid::Uuid;

use crate::m20230531_190127_vaults::Vaults;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[derive(Iden)]
enum Wallets {
    Table,
    Id,
    Name,
    Balance,
    Currency,
    Archived,
    VaultId,
}

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
}

#[derive(Iden)]
enum Entries {
    Table,
    Id,
    Amount,
    Currency,
    Note,
    Category,
    Date,
    VaultId,
    CashFlowId,
    WalletId,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        let backend = db.get_database_backend();

        if backend == DbBackend::Sqlite {
            db.execute(Statement::from_string(
                backend,
                "PRAGMA foreign_keys=OFF;".to_string(),
            ))
            .await?;
        }

        // --- Wallets ---
        db.execute(Statement::from_string(
            backend,
            "ALTER TABLE wallets RENAME TO wallets_old;".to_string(),
        ))
        .await?;

        manager
            .create_table(
                Table::create()
                    .table(Wallets::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Wallets::Id)
                            .string()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Wallets::Name).string().not_null())
                    .col(ColumnDef::new(Wallets::Balance).big_integer().not_null())
                    .col(ColumnDef::new(Wallets::Currency).string().not_null())
                    .col(ColumnDef::new(Wallets::Archived).boolean().not_null())
                    .col(ColumnDef::new(Wallets::VaultId).string().not_null())
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-wallets-vault_id")
                            .from(Wallets::Table, Wallets::VaultId)
                            .to(Vaults::Table, Vaults::Id),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx-wallets-vault_id-name-unique")
                    .table(Wallets::Table)
                    .col(Wallets::VaultId)
                    .col(Wallets::Name)
                    .unique()
                    .to_owned(),
            )
            .await?;

        let wallet_rows = db
            .query_all(Statement::from_string(
                backend,
                "SELECT name, balance, currency, archived, vault_id FROM wallets_old;".to_string(),
            ))
            .await?;

        let mut wallet_by_name: std::collections::HashMap<String, (String, String)> =
            std::collections::HashMap::new();

        for row in wallet_rows {
            let name: String = row.try_get("", "name")?;
            let balance: i64 = row.try_get("", "balance")?;
            let currency: String = row.try_get("", "currency")?;
            let archived: bool = row.try_get("", "archived")?;
            let vault_id: String = row.try_get("", "vault_id")?;

            if wallet_by_name.contains_key(&name) {
                return Err(DbErr::Custom(format!(
                    "cannot migrate wallets: wallet name '{name}' exists in multiple vaults; this was previously ambiguous for entries"
                )));
            }

            let id = Uuid::new_v4().to_string();
            wallet_by_name.insert(name.clone(), (id.clone(), vault_id.clone()));

            let stmt = Query::insert()
                .into_table(Wallets::Table)
                .columns([
                    Wallets::Id,
                    Wallets::Name,
                    Wallets::Balance,
                    Wallets::Currency,
                    Wallets::Archived,
                    Wallets::VaultId,
                ])
                .values_panic([
                    id.into(),
                    name.into(),
                    balance.into(),
                    currency.into(),
                    archived.into(),
                    vault_id.into(),
                ])
                .to_owned();

            db.execute(backend.build(&stmt)).await?;
        }

        db.execute(Statement::from_string(
            backend,
            "DROP TABLE wallets_old;".to_string(),
        ))
        .await?;

        // --- Cash flows ---
        db.execute(Statement::from_string(
            backend,
            "ALTER TABLE cash_flows RENAME TO cash_flows_old;".to_string(),
        ))
        .await?;

        manager
            .create_table(
                Table::create()
                    .table(CashFlows::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(CashFlows::Id)
                            .string()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(CashFlows::Name).string().not_null())
                    .col(ColumnDef::new(CashFlows::Balance).big_integer().not_null())
                    .col(ColumnDef::new(CashFlows::MaxBalance).big_integer())
                    .col(ColumnDef::new(CashFlows::IncomeBalance).big_integer())
                    .col(ColumnDef::new(CashFlows::Currency).string().not_null())
                    .col(ColumnDef::new(CashFlows::Archived).boolean().not_null())
                    .col(ColumnDef::new(CashFlows::VaultId).string().not_null())
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-cashflows-vault_id")
                            .from(CashFlows::Table, CashFlows::VaultId)
                            .to(Vaults::Table, Vaults::Id),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx-cash_flows-vault_id-name-unique")
                    .table(CashFlows::Table)
                    .col(CashFlows::VaultId)
                    .col(CashFlows::Name)
                    .unique()
                    .to_owned(),
            )
            .await?;

        let flow_rows = db
            .query_all(Statement::from_string(
                backend,
                "SELECT name, balance, max_balance, income_balance, currency, archived, vault_id FROM cash_flows_old;"
                    .to_string(),
            ))
            .await?;

        let mut flow_by_name: std::collections::HashMap<String, (String, String)> =
            std::collections::HashMap::new();

        for row in flow_rows {
            let name: String = row.try_get("", "name")?;
            let balance: i64 = row.try_get("", "balance")?;
            let max_balance: Option<i64> = row.try_get("", "max_balance")?;
            let income_balance: Option<i64> = row.try_get("", "income_balance")?;
            let currency: String = row.try_get("", "currency")?;
            let archived: bool = row.try_get("", "archived")?;
            let vault_id: String = row.try_get("", "vault_id")?;

            if flow_by_name.contains_key(&name) {
                return Err(DbErr::Custom(format!(
                    "cannot migrate cash_flows: cash flow name '{name}' exists in multiple vaults; this was previously ambiguous for entries"
                )));
            }

            let id = Uuid::new_v4().to_string();
            flow_by_name.insert(name.clone(), (id.clone(), vault_id.clone()));

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
                ])
                .values_panic([
                    id.into(),
                    name.into(),
                    balance.into(),
                    max_balance.into(),
                    income_balance.into(),
                    currency.into(),
                    archived.into(),
                    vault_id.into(),
                ])
                .to_owned();

            db.execute(backend.build(&stmt)).await?;
        }

        db.execute(Statement::from_string(
            backend,
            "DROP TABLE cash_flows_old;".to_string(),
        ))
        .await?;

        // --- Entries ---
        db.execute(Statement::from_string(
            backend,
            "ALTER TABLE entries RENAME TO entries_old;".to_string(),
        ))
        .await?;

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
                    .col(ColumnDef::new(Entries::Currency).string().not_null())
                    .col(ColumnDef::new(Entries::Note).string())
                    .col(ColumnDef::new(Entries::Category).string())
                    .col(ColumnDef::new(Entries::Date).timestamp().not_null())
                    .col(ColumnDef::new(Entries::VaultId).string().not_null())
                    .col(ColumnDef::new(Entries::CashFlowId).string())
                    .col(ColumnDef::new(Entries::WalletId).string())
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-entries-vault_id")
                            .from(Entries::Table, Entries::VaultId)
                            .to(Vaults::Table, Vaults::Id),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-entries-cashflow_id")
                            .from(Entries::Table, Entries::CashFlowId)
                            .to(CashFlows::Table, CashFlows::Id),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-entries-wallet_id")
                            .from(Entries::Table, Entries::WalletId)
                            .to(Wallets::Table, Wallets::Id),
                    )
                    .to_owned(),
            )
            .await?;

        let entry_rows = db
            .query_all(Statement::from_string(
                backend,
                "SELECT id, amount, currency, note, category, date, cash_flow_id, wallet_id FROM entries_old;"
                    .to_string(),
            ))
            .await?;

        for row in entry_rows {
            let id: String = row.try_get("", "id")?;
            let amount: i64 = row.try_get("", "amount")?;
            let currency: String = row.try_get("", "currency")?;
            let note: Option<String> = row.try_get("", "note")?;
            let category: Option<String> = row.try_get("", "category")?;
            let date: DateTimeUtc = row.try_get("", "date")?;
            let cash_flow_name: Option<String> = row.try_get("", "cash_flow_id")?;
            let wallet_name: Option<String> = row.try_get("", "wallet_id")?;

            let (wallet_id, wallet_vault_id) = wallet_name
                .as_ref()
                .and_then(|n| wallet_by_name.get(n))
                .map(|(id, vault_id)| (Some(id.clone()), Some(vault_id.clone())))
                .unwrap_or((None, None));

            let (cash_flow_id, flow_vault_id) = cash_flow_name
                .as_ref()
                .and_then(|n| flow_by_name.get(n))
                .map(|(id, vault_id)| (Some(id.clone()), Some(vault_id.clone())))
                .unwrap_or((None, None));

            let vault_id = match (wallet_vault_id, flow_vault_id) {
                (Some(w), Some(f)) if w == f => w,
                (Some(w), Some(f)) => {
                    return Err(DbErr::Custom(format!(
                        "cannot migrate entries: entry '{id}' refers to wallet in vault '{w}' and cash flow in vault '{f}'"
                    )));
                }
                (Some(w), None) => w,
                (None, Some(f)) => f,
                (None, None) => {
                    return Err(DbErr::Custom(format!(
                        "cannot migrate entries: entry '{id}' has neither wallet_id nor cash_flow_id"
                    )));
                }
            };

            let stmt = Query::insert()
                .into_table(Entries::Table)
                .columns([
                    Entries::Id,
                    Entries::Amount,
                    Entries::Currency,
                    Entries::Note,
                    Entries::Category,
                    Entries::Date,
                    Entries::VaultId,
                    Entries::CashFlowId,
                    Entries::WalletId,
                ])
                .values_panic([
                    id.into(),
                    amount.into(),
                    currency.into(),
                    note.into(),
                    category.into(),
                    date.into(),
                    vault_id.into(),
                    cash_flow_id.into(),
                    wallet_id.into(),
                ])
                .to_owned();

            db.execute(backend.build(&stmt)).await?;
        }

        db.execute(Statement::from_string(
            backend,
            "DROP TABLE entries_old;".to_string(),
        ))
        .await?;

        if backend == DbBackend::Sqlite {
            db.execute(Statement::from_string(
                backend,
                "PRAGMA foreign_keys=ON;".to_string(),
            ))
            .await?;
        }

        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Custom(
            "m20251214_090000_stable_ids is irreversible".to_string(),
        ))
    }
}
