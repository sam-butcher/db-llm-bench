use async_trait::async_trait;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    User,
    Assistant,
}

#[derive(Debug, Clone)]
pub struct Message {
    pub role: Role,
    pub content: String,
}

#[derive(Debug, Clone)]
pub struct ModelResponse {
    pub text: String,
    pub tokens: u64,
}

#[derive(Debug, Error)]
#[error("provider error: {0}")]
pub struct ProviderError(pub String);

/// Unified interface implemented by each model provider package.
#[async_trait]
pub trait ModelProvider: Send + Sync {
    /// Identifier recorded in the output's `model` field.
    fn model_id(&self) -> String;

    /// Send the conversation so far — the initial prompt plus any retry
    /// feedback — and return the model's response.
    async fn send_prompt(&self, conversation: &[Message]) -> Result<ModelResponse, ProviderError>;
}
