# Ring Buffer IPC Design

## Problem
CDP event handler and API gateway need zero-copy, low-latency communication.

## Solution
Shared memory ring buffer using `rtrb` crate (real-time safe, lock-free).

## Design
- Producer: CDP engine writes raw UTF-8 bytes from WebSocket frames
- Consumer: Gateway reads bytes, encodes as SSE, writes to client socket
- Capacity: 256KB default, configurable
- Backpressure: signal HTTP 429 when >80% full

## Flow
```
CDP Event → Parse frame → Write to ring buffer → Gateway reads → SSE encode → writev to socket
```

## Key Properties
- No allocations in hot path
- No mutexes (SPMC via atomic operations)
- Cache-line aligned for false sharing prevention

## See Also
- [[architecture-overview]] - System architecture
- [[sse-streaming]] - SSE encoding details
