# ADR-006: Dual Target Mode (Browser + API)

## Status: Accepted

## Context
The initial design only supports browser automation via CDP. IDE agents need native tool_calls support, which requires direct API access. MCP is also becoming standard for tool servers.

## Decision
Support three target modes behind a unified TargetProvider trait:
1. **Browser (CDP)**: Automate free LLM chats, parse tool calls from text
2. **API**: Direct OpenAI-compatible API with native tool_calls
3. **MCP**: Model Context Protocol for tool servers

## Configuration
```toml
[target]
mode = "browser"  # browser | api | mcp

[target.api]
endpoint = "https://api.deepseek.com/v1"
api_key_env = "DEEPSEEK_API_KEY"

[target.mcp]
servers = [
  { name = "filesystem", command = "mcp-server-fs" }
]
```

## Consequences
- Positive: Works with free LLMs (browser) and paid APIs
- Positive: MCP support for extensible tool servers
- Positive: Config-driven, easy to switch
- Negative: Three code paths to maintain
- Negative: Browser mode has inherent limitations (tool call parsing)

## See Also
- [[full-platform-architecture]] - Platform architecture
