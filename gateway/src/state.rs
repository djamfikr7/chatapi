use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use chatapi_ringbuf::{MessageProducer, MessageConsumer};

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
pub struct AppState {
    /// Active sessions keyed by session ID.
    pub sessions: Arc<Mutex<HashMap<String, Session>>>,
    /// Ring buffer producer for sending commands to CDP engine.
    pub producer: Arc<Mutex<MessageProducer>>,
    /// Ring buffer consumer for receiving responses from CDP engine.
    pub consumer: Arc<Mutex<MessageConsumer>>,
    /// Whether the browser/CDP engine is connected.
    pub browser_connected: Arc<Mutex<bool>>,
}

impl AppState {
    pub fn new(producer: MessageProducer, consumer: MessageConsumer) -> Self {
        Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
            producer: Arc::new(Mutex::new(producer)),
            consumer: Arc::new(Mutex::new(consumer)),
            browser_connected: Arc::new(Mutex::new(false)),
        }
    }
}

impl Clone for AppState {
    fn clone(&self) -> Self {
        Self {
            sessions: Arc::clone(&self.sessions),
            producer: Arc::clone(&self.producer),
            consumer: Arc::clone(&self.consumer),
            browser_connected: Arc::clone(&self.browser_connected),
        }
    }
}
