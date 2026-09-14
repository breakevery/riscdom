//! Stage 5c — real DeepSeek end-to-end test (ignored by default).
//!
//! Run manually:
//!
//! ```text
//! set DEEPSEEK_API_KEY=sk-...
//! cargo test -p agent -- --ignored --nocapture
//! ```
//!
//! Requires network access, a valid key, QEMU and the RISC-V toolchain.
//! The API key is read from the environment and never printed.

mod common;

use agent::llm::DeepSeekClient;
use agent::policy::WorkspacePolicy;
use agent::prompt::build_system_prompt;
use agent::{AgentConfig, AgentLoop, AgentOutcome};
use common::{constitution_path, sink, unique_dir};
use std::sync::Arc;

#[test]
#[ignore = "requires DEEPSEEK_API_KEY and network"]
fn real_deepseek_writes_and_runs_hello_world() {
    let config = AgentConfig::from_env().expect("DEEPSEEK_API_KEY must be set");
    println!("using model {} at {}", config.model, config.endpoint());

    let client = DeepSeekClient::new(config.clone()).expect("http client");
    let root = unique_dir("real");
    let policy = WorkspacePolicy::new(root.clone());
    let (audit, shared) = sink();
    let system = build_system_prompt(&constitution_path()).expect("system prompt");

    let mut agent = AgentLoop::new(Box::new(client), config, policy, Arc::clone(&audit), system)
        .expect("agent loop");

    let outcome = agent
        .run("写一个 RISC-V 裸机 Hello World，编译、启动并在串口打印 HELLO RISCV，然后读回串口输出")
        .expect("run");

    println!("outcome: {outcome:?}");

    let store = shared.lock().expect("lock store");
    let serial = common::tool_result_text(&store);
    println!("serial tool output:\n{serial}");

    assert!(
        serial.contains("HELLO RISCV"),
        "expected HELLO RISCV in serial output"
    );
    assert!(
        !matches!(outcome, AgentOutcome::Failed { .. }),
        "run failed: {outcome:?}"
    );
}
