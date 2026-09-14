//! Chat message / request / response types (DeepSeek-compatible).

use serde::{Deserialize, Serialize};

/// A single chat message.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatMessage {
    /// `"system"` / `"user"` / `"assistant"` / `"tool"`.
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

impl ChatMessage {
    /// A plain `role`/`content` message.
    pub fn text(role: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: role.into(),
            content: Some(content.into()),
            tool_calls: None,
            tool_call_id: None,
        }
    }

    /// A `tool` result message.
    pub fn tool_result(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: "tool".into(),
            content: Some(content.into()),
            tool_calls: None,
            tool_call_id: Some(tool_call_id.into()),
        }
    }
}

/// A tool call requested by the model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String, // "function"
    pub function: FunctionCall,
}

/// The function name + raw JSON arguments of a [`ToolCall`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FunctionCall {
    pub name: String,
    /// JSON-encoded arguments, exactly as the model produced them.
    pub arguments: String,
}

/// A chat-completions request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
}

/// A chat-completions response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResponse {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub choices: Vec<Choice>,
    #[serde(default)]
    pub usage: Option<Usage>,
}

impl ChatResponse {
    /// The first choice's message, if any.
    pub fn first_message(&self) -> Option<&ChatMessage> {
        self.choices.first().map(|c| &c.message)
    }

    /// Tool calls from the first choice, if any.
    pub fn tool_calls(&self) -> Option<&Vec<ToolCall>> {
        self.first_message().and_then(|m| m.tool_calls.as_ref())
    }

    /// Text content from the first choice, if any.
    pub fn content(&self) -> Option<&str> {
        self.first_message().and_then(|m| m.content.as_deref())
    }
}

/// One completion choice.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Choice {
    #[serde(default)]
    pub index: Option<u32>,
    pub message: ChatMessage,
    #[serde(default)]
    pub finish_reason: Option<String>,
}

/// Token usage reported by the API.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Usage {
    #[serde(default)]
    pub prompt_tokens: Option<u64>,
    #[serde(default)]
    pub completion_tokens: Option<u64>,
    #[serde(default)]
    pub total_tokens: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_message_serializes_minimal() {
        let m = ChatMessage::text("user", "hi");
        let v = serde_json::to_value(&m).unwrap();
        assert_eq!(v["role"], "user");
        assert_eq!(v["content"], "hi");
        // Optional fields are omitted.
        assert!(v.get("tool_calls").is_none());
        assert!(v.get("tool_call_id").is_none());
    }

    #[test]
    fn tool_call_field_names_match_api() {
        let m = ChatMessage {
            role: "assistant".into(),
            content: None,
            tool_calls: Some(vec![ToolCall {
                id: "call_1".into(),
                kind: "function".into(),
                function: FunctionCall {
                    name: "write_source".into(),
                    arguments: "{\"path\":\"a.c\"}".into(),
                },
            }]),
            tool_call_id: None,
        };
        let v = serde_json::to_value(&m).unwrap();
        let call = &v["tool_calls"][0];
        assert_eq!(call["id"], "call_1");
        assert_eq!(call["type"], "function");
        assert_eq!(call["function"]["name"], "write_source");
        assert_eq!(call["function"]["arguments"], "{\"path\":\"a.c\"}");
    }

    #[test]
    fn tool_result_message_roundtrip() {
        let m = ChatMessage::tool_result("call_1", "ok");
        let v = serde_json::to_value(&m).unwrap();
        assert_eq!(v["role"], "tool");
        assert_eq!(v["tool_call_id"], "call_1");
        let back: ChatMessage = serde_json::from_value(v).unwrap();
        assert_eq!(back, m);
    }

    #[test]
    fn response_parses_typical_payload() {
        let raw = r#"{
            "id": "resp-1",
            "model": "deepseek-chat",
            "choices": [{
                "index": 0,
                "message": {"role": "assistant", "content": "hello"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 5, "completion_tokens": 2, "total_tokens": 7}
        }"#;
        let resp: ChatResponse = serde_json::from_str(raw).unwrap();
        assert_eq!(resp.content(), Some("hello"));
        assert_eq!(resp.usage.as_ref().unwrap().total_tokens, Some(7));
        assert!(resp.tool_calls().is_none());
    }
}
