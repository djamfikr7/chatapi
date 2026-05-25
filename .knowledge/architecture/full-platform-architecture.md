# Full Platform Architecture

## Overview
Plugin-based architecture extending the initial ChatAPI bridge into a full-featured IDE agent platform.

## Crate Structure
```
chatapi/
├── gateway/        # Axum API server, SSE streaming, WebSocket for dashboard
├── shared/         # Types, ToolProvider trait, TargetProvider trait, parser
├── tools/          # Built-in tool implementations
│   ├── file_ops    # read_file, write_file, edit_file, list_dir, apply_patch
│   ├── terminal    # run_command, get_diagnostics
│   ├── git_ops     # git_status, git_diff, git_commit, git_log, git_show
│   └── search      # grep_code, search_symbols
├── targets/        # Backend implementations
│   ├── cdp         # Chrome DevTools Protocol (browser mode)
│   ├── api         # Direct API mode (OpenAI/DeepSeek/Anthropic)
│   └── mcp_client  # MCP protocol client
├── sessions/       # Conversation management, history, branching
├── rules/          # .chatapi config, system prompt templates
├── ringbuf/        # Lock-free IPC (existing)
└── dashboard/      # SolidJS frontend
```

## Core Traits

### ToolProvider
Every tool implements this trait:
- `name()` → tool name
- `description()` → human-readable description
- `parameters_schema()` → JSON Schema for parameters
- `execute(args, context)` → ToolResult

### TargetProvider
Each backend (CDP, API, MCP) implements this:
- `name()` → "cdp", "api", "mcp"
- `health_check()` → bool
- `send_request(req)` → CompletionResponse
- `stream_request(req)` → CompletionStream

### SessionStore
Persistence for sessions:
- `save(session)`, `load(id)`, `list()`, `delete(id)`
- Implementations: in-memory, SQLite

## Data Flow
```
IDE (Cursor/Continue/etc.)
  │ HTTP/SSE or WebSocket
  ▼
Gateway (Axum)
  ├─ Session Manager → SessionStore
  ├─ Rules Engine → .chatapi/config.toml
  ├─ Tool Registry → Built-in + MCP + custom tools
  └─ Target Router → TargetProvider
                       ├─ CDP (browser)
                       ├─ API (direct)
                       └─ MCP Client
                            │
                            ▼
                  Frontend Dashboard (SolidJS + WebSocket)
```

## Key Design Decisions
- **Target abstraction**: CDP, MCP, and API backends behind unified TargetProvider trait
- **Tool system**: Built-in tools + MCP client + custom trait-based tools
- **Sessions**: In-memory with optional SQLite persistence
- **Frontend**: SolidJS + WebSocket real-time updates
- **Rules**: Project-level .chatapi config file with system prompt templates

## See Also
- [[architecture-overview]] - Original system architecture
- [[gateway-module]] - API gateway
- [[cdp-engine-module]] - CDP automation
