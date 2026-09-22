//! Stage 16c — host forwards the LLM stream as `agent:stream:*` events.
//!
//! Uses a pure-text mock response, so no QEMU / toolchain is required and the
//! test runs in CI.

use agent::llm::MockLlm;
use agent::message::{ChatMessage, ChatResponse, Choice};
use host_core::events::{
    RecordingEventSink, EV_AGENT_FINAL, EV_AGENT_STREAM_DELTA, EV_AGENT_STREAM_DONE,
};
use host_core::state::AppState;
use std::sync::Arc;

const TEXT: &str = "HELLO STREAM OK";

fn unique_dir(tag: &str) -> std::path::PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "riscdom-stream-{tag}-{}-{nanos}",
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
fn stream_deltas_are_forwarded_then_done_then_final() {
    let state = AppState::in_memory(unique_dir("fwd")).expect("state");
    *state.llm_override.lock().unwrap() = Some(Arc::new(MockLlm::new(vec![final_response(TEXT)])));

    let sink = Arc::new(RecordingEventSink::new());
    state
        .run_agent(sink.clone() as Arc<dyn host_core::EventSink>, "hi")
        .expect("run_agent");

    let events = sink.events();

    // Deltas reassemble the full content.
    let text: String = events
        .iter()
        .filter(|(name, _)| name == EV_AGENT_STREAM_DELTA)
        .filter_map(|(_, payload)| {
            payload
                .get("text")
                .and_then(|v| v.as_str())
                .map(String::from)
        })
        .collect();
    assert_eq!(text, TEXT, "delta text mismatch: {events:?}");
    assert!(
        sink.count(EV_AGENT_STREAM_DELTA) >= 2,
        "expected several deltas, got events: {:?}",
        events.iter().map(|(n, _)| n.clone()).collect::<Vec<_>>()
    );

    // Exactly one done, and it arrives after the deltas.
    assert_eq!(sink.count(EV_AGENT_STREAM_DONE), 1, "{events:?}");
    let done_at = events
        .iter()
        .position(|(name, _)| name == EV_AGENT_STREAM_DONE)
        .expect("done");
    let last_delta_at = events
        .iter()
        .rposition(|(name, _)| name == EV_AGENT_STREAM_DELTA)
        .expect("delta");
    assert!(last_delta_at < done_at, "done must follow deltas");

    // `agent:final` is emitted after the stream is done.
    let final_at = events
        .iter()
        .position(|(name, _)| name == EV_AGENT_FINAL)
        .expect("final");
    assert!(done_at < final_at, "agent:final must follow stream:done");
}
