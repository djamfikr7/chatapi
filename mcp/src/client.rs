//! MCP client — spawns an MCP server process and communicates over stdio.

use std::collections::HashMap;
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{Mutex, oneshot};
use tracing::{debug, error, info, warn};

use crate::protocol::*;

/// An MCP server connection.
pub struct McpClient {
    name: String,
    child: Mutex<Child>,
    pending: Arc<Mutex<HashMap<u64, oneshot::Sender<JsonRpcResponse>>>>,
    next_id: AtomicU64,
}

impl McpClient {
    /// Spawn an MCP server process and connect via stdio.
    pub async fn spawn(
        name: &str,
        command: &str,
        args: &[String],
        env: &HashMap<String, String>,
    ) -> anyhow::Result<Self> {
        info!(name = %name, command = %command, "Spawning MCP server");

        let mut cmd = Command::new(command);
        cmd.args(args)
            .envs(env)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = cmd.spawn()
            .map_err(|e| anyhow::anyhow!("Failed to spawn MCP server '{}': {}", name, e))?;

        let stdin = child.stdin.take().expect("stdin piped");
        let stdout = child.stdout.take().expect("stdout piped");

        let pending: HashMap<u64, oneshot::Sender<JsonRpcResponse>> = HashMap::new();
        let pending = Arc::new(Mutex::new(pending));

        let client = Self {
            name: name.to_string(),
            child: Mutex::new(child),
            pending: pending.clone(),
            next_id: AtomicU64::new(1),
        };

        // Spawn stdout reader
        let pending_ref = pending;
        tokio::spawn(async move {
            Self::read_loop(stdout, pending_ref).await;
        });

        // Initialize
        let init_params = InitializeParams {
            protocol_version: "2024-11-05".to_string(),
            capabilities: ClientCapabilities {
                tools: Some(serde_json::json!({})),
            },
            client_info: ClientInfo {
                name: "chatapi".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
            },
        };

        let resp: InitializeResult = client.request("initialize", serde_json::to_value(init_params)?).await?;
        info!(
            name = %name,
            server = %resp.server_info.name,
            version = %resp.server_info.version,
            "MCP server initialized"
        );

        // Send initialized notification
        client.notify("notifications/initialized", None).await?;

        Ok(client)
    }

    async fn read_loop(
        stdout: tokio::process::ChildStdout,
        pending: std::sync::Arc<Mutex<HashMap<u64, oneshot::Sender<JsonRpcResponse>>>>,
    ) {
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        loop {
            line.clear();
            match reader.read_line(&mut line).await {
                Ok(0) => break, // EOF
                Ok(_) => {
                    let trimmed = line.trim();
                    if trimmed.is_empty() { continue; }
                    match serde_json::from_str::<JsonRpcResponse>(trimmed) {
                        Ok(resp) => {
                            let mut map = pending.lock().await;
                            if let Some(tx) = map.remove(&resp.id) {
                                let _ = tx.send(resp);
                            } else {
                                warn!(id = resp.id, "Response for unknown request ID");
                            }
                        }
                        Err(e) => {
                            debug!(error = %e, line = %trimmed, "Failed to parse MCP response");
                        }
                    }
                }
                Err(e) => {
                    error!(error = %e, "MCP stdout read error");
                    break;
                }
            }
        }
    }

    /// Send a JSON-RPC request and wait for the response.
    async fn request<T: serde::de::DeserializeOwned>(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> anyhow::Result<T> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let req = JsonRpcRequest::new(id, method, Some(params));
        let json = serde_json::to_string(&req)? + "\n";

        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(id, tx);

        // Write to stdin
        {
            let mut child = self.child.lock().await;
            let stdin = child.stdin.as_mut().ok_or_else(|| anyhow::anyhow!("stdin closed"))?;
            stdin.write_all(json.as_bytes()).await?;
            stdin.flush().await?;
        }

        // Wait for response
        let resp = rx.await.map_err(|_| anyhow::anyhow!("Channel closed"))?;

        if let Some(err) = resp.error {
            return Err(anyhow::anyhow!("MCP error {}: {}", err.code, err.message));
        }

        let result = resp.result.ok_or_else(|| anyhow::anyhow!("No result"))?;
        serde_json::from_value(result).map_err(|e| anyhow::anyhow!("Deserialize error: {}", e))
    }

    /// Send a JSON-RPC notification (no response expected).
    async fn notify(&self, method: &str, params: Option<serde_json::Value>) -> anyhow::Result<()> {
        let req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: 0,
            method: method.to_string(),
            params,
        };
        let json = serde_json::to_string(&req)? + "\n";

        let mut child = self.child.lock().await;
        let stdin = child.stdin.as_mut().ok_or_else(|| anyhow::anyhow!("stdin closed"))?;
        stdin.write_all(json.as_bytes()).await?;
        stdin.flush().await?;
        Ok(())
    }

    /// List tools from the MCP server.
    pub async fn list_tools(&self) -> anyhow::Result<Vec<McpTool>> {
        let result: ListToolsResult = self.request("tools/list", serde_json::json!({})).await?;
        Ok(result.tools)
    }

    /// Call a tool on the MCP server.
    pub async fn call_tool(&self, name: &str, arguments: serde_json::Value) -> anyhow::Result<CallToolResult> {
        let params = CallToolParams {
            name: name.to_string(),
            arguments,
        };
        self.request("tools/call", serde_json::to_value(params)?).await
    }

    /// Server name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Kill the server process.
    pub async fn shutdown(&self) {
        let mut child = self.child.lock().await;
        let _ = child.kill().await;
    }
}

impl Drop for McpClient {
    fn drop(&mut self) {
        // Can't await in drop, but the child process will be cleaned up
        // when the handle is dropped.
    }
}

/// Wrapper that makes an MCP server's tools available as ToolProviders.
pub struct McpToolProvider {
    client: std::sync::Arc<McpClient>,
    tool: McpTool,
}

impl McpToolProvider {
    pub fn new(client: std::sync::Arc<McpClient>, tool: McpTool) -> Self {
        Self { client, tool }
    }
}

#[async_trait::async_trait]
impl chatapi_shared::traits::ToolProvider for McpToolProvider {
    fn name(&self) -> &str {
        &self.tool.name
    }

    fn description(&self) -> &str {
        self.tool.description.as_deref().unwrap_or("")
    }

    fn parameters_schema(&self) -> serde_json::Value {
        self.tool.input_schema.clone().unwrap_or_else(|| serde_json::json!({}))
    }

    async fn execute(
        &self,
        args: serde_json::Value,
        _ctx: &chatapi_shared::traits::ToolContext,
    ) -> Result<chatapi_shared::traits::ToolResult, chatapi_shared::traits::ToolError> {
        let result = self.client.call_tool(&self.tool.name, args).await
            .map_err(|e| chatapi_shared::traits::ToolError::ExecutionFailed(e.to_string()))?;

        if result.is_error {
            let msg = result.content.iter()
                .filter_map(|c| c.text.as_deref())
                .collect::<Vec<_>>()
                .join("\n");
            return Ok(chatapi_shared::traits::ToolResult::Error {
                message: msg,
                recoverable: true,
            });
        }

        let text = result.content.iter()
            .filter_map(|c| c.text.as_deref())
            .collect::<Vec<_>>()
            .join("\n");

        Ok(chatapi_shared::traits::ToolResult::Text(text))
    }
}
