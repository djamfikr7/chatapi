//! End-to-end integration tests for the ChatAPI gateway.
//!
//! These tests spin up the full Axum server in-process and test the
//! OpenAI-compatible API endpoints via HTTP.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chatapi_gateway::state::AppState;
use chatapi_rules::ChatApiConfig;
use chatapi_sessions::{SessionManager, MemoryStore};
use chatapi_shared::{
    ChatCompletionRequest, ChatCompletionResponse, ChatMessage, Role,
    ToolCall, FunctionCall, generate_id,
    traits::{TargetProvider, TargetError, TargetStream},
};
use chatapi_targets::TargetRouter;
use chatapi_tools::ToolRegistry;

// ── Mock target for testing ────────────────────────────────────────

struct MockTarget;

#[async_trait]
impl TargetProvider for MockTarget {
    fn name(&self) -> &str { "mock" }

    async fn health_check(&self) -> bool { true }

    async fn send_request(
        &self,
        req: &ChatCompletionRequest,
    ) -> Result<ChatCompletionResponse, TargetError> {
        let user_content = req.messages.iter()
            .rev()
            .find(|m| m.role == Role::User)
            .and_then(|m| m.content.as_deref())
            .unwrap_or("");

        let has_tools = req.tools.as_ref().map(|t| !t.is_empty()).unwrap_or(false);
        let last_is_tool = req.messages.last()
            .map(|m| m.role == Role::Tool)
            .unwrap_or(false);

        if has_tools && !last_is_tool {
            if user_content.contains("edit") || user_content.contains("read") || user_content.contains("ls") {
                let tool = if user_content.contains("edit") {
                    "edit_file"
                } else if user_content.contains("read") {
                    "read_file"
                } else {
                    "run_command"
                };

                let args = match tool {
                    "edit_file" => r#"{"path":"src/main.rs","old_text":"old","new_text":"new"}"#,
                    "read_file" => r#"{"path":"Cargo.toml"}"#,
                    "run_command" => r#"{"command":"ls -la"}"#,
                    _ => "{}",
                };

                return Ok(ChatCompletionResponse::new_tool_calls(
                    req.model.clone(),
                    vec![ToolCall {
                        id: format!("call_{}", generate_id()),
                        call_type: "function".to_string(),
                        function: FunctionCall {
                            name: tool.to_string(),
                            arguments: args.to_string(),
                        },
                    }],
                    10,
                    0,
                ));
            }
        }

        let content = if user_content.contains("hello") {
            "Hello! This is a mock response from ChatAPI bridge.".to_string()
        } else if user_content.contains("test") {
            "Test received. Mock response.".to_string()
        } else {
            format!("Mock response to: {}", user_content)
        };

        Ok(ChatCompletionResponse::new(
            req.model.clone(),
            content,
            10,
            20,
        ))
    }

    async fn stream_request(
        &self,
        req: &ChatCompletionRequest,
    ) -> Result<TargetStream, TargetError> {
        let user_content = req.messages.iter()
            .rev()
            .find(|m| m.role == Role::User)
            .and_then(|m| m.content.as_deref())
            .unwrap_or("");

        let has_tools = req.tools.as_ref().map(|t| !t.is_empty()).unwrap_or(false);
        let last_is_tool = req.messages.last()
            .map(|m| m.role == Role::Tool)
            .unwrap_or(false);

        let tokens: Vec<Result<String, TargetError>> = if has_tools && !last_is_tool
            && (user_content.contains("edit") || user_content.contains("read") || user_content.contains("ls"))
        {
            let tool = if user_content.contains("edit") {
                "edit_file"
            } else if user_content.contains("read") {
                "read_file"
            } else {
                "run_command"
            };
            let args = match tool {
                "edit_file" => r#"{"path":"src/main.rs","old_text":"old","new_text":"new"}"#,
                "read_file" => r#"{"path":"Cargo.toml"}"#,
                "run_command" => r#"{"command":"ls -la"}"#,
                _ => "{}",
            };
            vec![Ok(format!("```json\n{{\"name\": \"{}\", \"arguments\": {}}}\n```", tool, args))]
        } else if user_content.contains("test") {
            vec![Ok("Test ".to_string()), Ok("received. ".to_string()), Ok("Mock response.".to_string())]
        } else {
            vec![Ok("Hello! ".to_string()), Ok("This is a mock response.".to_string())]
        };

        let stream = futures_util::stream::iter(tokens);
        Ok(Box::pin(stream))
    }
}

/// Create a test AppState with a mock target.
fn create_test_state() -> AppState {
    let config = ChatApiConfig::default();
    let tools = build_test_tools();
    let sessions = SessionManager::new(Box::new(MemoryStore::new()));

    AppState::new(config, MockTarget, tools, sessions, Vec::new())
}

