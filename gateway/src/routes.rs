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
use crate::ws::WsEvent;

/// POST /v1/chat/completions
pub async fn chat_completions(
    State(state): State<AppState>,
    Json(mut request): Json<ChatCompletionRequest>,
) -> Result<Response, ChatApiError> {
    if request.messages.is_empty() {
        return Err(ChatApiError::InvalidRequest(
            "messages must not be empty".to_string(),
        ));
    }

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

    let filtered_tools: Vec<_> = tool_schemas.iter().filter(|t| {
        let name = t["name"].as_str().unwrap_or("");
        filter::is_tool_allowed(name, &config)
    }).cloned().collect();

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

    let target_stream = state.target.stream_request(&request).await
        .map_err(|e| ChatApiError::AutomationFailure(format!("Target error: {}", e)))?;

    // Clone broadcaster and request_id for the streaming task
    let broadcaster = state.events.clone();
    let session_id = request_id.clone();

    // Pipe target stream into mpsc channel + broadcast to WS clients
    tokio::spawn(async move {
        let mut stream = target_stream;
        while let Some(result) = stream.next().await {
            match result {
                Ok(token) => {
                    if !token.is_empty() {
                        // Broadcast to WebSocket clients
                        broadcaster.send(WsEvent::Token {
                            session_id: session_id.clone(),
                            content: token.clone(),
                        });
                        // Send to SSE stream
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
        // Signal response complete
        broadcaster.send(WsEvent::ResponseDone {
            session_id: session_id.clone(),
            response: String::new(),
        });
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

    let response = state.target.send_request(&request).await
        .map_err(|e| ChatApiError::AutomationFailure(format!("Target error: {}", e)))?;

    if has_tools && !last_is_tool_result {
        // Check native tool_calls from API
        if let Some(tool_calls) = response.choices.first().and_then(|c| c.message.tool_calls.as_ref()) {
            if !tool_calls.is_empty() {
                let all_tool_calls = tool_calls.clone();

                let config = state.config.read().await;
                let working_dir = config.working_dir();
                let max_output = config.security.max_tool_output_bytes;

                for tc in tool_calls.iter() {
                    // Enforce allowed_tools check
                    if !filter::is_tool_allowed(&tc.function.name, &config) {
                        warn!("Tool '{}' blocked by allowed_tools config", tc.function.name);
                        continue;
                    }

                    // Broadcast tool call to WS clients
                    state.events.send(WsEvent::ToolCall {
                        session_id: request_id.clone(),
                        tool_name: tc.function.name.clone(),
                        arguments: tc.function.arguments.clone(),
                    });

                    let args: serde_json::Value = serde_json::from_str(&tc.function.arguments)
                        .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));

                    // Enforce blocked_paths on file-path arguments
                    let path_blocked = ["path", "cwd"].iter().any(|key| {
                        args.get(key)
                            .and_then(|v| v.as_str())
                            .is_some_and(|p| filter::is_path_blocked(p, &config))
                    });
                    if path_blocked {
                        warn!("Tool '{}' blocked: path in blocked_paths", tc.function.name);
                        state.events.send(WsEvent::ToolResult {
                            session_id: request_id.clone(),
                            tool_name: tc.function.name.clone(),
                            result: "Error: path blocked by config".to_string(),
                            is_error: true,
                        });
                        continue;
                    }

                    let ctx = ToolContext {
                        session_id: request_id.clone(),
                        working_dir: working_dir.clone(),
                        env: std::collections::HashMap::new(),
                    };

                    let (result_text, is_error) = match state.tools.execute(&tc.function.name, args, &ctx).await {
                        Ok(result) => {
                            let text = match &result {
                                chatapi_shared::traits::ToolResult::Text(t) => truncate_output(t, max_output),
                                chatapi_shared::traits::ToolResult::Diff { old, new, path } => {
                                    truncate_output(&format!("Diff for {}:\n--- old\n{}\n+++ new\n{}", path.display(), old, new), max_output)
                                }
                                chatapi_shared::traits::ToolResult::Error { message, .. } => {
                                    format!("Error: {}", message)
                                }
                            };
                            (text, false)
                        }
                        Err(e) => (format!("Tool error: {}", e), true),
                    };

                    // Broadcast tool result to WS clients
                    state.events.send(WsEvent::ToolResult {
                        session_id: request_id.clone(),
                        tool_name: tc.function.name.clone(),
                        result: result_text,
                        is_error,
                    });
                }
                drop(config);

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
                    for tc in &parsed.tool_calls {
                        state.events.send(WsEvent::ToolCall {
                            session_id: request_id.clone(),
                            tool_name: tc.function.name.clone(),
                            arguments: tc.function.arguments.clone(),
                        });
                    }

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

    // Regular text response — broadcast to WS
    if let Some(content) = response.choices.first().and_then(|c| c.message.content.as_ref()) {
        state.events.send(WsEvent::ResponseDone {
            session_id: request_id.clone(),
            response: content.clone(),
        });
    }

    let mut resp = response;
    resp.id = request_id;
    resp.created = created;
    Ok(Json(resp).into_response())
}

/// GET /v1/models — list all available models from all providers.
pub async fn list_models(State(state): State<AppState>) -> Json<serde_json::Value> {
    let config = state.config.read().await;
    let mut models = vec![serde_json::json!({
        "id": config.target.model,
        "object": "model",
        "created": 0,
        "owned_by": "default",
    })];

    // Add models from configured providers
    for provider in &config.models.providers {
        for model in &provider.models {
            models.push(serde_json::json!({
                "id": model.id,
                "object": "model",
                "created": 0,
                "owned_by": provider.name,
                "provider": provider.name,
                "endpoint": provider.endpoint,
            }));
        }
    }

    Json(serde_json::json!({
        "object": "list",
        "data": models,
    }))
}

/// GET /v1/providers — list all configured model providers.
pub async fn list_providers(State(state): State<AppState>) -> Json<serde_json::Value> {
    let config = state.config.read().await;
    let providers: Vec<serde_json::Value> = config.models.providers.iter().map(|p| {
        serde_json::json!({
            "name": p.name,
            "endpoint": p.endpoint,
            "models": p.models.iter().map(|m| {
                serde_json::json!({
                    "id": m.id,
                    "name": m.name,
                    "max_tokens": m.max_tokens,
                })
            }).collect::<Vec<_>>(),
        })
    }).collect();

    Json(serde_json::json!({
        "providers": providers,
        "default_model": config.target.model,
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

    // Broadcast session creation
    state.events.send(WsEvent::SessionEvent {
        session_id: session.id.clone(),
        action: "created".to_string(),
    });

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
        state.events.send(WsEvent::SessionEvent {
            session_id: session_id.clone(),
            action: "deleted".to_string(),
        });
        Ok(Json(serde_json::json!({"deleted": true})))
    } else {
        Err(ChatApiError::InvalidRequest(format!("Session not found: {}", session_id)))
    }
}

/// POST /sessions/:id/branch — fork a conversation at a given message index.
pub async fn branch_session(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ChatApiError> {
    let at_message = body.get("at_message").and_then(|v| v.as_u64()).map(|v| v as usize);

    match state.sessions.branch(&session_id, at_message) {
        Some(session) => {
            state.events.send(WsEvent::SessionEvent {
                session_id: session.id.clone(),
                action: "created".to_string(),
            });
            Ok(Json(serde_json::json!({
                "id": session.id,
                "model": session.metadata.model,
                "message_count": session.messages.len(),
                "created_at": session.created_at,
            })))
        }
        None => Err(ChatApiError::InvalidRequest(format!("Session not found: {}", session_id))),
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

    let config_path = std::env::var("CHATAPI_CONFIG")
        .unwrap_or_else(|_| ".chatapi/config.toml".to_string());
    let _ = config.save(std::path::Path::new(&config_path));

    Ok(Json(serde_json::json!({"updated": true})))
}

/// GET /files?path=.
pub async fn list_files(
    State(state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<Json<serde_json::Value>, ChatApiError> {
    let config = state.config.read().await;
    let working_dir = config.working_dir();
    drop(config);

    let rel_path = params.get("path").map(|s| s.as_str()).unwrap_or(".");
    let abs_path = working_dir.join(rel_path);

    // Security: prevent directory traversal
    if !abs_path.starts_with(&working_dir) {
        return Err(ChatApiError::InvalidRequest("Path outside working directory".to_string()));
    }

    if !abs_path.is_dir() {
        return Err(ChatApiError::InvalidRequest("Not a directory".to_string()));
    }

    let mut entries = Vec::new();
    let dir = tokio::fs::read_dir(&abs_path).await
        .map_err(|e| ChatApiError::AutomationFailure(e.to_string()))?;

    let mut dir_stream = dir;
    while let Some(entry) = dir_stream.next_entry().await
        .map_err(|e| ChatApiError::AutomationFailure(e.to_string()))?
    {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') && name != ".knowledge" {
            continue; // Skip hidden files except .knowledge
        }
        let is_dir = entry.file_type().await
            .map(|ft| ft.is_dir())
            .unwrap_or(false);
        let rel = if rel_path == "." {
            name.clone()
        } else {
            format!("{}/{}", rel_path, name)
        };
        entries.push(serde_json::json!({
            "name": name,
            "path": rel,
            "isDir": is_dir,
        }));
    }

    // Sort: dirs first, then alphabetically
    entries.sort_by(|a, b| {
        let a_dir = a["isDir"].as_bool().unwrap_or(false);
        let b_dir = b["isDir"].as_bool().unwrap_or(false);
        match (a_dir, b_dir) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a["name"].as_str().cmp(&b["name"].as_str()),
        }
    });

    Ok(Json(serde_json::json!({
        "path": rel_path,
        "entries": entries,
    })))
}

/// GET /files/read?path=Cargo.toml
pub async fn read_file(
    State(state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<Json<serde_json::Value>, ChatApiError> {
    let config = state.config.read().await;
    let working_dir = config.working_dir();
    drop(config);

    let rel_path = params.get("path").ok_or_else(|| {
        ChatApiError::InvalidRequest("Missing 'path' parameter".to_string())
    })?;

    let abs_path = working_dir.join(rel_path);

    // Security: prevent directory traversal
    if !abs_path.starts_with(&working_dir) {
        return Err(ChatApiError::InvalidRequest("Path outside working directory".to_string()));
    }

    if !abs_path.is_file() {
        return Err(ChatApiError::InvalidRequest("Not a file".to_string()));
    }

    // Check blocked paths
    let config = state.config.read().await;
    if chatapi_rules::filter::is_path_blocked(rel_path, &config) {
        return Err(ChatApiError::InvalidRequest("Path is blocked by config".to_string()));
    }
    drop(config);

    let content = tokio::fs::read_to_string(&abs_path).await
        .map_err(|e| ChatApiError::AutomationFailure(e.to_string()))?;

    Ok(Json(serde_json::json!({
        "path": rel_path,
        "content": content,
    })))
}

/// POST /tools/execute — execute a tool directly.
/// Body: {"name": "run_command", "args": {"command": "ls -la"}}
pub async fn execute_tool(
    State(state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ChatApiError> {
    let name = body.get("name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ChatApiError::InvalidRequest("name required".to_string()))?;
    let args = body.get("args").cloned()
        .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));

    // Enforce allowed_tools check
    let config = state.config.read().await;
    if !chatapi_rules::filter::is_tool_allowed(name, &config) {
        return Err(ChatApiError::InvalidRequest(format!("Tool '{}' is not allowed by config", name)));
    }

    // Enforce blocked_paths on file-path arguments (path, cwd)
    for key in &["path", "cwd"] {
        if let Some(p) = args.get(*key).and_then(|v| v.as_str()) {
            if chatapi_rules::filter::is_path_blocked(p, &config) {
                return Err(ChatApiError::InvalidRequest(format!("Path '{}' is blocked by config", p)));
            }
        }
    }
    let working_dir = config.working_dir();
    let max_output = config.security.max_tool_output_bytes;
    drop(config);

    let ctx = chatapi_shared::traits::ToolContext {
        session_id: String::new(),
        working_dir,
        env: std::collections::HashMap::new(),
    };

    match state.tools.execute(name, args, &ctx).await {
        Ok(result) => {
            let (text, is_error) = match &result {
                chatapi_shared::traits::ToolResult::Text(t) => (truncate_output(t, max_output), false),
                chatapi_shared::traits::ToolResult::Diff { old, new, path } => {
                    (truncate_output(&format!("Diff for {}:\n--- old\n{}\n+++ new\n{}", path.display(), old, new), max_output), false)
                }
                chatapi_shared::traits::ToolResult::Error { message, .. } => {
                    (format!("Error: {}", message), true)
                }
            };
            Ok(Json(serde_json::json!({
                "result": text,
                "is_error": is_error,
            })))
        }
        Err(e) => Err(ChatApiError::AutomationFailure(format!("Tool error: {}", e))),
    }
}

/// GET /tools/:name — get a tool's schema.
pub async fn get_tool_schema(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<serde_json::Value>, ChatApiError> {
    let tools = state.tools.list_tools();
    let tool = tools.iter().find(|(n, _, _)| *n == name);
    match tool {
        Some((name, desc, schema)) => Ok(Json(serde_json::json!({
            "name": name,
            "description": desc,
            "parameters": schema,
        }))),
        None => Err(ChatApiError::InvalidRequest(format!("Tool '{}' not found", name))),
    }
}

/// POST /tools/:name/test — test a tool with sample input.
pub async fn test_tool(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ChatApiError> {
    let args = body.get("args").cloned()
        .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));

    let config = state.config.read().await;
    let working_dir = config.working_dir();
    drop(config);

    let ctx = chatapi_shared::traits::ToolContext {
        session_id: String::new(),
        working_dir,
        env: std::collections::HashMap::new(),
    };

    match state.tools.execute(&name, args, &ctx).await {
        Ok(result) => {
            let (text, is_error) = match &result {
                chatapi_shared::traits::ToolResult::Text(t) => (t.clone(), false),
                chatapi_shared::traits::ToolResult::Diff { old, new, path } => {
                    (format!("Diff for {}:\n--- old\n{}\n+++ new\n{}", path.display(), old, new), false)
                }
                chatapi_shared::traits::ToolResult::Error { message, .. } => {
                    (format!("Error: {}", message), true)
                }
            };
            Ok(Json(serde_json::json!({
                "result": text,
                "is_error": is_error,
            })))
        }
        Err(e) => Err(ChatApiError::AutomationFailure(format!("Tool error: {}", e))),
    }
}

/// GET /logs — SSE stream of tool execution logs.
pub async fn logs_stream(
    State(state): State<AppState>,
) -> Sse<impl futures_util::Stream<Item = Result<axum::response::sse::Event, std::convert::Infallible>>> {
    let mut rx = state.events.subscribe();
    let stream = async_stream::stream! {
        loop {
            match rx.recv().await {
                Ok(event) => {
                    if let Ok(json) = serde_json::to_string(&event) {
                        yield Ok(axum::response::sse::Event::default().data(json));
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    };
    Sse::new(stream)
}

// ── Agent endpoints ──────────────────────────────────────────────

/// POST /agents/tasks — submit a high-level task to the orchestrator.
pub async fn agent_submit_task(
    State(state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ChatApiError> {
    let orch = state.orchestrator.as_ref()
        .ok_or_else(|| ChatApiError::InvalidRequest("Orchestrator not configured".to_string()))?;

    let description = body.get("description")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ChatApiError::InvalidRequest("description required".to_string()))?
        .to_string();

    let task_id = orch.submit_task(description).await
        .map_err(|e| ChatApiError::AutomationFailure(e.to_string()))?;

    Ok(Json(serde_json::json!({ "task_id": task_id })))
}

/// GET /agents/tasks — list all tasks.
pub async fn agent_list_tasks(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, ChatApiError> {
    let orch = state.orchestrator.as_ref()
        .ok_or_else(|| ChatApiError::InvalidRequest("Orchestrator not configured".to_string()))?;

    let tasks = orch.list_tasks().await;
    let tasks_json: Vec<serde_json::Value> = tasks.iter().map(|t| {
        serde_json::json!({
            "id": t.id,
            "description": t.description,
            "status": format!("{:?}", t.status),
            "steps": t.steps.len(),
            "created_at": t.created_at.to_rfc3339(),
            "updated_at": t.updated_at.to_rfc3339(),
        })
    }).collect();

    Ok(Json(serde_json::json!({ "tasks": tasks_json })))
}

/// GET /agents/tasks/:id — get task detail.
pub async fn agent_get_task(
    State(state): State<AppState>,
    axum::extract::Path(task_id): axum::extract::Path<String>,
) -> Result<Json<serde_json::Value>, ChatApiError> {
    let orch = state.orchestrator.as_ref()
        .ok_or_else(|| ChatApiError::InvalidRequest("Orchestrator not configured".to_string()))?;

    let task = orch.get_task(&task_id).await
        .ok_or_else(|| ChatApiError::InvalidRequest(format!("Task {} not found", task_id)))?;

    let steps_json: Vec<serde_json::Value> = task.steps.iter().map(|s| {
        serde_json::json!({
            "id": s.id,
            "description": s.description,
            "assigned_to": format!("{:?}", s.assigned_to),
            "status": format!("{:?}", s.status),
            "result": s.result,
            "error": s.error,
        })
    }).collect();

    Ok(Json(serde_json::json!({
        "id": task.id,
        "description": task.description,
        "status": format!("{:?}", task.status),
        "steps": steps_json,
        "created_at": task.created_at.to_rfc3339(),
        "updated_at": task.updated_at.to_rfc3339(),
    })))
}

/// POST /agents/tasks/:id/cancel — cancel a running task.
pub async fn agent_cancel_task(
    State(state): State<AppState>,
    axum::extract::Path(task_id): axum::extract::Path<String>,
) -> Result<Json<serde_json::Value>, ChatApiError> {
    let orch = state.orchestrator.as_ref()
        .ok_or_else(|| ChatApiError::InvalidRequest("Orchestrator not configured".to_string()))?;

    orch.cancel_task(&task_id).await
        .map_err(|e| ChatApiError::AutomationFailure(e.to_string()))?;

    Ok(Json(serde_json::json!({ "cancelled": true })))
}

/// GET /agents/capabilities — list available agent types.
pub async fn agent_capabilities(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, ChatApiError> {
    let orch = state.orchestrator.as_ref()
        .ok_or_else(|| ChatApiError::InvalidRequest("Orchestrator not configured".to_string()))?;

    let roles = orch.capabilities().await;
    let roles_json: Vec<String> = roles.iter().map(|r| format!("{:?}", r)).collect();

    Ok(Json(serde_json::json!({ "agents": roles_json })))
}

// ── Helper functions ─────────────────────────────────────────────

fn truncate_output(text: &str, max_bytes: usize) -> String {
    if text.len() <= max_bytes {
        text.to_string()
    } else {
        // Find a valid UTF-8 boundary at or before max_bytes
        let mut end = max_bytes;
        while end > 0 && !text.is_char_boundary(end) {
            end -= 1;
        }
        let trunc = &text[..end];
        format!("{}\n\n[...truncated at {} bytes]", trunc, max_bytes)
    }
}

fn estimate_tokens(text: &str) -> u32 {
    (text.len() as u32 + 3) / 4
}
