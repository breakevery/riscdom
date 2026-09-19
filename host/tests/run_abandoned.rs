//! v0.4 batch 1e — the startup hook that abandons runs left open by a previous
//! process.
//!
//! A run whose process disappeared keeps `end_seq = NULL` (nothing fabricates an
//! end), so the host marks it when it next starts: `AppState::new` scans the chain
//! and appends one `host.run.abandoned` event per stale run.

use audit::{RunStatus, ACTION_RUN_ABANDONED};
use host::state::AppState;
use std::path::PathBuf;
use std::sync::Mutex;

/// `AppState::new` opens the shared (temp) sessions database, so the tests in this
/// binary run one at a time.
static SERIAL: Mutex<()> = Mutex::new(());

fn unique_dir(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "riscdom-abandoned-{tag}-{}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Write an open run the way a crashed host would leave it: start marker in the
/// chain, index row updated, no end.
fn seed_open_run(state: &AppState, run_id: &str) {
    let config = state.run_fingerprint();
    let detail = audit::run_start_detail(run_id, Some("s1"), None, None, &config);
    let mut store = state.audit.lock().expect("audit");
    let stored = store
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
            fingerprint: audit::fingerprint(&config),
            fingerprint_schema: audit::FINGERPRINT_SCHEMA_V1.to_string(),
            started_at_ms: stored.event.timestamp_ms,
            ended_at_ms: None,
            start_seq: stored.id,
            end_seq: None,
            status: RunStatus::Open,
        })
        .expect("index");
}

fn runs(state: &AppState) -> Vec<audit::RunRecord> {
    state
        .audit
        .lock()
        .expect("audit")
        .list_runs(50)
        .expect("list")
}

fn abandoned_events(state: &AppState) -> usize {
    state
        .audit
        .lock()
        .expect("audit")
        .list(audit::EventFilter::default(), 1000)
        .expect("list")
        .iter()
        .filter(|e| e.event.action == ACTION_RUN_ABANDONED)
        .count()
}

#[test]
fn an_open_run_from_a_previous_process_is_abandoned_at_startup() {
    let _guard = SERIAL.lock().expect("serial");
    let dir = unique_dir("stale");

    {
        let crashed = AppState::new(&dir).expect("first start");
        seed_open_run(&crashed, "run_crashed");
        assert_eq!(runs(&crashed)[0].status, RunStatus::Open);
    } // the "process" disappears without closing the run

    let restarted = AppState::new(&dir).expect("restart");
    let runs = runs(&restarted);
    assert_eq!(runs.len(), 1);
    assert_eq!(
        runs[0].status,
        RunStatus::Abandoned,
        "the startup hook must make the stale run legible"
    );
    assert_eq!(runs[0].end_seq, None, "no fabricated end");
    assert_eq!(abandoned_events(&restarted), 1);
    assert!(matches!(
        audit::verify_chain(&restarted.audit.lock().expect("audit")).expect("verify"),
        audit::ChainStatus::Intact { .. }
    ));
    assert!(restarted
        .audit
        .lock()
        .expect("audit")
        .check_run_index()
        .expect("check")
        .is_empty());
}

#[test]
fn abandoning_is_idempotent_across_restarts() {
    let _guard = SERIAL.lock().expect("serial");
    let dir = unique_dir("idempotent");

    {
        let crashed = AppState::new(&dir).expect("first start");
        seed_open_run(&crashed, "run_crashed");
    }
    let second = AppState::new(&dir).expect("second start");
    assert_eq!(abandoned_events(&second), 1, "abandoned once");
    drop(second);

    let third = AppState::new(&dir).expect("third start");
    assert_eq!(
        abandoned_events(&third),
        1,
        "a later start must not append a second marker"
    );
    assert_eq!(runs(&third)[0].status, RunStatus::Abandoned);
    assert_eq!(
        third.abandon_stale_runs().expect("explicit"),
        Vec::<String>::new()
    );
    assert_eq!(abandoned_events(&third), 1);
}

#[test]
fn a_run_this_process_started_is_never_abandoned() {
    let _guard = SERIAL.lock().expect("serial");
    let dir = unique_dir("live");
    let state = AppState::new(&dir).expect("start");

    // Started by this process and still open: the hook must leave it alone, even
    // when it is invoked explicitly.
    seed_open_run(&state, "run_live");
    assert_eq!(
        state.abandon_stale_runs().expect("hook"),
        Vec::<String>::new(),
        "an in-flight run is not stale"
    );
    assert_eq!(runs(&state)[0].status, RunStatus::Open);
    assert_eq!(abandoned_events(&state), 0);
}
