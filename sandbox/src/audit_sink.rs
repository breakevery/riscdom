//! Audit sink abstraction.
//!
//! NOTE: This is a **placeholder interface**, not the final `audit` crate.
//! It exists so that every outbound sandbox operation can emit an audit event
//! from day one. The real append-only, hash-chained implementation lives in
//! the `audit` crate and will replace [`FileAuditSink`].
//!
//! Per the project constitution: the audit log lives OUTSIDE the AI, is
//! append-only, and cannot be disabled.

use serde::ser::{SerializeStruct, Serializer};
use serde::Serialize;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

/// A single audit event.
#[derive(Debug, Clone)]
pub struct AuditEvent {
    pub timestamp: SystemTime,
    /// Who caused the event. Sandbox events use `"sandbox"`.
    pub actor: String,
    /// What happened, e.g. `"vm.start"`, `"vm.stop"`, `"serial.write"`.
    pub action: String,
    /// Free-form structured detail.
    pub detail: serde_json::Value,
}

impl AuditEvent {
    /// Convenience constructor using `SystemTime::now()`.
    pub fn now(
        actor: impl Into<String>,
        action: impl Into<String>,
        detail: serde_json::Value,
    ) -> Self {
        Self {
            timestamp: SystemTime::now(),
            actor: actor.into(),
            action: action.into(),
            detail,
        }
    }
}

impl Serialize for AuditEvent {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let ms = self
            .timestamp
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        let mut s = serializer.serialize_struct("AuditEvent", 4)?;
        s.serialize_field("timestamp_ms", &ms)?;
        s.serialize_field("actor", &self.actor)?;
        s.serialize_field("action", &self.action)?;
        s.serialize_field("detail", &self.detail)?;
        s.end()
    }
}

/// Anything the sandbox can write audit events to.
pub trait AuditSink: Send + Sync {
    fn record(&self, event: AuditEvent);
}

/// MVP file-backed sink: appends one JSON object per line (JSONL).
///
/// Insertion order is preserved by an internal mutex. This is intentionally
/// minimal; the `audit` crate will provide hashing, chaining and verification.
pub struct FileAuditSink {
    path: PathBuf,
    lock: Mutex<()>,
}

impl FileAuditSink {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            lock: Mutex::new(()),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl AuditSink for FileAuditSink {
    fn record(&self, event: AuditEvent) {
        // Never let auditing take down the caller, but keep ordering stable.
        let _guard = self.lock.lock().unwrap();
        if let Some(parent) = self.path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(line) = serde_json::to_string(&event) {
            if let Ok(mut f) = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&self.path)
            {
                let _ = writeln!(f, "{line}");
            }
        }
    }
}
