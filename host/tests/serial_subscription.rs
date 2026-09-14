//! Stage 15c — `serial:chunk` now comes from the sandbox push stream.
//!
//! Run with `--ignored` (it boots a real QEMU guest):
//!
//! ```text
//! cargo test -p host --test serial_subscription -- --ignored --nocapture
//! ```

use agent::llm::MockLlm;
use agent::message::{ChatMessage, ChatResponse, Choice, FunctionCall, ToolCall};
use host::events::{RecordingEventSink, EV_SERIAL_CHUNK};
use host::state::AppState;
use serde_json::json;
use std::sync::Arc;

const HELLO_C: &str = include_str!("../../agent/tests/fixtures/hello.c");

fn unique_dir(tag: &str) -> std::path::PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir =
        std::env::temp_dir().join(format!("riscdom-sub-{tag}-{}-{nanos}", std::process::id()));
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

fn prefix() -> Vec<ChatResponse> {
    vec![
        tool_response(
            "c1",
            "write_source",
            json!({ "path": "hello.c", "content": HELLO_C }),
        ),
        tool_response(
            "c2",
            "compile",
            json!({ "source_path": "hello.c", "output_elf": "hello.elf" }),
        ),
        tool_response("c3", "start_vm", json!({ "elf_path": "hello.elf" })),
    ]
}

fn run_with(script: Vec<ChatResponse>, tag: &str) -> Arc<RecordingEventSink> {
    let state = AppState::in_memory(unique_dir(tag)).expect("state");
    *state.llm_override.lock().unwrap() = Some(Arc::new(MockLlm::new(script)));
    let sink = Arc::new(RecordingEventSink::new());
    state
        .start_serial_forwarder(sink.clone() as Arc<dyn host::EventSink>)
        .expect("serial forwarder");
    state
        .run_agent(
            sink.clone() as Arc<dyn host::EventSink>,
            "写一个 Hello World",
        )
        .expect("run_agent");
    sink
}

#[test]
#[ignore = "runs a real QEMU guest; run with --ignored"]
fn serial_chunk_arrives_from_push_stream() {
    let mut script = prefix();
    script.push(tool_response("c4", "read_serial", json!({})));
    script.push(tool_response("c5", "stop_vm", json!({})));
    script.push(final_response("done"));

    let sink = run_with(script, "push");

    let serial = sink.serial_text();
    println!(
        "--- serial:chunk (with read_serial) ---\n{}",
        serial.trim_end()
    );
    assert!(
        serial.contains("HELLO RISCV"),
        "serial:chunk missing banner: {serial:?}"
    );
    assert!(sink.count(EV_SERIAL_CHUNK) >= 1);
}

#[test]
#[ignore = "runs a real QEMU guest; run with --ignored"]
fn serial_chunk_no_longer_depends_on_the_audit_log() {
    // No `read_serial` at all: if the chunks still arrive, the data cannot have
    // come from the audit-derived path (which no longer exists).
    let mut script = prefix();
    script.push(final_response("done"));

    let sink = run_with(script, "push-noread");

    let serial = sink.serial_text();
    println!(
        "--- serial:chunk (no read_serial) ---\n{}",
        serial.trim_end()
    );
    assert!(
        serial.contains("HELLO RISCV"),
        "push stream should not depend on read_serial: {serial:?}"
    );
}
