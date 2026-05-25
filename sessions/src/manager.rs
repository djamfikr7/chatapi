use chatapi_shared::ChatMessage;
use chatapi_shared::traits::{SessionStore, SessionSummary};
use uuid::Uuid;

use crate::models::{Session, SessionMetadata};

/// High-level session management.
pub struct SessionManager {
    store: Box<dyn SessionStore>,
    active: std::sync::Mutex<std::collections::HashMap<String, Session>>,
}

impl SessionManager {
    pub fn new(store: Box<dyn SessionStore>) -> Self {
        Self {
            store,
            active: std::sync::Mutex::new(std::collections::HashMap::new()),
        }
    }

    /// Create a new session.
    pub fn create(&self, model: &str) -> Session {
        let id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now();
        let session = Session {
            id: id.clone(),
            created_at: now,
            updated_at: now,
            messages: Vec::new(),
            metadata: SessionMetadata::new(model.to_string()),
        };
        self.active.lock().unwrap().insert(id, session.clone());
        session
    }

    /// Add a message to a session.
    pub fn add_message(&self, session_id: &str, message: ChatMessage) {
        if let Some(session) = self.active.lock().unwrap().get_mut(session_id) {
            session.messages.push(message);
            session.updated_at = chrono::Utc::now();
        }
    }

    /// Get a session by ID.
    pub fn get(&self, id: &str) -> Option<Session> {
        self.active.lock().unwrap().get(id).cloned()
    }

    /// List all active sessions.
    pub fn list(&self) -> Vec<SessionSummary> {
        self.active
            .lock()
            .unwrap()
            .values()
            .map(|s| SessionSummary {
                id: s.id.clone(),
                created_at: s.created_at,
                updated_at: s.updated_at,
                message_count: s.messages.len(),
                model: s.metadata.model.clone(),
            })
            .collect()
    }

    /// Delete a session.
    pub fn delete(&self, id: &str) -> bool {
        self.active.lock().unwrap().remove(id).is_some()
    }

    /// Branch a session — create a new session with messages up to a given index.
    /// Returns the new session ID.
    pub fn branch(&self, source_id: &str, at_message: Option<usize>) -> Option<Session> {
        let mut active = self.active.lock().unwrap();
        let source = active.get(source_id)?;

        let messages = match at_message {
            Some(idx) => source.messages[..std::cmp::min(idx, source.messages.len())].to_vec(),
            None => source.messages.clone(),
        };

        let id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now();
        let branched = Session {
            id: id.clone(),
            created_at: now,
            updated_at: now,
            messages,
            metadata: SessionMetadata::new(source.metadata.model.clone()),
        };
        active.insert(id, branched.clone());
        Some(branched)
    }
}
