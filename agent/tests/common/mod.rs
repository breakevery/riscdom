//! Shared helpers for the agent integration tests.
#![allow(dead_code)]

use agent::config::AgentConfig;
use agent::message::{ChatMessage, ChatResponse, Choice, FunctionCall, ToolCall};
use audit::{AuditSink, AuditStore, SqliteAuditSink};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

/// A unique workspace directory (never deleted, so tests never collide).
pub fn unique_dir(tag: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("riscdom-agent-{tag}-{}-{nanos}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create workspace");
    dir
}

/// An in-memory audit sink plus a handle to inspect it.
pub fn sink() -> (Arc<Mutex<dyn AuditSink>>, Arc<Mutex<AuditStore>>) {
    let shared = Arc::new(Mutex::new(AuditStore::in_memory().expect("store")));
    let s: Arc<Mutex<dyn AuditSink>> =
        Arc::new(Mutex::new(SqliteAuditSink::from_shared(Arc::clone(&shared))));
    (s, shared)
}

/// Test configuration (a clearly fake key, never used against the network).
pub fn test_config() -> AgentConfig {
    AgentConfig {
        api_key: "sk-test-not-a-real-key-0000".into(),
        base_url: "https://api.deepseek.com".into(),
        model: "mock".into(),
        provider_id: "deepseek".into(),
        max_iterations: 10,
        request_timeout_secs: 30,
    }
}

/// Path to the repo constitution (`AGENTS.md`).
pub fn constitution_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("AGENTS.md")
}

/// A response that requests exactly one tool call.
pub fn tool_response(id: &str, name: &str, args: serde_json::Value) -> ChatResponse {
    ChatResponse {
        id: Some(format!("resp-{id}")),
        model: Some("mock".into()),
        choices: vec![Choice {
            index: Some(0),
            message: ChatMessage {
                role: "assistant".into(),
                content: None,
                tool_calls: Some(vec![ToolCall {
                    id: id.to_string(),
                    kind: "function".into(),
                    function: FunctionCall {
                        name: name.to_string(),
                        arguments: args.to_string(),
                    },
                }]),
                tool_call_id: None,
            },
            finish_reason: Some("tool_calls".into()),
        }],
        usage: None,
    }
}

/// A response with final assistant text and no tool calls.
pub fn final_response(text: &str) -> ChatResponse {
    ChatResponse {
        id: Some("resp-final".into()),
        model: Some("mock".into()),
        choices: vec![Choice {
            index: Some(0),
            message: ChatMessage::text("assistant", text),
            finish_reason: Some("stop".into()),
        }],
        usage: None,
    }
}

/// Count events with a given action.
pub fn count_action(store: &AuditStore, action: &str) -> usize {
    store
        .all()
        .expect("all")
        .iter()
        .filter(|e| e.event.action == action)
        .count()
}

/// All tool-result texts, joined.
pub fn tool_result_text(store: &AuditStore) -> String {
    store
        .all()
        .expect("all")
        .into_iter()
        .filter(|e| e.event.action == "agent.tool.result")
        .filter_map(|e| {
            e.event
                .detail
                .get("result")
                .and_then(|v| v.as_str())
                .map(str::to_string)
        })
        .collect::<Vec<_>>()
        .join("\n")
}
