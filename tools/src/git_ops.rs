use async_trait::async_trait;
use chatapi_shared::traits::{ToolProvider, ToolContext, ToolResult, ToolError};
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::process::Command;

// ── git_status ─────────────────────────────────────────────────────

pub struct GitStatus;

#[async_trait]
impl ToolProvider for GitStatus {
    fn name(&self) -> &str { "git_status" }
    fn description(&self) -> &str { "Show working tree status" }
    fn parameters_schema(&self) -> Value {
        json!({"type": "object", "properties": {}})
    }
    async fn execute(&self, _args: Value, ctx: &ToolContext) -> Result<ToolResult, ToolError> {
        let output = Command::new("git").arg("status").arg("--porcelain").current_dir(&ctx.working_dir).output().await
            .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
        Ok(ToolResult::Text(String::from_utf8_lossy(&output.stdout).to_string()))
    }
}

// ── git_diff ───────────────────────────────────────────────────────

pub struct GitDiff;

#[derive(Deserialize)]
struct GitDiffArgs {
    path: Option<String>,
    #[serde(default)]
    staged: bool,
}

#[async_trait]
impl ToolProvider for GitDiff {
    fn name(&self) -> &str { "git_diff" }
    fn description(&self) -> &str { "Show unified diff" }
    fn parameters_schema(&self) -> Value {
        json!({"type": "object", "properties": {"path": {"type": "string"}, "staged": {"type": "boolean"}}})
    }
    async fn execute(&self, args: Value, ctx: &ToolContext) -> Result<ToolResult, ToolError> {
        let a: GitDiffArgs = serde_json::from_value(args).map_err(|e| ToolError::InvalidArgs(e.to_string()))?;
        let mut cmd = Command::new("git");
        cmd.arg("diff");
        if a.staged { cmd.arg("--staged"); }
        if let Some(ref p) = a.path { cmd.arg(p); }
        cmd.current_dir(&ctx.working_dir);
        let output = cmd.output().await.map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
        Ok(ToolResult::Text(String::from_utf8_lossy(&output.stdout).to_string()))
    }
}

// ── git_commit ─────────────────────────────────────────────────────

pub struct GitCommit;

#[derive(Deserialize)]
struct GitCommitArgs {
    message: String,
    files: Option<Vec<String>>,
}

#[async_trait]
impl ToolProvider for GitCommit {
    fn name(&self) -> &str { "git_commit" }
    fn description(&self) -> &str { "Create a commit" }
    fn parameters_schema(&self) -> Value {
        json!({"type": "object", "properties": {"message": {"type": "string"}, "files": {"type": "array", "items": {"type": "string"}}}, "required": ["message"]})
    }
    async fn execute(&self, args: Value, ctx: &ToolContext) -> Result<ToolResult, ToolError> {
        let a: GitCommitArgs = serde_json::from_value(args).map_err(|e| ToolError::InvalidArgs(e.to_string()))?;
        if let Some(ref files) = a.files {
            let mut cmd = Command::new("git");
            cmd.arg("add");
            for f in files { cmd.arg(f); }
            cmd.current_dir(&ctx.working_dir);
            cmd.output().await.map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
        } else {
            Command::new("git").arg("add").arg("-A").current_dir(&ctx.working_dir).output().await
                .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
        }
        let output = Command::new("git").arg("commit").arg("-m").arg(&a.message).current_dir(&ctx.working_dir).output().await
            .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
        Ok(ToolResult::Text(String::from_utf8_lossy(&output.stdout).to_string()))
    }
}

// ── git_log ────────────────────────────────────────────────────────

pub struct GitLog;

#[derive(Deserialize)]
struct GitLogArgs {
    #[serde(default = "default_limit")]
    limit: u32,
}
fn default_limit() -> u32 { 10 }

#[async_trait]
impl ToolProvider for GitLog {
    fn name(&self) -> &str { "git_log" }
    fn description(&self) -> &str { "Recent commit history" }
    fn parameters_schema(&self) -> Value {
        json!({"type": "object", "properties": {"limit": {"type": "integer"}}})
    }
    async fn execute(&self, args: Value, ctx: &ToolContext) -> Result<ToolResult, ToolError> {
        let a: GitLogArgs = serde_json::from_value(args).map_err(|e| ToolError::InvalidArgs(e.to_string()))?;
        let output = Command::new("git").arg("log").arg("--oneline").arg("-n").arg(a.limit.to_string()).current_dir(&ctx.working_dir).output().await
            .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
        Ok(ToolResult::Text(String::from_utf8_lossy(&output.stdout).to_string()))
    }
}

// ── git_show ───────────────────────────────────────────────────────

pub struct GitShow;

#[derive(Deserialize)]
struct GitShowArgs {
    r#ref: String,
}

#[async_trait]
impl ToolProvider for GitShow {
    fn name(&self) -> &str { "git_show" }
    fn description(&self) -> &str { "Show commit details" }
    fn parameters_schema(&self) -> Value {
        json!({"type": "object", "properties": {"ref": {"type": "string"}}, "required": ["ref"]})
    }
    async fn execute(&self, args: Value, ctx: &ToolContext) -> Result<ToolResult, ToolError> {
        let a: GitShowArgs = serde_json::from_value(args).map_err(|e| ToolError::InvalidArgs(e.to_string()))?;
        let output = Command::new("git").arg("show").arg(&a.r#ref).current_dir(&ctx.working_dir).output().await
            .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
        Ok(ToolResult::Text(String::from_utf8_lossy(&output.stdout).to_string()))
    }
}
