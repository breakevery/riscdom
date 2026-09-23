//! Stage v0.3-5c-2b — an empty `read_serial` must return a usable notice.
//!
//! The guest can still be booting when the model calls `read_serial`; handing it
//! an empty string made it guess. The notice is **data**, not an instruction,
//! exactly like the serial text itself.

mod common;

use agent::compiler::CompilerConfig;
use agent::policy::WorkspacePolicy;
use agent::tools::{execute_tool, ToolContext};
use common::{constitution_path, sink, unique_dir};
use sandbox::platform::{QmpEndpoint, SerialEndpoint};
use sandbox::vm::{RiscVVirtualMachine, VMConfig};
use std::sync::Arc;

#[test]
fn an_empty_buffer_returns_a_notice_not_an_empty_string() {
    let root = unique_dir("read-serial-empty");
    let policy = WorkspacePolicy::new(root.clone());
    let (audit, _shared) = sink();

    // A VM handle that was never started: its serial buffer is empty, so
    // `read_serial` takes the "nothing yet" path without spawning QEMU.
    let kernel = root.join("never-booted.elf");
    std::fs::write(&kernel, b"not a real elf").expect("write dummy kernel");
    let config = VMConfig {
        kernel,
        memory_mb: 128,
        qmp: QmpEndpoint::tcp("127.0.0.1", 45_001),
        serial: SerialEndpoint::tcp("127.0.0.1", 45_002),
        snapshot_dir: root.join("snapshots"),
        serial_observer: None,
        incoming_snapshot: None,
        incoming_relay_addr: None,
        qemu_exe: None,
    };
    let vm = RiscVVirtualMachine::new(config, Arc::clone(&audit)).expect("vm handle");
    let mut slot = Some(vm);

    let compiler = CompilerConfig::from_env();
    let mut ctx = ToolContext {
        policy: &policy,
        audit: Arc::clone(&audit),
        vm: &mut slot,
        compiler: &compiler,
        serial_observers: Arc::new(std::sync::Mutex::new(Vec::new())),
        qemu_exe: &None,
        requester: None,
        agent_id: "local-0-test",
    };

    let started = std::time::Instant::now();
    let result = execute_tool("read_serial", "{}", &mut ctx).expect("read_serial must be Ok");
    println!("elapsed: {:?}", started.elapsed());
    println!("result: {result}");

    assert!(
        result.contains("still") && result.contains("booting"),
        "the notice must explain the empty buffer: {result:?}"
    );
    assert!(
        result.contains("read_serial"),
        "the notice must offer the next step: {result:?}"
    );
    assert!(
        !result.starts_with("error"),
        "an empty buffer is not an error: {result:?}"
    );

    // The system prompt tells the model the same thing.
    let prompt = agent::prompt::build_system_prompt(&constitution_path()).expect("prompt");
    assert!(
        prompt.contains("still") && prompt.contains("booting"),
        "prompt must mention it"
    );
}
