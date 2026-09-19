//! v0.5 batch 1 — exporting one run's audit interval.
//!
//! A range export must be **exactly** the chain slice a run occupies: nothing from
//! before it, nothing from after it, and every line still verifiable on its own.
//! The whole-log export and the range export share one writer, so the two cannot
//! drift apart.

use audit::{
    compute_hash, run_interval, AuditEvent, AuditStore, EventFilter, RunRecord, RunStatus,
};
use std::path::PathBuf;

fn unique_file(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "riscdom-audit-range-{tag}-{}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join("export.jsonl")
}

fn event(actor: &str, action: &str, detail: serde_json::Value) -> AuditEvent {
    AuditEvent::new(actor, action, detail)
}

/// A chain shaped like a real one: two events, a run, two events inside it, the
/// `run.end`, then two events after it.
fn store_with_a_finished_run() -> (AuditStore, RunRecord) {
    let mut store = AuditStore::in_memory().expect("store");
    store
        .append(event("host", "host.start", serde_json::json!({})))
        .unwrap();
    store
        .append(event(
            "human",
            "session.create",
            serde_json::json!({ "id": "s1" }),
        ))
        .unwrap();

    let config = serde_json::json!({
        "schema": { "app_version": "0.5.0" },
        "llm": { "provider_id": "deepseek", "model": "m" },
    });
    let start = store
        .append(event(
            "host",
            audit::ACTION_RUN_START,
            audit::run_start_detail("run_a", Some("s1"), None, None, &config),
        ))
        .unwrap();
    store
        .append(event(
            "agent",
            "agent.tool.call",
            serde_json::json!({ "name": "compile" }),
        ))
        .unwrap();
    store
        .append(event("sandbox", "vm.start", serde_json::json!({})))
        .unwrap();
    let end = store
        .append(event(
            "host",
            audit::ACTION_RUN_END,
            audit::run_end_detail("run_a", RunStatus::Ok, "done"),
        ))
        .unwrap();
    store
        .append(event("human", "session.close", serde_json::json!({})))
        .unwrap();
    store
        .append(event("host", "host.stop", serde_json::json!({})))
        .unwrap();

    let (runs, report) = store.derive_runs().unwrap();
    assert_eq!(report.orphans, 0, "{report:?}");
    let record = runs
        .into_iter()
        .find(|r| r.run_id == "run_a")
        .expect("the chain has the run");
    assert_eq!(record.start_seq, start.id);
    assert_eq!(record.end_seq, Some(end.id));
    (store, record)
}

fn lines(path: &std::path::Path) -> Vec<serde_json::Value> {
    std::fs::read_to_string(path)
        .expect("export file")
        .lines()
        .map(|line| serde_json::from_str(line).expect("one JSON object per line"))
        .collect()
}

#[test]
fn the_export_holds_exactly_the_runs_interval() {
    let (store, record) = store_with_a_finished_run();
    let file = unique_file("exact");
    let (from, to) =
        run_interval(&record, &store.all().unwrap()).expect("a finished run has an interval");

    let written = store
        .export_interval_jsonl(from, to, &file)
        .expect("export");

    let exported = lines(&file);
    assert_eq!(written, exported.len());
    let ids: Vec<i64> = exported.iter().map(|l| l["id"].as_i64().unwrap()).collect();
    assert_eq!(ids, (from..=to).collect::<Vec<_>>(), "{ids:?}");

    // Nothing from before the run, nothing from after it.
    let actions: Vec<&str> = exported
        .iter()
        .map(|l| l["action"].as_str().unwrap())
        .collect();
    assert_eq!(
        actions,
        vec![
            audit::ACTION_RUN_START,
            "agent.tool.call",
            "vm.start",
            audit::ACTION_RUN_END,
        ],
        "{actions:?}"
    );
    for leaked in ["host.start", "session.create", "session.close", "host.stop"] {
        assert!(
            !actions.contains(&leaked),
            "`{leaked}` is outside the run and must not be exported"
        );
    }

    // The run's metadata travels inside the file: no header line was needed.
    let start_line = &exported[0];
    assert_eq!(start_line["actor"], "host");
    let detail = &start_line["detail"];
    assert_eq!(detail["run_id"], "run_a");
    assert!(detail["fingerprint_json"].is_string(), "{detail}");
    let end_line = exported.last().unwrap();
    assert_eq!(end_line["detail"]["status"], "ok");
}

