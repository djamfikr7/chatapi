# Test Strategy

## Unit Tests
- Ring buffer: producer/consumer correctness, backpressure signaling
- SSE encoder: format compliance, chunk coalescing
- Request parser: OpenAI API validation

## Integration Tests
- Gateway → CDP engine end-to-end flow
- CDP connection lifecycle (connect, reconnect, disconnect)
- Ring buffer under load (burst traffic, backpressure)

## E2E Tests
- Full flow: HTTP request → CDP automation → SSE response
- Concurrent sessions (5+ simultaneous)
- Error recovery scenarios

## Benchmarks
- First-token latency vs native API baseline
- Streaming jitter (p50/p95/p99)
- Memory footprint under load
- CPU idle consumption

## Test Harness
- Mock CDP server for deterministic testing
- Chrome instance with test profile for real E2E
- Latency measurement via instrumented timestamps

## See Also
- [[benchmark-suite]] - Performance benchmarks
