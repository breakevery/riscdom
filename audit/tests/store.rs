//! Stage 4a — store basics and concurrent append behaviour.

use audit::{verify_chain, AuditEvent, AuditSink, AuditStore, ChainStatus, SqliteAuditSink};
use std::sync::{Arc, Mutex};
use std::thread;

fn ev(ts: i64, actor: &str, action: &str) -> AuditEvent {
    AuditEvent {
        timestamp_ms: ts,
        actor: actor.into(),
        action: action.into(),
        detail: serde_json::json!({ "ts": ts }),
        agent_id: None,
    }
}

#[test]
fn last_hash_matches_first_append() {
    let mut store = AuditStore::in_memory().expect("store");
    assert_eq!(store.last_hash().expect("last_hash"), None);
    let stored = store.append(ev(1, "sandbox", "vm.start")).expect("append");
    assert_eq!(
        store.last_hash().expect("last_hash"),
        Some(stored.hash.clone())
    );
    assert_eq!(store.count().expect("count"), 1);
}

#[test]
fn genesis_prev_hash_is_all_zeroes() {
    let mut store = AuditStore::in_memory().expect("store");
    let stored = store.append(ev(1, "sandbox", "vm.start")).expect("append");
    assert_eq!(stored.prev_hash, audit::GENESIS_PREV_HASH);
    assert_eq!(stored.prev_hash.len(), 64);
}

#[test]
fn agent_id_is_stored_beside_the_chain_and_leaves_it_intact() {
    let mut store = AuditStore::in_memory().expect("store");
    let plain = store.append(ev(1, "sandbox", "vm.start")).expect("append");
    assert_eq!(plain.event.agent_id, None, "producers opt in");

    let stamped = store
        .append(ev(2, "agent", "llm.request").with_agent("exec-1"))
        .expect("append");
    assert_eq!(stamped.event.agent_id.as_deref(), Some("exec-1"));

    // It round-trips through SQLite and the hash is the one the unchanged formula
    // gives (agent_id is not part of it).
    let read = store.get(stamped.id).expect("get").expect("present");
    assert_eq!(read.event.agent_id.as_deref(), Some("exec-1"));
    assert_eq!(read.hash, stamped.hash);

    assert_eq!(
        verify_chain(&store).expect("verify"),
        ChainStatus::Intact { length: 2 }
    );
}

#[test]
fn concurrent_appends_keep_chain_intact() {
    let shared = Arc::new(Mutex::new(AuditStore::in_memory().expect("store")));
    let sink = SqliteAuditSink::from_shared(Arc::clone(&shared));
    let audit: Arc<Mutex<dyn AuditSink>> = Arc::new(Mutex::new(sink));

    let mut handles = Vec::new();
    for t in 0..4i64 {
        let audit = Arc::clone(&audit);
        handles.push(thread::spawn(move || {
            for i in 0..25i64 {
                audit
                    .lock()
                    .expect("lock")
                    .record(ev(t * 100 + i, "sandbox", "tick"))
                    .expect("record");
            }
        }));
    }
    for h in handles {
        h.join().expect("join");
    }

    let store = shared.lock().expect("lock store");
    assert_eq!(store.count().expect("count"), 100);
    assert_eq!(
        verify_chain(&store).expect("verify"),
        ChainStatus::Intact { length: 100 }
    );
}
