//! The segment schema, and the chain it sits beside (v1.0 M5-1a).
//!
//! [docs/audit-v2.md](../../docs/audit-v2.md) records the semantics; this checks the **schema** half of
//! M5-1a: the `segments` table and the `segment_id` column exist, the file's version does **not** move, a
//! second open migrates nothing, and — the one that matters — a chain written under the new schema still
//! verifies `Intact` and every row written before M5 reads as the **main chain** (`segment_id IS NULL`).

use audit::{verify_chain, AuditEvent, AuditStore, ChainStatus, AUDIT_SCHEMA_VERSION};
use std::path::PathBuf;

fn temp_db(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("riscdom-segment-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("mkdir");
    dir.join(format!("audit-{nanos}.db"))
}

/// A count of `sqlite_master` rows (or any scalar `i64` query).
fn count(conn: &rusqlite::Connection, sql: &str) -> i64 {
    conn.query_row(sql, [], |row| row.get(0)).expect("query")
}

#[test]
fn the_segment_table_and_column_exist_and_the_version_does_not_move() {
    let path = temp_db("schema");
    let mut store = AuditStore::open(&path).expect("open");
    store
        .append(AuditEvent::new(
            "sandbox",
            "vm.start",
            serde_json::json!({ "n": 1 }),
        ))
        .expect("append");
    store
        .append(AuditEvent::new(
            "sandbox",
            "vm.stop",
            serde_json::json!({ "n": 2 }),
        ))
        .expect("append");

    // The chain verifies exactly as before, over the same two events.
    assert!(matches!(
        verify_chain(&store).expect("verify"),
        ChainStatus::Intact { length: 2 }
    ));
    assert_eq!(store.count().expect("count"), 2);

    // A new table and a new column beside the chain are **not** a format change: the version stays 1
    // (the `agent_id` precedent).
    assert_eq!(AUDIT_SCHEMA_VERSION, 1);
    assert_eq!(
        store.schema_version().expect("version"),
        AUDIT_SCHEMA_VERSION
    );
    drop(store);

    // Read the file directly: the table and the column are there, the table is empty, and every row
    // written before M5 reads as the main chain.
    let conn = rusqlite::Connection::open(&path).expect("conn");
    assert_eq!(
        count(
            &conn,
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='segments'"
        ),
        1,
        "the segments table exists"
    );
    assert_eq!(
        count(
            &conn,
            "SELECT COUNT(*) FROM pragma_table_info('audit_events') WHERE name='segment_id'"
        ),
        1,
        "audit_events carries segment_id"
    );
    assert_eq!(
        count(&conn, "SELECT COUNT(*) FROM segments"),
        0,
        "M5-1a writes no segment: the table is created empty"
    );
    assert_eq!(
        count(
            &conn,
            "SELECT COUNT(*) FROM audit_events WHERE segment_id IS NULL"
        ),
        2,
        "every event written before M5 reads as the main chain"
    );
    // The triggers are untouched: the log is still append-only through SQL.
    assert_eq!(
        count(
            &conn,
            "SELECT COUNT(*) FROM sqlite_master WHERE type='trigger' \
             AND name IN ('audit_no_update', 'audit_no_delete')"
        ),
        2,
        "both append-only triggers are still there"
    );
}

#[test]
fn reopening_migrates_nothing_and_keeps_the_chain() {
    let path = temp_db("idempotent");
    {
        let mut store = AuditStore::open(&path).expect("first open");
        store
            .append(AuditEvent::new(
                "sandbox",
                "vm.start",
                serde_json::json!({ "n": 1 }),
            ))
            .expect("append");
    }
    {
        // A second open runs the same schema statements and the same two idempotent column checks.
        let store = AuditStore::open(&path).expect("second open");
        assert!(matches!(
            verify_chain(&store).expect("verify"),
            ChainStatus::Intact { length: 1 }
        ));
        assert_eq!(
            store.schema_version().expect("version"),
            AUDIT_SCHEMA_VERSION
        );
    }

    let conn = rusqlite::Connection::open(&path).expect("conn");
    assert_eq!(
        count(
            &conn,
            "SELECT COUNT(*) FROM pragma_table_info('audit_events') WHERE name='segment_id'"
        ),
        1,
        "one column, added once"
    );
    assert_eq!(
        count(&conn, "SELECT COUNT(*) FROM audit_events"),
        1,
        "the chain still holds exactly the one row"
    );
    assert_eq!(
        count(&conn, "SELECT COUNT(*) FROM segments"),
        0,
        "and still no segment row"
    );
}

#[test]
fn the_segment_words_round_trip() {
    use audit::{SegmentKind, SegmentState};
    assert_eq!(SegmentKind::Main.as_str(), "main");
    assert_eq!(SegmentKind::Temporary.as_str(), "temporary");
    assert_eq!(
        SegmentKind::parse("temporary"),
        Some(SegmentKind::Temporary)
    );
    assert_eq!(SegmentKind::parse("other"), None);
    assert_eq!(SegmentState::Folded.as_str(), "folded");
    assert_eq!(SegmentState::parse("forked"), Some(SegmentState::Forked));
    assert_eq!(SegmentState::parse("other"), None);
}
