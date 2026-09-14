use neloa_relay::{router, RelayConfig, RelayState};
use tokio::net::TcpListener;
use tracing::info;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("neloa_relay=info")),
        )
        .without_time()
        .init();

    let config = RelayConfig::from_env().unwrap_or_else(|error| {
        eprintln!("invalid relay configuration: {error}");
        std::process::exit(2);
    });
    let state =
        RelayState::with_max_devices(config.token, config.max_devices).unwrap_or_else(|error| {
            eprintln!("invalid relay configuration: {error}");
            std::process::exit(2);
        });
    let listener = TcpListener::bind(config.bind)
        .await
        .unwrap_or_else(|error| {
            eprintln!("failed to bind relay listener: {error}");
            std::process::exit(1);
        });
    info!(bind = %config.bind, "Neloa relay listening");

    axum::serve(listener, router(state))
        .with_graceful_shutdown(shutdown_signal())
        .await
        .unwrap_or_else(|error| {
            eprintln!("relay server failed: {error}");
            std::process::exit(1);
        });
}

async fn shutdown_signal() {
    let interrupt = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = interrupt => {}
        () = terminate => {}
    }
}
