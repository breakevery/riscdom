//! LLM clients: the trait plus a DeepSeek HTTP client and a scripted mock.

use crate::config::AgentConfig;
use crate::error::AgentError;
use crate::message::{ChatRequest, ChatResponse};
use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::Duration;

/// Anything that can answer a [`ChatRequest`].
pub trait LlmClient: Send + Sync {
    fn chat(&self, req: ChatRequest) -> Result<ChatResponse, AgentError>;
}

/// DeepSeek chat-completions client (blocking).
pub struct DeepSeekClient {
    config: AgentConfig,
    http: reqwest::blocking::Client,
}

impl DeepSeekClient {
    /// Build a client from configuration.
    pub fn new(config: AgentConfig) -> Result<Self, AgentError> {
        let http = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(config.request_timeout_secs))
            .build()
            .map_err(|e| AgentError::Http(e.to_string()))?;
        Ok(Self { config, http })
    }

    /// The endpoint this client posts to.
    pub fn endpoint(&self) -> String {
        self.config.endpoint()
    }
}

impl LlmClient for DeepSeekClient {
    fn chat(&self, req: ChatRequest) -> Result<ChatResponse, AgentError> {
        let response = self
            .http
            .post(self.config.endpoint())
            .bearer_auth(&self.config.api_key)
            .json(&req)
            .send()
            .map_err(|e| AgentError::Http(e.to_string()))?;

        let status = response.status();
        let body = response
            .text()
            .map_err(|e| AgentError::Http(e.to_string()))?;

        if !status.is_success() {
            // `body` is the server's error text; it never contains our key.
            return Err(AgentError::Api(format!(
                "HTTP {}: {}",
                status.as_u16(),
                truncate(&body, 512)
            )));
        }

        serde_json::from_str::<ChatResponse>(&body).map_err(|e| {
            AgentError::Api(format!(
                "failed to parse response: {e}; body: {}",
                truncate(&body, 512)
            ))
        })
    }
}

/// A scripted LLM for tests: returns queued responses in order.
pub struct MockLlm {
    script: Mutex<VecDeque<ChatResponse>>,
}

impl MockLlm {
    pub fn new(script: Vec<ChatResponse>) -> Self {
        Self {
            script: Mutex::new(script.into()),
        }
    }
}

impl LlmClient for MockLlm {
    fn chat(&self, _req: ChatRequest) -> Result<ChatResponse, AgentError> {
        let mut script = self
            .script
            .lock()
            .map_err(|_| AgentError::Other("mock script mutex poisoned".into()))?;
        script
            .pop_front()
            .ok_or_else(|| AgentError::Other("MockLlm script exhausted".into()))
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        let mut end = max;
        while !s.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}…", &s[..end])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::{ChatMessage, Choice};

    fn resp(text: &str) -> ChatResponse {
        ChatResponse {
            id: None,
            model: None,
            choices: vec![Choice {
                index: Some(0),
                message: ChatMessage::text("assistant", text),
                finish_reason: Some("stop".into()),
            }],
            usage: None,
        }
    }

    fn req() -> ChatRequest {
        ChatRequest {
            model: "deepseek-chat".into(),
            messages: vec![ChatMessage::text("user", "hi")],
            tools: None,
            tool_choice: None,
            temperature: Some(0.0),
            stream: None,
        }
    }

    #[test]
    fn mock_returns_scripted_responses_in_order() {
        let mock = MockLlm::new(vec![resp("one"), resp("two")]);
        assert_eq!(mock.chat(req()).unwrap().content(), Some("one"));
        assert_eq!(mock.chat(req()).unwrap().content(), Some("two"));
        assert!(mock.chat(req()).is_err(), "exhausted script must error");
    }
}