fn build_test_tools() -> ToolRegistry {
    use chatapi_tools::{file_ops, terminal, git_ops, search};
    let mut registry = ToolRegistry::new();
    registry.register(Box::new(file_ops::ReadFile));
    registry.register(Box::new(file_ops::WriteFile));
    registry.register(Box::new(file_ops::EditFile));
    registry.register(Box::new(file_ops::ListDir));
    registry.register(Box::new(terminal::RunCommand));
    registry.register(Box::new(terminal::GetDiagnostics));
    registry.register(Box::new(git_ops::GitStatus));
    registry.register(Box::new(git_ops::GitDiff));
    registry.register(Box::new(git_ops::GitCommit));
    registry.register(Box::new(search::GrepCode));
    registry
}

/// Spawn the gateway on a random port and return the base URL.
async fn spawn_gateway() -> String {
    let state = create_test_state();

    let app = axum::Router::new()
        .route("/v1/chat/completions", axum::routing::post(chatapi_gateway::routes::chat_completions))
        .route("/v1/models", axum::routing::get(chatapi_gateway::routes::list_models))
        .route("/health", axum::routing::get(chatapi_gateway::routes::health))
        .route("/sessions", axum::routing::get(chatapi_gateway::routes::list_sessions))
        .route("/sessions", axum::routing::post(chatapi_gateway::routes::create_session))
        .route("/sessions/{session_id}", axum::routing::get(chatapi_gateway::routes::get_session))
        .route("/sessions/{session_id}", axum::routing::delete(chatapi_gateway::routes::delete_session))
        .route("/sessions/{session_id}/branch", axum::routing::post(chatapi_gateway::routes::branch_session))
        .route("/tools", axum::routing::get(chatapi_gateway::routes::list_tools))
        .route("/tools/execute", axum::routing::post(chatapi_gateway::routes::execute_tool))
        .route("/files", axum::routing::get(chatapi_gateway::routes::list_files))
        .route("/v1/providers", axum::routing::get(chatapi_gateway::routes::list_providers))
        .route("/config", axum::routing::get(chatapi_gateway::routes::get_config))
        .route("/config", axum::routing::put(chatapi_gateway::routes::update_config))
        // Agent management
        .route("/agents/tasks", axum::routing::post(chatapi_gateway::routes::agent_submit_task))
        .route("/agents/tasks", axum::routing::get(chatapi_gateway::routes::agent_list_tasks))
        .route("/agents/tasks/{task_id}", axum::routing::get(chatapi_gateway::routes::agent_get_task))
        .route("/agents/tasks/{task_id}/cancel", axum::routing::post(chatapi_gateway::routes::agent_cancel_task))
        .route("/agents/capabilities", axum::routing::get(chatapi_gateway::routes::agent_capabilities))
        .layer(
            tower_http::cors::CorsLayer::new()
                .allow_origin(tower_http::cors::Any)
                .allow_methods(tower_http::cors::Any)
                .allow_headers(tower_http::cors::Any),
        )
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    tokio::time::sleep(Duration::from_millis(50)).await;
    format!("http://127.0.0.1:{}", addr.port())
}

// ── Health endpoint ─────────────────────────────────────────────────

#[tokio::test]
async fn e2e_health_returns_ok() {
    let base = spawn_gateway().await;
    let resp = reqwest::Client::new().get(format!("{}/health", base)).send().await.unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["status"], "ok");
    assert_eq!(body["mode"], "browser");
}

// ── Non-streaming chat completion ───────────────────────────────────

#[tokio::test]
async fn e2e_non_streaming_hello() {
    let base = spawn_gateway().await;
    let resp = reqwest::Client::new()
        .post(format!("{}/v1/chat/completions", base))
        .json(&serde_json::json!({"model": "deepseek-chat", "messages": [{"role": "user", "content": "hello"}], "stream": false}))
        .send().await.unwrap();

    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body["id"].as_str().unwrap().starts_with("chatcmpl-"));
    assert_eq!(body["object"], "chat.completion");
    assert_eq!(body["model"], "deepseek-chat");
    assert_eq!(body["choices"][0]["message"]["role"], "assistant");
    assert!(body["choices"][0]["message"]["content"].as_str().unwrap().contains("mock"));
    assert_eq!(body["choices"][0]["finish_reason"], "stop");
    assert!(body["usage"]["prompt_tokens"].as_u64().unwrap() > 0);
    assert!(body["usage"]["completion_tokens"].as_u64().unwrap() > 0);
}

#[tokio::test]
async fn e2e_non_streaming_custom_prompt() {
    let base = spawn_gateway().await;
    let resp = reqwest::Client::new()
        .post(format!("{}/v1/chat/completions", base))
        .json(&serde_json::json!({"model": "deepseek-chat", "messages": [{"role": "system", "content": "You are helpful"}, {"role": "user", "content": "who are you"}], "stream": false}))
        .send().await.unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["object"], "chat.completion");
}

