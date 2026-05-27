use async_trait::async_trait;
use std::sync::Arc;

use crate::agent::{Agent, AgentConfig, AgentContext};
use crate::agents::coding::CodingAgent;
use crate::error::AgentError;
use crate::state::TaskState;
use crate::task::{AgentRole, TaskStep};

/// Testing agent — writes and runs tests.
///
/// Has access to file operations and command execution to write test files
/// and run `cargo test` or similar commands.
pub struct TestingAgent {
    inner: CodingAgent,
}

impl TestingAgent {
    pub fn new(ctx: Arc<AgentContext>, mut config: AgentConfig) -> Self {
        config.system_prompt = "You are an expert testing agent. You write comprehensive, \
            correct tests for Rust code. You use the available tools to read source code, \
            write test files, and run `cargo test` to verify everything passes. \
            Focus on: unit tests, integration tests, edge cases, error paths, and \
            property-based testing where appropriate. Always run tests after writing them \
            and fix any failures."
            .to_string();
        config.tool_filter = vec![
            "read_file".into(), "write_file".into(), "edit_file".into(),
            "list_dir".into(), "run_command".into(), "grep_code".into(),
        ];
        Self {
            inner: CodingAgent::new(ctx, config),
        }
    }
}

#[async_trait]
impl Agent for TestingAgent {
    fn role(&self) -> AgentRole {
        AgentRole::Testing
    }

    fn name(&self) -> &str {
        "TestingAgent"
    }

    fn available_tools(&self) -> Vec<String> {
        vec![
            "read_file".to_string(),
            "write_file".to_string(),
            "edit_file".to_string(),
            "list_dir".to_string(),
            "run_command".to_string(),
            "grep_code".to_string(),
        ]
    }

    async fn run(&self, step: TaskStep, ctx: &TaskState) -> Result<String, AgentError> {
        self.inner.run(step, ctx).await
    }
}
