use thiserror::Error;

#[derive(Debug, Error)]
pub enum AgentError {
    #[error("LLM request failed: {0}")]
    LlmFailed(String),
    #[error("Tool execution failed: {0}")]
    ToolFailed(String),
    #[error("Task planning failed: {0}")]
    PlanningFailed(String),
    #[error("Agent timeout after {0}s")]
    Timeout(u64),
    #[error("Agent cancelled")]
    Cancelled,
    #[error("Invalid state: {0}")]
    InvalidState(String),
    #[error("No agent available for role: {0}")]
    NoAgent(String),
    #[error("{0}")]
    Other(#[from] anyhow::Error),
}
