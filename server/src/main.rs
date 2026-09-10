//! `sparagne-server` binary: reads the environment, serves the router.

use std::net::SocketAddr;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    let bind: SocketAddr = std::env::var("SPARAGNE_BIND")
        .unwrap_or_else(|_| "127.0.0.1:3000".to_string())
        .parse()?;
    let listener = tokio::net::TcpListener::bind(bind).await?;
    tracing::info!(%bind, "listening");
    axum::serve(listener, sparagne_server::router()).await?;
    Ok(())
}
