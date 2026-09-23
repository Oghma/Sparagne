//! `sparagne-server` binary: reads the environment, serves the router.

use std::net::SocketAddr;

use sparagne_server::{AppState, Config, router};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    let bind: SocketAddr = std::env::var("SPARAGNE_BIND")
        .unwrap_or_else(|_| "127.0.0.1:3000".to_string())
        .parse()?;
    let data_dir = std::env::var("SPARAGNE_DATA_DIR").unwrap_or_else(|_| "./data".to_string());
    let config = Config::from_env();
    let state = AppState::open(&data_dir, config)?;
    let listener = tokio::net::TcpListener::bind(bind).await?;
    tracing::info!(
        %bind,
        data_dir,
        allow_registration = config.allow_registration,
        token_ttl_days = config.token_ttl_days,
        trust_proxy = config.trust_forwarded_for,
        login_max_failures = config.login_max_failures,
        login_window_secs = config.login_window_secs,
        ip_max_failures = config.ip_max_failures,
        "listening"
    );
    // The peer address is what the rate limits key on (`client_ip.rs`).
    let app = router(state).into_make_service_with_connect_info::<SocketAddr>();
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown())
        .await?;
    tracing::info!("stopped");
    Ok(())
}

/// Resolves on ctrl-c, or on SIGTERM where there is one.
async fn shutdown() {
    let interrupt = async {
        if let Err(err) = tokio::signal::ctrl_c().await {
            tracing::error!(%err, "cannot listen for ctrl-c");
        }
    };

    #[cfg(unix)]
    {
        let mut terminate =
            match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
                Ok(signal) => signal,
                Err(err) => {
                    tracing::error!(%err, "cannot listen for SIGTERM");
                    interrupt.await;
                    return;
                }
            };
        tokio::select! {
            () = interrupt => {}
            _ = terminate.recv() => {}
        }
    }

    #[cfg(not(unix))]
    interrupt.await;
}
