pub use sea_orm_migration::prelude::*;

mod m20251230_000000_init;
mod m20260115_000001_categories;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20251230_000000_init::Migration),
            Box::new(m20260115_000001_categories::Migration),
        ]
    }
}
