use async_trait::async_trait;
use std::path::PathBuf;
use std::sync::Arc;

use chatapi_shared::traits::TargetProvider;
use chatapi_tools::ToolRegistry;

use crate::error::AgentError;
use crate::state::TaskState;
use crate::task::{AgentRole, TaskStep};

/// Shared context available to all agents.
pub struct AgentContext {
    pub target: Arc<dyn TargetProvider>,
    pub tools: ToolRegistry,
    pub working_dir: PathBuf,
}

/// Configuration for an agent instance.
#[derive(Debug, Clone)]
pub struct AgentConfig {
    /// System prompt that defines the agent's behavior.
    pub system_prompt: String,
    /// Maximum LLM turns before the agent gives up.
    pub max_turns: usize,
    /// Maximum retries on transient errors.
    pub max_retries: usize,
    /// Timeout per step in seconds.
    pub step_timeout_secs: u64,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            system_prompt: String::new(),
            max_turns: 20,
            max_retries: 3,
            step_timeout_secs: 300,
        }
    }
}

/// Core trait that every agent must implement.
///
/// An agent receives a task step, executes it using LLM + tools,
/// and returns the result. The agent decides internally how to
/// use the LLM loop (target → tool_calls → execute → result → LLM).
#[async_trait]
pub trait Agent: Send + Sync {
    /// The role this agent plays (coding, testing, debugging, etc.)
    fn role(&self) -> AgentRole;

    /// Human-readable name for this agent instance.
    fn name(&self) -> &str;

    /// Execute a task step and return the result.
    async fn run(&self, step: TaskStep, ctx: &TaskState) -> Result<String, AgentError>;

    /// Tool names this agent is allowed to use.
    fn available_tools(&self) -> Vec<String>;
}
