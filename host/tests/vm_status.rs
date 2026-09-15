//! Stage v0.3-4c — the VM status badge data (`vm_status` + `vm:state` payload).

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
        "riscdom-vmstatus-{tag}-{}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
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
/// Deliberately **without** `stop_vm`: since v0.3 the prompt tells the model to
/// leave the VM running, so the host keeps it in the slot after the run.
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
fn without_a_vm_the_status_is_not_running() {
    let state = state_with("none", vec![final_response("hi")]);
    let status = state.vm_status();
    println!("running={} since_ms={:?}", status.running, status.since_ms);
    assert!(!status.running);
    assert!(status.since_ms.is_none());
}

#[test]
fn booting_a_vm_sets_running_and_since_ms_then_stopping_clears_it() {
    let state = state_with("run", boot_script());
    let sink = Arc::new(RecordingEventSink::new());
    state
        .start_serial_forwarder(sink.clone() as Arc<dyn host::EventSink>)
        .expect("serial forwarder");

    let before = now_ms();
    state
        .run_agent(
            sink.clone() as Arc<dyn host::EventSink>,
            "写一个 Hello World",
        )
        .expect("run");

    let status = state.vm_status();
    println!("running={} since_ms={:?}", status.running, status.since_ms);
    assert!(status.running, "the VM must survive the run");
    let since = status.since_ms.expect("since_ms must be set while running");
    assert!(
        since >= before - 5_000 && since <= now_ms() + 5_000,
        "implausible start time: {since} (run started around {before})"
    );

    // The `vm:state` event carries the new fields and keeps the old one.
    let vm_events: Vec<serde_json::Value> = sink
        .events()
        .into_iter()
        .filter(|(event, _)| event == "vm:state")
        .map(|(_, payload)| payload)
        .collect();
    println!("vm:state payloads: {vm_events:?}");
    assert!(
        vm_events
            .iter()
            .any(|p| p.get("state") == Some(&json!("running"))),
        "the pre-existing `state` field must stay: {vm_events:?}"
    );
    assert!(
        vm_events
            .iter()
            .any(|p| p.get("running") == Some(&json!(true))
                && p.get("since_ms").map(|v| !v.is_null()).unwrap_or(false)),
        "running + since_ms must be present: {vm_events:?}"
    );

    // Stopping the VM clears both fields.
    state.stop_current_vm().expect("stop");
    let status = state.vm_status();
    println!(
        "after stop: running={} since_ms={:?}",
        status.running, status.since_ms
    );
    assert!(!status.running);
    assert!(status.since_ms.is_none(), "{:?}", status.since_ms);
}

#[test]
fn the_status_follows_a_run_that_stops_the_vm_explicitly() {
    // The user asked for it: the script ends with stop_vm.
    let script = {
        let mut script = boot_script();
        let last = script.pop().expect("final");
        script.push(tool_response("c5", "stop_vm", json!({})));
        script.push(last);
        script
    };
    let state = state_with("explicit-stop", script);
    let sink = Arc::new(RecordingEventSink::new());
    state
        .run_agent(sink.clone() as Arc<dyn host::EventSink>, "跑完就停掉 VM")
        .expect("run");

    let status = state.vm_status();
    println!("running={} since_ms={:?}", status.running, status.since_ms);
    assert!(!status.running, "an explicit stop_vm must clear the status");
    assert!(status.since_ms.is_none());
}
