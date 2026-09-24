//! v0.9 F3a — a Zig source compiles and boots in the same bare-metal guest.
//!
//! The ELF is language-agnostic: `compile` dispatches on the source extension, so the
//! guest should not be able to tell C from Zig. This test boots the Zig fixture and reads
//! the UART back.
//!
//! It needs both a Zig compiler and a QEMU guest, so it carries the marker. It **also**
//! guards itself: the gate runs every ignored test with `--include-ignored`, and a
//! developer machine has QEMU and a RISC-V GCC but not necessarily Zig, so a marker alone
//! would turn that machine's gate red.

mod common;

use agent::compiler::{CompilerConfig, ZigConfig};
use agent::policy::WorkspacePolicy;
use agent::tools::{execute_tool, ToolContext, VM_MEMORY_MB};
use common::{sink, unique_dir};
use sandbox::vm::RiscVVirtualMachine;
use std::sync::{Arc, Mutex};

const HELLO_ZIG: &str = include_str!("fixtures/hello.zig");

#[test]
#[ignore = "requires a QEMU guest and a Zig compiler; run with --include-ignored"]
fn a_zig_source_boots_and_prints() {
    if ZigConfig::discover().is_err() {
        eprintln!("skip: a_zig_source_boots_and_prints -- no Zig found (ENVIRONMENT.md)");
        return;
    }

    let root = unique_dir("zig-vm");
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

    let write = serde_json::json!({ "path": "hello.zig", "content": HELLO_ZIG }).to_string();
    execute_tool("write_source", &write, &mut ctx).expect("write_source");
    execute_tool(
        "compile",
        r#"{"source_path":"hello.zig","output_elf":"hello-zig.elf"}"#,
        &mut ctx,
    )
    .expect("compile");
    println!(
        "{}",
        execute_tool("start_vm", r#"{"elf_path":"hello-zig.elf"}"#, &mut ctx).expect("start_vm")
    );

    let out = execute_tool("read_serial", "{}", &mut ctx).expect("read_serial");
    println!("serial: {out}");
    assert!(out.contains("hello from zig"), "{out}");

    let _ = execute_tool("stop_vm", "{}", &mut ctx);
}
