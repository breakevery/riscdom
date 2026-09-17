//! v0.4 batch 1b — `audit-verify --runs` / `--rebuild-index` exit codes.
//!
//! The chain verdict and the `0` / `1` / `2` codes are the contract every
//! existing script depends on; the new flags are additive.

use audit::{
    fingerprint, run_end_detail, run_start_detail, AuditEvent, AuditStore, RunRecord, RunStatus,
    ACTION_RUN_END, ACTION_RUN_START, FINGERPRINT_SCHEMA_V1,
};
use rusqlite::Connection;
use std::path::PathBuf;
use std::process::Command;

fn tmp_db(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("verifyruns");
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

fn verify(path: &str, flags: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_audit-verify"))
        .arg(path)
        .args(flags)
        .output()
        .expect("run audit-verify")
}

#[test]
fn plain_run_is_unchanged() {
    let path = db_with_one_run("plain");
    let out = verify(path.to_str().unwrap(), &[]);
    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("Intact"), "stdout: {stdout}");
    assert!(
        !stdout.contains("RunIndex"),
        "the new sections stay opt-in: {stdout}"
    );
}

#[test]
fn runs_flag_reports_a_consistent_index_and_exits_zero() {
    let path = db_with_one_run("consistent");
    let out = verify(path.to_str().unwrap(), &["--runs"]);
    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("Intact"), "stdout: {stdout}");
    assert!(
        stdout.contains("RunIndex { findings: 0 }"),
        "stdout: {stdout}"
    );
}

#[test]
fn runs_flag_exits_one_when_the_index_diverges() {
    let path = db_with_one_run("diverged");
    {
        let conn = Connection::open(&path).expect("raw");
        conn.execute(
            "UPDATE runs SET fingerprint = 'deadbeef' WHERE run_id = 'run_a'",
            [],
        )
        .expect("tamper");
    }

    let out = verify(path.to_str().unwrap(), &["--runs"]);
    assert_eq!(
        out.status.code(),
        Some(1),
        "a divergent index must not read as healthy"
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("Intact"),
        "the chain itself is fine: {stdout}"
    );
    assert!(stdout.contains("findings: 1"), "stdout: {stdout}");
}

#[test]
fn rebuild_index_repairs_the_index_and_exits_zero() {
    let path = db_with_one_run("repair");
    {
        let conn = Connection::open(&path).expect("raw");
        conn.execute("DELETE FROM runs", [])
            .expect("truncate index");
    }

    let out = verify(path.to_str().unwrap(), &["--rebuild-index"]);
    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("IndexRebuilt { runs: 1"),
        "stdout: {stdout}"
    );

    // Rebuilt from the chain alone, and now consistent.
    let out = verify(path.to_str().unwrap(), &["--rebuild-index", "--runs"]);
    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("RunIndex { findings: 0 }"),
        "stdout: {stdout}"
    );
}

#[test]
fn unknown_option_exits_two() {
    let path = db_with_one_run("badflag");
    let out = verify(path.to_str().unwrap(), &["--nope"]);
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("unknown option"), "stderr: {stderr}");
}

#[test]
fn missing_path_exits_two() {
    let out = Command::new(env!("CARGO_BIN_EXE_audit-verify"))
        .arg("--runs")
        .output()
        .expect("run audit-verify");
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("missing <path-to-db>"), "stderr: {stderr}");
}
