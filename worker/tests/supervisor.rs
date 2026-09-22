//! v0.8 main deliverable 2/2 — the supervisor driving **several** executor
//! processes.
//!
//! Same approach as `tests/stdio.rs`: the real `worker` binary
//! (`CARGO_BIN_EXE_worker`), so the fleet is genuinely several processes on one
//! machine. Each executor gets its own data dir and they share one workspace —
//! the case the earlier batches prepared (one audit chain, per-agent snapshots).
//!
//! No LLM is configured in the children (the API key is removed), so every run is
//! refused at the readiness check and comes back as data. That keeps the test
//! offline, free and QEMU-free while still exercising the whole plumbing.

use agent::{
    AgentHandle, AgentId, AgentOutcome, DispatchError, LocalDispatcher, Task, TaskOutcome,
};
use host::StdioExecutorHandle;
use std::path::PathBuf;
use std::sync::Arc;
use worker::supervisor::{
    dispatch_all, dispatcher, report, tally, ExecutorSpec, PlanOutcome, Tally,
};

const WORKER: &str = env!("CARGO_BIN_EXE_worker");

fn unique_dir(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "riscdom-fleet-{tag}-{}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A fleet of `count` executors sharing one workspace, each with its own data dir.
fn fleet(tag: &str, count: usize) -> (Vec<ExecutorSpec>, PathBuf) {
    let base = unique_dir(tag);
    let workspace = base.join("workspace");
    std::fs::create_dir_all(&workspace).unwrap();

    let specs = (0..count)
        .map(|index| {
            ExecutorSpec::new(
                format!("executor-{index}"),
                WORKER,
                vec![
                    "--workspace".into(),
                    workspace.display().to_string(),
                    "--data-dir".into(),
                    base.join(format!("data-{index}")).display().to_string(),
                ],
            )
            // An executor's environment is the supervisor's decision: without a
            // key the children stop at the readiness check, offline and free.
            .with_env_removed("DEEPSEEK_API_KEY")
            .with_env_removed("DEEPSEEK_BASE_URL")
        })
        .collect();
    (specs, base)
}

/// Handles built from the specs, kept so the test can read each child's events.
fn handles(specs: &[ExecutorSpec]) -> Vec<Arc<StdioExecutorHandle>> {
    specs.iter().map(|spec| spec.handle()).collect()
}

fn fleet_dispatcher(handles: &[Arc<StdioExecutorHandle>]) -> LocalDispatcher {
    LocalDispatcher::new(
        handles
            .iter()
            .map(|handle| Arc::clone(handle) as Arc<dyn AgentHandle>)
            .collect(),
    )
}

/// The `agent_id` of every event line a handle drained, if they agree on one.
fn child_id(handle: &StdioExecutorHandle) -> Option<String> {
    let mut id: Option<String> = None;
    for line in handle.events() {
        let parsed: serde_json::Value = serde_json::from_str(&line).expect("event line is JSON");
        let this = parsed["agent_id"]
            .as_str()
            .expect("a named agent")
            .to_string();
        match &id {
            Some(seen) => assert_eq!(seen, &this, "one agent per child"),
            None => id = Some(this),
        }
    }
    id
}

#[test]
fn two_executors_answer_one_task_each() {
    let (specs, _base) = fleet("two", 2);
    let handles = handles(&specs);
    let fleet = fleet_dispatcher(&handles);

    let tasks = vec![
        Task::new(AgentId::new("executor-0"), "say hi"),
        Task::new(AgentId::new("executor-1"), "say hi"),
    ];
    let outcomes = dispatch_all(&fleet, tasks.clone());

    assert_eq!(outcomes.len(), 2, "one outcome per task, in input order");
    for (outcome, task) in outcomes.iter().zip(&tasks) {
        let sent: TaskOutcome = outcome
            .result
            .as_ref()
            .expect("both executors answer")
            .clone();
        assert_eq!(sent.task_id, task.id, "task_id echo");
        // The outcome names the executor that **ran** the task — the child's own
        // identity, not the label the supervisor routed by.
        assert_ne!(
            sent.agent_id, task.target,
            "the outcome names the executor, not the label it was addressed by"
        );
        assert!(
            matches!(sent.outcome, AgentOutcome::Failed { .. }),
            "a child with no LLM answers Failed: {sent:?}"
        );
    }

    // Each outcome's identity is the one that executor announced on its own event
    // channel: the supervisor learns who did the work from the work's answer.
    for (outcome, handle) in outcomes.iter().zip(&handles) {
        let sent = outcome.result.as_ref().expect("answered");
        let announced = child_id(handle).expect("each executor announced itself");
        assert_eq!(sent.agent_id.to_string(), announced);
    }
    assert_eq!(
        tally(&outcomes),
        Tally {
            answered: 2,
            refused: 0,
            broken: 0
        }
    );

    // Two executors really are two processes: each minted its own identity
    // (v0.8 batch B), neither is this process, and they differ from each other.
    let ids: Vec<String> = handles
        .iter()
        .map(|handle| child_id(handle).expect("each executor announced itself"))
        .collect();
    assert_ne!(ids[0], ids[1], "two executors, two identities: {ids:?}");
    for id in &ids {
        let parts: Vec<&str> = id.split('-').collect();
        assert_eq!(parts.len(), 3, "device-pid-seq: {id}");
        assert_eq!(parts[0], "local");
        assert_ne!(
            parts[1],
            std::process::id().to_string(),
            "another process: {id}"
        );
    }
}

#[test]
fn a_task_for_an_executor_that_is_not_in_the_fleet_is_refused() {
    let (specs, _base) = fleet("refused", 2);
    let handles = handles(&specs);
    let fleet = fleet_dispatcher(&handles);

    let tasks = vec![
        Task::new(AgentId::new("executor-0"), "say hi"),
        Task::new(AgentId::new("no-such-executor"), "say hi"),
    ];
    let outcomes = dispatch_all(&fleet, tasks.clone());

    assert_eq!(
        outcomes.len(),
        2,
        "a refusal is still a result for its task"
    );
    assert!(outcomes[0].answered(), "the known executor answered");
    match &outcomes[1].result {
        Err(DispatchError::NoSuchAgent(agent)) => {
            assert_eq!(agent, &AgentId::new("no-such-executor"))
        }
        other => panic!("an unknown target must be refused, got {other:?}"),
    }
    assert_eq!(
        tally(&outcomes),
        Tally {
            answered: 1,
            refused: 1,
            broken: 0
        }
    );

    let report = report(&outcomes);
    assert!(
        report.contains("refused: no executor for no-such-executor"),
        "{report}"
    );
    assert!(
        report.contains("1 answered, 1 refused, 0 broken (of 2)"),
        "{report}"
    );
}

#[test]
fn the_dispatcher_registers_every_executor_label() {
    let (specs, _base) = fleet("labels", 3);
    let fleet = dispatcher(&specs);
    let labels: Vec<&str> = fleet.agent_ids().iter().map(|id| id.as_str()).collect();
    assert_eq!(labels, vec!["executor-0", "executor-1", "executor-2"]);
}

#[test]
fn an_empty_plan_reports_an_empty_tally() {
    let outcomes: Vec<PlanOutcome> = Vec::new();
    assert_eq!(
        report(&outcomes),
        "tally: 0 answered, 0 refused, 0 broken (of 0)"
    );
}
