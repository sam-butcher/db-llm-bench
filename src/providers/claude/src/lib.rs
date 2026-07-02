//! Claude provider. A small hand-rolled `reqwest` client against the
//! Messages API (no official Rust SDK).

use async_trait::async_trait;
use bench_core::{Message, ModelProvider, ModelResponse, ProviderError};
use serde::Deserialize;

/// Deserialized from this provider's entry in config.yml.
#[derive(Debug, Clone, Deserialize)]
pub struct ClaudeConfig {
    /// Model name, e.g. "opus-4.8".
    pub id: String,
    /// Thinking level, e.g. "xhigh".
    pub thinking: Option<String>,
    pub auth: String,
}

pub struct Claude {
    pub config: ClaudeConfig,
}

impl Claude {
    pub fn new(config: ClaudeConfig) -> Self {
        Self { config }
    }
}

#[async_trait]
impl ModelProvider for Claude {
    fn model_id(&self) -> String {
        format!("claude-{}", self.config.id)
    }

    async fn send_prompt(&self, _conversation: &[Message]) -> Result<ModelResponse, ProviderError> {
        // TODO: POST to the Messages API via reqwest; map usage into tokens.
        todo!("wire up the Claude Messages API")
    }
}
