use axum::{
    extract::State,
    response::{
        sse::Sse,
        IntoResponse, Json, Response,
    },
};
use chatapi_shared::{
    generate_id, now_epoch, ChatApiError, ChatCompletionRequest,
    ChatCompletionResponse, ChatMessage,
};
use tokio::sync::mpsc;
use tracing::info;

use crate::state::AppState;
use crate::streaming::SseStream;

/// POST /v1/chat/completions
pub async fn chat_completions(
    State(_state): State<AppState>,
    Json(request): Json<ChatCompletionRequest>,
) -> Result<Response, ChatApiError> {
    // Validate request
    if request.messages.is_empty() {
        return Err(ChatApiError::InvalidRequest(
            "messages must not be empty".to_string(),
        ));
    }

    let request_id = generate_id();
    let _created = now_epoch();

    info!(
        request_id = %request_id,
        model = %request.model,
        stream = request.stream,
        message_count = request.messages.len(),
        "Received chat completion request"
    );

    if request.stream {
        // Return SSE stream
        let (tx, rx) = mpsc::channel::<String>(64);

        // Spawn mock response task (will be replaced by CDP engine integration)
        let last_message = request
            .messages
            .last()
            .map(|m| m.content.clone())
            .unwrap_or_default();
        tokio::spawn(async move {
            simulate_streaming_response(tx, &last_message).await;
        });

        let sse_stream = SseStream::new(rx, request_id.clone(), request.model.clone());
        let sse = Sse::new(sse_stream).keep_alive(
            axum::response::sse::KeepAlive::new()
                .interval(std::time::Duration::from_secs(15))
                .text("ping"),
        );

        Ok(sse.into_response())
    } else {
        // Return complete response
        let response_text = simulate_complete_response(&request.messages).await;
        let prompt_tokens: u32 = request
            .messages
            .iter()
            .map(|m| estimate_tokens(&m.content))
            .sum();
        let completion_tokens = estimate_tokens(&response_text);

        let response = ChatCompletionResponse::new(
            request.model,
            response_text,
            prompt_tokens,
            completion_tokens,
        );

        Ok(Json(response).into_response())
    }
}

/// GET /health
pub async fn health(State(state): State<AppState>) -> Json<serde_json::Value> {
    let browser_connected = *state.browser_connected.lock().await;
    let session_count = state.sessions.lock().await.len();

    Json(serde_json::json!({
        "status": "ok",
        "browser_connected": browser_connected,
        "active_sessions": session_count,
    }))
}

// ── Mock implementations (to be replaced by CDP engine) ────────────────

async fn simulate_streaming_response(tx: mpsc::Sender<String>, prompt: &str) {
    let response = mock_llm_response(prompt);
    let tokens = tokenize_for_streaming(&response);

    for token in tokens {
        if tx.send(token).await.is_err() {
            break; // Client disconnected
        }
        // Simulate streaming latency
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
}

async fn simulate_complete_response(messages: &[ChatMessage]) -> String {
    let last = messages.last().map(|m| m.content.as_str()).unwrap_or("");
    mock_llm_response(last)
}

fn mock_llm_response(prompt: &str) -> String {
    let lower = prompt.to_lowercase();
    if lower.contains("hello") || lower.contains("hi") {
        "Hello! I'm a mock DeepSeek assistant running through the ChatAPI bridge. How can I help you today?".to_string()
    } else if lower.contains("who are you") {
        "I'm a mock response from the ChatAPI bridge — a high-performance middleware that proxies requests to free LLM chat interfaces via CDP automation. The real implementation will connect to DeepSeek Chat.".to_string()
    } else if lower.contains("test") {
        "Test received and processed. The gateway is functioning correctly. Streaming pipeline, ring buffer, and SSE encoding are operational.".to_string()
    } else {
        format!(
            "I received your message: \"{}\". This is a mock response from the ChatAPI gateway. \
             Once the CDP engine is connected, this will be replaced with actual DeepSeek Chat responses.",
            prompt
        )
    }
}

fn tokenize_for_streaming(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    for ch in text.chars() {
        current.push(ch);
        if ch == ' ' || ch == '.' || ch == '!' || ch == '?' {
            tokens.push(current.clone());
            current.clear();
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

fn estimate_tokens(text: &str) -> u32 {
    (text.len() as u32 + 3) / 4
}
