//! v0.6 batch 1 — the fingerprint diff over runs that are actually in a log.
//!
//! The unit tests in `host/src/run_diff.rs` cover the pure comparison. What is
//! covered here is the Rust-facing API around it — `AppState::compare_run_fingerprints`
//! — and the one thing the pure tests cannot see: whether the declared field list
//! still matches the fingerprint the host builds.

use host::run_diff::{diff_fingerprints, FINGERPRINT_FIELDS};
use host::state::AppState;
use std::path::PathBuf;

fn unique_dir(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "riscdom-rundiff-{tag}-{}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A configuration fingerprint with every declared field, differing only in the
/// model — a stand-in for two runs of the same setup with one setting changed.
fn config(model: &str) -> serde_json::Value {
    serde_json::json!({
        "schema": { "fingerprint_schema": audit::FINGERPRINT_SCHEMA_V1, "app_version": "0.6.0" },
        "llm": { "provider_id": "deepseek", "base_url": "https://api.example/v1", "model": model },
        "agent": { "max_iterations": 8, "request_timeout_secs": 120 },
        "vm": { "memory_mb": 256, "machine": "board-a", "cpu": "core-a" },
        "toolchain": { "path": "C:/toolchain/bin", "version": "13.2.0", "source": "download" },
        "policy": { "allowed_extensions": ["c", "h"], "traversal_guard": "normalize+containment" },
        "prompt": { "sha256": "00" },
    })
}

/// Write one complete run the way the host writes it, and return its digest.
fn seed_run(state: &AppState, run_id: &str, config: &serde_json::Value) -> String {
    let detail = audit::run_start_detail(run_id, Some("s1"), None, None, config);
    let mut store = state.audit.lock().expect("audit");
    let start = store
        .append(audit::AuditEvent::new(
            "host",
            audit::ACTION_RUN_START,
            detail,
        ))
        .expect("run.start");
    store
        .index_run_start(&audit::RunRecord {
            run_id: run_id.to_string(),
            session_id: Some("s1".to_string()),
            parent_run_id: None,
            resumed_from_snapshot: None,
            fingerprint: audit::fingerprint(config),
            fingerprint_schema: audit::FINGERPRINT_SCHEMA_V1.to_string(),
            started_at_ms: start.event.timestamp_ms,
            ended_at_ms: None,
            start_seq: start.id,
            end_seq: None,
            status: audit::RunStatus::Open,
        })
        .expect("index start");

    let end = store
        .append(audit::AuditEvent::new(
            "host",
            audit::ACTION_RUN_END,
            audit::run_end_detail(run_id, audit::RunStatus::Ok, "done"),
        ))
        .expect("run.end");
    assert!(store
        .index_run_end(run_id, end.id, end.event.timestamp_ms, audit::RunStatus::Ok)
        .expect("index end"));
    audit::fingerprint(config)
}

#[test]
fn the_declared_fields_still_match_the_fingerprint_the_host_builds() {
    let state = AppState::in_memory(unique_dir("shape")).expect("state");
    let real = state.run_fingerprint();
    let mut live: Vec<String> = real
        .as_object()
        .expect("the fingerprint is an object")
        .keys()
        .cloned()
        .collect();
    live.sort();
    let mut declared: Vec<String> = FINGERPRINT_FIELDS.iter().map(|f| f.to_string()).collect();
    declared.sort();

    assert_eq!(
        declared, live,
        "the diff's field list and the document it diffs must not drift apart"
    );
    // And nothing is left over on either side of the comparison.
    let diff = diff_fingerprints(&real, &real);
    assert_eq!(diff.len(), FINGERPRINT_FIELDS.len());
    assert!(diff.iter().all(|d| !d.is_different));
}

#[test]
fn two_runs_in_the_log_are_diffed_field_by_field() {
    let state = AppState::in_memory(unique_dir("two")).expect("state");
    let a = config("deepseek-chat");
    let b = config("deepseek-reasoner");
    let digest_a = seed_run(&state, "run_a", &a);
    let digest_b = seed_run(&state, "run_b", &b);
    assert_ne!(digest_a, digest_b, "a different model is a different run");

    let diff = state
        .compare_run_fingerprints("run_a", "run_b")
        .expect("diff");

    assert_eq!(
        diff.iter().map(|d| d.field.as_str()).collect::<Vec<_>>(),
        FINGERPRINT_FIELDS.to_vec(),
        "every declared field, in declaration order"
    );
    let different: Vec<&str> = diff
        .iter()
        .filter(|d| d.is_different)
        .map(|d| d.field.as_str())
        .collect();
    assert_eq!(different, vec!["llm"], "only the model moved");
    let llm = diff.iter().find(|d| d.field == "llm").expect("llm");
    assert_eq!(llm.a, a["llm"]);
    assert_eq!(llm.b, b["llm"]);
}

#[test]
fn a_run_against_itself_is_all_equal() {
    let state = AppState::in_memory(unique_dir("self")).expect("state");
    seed_run(&state, "run_a", &config("deepseek-chat"));

    let diff = state
        .compare_run_fingerprints("run_a", "run_a")
        .expect("diff");

    assert_eq!(diff.len(), FINGERPRINT_FIELDS.len(), "still the whole list");
    assert!(diff.iter().all(|d| !d.is_different));
}

#[test]
fn an_unknown_run_is_an_error() {
    let state = AppState::in_memory(unique_dir("unknown")).expect("state");
    seed_run(&state, "run_a", &config("deepseek-chat"));

    let err = state
        .compare_run_fingerprints("run_nope", "run_a")
        .expect_err("no such run");
    assert!(err.to_string().contains("no run run_nope"), "{err}");
}
