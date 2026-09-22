//! Stage 20a — external VM slot (`AgentLoop::with_vm`).

mod common;

use agent::llm::MockLlm;
use agent::message::{ChatMessage, ChatResponse, Choice, FunctionCall, ToolCall};
use agent::policy::WorkspacePolicy;
use agent::prompt::build_system_prompt;
use agent::{AgentLoop, AgentOutcome};
use common::{constitution_path, sink, test_config, unique_dir};
use sandbox::vm::RiscVVirtualMachine;
use serde_json::json;
use std::sync::{Arc, Mutex};

const HELLO_C: &str = include_str!("fixtures/hello.c");

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
        final_response("done"),
    ]
}

fn loop_with_slot(
    script: Vec<ChatResponse>,
    slot: Arc<Mutex<Option<RiscVVirtualMachine>>>,
) -> AgentLoop {
    let policy = WorkspacePolicy::new(unique_dir("vminject"));
    let (audit, _shared) = sink();
    let system = build_system_prompt(&constitution_path()).expect("system prompt");
    AgentLoop::with_vm(
        Box::new(MockLlm::new(script)),
        test_config(),
        policy,
        audit,
        slot,
        system,
        agent::next_agent_id(),
    )
    .expect("agent loop")
}

#[test]
fn vm_survives_the_run_in_the_external_slot() {
    let slot: Arc<Mutex<Option<RiscVVirtualMachine>>> = Arc::new(Mutex::new(None));
    let mut agent = loop_with_slot(boot_script(), Arc::clone(&slot));

    let outcome = agent.run("boot it").expect("run");
    assert!(matches!(outcome, AgentOutcome::Final { .. }), "{outcome:?}");

    assert!(
        slot.lock().expect("lock").is_some(),
        "the VM must stay in the external slot after the run"
    );
}

#[test]
fn a_second_run_reuses_the_running_vm_and_stop_clears_the_slot() {
    let slot: Arc<Mutex<Option<RiscVVirtualMachine>>> = Arc::new(Mutex::new(None));

    // First run boots the VM and leaves it in the slot.
    let mut first = loop_with_slot(boot_script(), Arc::clone(&slot));
    first.run("boot it").expect("run");
    assert!(slot.lock().expect("lock").is_some());

    // Second run: start_vm must refuse rather than boot a second guest.
    let second_script = vec![
        tool_response("d1", "start_vm", json!({ "elf_path": "hello.elf" })),
        final_response("stopped"),
    ];
    let (audit, shared) = sink();
    let policy = WorkspacePolicy::new(unique_dir("vminject2"));
    let system = build_system_prompt(&constitution_path()).expect("system prompt");
    let mut second = AgentLoop::with_vm(
        Box::new(MockLlm::new(second_script)),
        test_config(),
        policy,
        audit,
        Arc::clone(&slot),
        system,
        agent::next_agent_id(),
    )
    .expect("agent loop");
    second.run("再次启动").expect("run");

    let results: Vec<String> = shared
        .lock()
        .unwrap()
        .all()
        .unwrap()
        .into_iter()
        .filter(|e| e.event.action == "agent.tool.result")
        .filter_map(|e| {
            e.event
                .detail
                .get("result")
                .and_then(|v| v.as_str())
                .map(String::from)
        })
        .collect();
    assert!(
        results
            .iter()
            .any(|r| r.contains("already") && r.contains("running")),
        "expected an 'already running' refusal, got {results:?}"
    );

    // Now stop it through a third run and check the slot is cleared.
    let stop_script = vec![
        tool_response("e1", "stop_vm", json!({})),
        final_response("stopped"),
    ];
    let mut third = loop_with_slot_fresh(stop_script, Arc::clone(&slot));
    third.run("stop it").expect("run");
    assert!(
        slot.lock().expect("lock").is_none(),
        "stop_vm must clear the external slot"
    );
}

/// Same as [`loop_with_slot`] but without extra helpers (kept separate so the
/// intent of each step above is obvious).
fn loop_with_slot_fresh(
    script: Vec<ChatResponse>,
    slot: Arc<Mutex<Option<RiscVVirtualMachine>>>,
) -> AgentLoop {
    loop_with_slot(script, slot)
}

#[test]
fn without_injection_the_loop_owns_its_vm() {
    let policy = WorkspacePolicy::new(unique_dir("vminject-none"));
    let (audit, _shared) = sink();
    let system = build_system_prompt(&constitution_path()).expect("system prompt");
    let mut agent = AgentLoop::new(
        Box::new(MockLlm::new(boot_script())),
        test_config(),
        policy,
        audit,
        system,
        agent::next_agent_id(),
    )
    .expect("agent loop");

    let outcome = agent.run("boot it").expect("run");
    assert!(matches!(outcome, AgentOutcome::Final { .. }), "{outcome:?}");
}
