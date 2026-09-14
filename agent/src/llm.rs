//! LLM clients: the trait, an OpenAI-compatible HTTP client, and a scripted mock.

use crate::config::AgentConfig;
use crate::error::AgentError;
use crate::message::{
    ChatMessage, ChatRequest, ChatResponse, Choice, FunctionCall, StreamEvent, ToolCall,
};
use crate::sse::SseAccumulator;
use serde::Deserialize;
use std::collections::VecDeque;
use std::io::BufRead;
use std::sync::Mutex;
use std::time::Duration;

/// Anything that can answer a [`ChatRequest`].
pub trait LlmClient: Send + Sync {
    fn chat(&self, req: ChatRequest) -> Result<ChatResponse, AgentError>;

    /// Streaming variant.
    ///
    /// The default implementation degrades to [`LlmClient::chat`]: the whole
    /// content is delivered as a single [`StreamEvent::Delta`], then
    /// [`StreamEvent::Done`].
    fn chat_stream(
        &self,
        req: ChatRequest,
        on_event: &mut dyn FnMut(StreamEvent),
    ) -> Result<ChatResponse, AgentError> {
        let resp = self.chat(req)?;
        if let Some(content) = resp.content() {
            if !content.is_empty() {
                on_event(StreamEvent::Delta(content.to_string()));
            }
        }
        on_event(StreamEvent::Done);
        Ok(resp)
    }
}

/// OpenAI-compatible chat-completions client (blocking).
///
/// Speaks the OpenAI `/chat/completions` protocol and therefore works with any
/// compatible provider (DeepSeek, OpenAI, Ollama, LM Studio, ...). The provider
/// is selected purely by `base_url` / `model`; there is **no** provider-specific
/// logic here.
///
/// `DeepSeekClient` is kept as a back-compat alias.
pub struct OpenAiCompatClient {
    config: AgentConfig,
    http: reqwest::blocking::Client,
}

impl OpenAiCompatClient {
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

impl LlmClient for OpenAiCompatClient {
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

