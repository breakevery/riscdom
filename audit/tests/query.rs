//! Stage 4b — query API, get/count, and JSONL export.

use audit::{AuditEvent, AuditStore, EventFilter};
use std::path::PathBuf;

fn ev(ts: i64, actor: &str, action: &str) -> AuditEvent {
    AuditEvent {
        timestamp_ms: ts,
        actor: actor.into(),
        action: action.into(),
        detail: serde_json::json!({ "ts": ts }),
        agent_id: None,
    }
}

fn seeded() -> AuditStore {
    let mut s = AuditStore::in_memory().expect("store");
    s.append(ev(1000, "sandbox", "vm.start")).expect("append");
    s.append(ev(2000, "agent", "llm.request")).expect("append");
    s.append(ev(3000, "sandbox", "vm.stop")).expect("append");
    s.append(ev(4000, "human", "human.pause")).expect("append");
    s
}

fn ids(events: &[audit::StoredEvent]) -> Vec<i64> {
    events.iter().map(|e| e.id).collect()
}

#[test]
fn list_filters_by_actor() {
    let store = seeded();
    let got = store
        .list(
            EventFilter {
                actor: Some("sandbox".into()),
                ..Default::default()
            },
            100,
        )
        .expect("list");
    assert_eq!(ids(&got), vec![1, 3]);
}

#[test]
fn list_filters_by_action_prefix() {
    let store = seeded();
    let got = store
        .list(
            EventFilter {
                action_prefix: Some("vm.".into()),
                ..Default::default()
            },
            100,
        )
        .expect("list");
    assert_eq!(ids(&got), vec![1, 3]);

    let human = store
        .list(
            EventFilter {
                action_prefix: Some("human.".into()),
                ..Default::default()
            },
            100,
        )
        .expect("list");
    assert_eq!(ids(&human), vec![4]);
}

#[test]
fn list_filters_by_time_range() {
    let store = seeded();
    let got = store
        .list(
            EventFilter {
                from_ms: Some(2000),
                to_ms: Some(3000),
                ..Default::default()
            },
            100,
        )
        .expect("list");
    assert_eq!(ids(&got), vec![2, 3]);
}

#[test]
fn list_respects_limit() {
    let store = seeded();
    let got = store.list(EventFilter::default(), 2).expect("list");
    assert_eq!(ids(&got), vec![1, 2]);
}

#[test]
fn list_returns_the_other_end_when_descending() {
    // v1.0 gap 3/N, batch E: the pagination cursor asks for the rows nearest the window's
    // **upper** bound, which an ascending scan cannot answer — `LIMIT` would cut off the
    // oldest end. `descending` is the same query from the other end, and `false` (the
    // derived default) is the behaviour every existing caller had.
    let store = seeded();

    let newest = store
        .list(
            EventFilter {
                descending: true,
                ..Default::default()
            },
            2,
        )
        .expect("list");
    assert_eq!(ids(&newest), vec![4, 3]);

    // The cursor shape itself: everything strictly older than id 3, newest first.
    let page = store
        .list(
            EventFilter {
                to_id: Some(2),
                descending: true,
                ..Default::default()
            },
            2,
        )
        .expect("list");
    assert_eq!(ids(&page), vec![2, 1]);

    // The same window ascending is the *oldest* rows, which is exactly the trap the
    // cursor exists to avoid: the same filter, the other end of `LIMIT`.
    let ascending = store
        .list(
            EventFilter {
                to_id: Some(2),
                ..Default::default()
            },
            2,
        )
        .expect("list");
    assert_eq!(ids(&ascending), vec![1, 2]);
}

#[test]
fn get_by_id_and_count() {
    let store = seeded();
    assert_eq!(store.count().expect("count"), 4);
    let two = store.get(2).expect("get").expect("present");
    assert_eq!(two.event.action, "llm.request");
    assert_eq!(two.event.actor, "agent");
    assert!(store.get(99).expect("get").is_none());
}

#[test]
fn export_jsonl_is_independently_parseable() {
    let store = seeded();
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("export");
    std::fs::create_dir_all(&dir).expect("dir");
    let out = dir.join("audit.jsonl");

    let n = store.export_jsonl(&out).expect("export");
    assert_eq!(n, 4);

    let text = std::fs::read_to_string(&out).expect("read back");
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 4);

    for (i, line) in lines.iter().enumerate() {
        let v: serde_json::Value = serde_json::from_str(line).expect("parse line");
        assert_eq!(v["id"].as_i64().unwrap(), i as i64 + 1);
        for key in [
            "timestamp_ms",
            "actor",
            "action",
            "detail",
            "prev_hash",
            "hash",
        ] {
            assert!(!v[key].is_null(), "missing {key} in line {line}");
        }
        assert_eq!(v["hash"].as_str().unwrap().len(), 64);
    }
}
