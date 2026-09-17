//! Run provenance (v0.4 batch 1b).
//!
//! One **run** — a single `run_agent` invocation — is a first-class citizen of the
//! audit log: it gets a unique id, a configuration fingerprint and an audit
//! interval, written into the hash chain as two ordinary events:
//!
//! - `run.start` — carries the run id, the fingerprint, and the **canonical JSON
//!   text that was hashed** (`fingerprint_json`), so the log is self-sufficient;
//! - `run.end` — carries the run id, the terminal status and a reason.
//!
//! This module holds the pure part: the constants, the canonicalisation and
//! fingerprint, the payload types, and the derivation of the index rows from a
//! chain. The SQL side lives in [`crate::store`].
//!
//! The `runs` table is a **pure derived index**: every column is rebuilt from
//! those two events, nothing exists only in the table, and
//! [`crate::AuditStore::rebuild_run_index`] reconstructs it from the chain alone.
//! `audit_events`, its triggers and the hash formula are untouched by all of this
//! (see `docs/run-provenance.md`).

use crate::error::AuditError;
use crate::event::StoredEvent;
use sha2::{Digest, Sha256};

/// Action of the event that opens a run.
pub const ACTION_RUN_START: &str = "run.start";

/// Action of the event that closes a run.
pub const ACTION_RUN_END: &str = "run.end";

/// Action the host uses when it finds, at startup, a run whose process is gone.
///
/// It is an ordinary chained event: the chain is never given a fabricated
/// `run.end` for a run that never ended.
pub const ACTION_RUN_ABANDONED: &str = "host.run.abandoned";

/// Schema tag of the configuration fingerprint (v1).
pub const FINGERPRINT_SCHEMA_V1: &str = "riscdom.run.fingerprint.v1";

/// How many hex characters the short display form of a fingerprint keeps.
pub const SHORT_FINGERPRINT_LEN: usize = 16;

/// Canonical JSON: object keys sorted, no insignificant whitespace.
///
/// Sorting is done explicitly rather than relying on `serde_json`'s map
/// implementation, so the bytes do not depend on whether the `preserve_order`
/// feature happens to be enabled somewhere in the dependency graph.
pub fn canonical_json(value: &serde_json::Value) -> String {
    serde_json::to_string(&canonicalize(value)).unwrap_or_else(|_| "null".to_string())
}

fn canonicalize(value: &serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            let mut out = serde_json::Map::new();
            for key in keys {
                out.insert(key.clone(), canonicalize(&map[key]));
            }
            serde_json::Value::Object(out)
        }
        serde_json::Value::Array(items) => {
            serde_json::Value::Array(items.iter().map(canonicalize).collect())
        }
        other => other.clone(),
    }
}

/// SHA-256 (lowercase hex) of the canonical JSON of `config`.
pub fn fingerprint(config: &serde_json::Value) -> String {
    let mut hasher = Sha256::new();
    hasher.update(canonical_json(config).as_bytes());
    hex::encode(hasher.finalize())
}

/// Short display form of a fingerprint (first 16 hex characters).
pub fn short_fingerprint(fingerprint: &str) -> &str {
    let end = fingerprint
        .char_indices()
        .nth(SHORT_FINGERPRINT_LEN)
        .map(|(i, _)| i)
        .unwrap_or(fingerprint.len());
    &fingerprint[..end]
}

/// Terminal state of a run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RunStatus {
    /// Started and not finished yet (the process may still be running).
    Open,
    /// Finished normally.
    Ok,
    /// Finished with an error.
    Failed,
    /// Stopped by the operator.
    Interrupted,
    /// The process disappeared; the host noticed at startup.
    Abandoned,
}

impl RunStatus {
    /// Wire form, as stored in the index and in the `run.end` detail.
    pub fn as_str(self) -> &'static str {
        match self {
            RunStatus::Open => "open",
            RunStatus::Ok => "ok",
            RunStatus::Failed => "failed",
            RunStatus::Interrupted => "interrupted",
            RunStatus::Abandoned => "abandoned",
        }
    }

    /// Parse the wire form; unknown values fall back to `Failed` (an unknown
    /// terminal state must never be read as success).
    pub fn parse(raw: &str) -> Self {
        match raw {
            "open" => RunStatus::Open,
            "ok" => RunStatus::Ok,
            "interrupted" => RunStatus::Interrupted,
            "abandoned" => RunStatus::Abandoned,
            _ => RunStatus::Failed,
        }
    }
}

/// One row of the derived `runs` index.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RunRecord {
    pub run_id: String,
    pub session_id: Option<String>,
    pub parent_run_id: Option<String>,
    pub fingerprint: String,
    pub fingerprint_schema: String,
    pub started_at_ms: i64,
    pub ended_at_ms: Option<i64>,
    pub start_seq: i64,
    pub end_seq: Option<i64>,
    pub status: RunStatus,
}

/// A decoded `run.start` payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunStartPayload {
    pub run_id: String,
    pub fingerprint: String,
    pub fingerprint_schema: String,
    /// The canonical JSON text that was hashed.
    pub fingerprint_json: String,
    pub session_id: Option<String>,
    pub parent_run_id: Option<String>,
    pub resumed_from_snapshot: Option<String>,
}

