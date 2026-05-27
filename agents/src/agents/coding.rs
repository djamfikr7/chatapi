use async_trait::async_trait;
use std::sync::Arc;
use tracing::{debug, info, warn};

use chatapi_shared::{ChatCompletionRequest, ChatMessage, Role};
use chatapi_tools::ToolRegistry;

use crate::agent::{Agent, AgentConfig, AgentContext};
use crate::error::AgentError;
use crate::state::TaskState;
use crate::task::{AgentRole, TaskStep};

/// The main coding agent — writes and edits code using LLM + tools.
///
/// Implements a full agentic loop:
/// 1. Send task + context to LLM
/// 2. If LLM returns tool_calls, execute them
/// 3. Feed tool results back to LLM
/// 4. Repeat until LLM returns plain text or max turns reached
pub struct CodingAgent {
    ctx: Arc<AgentContext>,
    config: AgentConfig,
}

impl CodingAgent {
    pub fn new(ctx: Arc<AgentContext>, config: AgentConfig) -> Self {
        Self { ctx, config }
    }

    fn system_prompt(&self) -> String {
        if self.config.system_prompt.is_empty() {
            "You are an expert coding agent. You write clean, correct Rust code. \
             You use the available tools to read files, write code, run commands, \
             and verify your work. Always explain what you're doing briefly before \
             taking action. If you encounter errors, debug and fix them."
                .to_string()
        } else {
            self.config.system_prompt.clone()
        }
    }

    fn tool_schemas(&self) -> Vec<chatapi_shared::Tool> {
        let names = self.available_tools();
        self.ctx
            .tools
            .schemas()
            .into_iter()
            .filter(|s| names.contains(&s.function.name))
            .collect()
    }
}

#[async_trait]
impl Agent for CodingAgent {
    fn role(&self) -> AgentRole {
        AgentRole::Coding
    }

    fn name(&self) -> &str {
        "CodingAgent"
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
        let tools = self.tool_schemas();
        let system = self.system_prompt();

        // Build context from task state
        let context_str = if ctx.is_empty() {
            String::new()
        } else {
            let mut entries = Vec::new();
            for k in ctx.keys() {
                if let Some(v) = ctx.get(k) {
                    entries.push(format!("{}: {}", k, v));
                }
            }
            format!("\n\nTask context:\n{}", entries.join("\n"))
        };

        let user_msg = format!(
            "Complete this step:\n\n{}{}",
            step.description, context_str
        );

        let mut messages = vec![
            ChatMessage::new(Role::System, system),
            ChatMessage::new(Role::User, user_msg),
        ];

        let tool_ctx = chatapi_shared::traits::ToolContext {
            session_id: String::new(),
            working_dir: self.ctx.working_dir.clone(),
            env: std::collections::HashMap::new(),
        };

        // Agentic loop: LLM → tool calls → results → LLM
        for turn in 0..self.config.max_turns {
            debug!(turn = turn, step = %step.id, "Coding agent LLM turn");

            let request = ChatCompletionRequest {
                model: String::new(), // uses target's default
                messages: messages.clone(),
                stream: false,
                tools: if tools.is_empty() {
                    None
                } else {
                    Some(tools.clone())
                },
                temperature: 0.2,
                max_tokens: Some(4096),
                tool_choice: None,
                parallel_tool_calls: None,
            };

            let response = self
                .ctx
                .target
                .send_request(&request)
                .await
                .map_err(|e| AgentError::LlmFailed(e.to_string()))?;

            let choice = response
                .choices
                .first()
                .ok_or_else(|| AgentError::LlmFailed("No choices in response".into()))?;

            // Check if LLM wants to call tools
            if let Some(tool_calls) = &choice.message.tool_calls {
                if !tool_calls.is_empty() {
                    // Add assistant message with tool calls
                    messages.push(ChatMessage::with_tool_calls(tool_calls.clone()));

                    // Execute each tool call
                    for tc in tool_calls {
                        let args: serde_json::Value =
                            serde_json::from_str(&tc.function.arguments).unwrap_or(
                                serde_json::Value::Object(serde_json::Map::new()),
                            );

                        let result =
                            self.ctx.tools.execute(&tc.function.name, args, &tool_ctx).await;
                        let result_text = match result {
                            Ok(r) => match r {
                                chatapi_shared::traits::ToolResult::Text(t) => t,
                                chatapi_shared::traits::ToolResult::Diff {
                                    old,
                                    new,
                                    path,
                                } => {
                                    format!(
                                        "Diff for {}:\n--- old\n{}\n+++ new\n{}",
                                        path.display(),
                                        old,
                                        new
                                    )
                                }
                                chatapi_shared::traits::ToolResult::Error { message, .. } => {
                                    format!("Error: {}", message)
                                }
                            },
                            Err(e) => format!("Tool execution error: {}", e),
                        };

                        debug!(
                            tool = %tc.function.name,
                            result_len = result_text.len(),
                            "Tool executed"
                        );

                        // Add tool result message
                        messages.push(ChatMessage::tool_result(&tc.id, result_text));
                    }

                    continue; // Next LLM turn
                }
            }

            // LLM returned plain text — this is the final answer
            let content = choice
                .message
                .content
                .clone()
                .unwrap_or_else(|| "No response from LLM".to_string());

            info!(step = %step.id, turn = turn, "Coding agent completed step");
            return Ok(content);
        }

        warn!(step = %step.id, "Coding agent hit max turns");
        Err(AgentError::Timeout(self.config.step_timeout_secs))
    }
}
