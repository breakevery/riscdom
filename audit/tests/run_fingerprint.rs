//! v0.4 batch 1b — the run configuration fingerprint.

use audit::{
    canonical_json, fingerprint, parse_run_start, run_start_detail, run_start_detail_with,
    short_fingerprint, FINGERPRINT_SCHEMA_V1,
};

/// The exact canonical text and digest of [`pinned_config`], frozen on purpose:
/// changing them is a deliberate `fingerprint_schema` bump, not an accident.
const PINNED_CANONICAL: &str = r#"{"llm":{"base_url":"https://api.deepseek.com","model":"deepseek-chat","provider_id":"deepseek"},"schema":{"app_version":"0.3.1"},"vm":{"cpu":"rv64","machine":"virt","memory_mb":128}}"#;
const PINNED_FINGERPRINT: &str = "3173d3e8532f632d11005fc49d8987fd8d329c0adb470e6cbb3a4e923a3d1b41";

fn pinned_config() -> serde_json::Value {
    serde_json::json!({
        "schema": { "app_version": "0.3.1" },
        "llm": {
            "provider_id": "deepseek",
            "base_url": "https://api.deepseek.com",
            "model": "deepseek-chat"
        },
        "vm": { "machine": "virt", "cpu": "rv64", "memory_mb": 128 }
    })
}

#[test]
fn pinned_bytes() {
    let canonical = canonical_json(&pinned_config());
    let digest = fingerprint(&pinned_config());
    println!("canonical   = {canonical}");
    println!("fingerprint = {digest}");
    assert_eq!(canonical, PINNED_CANONICAL, "canonical JSON changed");
    assert_eq!(digest, PINNED_FINGERPRINT, "fingerprint changed");
}

#[test]
fn key_order_and_spacing_do_not_change_the_fingerprint() {
    let a = serde_json::json!({ "b": 1, "a": { "y": 2, "x": 3 } });
    let b = serde_json::json!({ "a": { "x": 3, "y": 2 }, "b": 1 });
    assert_eq!(canonical_json(&a), canonical_json(&b));
    assert_eq!(fingerprint(&a), fingerprint(&b));
    assert_eq!(
        canonical_json(&a),
        r#"{"a":{"x":3,"y":2},"b":1}"#,
        "canonical JSON must be sorted and compact"
    );
}

#[test]
fn different_configs_get_different_fingerprints() {
    let base = pinned_config();

    let mut memory = base.clone();
    memory["vm"]["memory_mb"] = serde_json::json!(256);
    assert_ne!(fingerprint(&base), fingerprint(&memory));

    let mut model = base.clone();
    model["llm"]["model"] = serde_json::json!("gpt-4o-mini");
    assert_ne!(fingerprint(&base), fingerprint(&model));

    // A field that appears must change the digest, even when it is "empty".
    let mut unknown = base.clone();
    unknown["toolchain"] = serde_json::json!({ "gcc_version": "unknown" });
    assert_ne!(fingerprint(&base), fingerprint(&unknown));
}

#[test]
fn array_order_is_significant() {
    let a = serde_json::json!({ "tools": ["compile", "read_serial"] });
    let b = serde_json::json!({ "tools": ["read_serial", "compile"] });
    assert_ne!(fingerprint(&a), fingerprint(&b));
}

#[test]
fn the_start_detail_carries_the_text_that_was_hashed() {
    let config = pinned_config();
    let detail = run_start_detail("run_test", Some("session-1"), None, Some("snap-a"), &config);
    assert_eq!(detail["fingerprint_schema"], FINGERPRINT_SCHEMA_V1);

    let payload = parse_run_start(&detail).expect("parse run.start");
    assert_eq!(payload.run_id, "run_test");
    assert_eq!(payload.fingerprint, fingerprint(&config));
    assert_eq!(payload.fingerprint_json, canonical_json(&config));
    assert_eq!(payload.session_id.as_deref(), Some("session-1"));
    assert_eq!(payload.parent_run_id, None);
    assert_eq!(payload.resumed_from_snapshot.as_deref(), Some("snap-a"));
    // The baseline builder writes the run's two declarations as `null` (v1.0 M2a-3).
    assert_eq!(detail["sandbox"], serde_json::Value::Null);
    assert_eq!(detail["instance"], serde_json::Value::Null);
    assert_eq!(payload.sandbox, None);
    assert_eq!(payload.instance, None);

    // The stored text re-hashes to the stored digest: the log is self-sufficient.
    let recovered: serde_json::Value =
        serde_json::from_str(&payload.fingerprint_json).expect("json");
    assert_eq!(fingerprint(&recovered), payload.fingerprint);
}

#[test]
fn the_start_detail_carries_the_runs_declarations() {
    // Which sandbox a run resolved to and which instance it ran on (v1.0 M2a-3).
    // They are *detail*: covered by this event's own hash like every other detail
    // key, and the hash formula is what it always was.
    let config = pinned_config();
    let detail = run_start_detail_with(
        "run_declared",
        Some("session-1"),
        None,
        None,
        Some("blink"),
        Some("local-1-1-7"),
        &config,
    );
    assert_eq!(detail["sandbox"], "blink");
    assert_eq!(detail["instance"], "local-1-1-7");
    let payload = parse_run_start(&detail).expect("parse run.start");
    assert_eq!(payload.sandbox.as_deref(), Some("blink"));
    assert_eq!(payload.instance.as_deref(), Some("local-1-1-7"));
    // The fingerprint half is unchanged by the two new keys.
    assert_eq!(payload.fingerprint, fingerprint(&config));
}

#[test]
fn a_run_start_written_before_the_declarations_still_parses() {
    // v1.0 M2a-3 added `sandbox` / `instance`. A row written before that is still a
    // row: the decoder reads each key with `get`, so an older detail parses with
    // both `None` — the log has to outlive the schema that wrote it.
    let config = pinned_config();
    let old = serde_json::json!({
        "run_id": "run_old",
        "fingerprint": fingerprint(&config),
        "fingerprint_schema": FINGERPRINT_SCHEMA_V1,
        "fingerprint_json": canonical_json(&config),
        "session_id": null,
        "parent_run_id": null,
        "resumed_from_snapshot": null,
    });
    let payload = parse_run_start(&old).expect("an older run.start parses");
    assert_eq!(payload.run_id, "run_old");
    assert_eq!(payload.sandbox, None);
    assert_eq!(payload.instance, None);
}

#[test]
fn short_form_is_the_first_16_hex_characters() {
    let digest = fingerprint(&pinned_config());
    assert_eq!(short_fingerprint(&digest).len(), 16);
    assert!(digest.starts_with(short_fingerprint(&digest)));
    assert_eq!(
        short_fingerprint("abc"),
        "abc",
        "short input is not truncated"
    );
}
