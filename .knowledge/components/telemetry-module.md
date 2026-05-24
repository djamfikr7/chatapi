# Telemetry Module

## Features
- Dark theme with gradient accents
- Real-time latency waterfall: CDP recv → parse → buffer → encode → flush
- P50/P95/P99 latency gauges at 60fps (WebGL)
- Session state indicator with animated transitions
- Manual intervention panel for prompt override

## Stack
- SolidJS for reactivity (minimal overhead)
- WebGL canvas for latency visualization
- WebSocket for real-time data from gateway

## Latency Waterfall Display
```
CDP Recv    ██░░░░░░░░  2ms
Parse       ████░░░░░░  4ms  
Buffer      █░░░░░░░░░  1ms
SSE Encode  ███░░░░░░░  3ms
Flush       ██░░░░░░░░  2ms
            ──────────
Total       12ms
```

## See Also
- [[architecture-overview]] - System architecture
