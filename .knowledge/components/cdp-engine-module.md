# CDP Engine Design

## Target: DeepSeek Chat
URL: https://chat.deepseek.com/

## Connection
- Unix domain socket to Chrome instance
- Persistent connection with auto-reconnect
- Subscribe to `Network.webSocketFrameReceived` for response capture

## Prompt Injection
1. Locate input textarea via accessibility tree (cached node IDs)
2. `Input.dispatchKeyEvent` with humanized timing (±5ms jitter)
3. Press Enter to submit

## Response Detection
- Monitor `Network.webSocketFrameReceived` for streaming chunks
- Detect completion via silence timeout heuristic (500ms no new frames)
- Parse WebSocket frames for chat response content

## Error Handling
- Connection drop: auto-reconnect with session state reconstruction
- UI changes: refresh accessibility tree on navigation events
- Anti-bot: humanized timing + persistent browser profile

## See Also
- [[deepseek-cdp-notes]] - DeepSeek-specific CDP behavior
- [[architecture-overview]] - System architecture
