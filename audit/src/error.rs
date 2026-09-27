//! Audit error type.

use thiserror::Error;

/// Errors produced by the audit layer.
#[derive(Debug, Error)]
pub enum AuditError {
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// The file was written by a **newer** build (v1.0 M2b-3a).
    ///
    /// Read from `PRAGMA user_version` before anything else is touched, and
    /// answered by refusing the open: a newer file's rows may mean something this
    /// build does not know, and half-reading it would be the quiet corruption the
    /// refusal exists to prevent.
    #[error(
        "audit database schema version {found} is newer than this build supports ({supported})"
    )]
    DataTooNew { found: i64, supported: i64 },

    #[error("{0}")]
    Other(String),
}
