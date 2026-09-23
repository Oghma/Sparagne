//! `sparagne-server` binary. With no arguments, or `serve`, it reads the
//! environment and serves the router; `sparagne-server user …` manages the
//! accounts in the same data directory (`admin.rs`).

use std::{net::SocketAddr, process::ExitCode};

use sparagne_server::{
    AppState, Config,
    admin::{self, AdminError},
    router,
};
use tracing_subscriber::EnvFilter;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        None | Some("serve") if args.len() <= 1 => serve(),
        _ => run_admin(&args),
    }
}

fn serve() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();
    let result = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(Into::into)
        .and_then(|runtime| runtime.block_on(serve_until_shutdown()));
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("sparagne-server: {err}");
            ExitCode::FAILURE
        }
    }
}

/// Usage mistakes exit with 2, failures with 1.
fn run_admin(args: &[String]) -> ExitCode {
    // Stdout is the command's own output; what the storage layer logs (the
    // detail behind "internal error") goes to stderr.
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();
    match admin::run(
        args,
        &mut std::io::stdin().lock(),
        &mut std::io::stdout().lock(),
    ) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("sparagne-server: {err}");
            if matches!(err, AdminError::Usage(_)) {
                ExitCode::from(2)
            } else {
                ExitCode::FAILURE
            }
        }
    }
}

async fn serve_until_shutdown() -> Result<(), Box<dyn std::error::Error>> {
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
