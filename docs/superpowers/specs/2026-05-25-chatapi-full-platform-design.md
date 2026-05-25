# ChatAPI Full Platform Design

## Date: 2026-05-25
## Status: Approved
## Architecture: Plugin-based with unified traits

---

## 1. Problem Statement

ChatAPI bridges IDE agents (Cursor, Continue, Aider, Copilot Workspace, Windsurf, Cline, Roo Code) to free LLM chats via CDP browser automation. The initial implementation handles basic chat and tool_call parsing, but lacks the features that make a flagship IDE agent experience:

- **No real tool implementations** — tools are mocked, not functional
- **No session management** — no conversation history, branching, or resume
- **No MCP support** — Model Context Protocol is the standard for tool servers
- **No rules/custom instructions** — no `.chatapi` project config
- **No context management** — no codebase indexing or @-mention support
- **No frontend dashboard** — no monitoring, config, or session UI
- **No diagnostics integration** — no LSP/compiler error collection
- **No git operations** — no commit, diff, status tools
- **No diff/patch support** — no unified diff application with conflict resolution

## 2. Architecture

### 2.1 Crate Structure

```
chatapi/
├── gateway/        # Axum API server, SSE streaming, WebSocket for dashboard
├── shared/         # Types, ToolProvider trait, TargetProvider trait, parser
├── tools/          # Built-in tool implementations (NEW)
│   ├── file_ops    # read_file, write_file, edit_file, list_dir, apply_patch
│   ├── terminal    # run_command, get_diagnostics
│   ├── git_ops     # git_status, git_diff, git_commit, git_log, git_show
│   └── search      # grep_code, search_symbols
├── targets/        # Backend implementations (NEW)
│   ├── cdp         # Chrome DevTools Protocol (browser mode, existing)
│   ├── api         # Direct API mode (OpenAI/DeepSeek/Anthropic, NEW)
│   └── mcp_client  # MCP protocol client (NEW)
├── sessions/       # Conversation management (NEW)
├── rules/          # .chatapi config, system prompt templates (NEW)
├── ringbuf/        # Lock-free IPC (existing)
└── dashboard/      # SolidJS frontend (NEW)
```

### 2.2 Core Traits

```rust
// shared/src/traits.rs

/// Every tool implements this.
#[async_trait]
pub trait ToolProvider: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters_schema(&self) -> serde_json::Value;
    async fn execute(&self, args: serde_json::Value, ctx: &ToolContext) -> Result<ToolResult>;
}

/// Context passed to every tool execution.
pub struct ToolContext {
    pub session_id: String,
    pub working_dir: PathBuf,
    pub env: HashMap<String, String>,
    pub rules: RulesConfig,
}

/// Each backend (CDP, API, MCP) implements this.
#[async_trait]
pub trait TargetProvider: Send + Sync {
    fn name(&self) -> &str;
    async fn health_check(&self) -> bool;
    async fn send_request(&self, req: &CompletionRequest) -> Result<CompletionResponse>;
    async fn stream_request(&self, req: &CompletionRequest) -> Result<CompletionStream>;
}

/// Persistence for sessions.
#[async_trait]
pub trait SessionStore: Send + Sync {
    async fn save(&self, session: &Session) -> Result<()>;
    async fn load(&self, id: &str) -> Result<Option<Session>>;
    async fn list(&self) -> Result<Vec<SessionSummary>>;
    async fn delete(&self, id: &str) -> Result<()>;
}
```

### 2.3 Data Flow

```
IDE (Cursor/Continue/etc.)
  │ HTTP/SSE or WebSocket
  ▼
Gateway (Axum)
  ├─ Routes: /v1/chat/completions, /health, /sessions, /tools, /config, /ws
  ├─ Session Manager ──▶ SessionStore (in-memory or SQLite)
  ├─ Rules Engine ──▶ .chatapi/config.toml
  ├─ Tool Registry ──▶ Built-in tools + MCP tools + custom tools
  └─ Target Router ──▶ TargetProvider
                         ├─ CDP (browser automation)
                         ├─ API (direct OpenAI-compatible)
                         └─ MCP Client (tool servers)
                              │
                              ▼
                    Frontend Dashboard (SolidJS)
                    WebSocket for real-time updates
```

## 3. Tool System

### 3.1 Built-in Tools

