//! v0.8 — several processes writing one `audit.db`.
//!
//! The multi-agent runtime runs one process per agent against a shared workspace,
//! which means a shared `audit.db`. These tests pin the three things that make
//! that safe: WAL + a busy timeout on the connection, retries instead of lost
//! events, and an error — not a silent drop — when the lock outlasts them.

use audit::{AuditEvent, AuditSink, AuditStore, ChainStatus, SqliteAuditSink};
use rusqlite::Connection;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

fn tmp_db(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("concurrency");
    std::fs::create_dir_all(&dir).expect("create dir");
    let path = dir.join(format!("{name}.db"));
    for suffix in ["", "-wal", "-shm"] {
        let _ = std::fs::remove_file(dir.join(format!("{name}.db{suffix}")));
    }
    path
}

fn ev(ts: i64, actor: &str) -> AuditEvent {
    AuditEvent {
        timestamp_ms: ts,
        actor: actor.into(),
        action: "tick".into(),
        detail: serde_json::json!({ "ts": ts }),
        agent_id: None,
    }
}

#[test]
fn a_file_database_gets_wal_and_a_busy_timeout() {
    let path = tmp_db("wal");
    let store = AuditStore::open(&path).expect("open");
    assert_eq!(store.journal_mode().expect("journal_mode"), "wal");
    assert_eq!(
        store.busy_timeout_ms().expect("busy_timeout"),
        audit::BUSY_TIMEOUT.as_millis() as i64
    );

    // WAL is a property of the file: a second connection sees it too.
    let second = AuditStore::open(&path).expect("open again");
    assert_eq!(second.journal_mode().expect("journal_mode"), "wal");
}

#[test]
fn two_connections_writing_one_file_lose_nothing() {
    let path = tmp_db("two-conn");
    const PER_THREAD: i64 = 50;

    let mut handles = Vec::new();
    for thread in 0..2i64 {
        let path = path.clone();
        handles.push(std::thread::spawn(move || {
            let mut store = AuditStore::open(&path).expect("open");
            for i in 0..PER_THREAD {
                store
                    .append(ev(thread * 1000 + i, &format!("writer{thread}")))
                    .expect("append");
            }
        }));
    }
    for handle in handles {
        handle.join().expect("join");
    }

    let store = AuditStore::open(&path).expect("open");
    assert_eq!(
        store.count().expect("count"),
        (PER_THREAD * 2) as usize,
        "every event from every process reached the chain"
    );
    assert_eq!(
        audit::verify_chain(&store).expect("verify"),
        ChainStatus::Intact {
            length: (PER_THREAD * 2) as usize
        }
    );
}

#[test]
fn a_lock_that_outlasts_the_retries_is_an_error_not_a_lost_event() {
    let path = tmp_db("locked");
    let mut store = AuditStore::open(&path).expect("open");
    // Fail fast: the production budget is five seconds per attempt.
    store
        .set_busy_timeout(Duration::from_millis(30))
        .expect("busy timeout");

    // Another connection holds the write lock for the whole test.
    let holder = Connection::open(&path).expect("holder");
    holder
        .execute_batch("BEGIN EXCLUSIVE")
        .expect("take the write lock");

    let error = store
        .append_with(ev(1, "sandbox"), 1, Duration::from_millis(1))
        .expect_err("a locked database must not be reported as success");
    let message = error.to_string();
    assert!(
        message.contains("locked") || message.contains("busy"),
        "expected a lock error, got: {message}"
    );

    holder.execute_batch("ROLLBACK").expect("release");
    // With the lock gone the same append succeeds: nothing was left half-written.
    store.append(ev(2, "sandbox")).expect("append");
    assert_eq!(store.count().expect("count"), 1);
}

#[test]
fn the_sink_reports_a_write_it_could_not_make() {
    let path = tmp_db("reporter");
    let store = AuditStore::open(&path).expect("open");
    store
        .set_busy_timeout(Duration::from_millis(30))
        .expect("busy timeout");
    let shared = Arc::new(Mutex::new(store));

    let reported = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&reported);
    let sink = SqliteAuditSink::from_shared(Arc::clone(&shared)).with_reporter(Arc::new(
        move |_error: &audit::AuditError| {
            counter.fetch_add(1, Ordering::Relaxed);
        },
    ));
    let mut sink = sink;

    let holder = Connection::open(&path).expect("holder");
    holder
        .execute_batch("BEGIN EXCLUSIVE")
        .expect("take the write lock");

    let result = sink.record(ev(1, "sandbox"));
    assert!(result.is_err(), "the sink must return the failure");
    assert_eq!(
        reported.load(Ordering::Relaxed),
        1,
        "and tell its reporter, so the host can alert"
    );

    holder.execute_batch("ROLLBACK").expect("release");
    holder
        .execute_batch("PRAGMA busy_timeout = 5000")
        .expect("timeout");
}
