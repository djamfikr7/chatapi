//! WebSocket handler for real-time frontend updates.
//!
//! The frontend connects via WS to receive streaming chat responses,
//! tool execution events, and session updates in real time.

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::IntoResponse,
};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use tracing::{info, warn};

use crate::state::AppState;

/// Events pushed to connected WebSocket clients.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum WsEvent {
    /// Streaming token from the LLM.
    #[serde(rename = "token")]
    Token {
        session_id: String,
        content: String,
    },
    /// LLM response complete.
    #[serde(rename = "response_done")]
    ResponseDone {
        session_id: String,
        response: String,
    },
    /// Tool call requested by the LLM.
    #[serde(rename = "tool_call")]
    ToolCall {
        session_id: String,
        tool_name: String,
        arguments: String,
    },
    /// Tool execution result.
    #[serde(rename = "tool_result")]
    ToolResult {
        session_id: String,
        tool_name: String,
        result: String,
        is_error: bool,
    },
    /// Session created or deleted.
    #[serde(rename = "session_event")]
    SessionEvent {
        session_id: String,
        action: String,
    },
}

/// Broadcast channel for sending events to all connected clients.
#[derive(Clone)]
pub struct EventBroadcaster {
    tx: broadcast::Sender<WsEvent>,
}

impl EventBroadcaster {
    pub fn new(capacity: usize) -> Self {
        let (tx, _) = broadcast::channel(capacity);
        Self { tx }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<WsEvent> {
        self.tx.subscribe()
    }

    pub fn send(&self, event: WsEvent) {
        let _ = self.tx.send(event);
    }
}

/// GET /ws — WebSocket upgrade handler.
pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_ws(socket, state))
}

async fn handle_ws(socket: WebSocket, state: AppState) {
    let (mut sender, mut receiver) = socket.split();
    let mut rx = state.events.subscribe();

    info!("WebSocket client connected");

    // Forward broadcast events to the client
    let mut send_task = tokio::spawn(async move {
        while let Ok(event) = rx.recv().await {
            match serde_json::to_string(&event) {
                Ok(json) => {
                    if sender.send(Message::Text(json.into())).await.is_err() {
                        break;
                    }
                }
                Err(e) => {
                    warn!("Failed to serialize WS event: {}", e);
                }
            }
        }
    });

    // Handle incoming messages from client (currently just keepalive)
    let mut recv_task = tokio::spawn(async move {
        while let Some(msg) = receiver.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    let text_str: &str = &text;
                    info!("WS received: {}", text_str);
                    // Future: handle client commands (e.g., cancel, approve tool)
                }
                Ok(Message::Close(_)) => break,
                Err(e) => {
                    warn!("WS error: {}", e);
                    break;
                }
                _ => {}
            }
        }
    });

    // Wait for either task to finish
    tokio::select! {
        _ = &mut send_task => recv_task.abort(),
        _ = &mut recv_task => send_task.abort(),
    }

    info!("WebSocket client disconnected");
}

/// GET /ws/terminal — WebSocket for interactive terminal.
pub async fn terminal_handler(
    ws: WebSocketUpgrade,
    State(_state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_terminal(socket))
}

async fn handle_terminal(socket: WebSocket) {
    use tokio::process::Command;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    info!("Terminal WebSocket connected");

    let (mut sender, mut receiver) = socket.split();

    // Spawn shell
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());
    let mut child = match Command::new(&shell)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .env("TERM", "xterm-256color")
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            warn!("Failed to spawn shell: {}", e);
            return;
        }
    };

    let mut stdin = child.stdin.take().expect("stdin piped");
    let mut stdout = child.stdout.take().expect("stdout piped");
    let mut stderr = child.stderr.take().expect("stderr piped");

    // Read from stdout/stderr and send to WS client
    let mut send_task = tokio::spawn(async move {
        let mut stdout_buf = [0u8; 4096];
        let mut stderr_buf = [0u8; 4096];
        loop {
            tokio::select! {
                n = stdout.read(&mut stdout_buf) => {
                    match n {
                        Ok(0) => break,
                        Ok(n) => {
                            if sender.send(Message::Binary(stdout_buf[..n].to_vec().into())).await.is_err() {
                                break;
                            }
                        }
                        Err(_) => break,
                    }
                }
                n = stderr.read(&mut stderr_buf) => {
                    match n {
                        Ok(0) => {},
                        Ok(n) => {
                            if sender.send(Message::Binary(stderr_buf[..n].to_vec().into())).await.is_err() {
                                break;
                            }
                        }
                        Err(_) => break,
                    }
                }
            }
        }
    });

    // Read from WS client and write to stdin
    let mut recv_task = tokio::spawn(async move {
        while let Some(msg) = receiver.next().await {
            match msg {
                Ok(Message::Binary(data)) => {
                    if stdin.write_all(&data).await.is_err() {
                        break;
                    }
                    let _ = stdin.flush().await;
                }
                Ok(Message::Text(text)) => {
                    let text_str: &str = &text;
                    if stdin.write_all(text_str.as_bytes()).await.is_err() {
                        break;
                    }
                    let _ = stdin.flush().await;
                }
                Ok(Message::Close(_)) => break,
                Err(_) => break,
                _ => {}
            }
        }
    });

    // Wait for either task to finish
    tokio::select! {
        _ = &mut send_task => recv_task.abort(),
        _ = &mut recv_task => send_task.abort(),
    }

    // Kill shell process
    let _ = child.kill().await;
    info!("Terminal WebSocket disconnected");
}
