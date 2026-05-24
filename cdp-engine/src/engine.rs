use anyhow::{Context, Result};
use serde_json::Value;
use std::time::Duration;
use tokio::time::{sleep, timeout};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use crate::cdp::{CdpConnection, CdpEvent, WebSocketFrame};

/// Response completion detection: if no new WebSocket frames arrive within this
/// duration, we consider the LLM response finished.
const SILENCE_TIMEOUT: Duration = Duration::from_millis(500);

/// Maximum time to wait for any response to start arriving.
const RESPONSE_START_TIMEOUT: Duration = Duration::Duration::from_secs(30);

/// Humanized typing jitter range in milliseconds.
const JITTER_MIN_MS: u64 = 2;
const JITTER_MAX_MS: u64 = 7;

/// High-level engine that automates an LLM chat session via CDP.
pub struct ChatEngine {
    cdp: CdpConnection,
    /// Session identifier for request tracking.
    session_id: String,
    /// Cached root DOM node ID.
    root_node_id: Option<i64>,
    /// Cached textarea node ID.
    textarea_node_id: Option<i64>,
}

impl ChatEngine {
    /// Create a new ChatEngine wrapping an existing CDP connection.
    pub fn new(cdp: CdpConnection) -> Self {
        Self {
            cdp,
            session_id: Uuid::new_v4().to_string(),
            root_node_id: None,
            textarea_node_id: None,
        }
    }

    /// Initialize: subscribe to network/page events and cache the DOM tree.
    pub async fn initialize(&mut self) -> Result<()> {
        info!("Initializing ChatEngine for session {}", self.session_id);
        self.cdp.subscribe_network_events().await?;
        self.refresh_dom_cache().await?;
        info!("ChatEngine initialized");
        Ok(())
    }

    /// Refresh the cached DOM node IDs (call after navigation).
    pub async fn refresh_dom_cache(&mut self) -> Result<()> {
        let doc = self
            .cdp
            .send_method("DOM.getDocument", serde_json::json!({"depth": 1}))
            .await?;

        if let Some(root) = doc.get("root") {
            self.root_node_id = root.get("nodeId").and_then(|v| v.as_i64());
            debug!("Cached root node ID: {:?}", self.root_node_id);
        }

        // TODO: DeepSeek-specific textarea selector discovery.
        // For now, try common selectors. This needs runtime adaptation.
        self.textarea_node_id = None;
        self.discover_textarea().await?;

        Ok(())
    }

    /// Attempt to find the chat input textarea in the current page.
    async fn discover_textarea(&mut self) -> Result<()> {
        let root_id = self.root_node_id.context("No root node cached")?;

        // Try to find textarea via querySelector.
        // DeepSeek uses a contenteditable div or textarea. Try both.
        let selectors = [
            "textarea",
            "[contenteditable='true']",
            "#chat-input",
            "[data-testid='chat-input']",
            ".chat-input",
        ];

        for selector in &selectors {
            match self
                .cdp
                .send_method(
                    "DOM.querySelector",
                    serde_json::json!({
                        "nodeId": root_id,
                        "selector": selector,
                    }),
                )
                .await
            {
                Ok(result) => {
                    if let Some(node_id) = result.get("nodeId").and_then(|v| v.as_i64()) {
                        if node_id != 0 {
                            // nodeId 0 means "not found"
                            self.textarea_node_id = Some(node_id);
                            info!(
                                "Found chat input via selector '{}', nodeId={}",
                                selector, node_id
                            );
                            return Ok(());
                        }
                    }
                }
                Err(e) => {
                    debug!("Selector '{}' failed: {}", selector, e);
                }
            }
        }

        warn!("Could not find chat input textarea; prompt injection may fail");
        Ok(())
    }

    /// Send a prompt to the chat: type it character by character with jitter, then press Enter.
    pub async fn send_prompt(&mut self, prompt: &str) -> Result<()> {
        info!("Sending prompt ({} chars)", prompt.len());

        // Focus the textarea first.
        if let Some(node_id) = self.textarea_node_id {
            self.cdp
                .send_method(
                    "DOM.focus",
                    serde_json::json!({"nodeId": node_id}),
                )
                .await
                .ok(); // Some pages don't support DOM.focus; fall through to click.
        }

        // If we couldn't focus, try clicking on the textarea area.
        if self.textarea_node_id.is_some() {
            // TODO: Use Input.dispatchMouseEvent to click the textarea center
            // based on getBoxModel result. For now, rely on DOM.focus.
        }

        // Type each character with humanized jitter.
        self.inject_prompt_with_jitter(prompt).await?;

        // Press Enter to submit.
        self.press_enter().await?;

        info!("Prompt submitted");
        Ok(())
    }

