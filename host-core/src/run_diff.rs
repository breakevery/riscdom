//! Golden-path step 8 (v0.6 batch 1) — the fingerprint diff, data layer.
//!
//! Two runs are comparable because each one's `run.start` carries the canonical
//! JSON of the configuration fingerprint it was given (`fingerprint_json`, see
//! `docs/run-provenance.md` §2.3). This module turns two of those documents into
//! an **ordered** list of top-level fields, each with both values and whether
//! they differ — the data the automatic comparison is built on. It is pure: no
//! store, no chain, no I/O.
//!
//! The boundaries this batch fixes deliberately:
//!
//! - **Top level only.** A nested object is compared as one whole item (`llm`,
//!   `vm`, … arrive as a single row each); walking the keys inside them is a later
//!   batch. That is why the list is short and stable.
//! - **Declaration order, never alphabetical.** [`FINGERPRINT_FIELDS`] mirrors the
//!   order the fields are written in by `AppState::run_fingerprint`, which is the
//!   document being diffed. The order is spelled out rather than read off the map
//!   because `serde_json`'s map hands keys back either in insertion order or
//!   sorted, depending on whether the `preserve_order` feature is enabled anywhere
//!   in the dependency graph — the same reason `audit::canonical_json` sorts
//!   explicitly instead of trusting the map.
//! - **Only fields that exist.** The list is restricted to the declared fields the
//!   two documents actually carry: nothing is invented for a field this version
//!   does not have, and a field present on one side only is diffed against `null`.
//! - **"Equal" is the digest's kind of equal.** Values are compared as canonical
//!   JSON, the normalisation the fingerprint hashes, so two documents this list
//!   calls identical are two `audit::fingerprint` would agree on — as long as both
//!   carry exactly the declared fields, since a foreign extra key would be
//!   invisible here and not in the digest.

use serde::Serialize;

/// The top-level fields of a run fingerprint, in the order
/// `AppState::run_fingerprint` declares them.
///
/// This order is the one the diff reports in; sorting it would make the list read
/// `agent, llm, policy, …` and lose the shape of the document it mirrors.
pub const FINGERPRINT_FIELDS: [&str; 7] = [
    "schema",
    "llm",
    "agent",
    "vm",
    "toolchain",
    "policy",
    "prompt",
];

/// One top-level field of two fingerprints, side by side.
///
/// `a` / `b` are the values as the documents carry them (whole nested objects, not
/// their keys); a field absent from one document is `null` there. Both values are
/// always present, so an unchanged field is still readable rather than implied.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FingerprintFieldDiff {
    /// The field name, e.g. `llm`.
    pub field: String,
    /// The value in the first fingerprint (`null` when it does not carry it).
    pub a: serde_json::Value,
    /// The value in the second fingerprint (`null` when it does not carry it).
    pub b: serde_json::Value,
    /// Whether the two values differ — canonical JSON, so key order inside a value
    /// is not a difference.
    pub is_different: bool,
}

