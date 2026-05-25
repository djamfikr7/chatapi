//! Target configuration for where requests are routed.
//!
//! Two modes:
//! - **Browser** (default): CDP automation of free chat (DeepSeek, etc.) — text-only, needs tool call parsing
//! - **API**: Native OpenAI-compatible API — full tool_calls support, key required

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Target {
    /// Automate a free browser chat via CDP.
    Browser,
    /// Use a native API with API key.
    Api,
}

impl Default for Target {
    fn default() -> Self {
        Self::Browser
    }
}

/// Configuration for the target provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetConfig {
    /// Which mode to use.
    #[serde(default)]
    pub target: Target,
    /// API endpoint URL.
    /// - Browser mode: WebSocket URL of the browser debug interface (ws://...)
    /// - API mode: e.g. "https://api.deepseek.com/v1" or any OpenAI-compatible endpoint
    #[serde(default = "default_api_endpoint")]
    pub api_endpoint: String,
    /// API key (required for API mode, ignored for browser mode).
    #[serde(default)]
    pub api_key: Option<String>,
    /// Model name to use in responses.
    #[serde(default = "default_model")]
    pub model: String,
}

impl Default for TargetConfig {
    fn default() -> Self {
        Self {
            target: Target::Browser,
            api_endpoint: default_api_endpoint(),
            api_key: None,
            model: default_model(),
        }
    }
}

impl TargetConfig {
    /// Whether this config uses native API (has tool_calls support).
    pub fn is_api(&self) -> bool {
        self.target == Target::Api
    }

    /// Whether this config uses browser automation (needs tool call parsing).
    pub fn is_browser(&self) -> bool {
        self.target == Target::Browser
    }

    /// Validate the config (e.g., API key required for API mode).
    pub fn validate(&self) -> Result<(), String> {
        match self.target {
            Target::Api => {
                if self.api_key.is_none() {
                    return Err("api_key is required when target = 'api'".to_string());
                }
                Ok(())
            }
            Target::Browser => Ok(()),
        }
    }
}

fn default_api_endpoint() -> String {
    "https://api.deepseek.com/v1".to_string()
}

fn default_model() -> String {
    "deepseek-chat".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_is_browser() {
        let config = TargetConfig::default();
        assert!(config.is_browser());
        assert!(!config.is_api());
    }

    #[test]
    fn test_api_requires_key() {
        let config = TargetConfig {
            target: Target::Api,
            api_endpoint: "https://api.deepseek.com/v1".to_string(),
            api_key: None,
            model: "deepseek-chat".to_string(),
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_api_with_key_validates() {
        let config = TargetConfig {
            target: Target::Api,
            api_endpoint: "https://api.deepseek.com/v1".to_string(),
            api_key: Some("sk-xxx".to_string()),
            model: "deepseek-chat".to_string(),
        };
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_browser_always_validates() {
        let config = TargetConfig::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_serde_roundtrip() {
        let config = TargetConfig {
            target: Target::Api,
            api_endpoint: "https://api.deepseek.com/v1".to_string(),
            api_key: Some("sk-test".to_string()),
            model: "deepseek-chat".to_string(),
        };
        let json = serde_json::to_string(&config).unwrap();
        let back: TargetConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back.target, Target::Api);
        assert_eq!(back.api_key, Some("sk-test".to_string()));
    }
}