#### File Operations
| Tool | Parameters | Description |
|------|-----------|-------------|
| `read_file` | `{ path: string }` | Read file contents |
| `write_file` | `{ path: string, content: string }` | Write/create file |
| `edit_file` | `{ path: string, old_text: string, new_text: string }` | Edit file with text replacement |
| `list_dir` | `{ path: string, recursive?: bool }` | List directory contents |
| `apply_patch` | `{ diff: string }` | Apply unified diff with conflict detection |

#### Terminal & Diagnostics
| Tool | Parameters | Description |
|------|-----------|-------------|
| `run_command` | `{ command: string, cwd?: string, timeout_ms?: number }` | Execute shell command |
| `get_diagnostics` | `{ path?: string }` | Get LSP/compiler errors and warnings |

#### Git Operations
| Tool | Parameters | Description |
|------|-----------|-------------|
| `git_status` | `{}` | Show working tree status |
| `git_diff` | `{ path?: string, staged?: bool }` | Show unified diff |
| `git_commit` | `{ message: string, files?: string[] }` | Create commit |
| `git_log` | `{ limit?: number }` | Recent commit history |
| `git_show` | `{ ref: string }` | Show commit details |

#### Search
| Tool | Parameters | Description |
|------|-----------|-------------|
| `grep_code` | `{ pattern: string, path?: string, glob?: string }` | Search file contents |
| `search_symbols` | `{ query: string, kind?: string }` | Find symbol definitions |

### 3.2 Tool Execution Flow

```
IDE sends tool_calls in request
  → Gateway validates tool_call_id references
  → Gateway checks allowed_tools from rules
  → Gateway checks blocked_paths for file tools
  → ToolRegistry.execute(tool_name, args, context)
    → Built-in tool? Execute directly
    → MCP tool? Forward to MCP server via JSON-RPC
    → Unknown? Return error
  → ToolResult returned
  → Gateway formats as tool role message
  → Sent back to LLM for continuation
```

### 3.3 Tool Safety

- **Path validation**: Blocked paths (`.env`, `secrets/`, `.git/config`) from rules config
- **Command sandboxing**: Optional allowlist for `run_command`
- **Timeout enforcement**: All tools have configurable timeouts
- **Output size limits**: Truncate large outputs (configurable, default 100KB)

## 4. Target Providers

### 4.1 Browser Mode (CDP)

Existing implementation. Enhanced with:
- Tool call injection into browser chat (system prompt with tool definitions)
- Response parsing via `tool_parser` (existing)
- Multi-turn conversation support (existing)
- Browser health monitoring with auto-reconnect

### 4.2 API Mode (NEW)

Direct OpenAI-compatible API calls. Supports:
- DeepSeek API, OpenAI, Anthropic, any OpenAI-compatible endpoint
- Native `tools` parameter (no parsing needed)
- Native `tool_calls` in responses
- API key management via environment variables

### 4.3 MCP Mode (NEW)

Model Context Protocol client. Supports:
- Tool discovery via `tools/list`
- Resource access via `resources/read`
- Prompt templates via `prompts/get`
- Multiple MCP servers (filesystem, github, custom)
- Auto-registers discovered tools in the tool registry

## 5. Session Management

### 5.1 Session Structure

```rust
pub struct Session {
    pub id: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub messages: Vec<ChatMessage>,
    pub branches: Vec<Branch>,
    pub metadata: SessionMetadata,
}

pub struct SessionMetadata {
    pub model: String,
    pub tools_used: Vec<String>,
    pub total_tokens: u64,
    pub total_tool_calls: u64,
    pub working_dir: PathBuf,
}
```

