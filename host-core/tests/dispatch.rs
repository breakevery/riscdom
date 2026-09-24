//! v0.8 batch C — the local dispatch path.
//!
//! A `Task` goes in through the `Dispatcher` and a `TaskOutcome` comes back, over
//! the host's existing run path; the events that run writes carry the agent
//! identity from batch B. The remote half of the seam is not implemented, so
//! nothing here tests it — there is nothing to test.

use agent::llm::MockLlm;
use agent::message::{ChatMessage, ChatResponse, Choice};
use agent::{AgentHandle, AgentId, DispatchError, Dispatcher, Task};
use host_core::events::RecordingEventSink;
use host_core::state::AppState;
use host_core::EventSink;
use std::path::PathBuf;
use std::sync::Arc;

fn unique_dir(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "riscdom-dispatch-{tag}-{}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn final_response(text: &str) -> ChatResponse {
    ChatResponse {
        id: Some("resp-final".into()),
        model: Some("mock".into()),
        choices: vec![Choice {
            index: Some(0),
            message: ChatMessage::text("assistant", text),
            finish_reason: Some("stop".into()),
        }],
        usage: None,
    }
}

#[test]
#[ignore = "requires a QEMU guest and a RISC-V GCC; run with --include-ignored"]
fn a_task_dispatched_locally_comes_back_as_an_outcome_that_names_the_agent() {
    let state = Arc::new(AppState::in_memory(unique_dir("ok")).expect("state"));
    *state.llm_override.lock().unwrap() = Some(Arc::new(MockLlm::new(vec![final_response("hi")])));
    let sink = Arc::new(RecordingEventSink::new());
    state
        .start_serial_forwarder(Arc::clone(&sink) as Arc<dyn EventSink>)
        .expect("forwarder");

    let dispatcher =
        host_core::local_dispatcher(Arc::clone(&state), Arc::clone(&sink) as Arc<dyn EventSink>);
    let agent_id = AgentId::new(state.agent_id());
    assert_eq!(dispatcher.agent_ids(), vec![&agent_id]);

    let task = Task::new(agent_id.clone(), "say hi");
    let outcome = dispatcher.dispatch(task.clone()).expect("dispatch");

    assert_eq!(outcome.task_id, task.id, "the outcome answers the task");
    assert_eq!(outcome.agent_id, agent_id, "the outcome names the executor");
    match &outcome.outcome {
        agent::AgentOutcome::Final { content, .. } => assert_eq!(content, "hi"),
        other => panic!("expected a final answer, got {other:?}"),
    }

    // The run really went through the host path: it opened and closed a run.
    let runs = state.audit.lock().unwrap().list_runs(10).expect("runs");
    assert_eq!(runs.len(), 1, "one dispatch, one run");

    // And every event that run wrote carries the agent identity (batch B).
    let events = state.list_events(200, None, None).expect("events");
    let stamped = events
        .iter()
        .filter(|event| event.agent_id.as_deref() == Some(agent_id.as_str()))
        .count();
    assert!(stamped > 0, "no event carried the agent id {agent_id}");
    let run_start = events
        .iter()
        .find(|event| event.action == "run.start")
        .expect("the run marker is in the chain");
    assert_eq!(run_start.agent_id.as_deref(), Some(agent_id.as_str()));
}

#[test]
fn a_task_for_an_agent_this_process_does_not_hold_is_refused_not_run() {
    let state = Arc::new(AppState::in_memory(unique_dir("miss")).expect("state"));
    let sink = Arc::new(RecordingEventSink::new());
    let dispatcher = host_core::local_dispatcher(Arc::clone(&state), sink as Arc<dyn EventSink>);

    let stranger = AgentId::new("elsewhere-1-1");
    let error = dispatcher
        .dispatch(Task::new(stranger.clone(), "do it"))
        .expect_err("no executor holds that identity");

    assert_eq!(error, DispatchError::NoSuchAgent(stranger));
    assert!(
        state
            .audit
            .lock()
            .unwrap()
            .list_runs(10)
            .expect("runs")
            .is_empty(),
        "a refused task must not open a run"
    );
}

#[test]
fn a_handle_refuses_a_task_addressed_to_someone_else() {
    let state = Arc::new(AppState::in_memory(unique_dir("handle")).expect("state"));
    let sink = Arc::new(RecordingEventSink::new());
    let handle = host_core::HostAgentHandle::new(Arc::clone(&state), sink as Arc<dyn EventSink>);

    assert_eq!(handle.agent_id(), &AgentId::new(state.agent_id()));

    let other = AgentId::new("elsewhere-2-2");
    assert_eq!(
        handle.run(&Task::new(other.clone(), "x")),
        Err(DispatchError::NoSuchAgent(other))
    );
    assert!(
        state
            .audit
            .lock()
            .unwrap()
            .list_runs(10)
            .expect("runs")
            .is_empty(),
        "the refused task must not open a run"
    );
}
