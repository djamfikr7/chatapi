use async_trait::async_trait;
use std::sync::Arc;

use crate::agent::{Agent, AgentConfig, AgentContext};
use crate::agents::coding::CodingAgent;
use crate::error::AgentError;
use crate::state::TaskState;
use crate::task::{AgentRole, TaskStep};

/// Wiki agent — tracks progress, updates docs and knowledge base.
///
/// Reads and writes markdown files to maintain project documentation,
/// STATUS.md, and the .knowledge/ directory.
pub struct WikiAgent {
    inner: CodingAgent,
}

impl WikiAgent {
    pub fn new(ctx: Arc<AgentContext>, config: AgentConfig) -> Self {
        Self {
            inner: CodingAgent::new(ctx, config),
        }
    }
}

#[async_trait]
impl Agent for WikiAgent {
    fn role(&self) -> AgentRole {
        AgentRole::Wiki
    }

    fn name(&self) -> &str {
        "WikiAgent"
    }

    fn available_tools(&self) -> Vec<String> {
        vec![
            "read_file".to_string(),
            "write_file".to_string(),
            "edit_file".to_string(),
            "list_dir".to_string(),
            "grep_code".to_string(),
        ]
    }

    async fn run(&self, step: TaskStep, ctx: &TaskState) -> Result<String, AgentError> {
        self.inner.run(step, ctx).await
    }
}