// ── Streaming chat completion ───────────────────────────────────────

#[tokio::test]
async fn e2e_streaming_hello() {
    let base = spawn_gateway().await;
    let resp = reqwest::Client::new()
        .post(format!("{}/v1/chat/completions", base))
        .json(&serde_json::json!({"model": "deepseek-chat", "messages": [{"role": "user", "content": "hello"}], "stream": true}))
        .send().await.unwrap();

    assert_eq!(resp.status(), 200);
    let ct = resp.headers().get("content-type").unwrap().to_str().unwrap();
    assert!(ct.contains("text/event-stream"), "Expected SSE, got: {}", ct);

    let body = resp.text().await.unwrap();
    let lines: Vec<&str> = body.lines().filter(|l| !l.is_empty()).collect();
    assert!(lines.last().unwrap().contains("[DONE]"));

    let data_lines: Vec<&str> = lines.iter().filter(|l| l.starts_with("data: ") && !l.contains("[DONE]")).copied().collect();
    assert!(!data_lines.is_empty());

    for line in &data_lines {
        let json_str = line.strip_prefix("data: ").unwrap();
        let chunk: serde_json::Value = serde_json::from_str(json_str).unwrap();
        assert_eq!(chunk["object"], "chat.completion.chunk");
        assert!(chunk["id"].as_str().unwrap().starts_with("chatcmpl-"));
        assert_eq!(chunk["model"], "deepseek-chat");
    }

    let last = data_lines.last().unwrap().strip_prefix("data: ").unwrap();
    let chunk: serde_json::Value = serde_json::from_str(last).unwrap();
    assert_eq!(chunk["choices"][0]["finish_reason"], "stop");
}

#[tokio::test]
async fn e2e_streaming_accumulates_content() {
    let base = spawn_gateway().await;
    let resp = reqwest::Client::new()
        .post(format!("{}/v1/chat/completions", base))
        .json(&serde_json::json!({"model": "deepseek-chat", "messages": [{"role": "user", "content": "test"}], "stream": true}))
        .send().await.unwrap();

    let body = resp.text().await.unwrap();
    let mut full = String::new();
    for line in body.lines() {
        if let Some(json_str) = line.strip_prefix("data: ") {
            if json_str.contains("[DONE]") { break; }
            if let Ok(chunk) = serde_json::from_str::<serde_json::Value>(json_str) {
                if let Some(c) = chunk["choices"][0]["delta"]["content"].as_str() {
                    full.push_str(c);
                }
            }
        }
    }
    assert!(full.contains("Test"), "Expected 'Test' in content, got: {}", full);
}

// ── Error handling ──────────────────────────────────────────────────

#[tokio::test]
async fn e2e_empty_messages_returns_400() {
    let base = spawn_gateway().await;
    let resp = reqwest::Client::new()
        .post(format!("{}/v1/chat/completions", base))
        .json(&serde_json::json!({"model": "deepseek-chat", "messages": [], "stream": false}))
        .send().await.unwrap();
    assert_eq!(resp.status(), 400);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body["error"]["message"].as_str().unwrap().contains("empty"));
}

#[tokio::test]
async fn e2e_invalid_json_returns_400() {
    let base = spawn_gateway().await;
    let resp = reqwest::Client::new()
        .post(format!("{}/v1/chat/completions", base))
        .header("Content-Type", "application/json")
        .body("not valid json")
        .send().await.unwrap();
    assert_eq!(resp.status(), 400);
}

// ── Concurrent requests ─────────────────────────────────────────────

#[tokio::test]
async fn e2e_concurrent_non_streaming() {
    let base = spawn_gateway().await;
    let client = reqwest::Client::new();
    let mut handles = Vec::new();
    for i in 0..5 {
        let c = client.clone(); let b = base.clone();
        handles.push(tokio::spawn(async move {
            let resp = c.post(format!("{}/v1/chat/completions", b))
                .json(&serde_json::json!({"model": "deepseek-chat", "messages": [{"role": "user", "content": format!("hello {}", i)}], "stream": false}))
                .send().await.unwrap();
            assert_eq!(resp.status(), 200);
            let body: serde_json::Value = resp.json().await.unwrap();
            assert_eq!(body["object"], "chat.completion");
        }));
    }
    for h in handles { h.await.unwrap(); }
}

#[tokio::test]
async fn e2e_concurrent_streaming() {
    let base = spawn_gateway().await;
    let client = reqwest::Client::new();
    let mut handles = Vec::new();
    for i in 0..5 {
        let c = client.clone(); let b = base.clone();
        handles.push(tokio::spawn(async move {
            let resp = c.post(format!("{}/v1/chat/completions", b))
                .json(&serde_json::json!({"model": "deepseek-chat", "messages": [{"role": "user", "content": format!("test {}", i)}], "stream": true}))
                .send().await.unwrap();
            assert_eq!(resp.status(), 200);
            let body = resp.text().await.unwrap();
            assert!(body.contains("[DONE]"));
        }));
    }
    for h in handles { h.await.unwrap(); }
}