/// A decoded `run.end` payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunEndPayload {
    pub run_id: String,
    pub status: RunStatus,
    pub reason: String,
}

/// What a rebuild (or a derivation) saw.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RebuildReport {
    /// Rows produced.
    pub runs: usize,
    /// `run.start` events seen.
    pub starts: usize,
    /// `run.end` events matched to a run.
    pub ends: usize,
    /// Runs marked abandoned by a `host.run.abandoned` event.
    pub abandoned: usize,
    /// `run.end` / `host.run.abandoned` events with no open run to attach to,
    /// plus repeated `run.start` for a run id already seen.
    pub orphans: usize,
}

/// Build the `detail` of a `run.start` event.
///
/// `config` is the fingerprint document (§1.2 of the design): its canonical JSON
/// travels with the event so the log alone can explain and re-verify the digest.
pub fn run_start_detail(
    run_id: &str,
    session_id: Option<&str>,
    parent_run_id: Option<&str>,
    resumed_from_snapshot: Option<&str>,
    config: &serde_json::Value,
) -> serde_json::Value {
    let canonical = canonical_json(config);
    serde_json::json!({
        "run_id": run_id,
        "fingerprint": fingerprint(config),
        "fingerprint_schema": FINGERPRINT_SCHEMA_V1,
        "fingerprint_json": canonical,
        "session_id": session_id,
        "parent_run_id": parent_run_id,
        "resumed_from_snapshot": resumed_from_snapshot,
    })
}

/// Build the `detail` of a `run.end` event.
pub fn run_end_detail(run_id: &str, status: RunStatus, reason: &str) -> serde_json::Value {
    serde_json::json!({
        "run_id": run_id,
        "status": status.as_str(),
        "reason": reason,
    })
}

/// Decode a `run.start` detail.
pub fn parse_run_start(detail: &serde_json::Value) -> Result<RunStartPayload, AuditError> {
    let text = |key: &str| -> Result<String, AuditError> {
        detail
            .get(key)
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .ok_or_else(|| AuditError::Other(format!("run.start is missing `{key}`")))
    };
    let opt = |key: &str| detail.get(key).and_then(|v| v.as_str()).map(str::to_string);
    Ok(RunStartPayload {
        run_id: text("run_id")?,
        fingerprint: text("fingerprint")?,
        fingerprint_schema: text("fingerprint_schema")?,
        fingerprint_json: text("fingerprint_json")?,
        session_id: opt("session_id"),
        parent_run_id: opt("parent_run_id"),
        resumed_from_snapshot: opt("resumed_from_snapshot"),
    })
}

/// Decode a `run.end` detail.
pub fn parse_run_end(detail: &serde_json::Value) -> Result<RunEndPayload, AuditError> {
    let run_id = detail
        .get("run_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AuditError::Other("run.end is missing `run_id`".into()))?
        .to_string();
    let status = detail
        .get("status")
        .and_then(|v| v.as_str())
        .map(RunStatus::parse)
        .unwrap_or(RunStatus::Failed);
    let reason = detail
        .get("reason")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    Ok(RunEndPayload {
        run_id,
        status,
        reason,
    })
}

/// Derive the index rows from a chain (oldest event first).
///
/// Events that are not run markers are ignored: they belong to whichever run's
/// interval contains them, which is a range lookup over the rows this returns.
pub fn derive_runs_from(events: &[StoredEvent]) -> (Vec<RunRecord>, RebuildReport) {
    let mut rows: Vec<RunRecord> = Vec::new();
    let mut report = RebuildReport::default();

    for stored in events {
        let action = stored.event.action.as_str();
        if action == ACTION_RUN_START {
            let Ok(payload) = parse_run_start(&stored.event.detail) else {
                report.orphans += 1;
                continue;
            };
            report.starts += 1;
            if rows.iter().any(|r| r.run_id == payload.run_id) {
                report.orphans += 1;
                continue;
            }
            rows.push(RunRecord {
                run_id: payload.run_id,
                session_id: payload.session_id,
                parent_run_id: payload.parent_run_id,
                fingerprint: payload.fingerprint,
                fingerprint_schema: payload.fingerprint_schema,
                started_at_ms: stored.event.timestamp_ms,
                ended_at_ms: None,
                start_seq: stored.id,
                end_seq: None,
                status: RunStatus::Open,
            });
        } else if action == ACTION_RUN_END {
            let Ok(payload) = parse_run_end(&stored.event.detail) else {
                report.orphans += 1;
                continue;
            };
            match rows
                .iter_mut()
                .find(|r| r.run_id == payload.run_id && r.end_seq.is_none())
            {
                Some(row) => {
                    row.end_seq = Some(stored.id);
                    row.ended_at_ms = Some(stored.event.timestamp_ms);
                    row.status = payload.status;
                    report.ends += 1;
                }
                None => report.orphans += 1,
            }
        } else if action == ACTION_RUN_ABANDONED {
            let run_id = stored
                .event
                .detail
                .get("run_id")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            match rows
                .iter_mut()
                .find(|r| r.run_id == run_id && r.end_seq.is_none())
            {
                Some(row) => {
                    row.status = RunStatus::Abandoned;
                    report.abandoned += 1;
                }
                None => report.orphans += 1,
            }
        }
    }

    report.runs = rows.len();
    (rows, report)
}
