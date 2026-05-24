# ChatAPI - Second Brain Index

## Project Overview
Low-latency middleware: OpenAI-compatible API → CDP automation → Free LLM chat (DeepSeek).

## Architecture
- [[architecture-overview]] - System architecture and data flow
- [[ring-buffer-ipc]] - Shared memory ring buffer design
- [[cdp-engine]] - Chrome DevTools Protocol engine

## Decisions
- [[adr-001-rust-axum]] - Rust + Axum for API gateway
- [[adr-002-cdp-over-unix-socket]] - CDP over Unix domain socket
- [[adr-003-deepseek-target]] - DeepSeek Chat as free LLM target
- [[adr-004-shared-memory-ringbuf]] - Shared memory ring buffer IPC

## Components
- [[gateway-module]] - API gateway (POST /v1/chat/completions)
- [[cdp-engine-module]] - Browser automation engine
- [[ringbuf-module]] - Lock-free ring buffer
- [[telemetry-module]] - SolidJS WebGL dashboard
- [[sse-streaming]] - Server-Sent Events streaming pipeline

## Testing
- [[test-strategy]] - E2E and integration testing strategy
- [[benchmark-suite]] - Latency benchmark suite

## References
- [[deepseek-cdp-notes]] - DeepSeek Chat CDP behavior notes
- [[openai-api-spec]] - OpenAI API compatibility notes

## Status
- Phase: Implementation
- Target: DeepSeek Chat (chat.deepseek.com)