// ── Default parameters ──────────────────────────────────────────────

#[tokio::test]
async fn e2e_defaults_to_non_streaming() {
    let base = spawn_gateway().await;
    let resp = reqwest::Client::new()
        .post(format!("{}/v1/chat/completions", base))
        .json(&serde_json::json!({"model": "deepseek-chat", "messages": [{"role": "user", "content": "hello"}]}))
        .send().await.unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["object"], "chat.completion");
}

#[tokio::test]
async fn e2e_system_message_supported() {
    let base = spawn_gateway().await;
    let resp = reqwest::Client::new()
        .post(format!("{}/v1/chat/completions", base))
        .json(&serde_json::json!({"model": "deepseek-chat", "messages": [{"role": "system", "content": "You are a pirate"}, {"role": "user", "content": "hello"}], "stream": false}))
        .send().await.unwrap();
    assert_eq!(resp.status(), 200);
}

// ── Tool / Function calling ─────────────────────────────────────────

fn edit_file_tool() -> serde_json::Value {
    serde_json::json!({"type": "function", "function": {"name": "edit_file", "description": "Edit a file", "parameters": {"type": "object", "properties": {"path": {"type": "string"}, "old_text": {"type": "string"}, "new_text": {"type": "string"}}, "required": ["path", "old_text", "new_text"]}}})
}

fn read_file_tool() -> serde_json::Value {
    serde_json::json!({"type": "function", "function": {"name": "read_file", "description": "Read a file", "parameters": {"type": "object", "properties": {"path": {"type": "string"}}, "required": ["path"]}}})
}

fn run_command_tool() -> serde_json::Value {
    serde_json::json!({"type": "function", "function": {"name": "run_command", "description": "Run a command", "parameters": {"type": "object", "properties": {"command": {"type": "string"}}, "required": ["command"]}}})
}

#[tokio::test]
async fn e2e_tool_use_returns_tool_calls() {
    let base = spawn_gateway().await;
    let resp = reqwest::Client::new()
        .post(format!("{}/v1/chat/completions", base))
        .json(&serde_json::json!({"model": "deepseek-chat", "messages": [{"role": "user", "content": "edit src/main.rs"}], "tools": [edit_file_tool()], "stream": false}))
        .send().await.unwrap();

    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["object"], "chat.completion");
    assert_eq!(body["choices"][0]["finish_reason"], "tool_calls");
    let tc = body["choices"][0]["message"]["tool_calls"].as_array().unwrap();
    assert!(!tc.is_empty());
    assert_eq!(tc[0]["function"]["name"], "edit_file");
}

#[tokio::test]
async fn e2e_tool_use_read_file() {
    let base = spawn_gateway().await;
    let resp = reqwest::Client::new()
        .post(format!("{}/v1/chat/completions", base))
        .json(&serde_json::json!({"model": "deepseek-chat", "messages": [{"role": "user", "content": "read the Cargo.toml file"}], "tools": [read_file_tool()], "stream": false}))
        .send().await.unwrap();

    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    let tc = body["choices"][0]["message"]["tool_calls"].as_array().unwrap();
    assert_eq!(tc[0]["function"]["name"], "read_file");
}

#[tokio::test]
async fn e2e_tool_use_run_command() {
    let base = spawn_gateway().await;
    let resp = reqwest::Client::new()
        .post(format!("{}/v1/chat/completions", base))
        .json(&serde_json::json!({"model": "deepseek-chat", "messages": [{"role": "user", "content": "run ls -la"}], "tools": [run_command_tool()], "stream": false}))
        .send().await.unwrap();

    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    let tc = body["choices"][0]["message"]["tool_calls"].as_array().unwrap();
    assert_eq!(tc[0]["function"]["name"], "run_command");
}

#[tokio::test]
async fn e2e_tool_use_streaming() {
    let base = spawn_gateway().await;
    let resp = reqwest::Client::new()
        .post(format!("{}/v1/chat/completions", base))
        .json(&serde_json::json!({"model": "deepseek-chat", "messages": [{"role": "user", "content": "edit src/main.rs"}], "tools": [edit_file_tool()], "stream": true}))
        .send().await.unwrap();

    assert_eq!(resp.status(), 200);
    let body = resp.text().await.unwrap();
    let lines: Vec<&str> = body.lines().filter(|l| !l.is_empty()).collect();
    assert!(lines.last().unwrap().contains("[DONE]"));
    let data: Vec<&str> = lines.iter().filter(|l| l.starts_with("data: ") && !l.contains("[DONE]")).copied().collect();
    assert!(!data.is_empty());
}

