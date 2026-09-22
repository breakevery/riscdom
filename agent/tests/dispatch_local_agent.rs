//! v0.8 main deliverable 3/3 — an in-process handle names itself.
//!
//! The outcome of a dispatched task carries the identity of the **executor that
//! ran it**, which the handle fills in. For `LocalAgent` that is the identity of
//! the loop it wraps; there is nobody else it could be, and the dispatcher no
//! longer stamps the target there.

mod common;

use agent::policy::WorkspacePolicy;
use agent::{AgentHandle, AgentLoop, AgentOutcome, LocalAgent, Task};
use common::{final_response, sink, test_config, unique_dir};

#[test]
fn a_local_agent_names_itself_in_the_outcome() {
    let policy = WorkspacePolicy::new(unique_dir("local-agent"));
    let (audit, _shared) = sink();
    let loop_ = AgentLoop::new(
        Box::new(agent::MockLlm::new(vec![final_response("done")])),
        test_config(),
        policy,
        audit,
        "test system prompt".into(),
        agent::next_agent_id(),
    )
    .expect("agent loop");

    let handle = LocalAgent::new(loop_);
    let task = Task::new(handle.agent_id().clone(), "say hi");
    let outcome = handle.run(&task).expect("the loop answers");

    assert_eq!(outcome.task_id, task.id, "the outcome answers the task");
    assert_eq!(
        &outcome.agent_id,
        handle.agent_id(),
        "the outcome names the executor, and the executor is this handle"
    );
    assert!(
        matches!(outcome.outcome, AgentOutcome::Final { .. }),
        "the mocked loop answers Final: {:?}",
        outcome.outcome
    );
}
