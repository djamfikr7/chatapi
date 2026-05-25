use async_trait::async_trait;
use chatapi_shared::traits::{SessionError, SessionStore, SessionSummary};
use chatapi_shared::ChatMessage;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::sync::RwLock;

// ── In-memory store ──────────────────────────────────────────────────

/// In-memory session store.
pub struct MemoryStore {
    sessions: RwLock<HashMap<String, (Vec<ChatMessage>, String, chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>)>>,
}

impl MemoryStore {
    pub fn new() -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
        }
    }
}

#[async_trait]
impl SessionStore for MemoryStore {
    async fn save(&self, message: &ChatMessage, id: &str) -> Result<(), SessionError> {
        let mut sessions = self.sessions.write().await;
        let now = chrono::Utc::now();
        if let Some(entry) = sessions.get_mut(id) {
            entry.0.push(message.clone());
            entry.3 = now;
        } else {
            sessions.insert(
                id.to_string(),
                (vec![message.clone()], "unknown".to_string(), now, now),
            );
        }
        Ok(())
    }

    async fn load(&self, id: &str) -> Result<Option<Vec<ChatMessage>>, SessionError> {
        let sessions = self.sessions.read().await;
        Ok(sessions.get(id).map(|(msgs, _, _, _)| msgs.clone()))
    }

    async fn list(&self) -> Result<Vec<SessionSummary>, SessionError> {
        let sessions = self.sessions.read().await;
        let summaries = sessions
            .iter()
            .map(|(id, (msgs, model, created, updated))| SessionSummary {
                id: id.clone(),
                created_at: *created,
                updated_at: *updated,
                message_count: msgs.len(),
                model: model.clone(),
            })
            .collect();
        Ok(summaries)
    }

    async fn delete(&self, id: &str) -> Result<(), SessionError> {
        let mut sessions = self.sessions.write().await;
        sessions.remove(id);
        Ok(())
    }
}

// ── File-backed store ─────────────────────────────────────────────────

