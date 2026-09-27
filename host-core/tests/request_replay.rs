//! The request queue's past is derived from the chain (v1.0 gap 3/N, batch D).
//!
//! The queue is **runtime state**: it lives in a `Vec` behind a `Mutex` and dies with the
//! process. What survives is the chain's `m.request.ask` / `m.request.approve` /
//! `m.request.reject` rows, and a restart folds them back into the live queue (decisions
//! §84). These tests drive both halves: the pure derivation over a chain, and the restart
//! it makes survivable — through a **file-backed** store, so "process two" really is a
//! second reader of the same chain.
//!
//! The rows are written here in the shapes the host writes them (`emit_m_action`), because
//! the derivation is only as good as those shapes: `{id, action, sandbox}` on the ask, and
//! `{id, action, sandbox, decided_by}` on a decision, with the requester/decider in the
//! row's `agent_id`.

use audit::{AuditEvent, AuditStore};
use host_core::events::RecordingEventSink;
use host_core::sandbox_request::{
    derive_requests_from, ReconciledRequest, SandboxAction, SandboxRequestService,
    SandboxRequestStatus, SandboxRequests,
};
use host_core::EventSink;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

fn unique_dir(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "riscdom-request-replay-{tag}-{}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn sink() -> Arc<dyn EventSink> {
    Arc::new(RecordingEventSink::new())
}

fn queue() -> SandboxRequests {
    SandboxRequests::new(Arc::new(Mutex::new(None)))
}

/// One `m.request.ask` row, in the host's shape.
fn ask(store: &mut AuditStore, id: &str, action: &str, sandbox: Option<&str>, by: &str) {
    let detail = serde_json::json!({ "id": id, "action": action, "sandbox": sandbox });
    store
        .append(AuditEvent::new("host", "m.request.ask", detail).with_agent(by))
        .expect("ask row");
}

/// One decision row, in the host's shape.
fn decide(store: &mut AuditStore, id: &str, verb: &str, action: &str, by: &str) {
    let detail =
        serde_json::json!({ "id": id, "action": action, "sandbox": null, "decided_by": by });
    store
        .append(AuditEvent::new("host", verb, detail).with_agent(by))
        .expect("decision row");
}

#[test]
fn an_empty_chain_derives_nothing() {
    // Every chain written before v1.0 gap 2/N has no ask rows at all: the answer is an
    // empty list, not a failure.
    let store = AuditStore::in_memory().expect("store");
    assert!(derive_requests_from(&store.all().expect("all")).is_empty());
}

#[test]
fn an_ask_is_pending_and_carries_what_the_chain_has() {
    let mut store = AuditStore::in_memory().expect("store");
    ask(&mut store, "req-9-1", "switch", Some("blink"), "m-1");

    let derived = derive_requests_from(&store.all().expect("all"));
    assert_eq!(derived.len(), 1);
    let record = &derived[0];
    assert_eq!(record.id, "req-9-1");
    assert_eq!(record.requester_agent_id, "m-1");
    assert_eq!(record.action, SandboxAction::Switch);
    assert_eq!(record.sandbox.as_deref(), Some("blink"));
    assert_eq!(record.status, SandboxRequestStatus::Pending);
    assert!(record.decided_by.is_none());
    assert!(record.decided_at_ms.is_none());
    assert!(record.requested_at_ms > 0);
    // The chain never carried a reason: the field is `None`, not invented.
    assert!(record.reason.is_none());
}

#[test]
fn a_decision_folds_the_record_forward() {
    let mut store = AuditStore::in_memory().expect("store");
    ask(&mut store, "req-9-1", "switch", Some("blink"), "m-1");
    ask(&mut store, "req-9-2", "assemble", None, "m-2");
    decide(&mut store, "req-9-1", "m.request.approve", "switch", "m-3");
    decide(&mut store, "req-9-2", "m.request.reject", "assemble", "m-4");

    let derived = derive_requests_from(&store.all().expect("all"));
    assert_eq!(derived.len(), 2, "{derived:?}");
    assert_eq!(derived[0].status, SandboxRequestStatus::Approved);
    assert_eq!(derived[0].decided_by.as_deref(), Some("m-3"));
    assert!(derived[0].decided_at_ms.is_some());
    assert_eq!(derived[1].status, SandboxRequestStatus::Rejected);
    assert_eq!(derived[1].decided_by.as_deref(), Some("m-4"));
    // The ask's own fields survive the fold.
    assert_eq!(derived[1].sandbox, None);
    assert_eq!(derived[1].action, SandboxAction::Assemble);
}

