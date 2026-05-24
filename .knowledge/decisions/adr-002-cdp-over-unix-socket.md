# ADR-002: CDP over Unix Domain Socket

## Status: Accepted

## Context
CDP connection to Chrome can use WebSocket (HTTP upgrade) or Unix domain socket.

## Decision
Use Unix domain socket exclusively for CDP communication.

## Consequences
- Positive: No TCP loopback overhead (~0.5ms savings)
- Positive: No WebSocket upgrade handshake latency
- Positive: More secure (filesystem permissions)
- Negative: Requires Chrome to be on same machine
- Negative: Can't connect to remote browser instances

## Alternatives Considered
- WebSocket over TCP: Higher latency, unnecessary network stack traversal
- Named pipes: Windows-specific, less portable
