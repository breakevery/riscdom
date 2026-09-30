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

/// The anchor's second half: how long the chain was at the point the segment continues from
/// (v1.0 M6-5-1). A hash names a point; the length says *which* point, and it is read before the
/// segment's own opening event is appended.
#[test]
fn opening_a_segment_records_the_chain_length_it_continues_from() {
    let path = temp_db("anchor-length");
    let mut store = AuditStore::open(&path).expect("open");

    // An empty chain: the honest answer is "nothing yet" — the `None` both halves carry.
    let first = store
        .open_segment(SegmentKind::Temporary)
        .expect("open on an empty chain");
    assert_eq!(first.head_prev_chain, None);
    assert_eq!(first.head_prev_length, None, "no event to count");

    // Three more events, then a second segment: both halves name the same point.
    for n in 1..=3 {
        store
            .append(AuditEvent::new(
                "sandbox",
                "vm.start",
                serde_json::json!({ "n": n }),
            ))
            .expect("append");
    }
    let head_before = store.last_hash().expect("head").expect("a head");
    let id_before = store.last_id().expect("id").expect("an id");
    let count_before = store.count().expect("count");
    assert_eq!(id_before, count_before as i64, "the length is the last id");

    let second = store
        .open_segment(SegmentKind::Temporary)
        .expect("open again");
    assert_eq!(
        second.head_prev_chain.as_deref(),
        Some(head_before.as_str())
    );
    assert_eq!(second.head_prev_length, Some(id_before));

    // It survives a round trip through the table, and the chain is untouched by any of it.
    let rows = store.segments().expect("segments");
    let read_back = rows
        .iter()
        .find(|row| row.segment_id == second.segment_id)
        .expect("the second row");
    assert_eq!(read_back.head_prev_length, second.head_prev_length);
    assert_eq!(read_back.head_prev_chain, second.head_prev_chain);
    assert!(matches!(
        verify_chain(&store).expect("verify"),
        ChainStatus::Intact { .. }
    ));
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

/// A segment id names its owner, and stays safe to use as a file name (v1.0 M5-3c-2).
#[test]
fn a_segment_id_names_its_owner_and_stays_a_file_name() {
    let path = temp_db("owner");
    let mut store = AuditStore::open(&path).expect("open");
    let mine = store
        .open_segment_for(SegmentKind::Temporary, "dev-a")
        .expect("open for owner");
    assert!(
        mine.segment_id.starts_with("seg-dev-a-"),
        "the owner is in the name: {}",
        mine.segment_id
    );

    // A deployer's name is free-form, so a hostile one has to come out harmless: no separators, no
    // path that goes anywhere, and the id is still a single path component.
    let nasty = store
        .open_segment_for(SegmentKind::Temporary, "a/b\\c:d *e")
        .expect("open for a nasty owner");
    assert!(!nasty.segment_id.contains('/'), "{}", nasty.segment_id);
    assert!(!nasty.segment_id.contains('\\'), "{}", nasty.segment_id);
    assert_eq!(
        std::path::Path::new(&nasty.segment_id).components().count(),
        1,
        "one component: {}",
        nasty.segment_id
    );
    assert_eq!(audit::safe_owner(""), "node");
    assert_eq!(
        audit::safe_owner("..."),
        "node",
        "dots alone are not a name"
    );
    assert_eq!(audit::safe_owner("a b"), "a_b");
}

/// The range read answers exactly the span between two ids, and nothing for an empty one (v1.0 M5-3c-2).
#[test]
fn events_in_range_answers_the_span_between_two_ids() {
    let path = temp_db("range");
    let mut store = AuditStore::open(&path).expect("open");
    let mut ids = Vec::new();
    for n in 1..=3 {
        let stored = store
            .append(AuditEvent::new(
                "host",
                "host.test.range",
                serde_json::json!({ "n": n }),
            ))
            .expect("append");
        ids.push(stored.id);
    }
    assert_eq!(store.last_id().expect("last id"), Some(ids[2]));

    let middle = store.events_in_range(ids[1], ids[1]).expect("middle");
    assert_eq!(middle.len(), 1);
    assert_eq!(middle[0].id, ids[1]);

    // A span that excludes both markers — which is how a segment's own events are read.
    let between = store
        .events_in_range(ids[0] + 1, ids[2] - 1)
        .expect("between");
    assert_eq!(between.len(), 1);
    assert_eq!(between[0].id, ids[1]);

    assert!(
        store
            .events_in_range(ids[2], ids[0])
            .expect("inverted")
            .is_empty(),
        "an inverted range is an honest nothing, not an error"
    );
    let fresh = AuditStore::in_memory().expect("memory");
    assert_eq!(
        fresh.last_id().expect("last id"),
        None,
        "an empty chain has no id"
    );
    assert!(fresh.events_in_range(0, 0).expect("empty").is_empty());

    // Reading wrote nothing.
    assert_eq!(store.count().expect("count"), 3);
    assert!(matches!(
        verify_chain(&store).expect("verify"),
        ChainStatus::Intact { .. }
    ));
}

/// A segment that stood in on another node is adopted, rebuilt and merged here (v1.0 M5-3c-2).
#[test]
fn an_adopted_segment_is_rebuilt_and_merged() {
    let dir = temp_dir("adopted");
    let mut main = AuditStore::open(&dir.join("audit.db")).expect("open");

    main.adopt_segment("seg-peer-a-1", Some("headhash"), Some(7))
        .expect("adopt");
    let row = main.segment("seg-peer-a-1").expect("row").expect("present");
    assert_eq!(row.kind, SegmentKind::Temporary);
    assert_eq!(
        row.state,
        SegmentState::Closed,
        "it was closed where it stood in"
    );
    assert_eq!(row.head_prev_chain.as_deref(), Some("headhash"));
    // Both halves of the anchor are recorded, and neither is verified here (v1.0 M6-5-1).
    assert_eq!(row.head_prev_length, Some(7));
    assert!(
        main.adopt_segment("seg-peer-a-1", None, None).is_ok(),
        "adopting twice is not an error"
    );
    assert_eq!(
        main.segment("seg-peer-a-1")
            .expect("row")
            .expect("present")
            .head_prev_length,
        Some(7),
        "a second adopt does not overwrite the recorded anchor"
    );

    // The rebuild: the centre writes the delivered events into the segment's own store.
    {
        let mut segment =
            AuditStore::open_segment_store(&dir, "seg-peer-a-1").expect("segment store");
        segment
            .append(AuditEvent::new(
                "host",
                "host.test.stood-in",
                serde_json::json!({ "what": "a stand-in did this" }),
            ))
            .expect("append");
        assert!(matches!(
            verify_chain(&segment).expect("verify"),
            ChainStatus::Intact { length: 1 }
        ));
    }

    let before = main.count().expect("count");
    let outcome = main
        .merge_segment(&dir, "seg-peer-a-1")
        .expect("merge an adopted segment");
    assert!(
        matches!(outcome, audit::MergeOutcome::Folded { merged: 1 }),
        "got {outcome:?}"
    );
    assert_eq!(
        main.count().expect("count"),
        before + 2,
        "the transcribed event and the merged row"
    );
    assert!(matches!(
        verify_chain(&main).expect("verify"),
        ChainStatus::Intact { .. }
    ));
}
