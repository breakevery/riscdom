//! v0.8 — audit write failures reach the host, and the alert setting gates only
//! the shouting, never the event.
//!
//! The sink's reporter queues a failure inside `AppState`; this file exercises
//! what the host does with the queue, without arranging a real database lock
//! (the sink → reporter wiring is tested in the `audit` crate).

use host::events::{RecordingEventSink, EV_AUDIT_FAILED};
use host::state::AppState;
use std::path::PathBuf;

fn unique_dir(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "riscdom-audit-alert-{tag}-{}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn a_queued_failure_is_announced_once_and_then_taken() {
    let state = AppState::in_memory(unique_dir("once")).expect("state");
    let sink = RecordingEventSink::new();

    state.push_audit_failure("sqlite error: database is locked");
    assert_eq!(state.audit_failures().len(), 1);

    // First announcement: one event, carrying the message.
    assert_eq!(state.emit_audit_failures(&sink), 1);
    assert_eq!(sink.count(EV_AUDIT_FAILED), 1);
    let payload = sink.events()[0].1.clone();
    assert_eq!(
        payload.get("error").and_then(|v| v.as_str()),
        Some("sqlite error: database is locked")
    );

    // Not announced twice: the cursor moved.
    assert_eq!(state.emit_audit_failures(&sink), 0);
    assert_eq!(sink.count(EV_AUDIT_FAILED), 1);

    // Taking it clears both the queue and the cursor, so the next failure starts
    // a fresh alert.
    assert_eq!(
        state.take_audit_failures(),
        vec!["sqlite error: database is locked".to_string()]
    );
    assert!(state.audit_failures().is_empty());

    state.push_audit_failure("second");
    assert_eq!(state.emit_audit_failures(&sink), 1);
    assert_eq!(sink.count(EV_AUDIT_FAILED), 2);
}

#[test]
fn the_event_is_sent_even_when_the_alert_is_off() {
    // The setting gates the banner and the popup, never the event.
    let state = AppState::in_memory(unique_dir("off")).expect("state");
    state
        .set_alert_on_audit_failure(false)
        .expect("turn the alert off");

    let sink = RecordingEventSink::new();
    state.push_audit_failure("boom");
    assert_eq!(state.emit_audit_failures(&sink), 1);
    assert_eq!(sink.count(EV_AUDIT_FAILED), 1);
    assert!(!state.audit_status().expect("status").alert_on_failure);
}

#[test]
fn the_failure_queue_stays_bounded() {
    let state = AppState::in_memory(unique_dir("cap")).expect("state");
    for i in 0..(host::state::AUDIT_FAILURE_QUEUE_CAP + 5) {
        state.push_audit_failure(&format!("failure {i}"));
    }
    let failures = state.audit_failures();
    assert_eq!(failures.len(), host::state::AUDIT_FAILURE_QUEUE_CAP);
    // The newest ones survive: they describe the current state.
    let expected = format!("failure {}", host::state::AUDIT_FAILURE_QUEUE_CAP + 4);
    assert_eq!(failures.last().map(String::as_str), Some(expected.as_str()));
}
