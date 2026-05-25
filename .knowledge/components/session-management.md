# Session Management Design

## Session Structure
```rust
pub struct Session {
    pub id: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub messages: Vec<ChatMessage>,
    pub branches: Vec<Branch>,
    pub metadata: SessionMetadata,
}

pub struct SessionMetadata {
    pub model: String,
    pub tools_used: Vec<String>,
    pub total_tokens: u64,
    pub total_tool_calls: u64,
    pub working_dir: PathBuf,
}
```

## API Endpoints
- GET /sessions - List all sessions
- POST /sessions - Create new session
- GET /sessions/:id - Get session with full history
- DELETE /sessions/:id - Delete session
- POST /sessions/:id/branch - Create branch from current state

## Persistence
- **In-memory**: Default, fast, lost on restart
- **SQLite**: Optional, configured via .chatapi/config.toml

## SessionStore Trait
```rust
#[async_trait]
pub trait SessionStore: Send + Sync {
    async fn save(&self, session: &Session) -> Result<()>;
    async fn load(&self, id: &str) -> Result<Option<Session>>;
    async fn list(&self) -> Result<Vec<SessionSummary>>;
    async fn delete(&self, id: &str) -> Result<()>;
}
```

## See Also
- [[full-platform-architecture]] - Platform architecture
