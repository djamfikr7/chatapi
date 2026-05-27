use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::{broadcast, RwLock};
use tracing::{info, warn, error};

use crate::agent::{Agent, AgentConfig, AgentContext};
use crate::error::AgentError;
use crate::message::OrchestratorEvent;
use crate::state::SharedTaskState;
use crate::task::{AgentRole, Task, TaskStatus, TaskStep};
use crate::state::TaskState;

/// Central orchestrator that decomposes tasks and coordinates agents.
pub struct Orchestrator {
    /// LLM target + tools + working dir.
    ctx: Arc<AgentContext>,
    /// Registered agents by role.
    agents: RwLock<HashMap<AgentRole, Arc<dyn Agent>>>,
    /// Active tasks.
    tasks: RwLock<HashMap<String, Task>>,
    /// Event broadcaster for real-time monitoring.
    event_tx: broadcast::Sender<OrchestratorEvent>,
    /// Default agent config.
    config: AgentConfig,
}

impl Orchestrator {
    pub fn new(ctx: Arc<AgentContext>, config: AgentConfig) -> Self {
        let (event_tx, _) = broadcast::channel(256);
        Self {
            ctx,
            agents: RwLock::new(HashMap::new()),
            tasks: RwLock::new(HashMap::new()),
            event_tx,
            config,
        }
    }

    /// Register an agent for a specific role.
    pub async fn register_agent(&self, agent: Arc<dyn Agent>) {
        let role = agent.role();
        info!(role = %role, name = agent.name(), "Registering agent");
        self.agents.write().await.insert(role, agent);
    }

    /// Subscribe to orchestrator events.
    pub fn subscribe(&self) -> broadcast::Receiver<OrchestratorEvent> {
        self.event_tx.subscribe()
    }

    /// Submit a high-level task. Returns the task ID.
    pub async fn submit_task(self: &Arc<Self>, description: String) -> Result<String, AgentError> {
        let task = Task::new(description);
        let task_id = task.id.clone();

        info!(task_id = %task_id, description = %task.description, "Task submitted");
        self.tasks.write().await.insert(task_id.clone(), task);

        let _ = self.event_tx.send(OrchestratorEvent::TaskStarted {
            task_id: task_id.clone(),
            description: self.tasks.read().await.get(&task_id).map(|t| t.description.clone()).unwrap_or_default(),
        });

        // Spawn background execution
        let orch = Arc::clone(self);
        let tid = task_id.clone();
        tokio::spawn(async move {
            if let Err(e) = orch.execute_task(&tid).await {
                error!(task_id = %tid, error = %e, "Task failed");
                orch.mark_task_failed(&tid, &e.to_string()).await;
            }
        });

        Ok(task_id)
    }

    /// Get a snapshot of a task.
    pub async fn get_task(&self, task_id: &str) -> Option<Task> {
        self.tasks.read().await.get(task_id).cloned()
    }

    /// List all tasks.
    pub async fn list_tasks(&self) -> Vec<Task> {
        self.tasks.read().await.values().cloned().collect()
    }

    /// Cancel a running task.
    pub async fn cancel_task(&self, task_id: &str) -> Result<(), AgentError> {
        let mut tasks = self.tasks.write().await;
        let task = tasks
            .get_mut(task_id)
            .ok_or_else(|| AgentError::InvalidState(format!("Task {} not found", task_id)))?;
        task.status = TaskStatus::Cancelled;
        task.updated_at = chrono::Utc::now();
        info!(task_id = %task_id, "Task cancelled");
        Ok(())
    }

    /// List registered agent roles.
    pub async fn capabilities(&self) -> Vec<AgentRole> {
        self.agents.read().await.keys().cloned().collect()
    }

    // ── Internal execution ─────────────────────────────────────────