    /// Type text into the focused element character by character with ±5ms jitter.
    pub async fn inject_prompt_with_jitter(&mut self, text: &str) -> Result<()> {
        for ch in text.chars() {
            let key_str = ch.to_string();

            // keyDown
            self.cdp
                .send_method(
                    "Input.dispatchKeyEvent",
                    serde_json::json!({
                        "type": "keyDown",
                        "text": key_str,
                        "key": key_str,
                        "code": char_to_code(ch),
                        "unmodifiedText": key_str,
                    }),
                )
                .await?;

            // keyUp
            self.cdp
                .send_method(
                    "Input.dispatchKeyEvent",
                    serde_json::json!({
                        "type": "keyUp",
                        "text": key_str,
                        "key": key_str,
                        "code": char_to_code(ch),
                        "unmodifiedText": key_str,
                    }),
                )
                .await?;

            // Humanized jitter: 2-7ms between keystrokes
            let jitter = JITTER_MIN_MS
                + (rand::random::<u64>() % (JITTER_MAX_MS - JITTER_MIN_MS + 1));
            sleep(Duration::from_millis(jitter)).await;
        }
        Ok(())
    }

    /// Simulate pressing Enter to submit the message.
    async fn press_enter(&mut self) -> Result<()> {
        // Enter keyDown
        self.cdp
            .send_method(
                "Input.dispatchKeyEvent",
                serde_json::json!({
                    "type": "keyDown",
                    "key": "Enter",
                    "code": "Enter",
                    "text": "\r",
                    "unmodifiedText": "\r",
                    "windowsVirtualKeyCode": 13,
                    "nativeVirtualKeyCode": 13,
                }),
            )
            .await?;

        // Enter keyUp
        self.cdp
            .send_method(
                "Input.dispatchKeyEvent",
                serde_json::json!({
                    "type": "keyUp",
                    "key": "Enter",
                    "code": "Enter",
                    "windowsVirtualKeyCode": 13,
                    "nativeVirtualKeyCode": 13,
                }),
            )
            .await?;

        Ok(())
    }

    /// Wait for the LLM streaming response to complete.
    ///
    /// Monitors `Network.webSocketFrameReceived` events for chat response frames,
    /// accumulates the content tokens, and detects completion via silence timeout.
    pub async fn wait_for_response(&mut self) -> Result<String> {
        info!("Waiting for streaming response...");
        let mut response_text = String::new();
        let mut got_first_frame = false;

        // First, wait for the response to start (longer timeout).
        let start_deadline = tokio::time::Instant::now() + RESPONSE_START_TIMEOUT;

        loop {
            let remaining = if !got_first_frame {
                let r = start_deadline
                    .checked_duration_since(tokio::time::Instant::now())
                    .unwrap_or(Duration::ZERO);
                if r.is_zero() {
                    anyhow::bail!("Timed out waiting for response to start");
                }
                r
            } else {
                SILENCE_TIMEOUT
            };

            match timeout(remaining, self.cdp.next_event()).await {
                Ok(Ok(event)) => match event {
                    CdpEvent::WebSocketFrameReceived { response, .. } => {
                        if !got_first_frame {
                            info!("First response frame received");
                            got_first_frame = true;
                        }
                        if let Some(token) = parse_chat_token(&response) {
                            debug!("Token: '{}'", token);
                            response_text.push_str(&token);
                        }
                    }
                    CdpEvent::DomReady => {
                        debug!("DOM ready event during response wait");
                        // Page refreshed mid-response — bad sign.
                        if got_first_frame {
                            warn!("DOM refreshed during response — partial response only");
                            break;
                        }
                    }
                    _ => {
                        // Other events — continue.
                    }
                },
                Ok(Err(e)) => {
                    error!("CDP error while waiting for response: {}", e);
                    return Err(e);
                }
                Err(_timeout) => {
                    // Silence timeout reached.
                    if got_first_frame {
                        info!(
                            "Response complete (silence timeout). {} chars received.",
                            response_text.len()
                        );
                        break;
                    }
                    // Not started yet — continue waiting.
                }
            }
        }

        Ok(response_text)
    }

    /// Send a prompt and wait for the full response.
    pub async fn chat(&mut self, prompt: &str) -> Result<String> {
        self.send_prompt(prompt).await?;
        self.wait_for_response().await
    }

