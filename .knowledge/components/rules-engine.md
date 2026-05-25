# Rules Engine Design

## Configuration File
`.chatapi/config.toml` in project root.

## Config Structure
```toml
[target]
mode = "browser"  # browser | api | mcp
model = "deepseek-chat"

[target.api]
endpoint = "https://api.deepseek.com/v1"
api_key_env = "DEEPSEEK_API_KEY"

[target.mcp]
servers = [
  { name = "filesystem", command = "mcp-server-fs" },
  { name = "github", command = "mcp-server-github", env = { GITHUB_TOKEN = "..." } }
]

[rules]
system_prompt = "You are a Rust expert."
working_dir = "."
allowed_tools = ["read_file", "edit_file", "run_command", "git_*"]
blocked_paths = [".env", ".git/config", "secrets/"]

[rules.context]
include_files = ["src/**/*.rs", "Cargo.toml"]
max_context_tokens = 50000

[sessions]
store = "memory"  # memory | sqlite
path = ".chatapi/sessions.db"

[dashboard]
port = 8091
theme = "dark"
```

## System Prompt Construction
The rules engine constructs the system prompt from:
1. User-defined system_prompt from config
2. Active tool definitions (JSON schema)
3. Context files (included in first message)
4. Working directory info

## See Also
- [[full-platform-architecture]] - Platform architecture
