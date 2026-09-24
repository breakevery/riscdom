//! Session persistence: conversations survive restarts.
//!
//! Stored in a separate SQLite database (reusing `rusqlite`, the same crate and
//! version `audit` already uses) under the app data directory.
//!
//! **Never persisted here**: API keys, the system prompt text, streamed
//! intermediate state, or audit events. Only the conversation messages are
//! stored, so a restored session can be replayed into the agent as history.

use rusqlite::{params, Connection};
use serde::Serialize;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use thiserror::Error;

/// Errors produced by the session store.
#[derive(Debug, Error)]
pub enum SessionError {
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("{0}")]
    Other(String),
}

/// Session summary for the session list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SessionMeta {
    pub id: String,
    pub title: String,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    pub message_count: usize,
}

/// One persisted conversation message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SessionMessage {
    pub id: i64,
    pub session_id: String,
    /// `"user"` / `"assistant"` / `"system"` / `"tool"`.
    pub role: String,
    pub content: String,
    /// Raw `tool_calls` JSON for assistant messages.
    pub tool_call_json: Option<String>,
    /// Which tool call a `tool` message answers.
    pub tool_call_id: Option<String>,
    pub created_at_ms: i64,
}

impl SessionMessage {
    /// A plain role/content message (id is assigned by the store).
    pub fn new(
        session_id: impl Into<String>,
        role: impl Into<String>,
        content: impl Into<String>,
    ) -> Self {
        Self {
            id: 0,
            session_id: session_id.into(),
            role: role.into(),
            content: content.into(),
            tool_call_json: None,
            tool_call_id: None,
            created_at_ms: now_ms(),
        }
    }
}