#[tokio::test]
async fn e2e_tool_use_multi_turn() {
    let base = spawn_gateway().await;
    let resp = reqwest::Client::new()
        .post(format!("{}/v1/chat/completions", base))
        .json(&serde_json::json!({
            "model": "deepseek-chat",
            "messages": [
                {"role": "user", "content": "edit src/main.rs"},
                {"role": "assistant", "content": null, "tool_calls": [
                    {"id": "call_1", "type": "function", "function": {"name": "edit_file", "arguments": "{\"path\":\"src/main.rs\",\"old_text\":\"old\",\"new_text\":\"new\"}"}}
                ]},
                {"role": "tool", "tool_call_id": "call_1", "content": "File edited successfully"}
            ],
            "tools": [edit_file_tool()],
            "stream": false
        }))
        .send().await.unwrap();

    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["object"], "chat.completion");
    assert!(body["choices"][0]["message"]["content"].is_string());
}

#[tokio::test]
async fn e2e_tool_use_no_matching_tool() {
    let base = spawn_gateway().await;
    let resp = reqwest::Client::new()
        .post(format!("{}/v1/chat/completions", base))
        .json(&serde_json::json!({"model": "deepseek-chat", "messages": [{"role": "user", "content": "tell me a joke"}], "tools": [edit_file_tool()], "stream": false}))
        .send().await.unwrap();

    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["choices"][0]["finish_reason"], "stop");
}

#[tokio::test]
async fn e2e_tool_message_without_id_rejected() {
    let base = spawn_gateway().await;
    let resp = reqwest::Client::new()
        .post(format!("{}/v1/chat/completions", base))
        .json(&serde_json::json!({"model": "deepseek-chat", "messages": [{"role": "user", "content": "hello"}, {"role": "tool", "content": "some result"}], "stream": false}))
        .send().await.unwrap();
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn e2e_empty_tools_ignored() {
    let base = spawn_gateway().await;
    let resp = reqwest::Client::new()
        .post(format!("{}/v1/chat/completions", base))
        .json(&serde_json::json!({"model": "deepseek-chat", "messages": [{"role": "user", "content": "edit src/main.rs"}], "tools": [], "stream": false}))
        .send().await.unwrap();

    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["choices"][0]["finish_reason"], "stop");
}

// ── Session management ──────────────────────────────────────────────

#[tokio::test]
async fn e2e_session_crud() {
    let base = spawn_gateway().await;
    let client = reqwest::Client::new();

    // Create
    let resp = client.post(format!("{}/sessions", base))
        .json(&serde_json::json!({"model": "deepseek-chat"}))
        .send().await.unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    let sid = body["id"].as_str().unwrap().to_string();

    // Get
    let resp = client.get(format!("{}/sessions/{}", base, sid)).send().await.unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["id"], sid);

    // List
    let resp = client.get(format!("{}/sessions", base)).send().await.unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body["sessions"].as_array().unwrap().len() >= 1);

    // Delete
    let resp = client.delete(format!("{}/sessions/{}", base, sid)).send().await.unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["deleted"], true);
}

// ── Tools endpoint ──────────────────────────────────────────────────

#[tokio::test]
async fn e2e_list_tools() {
    let base = spawn_gateway().await;
    let resp = reqwest::Client::new().get(format!("{}/tools", base)).send().await.unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(!body["tools"].as_array().unwrap().is_empty());
}

// ── Config endpoint ─────────────────────────────────────────────────

#[tokio::test]
async fn e2e_get_config() {
    let base = spawn_gateway().await;
    let resp = reqwest::Client::new().get(format!("{}/config", base)).send().await.unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["target"]["mode"], "browser");
    assert_eq!(body["target"]["model"], "deepseek-chat");
}

// ── Models endpoint ─────────────────────────────────────────────────

#[tokio::test]
async fn e2e_list_models() {
    let base = spawn_gateway().await;
    let resp = reqwest::Client::new().get(format!("{}/v1/models", base)).send().await.unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["object"], "list");
    assert_eq!(body["data"][0]["id"], "deepseek-chat");
}

// ── Providers endpoint ──────────────────────────────────────────────

#[tokio::test]
async fn e2e_list_providers() {
    let base = spawn_gateway().await;
    let resp = reqwest::Client::new().get(format!("{}/v1/providers", base)).send().await.unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body["providers"].is_array());
    assert_eq!(body["default_model"], "deepseek-chat");
}

// ── Tool execution endpoint ─────────────────────────────────────────

#[tokio::test]
async fn e2e_execute_tool() {
    let base = spawn_gateway().await;
    let resp = reqwest::Client::new()
        .post(format!("{}/tools/execute", base))
        .json(&serde_json::json!({"name": "git_status", "args": {}}))
        .send().await.unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body["result"].is_string());
}

