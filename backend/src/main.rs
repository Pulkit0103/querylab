use querylab_api::{build_router, config::Config, database, telemetry, AppState};
use tokio::net::TcpListener;

#[tokio::main]
async fn main() {
    if let Err(err) = run().await {
        eprintln!("fatal: {err}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    // A missing .env file is fine; real deployments inject variables directly.
    let _ = dotenvy::dotenv();

    let config = Config::from_env()?;
    telemetry::init(&config.log_format);

    let pool = database::create_pool(&config)?;
    if database::ping(&pool).await {
        tracing::info!("connected to metadata database");
    } else {
        tracing::warn!("metadata database is not reachable yet; continuing in degraded mode");
    }

    let addr = config.bind_addr;
    let app = build_router(AppState::new(config, pool));

    let listener = TcpListener::bind(addr).await?;
    tracing::info!(%addr, "QueryLab API listening");
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await?;
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        if let Ok(mut sig) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            sig.recv().await;
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    tracing::info!("shutdown signal received");
}
