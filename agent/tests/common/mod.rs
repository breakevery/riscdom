//! Shared helpers for the agent integration tests.
#![allow(dead_code)]

use agent::config::AgentConfig;
use agent::message::{ChatMessage, ChatResponse, Choice, FunctionCall, ToolCall};
use audit::{AuditSink, AuditStore, ChainStatus, SqliteAuditSink};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

/// A unique workspace directory (never deleted, so tests never collide).
pub fn unique_dir(tag: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "riscdom-agent-{tag}-{}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).expect("create workspace");
    dir
}

/// An in-memory audit sink plus a handle to inspect it.
pub fn sink() -> (Arc<Mutex<dyn AuditSink>>, Arc<Mutex<AuditStore>>) {
    let shared = Arc::new(Mutex::new(AuditStore::in_memory().expect("store")));
    let s: Arc<Mutex<dyn AuditSink>> = Arc::new(Mutex::new(SqliteAuditSink::from_shared(
        Arc::clone(&shared),
    )));
    (s, shared)
}

/// A unique temp `.db` path for a file-backed audit log.
pub fn unique_db_path(tag: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "riscdom-audit-{tag}-{}-{nanos}.db",
        std::process::id()
    ))
}

/// A **file-backed** audit sink (path, sink, shared store).
///
/// Unlike [`sink`], the log can be reopened from a fresh handle after the run,
/// which is what lets us verify the hash chain independently.
pub fn file_sink(tag: &str) -> (PathBuf, Arc<Mutex<dyn AuditSink>>, Arc<Mutex<AuditStore>>) {
    let path = unique_db_path(tag);
    let shared = Arc::new(Mutex::new(
        AuditStore::open(&path).expect("open audit store"),
    ));
    let s: Arc<Mutex<dyn AuditSink>> = Arc::new(Mutex::new(SqliteAuditSink::from_shared(
        Arc::clone(&shared),
    )));
    (path, s, shared)
}

/// Reopen `db_path` in a fresh handle, verify the whole chain and require the
/// events a real run must produce. Used by `real_api`.
pub fn assert_chain_intact(db_path: &Path) -> ChainStatus {
    let store = AuditStore::open(db_path).expect("reopen audit db");
    let status = audit::verify_chain(&store).expect("verify_chain");
    let length = match status {
        ChainStatus::Intact { length } => length,
        ChainStatus::Broken {
            ref at_id,
            ref reason,
        } => {
            panic!("audit chain broken at {at_id}: {reason}")
        }
    };
    assert!(length > 0, "expected at least one audit event");
    for action in ["agent.llm.request", "agent.tool.call", "agent.tool.result"] {
        let n = count_action(&store, action);
        assert!(n >= 1, "expected >=1 `{action}` event, found {n}");
    }
    println!("audit chain intact: {length} events");
    status
}

/// Test configuration (a clearly fake key, never used against the network).
pub fn test_config() -> AgentConfig {
    AgentConfig {
        api_key: "placeholder-key".into(),
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
