//! v0.9 sandbox F2c — the AI's sandbox-request tools.
//!
//! The two tools are a thin front for the host's [`SandboxRequester`]: they
//! validate the action, hand the ask on, and put the id in front of the model.
//! What they must **not** do is pretend: a host that injects no requester has to
//! come back as an error the model can report, not as a silent success.

use agent::compiler::CompilerConfig;
use agent::policy::WorkspacePolicy;
use agent::tools::{execute_tool, SandboxRequester, ToolContext, VM_MEMORY_MB};
use audit::{AuditSink, AuditStore, SqliteAuditSink};
use sandbox::vm::RiscVVirtualMachine;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

/// One recorded call: what the tool asked the host for.
///
/// Named rather than inlined: the tuple is long enough that clippy calls the
/// `Mutex<Vec<…>>` type complex, and a name reads better in the assertions anyway.
type RecordedCall = (String, Option<String>, Option<String>);

/// A requester that records what it was asked, so the test can assert on the
/// arguments as well as on the answer.
#[derive(Default)]
struct RecordingRequester {
    calls: Mutex<Vec<RecordedCall>>,
}

impl SandboxRequester for RecordingRequester {
    fn request(
        &self,
        action: &str,
        sandbox: Option<&str>,
        reason: Option<&str>,
    ) -> Result<String, String> {
        self.calls.lock().expect("calls").push((
            action.to_string(),
            sandbox.map(str::to_string),
            reason.map(str::to_string),
        ));
        Ok("req-4242-1".to_string())
    }

    fn status(&self) -> Result<String, String> {
        Ok("running: fallback; 1 request waiting".to_string())
    }
}

fn unique_dir(tag: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "riscdom-sandbox-tools-{tag}-{}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).expect("create workspace");
    dir
}

fn sink() -> Arc<Mutex<dyn AuditSink>> {
    let shared = Arc::new(Mutex::new(AuditStore::in_memory().expect("store")));
    Arc::new(Mutex::new(SqliteAuditSink::from_shared(shared)))
}

/// Run one tool call with `requester` injected (or not).
fn run(
    requester: Option<Arc<dyn SandboxRequester>>,
    tool: &str,
    args: &str,
) -> Result<String, String> {
    let policy = WorkspacePolicy::new(unique_dir("gateway"));
    let compiler = CompilerConfig::from_env();
    let audit = sink();
    let mut vm_slot: Option<RiscVVirtualMachine> = None;
    let mut ctx = ToolContext {
        policy: &policy,
        audit,
        vm: &mut vm_slot,
        compiler: &compiler,
        serial_observers: Arc::new(Mutex::new(Vec::new())),
        qemu_exe: &None,
        requester: requester.as_ref(),
        agent_id: "local-0-test",
        memory_mb: VM_MEMORY_MB,
    };
    execute_tool(tool, args, &mut ctx).map_err(|e| e.to_string())
}

#[test]
fn request_sandbox_hands_the_ask_on_and_reports_the_id() {
    let requester = Arc::new(RecordingRequester::default());
    let answer = run(
        Some(Arc::clone(&requester) as Arc<dyn SandboxRequester>),
        "request_sandbox",
        r#"{"action":"switch","sandbox":"big","reason":"needs more memory"}"#,
    )
    .expect("request_sandbox");
    assert!(answer.contains("req-4242-1"), "{answer}");
    assert!(answer.contains("waiting for approval"), "{answer}");

    let calls = requester.calls.lock().expect("calls");
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].0, "switch");
    assert_eq!(calls[0].1.as_deref(), Some("big"));
    assert_eq!(calls[0].2.as_deref(), Some("needs more memory"));
}

#[test]
fn request_sandbox_refuses_an_action_it_does_not_know() {
    let requester = Arc::new(RecordingRequester::default());
    let err = run(
        Some(Arc::clone(&requester) as Arc<dyn SandboxRequester>),
        "request_sandbox",
        r#"{"action":"rm -rf"}"#,
    )
    .expect_err("unknown action");
    assert!(err.contains("unknown action"), "{err}");
    // The ask never reached the host.
    assert!(requester.calls.lock().expect("calls").is_empty());
}

#[test]
fn sandbox_status_asks_the_host() {
    let requester = Arc::new(RecordingRequester::default());
    let answer = run(
        Some(Arc::clone(&requester) as Arc<dyn SandboxRequester>),
        "sandbox_status",
        "{}",
    )
    .expect("sandbox_status");
    assert!(answer.contains("fallback"), "{answer}");
}

#[test]
fn without_a_host_surface_the_tools_say_so() {
    let err = run(None, "request_sandbox", r#"{"action":"switch"}"#).expect_err("no surface");
    assert!(err.contains("no sandbox request surface"), "{err}");
    let err = run(None, "sandbox_status", "{}").expect_err("no surface");
    assert!(err.contains("no sandbox request surface"), "{err}");
}

#[test]
fn the_two_tools_are_in_the_catalogue() {
    let names: Vec<String> = agent::tools::tool_specs()
        .into_iter()
        .map(|spec| spec.name)
        .collect();
    assert!(names.iter().any(|n| n == "request_sandbox"), "{names:?}");
    assert!(names.iter().any(|n| n == "sandbox_status"), "{names:?}");
}