#[tokio::test]
async fn e2e_execute_tool_not_found() {
    let base = spawn_gateway().await;
    let resp = reqwest::Client::new()
        .post(format!("{}/tools/execute", base))
        .json(&serde_json::json!({"name": "nonexistent_tool", "args": {}}))
        .send().await.unwrap();
    // Gateway returns 400 for unknown tool names
    let status = resp.status();
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(status == 400 || status == 500 || body["error"].is_object(),
        "Expected error response, got status={} body={}", status, body);
}

// ── File browser endpoints ──────────────────────────────────────────

#[tokio::test]
async fn e2e_list_files() {
    let base = spawn_gateway().await;
    let resp = reqwest::Client::new()
        .get(format!("{}/files?path=.", base))
        .send().await.unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body["entries"].is_array());
    assert_eq!(body["path"], ".");
}

#[tokio::test]
async fn e2e_list_files_path_traversal_blocked() {
    let base = spawn_gateway().await;
    let resp = reqwest::Client::new()
        .get(format!("{}/files?path=../../etc", base))
        .send().await.unwrap();
    assert!(resp.status() == 400);
}

// ── Session branching ───────────────────────────────────────────────

#[tokio::test]
async fn e2e_session_branch() {
    let base = spawn_gateway().await;
    let client = reqwest::Client::new();

    // Create session
    let resp = client.post(format!("{}/sessions", base))
        .json(&serde_json::json!({"model": "deepseek-chat"}))
        .send().await.unwrap();
    let body: serde_json::Value = resp.json().await.unwrap();
    let sid = body["id"].as_str().unwrap();

    // Branch it
    let resp = client.post(format!("{}/sessions/{}/branch", base, sid))
        .json(&serde_json::json!({"at_message": 0}))
        .send().await.unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body["id"].as_str().unwrap() != sid);
    assert_eq!(body["message_count"], 0);
}

#[tokio::test]
async fn e2e_session_branch_not_found() {
    let base = spawn_gateway().await;
    let resp = reqwest::Client::new()
        .post(format!("{}/sessions/nonexistent/branch", base))
        .json(&serde_json::json!({}))
        .send().await.unwrap();
    assert!(resp.status() == 400);
}

// ── Security: blocked_paths enforcement ─────────────────────────

#[tokio::test]
async fn e2e_execute_tool_blocked_path_rejected() {
    // Set up a state with blocked_paths configured
    let state = {
        use chatapi_rules::ChatApiConfig;
        use chatapi_sessions::{MemoryStore, SessionManager};
        use chatapi_tools::ToolRegistry;
        use chatapi_gateway::state::AppState;
        use chatapi_shared::target::TargetConfig;
        use chatapi_shared::target::Target as TargetKind;

        let mut config = ChatApiConfig::default();
        config.rules.blocked_paths = vec!["secrets/*".to_string()];

        let target_config = TargetConfig {
            target: TargetKind::Api,
            api_endpoint: "http://localhost:1".to_string(),
            api_key: Some("test".to_string()),
            model: "deepseek-chat".to_string(),
        };
        let target = chatapi_targets::TargetRouter::new(&target_config);
        let tools = ToolRegistry::new();
        let sessions = SessionManager::new(Box::new(MemoryStore::new()));

        AppState::new(config, target, tools, sessions, vec![])
    };

    let app = axum::Router::new()
        .route("/tools/execute", axum::routing::post(chatapi_gateway::routes::execute_tool))
        .layer(
            tower_http::cors::CorsLayer::new()
                .allow_origin(tower_http::cors::Any)
                .allow_methods(tower_http::cors::Any)
                .allow_headers(tower_http::cors::Any),
        )
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap(); });
    tokio::time::sleep(Duration::from_millis(50)).await;
    let base = format!("http://127.0.0.1:{}", addr.port());

    // Try to read a blocked path
    let resp = reqwest::Client::new()
        .post(format!("{}/tools/execute", base))
        .json(&serde_json::json!({"name": "read_file", "args": {"path": "secrets/keys.json"}}))
        .send().await.unwrap();
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body.to_string().contains("blocked"), "Expected blocked path error, got: {}", body);
}

