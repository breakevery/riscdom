//! v0.4 batch 1b — the run configuration fingerprint.

use audit::{
    canonical_json, fingerprint, parse_run_start, run_start_detail, short_fingerprint,
    FINGERPRINT_SCHEMA_V1,
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

    // The stored text re-hashes to the stored digest: the log is self-sufficient.
    let recovered: serde_json::Value =
        serde_json::from_str(&payload.fingerprint_json).expect("json");
    assert_eq!(fingerprint(&recovered), payload.fingerprint);
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
