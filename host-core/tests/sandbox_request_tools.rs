//! v0.9 sandbox F2c — the AI's sandbox tools land a request in the host's queue.
//!
//! The tool is the only way an agent can ask: it holds the host's
//! `SandboxRequester` gateway, not `AppState`. So this drives the real
//! `AppState::run_agent` with a scripted LLM and asserts on what the queue and the
//! event stream say afterwards. Nothing here boots a guest: the two tools touch
//! neither QEMU nor the workspace.

use agent::llm::MockLlm;
use agent::message::{ChatMessage, ChatResponse, Choice, FunctionCall, ToolCall};
use host_core::events::{RecordingEventSink, EV_SANDBOX_REQUEST};
use host_core::state::AppState;
use host_core::{EventSink, SandboxRequestStatus};
use std::path::PathBuf;
use std::sync::Arc;

fn unique_dir(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "riscdom-sandbox-request-{tag}-{}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn tool_response(id: &str, name: &str, args: serde_json::Value) -> ChatResponse {
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

fn final_response(text: &str) -> ChatResponse {
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

#[test]
#[ignore = "requires a discoverable QEMU; run with --include-ignored"]
fn the_agents_ask_lands_in_the_queue_and_on_the_stream() {
    let state = AppState::in_memory(unique_dir("ask")).expect("state");
    let script = vec![
        tool_response(
            "c1",
            "request_sandbox",
            serde_json::json!({
                "action": "switch",
                "sandbox": "blink",
                "reason": "the guest needs more memory",
            }),
        ),
        tool_response("c2", "sandbox_status", serde_json::json!({})),
        final_response("asked"),
    ];
    *state.llm_override.lock().unwrap() = Some(Arc::new(MockLlm::new(script)));
    let sink = Arc::new(RecordingEventSink::new());

    state
        .run_agent(
            sink.clone() as Arc<dyn EventSink>,
            "switch this node to blink",
        )
        .expect("the run finishes even though it only asked");

    // One request, pending, naming this host's agent and what it wants.
    let pending = state.list_sandbox_requests(Some(SandboxRequestStatus::Pending));
    assert_eq!(pending.len(), 1, "{pending:?}");
    assert_eq!(pending[0].action, "switch");
    assert_eq!(pending[0].sandbox.as_deref(), Some("blink"));
    assert_eq!(
        pending[0].reason.as_deref(),
        Some("the guest needs more memory")
    );
    assert_eq!(pending[0].requester_agent_id, state.agent_id());
    assert!(pending[0].id.starts_with("req-"), "{}", pending[0].id);

    // The ask is announced, exactly once, as `pending`.
    let frames: Vec<serde_json::Value> = sink
        .events()
        .into_iter()
        .filter(|(name, _)| name == EV_SANDBOX_REQUEST)
        .map(|(_, payload)| payload)
        .collect();
    assert_eq!(frames.len(), 1, "{frames:?}");
    assert_eq!(frames[0]["id"], serde_json::json!(pending[0].id));
    assert_eq!(frames[0]["status"], "pending");
    assert_eq!(frames[0]["action"], "switch");
    assert_eq!(frames[0]["requester"], state.agent_id());

    // Asking does not switch anything: nothing is current, the default is.
    assert_eq!(state.current_sandbox(), None);
}

#[test]
#[ignore = "requires a discoverable QEMU; run with --include-ignored"]
fn the_agents_status_tool_reports_what_runs_and_what_waits() {
    let state = AppState::in_memory(unique_dir("status")).expect("state");
    // Seed the queue the way the HTTP surface would, then let the model read it.
    state
        .request_sandbox(
            "someone-else",
            host_core::SandboxAction::Assemble,
            Some("big".into()),
            None,
            None,
            Arc::new(RecordingEventSink::new()) as Arc<dyn EventSink>,
        )
        .expect("seed");
    let script = vec![
        tool_response("c1", "sandbox_status", serde_json::json!({})),
        final_response("looked"),
    ];
    *state.llm_override.lock().unwrap() = Some(Arc::new(MockLlm::new(script)));
    let sink = Arc::new(RecordingEventSink::new());

    state
        .run_agent(sink.clone() as Arc<dyn EventSink>, "what is running?")
        .expect("run");

    // The tool result is what the model was handed; the audit chain carries it.
    let text = state
        .audit
        .lock()
        .expect("audit")
        .all()
        .expect("events")
        .into_iter()
        .filter(|event| event.event.action == "agent.tool.result")
        .map(|event| event.event.detail.to_string())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(text.contains("running:"), "{text}");
    assert!(text.contains("1 request(s) waiting"), "{text}");
    assert!(text.contains("assemble"), "{text}");
}
