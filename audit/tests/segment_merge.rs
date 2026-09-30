//! Merging a temporary segment into the main chain (v1.0 M5-2a).
//!
//! [docs/audit-v2.md](../../docs/audit-v2.md) §2/§8 fix the shape: a segment is its own file, so a merge is
//! **transcription** — the segment's events are read and appended to the main chain as new events, and the
//! `provisional` mark is cleared by writing the copies **without** it. The segment's own file is never
//! touched, and neither chain's `verify_chain` changes.

use audit::{
    verify_chain, AuditEvent, AuditStore, ChainStatus, MergeOutcome, SegmentKind, SegmentState,
    ACTION_SEGMENT_FORKED, ACTION_SEGMENT_MERGED,
};
use std::path::PathBuf;

fn temp_dir(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "riscdom-merge-{tag}-{}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).expect("mkdir");
    dir
}

/// An event whose detail carries the provisional mark (§33: what a temporary centre writes).
fn provisional_event(ts: i64, action: &str, n: i64) -> AuditEvent {
    AuditEvent {
        timestamp_ms: ts,
        actor: "sandbox".to_string(),
        action: action.to_string(),
        detail: serde_json::json!({ "n": n, "provisional": true }),
        agent_id: None,
    }
}

#[test]
fn a_conflict_free_segment_merges_by_transcription() {
    let dir = temp_dir("free");
    let mut main = AuditStore::open(&dir.join("audit.db")).expect("main");
    main.append(AuditEvent::new(
        "host",
        "host.start",
        serde_json::json!({ "n": 0 }),
    ))
    .expect("append");

    let segment = main
        .open_segment(SegmentKind::Temporary)
        .expect("open segment");
    main.close_segment(&segment.segment_id).expect("close");

    // The temporary centre's own file, with provisional events in it.
    let mut seg = AuditStore::open_segment_store(&dir, &segment.segment_id).expect("segment store");
    seg.append(provisional_event(1_000, "vm.start", 1))
        .expect("append");
    seg.append(provisional_event(2_000, "vm.stop", 2))
        .expect("append");

    let outcome = main
        .merge_segment(&dir, &segment.segment_id)
        .expect("merge");
    assert_eq!(
        outcome,
        MergeOutcome::Folded { merged: 2 },
        "both segment events were transcribed"
    );

    // The main chain: host.start + segment_opened + segment_closed + 2 transcribed + segment_merged.
    assert_eq!(main.count().expect("count"), 6);
    let events = main.all().expect("all");
    let transcribed: Vec<_> = events
        .iter()
        .filter(|event| event.event.action.starts_with("vm."))
        .collect();
    assert_eq!(
        transcribed.len(),
        2,
        "the segment's events are on the main chain"
    );
    for event in &transcribed {
        assert!(
            event.event.detail.get("provisional").is_none(),
            "the transcribed copy is written with the mark cleared: {:?}",
            event.event.detail
        );
        assert!(
            event.event.detail.get("n").is_some(),
            "and nothing else moved"
        );
    }

    // The merged event, and the row.
    let merged = events
        .iter()
        .find(|event| event.event.action == ACTION_SEGMENT_MERGED)
        .expect("a segment_merged event");
    assert_eq!(
        merged.event.detail["segment_id"],
        serde_json::json!(segment.segment_id)
    );
    assert_eq!(merged.event.detail["kind"], serde_json::json!("temporary"));
    assert_eq!(merged.event.detail["event_count"], serde_json::json!(2));
    assert!(merged.event.detail.get("merged_at_ms").is_some());

    let row = main
        .segment(&segment.segment_id)
        .expect("row")
        .expect("present");
    assert_eq!(row.state, SegmentState::Folded);
    assert_eq!(
        row.head_hash,
        main.last_hash().expect("head"),
        "the row records where on the main chain the segment landed"
    );

    // The segment's own file is untouched: same two rows, and the mark is still on them.
    assert_eq!(seg.count().expect("count"), 2);
    for event in seg.all().expect("all") {
        assert_eq!(
            event.event.detail["provisional"],
            serde_json::json!(true),
            "the segment file keeps the original rows, mark and all"
        );
    }

    // Both chains verify: the main one grew, the segment's did not change.
    assert!(matches!(
        verify_chain(&main).expect("verify"),
        ChainStatus::Intact { length: 6 }
    ));
    assert!(matches!(
        verify_chain(&seg).expect("verify"),
        ChainStatus::Intact { length: 2 }
    ));
}

