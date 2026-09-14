//! Stage 6c — UI-shaped end-to-end test with a mock LLM (ignored by default).
//!
//! NOTE: the task listed this as `agent/tests/e2e_ui.rs`, but it exercises
//! `host::commands::run_agent` + host events. Putting it under `agent/tests`
//! would require an `agent → host` dev-dependency, i.e. a dependency cycle
//! (host already depends on agent). It therefore lives in `host/tests`.
//!
//! Manual run:
//!
//! ```text
//! cargo test -p host -- --ignored --nocapture
//! ```
//!
//! Requires QEMU and the RISC-V toolchain (same as the agent e2e test).

use agent::llm::MockLlm;
use agent::message::{ChatMessage, ChatResponse, Choice, FunctionCall, ToolCall};
use host::events::{RecordingEventSink, EV_AGENT_FINAL, EV_AGENT_TOOL_CALL, EV_SERIAL_CHUNK};
use host::state::AppState;
use host::ChainStatusView;
use std::path::PathBuf;
use std::sync::Arc;

const HELLO_C: &str = include_str!("../../agent/tests/fixtures/hello.c");

fn unique_dir(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("riscdom-ui-{tag}-{}-{nanos}", std::process::id()));
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
#[ignore = "runs a real QEMU guest; run with --ignored"]
fn e2e_ui_mock_run_emits_final_and_serial() {
    let ws = unique_dir("e2e");
    let state = AppState::in_memory(&ws).expect("state");

    let script = vec![
        tool_response(
            "c1",
            "write_source",
            serde_json::json!({ "path": "hello.c", "content": HELLO_C }),
        ),
        tool_response(
            "c2",
            "compile",
            serde_json::json!({ "source_path": "hello.c", "output_elf": "hello.elf" }),
        ),
        tool_response(
            "c3",
            "start_vm",
            serde_json::json!({ "elf_path": "hello.elf" }),
        ),
        tool_response("c4", "read_serial", serde_json::json!({})),
        tool_response("c5", "stop_vm", serde_json::json!({})),
        final_response("done"),
    ];
    *state.llm_override.lock().unwrap() = Some(Arc::new(MockLlm::new(script)));

    let sink = Arc::new(RecordingEventSink::new());
    let outcome = state
        .run_agent(sink.clone() as Arc<dyn host::EventSink>, "写一个 RISC-V 裸机 Hello World")
        .expect("run_agent");

    println!("outcome: {outcome:?}");
    println!("--- serial:chunk events ---");
    println!("{}", sink.serial_text());
    println!("--- event counts ---");
    println!("  agent:final     x{}", sink.count(EV_AGENT_FINAL));
    println!("  agent:tool_call x{}", sink.count(EV_AGENT_TOOL_CALL));
    println!("  serial:chunk    x{}", sink.count(EV_SERIAL_CHUNK));

    assert_eq!(outcome.kind, "final");
    assert_eq!(outcome.content.as_deref(), Some("done"));
    assert_eq!(sink.count(EV_AGENT_FINAL), 1, "agent:final must arrive once");
    assert!(
        sink.serial_text().contains("HELLO RISCV"),
        "serial:chunk must contain the guest banner, got: {:?}",
        sink.serial_text()
    );
    assert!(ws.join("hello.c").exists());
    assert!(ws.join("hello.elf").exists());

    let status = state.audit_status().expect("audit status");
    println!("--- verify_chain ---\n  {:?}", status.chain);
    assert!(
        matches!(status.chain, ChainStatusView::Intact { .. }),
        "chain not intact: {:?}",
        status.chain
    );
}
