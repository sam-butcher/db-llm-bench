//! Claude provider: a hand-rolled `reqwest` client against the Anthropic
//! Messages API (there is no official Rust SDK). Uses adaptive thinking by
//! default, per current API guidance for Claude 4.6+ models.

use std::time::Duration;

use async_trait::async_trait;
use bench_core::{Message, ModelProvider, ModelResponse, ProviderError, Role, TokenUsage};
use serde::{Deserialize, Serialize};

const MESSAGES_URL: &str = "https://api.anthropic.com/v1/messages";
const ANTHROPIC_VERSION: &str = "2023-06-01";
/// Generous ceiling; the runner's own 600s guard stays the last resort.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(300);

fn default_max_tokens() -> u32 {
    4096
}

/// Model families that accept `thinking: {type: "adaptive"}`. Older models
/// (Haiku 4.5, Sonnet 4.5 and earlier) reject the parameter with a 400.
fn supports_adaptive_thinking(model: &str) -> bool {
    ["fable-5", "mythos-5", "opus-4-6", "opus-4-7", "opus-4-8", "sonnet-4-6"]
        .iter()
        .any(|family| model.contains(family))
}

/// Deserialized from this provider's entry in config.yml.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClaudeConfig {
    /// Model ID, e.g. "claude-opus-4-8".
    pub model: String,
    /// Falls back to the ANTHROPIC_API_KEY environment variable, which is
    /// the recommended place for it — avoid committing keys in config.yml.
    #[serde(default)]
    pub api_key: Option<String>,
    /// Deliberately modest default: the expected output is a single query.
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
    /// Adaptive thinking on/off. Defaults per model: on for families that
    /// support it, off otherwise (e.g. Haiku 4.5 rejects the parameter).
    /// Set explicitly to override the family detection.
    #[serde(default)]
    pub thinking: Option<bool>,
    /// Optional output_config effort level: low | medium | high | xhigh |
    /// max. Only Opus-tier models, Sonnet 4.6, and Fable 5 accept it —
    /// setting it for Haiku 4.5 is a fatal 400.
    #[serde(default)]
    pub effort: Option<String>,
}

impl ClaudeConfig {
    fn thinking_enabled(&self) -> bool {
        self.thinking
            .unwrap_or_else(|| supports_adaptive_thinking(&self.model))
    }
}

pub struct Claude {
    config: ClaudeConfig,
    api_key: String,
    http: reqwest::Client,
}

impl Claude {
    pub fn new(config: ClaudeConfig) -> Result<Self, String> {
        let api_key = config
            .api_key
            .clone()
            .or_else(|| std::env::var("ANTHROPIC_API_KEY").ok())
            .ok_or_else(|| {
                "no Anthropic API key: set ANTHROPIC_API_KEY or `api_key` in the model config"
                    .to_string()
            })?;
        let http = reqwest::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .build()
            .map_err(|e| format!("building HTTP client: {e}"))?;
        Ok(Self {
            config,
            api_key,
            http,
        })
    }

    fn build_request<'a>(&'a self, conversation: &'a [Message]) -> MessagesRequest<'a> {
        MessagesRequest {
            model: &self.config.model,
            max_tokens: self.config.max_tokens,
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
            thinking: self
                .config
                .thinking_enabled()
                .then_some(Thinking { r#type: "adaptive" }),
            output_config: self
                .config
                .effort
                .as_deref()
                .map(|effort| OutputConfig { effort }),
        }
    }
}

#[async_trait]
impl ModelProvider for Claude {
    fn model_id(&self) -> String {
        self.config.model.clone()
    }

    async fn send_prompt(&self, conversation: &[Message]) -> Result<ModelResponse, ProviderError> {
        let response = self
            .http
            .post(MESSAGES_URL)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .json(&self.build_request(conversation))
            .send()
            .await
            // Network failures and client-side timeouts are worth a retry.
            .map_err(|e| ProviderError::Transient(format!("request failed: {e}")))?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(classify_error(status.as_u16(), &body));
        }
        let parsed: MessagesResponse = response
            .json()
            .await
            .map_err(|e| ProviderError::Fatal(format!("undecodable API response: {e}")))?;

        // A `refusal` stop reason arrives as a successful response with no
        // text content; the empty text flows through extraction as a
        // malformed response and is scored against the model, not the run.
        let text = parsed
            .content
            .iter()
            .filter(|block| block.kind == "text")
            .map(|block| block.text.as_str())
            .collect::<Vec<_>>()
            .join("");
        Ok(ModelResponse {
            text,
            tokens: TokenUsage {
                input: parsed.usage.input_tokens,
                output: parsed.usage.output_tokens,
            },
        })
    }
}

/// 408/429 and all 5xx (including 529 overloaded) are retryable; the
/// remaining 4xx (bad request, auth, not found) won't get better on retry.
fn classify_error(status: u16, body: &str) -> ProviderError {
    let message = parse_error_message(body).unwrap_or_else(|| body.to_string());
    let detail = format!("HTTP {status}: {message}");
    if status == 408 || status == 429 || status >= 500 {
        ProviderError::Transient(detail)
    } else {
        ProviderError::Fatal(detail)
    }
}

fn parse_error_message(body: &str) -> Option<String> {
    let parsed: ErrorResponse = serde_json::from_str(body).ok()?;
    Some(parsed.error.message)
}

#[derive(Serialize)]
struct MessagesRequest<'a> {
    model: &'a str,
    max_tokens: u32,
    messages: Vec<WireMessage<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    thinking: Option<Thinking>,
    #[serde(skip_serializing_if = "Option::is_none")]
    output_config: Option<OutputConfig<'a>>,
}

