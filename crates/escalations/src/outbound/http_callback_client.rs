//! HTTP delivery of escalation-resolution callbacks to the agent runtime.

use std::time::Duration;

use crate::domain::model::{EscalationError, Result};
use crate::domain::ports::EscalationCallbackClient;

/// Posts resolution payloads to the runtime's callback URL.
#[derive(Debug, Clone)]
pub struct HttpCallbackClient {
    client: reqwest::Client,
    /// Bearer token the runtime uses to verify the callback came from Macro.
    callback_token: Option<String>,
}

impl HttpCallbackClient {
    /// Build the client. `callback_token`, when set, is sent as a bearer
    /// token so the runtime can authenticate the callback.
    pub fn new(callback_token: Option<String>) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .expect("reqwest client construction cannot fail with static options");
        Self {
            client,
            callback_token,
        }
    }
}

impl EscalationCallbackClient for HttpCallbackClient {
    #[tracing::instrument(skip(self, payload), err)]
    async fn deliver(&self, callback_url: &str, payload: &serde_json::Value) -> Result<()> {
        let mut request = self.client.post(callback_url).json(payload);
        if let Some(token) = &self.callback_token {
            request = request.bearer_auth(token);
        }
        let response = request.send().await.map_err(|e| {
            EscalationError::InvalidRequest(format!("callback delivery failed: {e}"))
        })?;
        if !response.status().is_success() {
            return Err(EscalationError::InvalidRequest(format!(
                "callback endpoint returned {}",
                response.status()
            )));
        }
        Ok(())
    }
}
