//! v1.0 gap 2/N — every act a caller can take leaves a row that names it.
//!
//! The batch's claim is one sentence long: **the chain says who asked**. These tests drive
//! each act that needs no QEMU (the derive's own row is asserted where a VM really starts, on
//! the real machine) and read the row back out of the chain — its `action` and its `agent_id`.
//!
//! `agent_id` is the field the batch uses on purpose: it is deliberately **outside** the hash
//! formula (`audit/src/event.rs`), so naming the caller adds attribution without moving a
//! single historical row, and `verify_chain` recomputes every old row exactly as before.

use host_core::events::RecordingEventSink;
use host_core::sandbox_request::SandboxAction;
use host_core::state::AppState;
use host_core::{EventSink, StoredEventView};
use std::path::PathBuf;
use std::sync::Arc;

fn unique_dir(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "riscdom-mactions-{tag}-{}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn state(tag: &str) -> AppState {
    AppState::in_memory(unique_dir(tag)).expect("state")
}

fn sink() -> Arc<dyn EventSink> {
    Arc::new(RecordingEventSink::new())
}

/// The newest row with this `action`, read straight out of the chain.
fn row_for(state: &AppState, action: &str) -> Option<StoredEventView> {
    state
        .list_events(100, None, Some(action.to_string()))
        .expect("events")
        .into_iter()
        .find(|row| row.action == action)
}

#[test]
fn a_reap_names_the_caller_and_a_derive_names_it_too() {
    let app = state("reap");
    let instance = app.register_instance("scratch");

    app.stop_instance(&instance.id, Some("m-1"), sink())
        .expect("stop");

    let row = row_for(&app, "m.sandbox.reap").expect("a reap row");
    assert_eq!(row.agent_id.as_deref(), Some("m-1"), "{row:?}");
    assert_eq!(row.actor, "host", "the node acted: {row:?}");
    assert_eq!(row.detail["instance_id"], instance.id.as_str(), "{row:?}");
}

#[test]
fn without_a_caller_a_row_still_names_the_node() {
    // The behaviour every release before this one had: the node's own identity. Only the
    // acts an outside caller can take grew the caller; the forty-odd node events never had
    // one to record.
    let app = state("no-caller");
    let instance = app.register_instance("scratch");
    app.stop_instance(&instance.id, None, sink()).expect("stop");
    let row = row_for(&app, "m.sandbox.reap").expect("a reap row");
    assert_eq!(row.agent_id.as_deref(), Some(app.agent_id()), "{row:?}");

    app.set_theme("dark").expect("the theme is set");
    let row = row_for(&app, "host.theme.set").expect("a theme row");
    assert_eq!(row.agent_id.as_deref(), Some(app.agent_id()), "{row:?}");
}

#[test]
fn an_ask_and_its_decision_both_name_their_actor() {
    let app = state("requests");

    let asked = app
        .request_sandbox(
            "m-2",
            SandboxAction::Switch,
            Some("blink".into()),
            None,
            Some("because".into()),
            sink(),
        )
        .expect("the ask is recorded");
    let row = row_for(&app, "m.request.ask").expect("an ask row");
    assert_eq!(row.agent_id.as_deref(), Some("m-2"), "{row:?}");
    assert_eq!(row.detail["id"], asked.id.as_str(), "{row:?}");

    app.approve_sandbox_request(&asked.id, "m-3", sink())
        .expect("approve");
    let row = row_for(&app, "m.request.approve").expect("an approve row");
    assert_eq!(row.agent_id.as_deref(), Some("m-3"), "{row:?}");
    assert_eq!(row.detail["decided_by"], "m-3", "{row:?}");

    let second = app
        .request_sandbox("m-2", SandboxAction::Assemble, None, None, None, sink())
        .expect("the ask is recorded");
    app.reject_sandbox_request(&second.id, "m-4", sink())
        .expect("reject");
    let row = row_for(&app, "m.request.reject").expect("a reject row");
    assert_eq!(row.agent_id.as_deref(), Some("m-4"), "{row:?}");
    assert_eq!(row.detail["decided_by"], "m-4", "{row:?}");
}

#[test]
fn a_refused_switch_leaves_no_row_and_the_stream_says_so() {
    // A switch to a definition nobody has is the caller's `404`, and F2b-2's promise still
    // holds: the refusal touches nothing — no VM, no current definition, **no row**. What
    // this batch adds for a switch is the row a **successful** one leaves (`m.sandbox.switch`),
    // which needs a VM and is asserted on the real machine; the stream keeps its one frame
    // per attempt either way.
    let app = state("switch");
    let sink = Arc::new(RecordingEventSink::new());
    let before = app.audit_status().expect("status").count;
    assert!(
        app.switch_sandbox(
            "no-such-definition",
            Some("m-5"),
            Arc::clone(&sink) as Arc<dyn EventSink>
        )
        .is_err(),
        "there is no such definition"
    );
    assert_eq!(
        app.audit_status().expect("status").count,
        before,
        "a refused switch writes nothing"
    );
    assert!(
        row_for(&app, "m.sandbox.switch").is_none(),
        "and no switch row"
    );
    let frames = sink.events();
    assert_eq!(frames.len(), 1, "one frame per attempt: {frames:?}");
    assert_eq!(frames[0].0, host_core::EV_SANDBOX_SWITCH, "{frames:?}");
    assert_eq!(frames[0].1["ok"], false, "{frames:?}");
}

#[test]
fn the_chain_still_verifies_with_the_caller_rows_in_it() {
    // The batch's red line, as a test: new rows with a caller name are ordinary rows, and the
    // chain over them is intact.
    let app = state("verify");
    let instance = app.register_instance("scratch");
    app.stop_instance(&instance.id, Some("m-6"), sink())
        .expect("stop");
    app.request_sandbox("m-6", SandboxAction::Switch, None, None, None, sink())
        .expect("ask");
    let status = app.audit_status().expect("status");
    match status.chain {
        host_core::ChainStatusView::Intact { length } => assert!(length >= 2, "{length}"),
        other => panic!("expected Intact, got {other:?}"),
    }
}
