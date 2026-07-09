//! Provider for any model behind an OpenAI-compatible chat-completions
//! endpoint — OpenAI itself, Groq, OpenRouter, DeepSeek, or a local
//! Ollama/vLLM server. Deliberately speaks only the narrow, universally
//! cloned core of the API (model + messages + output cap), which is what
//! keeps it portable across providers.

use std::time::Duration;

use async_trait::async_trait;
use bench_core::{Message, ModelProvider, ModelResponse, ProviderError, Role, TokenUsage};
use serde::{Deserialize, Serialize};

/// Generous ceiling; the runner's own 600s guard stays the last resort.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(300);

fn default_max_tokens() -> u32 {
    4096
}

/// Which wire field carries the output cap. `max_tokens` is the widely
/// cloned form (Groq, OpenRouter, Ollama, ...); OpenAI's newest models
/// require `max_completion_tokens` instead.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MaxTokensField {
    #[default]
    MaxTokens,
    MaxCompletionTokens,
}

/// Deserialized from this provider's entry in config.yml.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpenAiCompatibleConfig {
    /// The provider's model identifier — naming schemes differ per provider
    /// (e.g. "gpt-5", "llama-3.3-70b-versatile", "deepseek/deepseek-chat").
    pub model: String,
    /// Endpoint base, e.g. "https://api.groq.com/openai/v1" or
    /// "http://localhost:11434/v1"; "/chat/completions" is appended.
    pub base_url: String,
    /// Label identifying this entry in output records; defaults to the
    /// model ID. Also useful to disambiguate the same weights served by
    /// different providers.
    #[serde(default)]
    pub label: Option<String>,
    /// Environment variable holding the API key (e.g. "GROQ_API_KEY").
    /// Omit entirely for unauthenticated local servers.
    #[serde(default)]
    pub api_key_env: Option<String>,
    /// Deliberately modest default: the expected output is a single query.
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
    /// Wire field for the output cap: "max_tokens" (default) or
    /// "max_completion_tokens".
    #[serde(default)]
    pub max_tokens_field: MaxTokensField,
}

pub struct OpenAiCompatible {
    config: OpenAiCompatibleConfig,
    /// None for unauthenticated local servers.
    api_key: Option<String>,
    url: String,
    http: reqwest::Client,
}

impl OpenAiCompatible {
    pub fn new(config: OpenAiCompatibleConfig) -> Result<Self, String> {
        let api_key = match &config.api_key_env {
            Some(env_var) => Some(std::env::var(env_var).map_err(|_| {
                format!("api_key_env `{env_var}` is set in config but the variable is not set")
            })?),
            None => None,
        };
        let url = format!(
            "{}/chat/completions",
            config.base_url.trim_end_matches('/')
        );
        let http = reqwest::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .build()
            .map_err(|e| format!("building HTTP client: {e}"))?;
        Ok(Self {
            config,
            api_key,
            url,
            http,
        })
    }

    fn build_request<'a>(&'a self, conversation: &'a [Message]) -> ChatRequest<'a> {
        let (max_tokens, max_completion_tokens) = match self.config.max_tokens_field {
            MaxTokensField::MaxTokens => (Some(self.config.max_tokens), None),
            MaxTokensField::MaxCompletionTokens => (None, Some(self.config.max_tokens)),
        };
        ChatRequest {
            model: &self.config.model,
            messages: conversation
                .iter()
                .map(|message| WireMessage {
                    role: match message.role {
                        Role::User => "user",
                        Role::Assistant => "assistant",
                    },
                    content: &message.content,
                })
                .collect(),
            max_tokens,
            max_completion_tokens,
        }
    }
}

#[async_trait]
impl ModelProvider for OpenAiCompatible {
    fn model_id(&self) -> String {
        self.config
            .label
            .clone()
            .unwrap_or_else(|| self.config.model.clone())
    }

    async fn send_prompt(&self, conversation: &[Message]) -> Result<ModelResponse, ProviderError> {
        let mut request = self.http.post(&self.url).json(&self.build_request(conversation));
        if let Some(api_key) = &self.api_key {
            request = request.bearer_auth(api_key);
        }
        let response = request
            .send()
            .await
            // Network failures and client-side timeouts are worth a retry.
            .map_err(|e| ProviderError::Transient(format!("request failed: {e}")))?;

        let status = response.status();
        if !status.is_success() {
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "<unreadable body>".to_string());
            return Err(classify_error(status.as_u16(), &body));
        }
        let parsed: ChatResponse = response
            .json()
            .await
            .map_err(|e| ProviderError::Fatal(format!("undecodable API response: {e}")))?;
        into_model_response(parsed)
    }
}

/// An abnormal finish_reason ("length", "content_filter", ...) travels
/// along as the stop reason so the attempt trace can say why there was no
/// usable query; null content (e.g. a filtered response) becomes empty
/// text and flows through extraction as a malformed response.
fn into_model_response(parsed: ChatResponse) -> Result<ModelResponse, ProviderError> {
    let choice = parsed
        .choices
        .into_iter()
        .next()
        .ok_or_else(|| ProviderError::Fatal("API response contained no choices".to_string()))?;
    let stop = choice
        .finish_reason
        .filter(|reason| reason != "stop");
    let usage = parsed.usage.unwrap_or_default();
    Ok(ModelResponse {
        text: choice.message.content.unwrap_or_default(),
        tokens: TokenUsage {
            input: usage.prompt_tokens,
            output: usage.completion_tokens,
        },
        stop,
    })
}