    /// Get a reference to the underlying CDP connection.
    pub fn cdp(&self) -> &CdpConnection {
        &self.cdp
    }

    /// Get a mutable reference to the underlying CDP connection.
    pub fn cdp_mut(&mut self) -> &mut CdpConnection {
        &mut self.cdp
    }
}

/// Parse a chat response token from a WebSocket frame payload.
///
/// DeepSeek and similar chat UIs stream JSON over WebSocket. The exact format
/// depends on the target. Common patterns:
///
/// DeepSeek format (server-sent events within WebSocket):
///   `data: {"choices":[{"delta":{"content":"Hello"}}]}`
///
/// Generic JSON:
///   `{"content": "Hello", "done": false}`
///
/// This function extracts the incremental text content from the frame.
fn parse_chat_token(frame: &WebSocketFrame) -> Option<String> {
    if frame.is_binary {
        return None;
    }

    let payload = &frame.payload;

    // DeepSeek streams SSE-style data within WebSocket frames.
    // Format: "data: {json}\n\n" or "data: [DONE]\n\n"
    if payload.starts_with("data: ") {
        for line in payload.lines() {
            let line = line.trim();
            if line == "data: [DONE]" {
                // Stream complete signal — no text to extract.
                return None;
            }
            if let Some(json_str) = line.strip_prefix("data: ") {
                if let Ok(val) = serde_json::from_str::<Value>(json_str) {
                    // Try OpenAI-style: choices[0].delta.content
                    if let Some(content) = val
                        .get("choices")
                        .and_then(|c| c.as_array())
                        .and_then(|arr| arr.first())
                        .and_then(|c| c.get("delta"))
                        .and_then(|d| d.get("content"))
                        .and_then(|c| c.as_str())
                    {
                        if !content.is_empty() {
                            return Some(content.to_string());
                        }
                    }
                    // Try generic: "content" field
                    if let Some(content) = val.get("content").and_then(|c| c.as_str()) {
                        return Some(content.to_string());
                    }
                }
            }
        }
        return None;
    }

    // Try plain JSON.
    if let Ok(val) = serde_json::from_str::<Value>(payload) {
        // OpenAI-style.
        if let Some(content) = val
            .get("choices")
            .and_then(|c| c.as_array())
            .and_then(|arr| arr.first())
            .and_then(|c| c.get("delta"))
            .and_then(|d| d.get("content"))
            .and_then(|c| c.as_str())
        {
            return Some(content.to_string());
        }
        // Generic content field.
        if let Some(content) = val.get("content").and_then(|c| c.as_str()) {
            return Some(content.to_string());
        }
        // Some UIs stream raw text tokens as JSON strings.
        if let Some(text) = val.as_str() {
            return Some(text.to_string());
        }
    }

    // Fallback: if the frame looks like plain text (not JSON), treat it as a token.
    // This handles simple WebSocket text frames.
    if !payload.is_empty() && !payload.starts_with('{') && !payload.starts_with('[') {
        return Some(payload.clone());
    }

    None
}

/// Map a character to a plausible `code` value for CDP key events.
fn char_to_code(ch: char) -> &'static str {
    match ch {
        ' ' => "Space",
        '\n' | '\r' => "Enter",
        '\t' => "Tab",
        'a'..='z' => "KeyA", // Simplified — real impl would map a→KeyA, b→KeyB, etc.
        'A'..='Z' => "KeyA",
        '0'..='9' => "Digit0",
        '.' => "Period",
        ',' => "Comma",
        '!' => "Digit1",
        '?' => "Slash",
        '-' => "Minus",
        '_' => "Minus",
        '/' => "Slash",
        '\\' => "Backslash",
        '\'' => "Quote",
        '"' => "Quote",
        '(' | ')' => "Digit9",
        '[' | '{' => "BracketLeft",
        ']' | '}' => "BracketRight",
        '@' => "Digit2",
        '#' => "Digit3",
        '$' => "Digit4",
        '%' => "Digit5",
        '^' => "Digit6",
        '&' => "Digit7",
        '*' => "Digit8",
        '=' => "Equal",
        '+' => "Equal",
        ';' => "Semicolon",
        ':' => "Semicolon",
        '`' => "Backquote",
        '~' => "Backquote",
        '|' => "Backslash",
        '<' => "Comma",
        '>' => "Period",
        _ => "Unidentified",
    }
}
