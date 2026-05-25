# ChatAPI — Project Status
**Date:** 2026-05-25
**Branch:** master
**Tests:** 82 passing (shared:31, gateway:23, rules:16, ringbuf:6, sessions:6)
**GitHub:** https://github.com/djamfikr7/chatapi

---

## DONE

### Phase 1-2: Core + Integration ✓
- 9 workspace crates, 82 tests
- 10 built-in tools + MCP tool discovery
- Rules engine, session management, gateway with 17 endpoints

### Phase 3: Frontend + Real-time ✓
- SolidJS IDE: Monaco editor, terminal, chat panel, file tree
- WebSocket endpoint with EventBroadcaster
- CDP engine wired as TargetProvider (browser mode)

### Phase 4: Production Ready ✓
- Static file serving from gateway (single-server)
- Chrome auto-launch with LAUNCH_CHROME=1
- Docker Compose deployment
- File browser API (/files, /files/read)
- FileTree wired to real workspace files
- Config UI panel with MCP server management
- Session branching (POST /sessions/:id/branch)
- WebSocket terminal (/ws/terminal) — real shell in IDE

### Phase 5: Multi-Model + Polish ✓
- Multi-model support with provider configuration
- GET /v1/providers — list all configured providers
- Model selector in frontend chat panel
- Tool execution result display (diff view, command output)

## HOW TO RUN

```bash
# Development
cargo run --bin gateway        # Terminal 1: gateway
cd frontend && npm run dev     # Terminal 2: frontend
# Open: http://localhost:3000

# Production (single server)
cargo run --bin gateway
# Open: http://localhost:8090

# Docker
docker compose up --build
# Open: http://localhost:8090

# Browser mode (CDP)
LAUNCH_CHROME=1 cargo run --bin gateway
```

## API ENDPOINTS (17)

| Method | Path | Description |
|--------|------|-------------|
| POST | /v1/chat/completions | OpenAI-compatible chat |
| GET | /v1/models | List all models |
| GET | /v1/providers | List providers |
| GET | /health | Health check |
| GET | /sessions | List sessions |
| POST | /sessions | Create session |
| GET | /sessions/:id | Get session |
| DELETE | /sessions/:id | Delete session |
| POST | /sessions/:id/branch | Fork session |
| GET | /tools | List tools |
| POST | /tools/execute | Execute tool |
| GET | /config | Get config |
| PUT | /config | Update config |
| GET | /files | List directory |
| GET | /files/read | Read file |
| GET | /ws | WebSocket events |
| GET | /ws/terminal | WebSocket terminal |

## CONFIG EXAMPLE

```toml
[target]
mode = "api"
model = "deepseek-chat"

[target.api]
endpoint = "https://api.deepseek.com/v1"
api_key_env = "DEEPSEEK_API_KEY"

[[models.providers]]
name = "openai"
endpoint = "https://api.openai.com/v1"
api_key_env = "OPENAI_API_KEY"

[[models.providers.models]]
id = "gpt-4"
name = "GPT-4"
max_tokens = 8192

[[models.providers.models]]
id = "gpt-3.5-turbo"
name = "GPT-3.5 Turbo"
```
