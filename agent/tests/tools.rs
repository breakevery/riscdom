//! Stage 5b — policy + tools + compiler integration.

use agent::compiler::CompilerConfig;
use agent::policy::WorkspacePolicy;
use agent::tools::{execute_tool, ToolContext};
use audit::{verify_chain, AuditSink, AuditStore, ChainStatus, SqliteAuditSink};
use sandbox::vm::RiscVVirtualMachine;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

fn unique_dir(tag: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("riscdom-tools-{tag}-{}-{nanos}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create workspace");
    dir
}

fn sink() -> (Arc<Mutex<dyn AuditSink>>, Arc<Mutex<AuditStore>>) {
    let shared = Arc::new(Mutex::new(AuditStore::in_memory().expect("store")));
    let s: Arc<Mutex<dyn AuditSink>> =
        Arc::new(Mutex::new(SqliteAuditSink::from_shared(Arc::clone(&shared))));
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
fn write_source_writes_file_and_audits_two_events() {
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
    assert_eq!(
        verify_chain(&store).unwrap(),
        ChainStatus::Intact { length: 2 }
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
    assert!(matches!(verify_chain(&store).unwrap(), ChainStatus::Intact { .. }));
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
    assert!(matches!(verify_chain(&store).unwrap(), ChainStatus::Intact { .. }));
}
