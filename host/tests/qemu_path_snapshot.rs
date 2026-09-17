//! v0.3.1 #1 — a manual QEMU path must survive a snapshot restore.
//!
//! `AppState::resume_from_snapshot_real` builds its **own** `VMConfig`. Before
//! the fix it hard-coded `qemu_exe: None`, so a restore silently fell back to
//! auto-discovery and could boot with a different QEMU than the one the user
//! configured in *Settings → Toolchain → QEMU*.

use agent::llm::MockLlm;
use agent::message::{ChatMessage, ChatResponse, Choice, FunctionCall, ToolCall};
use host::events::RecordingEventSink;
use host::state::AppState;
use serde_json::json;
use std::path::PathBuf;
use std::sync::Arc;

const HELLO_C: &str = include_str!("../../agent/tests/fixtures/hello.c");

fn unique_dir(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "riscdom-qemupath-{tag}-{}-{nanos}",
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
    assert!(
        state.vm_is_running(),
        "the boot must leave a VM in the slot"
    );
    state
}

#[test]
fn a_manual_qemu_path_is_used_by_a_snapshot_restore() {
    let state = booted_state("manual-qemu");
    let bytes = state.save_snapshot_real("s-manual").expect("save");
    assert!(bytes > 0, "the snapshot must not be empty");

    // The configured binary exists but cannot run.
    //
    // `set_qemu_path` is not used here on purpose: it rejects a file that is not
    // runnable, and a runnable stand-in (a copy of `cmd.exe`) makes every
    // restore attempt block for the sandbox's 10 s connect timeout. Writing the
    // same field the command writes keeps this test at about a second while
    // still exercising the exact state a manual path produces.
    let fake = state.workspace_root.join("not-qemu.exe");
    std::fs::write(&fake, b"this file is not an executable").expect("write fake");
    *state.qemu_path.lock().expect("qemu path lock") = Some(fake.clone());
    assert_eq!(
        state.probe_qemu().source,
        "Manual",
        "the app must see this as a manual path"
    );

    // The restore must use the configured binary — and therefore fail.
    let err = state
        .resume_from_snapshot_real("s-manual")
        .expect_err("the restore must honour the manual path, not auto-discovery");
    let msg = err.to_string();
    println!("resume error: {msg}");
    assert!(
        msg.contains("not-qemu.exe"),
        "the error must name the configured binary: {msg}"
    );

    // Control: clearing the manual path restores the same snapshot fine.
    state.clear_qemu_path().expect("clear");
    state
        .resume_from_snapshot_real("s-manual")
        .expect("with discovery the same snapshot must restore");
    assert!(state.vm_is_running(), "the restored VM must be in the slot");
}