/// Persisted session store — saves each session as a JSON file on disk.
pub struct FileStore {
    dir: PathBuf,
    index: RwLock<HashMap<String, SessionMeta>>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct SessionMeta {
    model: String,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
    message_count: usize,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct SessionFile {
    messages: Vec<ChatMessage>,
    model: String,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

impl FileStore {
    pub fn new(dir: impl AsRef<Path>) -> Self {
        let dir = dir.as_ref().to_path_buf();
        Self {
            dir,
            index: RwLock::new(HashMap::new()),
        }
    }

    /// Scan the directory and build the in-memory index.
    pub async fn init(&self) -> Result<(), SessionError> {
        if !self.dir.exists() {
            tokio::fs::create_dir_all(&self.dir).await
                .map_err(|e| SessionError::StoreError(e.to_string()))?;
            return Ok(());
        }

        let mut entries = tokio::fs::read_dir(&self.dir).await
            .map_err(|e| SessionError::StoreError(e.to_string()))?;

        let mut index = HashMap::new();
        while let Some(entry) = entries.next_entry().await
            .map_err(|e| SessionError::StoreError(e.to_string()))?
        {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let id = path.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string();
            if id.is_empty() { continue; }

            match tokio::fs::read_to_string(&path).await {
                Ok(data) => {
                    if let Ok(sf) = serde_json::from_str::<SessionFile>(&data) {
                        index.insert(id, SessionMeta {
                            model: sf.model,
                            created_at: sf.created_at,
                            updated_at: sf.updated_at,
                            message_count: sf.messages.len(),
                        });
                    }
                }
                Err(_) => continue,
            }
        }

        *self.index.write().await = index;
        Ok(())
    }

    fn session_path(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{}.json", id))
    }

    async fn read_session_file(&self, id: &str) -> Result<Option<SessionFile>, SessionError> {
        let path = self.session_path(id);
        if !path.exists() {
            return Ok(None);
        }
        let data = tokio::fs::read_to_string(&path).await
            .map_err(|e| SessionError::StoreError(e.to_string()))?;
        let sf: SessionFile = serde_json::from_str(&data)
            .map_err(|e| SessionError::StoreError(e.to_string()))?;
        Ok(Some(sf))
    }

    async fn write_session_file(&self, id: &str, sf: &SessionFile) -> Result<(), SessionError> {
        let path = self.session_path(id);
        let data = serde_json::to_string_pretty(sf)
            .map_err(|e| SessionError::StoreError(e.to_string()))?;
        tokio::fs::write(&path, data).await
            .map_err(|e| SessionError::StoreError(e.to_string()))?;
        Ok(())
    }
}

#[async_trait]
impl SessionStore for FileStore {
    async fn save(&self, message: &ChatMessage, id: &str) -> Result<(), SessionError> {
        let now = chrono::Utc::now();
        let mut sf = self.read_session_file(id).await?.unwrap_or_else(|| SessionFile {
            messages: Vec::new(),
            model: "unknown".to_string(),
            created_at: now,
            updated_at: now,
        });

        sf.messages.push(message.clone());
        sf.updated_at = now;
        self.write_session_file(id, &sf).await?;

        // Update index
        let mut index = self.index.write().await;
        let meta = index.entry(id.to_string()).or_insert_with(|| SessionMeta {
            model: sf.model.clone(),
            created_at: sf.created_at,
            updated_at: now,
            message_count: 0,
        });
        meta.message_count = sf.messages.len();
        meta.updated_at = now;

        Ok(())
    }

    async fn load(&self, id: &str) -> Result<Option<Vec<ChatMessage>>, SessionError> {
        Ok(self.read_session_file(id).await?.map(|sf| sf.messages))
    }

    async fn list(&self) -> Result<Vec<SessionSummary>, SessionError> {
        let index = self.index.read().await;
        let summaries = index
            .iter()
            .map(|(id, meta)| SessionSummary {
                id: id.clone(),
                created_at: meta.created_at,
                updated_at: meta.updated_at,
                message_count: meta.message_count,
                model: meta.model.clone(),
            })
            .collect();
        Ok(summaries)
    }

    async fn delete(&self, id: &str) -> Result<(), SessionError> {
        let path = self.session_path(id);
        if path.exists() {
            tokio::fs::remove_file(&path).await
                .map_err(|e| SessionError::StoreError(e.to_string()))?;
        }
        self.index.write().await.remove(id);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chatapi_shared::{ChatMessage, Role};

    #[tokio::test]
    async fn test_file_store_save_load_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let store = FileStore::new(dir.path());
        store.init().await.unwrap();

        let msg = ChatMessage::new(Role::User, "hello");
        store.save(&msg, "session-1").await.unwrap();

        let loaded = store.load("session-1").await.unwrap().unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].content.as_deref(), Some("hello"));
    }

    #[tokio::test]
    async fn test_file_store_list() {
        let dir = tempfile::tempdir().unwrap();
        let store = FileStore::new(dir.path());
        store.init().await.unwrap();

        store.save(&ChatMessage::new(Role::User, "a"), "s1").await.unwrap();
        store.save(&ChatMessage::new(Role::User, "b"), "s2").await.unwrap();

        let list = store.list().await.unwrap();
        assert_eq!(list.len(), 2);
    }

    #[tokio::test]
    async fn test_file_store_delete() {
        let dir = tempfile::tempdir().unwrap();
        let store = FileStore::new(dir.path());
        store.init().await.unwrap();

        store.save(&ChatMessage::new(Role::User, "x"), "s1").await.unwrap();
        assert!(store.load("s1").await.unwrap().is_some());

        store.delete("s1").await.unwrap();
        assert!(store.load("s1").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_file_store_persistence() {
        let dir = tempfile::tempdir().unwrap();
        let dir_path = dir.path().to_path_buf();

        // Write
        {
            let store = FileStore::new(&dir_path);
            store.init().await.unwrap();
            store.save(&ChatMessage::new(Role::User, "persisted"), "s1").await.unwrap();
        }

        // Re-read from same directory
        {
            let store = FileStore::new(&dir_path);
            store.init().await.unwrap();
            let loaded = store.load("s1").await.unwrap().unwrap();
            assert_eq!(loaded[0].content.as_deref(), Some("persisted"));
            assert_eq!(store.list().await.unwrap().len(), 1);
        }
    }

    #[tokio::test]
    async fn test_file_store_init_creates_dir() {
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join("new_sessions");
        let store = FileStore::new(&sub);
        store.init().await.unwrap();
        assert!(sub.exists());
    }

    #[tokio::test]
    async fn test_file_store_load_nonexistent() {
        let dir = tempfile::tempdir().unwrap();
        let store = FileStore::new(dir.path());
        store.init().await.unwrap();
        assert!(store.load("nope").await.unwrap().is_none());
    }
}
