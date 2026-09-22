//! v0.8 main deliverable 1/2 — the two-process prototype, end to end.
//!
//! The supervisor here is the test itself: it builds a `Task`, hands it to a
//! `LocalDispatcher` holding one `StdioExecutorHandle`, and reads the
//! `TaskOutcome` back. The executor is the **real** `worker` binary
//! (`CARGO_BIN_EXE_worker`), so the whole path — process spawn, JSON on stdin,
//! `AppState::with_data_dir`, `run_agent`, JSON on stdout — is exercised, not
//! mocked. This is the repo's established way to drive a binary from a test (see
//! `audit/tests/*.rs`, which do the same with `CARGO_BIN_EXE_audit-verify`).
//!
//! What a worker cannot do in a test is *complete* a run: it has no LLM
//! configured (the in-process tests inject one through `llm_override`, which does
//! not cross a process boundary), and QEMU is never called from here. The child
//! therefore answers with the failure the host reports, and that is the assertion:
//! a refusal arrives as a well-formed outcome, not as a crash or a hang. Full-run
//! behaviour stays covered in-process by `host`'s own tests.

use agent::{
    AgentHandle, AgentId, AgentOutcome, DispatchError, Dispatcher, LocalDispatcher, Task,
    TaskOutcome,
};
use host::StdioExecutorHandle;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::Duration;

const WORKER: &str = env!("CARGO_BIN_EXE_worker");

