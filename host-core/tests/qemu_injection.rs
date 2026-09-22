//! Stage v0.3-5b-1c — the host injects its manual QEMU path into the agent loop.
//!
//! Differential proof: the same script runs twice. Without a manual path the
//! sandbox discovers QEMU and the guest boots; with a manual path pointing at a
//! runnable-but-not-QEMU binary the VM never starts. Only an **injected** path
//! can explain the second outcome (`cmd.exe` answers `--version`, so the
//! pre-check accepts it).

use agent::llm::MockLlm;
use agent::message::{ChatMessage, ChatResponse, Choice, FunctionCall, ToolCall};
use host_core::events::RecordingEventSink;
use host_core::state::AppState;
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
        "riscdom-qemuinj-{tag}-{}-{nanos}",
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
        final_response("done"),
    ]
}

fn state_running(workspace: PathBuf) -> (AppState, Arc<RecordingEventSink>) {
    let state = AppState::in_memory(workspace).expect("state");
    *state.llm_override.lock().unwrap() = Some(Arc::new(MockLlm::new(boot_script())));
    let sink = Arc::new(RecordingEventSink::new());
    (state, sink)
}

/// Did the sandbox actually start a VM during that run?
fn vm_started(state: &AppState) -> bool {
    state
        .list_events(
            500,
            Some("sandbox".to_string()),
            Some("vm.start".to_string()),
        )
        .map(|events| !events.is_empty())
        .unwrap_or(false)
}

#[test]
fn a_manual_qemu_path_reaches_the_agent_loop() {
    // Control: no manual path → the sandbox discovers QEMU → the guest boots.
    let (control, sink) = state_running(unique_dir("control"));
    control
        .run_agent(sink as Arc<dyn host_core::EventSink>, "boot it")
        .expect("control run");
    assert!(
        vm_started(&control),
        "the discovered QEMU must boot the guest (control group)"
    );

    // Injected: a runnable binary that is **not** QEMU (`cmd.exe` answers
    // `--version`, so the pre-check accepts it). The VM cannot start, and the
    // only way that can happen is if the injected path was actually used.
    let injected_dir = unique_dir("injected");
    let fake = injected_dir.join("riscdom-not-qemu.exe");
    std::fs::copy(r"C:\Windows\System32\cmd.exe", &fake).expect("copy a runnable binary");

    let (state, sink) = state_running(injected_dir);
    state
        .set_qemu_path(&fake.display().to_string())
        .expect("set manual qemu path");
    assert_eq!(state.probe_qemu().source, "Manual");

    state
        .run_agent(sink as Arc<dyn host_core::EventSink>, "boot it")
        .expect("the run itself completes (tool failures are reported to the model)");

    assert!(
        !vm_started(&state),
        "the injected non-QEMU binary must not boot a guest — if it did, the path was ignored"
    );

    // And the failing start is visible in the audit trail.
    let results = state
        .list_events(500, None, Some("agent.tool.result".to_string()))
        .expect("events")
        .into_iter()
        .filter_map(|e| {
            e.detail
                .get("result")
                .and_then(|v| v.as_str())
                .map(str::to_string)
        })
        .collect::<Vec<_>>();
    println!("tool results: {results:?}");
    assert!(
        results
            .iter()
            .any(|r| r.contains("qemu") || r.contains("failed to start")),
        "the failing start must be visible in the audit trail: {results:?}"
    );
}
