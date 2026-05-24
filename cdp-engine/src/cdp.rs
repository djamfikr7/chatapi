use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use tokio::net::UnixStream;
use tokio::sync::oneshot;
use tokio_tungstenite::{tungstenite::Message, MaybeTlsStream, WebSocketStream};
use tracing::{debug, error, info, warn};

// We can't easily do Unix socket + tungstenite directly, so we connect via ws:// to the
// debug WebSocket URL obtained from Chrome's HTTP endpoint. The actual Unix socket optimization
// is a TODO — for now we use WebSocket over localhost which Chrome exposes.

type WsStream = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;

/// Events emitted by the CDP engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum CdpEvent {
    /// A WebSocket frame was received by the browser (from the chat LLM server).
    WebSocketFrameReceived {
        request_id: String,
        timestamp: f64,
        response: WebSocketFrame,
    },
    /// A WebSocket frame was sent by the browser (to the chat LLM server).
    WebSocketFrameSent {
        request_id: String,
        timestamp: f64,
        response: WebSocketFrame,
    },
    /// Page navigation occurred.
    PageNavigated { url: String },
    /// DOM document is ready.
    DomReady,
    /// A CDP event we don't specifically handle.
    Unknown { method: String, params: Value },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSocketFrame {
    pub payload: String,
    #[serde(default)]
    pub is_binary: bool,
}

/// Raw CDP response to a command.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CdpResponse {
    id: u64,
    #[serde(default)]
    result: Value,
    #[serde(default)]
    error: Option<CdpError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CdpError {
    code: i32,
    message: String,
}

/// Raw CDP event message.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CdpEventMessage {
    method: String,
    params: Value,
}

/// Wrapper around raw CDP messages from the WebSocket.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum CdpMessage {
    Response(CdpResponse),
    Event(CdpEventMessage),
}

/// A connection to Chrome's DevTools Protocol over WebSocket.
pub struct CdpConnection {
    ws: WsStream,
    next_id: u64,
    /// Pending command responses keyed by command ID.
    pending: HashMap<u64, oneshot::Sender<Result<Value>>>,
}

impl CdpConnection {
    /// Connect to Chrome's DevTools WebSocket endpoint.
    /// `ws_url` is obtained from `get_debug_ws_url()` in the discovery module.
    pub async fn connect(ws_url: &str) -> Result<Self> {
        info!("Connecting to CDP at {}", ws_url);
        let (ws, _resp) = tokio_tungstenite::connect_async(ws_url)
            .await
            .context("Failed to connect to Chrome DevTools WebSocket")?;
        info!("CDP WebSocket connected");
        Ok(Self {
            ws,
            next_id: 1,
            pending: HashMap::new(),
        })
    }

    /// Send a CDP method call and wait for its response.
    pub async fn send_method(&mut self, method: &str, params: Value) -> Result<Value> {
        let id = self.next_id;
        self.next_id += 1;

        let msg = serde_json::json!({
            "id": id,
            "method": method,
            "params": params,
        });

        debug!("CDP send [{}]: {}", id, method);
        self.ws
            .send(Message::Text(msg.to_string().into()))
            .await
            .context("Failed to send CDP message")?;

        // Read messages until we get the response for our ID.
        loop {
            match self.ws.next().await {
                Some(Ok(Message::Text(text))) => {
                    let text_str: &str = &text;
                    if let Ok(cdp_msg) = serde_json::from_str::<CdpMessage>(text_str) {
                        match cdp_msg {
                            CdpMessage::Response(resp) if resp.id == id => {
                                if let Some(err) = resp.error {
                                    anyhow::bail!(
                                        "CDP error {}: {}",
                                        err.code,
                                        err.message
                                    );
                                }
                                return Ok(resp.result);
                            }
                            CdpMessage::Response(resp) => {
                                // Response for a different ID — deliver to pending handler if any.
                                if let Some(tx) = self.pending.remove(&resp.id) {
                                    if let Some(err) = resp.error {
                                        let _ = tx.send(Err(anyhow::anyhow!(
                                            "CDP error {}: {}",
                                            err.code,
                                            err.message
                                        )));
                                    } else {
                                        let _ = tx.send(Ok(resp.result));
                                    }
                                }
                            }
                            CdpMessage::Event(_evt) => {
                                // Events are consumed via next_event(); during a send_method
                                // we just ignore them here. The caller can poll next_event
                                // separately if needed.
                                debug!("Ignoring event during send_method wait");
                            }
                        }
                    }
                }
                Some(Ok(Message::Close(_))) => {
                    anyhow::bail!("CDP WebSocket closed unexpectedly");
                }
                Some(Err(e)) => {
                    anyhow::bail!("CDP WebSocket error: {}", e);
                }
                None => {
                    anyhow::bail!("CDP WebSocket stream ended");
                }
                _ => {
                    // Ping, Pong, Binary — skip
                }
            }
        }
    }

