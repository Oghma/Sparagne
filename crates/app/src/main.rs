use std::{env, fs::OpenOptions, io::IsTerminal, path::PathBuf};

use migration::{Migrator, MigratorTrait};
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

use settings::{Database, Settings};

mod settings;

type AppResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[tokio::main]
async fn main() -> AppResult<()> {
    let settings = match Settings::new() {
        Ok(settings) => settings,
        Err(err) => {
            eprintln!("failed to load settings: {err}");
            return Err(err.into());
        }
    };
    let _log_guard = init_tracing(&settings.app.level);
    let mut tasks = tokio::task::JoinSet::new();

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

            let engine = match engine::Engine::builder().database(db.clone()).build().await {
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

async fn parse_database(config: &Database) -> AppResult<sea_orm::DatabaseConnection> {
    let url = match config {
        Database::Memory => String::from("sqlite::memory"),
        Database::Sqlite(path) => format!("sqlite:{}?mode=rwc", path),
    };

    let database = sea_orm::Database::connect(url).await?;
    Migrator::up(&database, None).await?;
    Ok(database)
}

fn init_tracing(level: &str) -> Option<tracing_appender::non_blocking::WorkerGuard> {
    let env_filter = build_env_filter(level);
    let stderr_layer = tracing_subscriber::fmt::layer().with_ansi(std::io::stderr().is_terminal());
    let registry = tracing_subscriber::registry()
        .with(env_filter)
        .with(stderr_layer);

    if let Some(path) = log_file_path() {
        match OpenOptions::new().create(true).append(true).open(&path) {
            Ok(file) => {
                let (non_blocking, guard) = tracing_appender::non_blocking(file);
                let file_layer = tracing_subscriber::fmt::layer()
                    .with_writer(non_blocking)
                    .with_ansi(false);
                registry.with(file_layer).init();
                return Some(guard);
            }
            Err(err) => {
                eprintln!(
                    "failed to open log file {}: {err}; falling back to stderr",
                    path.display()
                );
            }
        }
    }

    registry.init();
    None
}

fn log_file_path() -> Option<PathBuf> {
    let value = match env::var("SPARAGNE_LOG_FILE") {
        Ok(value) => value,
        Err(env::VarError::NotPresent) => return None,
        Err(err) => {
            eprintln!("failed to read SPARAGNE_LOG_FILE: {err}");
            return None;
        }
    };
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(PathBuf::from(trimmed))
}

fn build_env_filter(level: &str) -> EnvFilter {
    if let Some(filter) =
        env_filter_from_var("SPARAGNE_LOG").or_else(|| env_filter_from_var("RUST_LOG"))
    {
        return filter;
    }

    let normalized = level.trim();
    let normalized = if normalized.is_empty() {
        "info"
    } else {
        normalized
    };
    let normalized = normalized.to_ascii_lowercase();
    let default_filter = format!(
        "sparagne={0},telegram_bot={0},server={0},engine={0}",
        normalized
    );
    EnvFilter::try_new(default_filter).unwrap_or_else(|err| {
        eprintln!("invalid app.level '{normalized}': {err}; falling back to 'info'");
        EnvFilter::new("info")
    })
}

fn env_filter_from_var(name: &str) -> Option<EnvFilter> {
    let value = match env::var(name) {
        Ok(value) => value,
        Err(env::VarError::NotPresent) => return None,
        Err(err) => {
            eprintln!("failed to read {name}: {err}");
            return None;
        }
    };
    match EnvFilter::try_new(value.clone()) {
        Ok(filter) => Some(filter),
        Err(err) => {
            eprintln!("invalid {name}='{value}': {err}");
            None
        }
    }
}
