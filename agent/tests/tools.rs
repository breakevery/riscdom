//! Stage 5b — policy + tools + compiler integration.

use agent::compiler::CompilerConfig;
use agent::policy::WorkspacePolicy;
use agent::tools::{execute_tool, ToolContext, VM_MEMORY_MB};
use audit::{verify_chain, AuditSink, AuditStore, ChainStatus, SqliteAuditSink};
use sandbox::vm::RiscVVirtualMachine;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

/// A fixed identity for these tests: the field is an audit label, and a literal
/// keeps the assertions readable.
const TEST_AGENT_ID: &str = "local-0-test";

fn unique_dir(tag: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "riscdom-tools-{tag}-{}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).expect("create workspace");
    dir
}

fn sink() -> (Arc<Mutex<dyn AuditSink>>, Arc<Mutex<AuditStore>>) {
    let shared = Arc::new(Mutex::new(AuditStore::in_memory().expect("store")));
    let s: Arc<Mutex<dyn AuditSink>> = Arc::new(Mutex::new(SqliteAuditSink::from_shared(
        Arc::clone(&shared),
    )));
    (s, shared)
}

fn count_actions(store: &AuditStore, action: &str) -> usize {
    store
        .all()
        .unwrap()
        .iter()
        .filter(|e| e.event.action == action)
        .count()
}

#[test]
fn write_source_writes_file_and_audits_three_events() {
    let root = unique_dir("write");
    let policy = WorkspacePolicy::new(root.clone());
    let (audit, shared) = sink();
    let compiler = CompilerConfig::from_env();
    let mut vm_slot: Option<RiscVVirtualMachine> = None;
    let mut ctx = ToolContext {
        policy: &policy,
        audit: Arc::clone(&audit),
        vm: &mut vm_slot,
        compiler: &compiler,
        serial_observers: Arc::new(Mutex::new(Vec::new())),
        qemu_exe: &None,
        requester: None,
        agent_id: TEST_AGENT_ID,
        memory_mb: VM_MEMORY_MB,
    };

    let msg = execute_tool(
        "write_source",
        r#"{"path":"hello.c","content":"int main(void){return 0;}"}"#,
        &mut ctx,
    )
    .expect("write_source");
    assert!(msg.contains("wrote"), "{msg}");
    assert!(root.join("hello.c").exists(), "file not written");

    let store = shared.lock().unwrap();
    assert_eq!(count_actions(&store, "agent.tool.call"), 1);
    assert_eq!(count_actions(&store, "agent.tool.result"), 1);
    // And the file itself is on the chain (v0.9 project in/out): one row per write,
    // naming the path and how much landed, so a project's provenance does not have
    // to be re-derived from a truncated tool argument.
    assert_eq!(count_actions(&store, "agent.file.write"), 1);
    let written = store
        .all()
        .expect("events")
        .into_iter()
        .find(|event| event.event.action == "agent.file.write")
        .expect("the file write event");
    assert_eq!(written.event.detail["path"], "hello.c");
    assert_eq!(
        written.event.detail["bytes"].as_u64(),
        Some("int main(void){return 0;}".len() as u64)
    );
    assert_eq!(
        verify_chain(&store).unwrap(),
        ChainStatus::Intact { length: 3 }
    );
}

#[test]
fn compile_fixture_succeeds() {
    let root = unique_dir("compile");
    let policy = WorkspacePolicy::new(root.clone());
    let (audit, shared) = sink();
    let compiler = CompilerConfig::from_env();
    let mut vm_slot: Option<RiscVVirtualMachine> = None;
    let mut ctx = ToolContext {
        policy: &policy,
        audit: Arc::clone(&audit),
        vm: &mut vm_slot,
        compiler: &compiler,
        serial_observers: Arc::new(Mutex::new(Vec::new())),
        qemu_exe: &None,
        requester: None,
        agent_id: TEST_AGENT_ID,
        memory_mb: VM_MEMORY_MB,
    };

    let src = include_str!("fixtures/hello.c");
    let write_args = serde_json::json!({ "path": "hello.c", "content": src }).to_string();
    execute_tool("write_source", &write_args, &mut ctx).expect("write_source");

    let out = execute_tool(
        "compile",
        r#"{"source_path":"hello.c","output_elf":"hello.elf"}"#,
        &mut ctx,
    )
    .expect("compile");
    assert!(out.contains("compiled"), "{out}");
    assert!(root.join("hello.elf").exists(), "elf not produced");

    let store = shared.lock().unwrap();
    assert_eq!(count_actions(&store, "agent.compile.start"), 1);
    assert_eq!(count_actions(&store, "agent.compile.result"), 1);
    assert!(matches!(
        verify_chain(&store).unwrap(),
        ChainStatus::Intact { .. }
    ));
}

#[test]
fn policy_denies_traversal_and_bad_extension() {
    let root = unique_dir("deny");
    let policy = WorkspacePolicy::new(root.clone());
    let (audit, shared) = sink();
    let compiler = CompilerConfig::from_env();
    let mut vm_slot: Option<RiscVVirtualMachine> = None;
    let mut ctx = ToolContext {
        policy: &policy,
        audit: Arc::clone(&audit),
        vm: &mut vm_slot,
        compiler: &compiler,
        serial_observers: Arc::new(Mutex::new(Vec::new())),
        qemu_exe: &None,
        requester: None,
        agent_id: TEST_AGENT_ID,
        memory_mb: VM_MEMORY_MB,
    };

    let traversal = execute_tool(
        "write_source",
        r#"{"path":"../etc/passwd","content":"x"}"#,
        &mut ctx,
    );
    assert!(traversal.is_err(), "traversal must be denied");

    let bad_ext = execute_tool(
        "write_source",
        r#"{"path":"evil.py","content":"x"}"#,
        &mut ctx,
    );
    assert!(bad_ext.is_err(), ".py must be denied");

    assert!(!root.join("evil.py").exists());

    let store = shared.lock().unwrap();
    assert_eq!(count_actions(&store, "agent.policy.deny"), 2);
    assert_eq!(count_actions(&store, "agent.tool.result"), 2);
    assert!(matches!(
        verify_chain(&store).unwrap(),
        ChainStatus::Intact { .. }
    ));
}
