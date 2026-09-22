//! Stage 5c — end-to-end mock run: write → compile → boot → read serial → stop.

mod common;

use agent::llm::MockLlm;
use agent::policy::WorkspacePolicy;
use agent::prompt::build_system_prompt;
use agent::{AgentLoop, AgentOutcome};
use audit::{verify_chain, ChainStatus};
use common::{constitution_path, count_action, sink, test_config, tool_response, unique_dir};
use serde_json::json;
use std::sync::Arc;

#[test]
fn e2e_mock_full_cycle() {
    let root = unique_dir("e2e");
    let policy = WorkspacePolicy::new(root.clone());
    let (audit, shared) = sink();
    let hello = include_str!("fixtures/hello.c");

    // The scripted model: 5 tool calls, then a final answer.
    let script = vec![
        tool_response(
            "c1",
            "write_source",
            json!({"path": "hello.c", "content": hello}),
        ),
        tool_response(
            "c2",
            "compile",
            json!({"source_path": "hello.c", "output_elf": "hello.elf"}),
        ),
        tool_response("c3", "start_vm", json!({"elf_path": "hello.elf"})),
        tool_response("c4", "read_serial", json!({})),
        tool_response("c5", "stop_vm", json!({})),
        common::final_response("done"),
    ];

    let system = build_system_prompt(&constitution_path()).expect("system prompt");
    let mut agent = AgentLoop::new(
        Box::new(MockLlm::new(script)),
        test_config(),
        policy,
        Arc::clone(&audit),
        system,
        agent::next_agent_id(),
    )
    .expect("agent loop");

    let outcome = agent
        .run("写一个 RISC-V 裸机 Hello World 并运行")
        .expect("run");
    match outcome {
        AgentOutcome::Final {
            content,
            iterations,
        } => {
            assert_eq!(content, "done");
            assert_eq!(iterations, 6, "six LLM round-trips expected");
        }
        other => panic!("expected Final, got {other:?}"),
    }

    // Workspace artifacts.
    assert!(root.join("hello.c").exists(), "hello.c missing");
    assert!(root.join("hello.elf").exists(), "hello.elf missing");

    let store = shared.lock().expect("lock store");

    // Serial output captured by read_serial.
    let serial = common::tool_result_text(&store);
    println!("--- captured serial output ---\n{}", serial.trim_end());
    println!("--- audit actions ---");
    for action in [
        "agent.user.input",
        "agent.llm.request",
        "agent.llm.response",
        "agent.tool.call",
        "agent.tool.result",
        "agent.compile.start",
        "agent.compile.result",
    ] {
        println!("  {:<22} x{}", action, count_action(&store, action));
    }
    assert!(
        serial.contains("HELLO RISCV"),
        "serial output not captured: {serial:?}"
    );

    // Audit coverage. The script has 6 LLM round-trips and 5 tool calls.
    assert_eq!(count_action(&store, "agent.llm.request"), 6);
    assert_eq!(count_action(&store, "agent.llm.response"), 6);
    assert_eq!(count_action(&store, "agent.tool.call"), 5);
    assert_eq!(count_action(&store, "agent.tool.result"), 5);

    // Chain intact.
    let status = verify_chain(&store).expect("verify");
    println!("--- verify_chain ---\n  {status:?}");
    assert!(
        matches!(status, ChainStatus::Intact { .. }),
        "chain broken: {status:?}"
    );
}
