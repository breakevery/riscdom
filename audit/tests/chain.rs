//! Stage 4a — hash chain integrity and append-only guarantees.

use audit::{verify_chain, AuditEvent, AuditStore, ChainStatus};
use rusqlite::Connection;
use std::path::PathBuf;

fn tmp_db(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("chain");
    std::fs::create_dir_all(&dir).expect("create dir");
    let path = dir.join(format!("{name}.db"));
    for suffix in ["", "-wal", "-shm"] {
        let _ = std::fs::remove_file(dir.join(format!("{name}.db{suffix}")));
    }
    path
}

fn ev(ts: i64) -> AuditEvent {
    AuditEvent {
        timestamp_ms: ts,
        actor: "sandbox".into(),
        action: "vm.start".into(),
        detail: serde_json::json!({ "n": ts }),
    }
}

#[test]
fn empty_store_is_intact_with_length_zero() {
    let store = AuditStore::in_memory().expect("in-memory store");
    assert_eq!(
        verify_chain(&store).expect("verify"),
        ChainStatus::Intact { length: 0 }
    );
}

#[test]
fn three_events_chain_is_intact() {
    let mut store = AuditStore::in_memory().expect("in-memory store");
    for i in 0..3 {
        store.append(ev(i + 1)).expect("append");
    }
    assert_eq!(
        verify_chain(&store).expect("verify"),
        ChainStatus::Intact { length: 3 }
    );
}

#[test]
fn tampered_event_is_detected_at_its_id() {
    let path = tmp_db("tamper");
    {
        let mut store = AuditStore::open(&path).expect("open store");
        for i in 0..3 {
            store.append(ev(i + 1)).expect("append");
        }
    }

    // Bypass the API *and* the trigger: drop the guard, then edit row #2.
    {
        let conn = Connection::open(&path).expect("raw connection");
        conn.execute_batch(
            "DROP TRIGGER audit_no_update; \
             UPDATE audit_events SET action = 'evil' WHERE id = 2;",
        )
        .expect("tamper");
    }

    let store = AuditStore::open(&path).expect("reopen store");
    match verify_chain(&store).expect("verify") {
        ChainStatus::Broken { at_id, reason } => {
            assert_eq!(at_id, 2, "expected break at id 2, reason={reason}");
        }
        other => panic!("expected Broken at id 2, got {other:?}"),
    }
}

#[test]
fn update_and_delete_are_rejected_by_triggers() {
    let path = tmp_db("noupdate");
    {
        let mut store = AuditStore::open(&path).expect("open store");
        store.append(ev(1)).expect("append");
    }

    let conn = Connection::open(&path).expect("raw connection");

    let update = conn.execute("UPDATE audit_events SET action = 'x' WHERE id = 1", []);
    assert!(update.is_err(), "UPDATE must be rejected by trigger");
    let msg = format!("{}", update.unwrap_err());
    assert!(
        msg.contains("append-only"),
        "unexpected UPDATE error: {msg}"
    );

    let delete = conn.execute("DELETE FROM audit_events WHERE id = 1", []);
    assert!(delete.is_err(), "DELETE must be rejected by trigger");
    let msg = format!("{}", delete.unwrap_err());
    assert!(
        msg.contains("append-only"),
        "unexpected DELETE error: {msg}"
    );

    // The row is still there.
    let n: i64 = conn
        .query_row("SELECT COUNT(*) FROM audit_events", [], |r| r.get(0))
        .expect("count");
    assert_eq!(n, 1);
}
