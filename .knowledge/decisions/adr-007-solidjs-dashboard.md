# ADR-007: SolidJS Frontend Dashboard

## Status: Accepted

## Context
The platform needs a real-time monitoring dashboard for latency, sessions, tool execution, and configuration. Must be modern, intuitive, and visually attractive.

## Decision
Use SolidJS with Tailwind CSS for the frontend dashboard.

## Rationale
- SolidJS: Fine-grained reactivity, minimal overhead, fast updates
- Tailwind CSS: Utility-first, rapid styling, dark theme support
- WebSocket: Real-time data from gateway
- Optional WebGL for latency visualization (Phase 2)

## Pages
1. **Dashboard**: Overview with latency waterfall, session count, tool calls
2. **Sessions**: List, view, manage, delete sessions
3. **Tools**: View registered tools, test with sample input
4. **Config**: View/edit .chatapi/config.toml
5. **Logs**: Real-time tool execution log

## Consequences
- Positive: Modern, fast, responsive UI
- Positive: Real-time updates via WebSocket
- Positive: SolidJS has small bundle size
- Negative: Additional build tooling (Vite + SolidJS)
- Negative: Less ecosystem than React

## Alternatives Considered
- React: Larger ecosystem but more overhead
- Svelte: Good but SolidJS has finer reactivity
- Vanilla JS: Too much work for complex UI
