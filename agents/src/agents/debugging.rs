use async_trait::async_trait;
use std::sync::Arc;

use crate::agent::{Agent, AgentConfig, AgentContext};
use crate::agents::coding::CodingAgent;
use crate::error::AgentError;
use crate::state::TaskState;
use crate::task::{AgentRole, TaskStep};

/// Debugging agent — investigates failures, proposes fixes.
///
/// Has full tool access to read code, run commands, check git history,
/// and apply fixes.
pub struct DebuggingAgent {
    inner: CodingAgent,
}

impl DebuggingAgent {
    pub fn new(ctx: Arc<AgentContext>, config: AgentConfig) -> Self {
        Self {
            inner: CodingAgent::new(ctx, config),
        }
    }
}

#[async_trait]
impl Agent for DebuggingAgent {
    fn role(&self) -> AgentRole {
        AgentRole::Debugging
    }

    fn name(&self) -> &str {
        "DebuggingAgent"
    }

    fn available_tools(&self) -> Vec<String> {
        vec![
            "read_file".to_string(),
            "write_file".to_string(),
            "edit_file".to_string(),
            "list_dir".to_string(),
            "run_command".to_string(),
            "grep_code".to_string(),
            "git_status".to_string(),
            "git_diff".to_string(),
        ]
    }

    async fn run(&self, step: TaskStep, ctx: &TaskState) -> Result<String, AgentError> {
        self.inner.run(step, ctx).await
    }
}
