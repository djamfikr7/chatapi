# DeepSeek Chat CDP Notes

## Connection
- URL: https://chat.deepseek.com/
- Uses WebSocket for streaming responses
- WebSocket URL pattern: wss://chat.deepseek.com/...

## DOM Structure
- Input textarea: standard `<textarea>` element
- Send button: `<button>` or Enter key
- Message container: div-based message list

## WebSocket Frames
- Request: JSON with messages array
- Response: Streaming JSON chunks
- Each chunk contains partial content

## CDP Events to Monitor
- `Network.webSocketFrameSent` - Detect when user sends message
- `Network.webSocketFrameReceived` - Capture streaming response
- `Page.loadEventFired` - Refresh accessibility tree on navigation

## Timing
- Typical first token: 200-500ms after submit
- Streaming rate: ~50-100 tokens/sec
- Completion: silence timeout heuristic (500ms no new frames)

## Anti-Bot Considerations
- Rate limiting on rapid requests
- CAPTCHA triggers on suspicious patterns
- Solution: humanized timing, persistent browser profile