    fn chat_stream(
        &self,
        req: ChatRequest,
        on_event: &mut dyn FnMut(StreamEvent),
    ) -> Result<ChatResponse, AgentError> {
        // Turn streaming on for this request only (`chat` stays non-streaming).
        let mut body = serde_json::to_value(&req)?;
        if let Some(object) = body.as_object_mut() {
            object.insert("stream".to_string(), serde_json::Value::Bool(true));
        }

        let response = self
            .http
            .post(self.config.endpoint())
            .bearer_auth(&self.config.api_key)
            .json(&body)
            .send()
            .map_err(|e| AgentError::Http(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().unwrap_or_default();
            return Err(AgentError::Api(format!(
                "HTTP {}: {}",
                status.as_u16(),
                truncate(&text, 512)
            )));
        }

        let mut acc = SseAccumulator::new();
        let mut content = String::new();
        let mut calls: Vec<PartialToolCall> = Vec::new();

        let reader = std::io::BufReader::new(response);
        for line in reader.lines() {
            let line = line.map_err(|e| AgentError::Http(e.to_string()))?;
            if let Some(payload) = acc.feed(&line) {
                handle_stream_payload(&payload, &mut content, &mut calls, on_event)?;
            }
            if acc.is_done() {
                break;
            }
        }
        // Some servers omit the final blank line.
        if let Some(payload) = acc.flush() {
            handle_stream_payload(&payload, &mut content, &mut calls, on_event)?;
        }

        on_event(StreamEvent::Done);
        Ok(build_stream_response(content, calls))
    }
}

/// One streamed `choices[]` entry.
#[derive(Debug, Deserialize)]
struct StreamChunk {
    #[serde(default)]
    choices: Vec<StreamChoice>,
}

#[derive(Debug, Deserialize)]
struct StreamChoice {
    #[serde(default)]
    delta: StreamDelta,
}

#[derive(Debug, Default, Deserialize)]
struct StreamDelta {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<StreamToolCall>>,
}

#[derive(Debug, Deserialize)]
struct StreamToolCall {
    #[serde(default)]
    index: usize,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    function: Option<StreamFunction>,
}

#[derive(Debug, Deserialize)]
struct StreamFunction {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    arguments: Option<String>,
}

/// Tool-call fragments are buffered per `index` until the stream ends.
struct PartialToolCall {
    index: usize,
    id: Option<String>,
    name: Option<String>,
    args: String,
}

fn handle_stream_payload(
    payload: &str,
    content: &mut String,
    calls: &mut Vec<PartialToolCall>,
    on_event: &mut dyn FnMut(StreamEvent),
) -> Result<(), AgentError> {
    if payload.trim().is_empty() {
        return Ok(());
    }
    let chunk: StreamChunk = serde_json::from_str(payload)?;
    for choice in chunk.choices {
        if let Some(text) = choice.delta.content {
            if !text.is_empty() {
                content.push_str(&text);
                on_event(StreamEvent::Delta(text));
            }
        }
        for tc in choice.delta.tool_calls.unwrap_or_default() {
            let index = tc.index;
            let pos = calls.iter().position(|c| c.index == index);
            let slot = match pos {
                Some(i) => i,
                None => {
                    calls.push(PartialToolCall {
                        index,
                        id: None,
                        name: None,
                        args: String::new(),
                    });
                    calls.len() - 1
                }
            };
            let entry = &mut calls[slot];

            if let Some(id) = tc.id.filter(|s| !s.is_empty()) {
                entry.id = Some(id);
            }
            let mut name_delta = None;
            let mut args_delta = String::new();
            if let Some(function) = tc.function {
                if let Some(name) = function.name.filter(|s| !s.is_empty()) {
                    entry.name = Some(name.clone());
                    name_delta = Some(name);
                }
                if let Some(args) = function.arguments.filter(|s| !s.is_empty()) {
                    entry.args.push_str(&args);
                    args_delta = args;
                }
            }
            on_event(StreamEvent::ToolCallDelta {
                index,
                id: entry.id.clone(),
                name: name_delta,
                args_delta,
            });
        }
    }
    Ok(())
}

/// Assemble a [`ChatResponse`] from the buffered stream.
fn build_stream_response(content: String, mut calls: Vec<PartialToolCall>) -> ChatResponse {
    calls.sort_by_key(|c| c.index);
    let tool_calls: Vec<ToolCall> = calls
        .into_iter()
        .filter(|c| c.name.is_some() || !c.args.is_empty())
        .map(|c| ToolCall {
            id: c.id.unwrap_or_else(|| format!("call-{}", c.index)),
            kind: "function".to_string(),
            function: FunctionCall {
                name: c.name.unwrap_or_default(),
                arguments: c.args,
            },
        })
        .collect();

    ChatResponse {
        id: None,
        model: None,
        choices: vec![Choice {
            index: Some(0),
            message: ChatMessage {
                role: "assistant".to_string(),
                content: if content.is_empty() {
                    None
                } else {
                    Some(content)
                },
                tool_calls: if tool_calls.is_empty() {
                    None
                } else {
                    Some(tool_calls)
                },
                tool_call_id: None,
            },
            finish_reason: None,
        }],
        usage: None,
    }
}

/// Back-compat alias for the v0.1 name.
pub type DeepSeekClient = OpenAiCompatClient;

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

    fn chat_stream(
        &self,
        req: ChatRequest,
        on_event: &mut dyn FnMut(StreamEvent),
    ) -> Result<ChatResponse, AgentError> {
        // Scripted behaviour is unchanged; content is merely sliced into
        // fixed-size deltas so callers can exercise streaming.
        let resp = self.chat(req)?;
        if let Some(content) = resp.content() {
            let chars: Vec<char> = content.chars().collect();
            for chunk in chars.chunks(8) {
                on_event(StreamEvent::Delta(chunk.iter().collect::<String>()));
            }
        }
        on_event(StreamEvent::Done);
        Ok(resp)
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
