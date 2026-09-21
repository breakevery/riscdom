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
        agent_id: None,
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
fn a_pre_v08_database_gains_agent_id_without_touching_the_chain() {
    let path = tmp_db("agent_id_migration");

    // A database written before v0.8: the event table has no `agent_id` column.
    {
        let conn = Connection::open(&path).expect("raw connection");
        conn.execute_batch(
            "CREATE TABLE audit_events (\n                 id           INTEGER PRIMARY KEY AUTOINCREMENT,\n                 timestamp_ms INTEGER NOT NULL,\n                 actor        TEXT    NOT NULL,\n                 action       TEXT    NOT NULL,\n                 detail_json  TEXT    NOT NULL,\n                 prev_hash    TEXT    NOT NULL,\n                 hash         TEXT    NOT NULL UNIQUE\n             );",
        )
        .expect("old schema");
    }

    // One event, hashed with the *unchanged* formula.
    let prev = audit::GENESIS_PREV_HASH.to_string();
    let detail_json = "{\"n\":1}";
    let event = AuditEvent {
        timestamp_ms: 1,
        actor: "sandbox".into(),
        action: "vm.start".into(),
        detail: serde_json::json!({ "n": 1 }),
        agent_id: None,
    };
    let hash = audit::compute_hash(&prev, &event, detail_json);
    {
        let conn = Connection::open(&path).expect("raw connection");
        conn.execute(
            "INSERT INTO audit_events (timestamp_ms, actor, action, detail_json, prev_hash, hash) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![1i64, "sandbox", "vm.start", detail_json, prev, hash],
        )
        .expect("insert old row");
    }

    // Opening it through the store runs the migration; the old chain still verifies.
    let store = AuditStore::open(&path).expect("open store");
    assert_eq!(
        verify_chain(&store).expect("verify"),
        ChainStatus::Intact { length: 1 }
    );
    let row = store.get(1).expect("get").expect("present");
    assert_eq!(row.event.agent_id, None, "an old row has no agent identity");

    let conn = Connection::open(&path).expect("raw connection");
    let cols: Vec<String> = {
        let mut stmt = conn
            .prepare("PRAGMA table_info(audit_events)")
            .expect("pragma");
        let rows = stmt
            .query_map([], |r| r.get::<_, String>(1))
            .expect("query");
        rows.map(|r| r.expect("col")).collect()
    };
    assert!(
        cols.iter().any(|c| c == "agent_id"),
        "column added: {cols:?}"
    );
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
