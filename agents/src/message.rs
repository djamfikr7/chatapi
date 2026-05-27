use serde::{Deserialize, Serialize};

use crate::task::{AgentRole, TaskStatus};

/// Messages passed between agents and the orchestrator.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AgentMessage {
    /// Orchestrator assigns a step to an agent.
    StepAssigned {
        task_id: String,
        step_id: String,
        description: String,
    },

    /// Agent reports progress on a step.
    StepProgress {
        task_id: String,
        step_id: String,
        message: String,
    },

    /// Agent completed a step successfully.
    StepCompleted {
        task_id: String,
        step_id: String,
        result: String,
    },

    /// Agent failed a step.
    StepFailed {
        task_id: String,
        step_id: String,
        error: String,
    },

    /// Task status changed.
    TaskStatusChanged {
        task_id: String,
        status: TaskStatus,
    },
}

/// Events broadcast to WebSocket clients for real-time agent monitoring.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OrchestratorEvent {
    TaskStarted {
        task_id: String,
        description: String,
    },
    TaskCompleted {
        task_id: String,
        result: String,
    },
    TaskFailed {
        task_id: String,
        error: String,
    },
    StepStarted {
        task_id: String,
        step_id: String,
        agent_role: AgentRole,
        description: String,
    },
    StepCompleted {
        task_id: String,
        step_id: String,
        result: String,
    },
    StepFailed {
        task_id: String,
        step_id: String,
        error: String,
    },
}
