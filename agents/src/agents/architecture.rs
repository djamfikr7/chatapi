use async_trait::async_trait;
use std::sync::Arc;

use crate::agent::{Agent, AgentConfig, AgentContext};
use crate::agents::coding::CodingAgent;
use crate::error::AgentError;
use crate::state::TaskState;
use crate::task::{AgentRole, TaskStep};

/// Architecture agent — designs system structure, evaluates trade-offs.
///
/// Uses read-only tools (no write_file, no run_command) to analyze codebases
/// and produce architectural recommendations.
pub struct ArchitectureAgent {
    inner: CodingAgent,
}

impl ArchitectureAgent {
    pub fn new(ctx: Arc<AgentContext>, config: AgentConfig) -> Self {
        Self {
            inner: CodingAgent::new(ctx, config),
        }
    }
}

#[async_trait]
impl Agent for ArchitectureAgent {
    fn role(&self) -> AgentRole {
        AgentRole::Architecture
    }

    fn name(&self) -> &str {
        "ArchitectureAgent"
    }

    fn available_tools(&self) -> Vec<String> {
        vec![
            "read_file".to_string(),
            "list_dir".to_string(),
            "grep_code".to_string(),
        ]
    }

    async fn run(&self, step: TaskStep, ctx: &TaskState) -> Result<String, AgentError> {
        self.inner.run(step, ctx).await
    }
}
