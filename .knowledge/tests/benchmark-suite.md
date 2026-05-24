# Benchmark Suite

## Metrics to Measure
1. **First-token latency**: Time from HTTP request to first SSE chunk
2. **Streaming jitter**: Chunk interval histogram (p50/p95/p99)
3. **Memory footprint**: RSS under load via /proc/self/status
4. **CPU idle**: perf stat during idle state
5. **Startup time**: Wall-clock from exec to first 200 OK
6. **Concurrent sessions**: 5+ simultaneous connections

## Baseline Comparison
- Native DeepSeek API: ~200-500ms first token
- Our overhead target: <30ms additional
- Total acceptable: <530ms first token

## Test Scenarios
1. Single request, cold start
2. Single request, warm connection
3. 10 sequential requests
4. 5 concurrent requests
5. Burst of 20 requests (backpressure test)
6. Long conversation (100+ messages)

## Measurement Method
- Instrumented timestamps at each pipeline stage
- Histogram recording for latency distribution
- Continuous monitoring during load tests

## See Also
- [[test-strategy]] - Overall test strategy
