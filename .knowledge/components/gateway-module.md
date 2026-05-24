# Gateway Module

## Endpoint
`POST /v1/chat/completions`

## OpenAI API Compatibility
- Request format: messages array, model, stream, temperature, etc.
- Response format: SSE with `data: {JSON}` chunks
- Final chunk: `data: [DONE]`

## Streaming Flow
1. Receive request → validate
2. Forward to CDP engine via ring buffer command channel
3. Stream response chunks via SSE
4. Handle backpressure (429 when ring buffer full)

## Error Responses
- 400: Invalid request format
- 429: Rate limited / backpressure
- 502: CDP automation failure
- 503: Browser not connected

## See Also
- [[sse-streaming]] - SSE encoding
- [[architecture-overview]] - System architecture