#[test]
fn every_exported_line_still_verifies_on_its_own() {
    let (store, record) = store_with_a_finished_run();
    let file = unique_file("verify");
    let (from, to) = run_interval(&record, &store.all().unwrap()).unwrap();
    store.export_interval_jsonl(from, to, &file).unwrap();

    let exported = lines(&file);
    assert!(!exported.is_empty());

    let mut previous: Option<String> = None;
    for line in &exported {
        let event = AuditEvent {
            timestamp_ms: line["timestamp_ms"].as_i64().unwrap(),
            actor: line["actor"].as_str().unwrap().to_string(),
            action: line["action"].as_str().unwrap().to_string(),
            detail: line["detail"].clone(),
        };
        let stored_prev = line["prev_hash"].as_str().unwrap();
        let recomputed = compute_hash(
            stored_prev,
            &event,
            &serde_json::to_string(&line["detail"]).unwrap(),
        );
        assert_eq!(
            recomputed,
            line["hash"].as_str().unwrap(),
            "line {} does not re-hash: an outside tool could not check it",
            line["id"]
        );
        if let Some(prev) = &previous {
            assert_eq!(
                stored_prev, prev,
                "line {} does not link to the line before it",
                line["id"]
            );
        }
        previous = Some(line["hash"].as_str().unwrap().to_string());
    }
}

#[test]
fn a_full_range_export_is_byte_for_byte_the_whole_log_export() {
    let (store, record) = store_with_a_finished_run();
    let whole = unique_file("whole");
    let (from, to) = run_interval(&record, &store.all().unwrap()).unwrap();
    let full = unique_file("full");

    let all = store.all().unwrap();
    let written_all = store.export_jsonl(&whole).unwrap();
    let written_full = store
        .export_interval_jsonl(all.first().unwrap().id, all.last().unwrap().id, &full)
        .unwrap();

    assert_eq!(written_all, all.len());
    assert_eq!(written_full, all.len());
    assert_eq!(
        std::fs::read_to_string(&whole).unwrap(),
        std::fs::read_to_string(&full).unwrap(),
        "a range export is a slice of the chain, not a second format"
    );
    assert!(from < to);
}

#[test]
fn an_empty_range_writes_an_empty_file() {
    let (store, record) = store_with_a_finished_run();
    let file = unique_file("empty");
    let (_, to) = run_interval(&record, &store.all().unwrap()).unwrap();

    let written = store
        .export_interval_jsonl(to + 10, to + 20, &file)
        .unwrap();

    assert_eq!(written, 0);
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "");
}

#[test]
fn a_backwards_range_is_refused() {
    let (store, record) = store_with_a_finished_run();
    let file = unique_file("backwards");
    let (from, _) = run_interval(&record, &store.all().unwrap()).unwrap();

    let error = store
        .export_interval_jsonl(from + 3, from, &file)
        .expect_err("a backwards range is a caller bug");
    assert!(
        error.to_string().contains("empty audit interval"),
        "{error}"
    );
    assert!(!file.exists(), "nothing may be written on a refusal");
}

#[test]
fn a_run_without_an_end_has_no_interval() {
    let mut store = AuditStore::in_memory().unwrap();
    store
        .append(event(
            "host",
            audit::ACTION_RUN_START,
            audit::run_start_detail("run_open", None, None, None, &serde_json::json!({})),
        ))
        .unwrap();
    let (runs, _) = store.derive_runs().unwrap();
    let record = &runs[0];
    assert_eq!(record.status, RunStatus::Open);
    assert_eq!(record.end_seq, None);

    let error = run_interval(record, &store.all().unwrap())
        .expect_err("an open run has no interval to export");
    let text = error.to_string();
    assert!(text.contains("has not ended"), "{text}");
    assert!(text.contains("run_open"), "{text}");
    assert!(text.contains("open"), "{text}");
}

/// A chain with one abandoned run and a second run after it: the abandoned run
/// never got a `run.end`, so its interval has to end at its own marker.
fn store_with_an_abandoned_run() -> (AuditStore, i64, i64) {
    let mut store = AuditStore::in_memory().expect("store");
    let start = store
        .append(event(
            "host",
            audit::ACTION_RUN_START,
            audit::run_start_detail("run_stale", Some("s1"), None, None, &serde_json::json!({})),
        ))
        .expect("run.start");
    store
        .append(event(
            "agent",
            "agent.tool.call",
            serde_json::json!({ "name": "compile" }),
        ))
        .unwrap();
    let marker = store
        .append(event(
            "host",
            audit::ACTION_RUN_ABANDONED,
            serde_json::json!({ "run_id": "run_stale", "detected_at_ms": 1 }),
        ))
        .expect("host.run.abandoned");

    // A later run: its events must never enter the abandoned one's interval.
    store
        .append(event(
            "host",
            audit::ACTION_RUN_START,
            audit::run_start_detail("run_later", None, None, None, &serde_json::json!({})),
        ))
        .expect("run.start (later)");
    store
        .append(event("sandbox", "vm.start", serde_json::json!({})))
        .unwrap();

    (store, start.id, marker.id)
}