#[test]
fn a_decision_without_an_ask_is_not_a_record() {
    // An orphan: a decision row whose ask is not in this chain (a truncated export, a
    // hand-made store). It is skipped rather than turned into a half-record.
    let mut store = AuditStore::in_memory().expect("store");
    decide(&mut store, "req-9-9", "m.request.approve", "switch", "m-3");
    assert!(derive_requests_from(&store.all().expect("all")).is_empty());
}

#[test]
fn a_restart_restores_the_pending_ask_and_it_can_still_be_decided() {
    // The point of the batch: an ask left pending by one process is still decidable by the
    // next one. The store is **file-backed**, so "the next one" reads the same chain.
    let db = unique_dir("restart").join("audit.db");
    {
        let mut store = AuditStore::open(&db).expect("store");
        ask(&mut store, "req-9-1", "switch", Some("blink"), "m-1");
    }

    let store = AuditStore::open(&db).expect("store again");
    let derived = derive_requests_from(&store.all().expect("all"));
    assert_eq!(derived.len(), 1);

    // A fresh queue, seeded the way `AppState` seeds it at startup.
    let queue = queue();
    let (restored, conflicts) = queue.restore(&derived);
    assert_eq!(restored, 1);
    assert!(conflicts.is_empty());

    let waiting = queue.pending();
    assert_eq!(waiting.len(), 1);
    assert_eq!(waiting[0].id, "req-9-1");
    assert_eq!(waiting[0].requester_agent_id, "m-1");
    assert_eq!(waiting[0].action, "switch");

    // …and the decision a lost process never got to make still works.
    let service = SandboxRequestService::new(queue.clone(), sink());
    let decided = service
        .decide("req-9-1", SandboxRequestStatus::Approved, "operator")
        .expect("decide after restart");
    assert_eq!(decided.status, "approved");
    assert!(queue.pending().is_empty());
}

#[test]
fn a_decided_request_does_not_come_back_live() {
    // Only pending asks return: a decision is history, and history is the chain's. The
    // chain still knows it — `derive_requests_from` answers it — but the live queue does
    // not pretend it is waiting again.
    let db = unique_dir("decided").join("audit.db");
    {
        let mut store = AuditStore::open(&db).expect("store");
        ask(&mut store, "req-9-1", "switch", Some("blink"), "m-1");
        decide(&mut store, "req-9-1", "m.request.approve", "switch", "m-3");
    }

    let store = AuditStore::open(&db).expect("store again");
    let derived = derive_requests_from(&store.all().expect("all"));
    assert_eq!(derived.len(), 1);
    assert_eq!(derived[0].status, SandboxRequestStatus::Approved);

    let queue = queue();
    let (restored, conflicts) = queue.restore(&derived);
    assert_eq!(restored, 0, "a decided ask is not waiting");
    assert!(conflicts.is_empty());
    assert!(queue.list(None).is_empty());
}

#[test]
fn a_restore_never_overwrites_an_id_that_is_already_queued() {
    // A collision is reported, never resolved: replacing one queued ask with another is
    // the quiet loss the queue exists to prevent.
    let queue = queue();
    let queued = queue
        .enqueue(
            "m-7",
            SandboxAction::Switch,
            Some("blink".into()),
            None,
            None,
        )
        .expect("enqueue");
    let record = ReconciledRequest {
        id: queued.id.clone(),
        requester_agent_id: "m-8".into(),
        action: SandboxAction::Switch,
        sandbox: Some("other".into()),
        status: SandboxRequestStatus::Pending,
        decided_by: None,
        requested_at_ms: 1,
        decided_at_ms: None,
        reason: None,
    };

    let (restored, conflicts) = queue.restore(&[record]);
    assert_eq!(restored, 0);
    assert_eq!(conflicts, vec![queued.id.clone()]);
    let listed = queue.list(None);
    assert_eq!(listed.len(), 1);
    assert_eq!(
        listed[0].requester_agent_id, "m-7",
        "the queued ask stands: {listed:?}"
    );
    assert_eq!(listed[0].sandbox.as_deref(), Some("blink"));
}