    /// Enable the Network domain and subscribe to WebSocket frame events.
    pub async fn subscribe_network_events(&mut self) -> Result<()> {
        info!("Enabling Network domain");
        self.send_method("Network.enable", serde_json::json!({}))
            .await?;

        info!("Enabling Page domain for navigation events");
        self.send_method("Page.enable", serde_json::json!({}))
            .await?;

        info!("Enabling DOM domain");
        self.send_method("DOM.enable", serde_json::json!({}))
            .await?;

        info!("Subscribed to CDP network and page events");
        Ok(())
    }

    /// Read the next CDP event from the WebSocket. This is meant to be called
    /// in an event loop. It skips command responses and only returns events.
    pub async fn next_event(&mut self) -> Result<CdpEvent> {
        loop {
            match self.ws.next().await {
                Some(Ok(Message::Text(text))) => {
                    let text_str: &str = &text;
                    if let Ok(cdp_msg) = serde_json::from_str::<CdpMessage>(text_str) {
                        match cdp_msg {
                            CdpMessage::Event(evt) => {
                                return Ok(parse_cdp_event(&evt.method, evt.params));
                            }
                            CdpMessage::Response(resp) => {
                                // Deliver to pending handler or log and skip.
                                if let Some(tx) = self.pending.remove(&resp.id) {
                                    if let Some(err) = resp.error {
                                        let _ = tx.send(Err(anyhow::anyhow!(
                                            "CDP error {}: {}",
                                            err.code,
                                            err.message
                                        )));
                                    } else {
                                        let _ = tx.send(Ok(resp.result));
                                    }
                                } else {
                                    debug!("Received response for unknown id {}", resp.id);
                                }
                                // Continue to next event.
                            }
                        }
                    }
                }
                Some(Ok(Message::Close(_))) => {
                    anyhow::bail!("CDP WebSocket closed");
                }
                Some(Err(e)) => {
                    anyhow::bail!("CDP WebSocket error: {}", e);
                }
                None => {
                    anyhow::bail!("CDP WebSocket stream ended");
                }
                _ => {
                    // Skip binary, ping, pong.
                }
            }
        }
    }
}

fn parse_cdp_event(method: &str, params: Value) -> CdpEvent {
    match method {
        "Network.webSocketFrameReceived" => {
            let request_id = params
                .get("requestId")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let timestamp = params.get("timestamp").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let response = params.get("response").cloned().unwrap_or(Value::Null);
            let payload = response
                .get("payloadData")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let is_binary = response
                .get("opcode")
                .and_then(|v| v.as_u64())
                .map(|o| o == 2) // opcode 2 = binary frame
                .unwrap_or(false);
            CdpEvent::WebSocketFrameReceived {
                request_id,
                timestamp,
                response: WebSocketFrame { payload, is_binary },
            }
        }
        "Network.webSocketFrameSent" => {
            let request_id = params
                .get("requestId")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let timestamp = params.get("timestamp").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let response = params.get("response").cloned().unwrap_or(Value::Null);
            let payload = response
                .get("payloadData")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let is_binary = response
                .get("opcode")
                .and_then(|v| v.as_u64())
                .map(|o| o == 2)
                .unwrap_or(false);
            CdpEvent::WebSocketFrameSent {
                request_id,
                timestamp,
                response: WebSocketFrame { payload, is_binary },
            }
        }
        "Page.loadEventFired" | "Page.frameNavigated" => {
            let url = params
                .get("frame")
                .and_then(|f| f.get("url"))
                .and_then(|v| v.as_str())
                .unwrap_or("about:blank")
                .to_string();
            CdpEvent::PageNavigated { url }
        }
        "DOM.documentUpdated" | "Page.domContentEventFired" => CdpEvent::DomReady,
        _ => CdpEvent::Unknown {
            method: method.to_string(),
            params,
        },
    }
}
