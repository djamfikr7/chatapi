# ADR-003: DeepSeek Chat as Target

## Status: Accepted

## Context
Need a free LLM chat interface to automate via CDP. Must be accessible without payment.

## Decision
Use DeepSeek Chat (chat.deepseek.com) as the primary target.

## Rationale
- Free tier available with good quality models
- Standard WebSocket-based streaming (detectable via CDP)
- Stable UI with accessible DOM structure
- Good model quality (DeepSeek-V3/R1)

## CDP Behavior Notes
- Chat uses WebSocket for streaming responses
- Input is a standard textarea element
- Responses stream as JSON over WebSocket

## See Also
- [[cdp-engine-module]] - CDP automation implementation
- [[deepseek-cdp-notes]] - Detailed CDP behavior notes
