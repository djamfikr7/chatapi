# ChatAPI — Project Status
**Date:** 2026-05-27
**Branch:** master
**Tests:** 98 passing (shared:31, gateway:36, rules:19, ringbuf:6, sessions:6)
**Crates:** 10 (gateway, shared, tools, targets, sessions, rules, ringbuf, mcp, agents, frontend)
**GitHub:** https://github.com/djamfikr7/chatapi

---

## Phase 7: Multi-Agent Orchestration Framework ✓

### Architecture
```
User / API Request
       │
       ▼
┌──────────────┐
│  Orchestrator │ ◄── Decomposes tasks via LLM, manages lifecycle
└──────┬───────┘
       │ spawns sequentially
       ├──────────────┬──────────────┬──────────────┐
       ▼              ▼              ▼              ▼
┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐
│  Coding   │  │  Arch     │  │  Testing  │  │  Debugging│
│  Agent    │  │  Agent    │  │  Agent    │  │  Agent    │
└─────┬────┘  └─────┬────┘  └─────┬────┘  └─────┬────┘
      │             │             │             │
      ▼             ▼             ▼             ▼
  ToolRegistry  read-only    ToolRegistry  ToolRegistry
  TargetRouter  analysis     + run tests   + git tools
```

### New crate: `agents/`
- **Agent trait** — role(), name(), available_tools(), run()
- **CodingAgent** — full agentic loop (LLM → tool_calls → execute → result → LLM)
- **ArchitectureAgent** — read-only tools for codebase analysis
- **TestingAgent** — write_file + run_command for test authoring
- **DebuggingAgent** — full tool access + git for failure investigation
- **GitHubAgent** — git + command tools for PR/issue management
- **WikiAgent** — file ops for progress tracking and docs
- **Orchestrator** — LLM-driven task decomposition, sequential step execution, event broadcasting
- **TaskState** — shared key-value context between sub-agents
- **OrchestratorEvent** — real-time events for WebSocket monitoring

### Frontend Agent Dashboard
- **AgentPanel.tsx** — task submission form, task list with status dots, step detail view
- **agent.ts** — API client for all 5 agent endpoints
- **Agents tab** in left sidebar (alongside Files and Sessions)
- Auto-refresh every 3s for live task monitoring
- Cancel running tasks from the UI
- Step-level result/error display with role icons

### New API endpoints (22 total)
| Method | Path | Description |
|--------|------|-------------|
| POST | `/agents/tasks` | Submit high-level task |
| GET | `/agents/tasks` | List all tasks |
| GET | `/agents/tasks/:id` | Get task detail with steps |
| POST | `/agents/tasks/:id/cancel` | Cancel running task |
| GET | `/agents/capabilities` | List registered agent types |

### How it works
1. User submits task description via POST /agents/tasks
2. Orchestrator uses LLM to decompose into steps with agent role assignments
3. Steps execute sequentially — each step routed to the right agent
4. Each agent runs its own LLM loop with role-specific tool subset
5. Sub-agents share context via TaskState key-value store
6. All events broadcast via WebSocket for real-time monitoring

---

## QUICK START

```bash
# Production (single server)
cargo run --bin gateway
# Open: http://localhost:8090

# Development (hot reload)
cargo run --bin gateway     # Terminal 1
cd frontend && npm run dev  # Terminal 2
# Open: http://localhost:3000

# Docker
docker compose up --build
# Open: http://localhost:8090

# Browser mode (free LLM via Chrome)
LAUNCH_CHROME=1 cargo run --bin gateway
```

## API ENDPOINTS (22)

| Method | Path | Description |
|--------|------|-------------|
| POST | /v1/chat/completions | Chat (stream + non-stream, tool use) |
| GET | /v1/models | List available models |
| GET | /v1/providers | List configured providers |
| GET | /health | Health check |
| GET | /sessions | List sessions |
| POST | /sessions | Create session |
| GET | /sessions/:id | Get session |
| DELETE | /sessions/:id | Delete session |
| POST | /sessions/:id/branch | Branch session |
| GET | /tools | List tools |
| POST | /tools/execute | Execute tool directly |
| GET | /files | File browser |
| GET | /config | Get config |
| PUT | /config | Update config |
| GET | /ws | WebSocket events |
| GET | /ws/terminal | WebSocket terminal |
| POST | /agents/tasks | Submit agent task |
| GET | /agents/tasks | List agent tasks |
| GET | /agents/tasks/:id | Get agent task detail |
| POST | /agents/tasks/:id/cancel | Cancel agent task |
| GET | /agents/capabilities | List agent types |

## CONFIG

```toml
[target]
mode = "api"          # "api" or "browser"
model = "deepseek-chat"

[rules]
system_prompt = "You are a coding assistant."

[security]
chat_rate_limit = 60
api_rate_limit = 120
max_tool_output_bytes = 102400

[sessions]
store = "memory"      # "memory" or "file"
```
