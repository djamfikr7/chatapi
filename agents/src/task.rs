use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::state::TaskState;

/// High-level task submitted to the orchestrator.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub description: String,
    pub status: TaskStatus,
    pub steps: Vec<TaskStep>,
    pub context: TaskState,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Task {
    pub fn new(description: String) -> Self {
        let now = Utc::now();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            description,
            status: TaskStatus::Planning,
            steps: Vec::new(),
            context: TaskState::new(),
            created_at: now,
            updated_at: now,
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(
            self.status,
            TaskStatus::Completed | TaskStatus::Failed | TaskStatus::Cancelled
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TaskStatus {
    Planning,
    InProgress,
    Completed,
    Failed,
    Cancelled,
}

/// A single step within a task, assigned to a specific agent role.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskStep {
    pub id: String,
    pub description: String,
    pub assigned_to: AgentRole,
    pub status: TaskStatus,
    pub result: Option<String>,
    pub error: Option<String>,
}

impl TaskStep {
    pub fn new(description: String, assigned_to: AgentRole) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            description,
            assigned_to,
            status: TaskStatus::Planning,
            result: None,
            error: None,
        }
    }
}

/// The role an agent plays within the system.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum AgentRole {
    /// Writes and edits code
    Coding,
    /// Designs system structure, evaluates trade-offs
    Architecture,
    /// Writes and runs tests
    Testing,
    /// Investigates failures, proposes fixes
    Debugging,
    /// Manages PRs, issues, CI/CD via GitHub
    GitHub,
    /// Tracks progress, updates docs and knowledge base
    Wiki,
}

impl std::fmt::Display for AgentRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Coding => write!(f, "coding"),
            Self::Architecture => write!(f, "architecture"),
            Self::Testing => write!(f, "testing"),
            Self::Debugging => write!(f, "debugging"),
            Self::GitHub => write!(f, "github"),
            Self::Wiki => write!(f, "wiki"),
        }
    }
}