    async fn execute_task(self: &Arc<Self>, task_id: &str) -> Result<(), AgentError> {
        // Step 1: Plan — decompose the task description into steps
        let description = {
            let tasks = self.tasks.read().await;
            tasks.get(task_id).map(|t| t.description.clone())
                .ok_or_else(|| AgentError::InvalidState("Task not found".into()))?
        };

        let steps = self.plan_task(&description).await?;

        // Update task with planned steps
        {
            let mut tasks = self.tasks.write().await;
            if let Some(task) = tasks.get_mut(task_id) {
                task.steps = steps;
                task.status = TaskStatus::InProgress;
                task.updated_at = chrono::Utc::now();
            }
        }

        // Step 2: Execute steps sequentially
        let step_ids: Vec<String> = {
            let tasks = self.tasks.read().await;
            tasks.get(task_id)
                .map(|t| t.steps.iter().map(|s| s.id.clone()).collect())
                .unwrap_or_default()
        };

        let shared_ctx = SharedTaskState::new(TaskState::new());

        for step_id in &step_ids {
            // Check if task was cancelled
            {
                let tasks = self.tasks.read().await;
                if let Some(task) = tasks.get(task_id) {
                    if task.is_terminal() {
                        return Ok(());
                    }
                }
            }

            let step = {
                let tasks = self.tasks.read().await;
                tasks.get(task_id)
                    .and_then(|t| t.steps.iter().find(|s| s.id == *step_id).cloned())
                    .ok_or_else(|| AgentError::InvalidState("Step not found".into()))?
            };

            let _ = self.event_tx.send(OrchestratorEvent::StepStarted {
                task_id: task_id.to_string(),
                step_id: step.id.clone(),
                agent_role: step.assigned_to.clone(),
                description: step.description.clone(),
            });

            // Mark step as in progress
            self.update_step_status(task_id, &step.id, TaskStatus::InProgress).await;

            // Get the agent for this step's role
            let agent = {
                let agents = self.agents.read().await;
                agents.get(&step.assigned_to).cloned()
            };

            let result = match agent {
                Some(agent) => {
                    let ctx = shared_ctx.snapshot().await;
                    agent.run(step.clone(), &ctx).await
                }
                None => {
                    Err(AgentError::NoAgent(
                        format!("{:?}", step.assigned_to)
                    ))
                }
            };

            match result {
                Ok(output) => {
                    self.update_step_result(task_id, &step.id, Some(output.clone()), None).await;
                    self.update_step_status(task_id, &step.id, TaskStatus::Completed).await;
                    shared_ctx.set(
                        format!("step_{}", step.id),
                        serde_json::Value::String(output.clone()),
                    ).await;
                    let _ = self.event_tx.send(OrchestratorEvent::StepCompleted {
                        task_id: task_id.to_string(),
                        step_id: step.id.clone(),
                        result: output,
                    });
                }
                Err(e) => {
                    self.update_step_result(task_id, &step.id, None, Some(e.to_string())).await;
                    self.update_step_status(task_id, &step.id, TaskStatus::Failed).await;
                    let _ = self.event_tx.send(OrchestratorEvent::StepFailed {
                        task_id: task_id.to_string(),
                        step_id: step.id.clone(),
                        error: e.to_string(),
                    });
                    self.mark_task_failed(task_id, &e.to_string()).await;
                    return Err(e);
                }
            }
        }

        // All steps completed
        {
            let mut tasks = self.tasks.write().await;
            if let Some(task) = tasks.get_mut(task_id) {
                task.status = TaskStatus::Completed;
                task.updated_at = chrono::Utc::now();
            }
        }

        let _ = self.event_tx.send(OrchestratorEvent::TaskCompleted {
            task_id: task_id.to_string(),
            result: "All steps completed".to_string(),
        });

        info!(task_id = %task_id, "Task completed");
        Ok(())
    }

