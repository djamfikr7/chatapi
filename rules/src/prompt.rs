use crate::config::ChatApiConfig;

/// Build the system prompt from config + tool schemas + context files.
pub fn build_system_prompt(config: &ChatApiConfig, tool_schemas: &[serde_json::Value]) -> String {
    let mut parts = Vec::new();

    // 1. User-defined system prompt
    if let Some(ref prompt) = config.rules.system_prompt {
        if !prompt.is_empty() {
            parts.push(prompt.clone());
        }
    }

    // 2. Tool definitions
    if !tool_schemas.is_empty() {
        let tool_defs: Vec<serde_json::Value> = tool_schemas
            .iter()
            .map(|schema| {
                serde_json::json!({
                    "name": schema.get("name").and_then(|v| v.as_str()).unwrap_or(""),
                    "description": schema.get("description").and_then(|v| v.as_str()).unwrap_or(""),
                    "parameters": schema.get("parameters"),
                })
            })
            .collect();

        parts.push(format!(
            "You have access to the following tools. To call a tool, output a JSON code block:\n\n```json\n{{\"name\": \"function_name\", \"arguments\": {{\"param\": \"value\"}}}}\n```\n\nFor multiple calls, use an array.\n\nAvailable tools:\n{}",
            serde_json::to_string_pretty(&tool_defs).unwrap_or_default()
        ));
    }

    // 3. Context files
    if let Some(ref ctx) = config.rules.context {
        if !ctx.include_files.is_empty() {
            let mut file_contents = Vec::new();
            let mut total_chars = 0usize;
            let max_chars = ctx.max_context_tokens.unwrap_or(0) as usize * 4; // ~4 chars per token

            for pattern in &ctx.include_files {
                // Support glob patterns
                if let Ok(paths) = glob::glob(pattern) {
                    for entry in paths.flatten() {
                        if entry.is_file() {
                            match std::fs::read_to_string(&entry) {
                                Ok(content) => {
                                    let file_chars = entry.display().to_string().len() + content.len() + 20; // overhead for formatting
                                    if max_chars > 0 && total_chars + file_chars > max_chars {
                                        tracing::warn!(
                                            file = %entry.display(),
                                            "Skipping context file — would exceed max_context_tokens"
                                        );
                                        break;
                                    }
                                    total_chars += file_chars;
                                    file_contents.push(format!(
                                        "### {}\n```\n{}\n```",
                                        entry.display(),
                                        content
                                    ));
                                }
                                Err(e) => {
                                    tracing::warn!(file = %entry.display(), error = %e, "Failed to read context file");
                                }
                            }
                        }
                    }
                }
            }

            if !file_contents.is_empty() {
                parts.push(format!(
                    "Workspace context:\n\n{}",
                    file_contents.join("\n\n")
                ));
            }
        }
    }

    // 4. Working directory info
    let wd = config.working_dir();
    parts.push(format!("Working directory: {}", wd.display()));

    parts.join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ChatApiConfig, ContextSection};

    #[test]
    fn test_basic_system_prompt() {
        let mut config = ChatApiConfig::default();
        config.rules.system_prompt = Some("You are helpful.".to_string());

        let prompt = build_system_prompt(&config, &[]);
        assert!(prompt.contains("You are helpful."));
        assert!(prompt.contains("Working directory:"));
    }

    #[test]
    fn test_no_system_prompt() {
        let config = ChatApiConfig::default();
        let prompt = build_system_prompt(&config, &[]);
        assert!(prompt.contains("Working directory:"));
        assert!(!prompt.is_empty());
    }

    #[test]
    fn test_with_tools() {
        let config = ChatApiConfig::default();
        let tools = vec![serde_json::json!({
            "name": "read_file",
            "description": "Read a file",
            "parameters": {"type": "object", "properties": {"path": {"type": "string"}}}
        })];

        let prompt = build_system_prompt(&config, &tools);
        assert!(prompt.contains("read_file"));
        assert!(prompt.contains("JSON code block"));
    }

    #[test]
    fn test_with_context_files() {
        let mut config = ChatApiConfig::default();
        // Write a temp file
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        std::fs::write(&file_path, "fn main() {}").unwrap();

        config.rules.context = Some(ContextSection {
            include_files: vec![file_path.to_str().unwrap().to_string()],
            max_context_tokens: None,
        });

        let prompt = build_system_prompt(&config, &[]);
        assert!(prompt.contains("fn main()"));
        assert!(prompt.contains("Workspace context:"));
    }

    #[test]
    fn test_context_files_respect_token_limit() {
        let mut config = ChatApiConfig::default();
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("big.txt");
        std::fs::write(&file_path, "x".repeat(1000)).unwrap();

        config.rules.context = Some(ContextSection {
            include_files: vec![file_path.to_str().unwrap().to_string()],
            max_context_tokens: Some(10), // 10 tokens = ~40 chars — too small for 1000 chars
        });

        let prompt = build_system_prompt(&config, &[]);
        assert!(!prompt.contains("xxxxx")); // file should be skipped
    }
}
