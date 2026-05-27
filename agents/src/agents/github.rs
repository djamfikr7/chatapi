use async_trait::async_trait;
use std::sync::Arc;

use crate::agent::{Agent, AgentConfig, AgentContext};
use crate::agents::coding::CodingAgent;
use crate::error::AgentError;
use crate::state::TaskState;
use crate::task::{AgentRole, TaskStep};

/// GitHub agent — manages PRs, issues, CI/CD.
///
/// Uses git and command tools to interact with GitHub.
/// Future: will use octocrab for direct GitHub API access.
pub struct GitHubAgent {
    inner: CodingAgent,
}

impl GitHubAgent {
    pub fn new(ctx: Arc<AgentContext>, config: AgentConfig) -> Self {
        Self {
            inner: CodingAgent::new(ctx, config),
        }
    }
}

#[async_trait]
impl Agent for GitHubAgent {
    fn role(&self) -> AgentRole {
        AgentRole::GitHub
    }

    fn name(&self) -> &str {
        "GitHubAgent"
    }

    fn available_tools(&self) -> Vec<String> {
        vec![
            "read_file".to_string(),
            "list_dir".to_string(),
            "run_command".to_string(),
            "git_status".to_string(),
            "git_diff".to_string(),
        ]
    }

    async fn run(&self, step: TaskStep, ctx: &TaskState) -> Result<String, AgentError> {
        self.inner.run(step, ctx).await
    }
}
