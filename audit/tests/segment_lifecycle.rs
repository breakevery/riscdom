//! Opening and closing a segment, and what it writes (v1.0 M5-1b).
//!
//! [docs/audit-v2.md](../../docs/audit-v2.md) §3 and §6 are the shape; these check it: the `segments`
//! row carries the cross-segment reference (**the head before the event**), the lifecycle event lands on
//! the **main chain** (`segment_id IS NULL`), and the chain still verifies — a segment's life is recorded
//! by ordinary appends, never by a rewrite.

use audit::{
    verify_chain, AuditEvent, AuditStore, ChainStatus, SegmentKind, SegmentState,
    ACTION_SEGMENT_CLOSED, ACTION_SEGMENT_OPENED,
};
use std::path::PathBuf;

fn temp_db(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let dir =
        std::env::temp_dir().join(format!("riscdom-segment-life-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("mkdir");
    dir.join(format!("audit-{nanos}.db"))
}

fn count(conn: &rusqlite::Connection, sql: &str) -> i64 {
    conn.query_row(sql, [], |row| row.get(0)).expect("query")
}

fn temp_dir(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "riscdom-segment-store-{tag}-{}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).expect("mkdir");
    dir
}

#[test]
fn opening_a_segment_writes_the_row_and_the_event_on_the_main_chain() {
    let path = temp_db("open");
    let mut store = AuditStore::open(&path).expect("open");
    store
        .append(AuditEvent::new(
            "sandbox",
            "vm.start",
            serde_json::json!({ "n": 1 }),
        ))
        .expect("append");
    let head_before = store.last_hash().expect("head").expect("a head");

    let segment = store
        .open_segment(SegmentKind::Temporary)
        .expect("open segment");
    assert_eq!(segment.kind, SegmentKind::Temporary);
    assert_eq!(segment.state, SegmentState::Open);
    assert!(segment.head_hash.is_none(), "no events in it yet");
    assert_eq!(
        segment.head_prev_chain.as_deref(),
        Some(head_before.as_str()),
        "the cross-segment reference is the head **before** the event"
    );

    // The row is there, open.
    let rows = store.segments().expect("segments");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].segment_id, segment.segment_id);
    assert_eq!(rows[0].state, SegmentState::Open);
    assert_eq!(
        store
            .segment(&segment.segment_id)
            .expect("row")
            .expect("present")
            .kind,
        SegmentKind::Temporary
    );

    // The event is on the chain, with the detail §6 records.
    let events = store.all().expect("all");
    let opened = events
        .iter()
        .find(|event| event.event.action == ACTION_SEGMENT_OPENED)
        .expect("a segment_opened event");
    assert_eq!(
        opened.event.detail["segment_id"],
        serde_json::json!(segment.segment_id)
    );
    assert_eq!(opened.event.detail["kind"], serde_json::json!("temporary"));
    assert_eq!(
        opened.event.detail["head_prev_chain"],
        serde_json::json!(head_before)
    );

    // The chain verifies, and grew by exactly the one event.
    assert!(matches!(
        verify_chain(&store).expect("verify"),
        ChainStatus::Intact { length: 2 }
    ));
    drop(store);

    // The lifecycle event is a **main-chain** event: every row is `segment_id IS NULL`. No event
    // carries a segment id, because M5-1b writes none of a segment's own events.
    let conn = rusqlite::Connection::open(&path).expect("conn");
    assert_eq!(
        count(
            &conn,
            "SELECT COUNT(*) FROM audit_events WHERE segment_id IS NULL"
        ),
        2
    );
    assert_eq!(
        count(
            &conn,
            "SELECT COUNT(*) FROM audit_events WHERE segment_id IS NOT NULL"
        ),
        0
    );
}

#[test]
fn closing_a_segment_updates_the_row_and_writes_the_event() {
    let path = temp_db("close");
    let mut store = AuditStore::open(&path).expect("open");
    let segment = store
        .open_segment(SegmentKind::Temporary)
        .expect("open segment");

    let closed = store
        .close_segment(&segment.segment_id)
        .expect("close segment");
    assert_eq!(closed.state, SegmentState::Closed);
    assert!(closed.closed_at_ms.is_some(), "a close is stamped");

    let row = store
        .segment(&segment.segment_id)
        .expect("row")
        .expect("present");
    assert_eq!(row.state, SegmentState::Closed);
    assert_eq!(row.closed_at_ms, closed.closed_at_ms);
    assert_eq!(
        row.head_prev_chain, segment.head_prev_chain,
        "closing does not move the reference"
    );

    let events = store.all().expect("all");
    let closed_event = events
        .iter()
        .find(|event| event.event.action == ACTION_SEGMENT_CLOSED)
        .expect("a segment_closed event");
    assert_eq!(
        closed_event.event.detail["segment_id"],
        serde_json::json!(segment.segment_id)
    );
    assert_eq!(
        closed_event.event.detail["closed_at_ms"],
        serde_json::json!(closed.closed_at_ms)
    );

    // Two lifecycle acts, two events, one chain, still intact.
    assert!(matches!(
        verify_chain(&store).expect("verify"),
        ChainStatus::Intact { length: 2 }
    ));
}

