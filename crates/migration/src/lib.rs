pub use sea_orm_migration::prelude::*;

mod m20230309_180650_cash_flows;
mod m20230309_214510_entries;
mod m20230528_204409_wallets;
mod m20230531_190127_vaults;
mod m20230828_064600_users;
mod m20251212_120000_currency;
mod m20251214_090000_stable_ids;
mod m20251215_090000_system_flows;
mod m20251215_120000_transactions;
mod m20251217_090000_idempotency_key;
mod m20251217_120000_memberships;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20230828_064600_users::Migration),
            Box::new(m20230531_190127_vaults::Migration),
            Box::new(m20230528_204409_wallets::Migration),
            Box::new(m20230309_180650_cash_flows::Migration),
            Box::new(m20230309_214510_entries::Migration),
            Box::new(m20251212_120000_currency::Migration),
            Box::new(m20251214_090000_stable_ids::Migration),
            Box::new(m20251215_090000_system_flows::Migration),
            Box::new(m20251215_120000_transactions::Migration),
            Box::new(m20251217_090000_idempotency_key::Migration),
            Box::new(m20251217_120000_memberships::Migration),
        ]
    }
}