fn classify_error(status: u16, body: &str) -> ProviderError {
    let message = parse_error_message(body).unwrap_or_else(|| body.to_string());
    ProviderError::from_http_status(status, message)
}

fn parse_error_message(body: &str) -> Option<String> {
    let parsed: ErrorResponse = serde_json::from_str(body).ok()?;
    Some(parsed.error.message)
}

#[derive(Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: Vec<WireMessage<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_completion_tokens: Option<u32>,
}

#[derive(Serialize)]
struct WireMessage<'a> {
    role: &'static str,
    content: &'a str,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
    /// Some providers omit usage in edge cases; zeros beat a decode error.
    #[serde(default)]
    usage: Option<Usage>,
}

#[derive(Deserialize)]
struct Choice {
    message: ChoiceMessage,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct ChoiceMessage {
    #[serde(default)]
    content: Option<String>,
}

#[derive(Deserialize, Default)]
struct Usage {
    #[serde(default)]
    prompt_tokens: u64,
    #[serde(default)]
    completion_tokens: u64,
}

#[derive(Deserialize)]
struct ErrorResponse {
    error: ErrorDetail,
}

#[derive(Deserialize)]
struct ErrorDetail {
    message: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> OpenAiCompatibleConfig {
        OpenAiCompatibleConfig {
            model: "llama-3.3-70b-versatile".to_string(),
            base_url: "https://api.groq.com/openai/v1/".to_string(),
            label: None,
            api_key_env: None,
            max_tokens: default_max_tokens(),
            max_tokens_field: MaxTokensField::default(),
        }
    }

    #[test]
    fn builds_the_minimal_portable_request() {
        let provider = OpenAiCompatible::new(config()).unwrap();
        assert_eq!(
            provider.url,
            "https://api.groq.com/openai/v1/chat/completions"
        );
        let conversation = [
            Message::user("generate a query"),
            Message::assistant("```\nbad\n```"),
            Message::user("that failed; fix it"),
        ];
        let request = serde_json::to_value(provider.build_request(&conversation)).unwrap();
        assert_eq!(
            request,
            serde_json::json!({
                "model": "llama-3.3-70b-versatile",
                "messages": [
                    {"role": "user", "content": "generate a query"},
                    {"role": "assistant", "content": "```\nbad\n```"},
                    {"role": "user", "content": "that failed; fix it"},
                ],
                "max_tokens": 4096,
            })
        );
    }

    #[test]
    fn max_completion_tokens_knob_switches_the_wire_field() {
        let mut cfg = config();
        cfg.max_tokens_field = MaxTokensField::MaxCompletionTokens;
        let provider = OpenAiCompatible::new(cfg).unwrap();
        let request = serde_json::to_value(provider.build_request(&[Message::user("q")])).unwrap();
        assert!(request.get("max_tokens").is_none());
        assert_eq!(request["max_completion_tokens"], 4096);
    }

    #[test]
    fn parses_a_normal_response() {
        let parsed: ChatResponse = serde_json::from_str(
            r#"{
                "choices": [{
                    "message": {"role": "assistant", "content": "```\nselect 1\n```"},
                    "finish_reason": "stop"
                }],
                "usage": {"prompt_tokens": 640, "completion_tokens": 22, "total_tokens": 662}
            }"#,
        )
        .unwrap();
        let response = into_model_response(parsed).unwrap();
        assert_eq!(response.text, "```\nselect 1\n```");
        assert_eq!(response.tokens.input, 640);
        assert_eq!(response.tokens.output, 22);
        assert_eq!(response.stop, None);
    }

    #[test]
    fn truncation_and_null_content_surface_as_abnormal() {
        let parsed: ChatResponse = serde_json::from_str(
            r#"{
                "choices": [{"message": {"content": null}, "finish_reason": "content_filter"}]
            }"#,
        )
        .unwrap();
        let response = into_model_response(parsed).unwrap();
        assert_eq!(response.text, "");
        assert_eq!(response.stop.as_deref(), Some("content_filter"));
        // Missing usage decodes to zeros rather than failing.
        assert_eq!(response.tokens.input, 0);

        let empty: ChatResponse = serde_json::from_str(r#"{"choices": []}"#).unwrap();
        assert!(matches!(
            into_model_response(empty),
            Err(ProviderError::Fatal(_))
        ));
    }

    #[test]
    fn api_key_env_is_required_when_named() {
        let mut cfg = config();
        cfg.api_key_env = Some("BENCH_TEST_NO_SUCH_KEY".to_string());
        assert!(OpenAiCompatible::new(cfg).is_err());
        // No api_key_env at all means an unauthenticated local server.
        assert!(OpenAiCompatible::new(config()).unwrap().api_key.is_none());
    }

    #[test]
    fn label_overrides_the_record_model_id() {
        let provider = OpenAiCompatible::new(config()).unwrap();
        assert_eq!(provider.model_id(), "llama-3.3-70b-versatile");

        let mut labelled = config();
        labelled.label = Some("llama-70b-groq".to_string());
        let provider = OpenAiCompatible::new(labelled).unwrap();
        assert_eq!(provider.model_id(), "llama-70b-groq");
    }
}
