use axum::{
    extract::{Path, State},
    response::{
        sse::Sse,
        IntoResponse, Json, Response,
    },
};
use chatapi_shared::{
    generate_id, now_epoch, ChatApiError, ChatCompletionRequest,
    ChatCompletionResponse, ChatMessage, Role,
    tool_parser::{contains_tool_call_pattern, parse_tool_calls_from_text},
    traits::ToolContext,
};
use chatapi_rules::{filter, prompt};
use futures_util::StreamExt;
use tokio::sync::mpsc;
use tracing::{info, warn};

use crate::state::AppState;
use crate::streaming::SseStream;

/// POST /v1/chat/completions
pub async fn chat_completions(
    State(state): State<AppState>,
    Json(mut request): Json<ChatCompletionRequest>,
) -> Result<Response, ChatApiError> {
    // Validate request
    if request.messages.is_empty() {
        return Err(ChatApiError::InvalidRequest(
            "messages must not be empty".to_string(),
        ));
    }

    // Validate tool messages
    for msg in &request.messages {
        if msg.role == Role::Tool && msg.tool_call_id.is_none() {
            return Err(ChatApiError::InvalidRequest(
                "tool messages must include tool_call_id".to_string(),
            ));
        }
    }

    let request_id = generate_id();
    let created = now_epoch();

    // Get config and build system prompt
    let config = state.config.read().await;
    let tool_schemas = state.tools.list_tools().iter().map(|(name, desc, schema)| {
        serde_json::json!({
            "name": name,
            "description": desc,
            "parameters": schema,
        })
    }).collect::<Vec<_>>();

    // Filter tools by allowed_tools config
    let filtered_tools: Vec<_> = tool_schemas.iter().filter(|t| {
        let name = t["name"].as_str().unwrap_or("");
        filter::is_tool_allowed(name, &config)
    }).cloned().collect();

    // Inject system prompt if not already present
    let system_prompt = prompt::build_system_prompt(&config, &filtered_tools);
    if !system_prompt.is_empty() {
        let has_system = request.messages.first().map(|m| m.role == Role::System).unwrap_or(false);
        if !has_system {
            request.messages.insert(0, ChatMessage {
                role: Role::System,
                content: Some(system_prompt),
                tool_calls: None,
                tool_call_id: None,
                name: None,
            });
        }
    }

    let has_tools = request.tools.is_some() && !request.tools.as_ref().unwrap().is_empty();
    drop(config);

    info!(
        request_id = %request_id,
        model = %request.model,
        stream = request.stream,
        message_count = request.messages.len(),
        has_tools = has_tools,
        "Received chat completion request"
    );

    if request.stream {
        handle_streaming(state, request, request_id).await
    } else {
        handle_non_streaming(state, request, request_id, has_tools, created).await
    }
}

async fn handle_streaming(
    state: AppState,
    request: ChatCompletionRequest,
    request_id: String,
) -> Result<Response, ChatApiError> {
    let (tx, rx) = mpsc::channel::<String>(64);

    // Get streaming response from target
    let target_stream = state.target.stream_request(&request).await
        .map_err(|e| ChatApiError::AutomationFailure(format!("Target error: {}", e)))?;

    // Pipe target stream into mpsc channel
    tokio::spawn(async move {
        let mut stream = target_stream;
        while let Some(result) = stream.next().await {
            match result {
                Ok(token) => {
                    if !token.is_empty() {
                        if tx.send(token).await.is_err() {
                            break;
                        }
                    }
                }
                Err(e) => {
                    warn!("Stream error: {}", e);
                    break;
                }
            }
        }
    });

    let sse_stream = SseStream::new(rx, request_id, request.model);
    let sse = Sse::new(sse_stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(std::time::Duration::from_secs(15))
            .text("ping"),
    );

    Ok(sse.into_response())
}

