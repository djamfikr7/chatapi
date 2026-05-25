# MCP Integration Design

## Overview
Model Context Protocol (MCP) is the standard for tool servers. ChatAPI acts as an MCP client.

## MCP Client Architecture
```rust
pub struct McpClient {
    servers: Vec<McpServer>,
    tool_cache: HashMap<String, McpTool>,
}

pub struct McpServer {
    name: String,
    command: String,
    args: Vec<String>,
    env: HashMap<String, String>,
    process: Child,
    capabilities: McpCapabilities,
}
```

## MCP Capabilities
- Tool discovery via `tools/list`
- Resource access via `resources/read`
- Prompt templates via `prompts/get`

## Tool Registration
MCP tools are auto-discovered and registered in the tool registry. They implement ToolProvider trait.

## Configuration
```toml
[target.mcp]
servers = [
  { name = "filesystem", command = "mcp-server-fs" },
  { name = "github", command = "mcp-server-github", env = { GITHUB_TOKEN = "..." } }
]
```

## See Also
- [[tool-system]] - Tool registry
- [[full-platform-architecture]] - Platform architecture
