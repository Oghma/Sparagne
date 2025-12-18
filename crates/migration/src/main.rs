use sea_orm::Database;
use sea_orm_migration::prelude::*;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut args = std::env::args().skip(1);
    let cmd = args.next().unwrap_or_else(|| "up".to_string());

    let db_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "sqlite:./sparagne.db?mode=rwc".to_string());

    let db = Database::connect(&db_url).await?;

    match cmd.as_str() {
        "up" => migration::Migrator::up(&db, None).await?,
        "down" => migration::Migrator::down(&db, None).await?,
        "fresh" => migration::Migrator::fresh(&db).await?,
        "status" => {
            migration::Migrator::status(&db).await?;
        }
        _ => {
            eprintln!("Usage: cargo run -p migration -- [up|down|fresh|status]");
            std::process::exit(2);
        }
    }

    Ok(())
}
