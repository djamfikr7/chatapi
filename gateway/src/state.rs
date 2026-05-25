use std::collections::HashMap;
use std::sync::Arc;
use chatapi_mcp::McpClient;
use chatapi_rules::ChatApiConfig;
use chatapi_sessions::SessionManager;
use chatapi_shared::ToolCall;
use chatapi_shared::traits::TargetProvider;
use chatapi_tools::ToolRegistry;

use crate::ws::EventBroadcaster;

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
    /// WebSocket event broadcaster.
    pub events: EventBroadcaster,
    /// Pending tool calls awaiting user approval.
    pub pending_tools: Arc<tokio::sync::Mutex<HashMap<String, ToolCall>>>,
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
            events: EventBroadcaster::new(256),
            pending_tools: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
        }
    }
}