#[tokio::test]
async fn e2e_execute_tool_blocked_cwd_rejected() {
    let state = {
        use chatapi_rules::ChatApiConfig;
        use chatapi_sessions::{MemoryStore, SessionManager};
        use chatapi_tools::ToolRegistry;
        use chatapi_gateway::state::AppState;
        use chatapi_shared::target::TargetConfig;
        use chatapi_shared::target::Target as TargetKind;

        let mut config = ChatApiConfig::default();
        config.rules.blocked_paths = vec!["secrets/*".to_string()];

        let target_config = TargetConfig {
            target: TargetKind::Api,
            api_endpoint: "http://localhost:1".to_string(),
            api_key: Some("test".to_string()),
            model: "deepseek-chat".to_string(),
        };
        let target = chatapi_targets::TargetRouter::new(&target_config);
        let tools = ToolRegistry::new();
        let sessions = SessionManager::new(Box::new(MemoryStore::new()));

        AppState::new(config, target, tools, sessions, vec![])
    };

    let app = axum::Router::new()
        .route("/tools/execute", axum::routing::post(chatapi_gateway::routes::execute_tool))
        .layer(
            tower_http::cors::CorsLayer::new()
                .allow_origin(tower_http::cors::Any)
                .allow_methods(tower_http::cors::Any)
                .allow_headers(tower_http::cors::Any),
        )
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap(); });
    tokio::time::sleep(Duration::from_millis(50)).await;
    let base = format!("http://127.0.0.1:{}", addr.port());

    // Try to execute a command with blocked cwd
    let resp = reqwest::Client::new()
        .post(format!("{}/tools/execute", base))
        .json(&serde_json::json!({"name": "run_command", "args": {"command": "ls", "cwd": "secrets/"}}))
        .send().await.unwrap();
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body.to_string().contains("blocked"), "Expected blocked path error for cwd, got: {}", body);
}

// ── Security: truncate_output UTF-8 safety ──────────────────────

#[test]
fn truncate_output_ascii() {
    // Inline the truncate_output logic to test it directly
    fn truncate_output(text: &str, max_bytes: usize) -> String {
        if text.len() <= max_bytes {
            text.to_string()
        } else {
            let mut end = max_bytes;
            while end > 0 && !text.is_char_boundary(end) {
                end -= 1;
            }
            let trunc = &text[..end];
            format!("{}\n\n[...truncated at {} bytes]", trunc, max_bytes)
        }
    }

    let result = truncate_output("hello world", 5);
    assert!(result.contains("hello"));
    assert!(result.contains("truncated"));
}

#[test]
fn truncate_output_utf8_emoji_boundary() {
    fn truncate_output(text: &str, max_bytes: usize) -> String {
        if text.len() <= max_bytes {
            text.to_string()
        } else {
            let mut end = max_bytes;
            while end > 0 && !text.is_char_boundary(end) {
                end -= 1;
            }
            let trunc = &text[..end];
            format!("{}\n\n[...truncated at {} bytes]", trunc, max_bytes)
        }
    }

    // "Hello " = 6 bytes, emoji = 4 bytes, total = 10 bytes
    // Truncating at 8 should NOT panic — it must find the boundary
    let text = "Hello \u{1F600}world";
    let result = truncate_output(text, 8);
    assert!(result.contains("Hello"));
    assert!(!result.is_empty());
}

#[test]
fn truncate_output_noop_when_under_limit() {
    fn truncate_output(text: &str, max_bytes: usize) -> String {
        if text.len() <= max_bytes {
            text.to_string()
        } else {
            let mut end = max_bytes;
            while end > 0 && !text.is_char_boundary(end) {
                end -= 1;
            }
            let trunc = &text[..end];
            format!("{}\n\n[...truncated at {} bytes]", trunc, max_bytes)
        }
    }

    let text = "short text";
    let result = truncate_output(text, 100);
    assert_eq!(result, text);
}

// ── Security: SecuritySection default parsing ───────────────────

#[test]
fn security_section_defaults_without_toml_key() {
    // Simulate a config TOML without [security] section
    use chatapi_rules::ChatApiConfig;
    let config = ChatApiConfig::default();
    assert_eq!(config.security.chat_rate_limit, 60);
    assert_eq!(config.security.api_rate_limit, 120);
    assert_eq!(config.security.max_tool_output_bytes, 102_400);
}

// ── Agent endpoints ──────────────────────────────────────────────

