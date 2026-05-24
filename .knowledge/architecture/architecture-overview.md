# Architecture Overview

## Data Flow
```
IDE Plugin ──HTTP/SSE──▶ Axum Gateway ──RingBuf──▶ CDP Engine ──Unix Socket──▶ Chrome
                              │                          ▲
                              ◄──── Shared Mem ──────────┘
                              │
                        SolidJS Dashboard (WebGL Telemetry)
```

## Components
1. **Gateway** (`gateway/`) - Axum HTTP server, OpenAI-compatible API
2. **CDP Engine** (`cdp-engine/`) - Chrome DevTools Protocol automation
3. **Ring Buffer** (`ringbuf/`) - Lock-free IPC via shared memory
4. **Telemetry** (`telemetry/`) - SolidJS + WebGL dashboard

## Key Invariants
- CDP connection: Unix socket only (no WebSocket)
- Streaming: zero-copy from CDP → ring buffer → SSE → client
- Latency target: <30ms overhead vs native API
- Backpressure: 429 when ring buffer >80% full

## See Also
- [[ring-buffer-ipc]] - IPC mechanism details
- [[cdp-engine]] - CDP automation details
