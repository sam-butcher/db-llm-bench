//! Dummy provider for testing: replays scripted responses in order and
//! errors once the script runs out. Every conversation received is recorded
//! for assertions. Token count is the response text length.

use std::collections::VecDeque;
use std::sync::Mutex;

use async_trait::async_trait;
use bench_core::{Message, ModelProvider, ModelResponse, ProviderError};
use serde::Deserialize;

/// Deserialized from this provider's entry in config.yml, for smoke runs
/// through the CLI.
#[derive(Debug, Clone, Deserialize)]
pub struct DummyConfig {
    pub responses: Vec<String>,
}

pub struct DummyProvider {
    responses: Mutex<VecDeque<String>>,
    conversations: Mutex<Vec<Vec<Message>>>,
}

impl DummyProvider {
    pub fn new(responses: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            responses: Mutex::new(responses.into_iter().map(Into::into).collect()),
            conversations: Mutex::new(Vec::new()),
        }
    }

    pub fn push_response(&self, text: impl Into<String>) {
        self.responses.lock().unwrap().push_back(text.into());
    }

    /// Every conversation received so far, in order.
    pub fn conversations(&self) -> Vec<Vec<Message>> {
        self.conversations.lock().unwrap().clone()
    }
}

impl From<DummyConfig> for DummyProvider {
    fn from(config: DummyConfig) -> Self {
        Self::new(config.responses)
    }
}

#[async_trait]
impl ModelProvider for DummyProvider {
    fn model_id(&self) -> String {
        "dummy".to_string()
    }

    async fn send_prompt(&self, conversation: &[Message]) -> Result<ModelResponse, ProviderError> {
        self.conversations.lock().unwrap().push(conversation.to_vec());
        match self.responses.lock().unwrap().pop_front() {
            Some(text) => Ok(ModelResponse {
                tokens: text.len() as u64,
                text,
            }),
            None => Err(ProviderError(
                "dummy provider ran out of scripted responses".to_string(),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bench_core::Role;

    #[tokio::test]
    async fn replays_responses_in_order_then_errors() {
        let provider = DummyProvider::new(["first"]);
        provider.push_response("second");
        let conversation = [Message {
            role: Role::User,
            content: "hi".to_string(),
        }];
        assert_eq!(
            provider.send_prompt(&conversation).await.unwrap().text,
            "first"
        );
        assert_eq!(
            provider.send_prompt(&conversation).await.unwrap().text,
            "second"
        );
        assert!(provider.send_prompt(&conversation).await.is_err());
        assert_eq!(provider.conversations().len(), 3);
    }
}
