use axum::{routing::{get, post}, Router};
use tower_http::cors::{Any, CorsLayer};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use chatapi_ringbuf::channel;

mod lib; // ensure lib.rs is compiled
use chatapi_gateway::routes;
use chatapi_gateway::state::AppState;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| "gateway=info,tower_http=info".into()))
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Create ring buffer for IPC with CDP engine (256KB)
    let (producer, consumer) = channel(256 * 1024);

    // Create application state
    let state = AppState::new(producer, consumer);

    // CORS layer
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    // Build router
    let app = Router::new()
        .route("/v1/chat/completions", post(routes::chat_completions))
        .route("/health", get(routes::health))
        .layer(cors)
        .with_state(state);

    // Bind and serve
    let bind = "0.0.0.0:3000";
    tracing::info!("Gateway listening on {}", bind);

    let listener = tokio::net::TcpListener::bind(bind).await?;

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    tracing::info!("Gateway shut down gracefully");
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c().await.expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => tracing::info!("Received Ctrl+C, shutting down..."),
        _ = terminate => tracing::info!("Received SIGTERM, shutting down..."),
    }
}
