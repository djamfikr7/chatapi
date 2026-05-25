# Frontend Dashboard Design

## Stack
- SolidJS for reactivity
- Tailwind CSS for styling
- WebSocket for real-time data from gateway
- Vite for build tooling

## Pages
1. **Dashboard**: Overview with latency waterfall, session count, tool calls, config
2. **Sessions**: List, view, manage, delete sessions
3. **Tools**: View registered tools, test with sample input
4. **Config**: View/edit .chatapi/config.toml
5. **Logs**: Real-time tool execution log

## Real-time Features
- Latency waterfall: CDP recv -> parse -> buffer -> SSE encode -> flush
- Tool execution log: live updates as tools execute
- Session activity: new messages, tool calls
- Health status: browser connection, MCP server status

## WebSocket Protocol
Gateway sends JSON messages:
```json
{"type": "latency", "data": {"cdp_recv": 2, "parse": 4, "buffer": 1, "sse_encode": 3, "flush": 2}}
{"type": "tool_call", "data": {"tool": "edit_file", "args": {...}, "duration_ms": 14}}
{"type": "session_update", "data": {"session_id": "...", "message_count": 12}}
{"type": "health", "data": {"browser_connected": true, "mcp_servers": 2}}
```

## See Also
- [[full-platform-architecture]] - Platform architecture
