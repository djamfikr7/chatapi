use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};
use anyhow::{Context, Result};

// ── Top-level config ────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ChatApiConfig {
    #[serde(default)]
    pub target: TargetSection,
    #[serde(default)]
    pub rules: RulesSection,
    #[serde(default)]
    pub sessions: SessionsSection,
    #[serde(default)]
    pub dashboard: DashboardSection,
    #[serde(default)]
    pub models: ModelsSection,
}

// ── Target section ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetSection {
    #[serde(default = "default_mode")]
    pub mode: String,
    #[serde(default = "default_model")]
    pub model: String,
    pub api: Option<ApiSection>,
    pub mcp: Option<McpSection>,
}

impl Default for TargetSection {
    fn default() -> Self {
        Self {
            mode: default_mode(),
            model: default_model(),
            api: None,
            mcp: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiSection {
    pub endpoint: String,
    pub api_key_env: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct McpSection {
    #[serde(default)]
    pub servers: Vec<McpServerConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    pub name: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
}

// ── Rules section ───────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RulesSection {
    pub system_prompt: Option<String>,
    pub working_dir: Option<String>,
    #[serde(default)]
    pub allowed_tools: Vec<String>,
    #[serde(default)]
    pub blocked_paths: Vec<String>,
    pub context: Option<ContextSection>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ContextSection {
    #[serde(default)]
    pub include_files: Vec<String>,
    pub max_context_tokens: Option<u64>,
}

// ── Sessions section ────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionsSection {
    #[serde(default = "default_store")]
    pub store: String,
    pub path: Option<String>,
}

impl Default for SessionsSection {
    fn default() -> Self {
        Self {
            store: default_store(),
            path: None,
        }
    }
}

// ── Dashboard section ───────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DashboardSection {
    pub port: Option<u16>,
    pub theme: Option<String>,
}

// ── Models section (multi-provider support) ─────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModelsSection {
    #[serde(default)]
    pub providers: Vec<ModelProvider>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelProvider {
    pub name: String,
    pub endpoint: String,
    pub api_key_env: String,
    #[serde(default)]
    pub models: Vec<ModelEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelEntry {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub max_tokens: Option<u32>,
}

// ── Default functions ───────────────────────────────────────────────

fn default_mode() -> String { "browser".to_string() }
fn default_model() -> String { "deepseek-chat".to_string() }
fn default_store() -> String { "memory".to_string() }

// ── ChatApiConfig methods ───────────────────────────────────────────

impl ChatApiConfig {
    /// Load config from a TOML file.
    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read config: {}", path.display()))?;
        let config: Self = toml::from_str(&content)
            .with_context(|| format!("failed to parse config: {}", path.display()))?;
        Ok(config)
    }

    /// Load config, falling back to defaults if file doesn't exist.
    pub fn load_or_default(path: &Path) -> Self {
        Self::load(path).unwrap_or_default()
    }

    /// Save config to a TOML file.
    pub fn save(&self, path: &Path) -> Result<()> {
        let content = toml::to_string_pretty(self)
            .context("failed to serialize config")?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create config dir: {}", parent.display()))?;
        }
        std::fs::write(path, content)
            .with_context(|| format!("failed to write config: {}", path.display()))?;
        Ok(())
    }

    /// Get the effective working directory.
    pub fn working_dir(&self) -> std::path::PathBuf {
        self.rules
            .working_dir
            .as_deref()
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from(".")))
    }

    /// Get the dashboard port.
    pub fn dashboard_port(&self) -> u16 {
        self.dashboard.port.unwrap_or(8091)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_default_config() {
        let config = ChatApiConfig::default();
        assert_eq!(config.target.mode, "browser");
        assert_eq!(config.target.model, "deepseek-chat");
        assert_eq!(config.sessions.store, "memory");
        assert_eq!(config.dashboard_port(), 8091);
    }

    #[test]
    fn test_load_from_toml() {
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        writeln!(tmp, r#"
[target]
mode = "api"
model = "gpt-4"

[target.api]
endpoint = "https://api.openai.com/v1"
api_key_env = "OPENAI_API_KEY"

[rules]
system_prompt = "You are helpful."
working_dir = "/project"
allowed_tools = ["read_file", "edit_file"]
blocked_paths = [".env", "secrets/"]

[sessions]
store = "sqlite"
path = "sessions.db"
"#).unwrap();

        let config = ChatApiConfig::load(tmp.path()).unwrap();
        assert_eq!(config.target.mode, "api");
        assert_eq!(config.target.model, "gpt-4");
        assert_eq!(config.rules.system_prompt.as_deref(), Some("You are helpful."));
        assert_eq!(config.rules.allowed_tools, vec!["read_file", "edit_file"]);
        assert_eq!(config.rules.blocked_paths, vec![".env", "secrets/"]);
        assert_eq!(config.sessions.store, "sqlite");
        assert!(config.target.api.is_some());
    }

    #[test]
    fn test_save_and_reload() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");

        let mut config = ChatApiConfig::default();
        config.target.mode = "api".to_string();
        config.target.model = "gpt-4".to_string();
        config.rules.system_prompt = Some("Test prompt".to_string());

        config.save(&path).unwrap();
        let loaded = ChatApiConfig::load(&path).unwrap();

        assert_eq!(loaded.target.mode, "api");
        assert_eq!(loaded.target.model, "gpt-4");
        assert_eq!(loaded.rules.system_prompt.as_deref(), Some("Test prompt"));
    }

    #[test]
    fn test_load_or_default_missing_file() {
        let config = ChatApiConfig::load_or_default(Path::new("/nonexistent/config.toml"));
        assert_eq!(config.target.mode, "browser");
    }

    #[test]
    fn test_partial_config() {
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        writeln!(tmp, r#"
[target]
mode = "mcp"

[rules]
system_prompt = "Be concise."
"#).unwrap();

        let config = ChatApiConfig::load(tmp.path()).unwrap();
        assert_eq!(config.target.mode, "mcp");
        assert_eq!(config.target.model, "deepseek-chat"); // default
        assert_eq!(config.sessions.store, "memory"); // default
    }
}
