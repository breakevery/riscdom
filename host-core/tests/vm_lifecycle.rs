//! Stage 20b — host-owned VM lifecycle and cross-run serial.
//!
//! Boots a real QEMU guest through the agent, then checks that the VM survives
//! the run (host-owned slot) and that serial output keeps flowing afterwards.

use agent::llm::MockLlm;
use agent::message::{ChatMessage, ChatResponse, Choice, FunctionCall, ToolCall};
use host_core::events::{RecordingEventSink, EV_SERIAL_CHUNK};
use host_core::state::AppState;
use serde_json::json;
use std::sync::Arc;

const HELLO_C: &str = include_str!("../../agent/tests/fixtures/hello.c");

fn unique_dir(tag: &str) -> std::path::PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "riscdom-vmlife-{tag}-{}-{nanos}",
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

/// write_source → compile → start_vm → read_serial → final (no `stop_vm`).
fn boot_script() -> Vec<ChatResponse> {
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
        tool_response("c4", "read_serial", json!({})),
        final_response("booted"),
    ]
}

fn state_with(tag: &str, script: Vec<ChatResponse>) -> AppState {
    let state = AppState::in_memory(unique_dir(tag)).expect("state");
    *state.llm_override.lock().unwrap() = Some(Arc::new(MockLlm::new(script)));
    state
}

#[test]
fn vm_stays_alive_across_runs_and_can_be_stopped() {
    let state = state_with("lifecycle", boot_script());
    let sink = Arc::new(RecordingEventSink::new());
    state
        .start_serial_forwarder(sink.clone() as Arc<dyn host_core::EventSink>)
        .expect("serial forwarder");

    state
        .run_agent(sink.clone() as Arc<dyn host_core::EventSink>, "boot it")
        .expect("run 1");

    assert!(
        state.vm_is_running(),
        "the VM must stay in the host slot after the run"
    );
    assert!(
        sink.count(EV_SERIAL_CHUNK) >= 1,
        "the long-lived forwarder must see the guest's first output"
    );

    // Second run: the same VM is reused, so `start_vm` must refuse.
    *state.llm_override.lock().unwrap() = Some(Arc::new(MockLlm::new(vec![
        tool_response("d1", "start_vm", json!({ "elf_path": "hello.elf" })),
        tool_response("d2", "read_serial", json!({})),
        final_response("still the same guest"),
    ])));
    state
        .run_agent(sink.clone() as Arc<dyn host_core::EventSink>, "再说一次")
        .expect("run 2");

    assert!(state.vm_is_running(), "run 2 must not drop the VM");

    let results = state
        .list_events(500, None, Some("agent.tool.result".into()))
        .expect("events");
    assert!(
        results
            .iter()
            .any(|e| e.detail.to_string().contains("already running")),
        "the second start_vm must be refused with 'already running'"
    );

    // Cross-run serial: the same channel is still feeding the UI.
    assert!(
        state.serial_buffer().contains("HELLO RISCV"),
        "serial buffer must accumulate across runs: {:?}",
        state.serial_buffer()
    );

    // Stopping clears the slot.
    state.stop_current_vm().expect("stop");
    assert!(!state.vm_is_running(), "stop must clear the host slot");
}

#[test]
fn stopping_an_empty_slot_is_a_no_op() {
    let state = state_with("empty", vec![final_response("hi")]);
    assert!(!state.vm_is_running());
    state.stop_current_vm().expect("stop is a no-op");
    assert!(!state.vm_is_running());
}