#[derive(Serialize)]
struct WireMessage<'a> {
    role: &'static str,
    content: &'a str,
}

#[derive(Serialize)]
struct Thinking {
    r#type: &'static str,
}

#[derive(Serialize)]
struct OutputConfig<'a> {
    effort: &'a str,
}

#[derive(Deserialize)]
struct MessagesResponse {
    content: Vec<ContentBlock>,
    usage: Usage,
}

#[derive(Deserialize)]
struct ContentBlock {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    text: String,
}

#[derive(Deserialize)]
struct Usage {
    input_tokens: u64,
    output_tokens: u64,
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

    fn config() -> ClaudeConfig {
        ClaudeConfig {
            model: "claude-opus-4-8".to_string(),
            api_key: Some("test-key".to_string()),
            max_tokens: default_max_tokens(),
            thinking: None,
            effort: Some("high".to_string()),
        }
    }

    #[test]
    fn builds_the_expected_request_shape() {
        let claude = Claude::new(config()).unwrap();
        let conversation = [
            Message::user("generate a query"),
            Message::assistant("```\nbad\n```"),
            Message::user("that failed; fix it"),
        ];
        let request = serde_json::to_value(claude.build_request(&conversation)).unwrap();
        assert_eq!(
            request,
            serde_json::json!({
                "model": "claude-opus-4-8",
                "max_tokens": 4096,
                "messages": [
                    {"role": "user", "content": "generate a query"},
                    {"role": "assistant", "content": "```\nbad\n```"},
                    {"role": "user", "content": "that failed; fix it"},
                ],
                "thinking": {"type": "adaptive"},
                "output_config": {"effort": "high"},
            })
        );
    }

    #[test]
    fn thinking_and_effort_are_omitted_when_disabled() {
        let mut cfg = config();
        cfg.thinking = Some(false);
        cfg.effort = None;
        let claude = Claude::new(cfg).unwrap();
        let request = serde_json::to_value(claude.build_request(&[Message::user("q")])).unwrap();
        assert!(request.get("thinking").is_none());
        assert!(request.get("output_config").is_none());
    }

    /// The default follows model capability: models that reject the
    /// adaptive-thinking parameter must not receive it.
    #[test]
    fn thinking_defaults_follow_the_model_family() {
        let mut cfg = config();
        cfg.model = "claude-haiku-4-5-20251001".to_string();
        cfg.effort = None;
        let claude = Claude::new(cfg).unwrap();
        let request = serde_json::to_value(claude.build_request(&[Message::user("q")])).unwrap();
        assert!(request.get("thinking").is_none());

        let claude = Claude::new(config()).unwrap();
        let request = serde_json::to_value(claude.build_request(&[Message::user("q")])).unwrap();
        assert_eq!(request["thinking"], serde_json::json!({"type": "adaptive"}));

        // An explicit setting overrides the family detection both ways.
        let mut cfg = config();
        cfg.model = "claude-haiku-4-5-20251001".to_string();
        cfg.thinking = Some(true);
        cfg.effort = None;
        let claude = Claude::new(cfg).unwrap();
        let request = serde_json::to_value(claude.build_request(&[Message::user("q")])).unwrap();
        assert_eq!(request["thinking"], serde_json::json!({"type": "adaptive"}));
    }

    #[test]
    fn extracts_text_blocks_and_usage_from_a_response() {
        // Thinking blocks (empty text under the default display) are skipped.
        let parsed: MessagesResponse = serde_json::from_str(
            r#"{
                "content": [
                    {"type": "thinking", "thinking": "", "signature": "x"},
                    {"type": "text", "text": "Here you go:\n"},
                    {"type": "text", "text": "```\nselect 1\n```"}
                ],
                "stop_reason": "end_turn",
                "usage": {"input_tokens": 812, "output_tokens": 40}
            }"#,
        )
        .unwrap();
        let text: String = parsed
            .content
            .iter()
            .filter(|b| b.kind == "text")
            .map(|b| b.text.as_str())
            .collect();
        assert_eq!(text, "Here you go:\n```\nselect 1\n```");
        assert_eq!(parsed.usage.input_tokens, 812);
        assert_eq!(parsed.usage.output_tokens, 40);
    }

    #[test]
    fn classifies_statuses_by_retryability() {
        let body = r#"{"type":"error","error":{"type":"rate_limit_error","message":"slow down"}}"#;
        assert!(matches!(
            classify_error(429, body),
            ProviderError::Transient(m) if m.contains("slow down")
        ));
        assert!(matches!(classify_error(529, "overloaded"), ProviderError::Transient(_)));
        assert!(matches!(classify_error(500, ""), ProviderError::Transient(_)));
        assert!(matches!(classify_error(408, ""), ProviderError::Transient(_)));
        assert!(matches!(classify_error(400, "bad"), ProviderError::Fatal(_)));
        assert!(matches!(classify_error(401, ""), ProviderError::Fatal(_)));
        assert!(matches!(classify_error(404, ""), ProviderError::Fatal(_)));
    }

    #[test]
    fn missing_api_key_is_an_error() {
        let mut cfg = config();
        cfg.api_key = None;
        // Only meaningful when the env var is absent; skip otherwise.
        if std::env::var("ANTHROPIC_API_KEY").is_err() {
            assert!(Claude::new(cfg).is_err());
        }
    }
}
