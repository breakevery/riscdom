//! v0.4 batch 1d — the read-only run interface.
//!
//! `AppState::list_runs` / `get_run` are what the two Tauri commands wrap: a
//! window onto what the derived index says, with no way to change anything.

use audit::{RunStatus, ACTION_RUN_END, ACTION_RUN_START};
use host::state::AppState;
use std::path::PathBuf;

fn unique_dir(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "riscdom-runcmd-{tag}-{}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Write one complete run the way the host writes it.
fn seed_run(state: &AppState, run_id: &str, parent: Option<&str>) -> String {
    let config = state.run_fingerprint();
    let detail = audit::run_start_detail(run_id, Some("s1"), parent, None, &config);
    let mut store = state.audit.lock().expect("audit");
    let start = store
        .append(audit::AuditEvent::new("host", ACTION_RUN_START, detail))
        .expect("run.start");
    store
        .index_run_start(&audit::RunRecord {
            run_id: run_id.to_string(),
            session_id: Some("s1".to_string()),
            parent_run_id: parent.map(str::to_string),
            fingerprint: audit::fingerprint(&config),
            fingerprint_schema: audit::FINGERPRINT_SCHEMA_V1.to_string(),
            started_at_ms: start.event.timestamp_ms,
            ended_at_ms: None,
            start_seq: start.id,
            end_seq: None,
            status: RunStatus::Open,
        })
        .expect("index start");

    let end = store
        .append(audit::AuditEvent::new(
            "host",
            ACTION_RUN_END,
            audit::run_end_detail(run_id, RunStatus::Ok, "done"),
        ))
        .expect("run.end");
    assert!(store
        .index_run_end(run_id, end.id, end.event.timestamp_ms, RunStatus::Ok)
        .expect("index end"));
    audit::fingerprint(&config)
}

#[test]
fn an_empty_log_has_no_runs() {
    let state = AppState::in_memory(unique_dir("empty")).expect("state");
    assert!(state.list_runs(20).expect("list").is_empty());
    assert!(state.get_run("run_nope").expect("get").is_none());
}

#[test]
fn runs_are_exposed_with_every_field_the_panel_needs() {
    let state = AppState::in_memory(unique_dir("fields")).expect("state");
    let digest = seed_run(&state, "run_a", None);
    seed_run(&state, "run_b", Some("run_a")); // a restore, linked to its producer

    let listed = state.list_runs(20).expect("list");
    assert_eq!(
        listed.iter().map(|r| r.run_id.as_str()).collect::<Vec<_>>(),
        vec!["run_a", "run_b"],
        "oldest first, as the chain orders them"
    );

    let first = &listed[0];
    assert_eq!(first.status, "ok");
    assert_eq!(first.fingerprint, digest);
    assert_eq!(first.fingerprint_short, &digest[..16]);
    assert_eq!(first.session_id.as_deref(), Some("s1"));
    assert_eq!(first.parent_run_id, None);
    assert!(first.started_at_ms > 0);
    assert!(first.ended_at_ms.expect("ended") >= first.started_at_ms);

    let restored = state.get_run("run_b").expect("get").expect("present");
    assert_eq!(restored.parent_run_id.as_deref(), Some("run_a"));
    assert_eq!(restored.run_id, "run_b");

    // The JSON the frontend receives carries no audit internals.
    let json = serde_json::to_string(&listed).expect("json");
    for forbidden in ["prev_hash", "detail_json", "start_seq", "end_seq"] {
        assert!(
            !json.contains(forbidden),
            "{forbidden} must not be in the panel payload: {json}"
        );
    }
    assert!(json.contains("fingerprint_short"), "{json}");
}

#[test]
fn an_open_run_is_reported_as_open() {
    let state = AppState::in_memory(unique_dir("open")).expect("state");
    let config = state.run_fingerprint();
    let detail = audit::run_start_detail("run_live", Some("s1"), None, None, &config);
    let mut store = state.audit.lock().expect("audit");
    let start = store
        .append(audit::AuditEvent::new("host", ACTION_RUN_START, detail))
        .expect("run.start");
    store
        .index_run_start(&audit::RunRecord {
            run_id: "run_live".to_string(),
            session_id: Some("s1".to_string()),
            parent_run_id: None,
            fingerprint: audit::fingerprint(&config),
            fingerprint_schema: audit::FINGERPRINT_SCHEMA_V1.to_string(),
            started_at_ms: start.event.timestamp_ms,
            ended_at_ms: None,
            start_seq: start.id,
            end_seq: None,
            status: RunStatus::Open,
        })
        .expect("index");
    drop(store);

    let run = state.get_run("run_live").expect("get").expect("present");
    assert_eq!(run.status, "open");
    assert_eq!(run.ended_at_ms, None, "an unfinished run has no end time");
}