    /// Decompose a task description into concrete steps using the LLM.
    async fn plan_task(&self, description: &str) -> Result<Vec<TaskStep>, AgentError> {
        use chatapi_shared::{ChatCompletionRequest, ChatMessage, Role};

        let planning_prompt = format!(
            r#"You are a task planning agent. Given a high-level task description, decompose it into concrete, actionable steps.

For each step, specify:
- description: what needs to be done
- role: which agent should do it (coding, architecture, testing, debugging, github, wiki)

Respond with ONLY a JSON array of steps:
[
  {{"description": "...", "role": "architecture"}},
  {{"description": "...", "role": "coding"}},
  {{"description": "...", "role": "testing"}}
]

Available roles: coding, architecture, testing, debugging, github, wiki

Task: {}"#,
            description
        );

        let request = ChatCompletionRequest {
            model: String::new(),
            messages: vec![
                ChatMessage::new(Role::System, "You are a task planning agent. Respond with only valid JSON."),
                ChatMessage::new(Role::User, planning_prompt),
            ],
            stream: false,
            tools: None,
            temperature: 0.1,
            max_tokens: Some(2048),
            tool_choice: None,
            parallel_tool_calls: None,
        };

        let response = self.ctx.target.send_request(&request).await
            .map_err(|e| AgentError::PlanningFailed(e.to_string()))?;

        let content = response.choices.first()
            .and_then(|c| c.message.content.as_deref())
            .ok_or_else(|| AgentError::PlanningFailed("Empty LLM response".into()))?;

        // Parse JSON array from response (handle markdown code blocks)
        let json_str = content.trim();
        let json_str = if json_str.starts_with("```") {
            let inner = json_str.strip_prefix("```").unwrap_or(json_str);
            let inner = inner.strip_prefix("json\n").unwrap_or(inner);
            inner.trim_end_matches("```").trim().to_string()
        } else {
            json_str.to_string()
        };

        let parsed: Vec<serde_json::Value> = serde_json::from_str(&json_str)
            .map_err(|e| AgentError::PlanningFailed(format!("Failed to parse plan: {}", e)))?;

        let steps = parsed.into_iter().map(|v| {
            let desc = v["description"].as_str().unwrap_or("unnamed step").to_string();
            let role_str = v["role"].as_str().unwrap_or("coding");
            let role = match role_str {
                "architecture" => AgentRole::Architecture,
                "testing" => AgentRole::Testing,
                "debugging" => AgentRole::Debugging,
                "github" => AgentRole::GitHub,
                "wiki" => AgentRole::Wiki,
                _ => AgentRole::Coding,
            };
            TaskStep::new(desc, role)
        }).collect();

        Ok(steps)
    }

    async fn update_step_status(&self, task_id: &str, step_id: &str, status: TaskStatus) {
        let mut tasks = self.tasks.write().await;
        if let Some(task) = tasks.get_mut(task_id) {
            if let Some(step) = task.steps.iter_mut().find(|s| s.id == step_id) {
                step.status = status;
            }
            task.updated_at = chrono::Utc::now();
        }
    }

    async fn update_step_result(&self, task_id: &str, step_id: &str, result: Option<String>, error: Option<String>) {
        let mut tasks = self.tasks.write().await;
        if let Some(task) = tasks.get_mut(task_id) {
            if let Some(step) = task.steps.iter_mut().find(|s| s.id == step_id) {
                if let Some(r) = result {
                    step.result = Some(r);
                }
                if let Some(e) = error {
                    step.error = Some(e);
                }
            }
            task.updated_at = chrono::Utc::now();
        }
    }

    async fn mark_task_failed(&self, task_id: &str, error: &str) {
        let mut tasks = self.tasks.write().await;
        if let Some(task) = tasks.get_mut(task_id) {
            task.status = TaskStatus::Failed;
            task.updated_at = chrono::Utc::now();
        }
        let _ = self.event_tx.send(OrchestratorEvent::TaskFailed {
            task_id: task_id.to_string(),
            error: error.to_string(),
        });
    }
}
