# ChatAPI Knowledge Base — Map of Content

## Project Overview
- [ChatAPI Project](chatapi-project.md) — Rust LLM bridge evolved into full IDE agent platform
- [Implementation Status](implementation-status.md) — Current build progress across all phases
- [Full Platform Architecture](architecture/full-platform-architecture.md) — Complete system design

## Architecture Decisions
- [ADR-001: Ring Buffer](decisions/adr-001-ring-buffer.md)
- [ADR-002: OpenAI Compatibility](decisions/adr-002-openai-compatibility.md)
- [ADR-003: Axum Framework](decisions/adr-003-axum-framework.md)
- [ADR-004: Workspace Layout](decisions/adr-004-workspace-layout.md)
- [ADR-005: Plugin Architecture](decisions/adr-005-plugin-architecture.md)
- [ADR-006: Dual Target Mode](decisions/adr-006-dual-target-mode.md)
- [ADR-007: SolidJS Dashboard](decisions/adr-007-solidjs-dashboard.md)

## Components
- [Tool System](components/tool-system.md) — Built-in tools + MCP integration
- [Session Management](components/session-management.md) — Memory + file-backed session stores
- [Rules Engine](components/rules-engine.md) — Config, filtering, prompt building, context files
- [MCP Integration](components/mcp-integration.md) — Model Context Protocol client
- [Frontend Dashboard](components/frontend-dashboard.md) — SolidJS IDE UI (planned)

## Phase 2 Status
- **Targets:** API client working, CDP browser stub needs real implementation
- **Tools:** 10 built-in tools registered, MCP client functional
- **Sessions:** Memory + file stores working, 6 new tests
- **Rules:** Config, filtering, prompt building, context file inclusion working, 16 tests
- **Gateway:** All 10 endpoints wired to real components, 23 E2E tests
- **MCP:** JSON-RPC client, tool discovery, McpToolProvider wrapper
- **Total:** 82 tests passing

## Reference Blueprints (from Uber-Clone003)
- [Uber-Clone003 Master Blueprint](reference/uber-clone003-blueprint.md) — Full SSR with PAF, SEAO, Vault
- [Uber-Clone003 Project Status](reference/uber-clone003-status.md) — V1 implementation status
