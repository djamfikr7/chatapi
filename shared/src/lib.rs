use serde::{Deserialize, Serialize};
use thiserror::Error;

// ── Role ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
}

// ── Errors ─────────────────────────────────────────────────────────────

#[derive(Error, Debug)]
pub enum ChatApiError {
    #[error("Invalid request: {0}")]
    InvalidRequest(String),
    #[error("Rate limited")]
    RateLimited,
    #[error("Browser not connected")]
    BrowserNotConnected,
    #[error("Automation failure: {0}")]
    AutomationFailure(String),
    #[error("CDP connection error: {0}")]
    CdpConnection(String),
    #[error("CDP protocol error: {0}")]
    CdpProtocol(String),
    #[error("Browser discovery error: {0}")]
    Discovery(String),
    #[error("Timeout waiting for response after {0}ms")]
    ResponseTimeout(u64),
    #[error("Ring buffer error: {0}")]
    RingBuffer(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Internal error: {0}")]
    Internal(String),
}

impl axum::response::IntoResponse for ChatApiError {
    fn into_response(self) -> axum::response::Response {
        use axum::http::StatusCode;

        let (status, code, message) = match &self {
            ChatApiError::InvalidRequest(msg) => (
                StatusCode::BAD_REQUEST,
                "invalid_request_error",
                msg.clone(),
            ),
            ChatApiError::RateLimited => (
                StatusCode::TOO_MANY_REQUESTS,
                "rate_limit_error",
                "Rate limited".to_string(),
            ),
            ChatApiError::BrowserNotConnected => (
                StatusCode::SERVICE_UNAVAILABLE,
                "service_unavailable",
                "Browser not connected".to_string(),
            ),
            ChatApiError::AutomationFailure(msg) | ChatApiError::Discovery(msg) => (
                StatusCode::BAD_GATEWAY,
                "automation_error",
                msg.clone(),
            ),
            other => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                other.to_string(),
            ),
        };

        let body = serde_json::json!({
            "error": {
                "message": message,
                "type": code,
                "code": serde_json::Value::Null,
            }
        });

        (status, axum::Json(body)).into_response()
    }
}

// ── Request types ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatCompletionRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    #[serde(default)]
    pub stream: bool,
    #[serde(default = "default_temperature")]
    pub temperature: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
}

pub type ChatRequest = ChatCompletionRequest;

fn default_temperature() -> f64 {
    0.7
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: Role,
    pub content: String,
}

// ── Non-streaming response ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatCompletionResponse {
    pub id: String,
    pub object: String,
    pub created: i64,
    pub model: String,
    pub choices: Vec<Choice>,
    pub usage: Usage,
}

