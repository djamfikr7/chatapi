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
    pub fn new(ctx: Arc<AgentContext>, mut config: AgentConfig) -> Self {
        config.system_prompt = "You are an expert debugging agent. You investigate failures \
            methodically: reproduce the issue, read error messages and stack traces, \
            examine relevant source code, check git history for recent changes, \
            and propose targeted fixes. You use git_diff and git_status to understand \
            what changed. Always verify your fix by running the failing test or command \
            after applying changes."
            .to_string();
        config.tool_filter = vec![
            "read_file".into(), "write_file".into(), "edit_file".into(),
            "apply_patch".into(), "list_dir".into(), "run_command".into(),
            "grep_code".into(), "git_status".into(), "git_diff".into(),
            "git_log".into(), "git_show".into(),
        ];
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
