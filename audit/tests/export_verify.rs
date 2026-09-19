//! v0.5 batch 4 — an exported run record verifies on its own.
//!
//! The claim a run-scoped export makes is that the file *is* the record: hand it to
//! a third party, put it in an empty database, and the chain verdict is positive.
//! This walks exactly that — the source chain has events **before** the run and
//! **after** it, and none of them is carried over: the export, on its own, has to be
//! enough. The real `audit-verify` binary judges the result.
//!
//! Batch 2 pinned the honest case for a mid-chain slice (a run that opens the chain);
//! batch 4 removed the slice form altogether, so the run no longer has to be first for
//! the file to be verifiable.

use audit::{
    run_end_detail, run_start_detail, AuditEvent, AuditStore, RunStatus, ACTION_RUN_END,
    ACTION_RUN_START,
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

fn audit_verify(db: &Path, runs: bool) -> std::process::Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_audit-verify"));
    cmd.arg(db);
    if runs {
        cmd.arg("--runs");
    }
    cmd.output().expect("run audit-verify")
}

fn output_of(out: &std::process::Output) -> String {
    format!(
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

#[test]
fn an_exported_record_verifies_in_a_fresh_database() {
    let dir = unique_dir("e2e");
    let source = dir.join("source.db");
    let export = dir.join("run_a.jsonl");
    let verify_db = dir.join("verify.db");

    // The source chain has an event before the run and one after it, so "the file
    // stands alone" is a real claim rather than a side effect of the run being first.
    let (start_id, end_id, written) = {
        let mut store = AuditStore::open(&source).expect("source store");
        store
            .append(AuditEvent::new(
                "human",
                "session.create",
                serde_json::json!({ "id": "s1" }),
            ))
            .expect("before");
        let start = store
            .append(AuditEvent::new(
                "host",
                ACTION_RUN_START,
                run_start_detail("run_a", Some("s1"), None, None, &config()),
            ))
            .expect("run.start");
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
        assert_eq!(record.start_seq, start.id);

        let to = store.run_end(&record).unwrap();
        assert_eq!(to, end.id, "the export cuts at this run's `run.end`");
        let written = store.export_self_contained_jsonl(to, &export).unwrap();
        (start.id, end.id, written)
    };

    assert_eq!(
        (start_id, end_id),
        (2, 4),
        "the run starts after a session event"
    );
    let exported = lines(&export);
    assert_eq!(written, 4, "genesis .. run.end");
    assert_eq!(exported.len(), 4);
    assert_eq!(
        exported.first().unwrap()["prev_hash"],
        audit::GENESIS_PREV_HASH,
        "the first line is anchored at genesis"
    );
    assert!(
        exported.first().unwrap()["id"].as_i64().unwrap() < start_id,
        "the run is not the first line — the file carries what came before it"
    );
    assert_eq!(
        exported.last().unwrap()["action"],
        ACTION_RUN_END,
        "the export ends on `run.end`, not on the neighbour"
    );
    assert_eq!(exported.last().unwrap()["detail"]["status"], "ok");

    // The file, and nothing else, goes into a fresh database: no prefix, no other
    // artefact. The checker judges the imported chain on its own.
    let mut imported = import(&export, &verify_db, written);
    let out = audit_verify(&verify_db, false);
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    assert_eq!(out.status.code(), Some(0), "{}", output_of(&out));
    assert!(stdout.contains("Intact"), "stdout: {stdout}");
    assert!(
        !stdout.contains("Broken"),
        "a self-contained export must not be broken anywhere: {stdout}"
    );

    // The imported chain also *is* the run: its derived index can be rebuilt from it.
    let report = imported.rebuild_run_index().expect("rebuild derived index");
    assert_eq!(report.runs, 1, "{report:?}");
    assert_eq!(report.ends, 1, "{report:?}");
    assert_eq!(imported.check_run_index().unwrap(), Vec::<String>::new());

    let out = audit_verify(&verify_db, true);
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    assert_eq!(out.status.code(), Some(0), "{}", output_of(&out));
    assert!(stdout.contains("Intact"), "stdout: {stdout}");
    assert!(
        stdout.contains("RunIndex { findings: 0 }"),
        "stdout: {stdout}"
    );
}
