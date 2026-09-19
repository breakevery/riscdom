//! v0.5 batch 1 — `export_run_audit`: one run's audit interval, from the app.
//!
//! The golden path's step 5 is "get an audit record". The record is the slice of
//! the hash chain a run occupies; this test drives the host command that writes it
//! and pins the two refusals (no such run, no end yet).

use audit::{AuditEvent, RunRecord, RunStatus};
use host::state::AppState;
use std::path::PathBuf;

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

fn lines(path: &std::path::Path) -> Vec<serde_json::Value> {
    std::fs::read_to_string(path)
        .expect("export file")
        .lines()
        .map(|line| serde_json::from_str(line).expect("one JSON object per line"))
        .collect()
}

#[test]
fn exports_exactly_the_runs_interval_into_the_workspace() {
    let (state, dir) = state_with_a_finished_run("happy");
    std::fs::create_dir_all(dir.join("audit")).expect("target dir");

    let written = state
        .export_run_audit("run_a", "audit/run_a.jsonl".into())
        .expect("export");

    let file = dir.join("audit").join("run_a.jsonl");
    assert!(file.is_file(), "{}", file.display());
    let exported = lines(&file);
    assert_eq!(written, exported.len());

    let actions: Vec<&str> = exported
        .iter()
        .map(|l| l["action"].as_str().unwrap())
        .collect();
    assert_eq!(
        actions,
        vec![
            audit::ACTION_RUN_START,
            "agent.tool.call",
            audit::ACTION_RUN_END
        ],
        "{actions:?}"
    );
    for leaked in ["host.start", "session.close"] {
        assert!(
            !actions.contains(&leaked),
            "`{leaked}` is outside the run and must not be exported"
        );
    }

    // The run's own metadata is in the file, so nothing had to be invented.
    let detail = &exported[0]["detail"];
    assert_eq!(detail["run_id"], "run_a");
    assert!(detail["fingerprint_json"].is_string(), "{detail}");
    assert_eq!(exported.last().unwrap()["detail"]["status"], "ok");

    // Every line is the chain row itself: same hashes, so an outside tool can check it.
    let stored = state
        .audit
        .lock()
        .unwrap()
        .list(audit::EventFilter::default(), 500)
        .expect("list");
    for (line, row) in exported.iter().zip(
        stored
            .iter()
            .filter(|r| r.event.action != "host.start" && r.event.action != "session.close"),
    ) {
        assert_eq!(line["id"].as_i64().unwrap(), row.id);
        assert_eq!(line["hash"].as_str().unwrap(), row.hash);
        assert_eq!(line["prev_hash"].as_str().unwrap(), row.prev_hash);
    }
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
