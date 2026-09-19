//! v0.5 batch 2 — an exported run interval verifies on its own.
//!
//! Batch 1 made the export a chain slice; this walks the whole claim: build a run,
//! export its interval, write the exported lines into a **fresh** database, and run
//! the independent checker (`audit-verify --runs`) over it. Nothing from the source
//! database is carried over except the exported file, so the verdict is evidence
//! that the file is a self-sufficient audit record.
//!
//! The run has to open the chain for this to work at all, and that is the point:
//! `verify_chain` starts from the genesis link, so an exported slice can only be
//! verified in a fresh database when the export starts where the chain does. The
//! test therefore pins the honest case rather than pretending a mid-chain slice is
//! a stand-alone log.

use audit::{
    run_end_detail, run_interval, run_start_detail, AuditEvent, AuditStore, RunStatus,
    ACTION_RUN_END, ACTION_RUN_START,
};
use rusqlite::{params, Connection};
use std::path::{Path, PathBuf};
use std::process::Command;

fn unique_dir(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "riscdom-export-verify-{tag}-{}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn config() -> serde_json::Value {
    serde_json::json!({
        "schema": { "app_version": "0.5.0" },
        "llm": { "provider_id": "deepseek", "model": "m" },
    })
}

fn lines(path: &Path) -> Vec<serde_json::Value> {
    std::fs::read_to_string(path)
        .expect("export file")
        .lines()
        .map(|line| serde_json::from_str(line).expect("one JSON object per line"))
        .collect()
}

/// Copy exported lines verbatim into a fresh database's `audit_events` table.
///
/// `id` is kept as exported, so the copied rows are the same chain positions, and
/// `detail_json` is re-serialised from the parsed `detail` — the export test
/// (`interval_export.rs`) already proves that this reproduces the hashed bytes.
fn import(export: &Path, db: &Path, expected_rows: usize) -> AuditStore {
    // Open once so the schema (table + append-only triggers) exists.
    let _ = AuditStore::open(db).expect("fresh store");
    {
        let conn = Connection::open(db).expect("raw connection");
        for line in lines(export) {
            conn.execute(
                "INSERT INTO audit_events \
                 (id, timestamp_ms, actor, action, detail_json, prev_hash, hash) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    line["id"].as_i64().unwrap(),
                    line["timestamp_ms"].as_i64().unwrap(),
                    line["actor"].as_str().unwrap(),
                    line["action"].as_str().unwrap(),
                    serde_json::to_string(&line["detail"]).unwrap(),
                    line["prev_hash"].as_str().unwrap(),
                    line["hash"].as_str().unwrap(),
                ],
            )
            .expect("import row");
        }
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM audit_events", [], |r| r.get(0))
            .expect("count");
        assert_eq!(count as usize, expected_rows, "one row per exported line");
    }
    AuditStore::open(db).expect("imported store")
}

fn audit_verify(db: &Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_audit-verify"))
        .arg(db)
        .arg("--runs")
        .output()
        .expect("run audit-verify")
}

#[test]
fn an_exported_interval_verifies_in_a_fresh_database() {
    let dir = unique_dir("e2e");
    let source = dir.join("source.db");
    let export = dir.join("run_a.jsonl");
    let verify_db = dir.join("verify.db");

    // A run that opens the chain: its interval starts at the genesis link.
    let (start_id, end_id, written) = {
        let mut store = AuditStore::open(&source).expect("source store");
        let start = store
            .append(AuditEvent::new(
                "host",
                ACTION_RUN_START,
                run_start_detail("run_a", Some("s1"), None, None, &config()),
            ))
            .expect("run.start");
        store
            .append(AuditEvent::new(
                "agent",
                "agent.tool.call",
                serde_json::json!({ "name": "compile" }),
            ))
            .unwrap();
        store
            .append(AuditEvent::new(
                "sandbox",
                "vm.start",
                serde_json::json!({}),
            ))
            .unwrap();
        let end = store
            .append(AuditEvent::new(
                "host",
                ACTION_RUN_END,
                run_end_detail("run_a", RunStatus::Ok, "done"),
            ))
            .expect("run.end");
        // A neighbour after the run: it must not be in the file.
        store
            .append(AuditEvent::new(
                "human",
                "session.close",
                serde_json::json!({}),
            ))
            .unwrap();

        let (runs, report) = store.derive_runs().unwrap();
        assert_eq!(report.orphans, 0, "{report:?}");
        let record = runs
            .into_iter()
            .find(|r| r.run_id == "run_a")
            .expect("the chain has the run");

        let (from, to) = run_interval(&record, &store.all().unwrap()).unwrap();
        assert_eq!(
            (from, to),
            (start.id, end.id),
            "the interval is the run's own span"
        );
        let written = store.export_interval_jsonl(from, to, &export).unwrap();
        (start.id, end.id, written)
    };

    assert_eq!(
        (start_id, end_id),
        (1, 4),
        "`run.start` opens the chain, `run.end` closes it"
    );
    let exported = lines(&export);
    assert_eq!(written, 4);
    assert_eq!(exported.len(), 4);
    assert_eq!(
        exported.last().unwrap()["detail"]["status"],
        "ok",
        "the export ends on `run.end`, not on the neighbour"
    );

    // The file, and nothing else, goes into a fresh database.
    let mut imported = import(&export, &verify_db, written);
    let report = imported.rebuild_run_index().expect("rebuild derived index");
    assert_eq!(report.runs, 1, "{report:?}");
    assert_eq!(report.ends, 1, "{report:?}");
    assert_eq!(imported.check_run_index().unwrap(), Vec::<String>::new());

    let out = audit_verify(&verify_db);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(
        out.status.code(),
        Some(0),
        "stdout: {stdout}\nstderr: {stderr}"
    );
    assert!(stdout.contains("Intact"), "stdout: {stdout}");
    assert!(
        stdout.contains("RunIndex { findings: 0 }"),
        "stdout: {stdout}"
    );
}
