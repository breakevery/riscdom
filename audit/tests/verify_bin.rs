//! Stage 4b — `audit-verify` CLI exit codes.

use audit::{AuditEvent, AuditStore};
use rusqlite::Connection;
use std::path::PathBuf;
use std::process::Command;

fn tmp_db(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("verifybin");
    std::fs::create_dir_all(&dir).expect("dir");
    for suffix in ["", "-wal", "-shm"] {
        let _ = std::fs::remove_file(dir.join(format!("{name}.db{suffix}")));
    }
    dir.join(format!("{name}.db"))
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

fn run_verify(path: &str) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_audit-verify"))
        .arg(path)
        .output()
        .expect("run audit-verify")
}

#[test]
fn clean_chain_exits_zero() {
    let path = tmp_db("clean");
    {
        let mut store = AuditStore::open(&path).expect("store");
        for i in 0..3 {
            store.append(ev(i + 1)).expect("append");
        }
    }

    let out = run_verify(path.to_str().unwrap());
    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("Intact"), "stdout: {stdout}");
    assert!(stdout.contains("length: 3"), "stdout: {stdout}");
}

#[test]
fn tampered_chain_exits_one() {
    let path = tmp_db("tampered");
    {
        let mut store = AuditStore::open(&path).expect("store");
        for i in 0..3 {
            store.append(ev(i + 1)).expect("append");
        }
    }
    {
        let conn = Connection::open(&path).expect("raw");
        conn.execute_batch(
            "DROP TRIGGER audit_no_update; \
             UPDATE audit_events SET action = 'evil' WHERE id = 2;",
        )
        .expect("tamper");
    }

    let out = run_verify(path.to_str().unwrap());
    assert_eq!(out.status.code(), Some(1));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("Broken"), "stdout: {stdout}");
    assert!(stdout.contains("at_id: 2"), "stdout: {stdout}");
}

#[test]
fn missing_file_exits_two() {
    let path = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join("verifybin")
        .join("does-not-exist.db");
    let _ = std::fs::remove_file(&path);
    let out = run_verify(path.to_str().unwrap());
    assert_eq!(out.status.code(), Some(2));
}