impl ChatCompletionResponse {
    pub fn new(model: String, content: String, prompt_tokens: u32, completion_tokens: u32) -> Self {
        Self {
            id: generate_id(),
            object: "chat.completion".into(),
            created: now_epoch(),
            model,
            choices: vec![Choice {
                index: 0,
                message: ChatMessage {
                    role: Role::Assistant,
                    content,
                },
                finish_reason: Some("stop".into()),
            }],
            usage: Usage {
                prompt_tokens,
                completion_tokens,
                total_tokens: prompt_tokens + completion_tokens,
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Choice {
    pub index: u32,
    pub message: ChatMessage,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

// ── Streaming chunk ────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatCompletionChunk {
    pub id: String,
    pub object: String,
    pub created: i64,
    pub model: String,
    pub choices: Vec<ChunkChoice>,
}

impl ChatCompletionChunk {
    pub fn new_delta(model: &str, request_id: &str, content: &str) -> Self {
        Self {
            id: request_id.to_string(),
            object: "chat.completion.chunk".into(),
            created: now_epoch(),
            model: model.to_string(),
            choices: vec![ChunkChoice {
                index: 0,
                delta: Delta {
                    role: None,
                    content: Some(content.to_string()),
                },
                finish_reason: None,
            }],
        }
    }

    pub fn new_role_chunk(model: &str, request_id: &str) -> Self {
        Self {
            id: request_id.to_string(),
            object: "chat.completion.chunk".into(),
            created: now_epoch(),
            model: model.to_string(),
            choices: vec![ChunkChoice {
                index: 0,
                delta: Delta {
                    role: Some(Role::Assistant),
                    content: None,
                },
                finish_reason: None,
            }],
        }
    }

    pub fn new_finish(model: &str, request_id: &str) -> Self {
        Self {
            id: request_id.to_string(),
            object: "chat.completion.chunk".into(),
            created: now_epoch(),
            model: model.to_string(),
            choices: vec![ChunkChoice {
                index: 0,
                delta: Delta {
                    role: None,
                    content: None,
                },
                finish_reason: Some("stop".into()),
            }],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkChoice {
    pub index: u32,
    pub delta: Delta,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Delta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<Role>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
}

// ── CDP Types ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CdpCommand {
    SendPrompt {
        session_id: String,
        prompt: String,
    },
    NewSession,
    CloseSession {
        session_id: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CdpEvent {
    TokenReceived {
        session_id: String,
        token: String,
    },
    StreamComplete {
        session_id: String,
    },
    Error {
        session_id: String,
        message: String,
    },
    SessionCreated {
        session_id: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SessionStatus {
    Active,
    Idle,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CdpSession {
    pub id: String,
    pub status: SessionStatus,
}

impl CdpSession {
    pub fn new() -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            status: SessionStatus::Idle,
        }
    }
}

// ── Helpers ────────────────────────────────────────────────────────────

pub fn generate_id() -> String {
    let raw = uuid::Uuid::new_v4().to_string().replace('-', "");
    format!("chatcmpl-{}", &raw[..24])
}

pub fn now_epoch() -> i64 {
    chrono::Utc::now().timestamp()
}

pub type Result<T> = std::result::Result<T, ChatApiError>;

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use axum::response::IntoResponse;

    #[test]
    fn test_request_deserialize() {
        let json = r#"{
            "model": "deepseek-chat",
            "messages": [{"role": "user", "content": "hello"}],
            "stream": true
        }"#;
        let req: ChatCompletionRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.model, "deepseek-chat");
        assert!(req.stream);
        assert_eq!(req.messages[0].role, Role::User);
        assert_eq!(req.messages[0].content, "hello");
    }

    #[test]
    fn test_request_default_stream() {
        let json = r#"{
            "model": "deepseek-chat",
            "messages": [{"role": "user", "content": "hello"}]
        }"#;
        let req: ChatCompletionRequest = serde_json::from_str(json).unwrap();
        assert!(!req.stream);
        assert!((req.temperature - 0.7).abs() < f64::EPSILON);
    }

    #[test]
    fn test_response_serialize() {
        let resp = ChatCompletionResponse::new("deepseek-chat".into(), "hi there".into(), 10, 5);
        let json = serde_json::to_value(&resp).unwrap();
        assert_eq!(json["object"], "chat.completion");
        assert_eq!(json["choices"][0]["message"]["role"], "assistant");
        assert_eq!(json["choices"][0]["message"]["content"], "hi there");
        assert_eq!(json["choices"][0]["finish_reason"], "stop");
        assert_eq!(json["usage"]["prompt_tokens"], 10);
        assert_eq!(json["usage"]["total_tokens"], 15);
        assert!(json["id"].as_str().unwrap().starts_with("chatcmpl-"));
    }

    #[test]
    fn test_chunk_delta_serialize() {
        let chunk = ChatCompletionChunk::new_delta("deepseek-chat", "chatcmpl-123", "token");
        let json = serde_json::to_value(&chunk).unwrap();
        assert_eq!(json["object"], "chat.completion.chunk");
        assert_eq!(json["id"], "chatcmpl-123");
        assert_eq!(json["choices"][0]["delta"]["content"], "token");
        assert!(json["choices"][0]["delta"]["role"].is_null());
        assert!(json["choices"][0]["finish_reason"].is_null());
    }

    #[test]
    fn test_chunk_role_serialize() {
        let chunk = ChatCompletionChunk::new_role_chunk("deepseek-chat", "chatcmpl-123");
        let json = serde_json::to_value(&chunk).unwrap();
        assert_eq!(json["choices"][0]["delta"]["role"], "assistant");
        assert!(json["choices"][0]["delta"]["content"].is_null());
    }

    #[test]
    fn test_chunk_finish_serialize() {
        let chunk = ChatCompletionChunk::new_finish("deepseek-chat", "chatcmpl-123");
        let json = serde_json::to_value(&chunk).unwrap();
        assert_eq!(json["choices"][0]["finish_reason"], "stop");
        assert!(json["choices"][0]["delta"]["content"].is_null());
    }

    #[test]
    fn test_cdp_command_roundtrip() {
        let cmd = CdpCommand::SendPrompt {
            session_id: "abc".into(),
            prompt: "hello".into(),
        };
        let json = serde_json::to_string(&cmd).unwrap();
        assert!(json.contains("send_prompt"));
        let back: CdpCommand = serde_json::from_str(&json).unwrap();
        match back {
            CdpCommand::SendPrompt { session_id, prompt } => {
                assert_eq!(session_id, "abc");
                assert_eq!(prompt, "hello");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn test_cdp_event_roundtrip() {
        let event = CdpEvent::TokenReceived {
            session_id: "s1".into(),
            token: "hello".into(),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("token_received"));
        let back: CdpEvent = serde_json::from_str(&json).unwrap();
        match back {
            CdpEvent::TokenReceived { session_id, token } => {
                assert_eq!(session_id, "s1");
                assert_eq!(token, "hello");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn test_cdp_event_stream_complete() {
        let event = CdpEvent::StreamComplete { session_id: "s1".into() };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("stream_complete"));
    }

    #[test]
    fn test_cdp_command_new_session() {
        let cmd = CdpCommand::NewSession;
        let json = serde_json::to_string(&cmd).unwrap();
        assert!(json.contains("new_session"));
        let back: CdpCommand = serde_json::from_str(&json).unwrap();
        assert!(matches!(back, CdpCommand::NewSession));
    }

    #[test]
    fn test_session_status_serialize() {
        let session = CdpSession::new();
        assert_eq!(session.status, SessionStatus::Idle);
        let json = serde_json::to_string(&session).unwrap();
        assert!(json.contains("\"idle\""));
    }

    #[test]
    fn test_error_into_response() {
        use axum::http::StatusCode;
        let err = ChatApiError::InvalidRequest("missing model".into());
        let resp = err.into_response();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn test_role_deserialize() {
        let json = r#""user""#;
        let role: Role = serde_json::from_str(json).unwrap();
        assert_eq!(role, Role::User);

        let json = r#""assistant""#;
        let role: Role = serde_json::from_str(json).unwrap();
        assert_eq!(role, Role::Assistant);

        let json = r#""system""#;
        let role: Role = serde_json::from_str(json).unwrap();
        assert_eq!(role, Role::System);
    }

    #[test]
    fn test_generate_id_format() {
        let id = generate_id();
        assert!(id.starts_with("chatcmpl-"));
        assert_eq!(id.len(), 9 + 24); // "chatcmpl-" + 24 chars
    }

    #[test]
    fn test_backward_compat_alias() {
        let json = r#"{
            "model": "deepseek-chat",
            "messages": [{"role": "user", "content": "hello"}]
        }"#;
        let req: ChatRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.model, "deepseek-chat");
    }

    #[test]
    fn test_full_sse_flow_serialization() {
        // Simulates the exact SSE chunks an IDE would receive
        let model = "deepseek-chat";
        let req_id = "chatcmpl-test123";

        let role_chunk = ChatCompletionChunk::new_role_chunk(model, req_id);
        let json = serde_json::to_string(&role_chunk).unwrap();
        assert!(json.starts_with("data: ") == false); // just the JSON, gateway adds "data: "

        let delta1 = ChatCompletionChunk::new_delta(model, req_id, "Hello");
        let delta2 = ChatCompletionChunk::new_delta(model, req_id, " world");
        let finish = ChatCompletionChunk::new_finish(model, req_id);

        // All should serialize without error
        let _ = serde_json::to_string(&delta1).unwrap();
        let _ = serde_json::to_string(&delta2).unwrap();
        let _ = serde_json::to_string(&finish).unwrap();
    }
}
