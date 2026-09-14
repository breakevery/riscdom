//! Append-only SQLite store for audit events.

use crate::error::AuditError;
use crate::event::{AuditEvent, StoredEvent};
use crate::hash::{compute_hash, GENESIS_PREV_HASH};
use rusqlite::types::Value;
use rusqlite::{params, Connection};
use std::io::Write;
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

const SELECT_COLUMNS: &str = "id, timestamp_ms, actor, action, detail_json, prev_hash, hash";

/// Filter for [`AuditStore::list`]. All fields are optional (ANDed together).
#[derive(Debug, Clone, Default)]
pub struct EventFilter {
    /// Exact actor match, e.g. `"sandbox"`.
    pub actor: Option<String>,
    /// Action prefix match, e.g. `"vm."`.
    pub action_prefix: Option<String>,
    /// Inclusive lower bound on `timestamp_ms`.
    pub from_ms: Option<i64>,
    /// Inclusive upper bound on `timestamp_ms`.
    pub to_ms: Option<i64>,
}

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

    fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<RawRow> {
        Ok(RawRow {
            id: row.get(0)?,
            timestamp_ms: row.get(1)?,
            actor: row.get(2)?,
            action: row.get(3)?,
            detail_json: row.get(4)?,
            prev_hash: row.get(5)?,
            hash: row.get(6)?,
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

    /// Fetch a single event by id.
    pub fn get(&self, id: i64) -> Result<Option<StoredEvent>, AuditError> {
        let sql = format!("SELECT {SELECT_COLUMNS} FROM audit_events WHERE id = ?1");
        let mut stmt = self.conn.prepare(&sql)?;
        let mut rows = stmt.query([id])?;
        match rows.next()? {
            Some(row) => Ok(Some(RawRow::from_row(row)?.into_stored()?)),
            None => Ok(None),
        }
    }

    /// List events (oldest first) matching `filter`, capped at `limit`.
    pub fn list(&self, filter: EventFilter, limit: usize) -> Result<Vec<StoredEvent>, AuditError> {
        let mut sql = format!("SELECT {SELECT_COLUMNS} FROM audit_events");
        let mut clauses: Vec<&str> = Vec::new();
        let mut args: Vec<Value> = Vec::new();

        if let Some(actor) = &filter.actor {
            clauses.push("actor = ?");
            args.push(Value::Text(actor.clone()));
        }
        if let Some(prefix) = &filter.action_prefix {
            // True prefix match (no LIKE wildcard surprises).
            clauses.push("substr(action, 1, ?) = ?");
            args.push(Value::Integer(prefix.chars().count() as i64));
            args.push(Value::Text(prefix.clone()));
        }
        if let Some(from) = filter.from_ms {
            clauses.push("timestamp_ms >= ?");
            args.push(Value::Integer(from));
        }
        if let Some(to) = filter.to_ms {
            clauses.push("timestamp_ms <= ?");
            args.push(Value::Integer(to));
        }
        if !clauses.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&clauses.join(" AND "));
        }
        sql.push_str(" ORDER BY id ASC LIMIT ?");
        args.push(Value::Integer(limit as i64));

        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(args), RawRow::from_row)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?.into_stored()?);
        }
        Ok(out)
    }

    /// Write every event, in id order, as JSONL.
    ///
    /// Each line carries `id` / `timestamp_ms` / `actor` / `action` / `detail`
    /// (compatible with `sandbox`'s original file sink) plus `prev_hash` /
    /// `hash` so an external tool can verify the chain independently.
    pub fn export_jsonl(&self, path: &Path) -> Result<usize, AuditError> {
        let events = self.all()?;
        let mut file = std::fs::File::create(path)?;
        for e in &events {
            let line = serde_json::json!({
                "id": e.id,
                "timestamp_ms": e.event.timestamp_ms,
                "actor": e.event.actor,
                "action": e.event.action,
                "detail": e.event.detail,
                "prev_hash": e.prev_hash,
                "hash": e.hash,
            });
            writeln!(file, "{}", serde_json::to_string(&line)?)?;
        }
        Ok(events.len())
    }

    /// All rows in chain order (raw form, `pub(crate)` for verification).
    pub(crate) fn scan(&self) -> Result<Vec<RawRow>, AuditError> {
        let sql = format!("SELECT {SELECT_COLUMNS} FROM audit_events ORDER BY id ASC");
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map([], RawRow::from_row)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    /// All events in chain order, decoded.
    pub fn all(&self) -> Result<Vec<StoredEvent>, AuditError> {
        self.scan()?.into_iter().map(RawRow::into_stored).collect()
    }
}
