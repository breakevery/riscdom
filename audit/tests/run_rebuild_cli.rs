//! v0.4 batch 1b — `audit-rebuild` exit codes.
//!
//! `audit-verify` must stay read-only, so the rebuild lives in its own binary
//! (`audit-rebuild`). These tests pin its contract: it repairs the derived index
//! from the chain, reports what it did, and still fails when the log itself has
//! run markers that cannot form runs.

use audit::{
    fingerprint, run_end_detail, run_start_detail, AuditEvent, AuditStore, RunRecord, RunStatus,
    ACTION_RUN_END, ACTION_RUN_START, FINGERPRINT_SCHEMA_V1,
};
use rusqlite::Connection;
use std::path::PathBuf;
use std::process::Command;

fn tmp_db(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("rebuildruns");
    std::fs::create_dir_all(&dir).expect("dir");
    for suffix in ["", "-wal", "-shm"] {
        let _ = std::fs::remove_file(dir.join(format!("{name}.db{suffix}")));
    }
    dir.join(format!("{name}.db"))
}

fn config() -> serde_json::Value {
    serde_json::json!({ "llm": { "model": "deepseek-chat" } })
}

/// A database with one complete, correctly indexed run.
fn db_with_one_run(name: &str) -> PathBuf {
    let path = tmp_db(name);
    let mut store = AuditStore::open(&path).expect("store");
    let start = store
        .append(AuditEvent::new(
            "host",
            ACTION_RUN_START,
            run_start_detail("run_a", Some("s1"), None, None, &config()),
        ))
        .expect("run.start");
    store
        .index_run_start(&RunRecord {
            run_id: "run_a".into(),
            session_id: Some("s1".into()),
            parent_run_id: None,
            resumed_from_snapshot: None,
            fingerprint: fingerprint(&config()),
            fingerprint_schema: FINGERPRINT_SCHEMA_V1.into(),
            started_at_ms: start.event.timestamp_ms,
            ended_at_ms: None,
            start_seq: start.id,
            end_seq: None,
            status: RunStatus::Open,
        })
        .expect("index start");
    let end = store
        .append(AuditEvent::new(
            "host",
            ACTION_RUN_END,
            run_end_detail("run_a", RunStatus::Ok, "done"),
        ))
        .expect("run.end");
    assert!(store
        .index_run_end("run_a", end.id, end.event.timestamp_ms, RunStatus::Ok)
        .expect("index end"));
    path
}

fn rebuild(path: &str, flags: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_audit-rebuild"))
        .arg(path)
        .args(flags)
        .output()
        .expect("run audit-rebuild")
}

#[test]
fn rebuild_repairs_a_truncated_index_and_exits_zero() {
    let path = db_with_one_run("repair");
    {
        let conn = Connection::open(&path).expect("raw");
        conn.execute("DELETE FROM runs", [])
            .expect("truncate index");
    }

    let out = rebuild(path.to_str().unwrap(), &[]);
    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("IndexRebuilt { runs: 1, starts: 1, ends: 1, abandoned: 0, orphans: 0 }"),
        "stdout: {stdout}"
    );
    assert!(
        stdout.contains("RunIndex { findings: 0 }"),
        "stdout: {stdout}"
    );

    // Rebuilt from the chain alone: the row is back, with its configuration digest.
    let store = AuditStore::open(&path).expect("reopen");
    let row = store.get_run("run_a").expect("get").expect("present");
    assert_eq!(row.status, RunStatus::Ok);
    assert_eq!(row.fingerprint, fingerprint(&config()));
}

#[test]
fn rebuild_repairs_a_tampered_index_and_exits_zero() {
    let path = db_with_one_run("tampered");
    {
        let conn = Connection::open(&path).expect("raw");
        conn.execute(
            "UPDATE runs SET fingerprint = 'deadbeef', status = 'failed' WHERE run_id = 'run_a'",
            [],
        )
        .expect("tamper");
    }

    let out = rebuild(path.to_str().unwrap(), &[]);
    assert_eq!(out.status.code(), Some(0));
    let store = AuditStore::open(&path).expect("reopen");
    assert_eq!(
        store
            .get_run("run_a")
            .expect("get")
            .expect("present")
            .fingerprint,
        fingerprint(&config()),
        "the tampered row is replaced by the chain's own value"
    );
    assert!(store.check_run_index().expect("check").is_empty());
}

#[test]
fn rebuild_reports_chain_anomalies_and_exits_one() {
    // A rebuild cannot invent the missing counterpart of an orphan marker, so the
    // log is still not consistent afterwards — and the exit code says so.
    let path = tmp_db("orphan");
    {
        let mut store = AuditStore::open(&path).expect("store");
        store
            .append(AuditEvent::new(
                "host",
                ACTION_RUN_END,
                run_end_detail("never-started", RunStatus::Ok, "orphan"),
            ))
            .expect("orphan run.end");
    }

    let out = rebuild(path.to_str().unwrap(), &[]);
    assert_eq!(
        out.status.code(),
        Some(1),
        "an orphan marker must not read as healthy"
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("orphans: 1"), "stdout: {stdout}");
    assert!(
        stdout.contains("ChainAnomalies { orphans: 1 }"),
        "stdout: {stdout}"
    );
}

#[test]
fn rebuild_rejects_unknown_option_and_exits_two() {
    let path = db_with_one_run("badflag");
    let out = rebuild(path.to_str().unwrap(), &["--runs"]);
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("unknown option"), "stderr: {stderr}");
}

#[test]
fn rebuild_missing_path_exits_two() {
    let out = Command::new(env!("CARGO_BIN_EXE_audit-rebuild"))
        .output()
        .expect("run audit-rebuild");
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("missing <path-to-db>"), "stderr: {stderr}");
}

#[test]
fn rebuild_missing_file_exits_two() {
    let path = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join("rebuildruns")
        .join("does-not-exist.db");
    let _ = std::fs::remove_file(&path);
    let out = rebuild(path.to_str().unwrap(), &[]);
    assert_eq!(out.status.code(), Some(2));
}
