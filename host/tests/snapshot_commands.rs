//! Stage 20c — real snapshot save/resume through the host commands.
//!
//! Boots a real QEMU guest (host-owned VM), snapshots it, and restores it.

use agent::llm::MockLlm;
use agent::message::{ChatMessage, ChatResponse, Choice, FunctionCall, ToolCall};
use host::events::RecordingEventSink;
use host::state::AppState;
use serde_json::json;
use std::sync::Arc;

const HELLO_C: &str = include_str!("../../agent/tests/fixtures/hello.c");

fn unique_dir(tag: &str) -> std::path::PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "riscdom-snapcmd-{tag}-{}-{nanos}",
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

fn booted_state(tag: &str) -> AppState {
    let state = AppState::in_memory(unique_dir(tag)).expect("state");
    *state.llm_override.lock().unwrap() = Some(Arc::new(MockLlm::new(boot_script())));
    let sink = Arc::new(RecordingEventSink::new());
    state
        .start_serial_forwarder(sink.clone() as Arc<dyn host::EventSink>)
        .expect("forwarder");
    state
        .run_agent(sink as Arc<dyn host::EventSink>, "boot it")
        .expect("run");
    assert!(state.vm_is_running());
    state
}

#[test]
fn saving_without_a_vm_is_an_explicit_error() {
    let state = AppState::in_memory(unique_dir("novm")).expect("state");
    let err = state
        .save_snapshot_real("nope")
        .expect_err("must fail without a VM");
    assert!(
        err.to_string().contains("no running vm"),
        "unexpected error: {err}"
    );
}

#[test]
fn save_then_resume_round_trip() {
    let state = booted_state("roundtrip");

    let bytes = state.save_snapshot_real("s20c").expect("save");
    assert!(bytes > 0, "the .mig snapshot must not be empty");
    let path = state.workspace_root.join(".riscdom/snapshots/s20c.mig");
    assert!(path.is_file(), "expected {path:?}");

    let listed = state.list_snapshots().expect("list");
    let meta = listed.iter().find(|s| s.name == "s20c").expect("listed");
    assert_eq!(meta.mode, "tcp-relay");
    assert_eq!(meta.size_bytes, bytes);

    // Resume: stops the current VM and restores from the stream.
    state.resume_from_snapshot_real("s20c").expect("resume");
    assert!(state.vm_is_running(), "the restored VM must be in the slot");

    // A missing snapshot is reported, not silently ignored.
    let err = state
        .resume_from_snapshot_real("missing")
        .expect_err("must fail");
    assert!(err.to_string().contains("snapshot not found"), "{err}");
}

#[test]
fn invalid_and_traversal_names_are_rejected() {
    let state = booted_state("byname");
    for bad in ["", "../escape", "a/b", "x.mig"] {
        assert!(
            state.save_snapshot_real(bad).is_err(),
            "{bad:?} must be rejected"
        );
    }
}
