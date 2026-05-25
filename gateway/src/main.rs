use std::path::Path;
use std::sync::Arc;
use axum::{routing::{get, post, delete, put}, Router};
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::ServeDir;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use chatapi_mcp::{McpClient, McpToolProvider};
use chatapi_rules::ChatApiConfig;
use chatapi_sessions::{SessionManager, MemoryStore, FileStore};
use chatapi_targets::{TargetRouter, BrowserTarget};
use chatapi_tools::ToolRegistry;
use chatapi_shared::target::TargetConfig;
use chatapi_shared::target::Target as TargetKind;

use chatapi_gateway::routes;
use chatapi_gateway::state::AppState;
use chatapi_gateway::ws;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| "gateway=info,tower_http=info".into()))
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Load config
    let config_path = std::env::var("CHATAPI_CONFIG")
        .unwrap_or_else(|_| ".chatapi/config.toml".to_string());
    let config = ChatApiConfig::load_or_default(Path::new(&config_path));
    tracing::info!(mode = %config.target.mode, model = %config.target.model, "Loaded config");

    // Build target from config
    let target = if config.target.mode == "api" {
        // API mode: use direct API endpoint
        let api_key = config.target.api.as_ref().and_then(|a| {
            std::env::var(&a.api_key_env).ok()
        });
        let api_endpoint = config.target.api.as_ref()
            .map(|a| a.endpoint.clone())
            .unwrap_or_else(|| "https://api.deepseek.com/v1".to_string());
        let target_config = TargetConfig {
            target: TargetKind::Api,
            api_endpoint,
            api_key,
            model: config.target.model.clone(),
        };
        tracing::info!("Target: API mode");
        TargetRouter::new(&target_config)
    } else {
        // Browser mode: try to connect to Chrome via CDP
        let chrome_port = std::env::var("CHROME_DEBUG_PORT")
            .ok()
            .and_then(|p| p.parse::<u16>().ok())
            .unwrap_or(9222);

        // Auto-launch Chrome if LAUNCH_CHROME=1
        if std::env::var("LAUNCH_CHROME").ok().as_deref() == Some("1") {
            if let Err(e) = launch_chrome(chrome_port).await {
                tracing::warn!(error = %e, "Failed to auto-launch Chrome");
            }
        }

        match BrowserTarget::from_port(chrome_port, config.target.model.clone()).await {
            Ok(browser) => {
                tracing::info!(port = chrome_port, "Target: Browser (CDP connected)");
                TargetRouter::with_browser(browser)
            }
            Err(e) => {
                tracing::warn!(error = %e, port = chrome_port, "Chrome not available, falling back to API stub");
                let target_config = TargetConfig {
                    target: TargetKind::Browser,
                    api_endpoint: String::new(),
                    api_key: None,
                    model: config.target.model.clone(),
                };
                TargetRouter::new(&target_config)
            }
        }
    };

    // Build tool registry with built-in + MCP tools
    let mut tools = build_tool_registry();
    let mcp_clients = connect_mcp_servers(&config, &mut tools).await;

    // Build session manager
    let store: Box<dyn chatapi_shared::traits::SessionStore> = if config.sessions.store == "file" {
        let path = config.sessions.path.as_deref().unwrap_or(".chatapi/sessions");
        let fs = FileStore::new(path);
        fs.init().await?;
        tracing::info!(path = %path, "Using file-backed session store");
        Box::new(fs)
    } else {
        tracing::info!("Using in-memory session store");
        Box::new(MemoryStore::new())
    };
    let sessions = SessionManager::new(store);

    // Create application state
    let state = AppState::new(config, target, tools, sessions, mcp_clients);

    // CORS layer
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    // Static file serving for frontend build
    let frontend_dir = std::env::var("CHATAPI_FRONTEND_DIR")
        .unwrap_or_else(|_| "frontend/dist".to_string());
    let serve_frontend = ServeDir::new(&frontend_dir)
        .not_found_service(ServeDir::new(&frontend_dir)); // SPA fallback

    // Build router
    let app = Router::new()
        // OpenAI-compatible endpoints
        .route("/v1/chat/completions", post(routes::chat_completions))
        .route("/v1/models", get(routes::list_models))
        .route("/v1/providers", get(routes::list_providers))
        // Health
        .route("/health", get(routes::health))
        // Session management
        .route("/sessions", get(routes::list_sessions))
        .route("/sessions", post(routes::create_session))
        .route("/sessions/{session_id}", get(routes::get_session))
        .route("/sessions/{session_id}", delete(routes::delete_session))
        .route("/sessions/{session_id}/branch", post(routes::branch_session))
        // Tools
        .route("/tools", get(routes::list_tools))
        .route("/tools/execute", post(routes::execute_tool))
        // Files
        .route("/files", get(routes::list_files))
        // Config
        .route("/config", get(routes::get_config))
        .route("/config", put(routes::update_config))
        // WebSocket
        .route("/ws", get(ws::ws_handler))
        .route("/ws/terminal", get(ws::terminal_handler))
        .layer(cors)
        .with_state(state)
        // Serve frontend static files (must be last — catches all non-API routes)
        .fallback_service(serve_frontend);

    // Bind and serve
    let port = std::env::var("CHATAPI_PORT").unwrap_or_else(|_| "8090".to_string());
    let bind = format!("0.0.0.0:{}", port);
    tracing::info!("Gateway listening on {}", bind);
    tracing::info!("Frontend: http://localhost:{}", port);

    let listener = tokio::net::TcpListener::bind(bind).await?;

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    tracing::info!("Gateway shut down gracefully");
    Ok(())
}

