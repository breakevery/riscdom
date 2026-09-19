//! v0.5 batches 1–4 — `export_run_audit`: one run's audit record, from the app.
//!
//! The golden path's step 5 is "get an audit record". The record is the chain from
//! its first event up to the run's end (batch 4), so the file stands on its own; this
//! test drives the host command that writes it, checks the file is independently
//! verifiable, and pins the two refusals (no such run, no end yet).

use audit::{AuditEvent, RunRecord, RunStatus};
use host::state::AppState;
use rusqlite::{params, Connection};
use std::path::{Path, PathBuf};

fn unique_dir(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "riscdom-run-export-{tag}-{}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A chain with one finished run, plus neighbours on both sides of it.
///
/// Written straight into the store the way the host writes it, so the test needs
/// no QEMU and no model.
fn state_with_a_finished_run(tag: &str) -> (AppState, PathBuf) {
    let dir = unique_dir(tag);
    let state = AppState::in_memory(dir.clone()).expect("state");
    let config = state.run_fingerprint();
    {
        let mut store = state.audit.lock().expect("audit");
        store
            .append(AuditEvent::new("host", "host.start", serde_json::json!({})))
            .expect("before");
        let stored = store
            .append(AuditEvent::new(
                "host",
                audit::ACTION_RUN_START,
                audit::run_start_detail("run_a", Some("s1"), None, None, &config),
            ))
            .expect("run.start");
        store
            .append(AuditEvent::new(
                "agent",
                "agent.tool.call",
                serde_json::json!({ "name": "compile" }),
            ))
            .expect("inside");
        let end = store
            .append(AuditEvent::new(
                "host",
                audit::ACTION_RUN_END,
                audit::run_end_detail("run_a", RunStatus::Ok, "done"),
            ))
            .expect("run.end");
        store
            .append(AuditEvent::new(
                "human",
                "session.close",
                serde_json::json!({}),
            ))
            .expect("after");

        store
            .index_run_start(&RunRecord {
                run_id: "run_a".into(),
                session_id: Some("s1".into()),
                parent_run_id: None,
                resumed_from_snapshot: None,
                fingerprint: audit::fingerprint(&config),
                fingerprint_schema: audit::FINGERPRINT_SCHEMA_V1.into(),
                started_at_ms: stored.event.timestamp_ms,
                ended_at_ms: Some(end.event.timestamp_ms),
                start_seq: stored.id,
                end_seq: Some(end.id),
                status: RunStatus::Ok,
            })
            .expect("index");
    }
    (state, dir)
}

fn lines(path: &Path) -> Vec<serde_json::Value> {
    std::fs::read_to_string(path)
        .expect("export file")
        .lines()
        .map(|line| serde_json::from_str(line).expect("one JSON object per line"))
        .collect()
}

/// Copy an exported file into a fresh database and judge that database's chain.
///
/// This is the same call `audit-verify` makes; the point here is that the exported
/// file alone is enough input to it.
fn chain_verdict_of_the_export(export: &Path, db: &Path) -> audit::ChainStatus {
    let _ = audit::AuditStore::open(db).expect("fresh store");
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
    }
    let store = audit::AuditStore::open(db).expect("imported store");
    audit::verify_chain(&store).expect("verify")
}

#[test]
fn exports_a_self_contained_record_into_the_workspace() {
    let (state, dir) = state_with_a_finished_run("happy");
    std::fs::create_dir_all(dir.join("audit")).expect("target dir");

    let written = state
        .export_run_audit("run_a", "audit/run_a.jsonl".into())
        .expect("export");

    let file = dir.join("audit").join("run_a.jsonl");
    assert!(file.is_file(), "{}", file.display());
    let exported = lines(&file);
    assert_eq!(written, exported.len());

    // It begins at the chain's first event, anchored at genesis (v0.5 batch 4).
    assert_eq!(exported[0]["prev_hash"], audit::GENESIS_PREV_HASH);
    assert_eq!(exported[0]["id"].as_i64(), Some(1));

    let actions: Vec<&str> = exported
        .iter()
        .map(|l| l["action"].as_str().unwrap())
        .collect();
    assert_eq!(
        actions,
        vec![
            "host.start",
            audit::ACTION_RUN_START,
            "agent.tool.call",
            audit::ACTION_RUN_END
        ],
        "{actions:?}"
    );
    assert!(
        !actions.contains(&"session.close"),
        "what came after the run must not be exported"
    );

    // The run's own metadata is in the file, so nothing had to be invented.
    let start_line = exported
        .iter()
        .find(|l| l["action"] == audit::ACTION_RUN_START)
        .expect("the run.start is in the file");
    assert_eq!(start_line["detail"]["run_id"], "run_a");
    assert!(
        start_line["detail"]["fingerprint_json"].is_string(),
        "{start_line}"
    );
    assert_eq!(exported.last().unwrap()["action"], audit::ACTION_RUN_END);
    assert_eq!(exported.last().unwrap()["detail"]["status"], "ok");

    // Every line is the chain row itself: same ids and hashes.
    let stored = state
        .audit
        .lock()
        .unwrap()
        .list(audit::EventFilter::default(), 500)
        .expect("list");
    for (line, row) in exported.iter().zip(stored.iter()) {
        assert_eq!(line["id"].as_i64().unwrap(), row.id);
        assert_eq!(line["hash"].as_str().unwrap(), row.hash);
        assert_eq!(line["prev_hash"].as_str().unwrap(), row.prev_hash);
    }

    // The file stands on its own: a fresh database built from it verifies Intact.
    let verdict = chain_verdict_of_the_export(&file, &dir.join("verify.db"));
    assert!(
        matches!(verdict, audit::ChainStatus::Intact { .. }),
        "the export must verify without the source database: {verdict:?}"
    );
}

