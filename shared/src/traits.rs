//! Core trait system for the ChatAPI platform.
//!
//! - `ToolProvider`: Every tool (built-in, MCP, custom) implements this.
//! - `TargetProvider`: Each backend (CDP, API, MCP) implements this.
//! - `ToolContext` / `ToolResult` / error types: shared plumbing.

use std::collections::HashMap;
use std::path::PathBuf;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

// ── Tool system ───────────────────────────────────────────────────────

/// Context passed to every tool execution.
#[derive(Debug, Clone)]
pub struct ToolContext {
    pub session_id: String,
    pub working_dir: PathBuf,
    pub env: HashMap<String, String>,
}

/// Result of a tool execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum ToolResult {
    Text(String),
    Diff {
        old: String,
        new: String,
        path: PathBuf,
    },
    Error {
        message: String,
        recoverable: bool,
    },
}

/// Errors from tool execution.
#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    #[error("tool not found: {0}")]
    NotFound(String),
    #[error("invalid arguments: {0}")]
    InvalidArgs(String),
    #[error("execution failed: {0}")]
    ExecutionFailed(String),
    #[error("timeout after {0}ms")]
    Timeout(u64),
    #[error("path blocked: {0}")]
    PathBlocked(String),
}

/// Every tool implements this trait.
#[async_trait]
pub trait ToolProvider: Send + Sync {
    /// Short identifier used in tool_calls (e.g. "edit_file").
    fn name(&self) -> &str;

    /// Human-readable description shown to the LLM.
    fn description(&self) -> &str;

    /// JSON Schema for the tool's parameters.
    fn parameters_schema(&self) -> serde_json::Value;

    /// Execute the tool with the given arguments and context.
    async fn execute(
        &self,
        args: serde_json::Value,
        ctx: &ToolContext,
    ) -> Result<ToolResult, ToolError>;
}

// ── Target system ─────────────────────────────────────────────────────

/// Stream of tokens from a target provider.
pub type TargetStream =
    std::pin::Pin<Box<dyn futures_util::Stream<Item = Result<String, TargetError>> + Send>>;

/// Errors from target providers.
#[derive(Debug, thiserror::Error)]
pub enum TargetError {
    #[error("connection failed: {0}")]
    ConnectionFailed(String),
    #[error("request failed: {0}")]
    RequestFailed(String),
    #[error("timeout")]
    Timeout,
}

/// Each backend (CDP, API, MCP) implements this trait.
#[async_trait]
pub trait TargetProvider: Send + Sync {
    /// Short identifier ("cdp", "api", "mcp").
    fn name(&self) -> &str;

    /// Check whether the backend is reachable and ready.
    async fn health_check(&self) -> bool;

    /// Non-streaming completion request.
    async fn send_request(
        &self,
        req: &super::ChatCompletionRequest,
    ) -> Result<super::ChatCompletionResponse, TargetError>;

    /// Streaming completion request — yields raw token strings.
    async fn stream_request(
        &self,
        req: &super::ChatCompletionRequest,
    ) -> Result<TargetStream, TargetError>;
}

// ── Session store ─────────────────────────────────────────────────────

/// Lightweight summary returned by `list`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSummary {
    pub id: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub message_count: usize,
    pub model: String,
}

/// Persistence for sessions.
#[async_trait]
pub trait SessionStore: Send + Sync {
    async fn save(&self, session: &super::ChatMessage, id: &str) -> Result<(), SessionError>;
    async fn load(&self, id: &str) -> Result<Option<Vec<super::ChatMessage>>, SessionError>;
    async fn list(&self) -> Result<Vec<SessionSummary>, SessionError>;
    async fn delete(&self, id: &str) -> Result<(), SessionError>;
}

/// Errors from session persistence.
#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("session not found: {0}")]
    NotFound(String),
    #[error("store error: {0}")]
    StoreError(String),
}
