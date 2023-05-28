use engine;
use server;
use telegram_bot;

mod settings;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter("hodlTracker=debug,telegram_bot=debug")
        .init();

    let settings = settings::Settings::new().unwrap();

    let engine = engine::Engine::builder()
        .database(&settings.sqlite.path)
        .build()
        .await;
    tokio::spawn(async move { server::run(engine).await });

    if let Some(telegram) = settings.telegram {
        tracing::info!("Found telegram settings...");
        telegram_bot::Bot::builder()
            .token(&telegram.token)
            .server_url(&telegram.server_url)
            .build()
            .run()
            .await;
    }
}
