//! ChatGPT provider. A small hand-rolled `reqwest` client against the OpenAI
//! chat completions API (no official Rust SDK).

use async_trait::async_trait;
use bench_core::{Message, ModelProvider, ModelResponse, ProviderError};
use serde::Deserialize;

/// Deserialized from this provider's entry in config.yml.
#[derive(Debug, Clone, Deserialize)]
pub struct ChatGptConfig {
    /// Model name.
    pub id: String,
    pub auth: String,
}

pub struct ChatGpt {
    pub config: ChatGptConfig,
}

impl ChatGpt {
    pub fn new(config: ChatGptConfig) -> Self {
        Self { config }
    }
}

#[async_trait]
impl ModelProvider for ChatGpt {
    fn model_id(&self) -> String {
        format!("chatgpt-{}", self.config.id)
    }

    async fn send_prompt(&self, _conversation: &[Message]) -> Result<ModelResponse, ProviderError> {
        // TODO: POST to the chat completions API via reqwest; map usage into tokens.
        todo!("wire up the OpenAI chat completions API")
    }
}
