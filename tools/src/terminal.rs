use async_trait::async_trait;
use chatapi_shared::traits::{ToolProvider, ToolContext, ToolResult, ToolError};
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::process::Command;

// ── run_command ────────────────────────────────────────────────────

pub struct RunCommand;

#[derive(Deserialize)]
struct RunCommandArgs {
    command: String,
    cwd: Option<String>,
    #[serde(default = "default_timeout")]
    timeout_ms: u64,
}

fn default_timeout() -> u64 { 30_000 }

#[async_trait]
impl ToolProvider for RunCommand {
    fn name(&self) -> &str { "run_command" }
    fn description(&self) -> &str { "Execute a shell command" }
    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "command": {"type": "string"},
                "cwd": {"type": "string"},
                "timeout_ms": {"type": "integer"}
            },
            "required": ["command"]
        })
    }
    async fn execute(&self, args: Value, ctx: &ToolContext) -> Result<ToolResult, ToolError> {
        let a: RunCommandArgs = serde_json::from_value(args)
            .map_err(|e| ToolError::InvalidArgs(e.to_string()))?;
        let cwd = a.cwd.as_deref().map(|c| ctx.working_dir.join(c)).unwrap_or_else(|| ctx.working_dir.clone());
        let output = tokio::time::timeout(
            std::time::Duration::from_millis(a.timeout_ms),
            Command::new("sh").arg("-c").arg(&a.command).current_dir(&cwd).output()
        ).await
            .map_err(|_| ToolError::Timeout(a.timeout_ms))?
            .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let mut result = String::new();
        if !stdout.is_empty() { result.push_str(&stdout); }
        if !stderr.is_empty() { if !result.is_empty() { result.push_str("\n--- stderr ---\n"); } result.push_str(&stderr); }
        if !output.status.success() {
            let code = output.status.code().unwrap_or(-1);
            result.push_str(&format!("\n[exit code: {code}]"));
        }
        Ok(ToolResult::Text(result))
    }
}

// ── get_diagnostics ────────────────────────────────────────────────

pub struct GetDiagnostics;

#[derive(Deserialize)]
struct GetDiagnosticsArgs {
    path: Option<String>,
}

#[async_trait]
impl ToolProvider for GetDiagnostics {
    fn name(&self) -> &str { "get_diagnostics" }
    fn description(&self) -> &str { "Get compiler errors and warnings" }
    fn parameters_schema(&self) -> Value {
        json!({"type": "object", "properties": {"path": {"type": "string"}}})
    }
    async fn execute(&self, args: Value, ctx: &ToolContext) -> Result<ToolResult, ToolError> {
        let _a: GetDiagnosticsArgs = serde_json::from_value(args)
            .map_err(|e| ToolError::InvalidArgs(e.to_string()))?;
        let output = tokio::time::timeout(
            std::time::Duration::from_secs(60),
            Command::new("cargo").arg("check").current_dir(&ctx.working_dir).output()
        ).await
            .map_err(|_| ToolError::Timeout(60_000))?
            .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
        let stderr = String::from_utf8_lossy(&output.stderr);
        let lines: Vec<&str> = stderr.lines().filter(|l| l.contains("error") || l.contains("warning")).collect();
        Ok(ToolResult::Text(if lines.is_empty() { "No errors or warnings".to_string() } else { lines.join("\n") }))
    }
}
