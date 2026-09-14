//! Append-only SQLite store for audit events.

use crate::error::AuditError;
use crate::event::{AuditEvent, StoredEvent};
use crate::hash::{compute_hash, GENESIS_PREV_HASH};
use rusqlite::{params, Connection};
use std::path::Path;

/// Schema + append-only triggers.
///
/// The `BEFORE UPDATE` / `BEFORE DELETE` triggers are the hard guarantee that
/// the log cannot be rewritten or pruned through SQL.
const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS audit_events (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp_ms INTEGER NOT NULL,
    actor        TEXT    NOT NULL,
    action       TEXT    NOT NULL,
    detail_json  TEXT    NOT NULL,
    prev_hash    TEXT    NOT NULL,
    hash         TEXT    NOT NULL UNIQUE
);

CREATE TRIGGER IF NOT EXISTS audit_no_update
BEFORE UPDATE ON audit_events
BEGIN
    SELECT RAISE(ABORT, 'audit_events is append-only');
END;

CREATE TRIGGER IF NOT EXISTS audit_no_delete
BEFORE DELETE ON audit_events
BEGIN
    SELECT RAISE(ABORT, 'audit_events is append-only');
END;
"#;

/// A raw row straight from SQLite (keeps `detail_json` verbatim so the hash can
/// be recomputed byte-for-byte during verification).
#[derive(Debug, Clone)]
pub(crate) struct RawRow {
    pub id: i64,
    pub timestamp_ms: i64,
    pub actor: String,
    pub action: String,
    pub detail_json: String,
    pub prev_hash: String,
    pub hash: String,
}

impl RawRow {
    fn into_stored(self) -> Result<StoredEvent, AuditError> {
        let detail: serde_json::Value = serde_json::from_str(&self.detail_json)?;
        Ok(StoredEvent {
            id: self.id,
            event: AuditEvent {
                timestamp_ms: self.timestamp_ms,
                actor: self.actor,
                action: self.action,
                detail,
            },
            prev_hash: self.prev_hash,
            hash: self.hash,
        })
    }
}

/// Append-only audit store backed by SQLite.
pub struct AuditStore {
    conn: Connection,
}

impl AuditStore {
    /// Open (or create) a store at `path`.
    pub fn open(path: &Path) -> Result<Self, AuditError> {
        let conn = Connection::open(path)?;
        let store = Self { conn };
        store.init_schema()?;
        Ok(store)
    }

    /// Open a private in-memory store (tests).
    pub fn in_memory() -> Result<Self, AuditError> {
        let conn = Connection::open_in_memory()?;
        let store = Self { conn };
        store.init_schema()?;
        Ok(store)
    }

    fn init_schema(&self) -> Result<(), AuditError> {
        self.conn.execute_batch(SCHEMA)?;
        Ok(())
    }

    /// Append an event, chaining it onto the current head.
    pub fn append(&mut self, event: AuditEvent) -> Result<StoredEvent, AuditError> {
        let prev_hash = self
            .last_hash()?
            .unwrap_or_else(|| GENESIS_PREV_HASH.to_string());
        let detail_json = serde_json::to_string(&event.detail)?;
        let hash = compute_hash(&prev_hash, &event, &detail_json);

        self.conn.execute(
            "INSERT INTO audit_events \
             (timestamp_ms, actor, action, detail_json, prev_hash, hash) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                event.timestamp_ms,
                event.actor,
                event.action,
                detail_json,
                prev_hash,
                hash
            ],
        )?;

        let id = self.conn.last_insert_rowid();
        Ok(StoredEvent {
            id,
            event,
            prev_hash,
            hash,
        })
    }

    /// Hash of the most recent event, or `None` when the log is empty.
    pub fn last_hash(&self) -> Result<Option<String>, AuditError> {
        let mut stmt = self
            .conn
            .prepare("SELECT hash FROM audit_events ORDER BY id DESC LIMIT 1")?;
        let mut rows = stmt.query([])?;
        match rows.next()? {
            Some(row) => Ok(Some(row.get(0)?)),
            None => Ok(None),
        }
    }

    /// Number of events.
    pub fn count(&self) -> Result<usize, AuditError> {
        let n: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM audit_events", [], |r| r.get(0))?;
        Ok(n as usize)
    }

    /// All rows in chain order (raw form, `pub(crate)` for verification).
    pub(crate) fn scan(&self) -> Result<Vec<RawRow>, AuditError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, timestamp_ms, actor, action, detail_json, prev_hash, hash \
             FROM audit_events ORDER BY id ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(RawRow {
                id: row.get(0)?,
                timestamp_ms: row.get(1)?,
                actor: row.get(2)?,
                action: row.get(3)?,
                detail_json: row.get(4)?,
                prev_hash: row.get(5)?,
                hash: row.get(6)?,
            })
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    /// All events in chain order, decoded.
    pub fn all(&self) -> Result<Vec<StoredEvent>, AuditError> {
        self.scan()?
            .into_iter()
            .map(RawRow::into_stored)
            .collect()
    }
}
