use async_trait::async_trait;
use serde::Serialize;
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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct TokenUsage {
    pub input: u64,
    pub output: u64,
}

impl TokenUsage {
    pub fn add(&mut self, other: TokenUsage) {
        self.input += other.input;
        self.output += other.output;
    }
}

#[derive(Debug, Clone)]
pub struct ModelResponse {
    pub text: String,
    pub tokens: TokenUsage,
}

/// Provider errors are never the model's fault, so neither variant counts
/// against the retry budget; the split decides what the harness does next.
#[derive(Debug, Error)]
pub enum ProviderError {
    /// Rate limits, network blips — the harness retries with backoff.
    #[error("transient provider error: {0}")]
    Transient(String),
    /// Auth failures, malformed requests — abort the run.
    #[error("fatal provider error: {0}")]
    Fatal(String),
}

/// Unified interface implemented by each model provider package.
#[async_trait]
pub trait ModelProvider: Send + Sync {
    /// Identifier recorded in the output's `model` field.
    fn model_id(&self) -> String;

    /// Send the conversation so far — the initial prompt plus any retry
    /// feedback — and return the model's response.
    async fn send_prompt(&self, conversation: &[Message]) -> Result<ModelResponse, ProviderError>;
}
