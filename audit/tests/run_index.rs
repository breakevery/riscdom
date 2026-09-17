//! v0.4 batch 1b — the derived `runs` index.
//!
//! The index is derived from the chain, so the tests are written the same way:
//! build a chain, look at what the index says, and require the two to agree.

use audit::{
    fingerprint, run_end_detail, run_start_detail, verify_chain, AuditEvent, AuditStore,
    ChainStatus, RunRecord, RunStatus, ACTION_RUN_ABANDONED, ACTION_RUN_END, ACTION_RUN_START,
    FINGERPRINT_SCHEMA_V1,
};
use rusqlite::Connection;
use std::path::PathBuf;

fn tmp_db(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("runindex");
    std::fs::create_dir_all(&dir).expect("dir");
    for suffix in ["", "-wal", "-shm"] {
        let _ = std::fs::remove_file(dir.join(format!("{name}.db{suffix}")));
    }
    dir.join(format!("{name}.db"))
}

fn config(model: &str) -> serde_json::Value {
    serde_json::json!({ "llm": { "model": model }, "vm": { "memory_mb": 128 } })
}

fn start_row(run_id: &str, model: &str, seq: i64, at_ms: i64) -> RunRecord {
    RunRecord {
        run_id: run_id.to_string(),
        session_id: Some("s1".to_string()),
        parent_run_id: None,
        fingerprint: fingerprint(&config(model)),
        fingerprint_schema: FINGERPRINT_SCHEMA_V1.to_string(),
        started_at_ms: at_ms,
        ended_at_ms: None,
        start_seq: seq,
        end_seq: None,
        status: RunStatus::Open,
    }
}

/// Append `run.start` and maintain the index (what the host does).
fn begin(store: &mut AuditStore, run_id: &str, model: &str) -> i64 {
    let start = store
        .append(AuditEvent::new(
            "host",
            ACTION_RUN_START,
            run_start_detail(run_id, Some("s1"), None, None, &config(model)),
        ))
        .expect("run.start");
    store
        .index_run_start(&start_row(
            run_id,
            model,
            start.id,
            start.event.timestamp_ms,
        ))
        .expect("index run.start");
    start.id
}

/// Append `run.end` and maintain the index.
fn finish(store: &mut AuditStore, run_id: &str, status: RunStatus) -> i64 {
    let end = store
        .append(AuditEvent::new(
            "host",
            ACTION_RUN_END,
            run_end_detail(run_id, status, "done"),
        ))
        .expect("run.end");
    assert!(
        store
            .index_run_end(run_id, end.id, end.event.timestamp_ms, status)
            .expect("index run.end"),
        "the index row must exist before it is closed"
    );
    end.id
}

#[test]
fn run_markers_keep_the_chain_intact() {
    let mut store = AuditStore::in_memory().expect("store");
    begin(&mut store, "run_a", "deepseek-chat");
    store
        .append(AuditEvent::new(
            "agent",
            "agent.tool.call",
            serde_json::json!({ "name": "compile" }),
        ))
        .expect("tool call");
    finish(&mut store, "run_a", RunStatus::Ok);

    assert_eq!(
        verify_chain(&store).expect("verify"),
        ChainStatus::Intact { length: 3 },
        "run markers are ordinary chained events"
    );
}

#[test]
fn list_and_get_read_the_index() {
    let mut store = AuditStore::in_memory().expect("store");
    begin(&mut store, "run_a", "deepseek-chat");
    let end_seq = finish(&mut store, "run_a", RunStatus::Ok);
    begin(&mut store, "run_b", "gpt-4o-mini");

    let runs = store.list_runs(10).expect("list");
    assert_eq!(
        runs.iter().map(|r| r.run_id.as_str()).collect::<Vec<_>>(),
        vec!["run_a", "run_b"],
        "oldest first"
    );

    let a = store.get_run("run_a").expect("get").expect("present");
    assert_eq!(a.status, RunStatus::Ok);
    assert_eq!(a.end_seq, Some(end_seq));
    let ended = a.ended_at_ms.expect("an ended run has an end timestamp");
    assert!(ended >= a.started_at_ms, "end before start: {a:?}");
    assert_eq!(a.fingerprint, fingerprint(&config("deepseek-chat")));

    let b = store.get_run("run_b").expect("get").expect("present");
    assert_eq!(b.status, RunStatus::Open, "an unfinished run stays open");
    assert_eq!(b.end_seq, None);

    assert!(store.get_run("nope").expect("get").is_none());
    assert_eq!(store.list_runs(1).expect("list").len(), 1, "limit applies");
}

