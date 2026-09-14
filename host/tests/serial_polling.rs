//! Stage 6a — serial increment logic (no duplicate, no loss).

use host::state::{serial_full_text, SerialDiff};
use audit::{AuditEvent, AuditStore, StoredEvent};

fn tool_call(store: &mut AuditStore, id: &str, name: &str) -> StoredEvent {
    store
        .append(AuditEvent {
            timestamp_ms: 1,
            actor: "agent".into(),
            action: "agent.tool.call".into(),
            detail: serde_json::json!({ "id": id, "name": name }),
        })
        .unwrap()
}

fn tool_result(store: &mut AuditStore, call_id: &str, result: &str) -> StoredEvent {
    store
        .append(AuditEvent {
            timestamp_ms: 2,
            actor: "agent".into(),
            action: "agent.tool.result".into(),
            detail: serde_json::json!({ "call_id": call_id, "ok": true, "result": result }),
        })
        .unwrap()
}

#[test]
fn diff_emits_exact_increments_without_dup_or_loss() {
    let mut diff = SerialDiff::new();
    assert_eq!(diff.next_chunk(""), None);
    assert_eq!(diff.next_chunk("HEL"), Some("HEL".to_string()));
    assert_eq!(diff.next_chunk("HEL"), None, "no new data -> no chunk");
    assert_eq!(diff.next_chunk("HELLO"), Some("LO".to_string()));
    assert_eq!(diff.next_chunk("HELLO RISCV"), Some(" RISCV".to_string()));

    // Nothing lost: concatenating all chunks reproduces the full text.
    let mut diff = SerialDiff::new();
    let mut collected = String::new();
    for snapshot in ["", "H", "HE", "HEL", "HELLO", "HELLO R", "HELLO RISCV"] {
        if let Some(c) = diff.next_chunk(snapshot) {
            collected.push_str(&c);
        }
    }
    assert_eq!(collected, "HELLO RISCV");
}

#[test]
fn full_text_only_includes_read_serial_results() {
    let mut store = AuditStore::in_memory().unwrap();
    tool_call(&mut store, "t1", "write_source");
    tool_result(&mut store, "t1", "wrote 10 bytes");
    tool_call(&mut store, "t2", "read_serial");
    tool_result(&mut store, "t2", "HELLO RISCV\n");
    tool_call(&mut store, "t3", "compile");
    tool_result(&mut store, "t3", "compiled ok");

    let events = store.all().unwrap();
    let text = serial_full_text(&events);
    assert_eq!(text, "HELLO RISCV\n");
}
