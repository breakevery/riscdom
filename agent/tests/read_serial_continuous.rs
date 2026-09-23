//! v0.3.1 #2 — a continuously printing guest must not be reported as silent.
//!
//! `read_serial` returns once the buffer has been quiet for ~150 ms; a guest that
//! keeps printing (a heartbeat line, a busy log) never reaches that state, so the
//! tool falls through to its overall wait. Before the fix that branch always
//! answered "No serial output yet" — telling the model the console was empty
//! while a full buffer sat in memory.

mod common;

use agent::compiler::CompilerConfig;
use agent::policy::WorkspacePolicy;
use agent::tools::{execute_tool, ToolContext, VM_MEMORY_MB};
use common::{sink, unique_dir};
use sandbox::vm::RiscVVirtualMachine;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const CHATTER_C: &str = include_str!("fixtures/chatter.c");

#[test]
fn a_guest_that_never_goes_quiet_returns_its_output() {
    let root = unique_dir("read-serial-continuous");
    let policy = WorkspacePolicy::new(root.clone());
    let (audit, _shared) = sink();
    let compiler = CompilerConfig::from_env();
    let mut slot: Option<RiscVVirtualMachine> = None;
    let mut ctx = ToolContext {
        policy: &policy,
        audit: Arc::clone(&audit),
        vm: &mut slot,
        compiler: &compiler,
        serial_observers: Arc::new(Mutex::new(Vec::new())),
        qemu_exe: &None,
        requester: None,
        agent_id: "local-0-test",
        memory_mb: VM_MEMORY_MB,
    };

    let write = serde_json::json!({ "path": "chatter.c", "content": CHATTER_C }).to_string();
    execute_tool("write_source", &write, &mut ctx).expect("write_source");
    execute_tool(
        "compile",
        r#"{"source_path":"chatter.c","output_elf":"chatter.elf"}"#,
        &mut ctx,
    )
    .expect("compile");
    println!(
        "{}",
        execute_tool("start_vm", r#"{"elf_path":"chatter.elf"}"#, &mut ctx).expect("start_vm")
    );

    let started = Instant::now();
    let result = execute_tool("read_serial", "{}", &mut ctx).expect("read_serial must be Ok");
    let elapsed = started.elapsed();
    println!("elapsed={elapsed:?} result_bytes={}", result.len());

    assert!(
        elapsed >= Duration::from_millis(4000),
        "the quiet window must not close for a continuously printing guest: {elapsed:?}"
    );
    assert!(
        !result.contains("No serial output yet"),
        "captured output must never be reported as silence: {result:?}"
    );
    assert!(
        result.contains("CHATTER"),
        "the guest's output must be handed back: {result:?}"
    );

    let _ = execute_tool("stop_vm", "{}", &mut ctx);
}
