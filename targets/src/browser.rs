//! Browser target — implements TargetProvider by automating a real browser chat
//! window via Chrome DevTools Protocol (CDP).
//!
//! This bridges the CDP engine's ChatEngine to the TargetProvider trait, allowing
//! the gateway to use free web-based LLM chat windows (DeepSeek, ChatGPT, etc.)
//! as the backend instead of direct API calls.

use async_trait::async_trait;
use chatapi_cdp_engine::{ChatEngine, CdpConnection};
use chatapi_shared::traits::{TargetError, TargetProvider, TargetStream};
use chatapi_shared::{ChatCompletionRequest, ChatCompletionResponse, ChatMessage, Role};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{info, warn};

/// A TargetProvider that sends prompts to a browser-based LLM chat via CDP.
///
/// The browser must already be open with the chat page loaded. The ChatEngine
/// discovers the input textarea, types the prompt, and monitors WebSocket
/// frames for the streaming response.
pub struct BrowserTarget {
    engine: Arc<Mutex<ChatEngine>>,
    model: String,
}

impl BrowserTarget {
    /// Create a new BrowserTarget from an existing CDP connection.
    pub async fn new(cdp: CdpConnection, model: String) -> Result<Self, TargetError> {
        let mut engine = ChatEngine::new(cdp);
        engine.initialize().await.map_err(|e| {
            TargetError::ConnectionFailed(format!("ChatEngine init failed: {}", e))
        })?;

        info!("BrowserTarget initialized with model: {}", model);
        Ok(Self {
            engine: Arc::new(Mutex::new(engine)),
            model,
        })
    }

    /// Create from a Chrome debug port.
    pub async fn from_port(port: u16, model: String) -> Result<Self, TargetError> {
        let ws_url = chatapi_cdp_engine::get_debug_ws_url(port)
            .await
            .map_err(|e| TargetError::ConnectionFailed(format!("Failed to get WS URL: {}", e)))?;

        let cdp = CdpConnection::connect(&ws_url)
            .await
            .map_err(|e| TargetError::ConnectionFailed(format!("CDP connect failed: {}", e)))?;

        Self::new(cdp, model).await
    }

    /// Build the full prompt from the request messages.
    fn build_prompt(request: &ChatCompletionRequest) -> String {
        let mut parts = Vec::new();
        for msg in &request.messages {
            let role = match msg.role {
                Role::System => "System",
                Role::User => "User",
                Role::Assistant => "Assistant",
                Role::Tool => "Tool",
            };
            if let Some(content) = &msg.content {
                parts.push(format!("{}: {}", role, content));
            }
            if let Some(tool_calls) = &msg.tool_calls {
                for tc in tool_calls {
                    parts.push(format!(
                        "Assistant tool_call: {}({})",
                        tc.function.name, tc.function.arguments
                    ));
                }
            }
        }
        parts.join("\n\n")
    }
}

#[async_trait]
impl TargetProvider for BrowserTarget {
    fn name(&self) -> &str {
        "browser"
    }

    async fn health_check(&self) -> bool {
        // If we can lock the engine, the connection is alive.
        self.engine.try_lock().is_ok()
    }

    async fn send_request(
        &self,
        request: &ChatCompletionRequest,
    ) -> Result<ChatCompletionResponse, TargetError> {
        let prompt = Self::build_prompt(request);
        let mut engine = self.engine.lock().await;

        let response = engine.chat(&prompt).await.map_err(|e| {
            TargetError::RequestFailed(format!("Browser chat failed: {}", e))
        })?;

        Ok(ChatCompletionResponse::new(
            self.model.clone(),
            response.clone(),
            (prompt.len() as u32 + 3) / 4,
            (response.len() as u32 + 3) / 4,
        ))
    }

    async fn stream_request(
        &self,
        request: &ChatCompletionRequest,
    ) -> Result<TargetStream, TargetError> {
        let prompt = Self::build_prompt(request);
        let engine = self.engine.clone();

        // For streaming, we collect the full response and return it as a stream.
        // CDP engine's ChatEngine monitors WebSocket frames but returns the full
        // response at once. We chunk it into tokens for the streaming path.
        let response = {
            let mut eng = engine.lock().await;
            eng.chat(&prompt).await.map_err(|e| {
                TargetError::RequestFailed(format!("Browser chat failed: {}", e))
            })?
        };

        // Split into word-sized chunks for streaming feel
        let tokens: Vec<Result<String, TargetError>> = response
            .split_whitespace()
            .map(|w| Ok(format!("{} ", w)))
            .collect();

        Ok(Box::pin(futures_util::stream::iter(tokens)))
    }
}
