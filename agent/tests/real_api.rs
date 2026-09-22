//! Stage 5c — real DeepSeek end-to-end test (ignored by default).
//!
//! Stage 21 adds an **audit chain integrity** check: the audit log is written
//! to a file-backed SQLite DB, then reopened from a fresh handle after the run
//! and verified with `audit::verify_chain` (an in-memory store cannot be read
//! once the sink is dropped).
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
use common::{constitution_path, file_sink, unique_dir};
use std::path::PathBuf;
use std::sync::Arc;

/// Deletes the temporary audit DB on the way out — including when the test
/// panics (a `Drop` runs during unwinding).
struct TempAuditDb(PathBuf);

impl Drop for TempAuditDb {
    fn drop(&mut self) {
        match std::fs::remove_file(&self.0) {
            Ok(()) => println!("removed temp audit db: {}", self.0.display()),
            Err(e) => println!("could not remove {}: {e}", self.0.display()),
        }
    }
}

#[test]
#[ignore = "requires DEEPSEEK_API_KEY and network"]
fn real_deepseek_writes_and_runs_hello_world() {
    let config = AgentConfig::from_env().expect("DEEPSEEK_API_KEY must be set");
    println!("using model {} at {}", config.model, config.endpoint());

    let client = DeepSeekClient::new(config.clone()).expect("http client");
    let root = unique_dir("real");
    let policy = WorkspacePolicy::new(root.clone());

    // File-backed audit log (stage 21): readable after the run.
    let (db_path, audit, shared) = file_sink("real");
    let _cleanup = TempAuditDb(db_path.clone());

    let system = build_system_prompt(&constitution_path()).expect("system prompt");

    let mut agent = AgentLoop::new(
        Box::new(client),
        config,
        policy,
        Arc::clone(&audit),
        system,
        agent::next_agent_id(),
    )
    .expect("agent loop");

    let outcome = agent
        .run("写一个 RISC-V 裸机 Hello World，编译、启动并在串口打印 HELLO RISCV，然后读回串口输出")
        .expect("run");

    println!("outcome: {outcome:?}");

    // ----- existing assertions (unchanged) ---------------------------------
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
    assert!(
        !matches!(outcome, AgentOutcome::MaxIterations { .. }),
        "run hit the iteration limit: {outcome:?}"
    );
    drop(store); // release this handle before reopening the same file

    // ----- stage 21: audit chain integrity --------------------------------
    let status = common::assert_chain_intact(&db_path);
    println!("verified audit chain from an independent handle: {status:?}");
}