#[test]
fn an_abandoned_run_exports_up_to_its_abandoned_event() {
    // Nothing fabricates a `run.end`, so the index leaves `end_seq` empty — but the
    // chain does say where the run stopped, and the export ends on that line.
    let (store, start_id, marker_id) = store_with_an_abandoned_run();
    let (runs, report) = store.derive_runs().unwrap();
    assert_eq!(report.abandoned, 1, "{report:?}");

    let record = runs
        .iter()
        .find(|r| r.run_id == "run_stale")
        .expect("the chain has the abandoned run");
    assert_eq!(record.status, RunStatus::Abandoned);
    assert_eq!(record.end_seq, None, "the index invents no end");
    assert_eq!(store.get_run("run_stale").unwrap(), None, "not indexed yet");

    let (from, to) = run_interval(record, &store.all().unwrap())
        .expect("an abandoned run has an interval: `run.start` .. `host.run.abandoned`");
    assert_eq!(from, start_id);
    assert_eq!(to, marker_id);

    let file = unique_file("abandoned");
    store.export_interval_jsonl(from, to, &file).unwrap();
    let exported = lines(&file);
    let ids: Vec<i64> = exported.iter().map(|l| l["id"].as_i64().unwrap()).collect();
    assert_eq!(ids, (from..=to).collect::<Vec<_>>(), "{ids:?}");

    // The last line is the event that explains the run: self-describing, no header.
    let last = exported.last().unwrap();
    assert_eq!(last["action"], audit::ACTION_RUN_ABANDONED);
    assert_eq!(last["detail"]["run_id"], "run_stale");

    // The next run is out of the interval, so the export cannot bleed into it.
    let actions: Vec<&str> = exported
        .iter()
        .map(|l| l["action"].as_str().unwrap())
        .collect();
    assert_eq!(
        actions,
        vec![
            audit::ACTION_RUN_START,
            "agent.tool.call",
            audit::ACTION_RUN_ABANDONED,
        ],
        "{actions:?}"
    );
    assert!(
        !ids.contains(&(marker_id + 1)),
        "`run_later` starts right after the marker and must not be exported"
    );
}

#[test]
fn an_abandoned_run_without_a_marker_is_refused() {
    // A status with no event behind it has no interval either: the refusal names
    // the missing marker instead of guessing where the run stopped.
    let record = RunRecord {
        run_id: "run_ghost".into(),
        session_id: None,
        parent_run_id: None,
        fingerprint: "f".into(),
        fingerprint_schema: "s".into(),
        started_at_ms: 0,
        ended_at_ms: None,
        start_seq: 1,
        end_seq: None,
        status: RunStatus::Abandoned,
    };

    let text = run_interval(&record, &[]).unwrap_err().to_string();
    assert!(text.contains("abandoned"), "{text}");
    assert!(text.contains("run_ghost"), "{text}");
}

#[test]
fn a_marker_for_another_run_does_not_close_this_one() {
    // The window is bounded by the next `run.start`, but it is also keyed by run id.
    let mut store = AuditStore::in_memory().unwrap();
    let start = store
        .append(event(
            "host",
            audit::ACTION_RUN_START,
            audit::run_start_detail("run_mine", None, None, None, &serde_json::json!({})),
        ))
        .unwrap();
    store
        .append(event(
            "host",
            audit::ACTION_RUN_ABANDONED,
            serde_json::json!({ "run_id": "run_other", "detected_at_ms": 1 }),
        ))
        .unwrap();
    let record = RunRecord {
        run_id: "run_mine".into(),
        session_id: None,
        parent_run_id: None,
        fingerprint: "f".into(),
        fingerprint_schema: "s".into(),
        started_at_ms: 0,
        ended_at_ms: None,
        start_seq: start.id,
        end_seq: None,
        status: RunStatus::Abandoned,
    };

    assert!(run_interval(&record, &store.all().unwrap()).is_err());
}

#[test]
fn the_index_path_yields_the_same_interval_as_the_chain() {
    let (mut store, record) = store_with_a_finished_run();
    store.index_run_start(&record).unwrap();
    let from_index = store.get_run("run_a").unwrap().expect("indexed run");

    assert_eq!(from_index, record);
    assert_eq!(
        store.run_interval(&from_index).unwrap(),
        store.run_interval(&record).unwrap()
    );

    // An unknown run is simply absent — the caller decides what that means.
    assert!(store.get_run("run_missing").unwrap().is_none());
}

#[test]
fn the_id_filter_bounds_are_inclusive() {
    let (store, record) = store_with_a_finished_run();
    let (from, to) = run_interval(&record, &store.all().unwrap()).unwrap();

    let only_first = store
        .list(
            EventFilter {
                from_id: Some(from),
                to_id: Some(from),
                ..EventFilter::default()
            },
            100,
        )
        .unwrap();
    assert_eq!(only_first.len(), 1);
    assert_eq!(only_first[0].id, from);
    assert_eq!(only_first[0].event.action, audit::ACTION_RUN_START);

    let whole = store
        .list(
            EventFilter {
                from_id: Some(from),
                to_id: Some(to),
                ..EventFilter::default()
            },
            100,
        )
        .unwrap();
    assert_eq!(whole.len() as i64, to - from + 1);
    assert_eq!(whole.last().unwrap().id, to);
}
