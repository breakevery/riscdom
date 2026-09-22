//! v0.8 batch B — the identity an `AgentLoop` is built with reaches its events.
//!
//! The loop does not invent an identity: the caller owns it (the host mints one
//! per instance), and every event the loop writes carries it. That is what lets
//! one chain say which agent caused what once several agents write to it.

mod common;

use agent::llm::MockLlm;
use agent::policy::WorkspacePolicy;
use agent::AgentLoop;
use audit::{verify_chain, ChainStatus};
use common::{final_response, sink, test_config, unique_dir};

#[test]
fn the_injected_agent_id_reaches_every_event() {
    let policy = WorkspacePolicy::new(unique_dir("identity"));
    let (audit, shared) = sink();
    let id = agent::next_agent_id();

    let mut loop_ = AgentLoop::new(
        Box::new(MockLlm::new(vec![final_response("done")])),
        test_config(),
        policy,
        audit,
        "system".into(),
        id.clone(),
    )
    .expect("agent loop");
    assert_eq!(loop_.agent_id(), id);

    loop_.run("hello").expect("run");

    let store = shared.lock().expect("store");
    let events = store.all().expect("all");
    assert!(!events.is_empty(), "a run audits something");
    for event in &events {
        assert_eq!(
            event.event.agent_id.as_deref(),
            Some(id.as_str()),
            "every event carries the loop's id: {:?}",
            event.event.action
        );
    }
    assert!(matches!(
        verify_chain(&store).expect("verify"),
        ChainStatus::Intact { .. }
    ));
}

#[test]
fn two_loops_in_one_process_get_different_identities() {
    let first = agent::next_agent_id();
    let second = agent::next_agent_id();
    assert_ne!(first, second);
}
