# ChatAPI — Full Platform Specifications

## Version: 2.0
## Date: 2026-05-25
## Status: Active Development

---

## 1. Overview

ChatAPI is a high-performance, open-source middleware that exposes any free or paid LLM chat
as an OpenAI-compatible API with full IDE agent support. It bridges IDE agents (Cursor,
Continue, Aider, Copilot Workspace, Windsurf, Cline, Roo Code) to LLM backends via:

- **Browser automation** (CDP) for free LLM chats (DeepSeek, ChatGPT, etc.)
- **Direct API** for paid endpoints (OpenAI, DeepSeek API, Anthropic, etc.)
- **MCP servers** for extensible tool capabilities

## 2. Architecture

Plugin-based Rust workspace with unified traits:

```
IDE (Cursor/Continue/etc.)
  │ HTTP/SSE or WebSocket
  ▼
Gateway (Axum) ─── Session Manager ──▶ SessionStore
  │                  Rules Engine ──▶ .chatapi/config.toml
  │                  Tool Registry ──▶ Built-in + MCP + custom tools
  └─ Target Router ──▶ TargetProvider
                         ├─ CDP (browser automation)
                         ├─ API (direct OpenAI-compatible)
                         └─ MCP Client (tool servers)
                              │
                              ▼
                    Frontend Dashboard (SolidJS + WebSocket)
```

### 2.1 Crates

| Crate | Purpose |
|-------|---------|
| `gateway/` | Axum API server, SSE streaming, WebSocket for dashboard |
| `shared/` | Types, ToolProvider trait, TargetProvider trait, parser |
| `tools/` | Built-in tool implementations |
| `targets/` | Backend implementations (CDP, API, MCP) |
| `sessions/` | Conversation management, history, branching |
| `rules/` | .chatapi config, system prompt templates |
| `ringbuf/` | Lock-free IPC buffer |
| `dashboard/` | SolidJS frontend |

## 3. API Surface

### 3.1 OpenAI-Compatible Endpoints

| Method | Path | Description |
|--------|------|-------------|
| POST | `/v1/chat/completions` | Chat completion (stream + non-stream) |
| GET | `/v1/models` | List available models |
| GET | `/health` | Health check |

### 3.2 Management Endpoints

| Method | Path | Description |
|--------|------|-------------|
| GET | `/sessions` | List sessions |
| POST | `/sessions` | Create session |
| GET | `/sessions/:id` | Get session with full history |
| DELETE | `/sessions/:id` | Delete session |
| POST | `/sessions/:id/branch` | Create branch |
| GET | `/tools` | List registered tools |
| GET | `/tools/:name` | Get tool schema |
| POST | `/tools/:name/test` | Test tool with sample input |
| GET | `/config` | Get current config |
| PUT | `/config` | Update config |
| GET | `/logs` | SSE stream of tool execution logs |
| WS | `/ws` | WebSocket for dashboard real-time updates |

## 4. Tool System

### 4.1 ToolProvider Trait

```rust
#[async_trait]
pub trait ToolProvider: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters_schema(&self) -> serde_json::Value;
    async fn execute(&self, args: serde_json::Value, ctx: &ToolContext) -> Result<ToolResult>;
}
```

### 4.2 Built-in Tools

#### File Operations
| Tool | Parameters | Description |
|------|-----------|-------------|
| `read_file` | `{ path }` | Read file contents |
| `write_file` | `{ path, content }` | Write/create file |
| `edit_file` | `{ path, old_text, new_text }` | Edit with text replacement |
| `list_dir` | `{ path, recursive? }` | List directory contents |
| `apply_patch` | `{ diff }` | Apply unified diff with conflict detection |

#### Terminal & Diagnostics
| Tool | Parameters | Description |
|------|-----------|-------------|
| `run_command` | `{ command, cwd?, timeout_ms? }` | Execute shell command |
| `get_diagnostics` | `{ path? }` | Get LSP/compiler errors |

#### Git Operations
| Tool | Parameters | Description |
|------|-----------|-------------|
| `git_status` | `{}` | Show working tree status |
| `git_diff` | `{ path?, staged? }` | Show unified diff |
| `git_commit` | `{ message, files? }` | Create commit |
| `git_log` | `{ limit? }` | Recent commit history |
| `git_show` | `{ ref }` | Show commit details |

#### Search
| Tool | Parameters | Description |
|------|-----------|-------------|
| `grep_code` | `{ pattern, path?, glob? }` | Search file contents |
| `search_symbols` | `{ query, kind? }` | Find symbol definitions |