fn unique_dir(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "riscdom-worker-{tag}-{}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// The worker's command line for one test: a private workspace and a private data
/// dir, plus whatever `extra` flags the test wants.
fn worker_args(tag: &str, extra: Vec<String>) -> (Vec<String>, PathBuf, PathBuf) {
    let base = unique_dir(tag);
    let workspace = base.join("workspace");
    let data_dir = base.join("data");
    std::fs::create_dir_all(&workspace).unwrap();

    let mut args = vec![
        "--workspace".to_string(),
        workspace.display().to_string(),
        "--data-dir".to_string(),
        data_dir.display().to_string(),
    ];
    args.extend(extra);
    (args, workspace, data_dir)
}

/// A supervisor-side handle for the worker binary, everything inside `tag`'s dir.
///
/// The inherited `DEEPSEEK_API_KEY` is removed on purpose: an executor's
/// environment is the supervisor's decision, and without a key the child stops at
/// the readiness check — deterministically, and without ever calling a model.
fn handle(tag: &str, extra: Vec<String>) -> (StdioExecutorHandle, PathBuf, PathBuf) {
    let (args, workspace, data_dir) = worker_args(tag, extra);
    let handle = StdioExecutorHandle::new(AgentId::new(format!("supervisor:{tag}")), WORKER, args)
        .with_env_removed("DEEPSEEK_API_KEY")
        .with_env_removed("DEEPSEEK_BASE_URL");
    (handle, workspace, data_dir)
}

#[test]
fn a_task_crosses_the_process_boundary_and_comes_back_as_an_outcome() {
    let (handle, _workspace, _data_dir) = handle("roundtrip", Vec::new());
    let handle = Arc::new(handle);
    let dispatcher = LocalDispatcher::new(vec![Arc::clone(&handle) as Arc<dyn AgentHandle>]);

    let task = Task::new(handle.agent_id().clone(), "say hi");
    let outcome = dispatcher
        .dispatch(task.clone())
        .expect("the worker answers");

    // The outcome answers the task that was sent...
    assert_eq!(outcome.task_id, task.id);

    // With no LLM configured the run is refused, and the refusal is data. Checked
    // first: if the child never got as far as its own identity, this says why.
    match &outcome.outcome {
        AgentOutcome::Failed { reason, iterations } => {
            assert_eq!(*iterations, 0, "the run never entered the loop");
            assert!(
                reason.starts_with("no_config"),
                "the readiness refusal must arrive intact: {reason}"
            );
        }
        other => panic!("a worker without an LLM must answer Failed, got {other:?}"),
    }

    // ...and names the executor that **actually ran it**: the child's own identity,
    // minted in the child process, not the label the supervisor addressed it by.
    // The two differ here — that is the whole point of this field.
    let child_id = outcome.agent_id.to_string();
    assert_ne!(
        child_id,
        task.target.to_string(),
        "the outcome names the executor, not the target it was sent to"
    );
    assert_ne!(
        child_id,
        handle.agent_id().to_string(),
        "the child's identity is its own, not the supervisor's label"
    );
    let parts: Vec<&str> = child_id.split('-').collect();
    assert_eq!(parts.len(), 3, "device-pid-seq: {child_id}");
    assert_eq!(parts[0], "local");
    assert_ne!(
        parts[1],
        std::process::id().to_string(),
        "the identity was minted in another process: {child_id}"
    );

    // The same identity is what the child announced on its event channel, and
    // every event line agrees on it: one JSON object per line, one agent per child.
    let events = handle.events();
    assert!(
        events.iter().any(|line| line.contains("worker:ready")),
        "the worker announces itself: {events:?}"
    );
    let mut announced: Option<String> = None;
    let mut ready_id: Option<String> = None;
    for line in &events {
        let parsed: serde_json::Value = serde_json::from_str(line)
            .unwrap_or_else(|e| panic!("event line is JSON ({e}): {line}"));
        assert_eq!(parsed["kind"], "event");
        let id = parsed["agent_id"]
            .as_str()
            .unwrap_or_else(|| panic!("every event names its agent: {line}"))
            .to_string();
        assert_eq!(
            id,
            announced.clone().unwrap_or_else(|| id.clone()),
            "one agent per child"
        );
        if parsed["event"] == "worker:ready" {
            ready_id = Some(id.clone());
        }
        announced = Some(id);
    }
    let ready_id = ready_id.expect("the child announces itself before it runs");
    assert_eq!(
        ready_id, child_id,
        "the outcome's identity is the one the child announced"
    );
}

#[test]
fn a_worker_that_cannot_start_is_reported_as_a_failure_not_a_hang() {
    // Valid workspace, no `--data-dir`: the worker exits with a usage error and
    // never writes an outcome.
    let (args, _workspace, _data_dir) = worker_args("usage", Vec::new());
    let args = args[..2].to_vec(); // drop --data-dir and its value
    let handle = StdioExecutorHandle::new(AgentId::new("supervisor:usage"), WORKER, args)
        .with_timeout(Duration::from_secs(60));

    let error = handle
        .run(&Task::new(handle.agent_id().clone(), "hi"))
        .expect_err("a worker that answers nothing is a failure");
    match error {
        DispatchError::Failed(message) => assert!(
            message.contains("without an outcome"),
            "the failure must say the executor answered nothing: {message}"
        ),
        other => panic!("expected a transport failure, got {other:?}"),
    }
}

#[test]
fn a_worker_that_never_answers_is_killed_and_reported() {
    let (args, _workspace, _data_dir) =
        worker_args("timeout", vec!["--sleep-ms".into(), "30000".into()]);
    let handle = StdioExecutorHandle::new(AgentId::new("supervisor:timeout"), WORKER, args)
        .with_env_removed("DEEPSEEK_API_KEY")
        .with_timeout(Duration::from_millis(500));

    let error = handle
        .run(&Task::new(handle.agent_id().clone(), "hi"))
        .expect_err("a worker that never answers is a failure");
    match error {
        DispatchError::Failed(message) => assert!(
            message.contains("did not answer within"),
            "the failure must name the deadline: {message}"
        ),
        other => panic!("expected a timeout, got {other:?}"),
    }
}

#[test]
fn a_malformed_task_is_answered_with_a_failure_and_no_crash() {
    let (args, _workspace, _data_dir) = worker_args("malformed", Vec::new());

    let mut child = Command::new(WORKER)
        .args(&args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn the worker");

    {
        use std::io::Write;
        let mut stdin = child.stdin.take().expect("stdin");
        stdin.write_all(b"{ this is not a task }\n").expect("write");
    } // dropped: stdin closed

    let output = child.wait_with_output().expect("wait");
    assert!(
        output.status.success(),
        "the worker must not crash: {output:?}"
    );

    let line = String::from_utf8(output.stdout).expect("utf8");
    let outcome: TaskOutcome =
        serde_json::from_str(line.trim()).unwrap_or_else(|e| panic!("outcome JSON ({e}): {line}"));
    assert_eq!(outcome.task_id.to_string(), "task-unparsed");
    assert_eq!(outcome.agent_id.to_string(), "unparsed");
    assert!(
        matches!(outcome.outcome, AgentOutcome::Failed { .. }),
        "a malformed task is answered with a failure: {outcome:?}"
    );
}

#[test]
fn two_workers_keep_their_own_data_dirs() {
    let (first, first_workspace, first_data) = handle("isolation-a", Vec::new());
    let (second, second_workspace, second_data) = handle("isolation-b", Vec::new());
    let first = Arc::new(first);
    let second = Arc::new(second);

    for (handle, task) in [
        (
            Arc::clone(&first) as Arc<dyn AgentHandle>,
            Task::new(first.agent_id().clone(), "hi"),
        ),
        (
            Arc::clone(&second) as Arc<dyn AgentHandle>,
            Task::new(second.agent_id().clone(), "hi"),
        ),
    ] {
        let dispatcher = LocalDispatcher::new(vec![handle]);
        dispatcher.dispatch(task).expect("both workers answer");
    }

    // Each executor owns its data dir: its own sessions DB, never the other's and
    // never one inside the shared workspace.
    assert!(first_data.join("sessions.db").is_file(), "{first_data:?}");
    assert!(second_data.join("sessions.db").is_file(), "{second_data:?}");
    assert_ne!(first_data, second_data);
    assert!(!first_workspace.join("sessions.db").exists());
    assert!(!second_workspace.join("sessions.db").exists());
}
