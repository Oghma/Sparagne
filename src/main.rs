use migration::{Migrator, MigratorTrait};
use settings::Database;

mod settings;

#[tokio::main]
async fn main() {
    let settings = settings::Settings::new().unwrap();
    let mut tasks = tokio::task::JoinSet::new();

    tracing_subscriber::fmt()
        .with_env_filter(format!(
            "hodl_tracker={level},telegram_bot={level},server={level},engine={level}",
            level = settings.app.level
        ))
        .init();

    if let Some(server) = settings.server {
        tasks.spawn(async move {
            tracing::info!("Found server settings...");
            let db = parse_database(&server.database).await;

            let engine = engine::Engine::builder().database(db.clone()).build().await;
            server::run(engine, db).await;
        });
    }

    if let Some(telegram) = settings.telegram {
        tasks.spawn(async move {
            tracing::info!("Found telegram settings...");
            telegram_bot::Bot::builder()
                .token(&telegram.token)
                .server(&telegram.server, &telegram.username, &telegram.password)
                .build()
                .run()
                .await;
        });
    }

    while tasks.join_next().await.is_some() {
        tasks.shutdown().await;
    }
}

async fn parse_database(config: &settings::Database) -> sea_orm::DatabaseConnection {
    let url = match config {
        Database::Memory => String::from("sqlite::memory"),
        Database::Sqlite(path) => format!("sqlite:{}?mode=rwc", path),
    };

    let database = sea_orm::Database::connect(url)
        .await
        .expect("Failed to connect to the database");

    Migrator::up(&database, None).await.unwrap();

    database
}