### 4.3 Tool Safety
- Path validation against `blocked_paths` from rules config
- Command sandboxing with optional allowlist
- Timeout enforcement on all tools
- Output size limits (configurable, default 100KB)

## 5. Target Providers

### 5.1 TargetProvider Trait

```rust
#[async_trait]
pub trait TargetProvider: Send + Sync {
    fn name(&self) -> &str;
    async fn health_check(&self) -> bool;
    async fn send_request(&self, req: &CompletionRequest) -> Result<CompletionResponse>;
    async fn stream_request(&self, req: &CompletionRequest) -> Result<CompletionStream>;
}
```

### 5.2 Browser Mode (CDP)
- Automates free LLM chats via Chrome DevTools Protocol
- Injects tool definitions into system prompt
- Parses tool calls from LLM text output via `tool_parser`
- Supports DeepSeek Chat, ChatGPT, and other browser-based LLMs

### 5.3 API Mode
- Direct OpenAI-compatible API calls
- Native `tools` parameter (no parsing needed)
- Supports DeepSeek API, OpenAI, Anthropic, any compatible endpoint
- API key via environment variable

### 5.4 MCP Mode
- Model Context Protocol client
- Tool discovery via `tools/list`
- Resource access via `resources/read`
- Auto-registers discovered tools in tool registry

## 6. Session Management

### 6.1 Session Structure

```rust
pub struct Session {
    pub id: String,
    pub created_at: DateTime<Utc>,
    pub messages: Vec<ChatMessage>,
    pub branches: Vec<Branch>,
    pub metadata: SessionMetadata,
}
```

### 6.2 Persistence
- In-memory (default): fast, lost on restart
- SQLite (optional): persistent, configured via .chatapi/config.toml

## 7. Rules Engine

### 7.1 Configuration (`.chatapi/config.toml`)

```toml
[target]
mode = "browser"              # browser | api | mcp
model = "deepseek-chat"

[target.api]
endpoint = "https://api.deepseek.com/v1"
api_key_env = "DEEPSEEK_API_KEY"

[target.mcp]
servers = [
  { name = "filesystem", command = "mcp-server-fs" },
  { name = "github", command = "mcp-server-github", env = { GITHUB_TOKEN = "..." } }
]

[rules]
system_prompt = "You are a Rust expert."
working_dir = "."
allowed_tools = ["read_file", "edit_file", "run_command", "git_*"]
blocked_paths = [".env", ".git/config", "secrets/"]

[rules.context]
include_files = ["src/**/*.rs", "Cargo.toml"]
max_context_tokens = 50000

[sessions]
store = "memory"              # memory | sqlite
path = ".chatapi/sessions.db"

[dashboard]
port = 8091
theme = "dark"
```

## 8. Frontend Dashboard

### 8.1 Stack
- SolidJS + Tailwind CSS
- WebSocket for real-time data
- Vite for build tooling

### 8.2 Pages
1. **Dashboard**: Latency waterfall, session count, tool calls, config overview
2. **Sessions**: List, view, manage, delete sessions
3. **Tools**: View registered tools, test with sample input
4. **Config**: View/edit .chatapi/config.toml
5. **Logs**: Real-time tool execution log

### 8.3 Design
- Dark theme with gradient accents
- Responsive layout
- Real-time latency waterfall visualization
- Animated session state indicators

## 9. Performance Targets

| Metric | Target |
|--------|--------|
| First-token latency overhead | <30ms vs native API |
| Streaming jitter p99 | <5ms |
| Memory footprint | <50MB idle |
| CPU idle | <1% |
| Startup time | <200ms |
| Concurrent sessions | 10+ |

## 10. Testing

### Unit Tests
- Each tool: happy path, error cases, edge cases
- Rules engine: config parsing, system prompt construction
- Session manager: CRUD, branching, persistence
- Target providers: mock HTTP/WebSocket responses

### Integration Tests
- Gateway → Tool Registry → Tool execution
- Gateway → Target Provider → Response
- Session CRUD via API

### E2E Tests
- Full IDE agent simulation: request → tool_calls → tool results → response
- Multi-turn tool conversations
- Session persistence across restarts
- Dashboard real-time updates

## 11. Security

- Path traversal prevention: resolve symlinks, validate against blocked_paths
- Command injection prevention: sanitize arguments, allowlist approach
- API key management: never logged, env vars only
- MCP server isolation: subprocesses with limited permissions
- Rate limiting: configurable per-endpoint
- CORS: configurable allowed origins

## 12. Non-Goals

- No cloud deployment (local-first tool)
- No user authentication (single-user local tool)
- No model training or fine-tuning
- No IDE extension (works via OpenAI-compatible API)