/// How long a writer waits inside SQLite for another process's lock.
///
/// The same five seconds the audit store waits with, and it is set **first**,
/// before this connection's first write (the schema is that write): with
/// SQLite's default of zero, a second process holding the lock is answered
/// `SQLITE_BUSY` immediately instead of being waited out.
///
/// The sessions DB is per-instance by default (v0.8), so this is not the audit
/// store's situation — but two processes can still meet on one file (two
/// default-path CLI or server processes, or an explicitly shared `--data-dir`),
/// and a failed open takes the whole instance down with it.
///
/// **WAL is deliberately not set here**, and neither is `synchronous`: the audit
/// store is shared across processes on purpose, this one is not, so the two
/// stores' concurrency models differ by design — see `docs/decisions.md` §54.
pub const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS sessions (
    id            TEXT PRIMARY KEY,
    title         TEXT NOT NULL,
    created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS session_messages (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id     TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    role           TEXT NOT NULL,
    content        TEXT NOT NULL,
    tool_call_json TEXT,
    tool_call_id   TEXT,
    created_at_ms  INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_messages_session ON session_messages(session_id, id);
"#;

/// SQLite-backed session store.
pub struct SessionStore {
    conn: Connection,
}

impl SessionStore {
    /// Open (or create) the store at `path`; parent directories are created.
    pub fn open(path: &Path) -> Result<Self, SessionError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        Self::init(conn)
    }

    /// Private in-memory store (tests).
    pub fn in_memory() -> Result<Self, SessionError> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self, SessionError> {
        // The busy timeout goes on **first**: the schema below is this
        // connection's first write, and with SQLite's default of zero a second
        // process holding the lock turns it into an immediate `SQLITE_BUSY`.
        conn.busy_timeout(BUSY_TIMEOUT)?;
        // Required for ON DELETE CASCADE to behave.
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;
        conn.execute_batch(SCHEMA)?;
        Ok(Self { conn })
    }

    /// Create a session and return its id.
    pub fn create_session(&self, title: &str) -> Result<String, SessionError> {
        let id = new_session_id();
        let now = now_ms();
        self.conn.execute(
            "INSERT INTO sessions (id, title, created_at_ms, updated_at_ms) VALUES (?1, ?2, ?3, ?3)",
            params![id, title, now],
        )?;
        Ok(id)
    }

    /// Sessions, most recently updated first.
    pub fn list_sessions(&self, limit: usize) -> Result<Vec<SessionMeta>, SessionError> {
        let mut stmt = self.conn.prepare(
            "SELECT s.id, s.title, s.created_at_ms, s.updated_at_ms, COUNT(m.id) \
             FROM sessions s LEFT JOIN session_messages m ON m.session_id = s.id \
             GROUP BY s.id ORDER BY s.updated_at_ms DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit as i64], |row| {
            Ok(SessionMeta {
                id: row.get(0)?,
                title: row.get(1)?,
                created_at_ms: row.get(2)?,
                updated_at_ms: row.get(3)?,
                message_count: row.get::<_, i64>(4)? as usize,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// Rename a session.
    pub fn rename_session(&self, id: &str, title: &str) -> Result<(), SessionError> {
        self.conn.execute(
            "UPDATE sessions SET title = ?2, updated_at_ms = ?3 WHERE id = ?1",
            params![id, title, now_ms()],
        )?;
        Ok(())
    }

    /// Delete a session (its messages cascade).
    pub fn delete_session(&self, id: &str) -> Result<(), SessionError> {
        self.conn
            .execute("DELETE FROM sessions WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// Append a message; returns its row id. Also bumps the session timestamp.
    ///
    /// The insert and the timestamp bump are **one transaction**: a message that
    /// landed while its session's `updated_at_ms` did not would leave the
    /// session list lying about when the session was last used. The transaction
    /// is opened with [`Connection::unchecked_transaction`] because the store is
    /// shared behind a `Mutex` and this method takes `&self`; nothing here opens
    /// a second transaction, and a failure anywhere inside rolls the whole
    /// append back rather than leaving half of it.
    pub fn append_message(
        &self,
        session_id: &str,
        msg: SessionMessage,
    ) -> Result<i64, SessionError> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO session_messages \
             (session_id, role, content, tool_call_json, tool_call_id, created_at_ms) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                session_id,
                msg.role,
                msg.content,
                msg.tool_call_json,
                msg.tool_call_id,
                msg.created_at_ms
            ],
        )?;
        let id = tx.last_insert_rowid();
        touch_session_in(&tx, session_id)?;
        tx.commit()?;
        Ok(id)
    }

    /// The most recent `limit` messages, oldest first.
    pub fn load_messages(
        &self,
        session_id: &str,
        limit: usize,
    ) -> Result<Vec<SessionMessage>, SessionError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, session_id, role, content, tool_call_json, tool_call_id, created_at_ms \
             FROM session_messages WHERE session_id = ?1 ORDER BY id DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![session_id, limit as i64], |row| {
            Ok(SessionMessage {
                id: row.get(0)?,
                session_id: row.get(1)?,
                role: row.get(2)?,
                content: row.get(3)?,
                tool_call_json: row.get(4)?,
                tool_call_id: row.get(5)?,
                created_at_ms: row.get(6)?,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        out.reverse(); // newest-first query -> oldest-first result
        Ok(out)
    }

    /// Update `updated_at_ms`.
    pub fn touch_session(&self, id: &str) -> Result<(), SessionError> {
        touch_session_in(&self.conn, id)
    }

    /// Delete every session (their messages cascade).
    pub fn clear_all(&self) -> Result<(), SessionError> {
        self.conn.execute("DELETE FROM sessions", [])?;
        Ok(())
    }

    /// Number of persisted messages for a session.
    pub fn message_count(&self, session_id: &str) -> Result<usize, SessionError> {
        let n: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM session_messages WHERE session_id = ?1",
            params![session_id],
            |row| row.get(0),
        )?;
        Ok(n as usize)
    }

    /// This connection's current `busy_timeout`, in milliseconds ([`BUSY_TIMEOUT`]
    /// unless something changed it). It is a per-connection setting, so this
    /// reports the value *this* store was opened with. Diagnostics and tests.
    pub fn busy_timeout_ms(&self) -> Result<i64, SessionError> {
        Ok(self
            .conn
            .query_row("PRAGMA busy_timeout", [], |row| row.get(0))?)
    }
}

/// `UPDATE sessions SET updated_at_ms = ?2 WHERE id = ?1`.
///
/// One writer for both [`SessionStore::touch_session`] and the append
/// transaction, so the public method and the update inside an append cannot
/// drift apart.
fn touch_session_in(conn: &Connection, id: &str) -> Result<(), SessionError> {
    conn.execute(
        "UPDATE sessions SET updated_at_ms = ?2 WHERE id = ?1",
        params![id, now_ms()],
    )?;
    Ok(())
}

/// Milliseconds since the Unix epoch.
pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// A sortable, collision-resistant id: timestamp + process-local counter + nanos.
fn new_session_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let seq = SEQ.fetch_add(1, Ordering::SeqCst);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("sess-{nanos:x}-{seq:x}")
}