/// Compare two fingerprint documents field by field, in declaration order.
///
/// Every declared field the two documents carry comes back, whether or not it
/// differs: two identical fingerprints produce a full list with every
/// `is_different` false, never an empty one.
pub fn diff_fingerprints(
    a: &serde_json::Value,
    b: &serde_json::Value,
) -> Vec<FingerprintFieldDiff> {
    let mut out = Vec::with_capacity(FINGERPRINT_FIELDS.len());
    for name in FINGERPRINT_FIELDS {
        if a.get(name).is_none() && b.get(name).is_none() {
            continue; // a field neither document carries is not part of the diff
        }
        let left = a.get(name).cloned().unwrap_or(serde_json::Value::Null);
        let right = b.get(name).cloned().unwrap_or(serde_json::Value::Null);
        out.push(FingerprintFieldDiff {
            field: name.to_string(),
            is_different: audit::canonical_json(&left) != audit::canonical_json(&right),
            a: left,
            b: right,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// A fingerprint shaped like the host writes it: every declared field, nested
    /// objects and all.
    fn fingerprint() -> serde_json::Value {
        json!({
            "schema": { "fingerprint_schema": "riscdom.run.fingerprint.v1", "app_version": "0.6.0" },
            "llm": { "provider_id": "deepseek", "base_url": "https://api.example/v1", "model": "deepseek-chat" },
            "agent": { "max_iterations": 8, "request_timeout_secs": 120 },
            "vm": { "memory_mb": 256, "machine": "board-a", "cpu": "core-a" },
            "toolchain": { "path": "C:/toolchain/bin", "version": "13.2.0", "source": "download" },
            "policy": { "allowed_extensions": ["c", "h", "ld"], "traversal_guard": "normalize+containment" },
            "prompt": { "sha256": "00" },
        })
    }

    fn field<'a>(diff: &'a [FingerprintFieldDiff], name: &str) -> &'a FingerprintFieldDiff {
        diff.iter()
            .find(|d| d.field == name)
            .unwrap_or_else(|| panic!("{name} must be in the diff"))
    }

    #[test]
    fn every_declared_field_comes_back_in_declaration_order() {
        let a = fingerprint();
        let mut b = fingerprint();
        b["llm"]["model"] = json!("deepseek-reasoner");
        b["vm"]["memory_mb"] = json!(512);

        let diff = diff_fingerprints(&a, &b);

        assert_eq!(
            diff.iter().map(|d| d.field.as_str()).collect::<Vec<_>>(),
            FINGERPRINT_FIELDS.to_vec(),
            "all seven fields, in the order the fingerprint declares them"
        );
        for name in ["llm", "vm"] {
            let row = field(&diff, name);
            assert!(row.is_different, "{name} changed");
        }
        assert_eq!(field(&diff, "llm").a, a["llm"]);
        assert_eq!(field(&diff, "llm").b, b["llm"]);
        assert_eq!(field(&diff, "vm").a, a["vm"]);
        assert_eq!(field(&diff, "vm").b, b["vm"]);
        for name in ["schema", "agent", "toolchain", "policy", "prompt"] {
            let row = field(&diff, name);
            assert!(!row.is_different, "{name} did not change");
            assert_eq!(row.a, row.b, "{name}");
            assert_eq!(row.a, a[name], "{name} keeps the value it had");
        }
    }

    #[test]
    fn identical_fingerprints_still_come_back_complete() {
        let doc = fingerprint();
        let diff = diff_fingerprints(&doc, &doc);

        assert_eq!(diff.len(), FINGERPRINT_FIELDS.len(), "not an empty list");
        assert_eq!(
            diff.iter().map(|d| d.field.as_str()).collect::<Vec<_>>(),
            FINGERPRINT_FIELDS.to_vec()
        );
        assert!(
            diff.iter().all(|d| !d.is_different),
            "nothing changed, so every flag is false"
        );
        assert!(diff.iter().all(|d| d.a == d.b));
    }

    #[test]
    fn a_single_changed_field_is_the_only_difference() {
        let a = fingerprint();
        let mut b = fingerprint();
        b["agent"]["max_iterations"] = json!(9);

        let diff = diff_fingerprints(&a, &b);

        let different: Vec<&str> = diff
            .iter()
            .filter(|d| d.is_different)
            .map(|d| d.field.as_str())
            .collect();
        assert_eq!(different, vec!["agent"]);
        assert_eq!(
            field(&diff, "agent").a,
            json!({ "max_iterations": 8, "request_timeout_secs": 120 })
        );
        assert_eq!(
            field(&diff, "agent").b,
            json!({ "max_iterations": 9, "request_timeout_secs": 120 })
        );
    }

    #[test]
    fn there_is_no_alphabetical_order_to_fall_back_on() {
        let mut sorted = FINGERPRINT_FIELDS.to_vec();
        sorted.sort();
        assert_ne!(
            FINGERPRINT_FIELDS.to_vec(),
            sorted,
            "the guard is pointless if the declaration order happens to be sorted"
        );
        assert_eq!(FINGERPRINT_FIELDS.first().copied(), Some("schema"));
        assert_eq!(FINGERPRINT_FIELDS.last().copied(), Some("prompt"));
    }

    #[test]
    fn fields_the_documents_do_not_carry_are_not_invented() {
        let a = json!({ "llm": { "model": "m" }, "prompt": { "sha256": "00" } });
        let b = json!({ "llm": { "model": "m" } });

        let diff = diff_fingerprints(&a, &b);

        assert_eq!(
            diff.iter().map(|d| d.field.as_str()).collect::<Vec<_>>(),
            vec!["llm", "prompt"],
            "only what the two documents carry, in declaration order"
        );
        assert!(!field(&diff, "llm").is_different);
        let prompt = field(&diff, "prompt");
        assert!(
            prompt.is_different,
            "present on one side only is a difference"
        );
        assert_eq!(prompt.a, a["prompt"]);
        assert_eq!(prompt.b, serde_json::Value::Null);
    }

    #[test]
    fn key_order_inside_a_value_is_not_a_difference() {
        let a = json!({ "llm": { "provider_id": "p", "model": "m" } });
        let b = json!({ "llm": { "model": "m", "provider_id": "p" } });

        let diff = diff_fingerprints(&a, &b);

        assert_eq!(diff.len(), 1);
        assert!(
            !diff[0].is_different,
            "the digest would not change either: {} vs {}",
            audit::fingerprint(&a),
            audit::fingerprint(&b)
        );
        assert_eq!(audit::fingerprint(&a), audit::fingerprint(&b));
    }
}
