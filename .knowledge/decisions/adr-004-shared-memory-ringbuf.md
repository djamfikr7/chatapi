# ADR-004: Shared Memory Ring Buffer IPC

## Status: Accepted

## Context
CDP engine and gateway need to communicate with minimal latency. Traditional channels involve copies.

## Decision
Use `rtrb` crate for lock-free single-producer single-consumer ring buffer in shared memory.

## Consequences
- Positive: Zero-copy data transfer
- Positive: No mutex contention
- Positive: Cache-line aligned to prevent false sharing
- Negative: Fixed capacity requires backpressure handling
- Negative: Only works for single-producer (fine for our use case)

## Alternatives Considered
- Crossbeam channel: Involves allocation per message
- Tokio mpsc: Async overhead, allocation per message
- Unix pipe: Kernel overhead for each byte
