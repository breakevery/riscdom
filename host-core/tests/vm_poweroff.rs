//! v0.3.1 #3 — a QEMU process that has exited must not be reported as running.
//!
//! The VM handle stays in `AppState::vm_slot` after the guest powers itself off,
//! so a slot-only check keeps the top-bar badge on "VM 运行中" forever. The
//! fixture writes the SiFive finisher value, which makes QEMU shut down — no
//! external process killing, no Windows-only helper.

use agent::llm::MockLlm;
use agent::message::{ChatMessage, ChatResponse, Choice, FunctionCall, ToolCall};
use host_core::events::RecordingEventSink;
use host_core::state::AppState;
use serde_json::json;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

const POWEROFF_C: &str = include_str!("../../agent/tests/fixtures/poweroff.c");

fn unique_dir(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "riscdom-poweroff-{tag}-{}-{nanos}",
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

/// write_source → compile → start_vm → read_serial → final.
///
/// No `stop_vm`: the guest ends the run itself.
fn poweroff_script() -> Vec<ChatResponse> {
    vec![
        tool_response(
            "c1",
            "write_source",
            json!({ "path": "poweroff.c", "content": POWEROFF_C }),
        ),
        tool_response(
            "c2",
            "compile",
            json!({ "source_path": "poweroff.c", "output_elf": "poweroff.elf" }),
        ),
        tool_response("c3", "start_vm", json!({ "elf_path": "poweroff.elf" })),
        tool_response("c4", "read_serial", json!({})),
        final_response("the guest powered off"),
    ]
}

#[test]
#[ignore = "requires a QEMU guest and a RISC-V GCC; run with --include-ignored"]
fn a_guest_that_powered_off_is_not_reported_as_running() {
    let state = AppState::in_memory(unique_dir("halt")).expect("state");
    *state.llm_override.lock().unwrap() = Some(Arc::new(MockLlm::new(poweroff_script())));
    let sink = Arc::new(RecordingEventSink::new());
    state
        .start_serial_forwarder(sink.clone() as Arc<dyn host_core::EventSink>)
        .expect("forwarder");
    state
        .run_agent(sink as Arc<dyn host_core::EventSink>, "boot and halt")
        .expect("run");

    // The banner proves the guest really ran and reached the finisher write.
    let serial: String = state
        .list_events(500, None, Some("agent.tool.result".to_string()))
        .expect("events")
        .iter()
        .filter_map(|e| e.detail.get("result").and_then(|v| v.as_str()))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        serial.contains("BYE RISCV"),
        "the guest must have reached its finisher write: {serial:?}"
    );

    // QEMU may need a moment to leave after the request.
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut running = state.vm_is_running();
    while running && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
        running = state.vm_is_running();
    }

    let status = state.vm_status();
    println!("running={running} since_ms={:?}", status.since_ms);
    assert!(
        !running,
        "a halted QEMU must not be reported as running (the slot still holds a dead handle)"
    );
    assert!(!status.running, "the badge must say stopped: {status:?}");
    assert!(
        status.since_ms.is_none(),
        "the start time must be cleared with the dead handle: {:?}",
        status.since_ms
    );
}