#[test]
fn a_second_segment_points_at_the_head_the_opening_event_left() {
    // The ordering matters: the reference is the head **before** the opening event, so a chain read in
    // order shows the segment continuing from a position its own event does not overwrite.
    let path = temp_db("second");
    let mut store = AuditStore::open(&path).expect("open");
    store
        .append(AuditEvent::new("host", "host.start", serde_json::json!({})))
        .expect("append");
    let head_before_first = store.last_hash().expect("head").expect("a head");
    let first = store
        .open_segment(SegmentKind::Temporary)
        .expect("first segment");
    assert_eq!(
        first.head_prev_chain.as_deref(),
        Some(head_before_first.as_str())
    );

    // The opening event is itself the head now, so a second segment references *it*.
    let head_after_first = store.last_hash().expect("head").expect("a head");
    assert_ne!(head_after_first, head_before_first);
    let second = store
        .open_segment(SegmentKind::Temporary)
        .expect("second segment");
    assert_eq!(
        second.head_prev_chain.as_deref(),
        Some(head_after_first.as_str())
    );
    assert_ne!(first.head_prev_chain, second.head_prev_chain);
    assert_eq!(store.segments().expect("segments").len(), 2);
    assert!(matches!(
        verify_chain(&store).expect("verify"),
        ChainStatus::Intact { length: 3 }
    ));
}

#[test]
fn closing_an_unknown_segment_is_an_error_not_a_silent_write() {
    let path = temp_db("unknown");
    let mut store = AuditStore::open(&path).expect("open");
    let error = store.close_segment("seg-nobody").expect_err("refused");
    assert!(error.to_string().contains("seg-nobody"), "{error}");
    // Nothing was appended for a close that did not happen.
    assert_eq!(store.count().expect("count"), 0);
    assert!(store.segments().expect("segments").is_empty());
}

#[test]
fn a_segment_gets_its_own_store_and_the_main_chain_is_not_touched() {
    // Physical shape (b), v1.0 M5-1c: a temporary segment is its **own** SQLite file beside the main
    // chain, so the main chain stays one linear, verifiable chain.
    let dir = temp_dir("own");
    let mut main = AuditStore::open(&dir.join("audit.db")).expect("main");
    main.append(AuditEvent::new(
        "sandbox",
        "vm.start",
        serde_json::json!({}),
    ))
    .expect("append");
    let segment = main
        .open_segment(SegmentKind::Temporary)
        .expect("open segment");

    // The path is `<audit dir>/audit-segments/<segment_id>.db`, and nothing is there until one is opened.
    let segment_path = audit::segment_db_path_in(&dir, &segment.segment_id);
    assert_eq!(
        segment_path,
        dir.join("audit-segments")
            .join(format!("{}.db", segment.segment_id))
    );
    assert!(!segment_path.exists(), "no file until one is opened");

    // Its own store: the same schema, and its own genesis.
    let mut seg = AuditStore::open_segment_store(&dir, &segment.segment_id).expect("segment store");
    assert!(segment_path.exists(), "opening created the file");
    seg.append(AuditEvent::new(
        "sandbox",
        "vm.exec",
        serde_json::json!({ "in": "segment" }),
    ))
    .expect("append");
    assert!(matches!(
        verify_chain(&seg).expect("verify"),
        ChainStatus::Intact { length: 1 }
    ));
    let first = &seg.all().expect("all")[0];
    assert_eq!(
        first.prev_hash,
        audit::GENESIS_PREV_HASH,
        "a segment chain starts at genesis, like any chain"
    );

    // The main chain is a different file, and the segment's writes did not touch it.
    assert!(matches!(
        verify_chain(&main).expect("verify"),
        ChainStatus::Intact { length: 2 }
    ));
    assert_eq!(main.count().expect("count"), 2);
    assert_eq!(
        main.segments().expect("segments").len(),
        1,
        "the segment is registered on the main chain, its events are not"
    );
}
