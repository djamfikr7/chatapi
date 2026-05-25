use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use chatapi_shared::ChatMessage;

/// A conversation session with full history.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub messages: Vec<ChatMessage>,
    pub metadata: SessionMetadata,
}

/// Metadata about a session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMetadata {
    pub model: String,
    pub tools_used: Vec<String>,
    pub total_tokens: u64,
    pub total_tool_calls: u64,
    pub tool_history: Vec<ToolExecution>,
}

impl SessionMetadata {
    pub fn new(model: String) -> Self {
        Self {
            model,
            tools_used: Vec::new(),
            total_tokens: 0,
            total_tool_calls: 0,
            tool_history: Vec::new(),
        }
    }
}

/// Record of a single tool execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolExecution {
    pub tool_name: String,
    pub arguments: String,
    pub result: String,
    pub is_error: bool,
    pub executed_at: DateTime<Utc>,
    pub duration_ms: u64,
}

/// Lightweight summary for listing sessions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSummary {
    pub id: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub message_count: usize,
    pub model: String,
}
