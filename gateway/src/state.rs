use std::sync::Arc;
use chatapi_mcp::McpClient;
use chatapi_rules::ChatApiConfig;
use chatapi_sessions::SessionManager;
use chatapi_shared::traits::TargetProvider;
use chatapi_tools::ToolRegistry;

/// Application state shared across all handlers.
#[derive(Clone)]
pub struct AppState {
    /// Config loaded from .chatapi/config.toml.
    pub config: Arc<tokio::sync::RwLock<ChatApiConfig>>,
    /// Target provider (API or browser).
    pub target: Arc<dyn TargetProvider>,
    /// Tool registry with built-in + MCP tools.
    pub tools: Arc<ToolRegistry>,
    /// Session manager for conversation history.
    pub sessions: Arc<SessionManager>,
    /// Active MCP server connections.
    pub mcp_clients: Arc<Vec<Arc<McpClient>>>,
}

impl AppState {
    pub fn new(
        config: ChatApiConfig,
        target: impl TargetProvider + 'static,
        tools: ToolRegistry,
        sessions: SessionManager,
        mcp_clients: Vec<Arc<McpClient>>,
    ) -> Self {
        Self {
            config: Arc::new(tokio::sync::RwLock::new(config)),
            target: Arc::new(target),
            tools: Arc::new(tools),
            sessions: Arc::new(sessions),
            mcp_clients: Arc::new(mcp_clients),
        }
    }
}
