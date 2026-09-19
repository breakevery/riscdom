//! v0.5 batches 1–4 — exporting one run's record.
//!
//! A run-scoped export must be **self-contained** (batch 4): it runs from the
//! chain's first event to the event that closes the run, so its first line is
//! anchored at genesis and the file can be judged on its own — and it must stop at
//! the run's end, never spilling into what came after. The interval tests below pin
//! which events belong to the run; the export tests pin what the file contains.
//! The whole-log export and the run export share one writer, so the two cannot drift
//! apart.

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
fn the_export_starts_at_genesis_and_ends_at_the_run() {
    let (store, record) = store_with_a_finished_run();
    let file = unique_file("exact");
    let to = store
        .run_end(&record)
        .expect("a finished run has a closing event");

    let written = store
        .export_self_contained_jsonl(to, &file)
        .expect("export");

    let exported = lines(&file);
    assert_eq!(written, exported.len());
    let ids: Vec<i64> = exported.iter().map(|l| l["id"].as_i64().unwrap()).collect();
    let first_id = store.all().unwrap().first().unwrap().id;
    assert_eq!(ids, (first_id..=to).collect::<Vec<_>>(), "{ids:?}");

    // The anchor is genesis: this is what lets an empty database verify the file.
    assert_eq!(exported[0]["prev_hash"], audit::GENESIS_PREV_HASH);
    assert_eq!(exported[0]["id"].as_i64(), Some(first_id));

    // The run's metadata travels inside the file: no header line was needed.
    let start_line = exported
        .iter()
        .find(|l| l["action"] == audit::ACTION_RUN_START)
        .expect("the run.start is in the file");
    assert_eq!(start_line["actor"], "host");
    assert_eq!(start_line["detail"]["run_id"], "run_a");
    assert!(
        start_line["detail"]["fingerprint_json"].is_string(),
        "{start_line}"
    );

    // It ends on `run.end`, and nothing after the run is in the file.
    let end_line = exported.last().unwrap();
    assert_eq!(end_line["action"], audit::ACTION_RUN_END);
    assert_eq!(end_line["detail"]["status"], "ok");
    let actions: Vec<&str> = exported
        .iter()
        .map(|l| l["action"].as_str().unwrap())
        .collect();
    for leaked in ["session.close", "host.stop"] {
        assert!(
            !actions.contains(&leaked),
            "`{leaked}` is after the run and must not be exported"
        );
    }
    // What precedes the run is part of the record: it is where the run happened.
    for included in ["host.start", "session.create"] {
        assert!(
            actions.contains(&included),
            "`{included}` precedes the run and belongs in a self-contained record"
        );
    }
}

#[test]
fn every_exported_line_still_verifies_on_its_own() {
    let (store, record) = store_with_a_finished_run();
    let file = unique_file("verify");
    let to = store.run_end(&record).unwrap();
    store.export_self_contained_jsonl(to, &file).unwrap();

    let exported = lines(&file);
    assert!(!exported.is_empty());
    assert_eq!(exported[0]["prev_hash"], audit::GENESIS_PREV_HASH);

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
fn a_whole_chain_export_is_byte_for_byte_the_plain_whole_log_export() {
    // The self-contained form is a prefix of the chain, not a second format: taking
    // the whole prefix gives exactly what `export_jsonl` writes.
    let (store, _record) = store_with_a_finished_run();
    let whole = unique_file("whole");
    let full = unique_file("full");

    let all = store.all().unwrap();
    let written_all = store.export_jsonl(&whole).unwrap();
    let written_full = store
        .export_self_contained_jsonl(all.last().unwrap().id, &full)
        .unwrap();

    assert_eq!(written_all, all.len());
    assert_eq!(written_full, all.len());
    assert_eq!(
        std::fs::read_to_string(&whole).unwrap(),
        std::fs::read_to_string(&full).unwrap(),
        "an export is a prefix of the chain, not a second format"
    );
}

#[test]
fn an_export_before_the_first_event_writes_an_empty_file() {
    let (store, _record) = store_with_a_finished_run();
    let file = unique_file("empty");
    let first_id = store.all().unwrap().first().unwrap().id;

    let written = store
        .export_self_contained_jsonl(first_id - 1, &file)
        .unwrap();

    assert_eq!(written, 0);
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "");
}

#[test]
fn a_run_that_opens_the_chain_exports_only_itself() {
    // The boundary case: `run.start` is the first event, so the run's own interval
    // and the self-contained prefix are the same three events.
    let mut store = AuditStore::in_memory().unwrap();
    let start = store
        .append(event(
            "host",
            audit::ACTION_RUN_START,
            audit::run_start_detail("run_first", None, None, None, &serde_json::json!({})),
        ))
        .unwrap();
    store
        .append(event(
            "agent",
            "agent.tool.call",
            serde_json::json!({ "name": "compile" }),
        ))
        .unwrap();
    let end = store
        .append(event(
            "host",
            audit::ACTION_RUN_END,
            audit::run_end_detail("run_first", RunStatus::Ok, "done"),
        ))
        .unwrap();
    let (runs, _) = store.derive_runs().unwrap();
    let record = &runs[0];

    let to = store.run_end(record).unwrap();
    assert_eq!((start.id, to), (1, end.id));

    let file = unique_file("first");
    store.export_self_contained_jsonl(to, &file).unwrap();
    let exported = lines(&file);
    assert_eq!(exported.len(), 3);
    assert_eq!(exported[0]["prev_hash"], audit::GENESIS_PREV_HASH);
    assert_eq!(exported[0]["action"], audit::ACTION_RUN_START);
    assert_eq!(exported.last().unwrap()["action"], audit::ACTION_RUN_END);
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
    assert_eq!(
        store.run_end(record).unwrap(),
        marker_id,
        "the export cuts at the abandoned marker"
    );

    let file = unique_file("abandoned");
    store.export_self_contained_jsonl(to, &file).unwrap();
    let exported = lines(&file);
    assert_eq!(exported[0]["prev_hash"], audit::GENESIS_PREV_HASH);
    let ids: Vec<i64> = exported.iter().map(|l| l["id"].as_i64().unwrap()).collect();
    assert_eq!(ids, (1..=to).collect::<Vec<_>>(), "{ids:?}");

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
        resumed_from_snapshot: None,
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
        resumed_from_snapshot: None,
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
