use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use chatapi_ringbuf::CommandSender;
pub struct Session {
    pub id: String,
    pub model: String,
    pub created: i64,
    pub status: SessionStatus,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SessionStatus {
    Idle,
    Processing,
    Streaming,
}

/// Application state shared across handlers.
#[derive(Clone)]
pub struct AppState {
    /// Active sessions keyed by session ID.
    pub sessions: Arc<Mutex<HashMap<String, Session>>>,
    /// Channel for sending commands to and receiving events from the CDP engine.
    pub cdp: Arc<Mutex<CommandSender>>,
    /// Whether the browser/CDP engine is connected.
    pub browser_connected: Arc<Mutex<bool>>,
}

impl AppState {
    pub fn new(cdp: CommandSender) -> Self {
        Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
            cdp: Arc::new(Mutex::new(cdp)),
            browser_connected: Arc::new(Mutex::new(false)),
        }
    }
}
