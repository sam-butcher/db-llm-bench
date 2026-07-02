//! Llama provider: a local model behind a URL, spoken to over plain HTTP —
//! the same shape as the other hand-rolled provider clients.

use async_trait::async_trait;
use bench_core::{Message, ModelProvider, ModelResponse, ProviderError};
use serde::Deserialize;

/// Deserialized from this provider's entry in config.yml.
#[derive(Debug, Clone, Deserialize)]
pub struct LlamaConfig {
    pub url: String,
}

pub struct Llama {
    pub config: LlamaConfig,
}

impl Llama {
    pub fn new(config: LlamaConfig) -> Self {
        Self { config }
    }
}

#[async_trait]
impl ModelProvider for Llama {
    fn model_id(&self) -> String {
        "llama".to_string()
    }

    async fn send_prompt(&self, _conversation: &[Message]) -> Result<ModelResponse, ProviderError> {
        // TODO: POST to the local model's completion endpoint via reqwest.
        todo!("wire up the local llama endpoint")
    }
}
