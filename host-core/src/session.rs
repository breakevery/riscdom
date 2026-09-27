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

/// The session schema version this build writes, in SQLite's own
/// `PRAGMA user_version` (v1.0 M2b-2).
///
/// SQLite's header field is the analogue of the JSON formats' first-field version
/// marker: it travels **with the file**, needs no table of its own, and is read
/// before anything else. `0` means "written before this batch" (SQLite's default)
/// and is migrated on open.
pub const SESSION_SCHEMA_VERSION: i64 = 1;

/// Errors produced by the session store.
#[derive(Debug, Error)]
pub enum SessionError {
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// The file was written by a newer build (v1.0 M2b-2).
    ///
    /// `docs/api-compatibility.md` §6: an old reader never reads a newer format,
    /// never reads part of it and never downgrades it silently — so this is an
    /// error the caller shows, not a fallback.
    #[error("data_too_new: the session database is version {found}, this build reads {supported}")]
    DataTooNew { found: i64, supported: i64 },

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
    /// Which executor this session belongs to (v1.0 M2b-2).
    ///
    /// `None` means the row predates the column: it belongs to the **node itself**,
    /// which is what every session was before this batch. It is serialised as
    /// `null` rather than omitted — a field that sometimes vanishes is a field every
    /// client has to guess about.
    pub executor_id: Option<String>,
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
    ///
    /// A file written by an **older** build is migrated here (v1.0 M2b-2): its bytes
    /// are copied to `sessions.db.bak` **before** the connection touches it, the
    /// column is added, and the version is stamped. A file from a **newer** build is
    /// refused.
    pub fn open(path: &Path) -> Result<Self, SessionError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        // The backup belongs to the migration and only to the migration: an ordinary
        // open must not overwrite the snapshot that exists to undo one. A brand-new
        // file has nothing to copy.
        if path.is_file() && Self::file_version(path)? < SESSION_SCHEMA_VERSION {
            std::fs::copy(path, path.with_extension("db.bak"))?;
        }
        let conn = Connection::open(path)?;
        Self::init(conn)
    }

    /// Private in-memory store (tests).
    pub fn in_memory() -> Result<Self, SessionError> {
        Self::init(Connection::open_in_memory()?)
    }

    /// The `user_version` a session file carries; a missing file reads as `0`.
    fn file_version(path: &Path) -> Result<i64, SessionError> {
        let conn = Connection::open(path)?;
        Ok(conn.query_row("PRAGMA user_version", [], |row| row.get(0))?)
    }

    fn init(conn: Connection) -> Result<Self, SessionError> {
        // The busy timeout goes on **first**: the schema below is this
        // connection's first write, and with SQLite's default of zero a second
        // process holding the lock turns it into an immediate `SQLITE_BUSY`.
        conn.busy_timeout(BUSY_TIMEOUT)?;
        // Required for ON DELETE CASCADE to behave.
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;

        // The format's own version, before anything is read out of it (v1.0 M2b-2).
        let found: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
        if found > SESSION_SCHEMA_VERSION {
            return Err(SessionError::DataTooNew {
                found,
                supported: SESSION_SCHEMA_VERSION,
            });
        }
        conn.execute_batch(SCHEMA)?;
        if found < SESSION_SCHEMA_VERSION {
            // 0 → 1: `sessions` gains `executor_id` (v1.0 M2b-2). Rows that already
            // exist keep `NULL` — "not named by any executor", which is what every
            // session was before this batch, and what the queries treat as the
            // node's own.
            Self::add_column_if_missing(&conn, "sessions", "executor_id", "TEXT")?;
            conn.execute_batch(&format!("PRAGMA user_version = {SESSION_SCHEMA_VERSION};"))?;
        }
        Ok(Self { conn })
    }

    /// `ALTER TABLE … ADD COLUMN …`, but only when the column is absent.
    ///
    /// SQLite has no `ADD COLUMN IF NOT EXISTS`, and telling an "already there" error
    /// apart from a real one is guesswork; asking `PRAGMA table_info` is the honest
    /// way and makes the step **idempotent** — a database that already has the column
    /// (a migration that was interrupted, a hand-made file) is simply current.
    fn add_column_if_missing(
        conn: &Connection,
        table: &str,
        column: &str,
        ty: &str,
    ) -> Result<(), SessionError> {
        let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
        let names = stmt
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<Result<Vec<_>, _>>()?;
        if names.iter().any(|name| name == column) {
            return Ok(());
        }
        conn.execute_batch(&format!("ALTER TABLE {table} ADD COLUMN {column} {ty};"))?;
        Ok(())
    }

    /// Create a session and return its id.
    ///
    /// `executor_id` is written **explicitly** (v1.0 M2b-2): a session made now says
    /// whose it is, and only rows that predate the column are unnamed.
    pub fn create_session(&self, title: &str, executor_id: &str) -> Result<String, SessionError> {
        let id = new_session_id();
        let now = now_ms();
        self.conn.execute(
            "INSERT INTO sessions (id, title, created_at_ms, updated_at_ms, executor_id) \
             VALUES (?1, ?2, ?3, ?3, ?4)",
            params![id, title, now, executor_id],
        )?;
        Ok(id)
    }

    /// Sessions of one executor, most recently updated first.
    ///
    /// `include_unnamed` is `true` only for the node's **own** executor: a row with no
    /// executor predates the column and belongs to the node itself, so the node's query
    /// returns it and nobody else's does (v1.0 M2b-2).
    pub fn list_sessions(
        &self,
        limit: usize,
        executor: &str,
        include_unnamed: bool,
    ) -> Result<Vec<SessionMeta>, SessionError> {
        let mut stmt = self.conn.prepare(
            "SELECT s.id, s.title, s.created_at_ms, s.updated_at_ms, COUNT(m.id), s.executor_id \
             FROM sessions s LEFT JOIN session_messages m ON m.session_id = s.id \
             WHERE s.executor_id = ?2 OR (?3 AND s.executor_id IS NULL) \
             GROUP BY s.id ORDER BY s.updated_at_ms DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit as i64, executor, include_unnamed], row_meta)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// Every executor's sessions, newest first (v1.0 M2b-3a).
    ///
    /// The wildcard reading of [`Self::list_sessions`]: one SQL statement, one
    /// `limit`, so the number means "rows returned" rather than "rows per
    /// executor". Rows written before the `executor_id` column are included —
    /// they belong to the node itself, and the node is one of the executors this
    /// asks about.
    pub fn list_all_sessions(&self, limit: usize) -> Result<Vec<SessionMeta>, SessionError> {
        let mut stmt = self.conn.prepare(
            "SELECT s.id, s.title, s.created_at_ms, s.updated_at_ms, COUNT(m.id), s.executor_id \
             FROM sessions s LEFT JOIN session_messages m ON m.session_id = s.id \
             GROUP BY s.id ORDER BY s.updated_at_ms DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit as i64], row_meta)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// One session's summary, by id — whatever executor it belongs to.
    ///
    /// Unscoped on purpose: the caller is the one that knows whose question it is
    /// answering, and it needs the row's own `executor_id` to decide (v1.0 M2b-2).
    pub fn session(&self, id: &str) -> Result<Option<SessionMeta>, SessionError> {
        let mut stmt = self.conn.prepare(
            "SELECT s.id, s.title, s.created_at_ms, s.updated_at_ms, COUNT(m.id), s.executor_id \
             FROM sessions s LEFT JOIN session_messages m ON m.session_id = s.id \
             WHERE s.id = ?1 GROUP BY s.id",
        )?;
        let mut rows = stmt.query_map(params![id], row_meta)?;
        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
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

    /// Delete every session of one executor (their messages cascade).
    ///
    /// `include_unnamed` mirrors [`Self::list_sessions`]: clearing the node's own
    /// sessions also clears the rows that predate the column (v1.0 M2b-2).
    pub fn clear_all(&self, executor: &str, include_unnamed: bool) -> Result<(), SessionError> {
        self.conn.execute(
            "DELETE FROM sessions WHERE executor_id = ?1 OR (?2 AND executor_id IS NULL)",
            params![executor, include_unnamed],
        )?;
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

    /// The schema version this store opened with ([`SESSION_SCHEMA_VERSION`], unless
    /// something stamped the file differently). Diagnostics and tests (v1.0 M2b-2).
    pub fn schema_version(&self) -> Result<i64, SessionError> {
        Ok(self
            .conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))?)
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

/// One row of the session list or of [`SessionStore::session`].
fn row_meta(row: &rusqlite::Row<'_>) -> rusqlite::Result<SessionMeta> {
    Ok(SessionMeta {
        id: row.get(0)?,
        title: row.get(1)?,
        created_at_ms: row.get(2)?,
        updated_at_ms: row.get(3)?,
        message_count: row.get::<_, i64>(4)? as usize,
        executor_id: row.get(5)?,
    })
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