### 5.2 API Endpoints

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/sessions` | List all sessions |
| `POST` | `/sessions` | Create new session |
| `GET` | `/sessions/:id` | Get session with full history |
| `DELETE` | `/sessions/:id` | Delete session |
| `POST` | `/sessions/:id/branch` | Create branch from current state |
| `GET` | `/sessions/:id/messages` | Get messages with pagination |

### 5.3 Persistence

- **In-memory**: Default, fast, lost on restart
- **SQLite**: Optional, configured via `.chatapi/config.toml`
  ```toml
  [sessions]
  store = "sqlite"
  path = ".chatapi/sessions.db"
  ```

## 6. Rules Engine

### 6.1 Configuration File

`.chatapi/config.toml` in project root:

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
system_prompt = "You are a Rust expert. Prefer functional style."
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

### 6.2 System Prompt Injection

The rules engine constructs the system prompt from:
1. User-defined `system_prompt` from config
2. Active tool definitions (JSON schema)
3. Context files (included in first message)
4. Working directory info

## 7. Frontend Dashboard

### 7.1 Stack

- **SolidJS** for reactivity (minimal overhead, fast updates)
- **Tailwind CSS** for styling
- **WebSocket** for real-time data from gateway
- **WebGL** for latency visualization (optional, Phase 2)

### 7.2 Pages

| Page | Description |
|------|-------------|
| **Dashboard** | Overview: latency, sessions, tool calls, config |
| **Sessions** | List, view, manage, delete sessions |
| **Tools** | View registered tools, test with sample input |
| **Config** | View/edit `.chatapi/config.toml` |
| **Logs** | Real-time tool execution log, errors |

### 7.3 Real-time Features

- Latency waterfall: CDP recv → parse → buffer → SSE encode → flush
- Tool execution log: live updates as tools execute
- Session activity: new messages, tool calls
- Health status: browser connection, MCP server status

## 8. API Surface

### 8.1 OpenAI-Compatible Endpoints

| Method | Path | Description |
|--------|------|-------------|
| `POST` | `/v1/chat/completions` | Chat completion (stream + non-stream) |
| `GET` | `/v1/models` | List available models |
| `GET` | `/health` | Health check |

### 8.2 Management Endpoints

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/sessions` | List sessions |
| `POST` | `/sessions` | Create session |
| `GET` | `/sessions/:id` | Get session |
| `DELETE` | `/sessions/:id` | Delete session |
| `GET` | `/tools` | List registered tools |
| `GET` | `/tools/:name` | Get tool schema |
| `POST` | `/tools/:name/test` | Test tool with sample input |
| `GET` | `/config` | Get current config |
| `PUT` | `/config` | Update config |
| `GET` | `/logs` | SSE stream of tool execution logs |
| `WS` | `/ws` | WebSocket for dashboard real-time updates |

### 8.3 Error Responses

```json
{
  "error": {
    "message": "Tool 'edit_file' blocked: path 'secrets/keys.json' is in blocked_paths",
    "type": "tool_blocked",
    "code": "path_blocked"
  }
}
```

## 9. Implementation Plan

### Phase 1: Core Tools & Traits (Week 1)
- Define `ToolProvider`, `TargetProvider`, `SessionStore` traits
- Implement `tools/` crate: file_ops, terminal, git_ops, search
- Implement `apply_patch` with unified diff parsing
- Update tool registry to dispatch to built-in tools

### Phase 2: Sessions & Rules (Week 1)
- Implement `sessions/` crate: SessionManager, in-memory store
- Implement `rules/` crate: config parser, system prompt builder
- Add session API endpoints to gateway
- Wire rules engine into request flow

### Phase 3: API & MCP Targets (Week 2)
- Implement `targets/api/`: direct OpenAI-compatible API client
- Implement `targets/mcp_client/`: MCP protocol client
- Add target router to gateway
- Config-driven target selection

### Phase 4: Frontend Dashboard (Week 2)
- Scaffold SolidJS project in `dashboard/`
- Implement WebSocket connection to gateway
- Build dashboard pages: overview, sessions, tools, config, logs
- Latency waterfall visualization
- Real-time tool execution log

### Phase 5: Integration & Testing (Week 3)
- End-to-end tests with real IDE agents
- Benchmark suite: latency, throughput, memory
- Security audit: path traversal, command injection
- Documentation: API reference, config reference, deployment guide

## 10. Testing Strategy

### Unit Tests
- Each tool: happy path, error cases, edge cases
- Rules engine: config parsing, system prompt construction
- Session manager: CRUD, branching, persistence
- Target providers: mock HTTP/WebSocket responses

### Integration Tests
- Gateway → Tool Registry → Tool execution
- Gateway → Target Provider → Response
- Session CRUD via API
- Config loading and validation

### E2E Tests
- Full IDE agent simulation: request → tool_calls → tool results → response
- Multi-turn tool conversations
- Session persistence across restarts
- Dashboard real-time updates

## 11. Security Considerations

- **Path traversal**: Validate all paths against blocked_paths, resolve symlinks
- **Command injection**: Sanitize command arguments, allowlist approach
- **API key management**: Never log API keys, use env vars
- **MCP server isolation**: MCP servers run as subprocesses with limited permissions
- **Rate limiting**: Configurable per-endpoint rate limits
- **CORS**: Configurable allowed origins

## 12. Non-Goals

- No cloud deployment (local-first tool)
- No user authentication (single-user local tool)
- No model training or fine-tuning
- No IDE extension (works via OpenAI-compatible API, IDEs connect to it)
