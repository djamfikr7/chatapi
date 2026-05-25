use async_trait::async_trait;
use chatapi_shared::traits::{TargetError, TargetProvider, TargetStream};
use chatapi_shared::{ChatCompletionRequest, ChatCompletionResponse};
use chatapi_shared::target::{TargetConfig, Target};
use tracing::info;

use crate::api::ApiTarget;
use crate::browser::BrowserTarget;

/// Routes requests to the appropriate target provider.
pub struct TargetRouter {
    inner: Box<dyn TargetProvider>,
}

impl TargetRouter {
    pub fn new(config: &TargetConfig) -> Self {
        match config.target {
            Target::Api => {
                let api = ApiTarget::new(
                    config.api_endpoint.clone(),
                    config.api_key.clone().unwrap_or_default(),
                    config.model.clone(),
                );
                info!("Target: API ({})", config.api_endpoint);
                Self { inner: Box::new(api) }
            }
            Target::Browser => {
                // Browser target requires a live Chrome connection.
                // If Chrome isn't available, fall back to a stub that returns errors.
                info!("Target: Browser (CDP) — requires Chrome with --remote-debugging-port");
                // For now, create a stub. The gateway main.rs will create the real
                // BrowserTarget when Chrome is available.
                Self { inner: Box::new(BrowserStub) }
            }
        }
    }

    /// Create a TargetRouter with a pre-built BrowserTarget.
    pub fn with_browser(browser: BrowserTarget) -> Self {
        Self { inner: Box::new(browser) }
    }

    /// Create a TargetRouter with a pre-built ApiTarget.
    pub fn with_api(api: ApiTarget) -> Self {
        Self { inner: Box::new(api) }
    }
}

#[async_trait]
impl TargetProvider for TargetRouter {
    fn name(&self) -> &str {
        self.inner.name()
    }

    async fn health_check(&self) -> bool {
        self.inner.health_check().await
    }

    async fn send_request(
        &self,
        request: &ChatCompletionRequest,
    ) -> Result<ChatCompletionResponse, TargetError> {
        self.inner.send_request(request).await
    }

    async fn stream_request(
        &self,
        request: &ChatCompletionRequest,
    ) -> Result<TargetStream, TargetError> {
        self.inner.stream_request(request).await
    }
}

/// Stub for when browser mode is configured but Chrome isn't available.
struct BrowserStub;

#[async_trait]
impl TargetProvider for BrowserStub {
    fn name(&self) -> &str { "browser-stub" }

    async fn health_check(&self) -> bool { false }

    async fn send_request(&self, _: &ChatCompletionRequest) -> Result<ChatCompletionResponse, TargetError> {
        Err(TargetError::ConnectionFailed(
            "Browser target not connected. Start Chrome with --remote-debugging-port=9222".to_string()
        ))
    }

    async fn stream_request(&self, _: &ChatCompletionRequest) -> Result<TargetStream, TargetError> {
        Err(TargetError::ConnectionFailed(
            "Browser target not connected. Start Chrome with --remote-debugging-port=9222".to_string()
        ))
    }
}
