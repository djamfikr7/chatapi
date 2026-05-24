# ADR-001: Rust + Axum for API Gateway

## Status: Accepted

## Context
Need sub-millisecond request handling with zero GC pauses for latency-sensitive streaming.

## Decision
Use Rust with Axum framework for the API gateway.

## Consequences
- Positive: Zero-cost async, no GC, excellent performance
- Positive: Strong type system catches errors at compile time
- Positive: Tower middleware ecosystem for auth, logging, etc.
- Negative: Steeper learning curve than Node.js/Python
- Negative: Longer compile times during development

## Alternatives Considered
- Go (Gin): GC pauses could cause latency spikes
- Node.js (Fastify): Single-threaded, GC overhead
- Python (FastAPI): Too slow for <30ms target
