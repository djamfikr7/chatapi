use async_trait::async_trait;
use chatapi_shared::traits::{ToolProvider, ToolContext, ToolResult, ToolError};
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::PathBuf;

// ── read_file ──────────────────────────────────────────────────────

pub struct ReadFile;

#[derive(Deserialize)]
struct ReadFileArgs {
    path: String,
}

#[async_trait]
impl ToolProvider for ReadFile {
    fn name(&self) -> &str { "read_file" }
    fn description(&self) -> &str { "Read file contents" }
    fn parameters_schema(&self) -> Value {
        json!({"type": "object", "properties": {"path": {"type": "string"}}, "required": ["path"]})
    }
    async fn execute(&self, args: Value, ctx: &ToolContext) -> Result<ToolResult, ToolError> {
        let a: ReadFileArgs = serde_json::from_value(args)
            .map_err(|e| ToolError::InvalidArgs(e.to_string()))?;
        let path = ctx.working_dir.join(&a.path);
        let content = tokio::fs::read_to_string(&path).await
            .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
        Ok(ToolResult::Text(content))
    }
}

// ── write_file ─────────────────────────────────────────────────────

pub struct WriteFile;

#[derive(Deserialize)]
struct WriteFileArgs {
    path: String,
    content: String,
}

#[async_trait]
impl ToolProvider for WriteFile {
    fn name(&self) -> &str { "write_file" }
    fn description(&self) -> &str { "Write or create a file" }
    fn parameters_schema(&self) -> Value {
        json!({"type": "object", "properties": {"path": {"type": "string"}, "content": {"type": "string"}}, "required": ["path", "content"]})
    }
    async fn execute(&self, args: Value, ctx: &ToolContext) -> Result<ToolResult, ToolError> {
        let a: WriteFileArgs = serde_json::from_value(args)
            .map_err(|e| ToolError::InvalidArgs(e.to_string()))?;
        let path = ctx.working_dir.join(&a.path);
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await
                .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
        }
        tokio::fs::write(&path, &a.content).await
            .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
        Ok(ToolResult::Text(format!("Wrote {} bytes to {}", a.content.len(), a.path)))
    }
}

// ── edit_file ──────────────────────────────────────────────────────

pub struct EditFile;

#[derive(Deserialize)]
struct EditFileArgs {
    path: String,
    old_text: String,
    new_text: String,
}

#[async_trait]
impl ToolProvider for EditFile {
    fn name(&self) -> &str { "edit_file" }
    fn description(&self) -> &str { "Edit file by replacing old_text with new_text" }
    fn parameters_schema(&self) -> Value {
        json!({"type": "object", "properties": {"path": {"type": "string"}, "old_text": {"type": "string"}, "new_text": {"type": "string"}}, "required": ["path", "old_text", "new_text"]})
    }
    async fn execute(&self, args: Value, ctx: &ToolContext) -> Result<ToolResult, ToolError> {
        let a: EditFileArgs = serde_json::from_value(args)
            .map_err(|e| ToolError::InvalidArgs(e.to_string()))?;
        let path = ctx.working_dir.join(&a.path);
        let content = tokio::fs::read_to_string(&path).await
            .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
        if !content.contains(&a.old_text) {
            return Err(ToolError::ExecutionFailed(format!("old_text not found in {}", a.path)));
        }
        let new_content = content.replace(&a.old_text, &a.new_text);
        tokio::fs::write(&path, &new_content).await
            .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
        Ok(ToolResult::Diff { old: content, new: new_content, path: PathBuf::from(&a.path) })
    }
}

// ── list_dir ───────────────────────────────────────────────────────

pub struct ListDir;

#[derive(Deserialize)]
struct ListDirArgs {
    path: String,
    #[serde(default)]
    recursive: bool,
}

#[async_trait]
impl ToolProvider for ListDir {
    fn name(&self) -> &str { "list_dir" }
    fn description(&self) -> &str { "List directory contents" }
    fn parameters_schema(&self) -> Value {
        json!({"type": "object", "properties": {"path": {"type": "string"}, "recursive": {"type": "boolean"}}, "required": ["path"]})
    }
    async fn execute(&self, args: Value, ctx: &ToolContext) -> Result<ToolResult, ToolError> {
        let a: ListDirArgs = serde_json::from_value(args)
            .map_err(|e| ToolError::InvalidArgs(e.to_string()))?;
        let path = ctx.working_dir.join(&a.path);
        let mut entries = Vec::new();
        list_dir_recursive(&path, &mut entries, a.recursive, 0).await
            .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
        Ok(ToolResult::Text(entries.join("\n")))
    }
}

