use migration::{Migrator, MigratorTrait};
use settings::Database;

mod settings;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let settings = settings::Settings::new()?;
    let mut tasks = tokio::task::JoinSet::new();

    tracing_subscriber::fmt()
        .with_env_filter(format!(
            "sparagne={level},telegram_bot={level},server={level},engine={level}",
            level = settings.app.level
        ))
        .init();

    if let Some(server) = settings.server {
        tasks.spawn(async move {
            tracing::info!("Found server settings...");
            let db = match parse_database(&server.database).await {
                Ok(db) => db,
                Err(err) => {
                    tracing::error!("failed to initialize database: {err}");
                    return;
                }
            };

            let engine = match engine::Engine::builder()
                .database(db.clone())
                .build()
                .await
            {
                Ok(engine) => engine,
                Err(err) => {
                    tracing::error!("failed to build engine from database: {err}");
                    return;
                }
            };
            let bind = server.bind.unwrap_or_else(|| "127.0.0.1".to_string());
            let addr = format!("{}:{}", bind, server.port);
            let listener = match tokio::net::TcpListener::bind(addr).await {
                Ok(listener) => listener,
                Err(err) => {
                    tracing::error!("failed to bind server listener: {err}");
                    return;
                }
            };
            if let Err(err) = server::run_with_listener(engine, db, listener).await {
                tracing::error!("server failed: {err}");
            }
        });
    }

    if let Some(telegram) = settings.telegram {
        tasks.spawn(async move {
            tracing::info!("Found telegram settings...");
            match telegram_bot::Bot::builder()
                .token(&telegram.token)
                .server(&telegram.server, &telegram.username, &telegram.password)
                .build()
            {
                Ok(bot) => bot.run().await,
                Err(err) => tracing::error!("failed to initialize telegram bot: {err}"),
            }
        });
    }

    while tasks.join_next().await.is_some() {
        tasks.shutdown().await;
    }

    Ok(())
}

async fn parse_database(
    config: &settings::Database,
) -> Result<sea_orm::DatabaseConnection, Box<dyn std::error::Error + Send + Sync>> {
    let url = match config {
        Database::Memory => String::from("sqlite::memory"),
        Database::Sqlite(path) => format!("sqlite:{}?mode=rwc", path),
    };

    let database = sea_orm::Database::connect(url).await?;
    Migrator::up(&database, None).await?;
    Ok(database)
}
