//! Stage 4a — store basics and concurrent append behaviour.

use audit::{
    verify_chain, AuditError, AuditEvent, AuditSink, AuditStore, ChainStatus, SqliteAuditSink,
    AUDIT_SCHEMA_VERSION,
};
use rusqlite::Connection;
use std::path::PathBuf;
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

// ----- schema version (v1.0 M2b-3a) ------------------------------------------

/// A unique `audit.db` path under the temp directory.
fn temp_db(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "riscdom-audit-{tag}-{}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join("audit.db")
}

#[test]
fn a_fresh_file_is_stamped_and_keeps_no_backup() {
    let db = temp_db("fresh");
    let store = AuditStore::open(&db).expect("open");
    assert_eq!(
        store.schema_version().expect("version"),
        AUDIT_SCHEMA_VERSION
    );
    drop(store);
    // The audit store writes **no** `.bak`: this file is WAL and opened by several
    // processes, so copying `audit.db` alone can miss frames still in `-wal`.
    assert!(!db.with_extension("db.bak").exists());
}

#[test]
fn a_file_from_before_the_marker_is_migrated_on_open() {
    let db = temp_db("migrate");
    // The shape before v1.0 M2b-3a: no version row, and neither of the two columns
    // the earlier batches added.
    {
        let conn = Connection::open(&db).expect("raw open");
        conn.execute_batch(
            "CREATE TABLE audit_events (\n                 id INTEGER PRIMARY KEY AUTOINCREMENT, timestamp_ms INTEGER NOT NULL,\n                 actor TEXT NOT NULL, action TEXT NOT NULL, detail_json TEXT NOT NULL,\n                 prev_hash TEXT NOT NULL, hash TEXT NOT NULL UNIQUE);\n             CREATE TABLE runs (\n                 run_id TEXT PRIMARY KEY, session_id TEXT, parent_run_id TEXT,\n                 fingerprint TEXT NOT NULL, fingerprint_schema TEXT NOT NULL,\n                 started_at_ms INTEGER NOT NULL, ended_at_ms INTEGER,\n                 start_seq INTEGER NOT NULL, end_seq INTEGER, status TEXT NOT NULL);",
        )
        .expect("old shape");
        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .expect("version");
        assert_eq!(version, 0, "the fixture carries no marker");
    }

    let mut store = AuditStore::open(&db).expect("open old");
    assert_eq!(
        store.schema_version().expect("version"),
        AUDIT_SCHEMA_VERSION
    );
    // Both columns arrived and the chain is untouched: a row appended now is the
    // first link of this file's chain, and its hash is the formula's.
    let stored = store.append(ev(1, "sandbox", "vm.start")).expect("append");
    assert!(stored.event.agent_id.is_none(), "producers opt in");
    assert_eq!(stored.prev_hash, audit::GENESIS_PREV_HASH);
    assert_eq!(
        verify_chain(&store).expect("verify"),
        ChainStatus::Intact { length: 1 }
    );
    assert_eq!(
        store.schema_version().expect("version"),
        AUDIT_SCHEMA_VERSION
    );
    drop(store);
    assert!(!db.with_extension("db.bak").exists());
}

#[test]
fn a_marker_whose_columns_are_missing_is_still_migrated() {
    let db = temp_db("half");
    {
        let conn = Connection::open(&db).expect("raw open");
        conn.execute_batch(
            "CREATE TABLE audit_events (\n                 id INTEGER PRIMARY KEY AUTOINCREMENT, timestamp_ms INTEGER NOT NULL,\n                 actor TEXT NOT NULL, action TEXT NOT NULL, detail_json TEXT NOT NULL,\n                 prev_hash TEXT NOT NULL, hash TEXT NOT NULL UNIQUE);\n             CREATE TABLE runs (\n                 run_id TEXT PRIMARY KEY, session_id TEXT, parent_run_id TEXT,\n                 fingerprint TEXT NOT NULL, fingerprint_schema TEXT NOT NULL,\n                 started_at_ms INTEGER NOT NULL, ended_at_ms INTEGER,\n                 start_seq INTEGER NOT NULL, end_seq INTEGER, status TEXT NOT NULL);\n             PRAGMA user_version = 1;",
        )
        .expect("half-migrated fixture");
    }
    // The column work is idempotent, so a file that claims the version without the
    // columns converges on the same shape instead of failing.
    let mut store = AuditStore::open(&db).expect("open half");
    assert_eq!(
        store.schema_version().expect("version"),
        AUDIT_SCHEMA_VERSION
    );
    store
        .append(ev(2, "agent", "llm.request").with_agent("exec-1"))
        .expect("append");
    let read = store.get(1).expect("get").expect("present");
    assert_eq!(read.event.agent_id.as_deref(), Some("exec-1"));
}

#[test]
fn a_newer_file_is_refused() {
    let db = temp_db("newer");
    drop(AuditStore::open(&db).expect("open"));
    {
        let conn = Connection::open(&db).expect("raw open");
        conn.execute_batch("PRAGMA user_version = 99;")
            .expect("stamp 99");
    }

    let refused = AuditStore::open(&db);
    match refused.as_ref() {
        Err(AuditError::DataTooNew { found, supported }) => {
            assert_eq!(*found, 99);
            assert_eq!(*supported, AUDIT_SCHEMA_VERSION);
        }
        Err(e) => panic!("expected DataTooNew, got {e}"),
        Ok(_) => panic!("a file from a newer build must not open"),
    }
}