#[test]
fn an_abandoned_run_exports_up_to_its_marker() {
    // v0.5 batch 2: a run whose process disappeared has no `run.end`, but the chain
    // records `host.run.abandoned`, and the export ends on that line.
    let dir = unique_dir("abandoned");
    let state = AppState::in_memory(dir.clone()).expect("state");
    let config = state.run_fingerprint();
    {
        let mut store = state.audit.lock().expect("audit");
        store
            .append(AuditEvent::new(
                "host",
                audit::ACTION_RUN_START,
                audit::run_start_detail("run_stale", None, None, None, &config),
            ))
            .expect("run.start");
        store
            .append(AuditEvent::new(
                "agent",
                "agent.tool.call",
                serde_json::json!({ "name": "compile" }),
            ))
            .expect("inside");
        store
            .append(AuditEvent::new(
                "host",
                audit::ACTION_RUN_ABANDONED,
                serde_json::json!({ "run_id": "run_stale", "detected_at_ms": 1 }),
            ))
            .expect("marker");
        // The hook rebuilds the derived index after appending its marker.
        store.rebuild_run_index().expect("rebuild");
    }
    std::fs::create_dir_all(dir.join("audit")).expect("target dir");

    let written = state
        .export_run_audit("run_stale", "audit/run_stale.jsonl".into())
        .expect("an abandoned run is exportable");

    let file = dir.join("audit").join("run_stale.jsonl");
    let exported = lines(&file);
    assert_eq!(written, exported.len());
    assert_eq!(
        exported[0]["prev_hash"],
        audit::GENESIS_PREV_HASH,
        "self-contained even when the run never ended normally"
    );
    let actions: Vec<&str> = exported
        .iter()
        .map(|l| l["action"].as_str().unwrap())
        .collect();
    assert_eq!(
        actions,
        vec![
            audit::ACTION_RUN_START,
            "agent.tool.call",
            audit::ACTION_RUN_ABANDONED
        ],
        "{actions:?}"
    );
    let last = exported.last().unwrap();
    assert_eq!(last["detail"]["run_id"], "run_stale");
    assert_eq!(last["actor"], "host");

    let verdict = chain_verdict_of_the_export(&file, &dir.join("verify.db"));
    assert!(
        matches!(verdict, audit::ChainStatus::Intact { .. }),
        "an abandoned run's record must verify on its own: {verdict:?}"
    );
}

#[test]
fn an_unknown_run_is_refused() {
    let (state, dir) = state_with_a_finished_run("unknown");

    let error = state
        .export_run_audit("run_nope", "audit/nope.jsonl".into())
        .expect_err("an unknown run has nothing to export");

    assert!(error.to_string().contains("unknown run"), "{error}");
    assert!(!dir.join("audit").join("nope.jsonl").exists());
}

#[test]
fn an_unfinished_run_is_refused_rather_than_truncated() {
    // A run whose `run.end` never arrived: exporting "up to the end of the chain"
    // would produce a different file every time we asked.
    let dir = unique_dir("open");
    let state = AppState::in_memory(dir.clone()).expect("state");
    let config = state.run_fingerprint();
    {
        let mut store = state.audit.lock().expect("audit");
        let stored = store
            .append(AuditEvent::new(
                "host",
                audit::ACTION_RUN_START,
                audit::run_start_detail("run_open", None, None, None, &config),
            ))
            .expect("run.start");
        store
            .append(AuditEvent::new(
                "agent",
                "agent.tool.call",
                serde_json::json!({ "name": "compile" }),
            ))
            .expect("inside");
        store
            .index_run_start(&RunRecord {
                run_id: "run_open".into(),
                session_id: None,
                parent_run_id: None,
                resumed_from_snapshot: None,
                fingerprint: audit::fingerprint(&config),
                fingerprint_schema: audit::FINGERPRINT_SCHEMA_V1.into(),
                started_at_ms: stored.event.timestamp_ms,
                ended_at_ms: None,
                start_seq: stored.id,
                end_seq: None,
                status: RunStatus::Open,
            })
            .expect("index");
    }
    std::fs::create_dir_all(dir.join("audit")).expect("target dir");

    let error = state
        .export_run_audit("run_open", "audit/open.jsonl".into())
        .expect_err("a run without an end has no interval");

    let text = error.to_string();
    assert!(text.contains("has not ended"), "{text}");
    assert!(text.contains("run_open"), "{text}");
    assert!(
        !dir.join("audit").join("open.jsonl").exists(),
        "a refusal must not leave a file behind"
    );
}

#[test]
fn a_path_outside_the_workspace_is_refused() {
    let (state, _dir) = state_with_a_finished_run("escape");

    let error = state
        .export_run_audit("run_a", "../escaped.jsonl".into())
        .expect_err("the export may only write inside the workspace");

    let text = error.to_string();
    assert!(
        text.contains("traversal") || text.contains("workspace"),
        "{text}"
    );
}
