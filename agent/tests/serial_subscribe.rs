//! Stage 15b — `AgentLoop::subscribe_serial`.

mod common;

use agent::llm::MockLlm;
use agent::message::{ChatMessage, ChatResponse, Choice, FunctionCall, ToolCall};
use agent::policy::WorkspacePolicy;
use agent::prompt::build_system_prompt;
use agent::{AgentLoop, AgentOutcome};
use common::{constitution_path, sink, test_config, unique_dir};
use serde_json::json;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

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

fn script() -> Vec<ChatResponse> {
    let one_run = || {
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
            tool_response("c5", "stop_vm", json!({})),
            final_response("done"),
        ]
    };
    // Two full runs, so a test can run the loop twice.
    let mut all = one_run();
    all.extend(one_run());
    all
}

fn agent_loop(tag: &str) -> AgentLoop {
    let policy = WorkspacePolicy::new(unique_dir(tag));
    let (audit, _shared) = sink();
    let system = build_system_prompt(&constitution_path()).expect("system prompt");
    AgentLoop::new(
        Box::new(MockLlm::new(script())),
        test_config(),
        policy,
        audit,
        system,
        agent::next_agent_id(),
    )
    .expect("agent loop")
}

/// Drain the channel until `needle` shows up (or the timeout elapses).
fn collect_until(rx: &Receiver<Vec<u8>>, needle: &[u8], timeout: Duration) -> Vec<u8> {
    let mut out = Vec::new();
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(chunk) => {
                out.extend_from_slice(&chunk);
                if out.windows(needle.len()).any(|w| w == needle) {
                    break;
                }
            }
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
    out
}

#[test]
#[ignore = "requires a QEMU guest and a RISC-V GCC; run with --include-ignored"]
fn subscriber_receives_live_serial_output() {
    let mut agent = agent_loop("sub-one");
    let rx = agent.subscribe_serial();

    let outcome = agent.run("写一个 Hello World").expect("run");
    assert!(matches!(outcome, AgentOutcome::Final { .. }), "{outcome:?}");

    let data = collect_until(&rx, b"HELLO RISCV", Duration::from_secs(10));
    let text = String::from_utf8_lossy(&data);
    assert!(
        text.contains("HELLO RISCV"),
        "subscriber did not receive the banner: {text:?}"
    );
}

#[test]
#[ignore = "requires a QEMU guest and a RISC-V GCC; run with --include-ignored"]
fn multiple_subscribers_receive_the_same_data() {
    let mut agent = agent_loop("sub-two");
    let rx1 = agent.subscribe_serial();
    let rx2 = agent.subscribe_serial();

    agent.run("写一个 Hello World").expect("run");

    let first = collect_until(&rx1, b"HELLO RISCV", Duration::from_secs(10));
    let second = collect_until(&rx2, b"HELLO RISCV", Duration::from_secs(10));

    assert!(String::from_utf8_lossy(&first).contains("HELLO RISCV"));
    assert!(String::from_utf8_lossy(&second).contains("HELLO RISCV"));
    assert_eq!(first, second, "subscribers must see identical bytes");
}

#[test]
#[ignore = "requires a QEMU guest and a RISC-V GCC; run with --include-ignored"]
fn dropping_a_receiver_does_not_break_the_loop() {
    let mut agent = agent_loop("sub-drop");
    let rx = agent.subscribe_serial();
    drop(rx); // closed receiver -> the fan-out must skip it without panicking

    let outcome = agent.run("写一个 Hello World").expect("run");
    assert!(
        matches!(outcome, AgentOutcome::Final { .. }),
        "run should still finish: {outcome:?}"
    );

    // A subscriber added afterwards still works (second scripted run).
    let rx2 = agent.subscribe_serial();
    let outcome2 = agent.run("再来一次").expect("second run");
    assert!(
        matches!(outcome2, AgentOutcome::Final { .. }),
        "second run should finish: {outcome2:?}"
    );
    let data = collect_until(&rx2, b"HELLO RISCV", Duration::from_secs(10));
    assert!(
        String::from_utf8_lossy(&data).contains("HELLO RISCV"),
        "later subscriber got nothing"
    );
}