async fn handle_non_streaming(
    state: AppState,
    request: ChatCompletionRequest,
    request_id: String,
    has_tools: bool,
    created: i64,
) -> Result<Response, ChatApiError> {
    let last_is_tool_result = request.messages.last()
        .map(|m| m.role == Role::Tool)
        .unwrap_or(false);

    // Send to target
    let response = state.target.send_request(&request).await
        .map_err(|e| ChatApiError::AutomationFailure(format!("Target error: {}", e)))?;

    // Check if response contains tool calls (skip if last message was a tool result)
    if has_tools && !last_is_tool_result {
        // First check native tool_calls from API
        if let Some(tool_calls) = response.choices.first().and_then(|c| c.message.tool_calls.as_ref()) {
            if !tool_calls.is_empty() {
                let all_tool_calls = tool_calls.clone();

                let config = state.config.read().await;
                let working_dir = config.working_dir();
                drop(config);

                let mut tool_results = Vec::new();
                for tc in tool_calls.iter() {
                    let args: serde_json::Value = serde_json::from_str(&tc.function.arguments)
                        .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));

                    let ctx = ToolContext {
                        session_id: request_id.clone(),
                        working_dir: working_dir.clone(),
                        env: std::collections::HashMap::new(),
                    };

                    match state.tools.execute(&tc.function.name, args, &ctx).await {
                        Ok(result) => {
                            let result_text = match &result {
                                chatapi_shared::traits::ToolResult::Text(t) => t.clone(),
                                chatapi_shared::traits::ToolResult::Diff { old, new, path } => {
                                    format!("Diff for {}:\n--- old\n{}\n+++ new\n{}", path.display(), old, new)
                                }
                                chatapi_shared::traits::ToolResult::Error { message, .. } => {
                                    format!("Error: {}", message)
                                }
                            };
                            tool_results.push(ChatMessage {
                                role: Role::Tool,
                                content: Some(result_text),
                                tool_calls: None,
                                tool_call_id: Some(tc.id.clone()),
                                name: Some(tc.function.name.clone()),
                            });
                        }
                        Err(e) => {
                            tool_results.push(ChatMessage {
                                role: Role::Tool,
                                content: Some(format!("Tool error: {}", e)),
                                tool_calls: None,
                                tool_call_id: Some(tc.id.clone()),
                                name: Some(tc.function.name.clone()),
                            });
                        }
                    }
                }

                // Return tool_calls response
                let prompt_tokens = estimate_tokens(&format!("{:?}", &request.messages));
                let mut resp = ChatCompletionResponse::new_tool_calls(
                    request.model,
                    all_tool_calls,
                    prompt_tokens,
                    0,
                );
                resp.id = request_id;
                resp.created = created;
                return Ok(Json(resp).into_response());
            }
        }

        // Check for tool call patterns in text response (browser mode)
        if let Some(content) = response.choices.first().and_then(|c| c.message.content.as_ref()) {
            if contains_tool_call_pattern(content) {
                let parsed = parse_tool_calls_from_text(content);
                if parsed.has_tool_calls {
                    let prompt_tokens = estimate_tokens(&format!("{:?}", &request.messages));
                    let completion_tokens = estimate_tokens(content);
                    let mut resp = ChatCompletionResponse::new_tool_calls(
                        request.model,
                        parsed.tool_calls,
                        prompt_tokens,
                        completion_tokens,
                    );
                    resp.id = request_id;
                    resp.created = created;
                    return Ok(Json(resp).into_response());
                }
            }
        }
    }

    // Regular text response
    let mut resp = response;
    resp.id = request_id;
    resp.created = created;
    Ok(Json(resp).into_response())
}

/// GET /v1/models
pub async fn list_models(State(state): State<AppState>) -> Json<serde_json::Value> {
    let config = state.config.read().await;
    Json(serde_json::json!({
        "object": "list",
        "data": [{
            "id": config.target.model,
            "object": "model",
            "created": 0,
            "owned_by": "chatapi",
        }]
    }))
}

