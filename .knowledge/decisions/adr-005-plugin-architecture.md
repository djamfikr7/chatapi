# ADR-005: Plugin-based Architecture

## Status: Accepted

## Context
The initial ChatAPI bridge handles basic chat and tool_call parsing, but lacks real tool implementations, session management, MCP support, and a frontend dashboard. The platform needs to be extensible to support new tools, backends, and features.

## Decision
Use a plugin-based architecture with core traits (ToolProvider, TargetProvider, SessionStore) that allow extensibility without modifying the gateway.

## Architecture
- **ToolProvider trait**: Every tool (built-in, MCP, custom) implements this
- **TargetProvider trait**: Each backend (CDP, API, MCP) implements this
- **SessionStore trait**: Persistence layer abstraction
- **Tool Registry**: Dynamic registration and dispatch
- **Config-driven**: .chatapi/config.toml controls behavior

## Consequences
- Positive: Extensible without gateway changes
- Positive: Clean separation of concerns
- Positive: Easy to test each component independently
- Positive: MCP support via standard protocol
- Negative: More initial complexity
- Negative: Trait design must be stable early

## Alternatives Considered
- Monolithic: Simple but rigid, recompile for every change
- Service-oriented: Maximum flexibility but highest complexity, IPC overhead
