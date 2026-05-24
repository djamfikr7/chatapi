# SSE Streaming Pipeline

## Format
```
data: {"id":"chatcmpl-xxx","object":"chat.completion.chunk","choices":[{"delta":{"content":"token"}}]}

data: [DONE]
```

## Zero-Copy Path
1. CDP frame → raw bytes written to ring buffer (no copy)
2. Gateway reads from ring buffer → wraps in SSE envelope
3. Vectored I/O (`writev`) sends header + body in single syscall

## Adaptive Chunk Coalescing
- Fragments <64B: merge with next chunk
- Fragments >256B: flush immediately
- Default: flush every 10ms

## See Also
- [[ring-buffer-ipc]] - Buffer design
- [[gateway-module]] - API endpoint
