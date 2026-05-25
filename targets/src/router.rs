use async_trait::async_trait;
use chatapi_shared::target::{Target, TargetConfig};
use chatapi_shared::traits::{TargetError, TargetProvider, TargetStream};
use chatapi_shared::{ChatCompletionRequest, ChatCompletionResponse};
use tracing::info;

use crate::api::ApiTarget;

/// Routes requests to the appropriate target provider based on config.
pub struct TargetRouter {
    inner: Box<dyn TargetProvider>,
}

impl TargetRouter {
    pub fn new(config: &TargetConfig) -> Self {
        let inner: Box<dyn TargetProvider> = match config.target {
            Target::Api => {
                let api_key = config
                    .api_key
                    .clone()
                    .expect("api_key required for API target");
                info!(endpoint = %config.api_endpoint, model = %config.model, "Creating API target");
                Box::new(ApiTarget::new(
                    config.api_endpoint.clone(),
                    api_key,
                    config.model.clone(),
                ))
            }
            Target::Browser => {
                // For browser mode, we'll use a stub that returns an error
                // The actual CDP engine integration happens separately
                info!("Browser target selected (CDP engine handles this separately)");
                Box::new(BrowserStub)
            }
        };

        Self { inner }
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
        req: &ChatCompletionRequest,
    ) -> Result<ChatCompletionResponse, TargetError> {
        self.inner.send_request(req).await
    }

    async fn stream_request(
        &self,
        req: &ChatCompletionRequest,
    ) -> Result<TargetStream, TargetError> {
        self.inner.stream_request(req).await
    }
}

/// Stub for browser mode — actual CDP integration is in the cdp-engine crate.
struct BrowserStub;

#[async_trait]
impl TargetProvider for BrowserStub {
    fn name(&self) -> &str {
        "browser"
    }

    async fn health_check(&self) -> bool {
        false
    }

    async fn send_request(
        &self,
        _req: &ChatCompletionRequest,
    ) -> Result<ChatCompletionResponse, TargetError> {
        Err(TargetError::RequestFailed(
            "Browser target not yet connected. Use CDP engine directly.".into(),
        ))
    }

    async fn stream_request(
        &self,
        _req: &ChatCompletionRequest,
    ) -> Result<TargetStream, TargetError> {
        Err(TargetError::RequestFailed(
            "Browser target not yet connected. Use CDP engine directly.".into(),
        ))
    }
}