/// GET /health
pub async fn health(State(state): State<AppState>) -> Json<serde_json::Value> {
    let config = state.config.read().await;
    let mode = config.target.mode.clone();
    drop(config);

    Json(serde_json::json!({
        "status": "ok",
        "mode": mode,
        "tool_count": state.tools.names().len(),
    }))
}

/// GET /sessions
pub async fn list_sessions(State(state): State<AppState>) -> Json<serde_json::Value> {
    let sessions = state.sessions.list();
    Json(serde_json::json!({
        "sessions": sessions,
    }))
}

/// POST /sessions
pub async fn create_session(
    State(state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let model = body["model"].as_str().unwrap_or("deepseek-chat");
    let session = state.sessions.create(model);
    Json(serde_json::json!({
        "id": session.id,
        "model": session.metadata.model,
        "created_at": session.created_at,
    }))
}

/// GET /sessions/:id
pub async fn get_session(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> Result<Json<serde_json::Value>, ChatApiError> {
    match state.sessions.get(&session_id) {
        Some(session) => Ok(Json(serde_json::json!({
            "id": session.id,
            "messages": session.messages,
            "metadata": session.metadata,
            "created_at": session.created_at,
            "updated_at": session.updated_at,
        }))),
        None => Err(ChatApiError::InvalidRequest(format!("Session not found: {}", session_id))),
    }
}

/// DELETE /sessions/:id
pub async fn delete_session(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> Result<Json<serde_json::Value>, ChatApiError> {
    if state.sessions.delete(&session_id) {
        Ok(Json(serde_json::json!({"deleted": true})))
    } else {
        Err(ChatApiError::InvalidRequest(format!("Session not found: {}", session_id)))
    }
}

/// GET /tools
pub async fn list_tools(State(state): State<AppState>) -> Json<serde_json::Value> {
    let tools = state.tools.list_tools();
    let tool_list: Vec<serde_json::Value> = tools.iter().map(|(name, desc, schema)| {
        serde_json::json!({
            "name": name,
            "description": desc,
            "parameters": schema,
        })
    }).collect();
    Json(serde_json::json!({
        "tools": tool_list,
    }))
}

/// GET /config
pub async fn get_config(State(state): State<AppState>) -> Json<serde_json::Value> {
    let config = state.config.read().await;
    Json(serde_json::json!({
        "target": {
            "mode": config.target.mode,
            "model": config.target.model,
        },
        "rules": {
            "system_prompt": config.rules.system_prompt,
            "working_dir": config.rules.working_dir,
            "allowed_tools": config.rules.allowed_tools,
            "blocked_paths": config.rules.blocked_paths,
        },
        "sessions": {
            "store": config.sessions.store,
        },
    }))
}

/// PUT /config
pub async fn update_config(
    State(state): State<AppState>,
    Json(updates): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ChatApiError> {
    let mut config = state.config.write().await;

    if let Some(rules) = updates.get("rules") {
        if let Some(prompt) = rules.get("system_prompt").and_then(|v| v.as_str()) {
            config.rules.system_prompt = Some(prompt.to_string());
        }
        if let Some(tools) = rules.get("allowed_tools").and_then(|v| v.as_array()) {
            config.rules.allowed_tools = tools.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect();
        }
        if let Some(paths) = rules.get("blocked_paths").and_then(|v| v.as_array()) {
            config.rules.blocked_paths = paths.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect();
        }
    }

    // Save to disk if config file exists
    let config_path = std::env::var("CHATAPI_CONFIG")
        .unwrap_or_else(|_| ".chatapi/config.toml".to_string());
    let _ = config.save(std::path::Path::new(&config_path));

    Ok(Json(serde_json::json!({"updated": true})))
}

fn estimate_tokens(text: &str) -> u32 {
    (text.len() as u32 + 3) / 4
}
