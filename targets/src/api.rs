use async_trait::async_trait;
use chatapi_shared::traits::{TargetError, TargetProvider, TargetStream};
use chatapi_shared::{ChatCompletionRequest, ChatCompletionResponse};
use futures_util::StreamExt;
use reqwest::Client;
use tracing::{debug, error};

/// Direct OpenAI-compatible API target.
pub struct ApiTarget {
    client: Client,
    endpoint: String,
    api_key: String,
    model: String,
}

impl ApiTarget {
    pub fn new(endpoint: String, api_key: String, model: String) -> Self {
        Self {
            client: Client::new(),
            endpoint,
            api_key,
            model,
        }
    }

    fn completions_url(&self) -> String {
        format!("{}/chat/completions", self.endpoint.trim_end_matches('/'))
    }
}

#[async_trait]
impl TargetProvider for ApiTarget {
    fn name(&self) -> &str {
        "api"
    }

    async fn health_check(&self) -> bool {
        // Try a lightweight GET to the models endpoint
        let url = format!("{}/models", self.endpoint.trim_end_matches('/'));
        match self
            .client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .timeout(std::time::Duration::from_secs(5))
            .send()
            .await
        {
            Ok(resp) => resp.status().is_success() || resp.status().as_u16() == 404,
            Err(_) => false,
        }
    }

    async fn send_request(
        &self,
        req: &ChatCompletionRequest,
    ) -> Result<ChatCompletionResponse, TargetError> {
        let mut body = serde_json::to_value(req)
            .map_err(|e| TargetError::RequestFailed(e.to_string()))?;
        // Override model with configured model
        body["model"] = serde_json::Value::String(self.model.clone());
        body["stream"] = serde_json::Value::Bool(false);

        debug!(url = %self.completions_url(), "API request (non-streaming)");

        let resp = self
            .client
            .post(self.completions_url())
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| TargetError::ConnectionFailed(e.to_string()))?;

        let status = resp.status();
        if status.as_u16() == 401 {
            return Err(TargetError::RequestFailed("Unauthorized (401)".into()));
        }
        if status.as_u16() == 429 {
            return Err(TargetError::RequestFailed("Rate limited (429)".into()));
        }
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            error!(status = %status, body = %text, "API error");
            return Err(TargetError::RequestFailed(format!(
                "API returned {}: {}",
                status, text
            )));
        }

        resp.json::<ChatCompletionResponse>()
            .await
            .map_err(|e| TargetError::RequestFailed(e.to_string()))
    }

    async fn stream_request(
        &self,
        req: &ChatCompletionRequest,
    ) -> Result<TargetStream, TargetError> {
        let mut body = serde_json::to_value(req)
            .map_err(|e| TargetError::RequestFailed(e.to_string()))?;
        body["model"] = serde_json::Value::String(self.model.clone());
        body["stream"] = serde_json::Value::Bool(true);

        debug!(url = %self.completions_url(), "API request (streaming)");

        let resp = self
            .client
            .post(self.completions_url())
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| TargetError::ConnectionFailed(e.to_string()))?;

        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            error!(status = %status, body = %text, "API stream error");
            return Err(TargetError::RequestFailed(format!(
                "API returned {}: {}",
                status, text
            )));
        }

        let stream = resp.bytes_stream().map(|chunk: Result<bytes::Bytes, reqwest::Error>| {
            chunk
                .map(|bytes: bytes::Bytes| {
                    let text = String::from_utf8_lossy(&bytes).to_string();
                    // Extract token content from SSE lines
                    let mut tokens = Vec::new();
                    for line in text.lines() {
                        if let Some(data) = line.strip_prefix("data: ") {
                            if data.trim() == "[DONE]" {
                                continue;
                            }
                            if let Ok(val) = serde_json::from_str::<serde_json::Value>(data) {
                                if let Some(content) =
                                    val["choices"][0]["delta"]["content"].as_str()
                                {
                                    tokens.push(content.to_string());
                                }
                            }
                        }
                    }
                    tokens.join("")
                })
                .map_err(|e: reqwest::Error| TargetError::RequestFailed(e.to_string()))
        });

        Ok(Box::pin(stream))
    }
}