#[test]
fn rebuild_reproduces_the_index_field_by_field() {
    let mut store = AuditStore::in_memory().expect("store");
    begin(&mut store, "run_a", "deepseek-chat");
    finish(&mut store, "run_a", RunStatus::Failed);
    begin(&mut store, "run_b", "gpt-4o-mini"); // left open on purpose
                                               // An abandoned run: noticed at startup, never given a fabricated run.end.
    begin(&mut store, "run_c", "qwen2.5-coder");
    store
        .append(AuditEvent::new(
            "host",
            ACTION_RUN_ABANDONED,
            serde_json::json!({ "run_id": "run_c", "detected_at_ms": 42 }),
        ))
        .expect("abandoned");

    let (derived, report) = store.derive_runs().expect("derive");
    assert_eq!(report.starts, 3);
    assert_eq!(report.ends, 1);
    assert_eq!(report.abandoned, 1);
    assert_eq!(report.orphans, 0);

    // The incremental index diverges from the chain only for run_c (the host did
    // not maintain it there), which is exactly what the cross-check reports.
    let findings = store.check_run_index().expect("check");
    assert_eq!(findings.len(), 1, "findings: {findings:?}");
    assert!(findings[0].contains("run_c"), "findings: {findings:?}");

    let rebuilt = store.rebuild_run_index().expect("rebuild");
    assert_eq!(rebuilt, report);
    assert_eq!(store.all_runs().expect("all"), derived);
    assert!(
        store.check_run_index().expect("check").is_empty(),
        "after a rebuild the index agrees with the chain"
    );

    let c = store.get_run("run_c").expect("get").expect("present");
    assert_eq!(c.status, RunStatus::Abandoned);
    assert_eq!(c.end_seq, None, "no fabricated end");
}

#[test]
fn an_old_database_without_the_index_is_readable() {
    let path = tmp_db("old");
    {
        let mut store = AuditStore::open(&path).expect("store");
        begin(&mut store, "run_a", "deepseek-chat");
        finish(&mut store, "run_a", RunStatus::Ok);
    }
    // Simulate a database written before this batch: no `runs` table at all.
    {
        let conn = Connection::open(&path).expect("raw");
        conn.execute_batch("DROP TABLE runs").expect("drop index");
    }
    assert!(!table_exists(&path, "runs"), "precondition");

    let mut store = AuditStore::open(&path).expect("reopen");
    assert!(
        table_exists(&path, "runs"),
        "opening re-creates the derived table"
    );
    assert_eq!(
        verify_chain(&store).expect("verify"),
        ChainStatus::Intact { length: 2 },
        "old rows are untouched"
    );
    assert!(store.list_runs(10).expect("list").is_empty());
    assert!(store.get_run("run_a").expect("get").is_none());

    // The chain still knows the run: one rebuild brings the index back.
    let report = store.rebuild_run_index().expect("rebuild");
    assert_eq!(report.runs, 1);
    let row = store.get_run("run_a").expect("get").expect("present");
    assert_eq!(row.status, RunStatus::Ok);
    assert!(store.check_run_index().expect("check").is_empty());
}

#[test]
fn cross_check_detects_a_tampered_index_row() {
    let path = tmp_db("tamper");
    {
        let mut store = AuditStore::open(&path).expect("store");
        begin(&mut store, "run_a", "deepseek-chat");
        finish(&mut store, "run_a", RunStatus::Ok);
    }
    {
        let conn = Connection::open(&path).expect("raw");
        conn.execute(
            "UPDATE runs SET fingerprint = 'deadbeef', status = 'ok' WHERE run_id = 'run_a'",
            [],
        )
        .expect("tamper the derived row");
    }

    let mut store = AuditStore::open(&path).expect("reopen");
    // The chain is still intact: the index is not part of it.
    assert!(matches!(
        verify_chain(&store).expect("verify"),
        ChainStatus::Intact { .. }
    ));
    let findings = store.check_run_index().expect("check");
    assert_eq!(findings.len(), 1, "findings: {findings:?}");
    assert!(findings[0].contains("differs"), "findings: {findings:?}");

    store.rebuild_run_index().expect("rebuild");
    assert!(
        store.check_run_index().expect("check").is_empty(),
        "a rebuild repairs the index from the chain"
    );
    let row = store.get_run("run_a").expect("get").expect("present");
    assert_eq!(row.fingerprint, fingerprint(&config("deepseek-chat")));
}

#[test]
fn orphan_markers_are_reported_not_invented() {
    let mut store = AuditStore::in_memory().expect("store");
    store
        .append(AuditEvent::new(
            "host",
            ACTION_RUN_END,
            run_end_detail("never-started", RunStatus::Ok, "orphan"),
        ))
        .expect("orphan end");
    store
        .append(AuditEvent::new(
            "host",
            ACTION_RUN_START,
            run_start_detail("run_a", None, None, None, &config("m")),
        ))
        .expect("start");
    // A second start for the same id is a duplicate, not a second run.
    store
        .append(AuditEvent::new(
            "host",
            ACTION_RUN_START,
            run_start_detail("run_a", None, None, None, &config("m")),
        ))
        .expect("duplicate start");

    let (derived, report) = store.derive_runs().expect("derive");
    assert_eq!(derived.len(), 1, "one run id, one row");
    assert_eq!(report.starts, 2);
    assert_eq!(report.ends, 0);
    assert_eq!(report.orphans, 2, "orphan end + duplicate start");
    match verify_chain(&store).expect("verify") {
        ChainStatus::Intact { length } => assert_eq!(length, 3),
        other => panic!("expected an intact chain, got {other:?}"),
    }
}

/// Small helper: does the database have a table with this name.
fn table_exists(path: &std::path::Path, name: &str) -> bool {
    let conn = Connection::open(path).expect("raw");
    conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
        [name],
        |r| r.get::<_, i64>(0),
    )
    .expect("query")
        > 0
}