#[test]
fn a_segment_that_conflicts_with_the_main_chain_is_forked_not_merged() {
    let dir = temp_dir("conflict");
    let mut main = AuditStore::open(&dir.join("audit.db")).expect("main");
    // The main chain already holds this act (without the mark).
    main.append(AuditEvent::new(
        "sandbox",
        "vm.start",
        serde_json::json!({ "n": 1 }),
    ))
    .expect("append");
    let segment = main
        .open_segment(SegmentKind::Temporary)
        .expect("open segment");

    // The segment claims the same act, marked provisional.
    let mut seg = AuditStore::open_segment_store(&dir, &segment.segment_id).expect("segment store");
    seg.append(provisional_event(999, "vm.start", 1))
        .expect("append");

    let before = main.count().expect("count");
    let outcome = main
        .merge_segment(&dir, &segment.segment_id)
        .expect("a fork is an outcome, not an error");
    let MergeOutcome::Forked { reason } = outcome else {
        panic!("expected a fork, got {outcome:?}");
    };
    assert!(reason.contains("vm.start"), "{reason}");

    // Nothing was transcribed: the only `vm.start` on the chain is the one that was already there, and the
    // only new event is the fork itself.
    assert_eq!(main.count().expect("count"), before + 1);
    let events = main.all().expect("all");
    assert_eq!(
        events
            .iter()
            .filter(|event| event.event.action == "vm.start")
            .count(),
        1,
        "the segment's event was not transcribed"
    );
    let forked = events
        .iter()
        .find(|event| event.event.action == ACTION_SEGMENT_FORKED)
        .expect("a segment_forked event");
    assert_eq!(
        forked.event.detail["segment_id"],
        serde_json::json!(segment.segment_id)
    );
    assert_eq!(forked.event.detail["kind"], serde_json::json!("temporary"));
    assert!(forked.event.detail.get("forked_at_ms").is_some());
    assert_eq!(forked.event.detail["reason"], serde_json::json!(reason));
    assert!(forked.event.detail.get("conflicting_event_id").is_some());
    assert_eq!(
        seg.count().expect("count"),
        1,
        "the segment file is untouched"
    );

    // The row is forked, and says why.
    let row = main
        .segment(&segment.segment_id)
        .expect("row")
        .expect("present");
    assert_eq!(row.state, SegmentState::Forked);
    assert_eq!(row.note.as_deref(), Some(reason.as_str()));

    // Both chains still verify.
    assert!(matches!(
        verify_chain(&main).expect("verify"),
        ChainStatus::Intact { .. }
    ));
    assert!(matches!(
        verify_chain(&seg).expect("verify"),
        ChainStatus::Intact { length: 1 }
    ));
}

#[test]
fn a_partial_merge_is_refused_rather_than_called_a_conflict() {
    let dir = temp_dir("partial");
    let mut main = AuditStore::open(&dir.join("audit.db")).expect("main");
    let segment = main
        .open_segment(SegmentKind::Temporary)
        .expect("open segment");
    let mut seg = AuditStore::open_segment_store(&dir, &segment.segment_id).expect("segment store");
    seg.append(provisional_event(1_000, "vm.start", 1))
        .expect("append");

    // What M5-2a leaves behind when an append fails part way through.
    let conn = rusqlite::Connection::open(dir.join("audit.db")).expect("conn");
    conn.execute(
        "UPDATE segments SET note = ?2 WHERE segment_id = ?1",
        rusqlite::params![
            segment.segment_id,
            format!(
                "{}1 of 2 events: database is locked",
                audit::PARTIAL_MERGE_PREFIX
            )
        ],
    )
    .expect("note");

    let count = main.count().expect("count");
    let error = main
        .merge_segment(&dir, &segment.segment_id)
        .expect_err("refused");
    assert!(error.to_string().contains("partial merge"), "{error}");
    assert_eq!(main.count().expect("count"), count, "nothing was written");
    assert_eq!(
        main.segment(&segment.segment_id)
            .expect("row")
            .expect("present")
            .state,
        SegmentState::Open,
        "a half-done merge is not re-labelled forked"
    );
}

#[test]
fn merging_twice_is_refused() {
    let dir = temp_dir("twice");
    let mut main = AuditStore::open(&dir.join("audit.db")).expect("main");
    let segment = main
        .open_segment(SegmentKind::Temporary)
        .expect("open segment");
    let mut seg = AuditStore::open_segment_store(&dir, &segment.segment_id).expect("segment store");
    seg.append(provisional_event(1_000, "vm.start", 1))
        .expect("append");

    main.merge_segment(&dir, &segment.segment_id)
        .expect("first merge");
    let count = main.count().expect("count");
    // The second attempt is refused — the row is `folded` now, and the `merged_already` guard behind that
    // check would refuse it too.
    let error = main
        .merge_segment(&dir, &segment.segment_id)
        .expect_err("refused");
    let message = error.to_string();
    assert!(
        message.contains("folded") || message.contains("already merged"),
        "{message}"
    );
    assert_eq!(main.count().expect("count"), count, "nothing was written");
}

#[test]
fn only_a_temporary_segment_is_merged() {
    let dir = temp_dir("kind");
    let mut main = AuditStore::open(&dir.join("audit.db")).expect("main");
    let segment = main.open_segment(SegmentKind::Main).expect("open segment");
    let error = main
        .merge_segment(&dir, &segment.segment_id)
        .expect_err("refused");
    assert!(
        error.to_string().contains("not a temporary segment"),
        "{error}"
    );
}
