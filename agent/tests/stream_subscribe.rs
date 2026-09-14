//! Stage 16b — `AgentLoop::subscribe_stream`.

mod common;

use agent::llm::MockLlm;
use agent::message::{ChatMessage, ChatResponse, Choice, StreamEvent};
use agent::policy::WorkspacePolicy;
use agent::prompt::build_system_prompt;
use agent::{AgentLoop, AgentOutcome};
use common::{constitution_path, sink, test_config, unique_dir};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::sync::Arc;
use std::time::{Duration, Instant};

const TEXT: &str = "HELLO STREAM OK";

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

fn agent_loop(tag: &str) -> (AgentLoop, Arc<Mutex<audit::AuditStore>>) {
    let policy = WorkspacePolicy::new(unique_dir(tag));
    let (audit, shared) = sink();
    let system = build_system_prompt(&constitution_path()).expect("system prompt");
    let agent = AgentLoop::new(
        Box::new(MockLlm::new(vec![final_response(TEXT)])),
        test_config(),
        policy,
        audit,
        system,
    )
    .expect("agent loop");
    (agent, shared)
}

use std::sync::Mutex;

fn collect(rx: &Receiver<StreamEvent>, timeout: Duration) -> Vec<StreamEvent> {
    let mut out = Vec::new();
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(event) => {
                let done = matches!(event, StreamEvent::Done);
                out.push(event);
                if done {
                    break;
                }
            }
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
    out
}

fn deltas_text(events: &[StreamEvent]) -> String {
    events
        .iter()
        .filter_map(|e| match e {
            StreamEvent::Delta(d) => Some(d.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn subscriber_receives_deltas_then_done() {
    let (mut agent, shared) = agent_loop("stream-one");
    let rx = agent.subscribe_stream();

    let outcome = agent.run("hello").expect("run");
    let events = collect(&rx, Duration::from_secs(5));

    assert!(matches!(outcome, AgentOutcome::Final { .. }), "{outcome:?}");
    let deltas: Vec<&StreamEvent> = events
        .iter()
        .filter(|e| matches!(e, StreamEvent::Delta(_)))
        .collect();
    assert!(deltas.len() >= 2, "expected several deltas: {events:?}");
    assert_eq!(events.last(), Some(&StreamEvent::Done), "{events:?}");
    assert_eq!(deltas_text(&events), TEXT, "deltas must reassemble TEXT");

    // Audit: exactly one start + one end, and nothing per chunk.
    let store = shared.lock().unwrap();
    let actions: Vec<String> = store
        .all()
        .unwrap()
        .into_iter()
        .map(|e| e.event.action)
        .collect();
    assert_eq!(
        actions
            .iter()
            .filter(|a| *a == "agent.llm.stream.start")
            .count(),
        1,
        "{actions:?}"
    );
    assert_eq!(
        actions
            .iter()
            .filter(|a| *a == "agent.llm.stream.end")
            .count(),
        1,
        "{actions:?}"
    );
    let stream_events = actions
        .iter()
        .filter(|a| a.starts_with("agent.llm.stream."))
        .count();
    assert_eq!(stream_events, 2, "no per-chunk audit events: {actions:?}");
    assert!(actions.iter().any(|a| a == "agent.llm.response"));
}

#[test]
fn multiple_subscribers_receive_the_same_sequence() {
    let (mut agent, _shared) = agent_loop("stream-two");
    let rx1 = agent.subscribe_stream();
    let rx2 = agent.subscribe_stream();

    agent.run("hello").expect("run");

    let first = collect(&rx1, Duration::from_secs(5));
    let second = collect(&rx2, Duration::from_secs(5));
    assert_eq!(first, second, "subscribers must see identical sequences");
    assert_eq!(deltas_text(&first), TEXT);
}
