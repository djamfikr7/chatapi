# Tool System Design

## Architecture
Plugin-based tool registry with ToolProvider trait.

## ToolProvider Trait
```rust
#[async_trait]
pub trait ToolProvider: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters_schema(&self) -> serde_json::Value;
    async fn execute(&self, args: serde_json::Value, ctx: &ToolContext) -> Result<ToolResult>;
}
```

## Built-in Tools

### File Operations
- `read_file` { path } -> contents
- `write_file` { path, content } -> success
- `edit_file` { path, old_text, new_text } -> diff
- `list_dir` { path } -> file tree
- `apply_patch` { diff } -> success/conflicts

### Terminal & Diagnostics
- `run_command` { command, cwd?, timeout? } -> stdout/stderr
- `get_diagnostics` { path? } -> errors/warnings

### Git Operations
- `git_status` {} -> changed files
- `git_diff` { path?, staged? } -> unified diff
- `git_commit` { message, files? } -> commit hash
- `git_log` { limit? } -> recent commits
- `git_show` { ref } -> commit details

### Search
- `grep_code` { pattern, path?, glob? } -> matches
- `search_symbols` { query, kind? } -> symbol definitions

## Tool Safety
- Path validation against blocked_paths from rules config
- Command sandboxing with optional allowlist
- Timeout enforcement on all tools
- Output size limits (configurable, default 100KB)

## Tool Execution Flow
```
IDE sends tool_calls in request
  → Gateway validates tool_call_id references
  → Gateway checks allowed_tools from rules
  → Gateway checks blocked_paths for file tools
  → ToolRegistry.execute(tool_name, args, context)
    → Built-in tool? Execute directly
    → MCP tool? Forward to MCP server via JSON-RPC
    → Unknown? Return error
  → ToolResult returned
  → Gateway formats as tool role message
  → Sent back to LLM for continuation
```

## See Also
- [[full-platform-architecture]] - Platform architecture
- [[mcp-integration]] - MCP tool integration