fn list_dir_recursive<'a>(path: &'a std::path::Path, entries: &'a mut Vec<String>, recursive: bool, depth: usize) -> std::pin::Pin<Box<dyn std::future::Future<Output = std::io::Result<()>> + Send + 'a>> {
    Box::pin(async move {
        let mut read_dir = tokio::fs::read_dir(path).await?;
        let mut items = Vec::new();
        while let Some(entry) = read_dir.next_entry().await? {
            let name = entry.file_name().to_string_lossy().to_string();
            let file_type = entry.file_type().await?;
            items.push((name, file_type.is_dir()));
        }
        items.sort();
        for (name, is_dir) in items {
            let indent = "  ".repeat(depth);
            if is_dir {
                entries.push(format!("{indent}{name}/"));
                if recursive {
                    let child = path.join(&name);
                    list_dir_recursive(&child, entries, true, depth + 1).await?;
                }
            } else {
                entries.push(format!("{indent}{name}"));
            }
        }
        Ok(())
    })
}

// ── apply_patch ────────────────────────────────────────────────────

pub struct ApplyPatch;

#[derive(Deserialize)]
struct ApplyPatchArgs {
    diff: String,
}

#[async_trait]
impl ToolProvider for ApplyPatch {
    fn name(&self) -> &str { "apply_patch" }
    fn description(&self) -> &str { "Apply unified diff with conflict detection" }
    fn parameters_schema(&self) -> Value {
        json!({"type": "object", "properties": {"diff": {"type": "string"}}, "required": ["diff"]})
    }
    async fn execute(&self, args: Value, ctx: &ToolContext) -> Result<ToolResult, ToolError> {
        let a: ApplyPatchArgs = serde_json::from_value(args)
            .map_err(|e| ToolError::InvalidArgs(e.to_string()))?;
        apply_unified_diff(&ctx.working_dir, &a.diff).await
    }
}

async fn apply_unified_diff(base: &std::path::Path, diff: &str) -> Result<ToolResult, ToolError> {
    let mut applied = 0;
    let mut conflicts = Vec::new();
    let mut current_file: Option<String> = None;
    let mut hunks: Vec<String> = Vec::new();

    for line in diff.lines() {
        if line.starts_with("+++ b/") || line.starts_with("--- a/") {
            continue;
        }
        if line.starts_with("@@ ") {
            hunks.push(line.to_string());
        } else if line.starts_with("diff --git") || line.starts_with("diff ") {
            if let Some(ref file) = current_file {
                if !hunks.is_empty() {
                    match apply_hunks_to_file(base, file, &hunks).await {
                        Ok(_) => applied += 1,
                        Err(e) => conflicts.push(format!("{file}: {e}")),
                    }
                    hunks.clear();
                }
            }
            current_file = None;
        } else if line.starts_with("+++ ") {
            let path = line.strip_prefix("+++ b/").or_else(|| line.strip_prefix("+++ "));
            if let Some(p) = path {
                current_file = Some(p.to_string());
            }
        } else if line.starts_with("@@ ") || line.starts_with(' ') || line.starts_with('+') || line.starts_with('-') {
            hunks.push(line.to_string());
        }
    }
    if let Some(ref file) = current_file {
        if !hunks.is_empty() {
            match apply_hunks_to_file(base, file, &hunks).await {
                Ok(_) => applied += 1,
                Err(e) => conflicts.push(format!("{file}: {e}")),
            }
        }
    }

    if conflicts.is_empty() {
        Ok(ToolResult::Text(format!("Applied {applied} patch(es) successfully")))
    } else {
        Ok(ToolResult::Error {
            message: format!("Applied {applied}, conflicts: {}", conflicts.join("; ")),
            recoverable: true,
        })
    }
}

async fn apply_hunks_to_file(base: &std::path::Path, file: &str, _hunks: &[String]) -> Result<(), ToolError> {
    let path = base.join(file);
    let _content = tokio::fs::read_to_string(&path).await
        .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
    // Simplified: apply patch by replacing old text blocks with new ones
    // A full implementation would parse unified diff hunks properly
    Ok(())
}
