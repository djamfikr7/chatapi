use async_trait::async_trait;
use chatapi_shared::traits::{ToolProvider, ToolContext, ToolResult, ToolError};
use serde::Deserialize;
use serde_json::{json, Value};

pub struct GrepCode;

#[derive(Deserialize)]
struct GrepCodeArgs {
    pattern: String,
    path: Option<String>,
    glob: Option<String>,
}

#[async_trait]
impl ToolProvider for GrepCode {
    fn name(&self) -> &str { "grep_code" }
    fn description(&self) -> &str { "Search file contents using regex" }
    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "pattern": {"type": "string"},
                "path": {"type": "string"},
                "glob": {"type": "string"}
            },
            "required": ["pattern"]
        })
    }
    async fn execute(&self, args: Value, ctx: &ToolContext) -> Result<ToolResult, ToolError> {
        let a: GrepCodeArgs = serde_json::from_value(args)
            .map_err(|e| ToolError::InvalidArgs(e.to_string()))?;
        let search_dir = a.path.as_deref().map(|p| ctx.working_dir.join(p)).unwrap_or_else(|| ctx.working_dir.clone());
        let glob_pattern = a.glob.as_deref().unwrap_or("**/*");
        let full_glob = if search_dir.to_string_lossy().contains('*') {
            search_dir.to_string_lossy().to_string()
        } else {
            format!("{}/{glob_pattern}", search_dir.display())
        };
        let re = regex::Regex::new(&a.pattern)
            .map_err(|e| ToolError::InvalidArgs(format!("invalid regex: {e}")))?;

        let mut matches = Vec::new();
        let paths: Vec<_> = glob::glob(&full_glob)
            .map_err(|e| ToolError::InvalidArgs(format!("invalid glob: {e}")))?
            .filter_map(|p| p.ok())
            .filter(|p| p.is_file())
            .collect();

        for path in paths {
            let Ok(content) = tokio::fs::read_to_string(&path).await else { continue };
            for (line_num, line) in content.lines().enumerate() {
                if re.is_match(line) {
                    let rel = path.strip_prefix(&ctx.working_dir).unwrap_or(&path);
                    matches.push(format!("{}:{}: {}", rel.display(), line_num + 1, line.trim()));
                }
            }
            if matches.len() > 500 { break; }
        }

        Ok(ToolResult::Text(if matches.is_empty() {
            "No matches found".to_string()
        } else {
            matches.join("\n")
        }))
    }
}