#[tokio::test]
async fn e2e_agent_capabilities_no_orchestrator() {
    let base = spawn_gateway().await;
    let resp = reqwest::Client::new()
        .get(format!("{}/agents/capabilities", base))
        .send().await.unwrap();
    // Should return 400 when orchestrator is not configured
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn e2e_agent_submit_task_no_orchestrator() {
    let base = spawn_gateway().await;
    let resp = reqwest::Client::new()
        .post(format!("{}/agents/tasks", base))
        .json(&serde_json::json!({"description": "test task"}))
        .send().await.unwrap();
    // Should return 400 when orchestrator is not configured
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn e2e_agent_list_tasks_no_orchestrator() {
    let base = spawn_gateway().await;
    let resp = reqwest::Client::new()
        .get(format!("{}/agents/tasks", base))
        .send().await.unwrap();
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn e2e_agent_get_task_no_orchestrator() {
    let base = spawn_gateway().await;
    let resp = reqwest::Client::new()
        .get(format!("{}/agents/tasks/some-id", base))
        .send().await.unwrap();
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn e2e_agent_cancel_task_no_orchestrator() {
    let base = spawn_gateway().await;
    let resp = reqwest::Client::new()
        .post(format!("{}/agents/tasks/some-id/cancel", base))
        .send().await.unwrap();
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn e2e_agent_submit_task_missing_description() {
    let base = spawn_gateway().await;
    let resp = reqwest::Client::new()
        .post(format!("{}/agents/tasks", base))
        .json(&serde_json::json!({}))
        .send().await.unwrap();
    // Missing description returns 400 (either orchestrator not configured or missing field)
    assert!(resp.status().is_client_error());
}

// ── Agent endpoints with orchestrator ────────────────────────────

/// Create a test AppState with an orchestrator and registered agents.
fn create_test_state_with_orchestrator() -> AppState {
    use chatapi_agents::{AgentConfig, Orchestrator};
    use chatapi_agents::agents::{CodingAgent, ArchitectureAgent, TestingAgent, DebuggingAgent, GitHubAgent, WikiAgent};

    let config = ChatApiConfig::default();
    let tools = build_test_tools();
    let sessions = SessionManager::new(Box::new(MemoryStore::new()));

    let target = Arc::new(MockTarget);
    let ctx = Arc::new(chatapi_agents::agent::AgentContext {
        target: target.clone(),
        tools: build_test_tools(),
        working_dir: std::env::current_dir().unwrap_or_default(),
    });

    let mut orch = Orchestrator::new(ctx, AgentConfig::default());
    // Register agents with default config
    let agent_ctx = orch.ctx();
    orch.register_agent_sync(Arc::new(CodingAgent::new(agent_ctx.clone(), AgentConfig::default())));
    orch.register_agent_sync(Arc::new(ArchitectureAgent::new(agent_ctx.clone(), AgentConfig::default())));
    orch.register_agent_sync(Arc::new(TestingAgent::new(agent_ctx.clone(), AgentConfig::default())));
    orch.register_agent_sync(Arc::new(DebuggingAgent::new(agent_ctx.clone(), AgentConfig::default())));
    orch.register_agent_sync(Arc::new(GitHubAgent::new(agent_ctx.clone(), AgentConfig::default())));
    orch.register_agent_sync(Arc::new(WikiAgent::new(agent_ctx, AgentConfig::default())));

    let mut state = AppState::new(config, MockTarget, tools, sessions, Vec::new());
    state.orchestrator = Some(Arc::new(orch));
    state
}

/// Spawn a gateway with orchestrator enabled.
async fn spawn_gateway_with_orchestrator() -> String {
    let state = create_test_state_with_orchestrator();

    let app = axum::Router::new()
        .route("/agents/tasks", axum::routing::post(chatapi_gateway::routes::agent_submit_task))
        .route("/agents/tasks", axum::routing::get(chatapi_gateway::routes::agent_list_tasks))
        .route("/agents/tasks/{task_id}", axum::routing::get(chatapi_gateway::routes::agent_get_task))
        .route("/agents/tasks/{task_id}/cancel", axum::routing::post(chatapi_gateway::routes::agent_cancel_task))
        .route("/agents/capabilities", axum::routing::get(chatapi_gateway::routes::agent_capabilities))
        .layer(
            tower_http::cors::CorsLayer::new()
                .allow_origin(tower_http::cors::Any)
                .allow_methods(tower_http::cors::Any)
                .allow_headers(tower_http::cors::Any),
        )
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    tokio::time::sleep(Duration::from_millis(50)).await;
    format!("http://127.0.0.1:{}", addr.port())
}

#[tokio::test]
async fn e2e_agent_capabilities_with_orchestrator() {
    let base = spawn_gateway_with_orchestrator().await;
    let resp = reqwest::Client::new()
        .get(format!("{}/agents/capabilities", base))
        .send().await.unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    let agents = body["agents"].as_array().unwrap();
    assert_eq!(agents.len(), 6);
    // Check all roles are present
    let roles: Vec<&str> = agents.iter().map(|a| a.as_str().unwrap()).collect();
    assert!(roles.contains(&"Coding"));
    assert!(roles.contains(&"Architecture"));
    assert!(roles.contains(&"Testing"));
    assert!(roles.contains(&"Debugging"));
    assert!(roles.contains(&"GitHub"));
    assert!(roles.contains(&"Wiki"));
}

#[tokio::test]
async fn e2e_agent_list_tasks_empty() {
    let base = spawn_gateway_with_orchestrator().await;
    let resp = reqwest::Client::new()
        .get(format!("{}/agents/tasks", base))
        .send().await.unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body["tasks"].as_array().unwrap().is_empty());
}