/// Check if Chrome is already running on the given debug port.
async fn chrome_running_on_port(port: u16) -> bool {
    match tokio::net::TcpStream::connect(format!("127.0.0.1:{}", port)).await {
        Ok(_) => {
            tracing::info!(port = port, "Chrome already running on debug port");
            true
        }
        Err(_) => false,
    }
}

/// Find a free port starting from the given port.
async fn find_free_port(start: u16) -> u16 {
    for port in start..start + 100 {
        if tokio::net::TcpListener::bind(format!("127.0.0.1:{}", port)).await.is_ok() {
            return port;
        }
    }
    start
}

/// Launch Chrome with remote debugging enabled.
async fn launch_chrome(port: u16) -> anyhow::Result<()> {
    // Check if Chrome is already running
    if chrome_running_on_port(port).await {
        return Ok(());
    }

    // Find a free port if the requested one is taken
    let actual_port = find_free_port(port).await;
    if actual_port != port {
        tracing::warn!(requested = port, actual = actual_port, "Port {} in use, using {}", port, actual_port);
    }

    use tokio::process::Command;

    let chrome_bin = if cfg!(target_os = "linux") {
        "google-chrome"
    } else if cfg!(target_os = "macos") {
        "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
    } else {
        "chrome.exe"
    };

    tracing::info!(port = actual_port, binary = chrome_bin, "Launching Chrome with remote debugging");

    Command::new(chrome_bin)
        .arg(format!("--remote-debugging-port={}", actual_port))
        .arg("--no-first-run")
        .arg("--no-default-browser-check")
        .arg("--disable-background-networking")
        .arg("--disable-sync")
        .arg("--disable-translate")
        .arg("--metrics-recording-only")
        .arg("--safebrowsing-disable-auto-update")
        .spawn()?;

    // Wait for Chrome to start
    for _ in 0..10 {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        if chrome_running_on_port(actual_port).await {
            tracing::info!(port = actual_port, "Chrome started successfully");
            return Ok(());
        }
    }

    anyhow::bail!("Chrome failed to start within 5 seconds")
}

fn build_tool_registry() -> ToolRegistry {
    use chatapi_tools::{file_ops, terminal, git_ops, search};
    let mut registry = ToolRegistry::new();
    registry.register(Box::new(file_ops::ReadFile));
    registry.register(Box::new(file_ops::WriteFile));
    registry.register(Box::new(file_ops::EditFile));
    registry.register(Box::new(file_ops::ListDir));
    registry.register(Box::new(terminal::RunCommand));
    registry.register(Box::new(terminal::GetDiagnostics));
    registry.register(Box::new(git_ops::GitStatus));
    registry.register(Box::new(git_ops::GitDiff));
    registry.register(Box::new(git_ops::GitCommit));
    registry.register(Box::new(search::GrepCode));
    registry
}

/// Connect to configured MCP servers and register their tools.
async fn connect_mcp_servers(
    config: &ChatApiConfig,
    tools: &mut ToolRegistry,
) -> Vec<Arc<McpClient>> {
    let mcp_config = match config.target.mcp.as_ref() {
        Some(m) => m,
        None => return Vec::new(),
    };

    let mut clients = Vec::new();
    for server in &mcp_config.servers {
        match McpClient::spawn(&server.name, &server.command, &server.args, &server.env).await {
            Ok(client) => {
                let client = Arc::new(client);
                match client.list_tools().await {
                    Ok(mcp_tools) => {
                        tracing::info!(server = %server.name, tool_count = mcp_tools.len(), "MCP server connected");
                        for tool in mcp_tools {
                            tracing::debug!(tool = %tool.name, server = %server.name, "Registering MCP tool");
                            tools.register(Box::new(McpToolProvider::new(client.clone(), tool)));
                        }
                        clients.push(client);
                    }
                    Err(e) => {
                        tracing::error!(server = %server.name, error = %e, "Failed to list MCP tools");
                        client.shutdown().await;
                    }
                }
            }
            Err(e) => {
                tracing::error!(server = %server.name, error = %e, "Failed to connect MCP server");
            }
        }
    }
    clients
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
