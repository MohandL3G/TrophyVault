use std::sync::Arc;
use tokio::sync::RwLock;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};
use trophyvault_server::{
    create_app,
    state::{AppState, ServerConfig},
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Initialize structured logging
    tracing_subscriber::registry()
        .with(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "trophyvault_server=info,tower_http=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // 2. Load configuration from environment
    let config = ServerConfig::from_env();
    tracing::info!(
        "Starting TrophyVault Server v{} on port {}",
        env!("CARGO_PKG_VERSION"),
        config.port
    );
    tracing::info!("Database path: {}", config.db_path.display());
    tracing::info!("Static assets directory: {}", config.static_dir.display());

    // 3. Initialize in-memory and persistent state
    let state = Arc::new(RwLock::new(AppState::new(&config)));

    // 4. Construct application router
    let app = create_app(state, config.static_dir);

    // 5. Bind listener and launch Axum server
    let addr = format!("0.0.0.0:{}", config.port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("Server listening on http://{}", addr);

    axum::serve(listener, app).await?;
    Ok(())
}
