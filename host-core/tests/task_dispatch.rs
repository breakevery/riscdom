//! v0.9 interface E0 — the host half of `POST /v0/tasks`: the executor registry
//! comes from `settings.json`, and one task is routed to the executor its target
//! names.
//!
//! What is exercised here: the fleet is configuration and nothing else; the node
//! is never one of its own executors; a target nobody owns is refused as
//! [`HostError::NoSuchExecutor`] (the endpoint's `404 cause "target"`); a program
//! that cannot start is a *dispatch* failure, not a missing target; and a child
//! that really speaks the protocol answers with an outcome whose `agent_id` is the
//! identity the **child announced**, not the label it was addressed by.
//!
//! No QEMU, no network, no LLM. The answering executor is a script this test
//! writes; `StdioExecutorHandle` spawns nothing until a task arrives, so
//! registering a fleet starts no process at all.

use host_core::state::AppState;
use host_core::HostError;
use serde_json::json;
use std::path::{Path, PathBuf};

fn unique_dir(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "riscdom-taskdispatch-{tag}-{}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Write `settings.json` into the data directory, before the state reads it —
/// the same helper the sandbox tests use.
fn write_settings(data_dir: &Path, settings: serde_json::Value) {
    std::fs::write(
        data_dir.join("settings.json"),
        serde_json::to_string_pretty(&settings).expect("serde"),
    )
    .expect("write settings");
}

/// A state whose data directory the test controls, so settings can be planted.
fn state_with(tag: &str) -> (AppState, PathBuf) {
    let workspace = unique_dir(&format!("{tag}-ws"));
    let data_dir = unique_dir(&format!("{tag}-data"));
    let state = AppState::with_data_dir(&workspace, &data_dir).expect("state");
    (state, data_dir)
}

#[test]
fn a_node_with_no_fleet_refuses_every_target() {
    let (state, _data) = state_with("empty");
    assert!(state.executors().is_empty(), "no executors configured");

    let error = state
        .dispatch_task("executor-0", "say hi", None, None)
        .expect_err("a node with no fleet owns nobody");
    match error {
        HostError::NoSuchExecutor(target) => assert_eq!(target, "executor-0"),
        other => panic!("expected NoSuchExecutor, got {other:?}"),
    }
}

#[test]
fn the_fleet_comes_from_the_settings_file_in_order() {
    let (_, data_dir) = state_with("fleet");
    write_settings(
        &data_dir,
        json!({
            "version": 1,
            "executors": [
                { "label": "executor-0", "program": "worker.exe", "args": ["--workspace", "W"] },
                { "label": "executor-1", "program": "worker.exe" },
            ],
        }),
    );
    let workspace = unique_dir("fleet-ws");
    let state = AppState::with_data_dir(&workspace, &data_dir).expect("state");

    assert_eq!(state.executors(), vec!["executor-0", "executor-1"]);
    // Registration spawns nothing: the handles are data until a task arrives.
    assert!(state.executors().len() == 2);
}

#[test]
fn a_settings_file_written_before_the_field_still_loads() {
    let (_, data_dir) = state_with("older");
    write_settings(&data_dir, json!({ "version": 1 }));
    let workspace = unique_dir("older-ws");
    let state = AppState::with_data_dir(&workspace, &data_dir).expect("state");
    assert!(state.executors().is_empty());
}

#[test]
fn the_node_is_never_one_of_its_own_executors() {
    let (_, data_dir) = state_with("self");
    write_settings(
        &data_dir,
        json!({
            "version": 1,
            "executors": [{ "label": "executor-0", "program": "worker.exe" }],
        }),
    );
    let workspace = unique_dir("self-ws");
    let state = AppState::with_data_dir(&workspace, &data_dir).expect("state");

    // The node runs through `POST /v0/agent/run`, not through its own task
    // endpoint: its identity must not appear in the fleet.
    let own = state.agent_id().to_string();
    assert!(
        !state.executors().contains(&own),
        "the node registered itself: {own}"
    );
    let error = state
        .dispatch_task(&own, "say hi", None, None)
        .expect_err("the node is not its own executor");
    assert!(matches!(error, HostError::NoSuchExecutor(_)), "{error:?}");
}

#[test]
fn a_program_that_cannot_start_is_a_task_failure_not_a_missing_target() {
    let (_, data_dir) = state_with("noprogram");
    // Registered, so the target *exists* — the dispatch itself is what breaks.
    write_settings(
        &data_dir,
        json!({
            "version": 1,
            "executors": [{
                "label": "executor-0",
                "program": r"C:\definitely\not\here\worker.exe",
                "args": [],
            }],
        }),
    );
    let workspace = unique_dir("noprogram-ws");
    let state = AppState::with_data_dir(&workspace, &data_dir).expect("state");

    let error = state
        .dispatch_task("executor-0", "say hi", None, None)
        .expect_err("nothing can start");
    match &error {
        // Not `NoSuchExecutor`: the endpoint answers `500 cause "task"` here, and
        // `404 cause "target"` only when the fleet really has nobody by that name.
        HostError::TaskFailed(message) => {
            assert!(message.contains("cannot start"), "{message}")
        }
        other => panic!("expected TaskFailed, got {other:?}"),
    }
}

/// A `.bat` that speaks the worker protocol: read the task line, answer with one
/// outcome line on stdout, announce an identity on stderr.
///
/// `cmd` so the fixture is a file, not a compiled binary. It echoes back **the id
/// it was given** (read out of the task line), which is what the protocol checks —
/// and its `agent_id` is deliberately not the label the supervisor addressed it
/// by, because the identity is the child's to mint.
#[cfg(windows)]
fn a_speaking_executor(dir: &Path) -> PathBuf {
    let path = dir.join("fake-executor.bat");
    let outcome = json!({
        "task_id": "%_id%",
        "agent_id": "the-label-is-not-the-answer",
        "outcome": { "Final": { "content": "hello from the fake", "iterations": 1 } },
    });
    let ready = json!({ "event": "worker:ready", "agent_id": "fake-child-1-1" });
    // The task line serde writes starts `{"id":"…","target":…`, so after the
    // quotes come off, splitting on `:` and `,` makes token 2 the id.
    // CRLF: `cmd` reads a batch file line by line and is happier with them.
    let script = format!(
        "@echo off\r\nset /p _task=\r\nset _task=%_task:\"=%\r\n\
         for /f \"tokens=2 delims=:,\" %%a in (\"%_task%\") do set _id=%%a\r\n\
         echo {outcome}\r\necho {ready} 1>&2\r\n",
        outcome = outcome,
        ready = ready,
    );
    std::fs::write(&path, script).expect("write the fake executor");
    path
}

#[cfg(windows)]
#[test]
fn a_child_that_answers_reports_the_identity_it_announced() {
    let (_, data_dir) = state_with("answers");
    let script = a_speaking_executor(&data_dir);
    write_settings(
        &data_dir,
        json!({
            "version": 1,
            "executors": [{
                "label": "executor-0",
                "program": "cmd.exe",
                "args": ["/C", script.display().to_string()],
            }],
        }),
    );
    let workspace = unique_dir("answers-ws");
    let state = AppState::with_data_dir(&workspace, &data_dir).expect("state");

    let outcome = state
        .dispatch_task("executor-0", "say hi", None, Some("task-1-1"))
        .expect("the fake executor answers");

    // The id the caller sent comes back unchanged — that is what matches the two
    // halves of the protocol.
    assert_eq!(outcome.task_id.as_str(), "task-1-1");
    // The identity is the child's own announcement, **not** the label.
    assert_eq!(outcome.agent_id.as_str(), "fake-child-1-1");
    assert_eq!(
        outcome.outcome,
        agent::AgentOutcome::Final {
            content: "hello from the fake".into(),
            iterations: 1,
        }
    );
}

#[cfg(windows)]
#[test]
fn a_task_without_an_id_gets_one_from_the_server() {
    let (_, data_dir) = state_with("minted");
    let script = a_speaking_executor(&data_dir);
    write_settings(
        &data_dir,
        json!({
            "version": 1,
            "executors": [{
                "label": "executor-0",
                "program": "cmd.exe",
                "args": ["/C", script.display().to_string()],
            }],
        }),
    );
    let workspace = unique_dir("minted-ws");
    let state = AppState::with_data_dir(&workspace, &data_dir).expect("state");

    // No id: the server mints one, and the executor echoes it back — so the two
    // ids agreeing is the proof that the minted one travelled the whole way.
    let outcome = state
        .dispatch_task("executor-0", "say hi", None, None)
        .expect("the fake executor answers");
    assert!(
        outcome.task_id.as_str().starts_with("task-"),
        "{}",
        outcome.task_id.as_str()
    );
}
