//! v0.8 — several processes writing one `audit.db`.
//!
//! The multi-agent runtime runs one process per agent against a shared workspace,
//! which means a shared `audit.db`. These tests pin the three things that make
//! that safe: WAL + a busy timeout on the connection, retries instead of lost
//! events, and an error — not a silent drop — when the lock outlasts them.

use audit::{verify_chain, AuditEvent, AuditSink, AuditStore, ChainStatus, SqliteAuditSink};
use rusqlite::Connection;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Barrier, Mutex};
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

/// Every connection that opens the same **new** file at the same moment succeeds.
///
/// Opening is not only `busy_timeout` + WAL: it also creates the schema and
/// migrates the tables, and some of those statements answer `SQLITE_BUSY`
/// *without* consulting the busy handler — the journal-mode switch is the
/// documented one. Before this test existed, the race showed up as a one-in-three
/// flake in the gate, reported as `open: database is locked`.
#[test]
fn many_connections_open_one_new_file_at_once() {
    const THREADS: usize = 8;
    const ROUNDS: usize = 25;
    let mut opened = 0usize;
    for round in 0..ROUNDS {
        let path = tmp_db(&format!("open-race-new-{round}"));
        let barrier = Arc::new(Barrier::new(THREADS));
        let mut handles = Vec::new();
        for _ in 0..THREADS {
            let path = path.clone();
            let barrier = Arc::clone(&barrier);
            handles.push(std::thread::spawn(move || {
                barrier.wait();
                AuditStore::open(&path)
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            }));
        }
        for handle in handles {
            let result = handle.join().expect("join");
            assert!(result.is_ok(), "round {round}: open failed: {result:?}");
            opened += 1;
        }
    }
    assert_eq!(opened, THREADS * ROUNDS);
}

/// The same race against a file that is **already** WAL (the steady state).
#[test]
fn many_connections_open_one_existing_file_at_once() {
    const THREADS: usize = 8;
    const ROUNDS: usize = 25;
    for round in 0..ROUNDS {
        let path = tmp_db(&format!("open-race-existing-{round}"));
        // Create it (and switch it to WAL) once, the way a long-lived host does.
        AuditStore::open(&path).expect("first open");
        let barrier = Arc::new(Barrier::new(THREADS));
        let mut handles = Vec::new();
        for _ in 0..THREADS {
            let path = path.clone();
            let barrier = Arc::clone(&barrier);
            handles.push(std::thread::spawn(move || {
                barrier.wait();
                AuditStore::open(&path)
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            }));
        }
        for handle in handles {
            let result = handle.join().expect("join");
            assert!(result.is_ok(), "round {round}: open failed: {result:?}");
        }
    }
}

/// An existing database that was never switched to WAL is switched on open.
#[test]
fn an_existing_non_wal_file_is_switched_on_open() {
    let path = tmp_db("legacy-journal");
    {
        let plain = Connection::open(&path).expect("plain");
        plain
            .execute_batch("CREATE TABLE t(x); PRAGMA journal_mode = DELETE;")
            .expect("legacy database");
        let mode: String = plain
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .expect("mode");
        assert_eq!(mode, "delete");
    }

    let store = AuditStore::open(&path).expect("open");
    assert_eq!(store.journal_mode().expect("mode"), "wal");
    assert_eq!(store.count().expect("count"), 0);
}

/// An open that cannot switch to WAL inside its budget is an **error**, never a
/// silent fallback to the rollback journal.
#[test]
fn an_open_that_cannot_switch_to_wal_is_reported_not_ignored() {
    let path = tmp_db("wal-blocked");
    {
        let plain = Connection::open(&path).expect("plain");
        plain
            .execute_batch("CREATE TABLE t(x);")
            .expect("legacy schema");
    }
    let holder = Connection::open(&path).expect("holder");
    holder
        .execute_batch("BEGIN EXCLUSIVE")
        .expect("take the write lock");

    let error = AuditStore::open_with(&path, 1, Duration::from_millis(1))
        .err()
        .expect("the budget is spent, so the failure must be reported");
    let message = error.to_string();
    assert!(
        message.contains("locked") || message.contains("busy"),
        "expected a lock error, got: {message}"
    );

    holder.execute_batch("ROLLBACK").expect("release");
    // Free again, the same open succeeds — and it did switch to WAL.
    let store = AuditStore::open(&path).expect("open after the lock is gone");
    assert_eq!(store.journal_mode().expect("mode"), "wal");
}

/// Opening and appending at once loses nothing: the events are the assertion.
#[test]
fn opening_and_appending_at_the_same_time_loses_nothing() {
    const WRITERS: i64 = 3;
    const OPENERS: usize = 3;
    const PER_WRITER: i64 = 40;
    let path = tmp_db("open-plus-write");
    let barrier = Arc::new(Barrier::new(WRITERS as usize + OPENERS));

    let mut handles = Vec::new();
    for writer in 0..WRITERS {
        let path = path.clone();
        let barrier = Arc::clone(&barrier);
        handles.push(std::thread::spawn(move || {
            barrier.wait();
            let mut store = AuditStore::open(&path).expect("open by a writer");
            for i in 0..PER_WRITER {
                store
                    .append(ev(writer * 1000 + i, "writer"))
                    .expect("append");
            }
        }));
    }
    for _ in 0..OPENERS {
        let path = path.clone();
        let barrier = Arc::clone(&barrier);
        handles.push(std::thread::spawn(move || {
            barrier.wait();
            AuditStore::open(&path).expect("concurrent open");
        }));
    }
    for handle in handles {
        handle.join().expect("join");
    }

    let store = AuditStore::open(&path).expect("open");
    assert_eq!(
        store.count().expect("count"),
        (WRITERS * PER_WRITER) as usize
    );
    assert_eq!(
        verify_chain(&store).expect("verify"),
        ChainStatus::Intact {
            length: (WRITERS * PER_WRITER) as usize
        },
        "the chain stayed a chain through the race"
    );
}
