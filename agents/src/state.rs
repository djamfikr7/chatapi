use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

/// Shared key-value state for a task.
///
/// Sub-agents can read and write context entries to pass results
/// between steps. Thread-safe via Arc<RwLock>.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskState {
    entries: HashMap<String, serde_json::Value>,
}

impl TaskState {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    pub fn get(&self, key: &str) -> Option<&serde_json::Value> {
        self.entries.get(key)
    }

    pub fn set(&mut self, key: String, value: serde_json::Value) {
        self.entries.insert(key, value);
    }

    pub fn keys(&self) -> Vec<&String> {
        self.entries.keys().collect()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl Default for TaskState {
    fn default() -> Self {
        Self::new()
    }
}

/// Thread-safe wrapper around TaskState for concurrent agent access.
#[derive(Debug, Clone)]
pub struct SharedTaskState {
    inner: Arc<RwLock<TaskState>>,
}

impl SharedTaskState {
    pub fn new(state: TaskState) -> Self {
        Self {
            inner: Arc::new(RwLock::new(state)),
        }
    }

    pub async fn get(&self, key: &str) -> Option<serde_json::Value> {
        self.inner.read().await.get(key).cloned()
    }

    pub async fn set(&self, key: String, value: serde_json::Value) {
        self.inner.write().await.set(key, value);
    }

    pub async fn snapshot(&self) -> TaskState {
        self.inner.read().await.clone()
    }
}

impl Default for SharedTaskState {
    fn default() -> Self {
        Self::new(TaskState::new())
    }
}

use crate::task::Task;

/// Thread-safe store for active and recent tasks.
#[derive(Clone)]
pub struct TaskStore {
    tasks: Arc<RwLock<HashMap<String, Task>>>,
}

impl TaskStore {
    pub fn new() -> Self {
        Self {
            tasks: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Insert or update a task.
    pub async fn upsert(&self, task: Task) {
        let mut tasks = self.tasks.write().await;
        tasks.insert(task.id.clone(), task);
    }

    /// Get a task by ID.
    pub async fn get(&self, id: &str) -> Option<Task> {
        let tasks = self.tasks.read().await;
        tasks.get(id).cloned()
    }

    /// List all tasks, most recent first.
    pub async fn list(&self) -> Vec<Task> {
        let tasks = self.tasks.read().await;
        let mut list: Vec<Task> = tasks.values().cloned().collect();
        list.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        list
    }

    /// Remove a task by ID.
    pub async fn remove(&self, id: &str) -> Option<Task> {
        let mut tasks = self.tasks.write().await;
        tasks.remove(id)
    }
}

impl Default for TaskStore {
    fn default() -> Self {
        Self::new()
    }
}
