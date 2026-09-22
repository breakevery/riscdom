//! Append-only SQLite store for audit events.

use crate::error::AuditError;
use crate::event::{AuditEvent, StoredEvent};
use crate::hash::{compute_hash, GENESIS_PREV_HASH};
use crate::run::{RebuildReport, RunRecord, RunStatus};
use rusqlite::types::Value;
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use std::io::Write;
use std::path::Path;
use std::time::Duration;

/// How many times [`AuditStore::append`] retries a **locked** database (v0.8).
///
/// A lock that survives this budget is returned as an error — an audit event is
/// never dropped silently.
pub const APPEND_MAX_ATTEMPTS: u32 = 5;

/// First backoff step; it doubles per retry (v0.8): 20 / 40 / 80 / 160 ms.
pub const APPEND_BACKOFF_BASE: Duration = Duration::from_millis(20);

/// How long a writer waits inside SQLite for another process's lock (v0.8).
///
/// Multi-process writing is the normal case for the multi-agent runtime, so this
/// is deliberately generous: the alternative is a failed append and a missing
/// row.
pub const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

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
    hash         TEXT    NOT NULL UNIQUE,
    agent_id     TEXT
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

-- v0.4 batch 1b: the run index. It is NOT part of the hash chain and holds no
-- fact the chain does not already hold: every column is derived from the
-- run.start / run.end events (see `crate::run`), and `rebuild_run_index`
-- reconstructs it from the chain alone. Created here so an existing database
-- picks it up on the next open; no event is ever rewritten.
CREATE TABLE IF NOT EXISTS runs (
    run_id            TEXT PRIMARY KEY,
    session_id        TEXT,
    parent_run_id     TEXT,
    resumed_from_snapshot TEXT,
    fingerprint       TEXT NOT NULL,
    fingerprint_schema TEXT NOT NULL,
    started_at_ms     INTEGER NOT NULL,
    ended_at_ms       INTEGER,
    start_seq         INTEGER NOT NULL,
    end_seq           INTEGER,
    status            TEXT NOT NULL
);
"#;

const SELECT_COLUMNS: &str =
    "id, timestamp_ms, actor, action, detail_json, prev_hash, hash, agent_id";

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
    /// Inclusive lower bound on the event id (v0.5 batch 1).
    ///
    /// The id is the chain position, which is what a run's interval is expressed
    /// in (`run.start` / `run.end` carry their own id as `start_seq` / `end_seq`).
    pub from_id: Option<i64>,
    /// Inclusive upper bound on the event id (v0.5 batch 1).
    pub to_id: Option<i64>,
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
    /// `NULL` on rows written before v0.8 (the honest state: those events had no
    /// agent identity to record).
    pub agent_id: Option<String>,
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
                agent_id: self.agent_id,
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
            agent_id: row.get(7)?,
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
        Self::configure_connection(&self.conn)?;
        self.conn.execute_batch(SCHEMA)?;
        self.migrate_events_table()?;
        self.migrate_runs_table()?;
        Ok(())
    }

    /// The connection settings that make several processes writing one
    /// `audit.db` behave (v0.8).
    ///
    /// - `journal_mode = WAL` — a property of the *file*, so it survives the
    ///   connection and an existing database is switched on first open. WAL is
    ///   what lets a reader and a writer (or two writers, one waiting) coexist
    ///   instead of failing each other;
    /// - `busy_timeout` — how long SQLite itself waits for the lock before
    ///   answering `SQLITE_BUSY` (per connection, so set on every open);
    /// - `synchronous = NORMAL` — the durability level WAL is designed around:
    ///   a commit survives a process crash, and the cheap fsync is dropped.
    ///
    /// This touches no schema, no chain row and no trigger: it is transport-level
    /// only. `:memory:` has no WAL to offer and reports `memory`, which is fine.
    fn configure_connection(conn: &Connection) -> Result<(), AuditError> {
        // The busy timeout goes on **first**. Switching a database to WAL takes a
        // brief exclusive lock, and with SQLite's default timeout of zero that
        // fails instantly — `SQLITE_BUSY` — while another process is writing. The
        // concurrency test caught exactly that (`open` racing another connection).
        conn.busy_timeout(BUSY_TIMEOUT)?;
        // WAL is a property of the *file*, so an existing database is switched
        // once and every later open reads the answer back instead of taking the
        // lock again.
        let mode: String = conn.query_row("PRAGMA journal_mode", [], |row| row.get(0))?;
        if !mode.eq_ignore_ascii_case("wal") && mode != "memory" {
            let _mode: String =
                conn.query_row("PRAGMA journal_mode = WAL", [], |row| row.get(0))?;
        }
        conn.execute_batch("PRAGMA synchronous = NORMAL;")?;
        Ok(())
    }

    /// The connection's journal mode (`wal` for a file database, `memory` for
    /// an in-memory one). Diagnostics and tests.
    pub fn journal_mode(&self) -> Result<String, AuditError> {
        Ok(self
            .conn
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))?)
    }

    /// The connection's current `busy_timeout`, in milliseconds.
    pub fn busy_timeout_ms(&self) -> Result<i64, AuditError> {
        Ok(self
            .conn
            .query_row("PRAGMA busy_timeout", [], |row| row.get(0))?)
    }

    /// Shorten (or lengthen) this connection's busy timeout.
    ///
    /// The default is [`BUSY_TIMEOUT`]; the setter exists so a caller — and the
    /// exhaustion test — can wait far less than five seconds.
    pub fn set_busy_timeout(&self, timeout: Duration) -> Result<(), AuditError> {
        self.conn.busy_timeout(timeout)?;
        Ok(())
    }

    /// Add the `agent_id` column an older database lacks (v0.8 technical debt).
    ///
    /// `CREATE TABLE IF NOT EXISTS` cannot alter a table that already exists, so a
    /// log written before the multi-agent work keeps the old shape until this
    /// runs. Existing rows are left `NULL` — the honest state, since those events
    /// carried no agent identity — and nothing is rewritten.
    ///
    /// This adds a **column beside the chain**: the hash formula, the `prev_hash`
    /// linkage and every existing row's `hash` are untouched, so a pre-v0.8 chain
    /// verifies exactly as it did before.
    fn migrate_events_table(&self) -> Result<(), AuditError> {
        if !self.column_exists("audit_events", "agent_id")? {
            self.conn
                .execute("ALTER TABLE audit_events ADD COLUMN agent_id TEXT", [])?;
        }
        Ok(())
    }

    /// Does `table` already have `column`?
    fn column_exists(&self, table: &str, column: &str) -> Result<bool, AuditError> {
        let mut stmt = self.conn.prepare(&format!("PRAGMA table_info({table})"))?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            if row.get::<_, String>(1)? == column {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Add a derived-index column an older database lacks (v0.5 batch 3).
    ///
    /// `CREATE TABLE IF NOT EXISTS` cannot alter a table that already exists, so a
    /// database written before `resumed_from_snapshot` joined the index keeps the
    /// old shape until this runs. The column is left `NULL` for rows written
    /// earlier — the honest state, since those rows were derived without it — and
    /// the next `rebuild_run_index` (or `audit-rebuild`) fills it from the chain.
    ///
    /// This touches the **derived index only**: the `audit_events` chain — its
    /// triggers, its hash formula and every existing row's `hash` — is never
    /// altered. (The `agent_id` column that [`Self::migrate_events_table`] adds
    /// sits *beside* the chain: it changes no hash and no historical record.)
    fn migrate_runs_table(&self) -> Result<(), AuditError> {
        let has_column = {
            let mut stmt = self.conn.prepare("PRAGMA table_info(runs)")?;
            let mut rows = stmt.query([])?;
            let mut found = false;
            while let Some(row) = rows.next()? {
                if row.get::<_, String>(1)? == "resumed_from_snapshot" {
                    found = true;
                }
            }
            found
        };
        if !has_column {
            self.conn
                .execute("ALTER TABLE runs ADD COLUMN resumed_from_snapshot TEXT", [])?;
        }
        Ok(())
    }

    /// Append an event, chaining it onto the current head.
    ///
    /// A **locked** database (another process is writing the same file) is
    /// retried with exponential backoff up to [`APPEND_MAX_ATTEMPTS`]; a lock
    /// that survives the retries is returned as an error, never dropped (v0.8).
    pub fn append(&mut self, event: AuditEvent) -> Result<StoredEvent, AuditError> {
        self.append_with(event, APPEND_MAX_ATTEMPTS, APPEND_BACKOFF_BASE)
    }

    /// [`Self::append`] with an explicit attempt budget and backoff base.
    ///
    /// Exposed so a caller can fail fast, and so the exhaustion path has a
    /// deterministic test instead of one that waits out the real budget.
    pub fn append_with(
        &mut self,
        event: AuditEvent,
        attempts: u32,
        backoff_base: Duration,
    ) -> Result<StoredEvent, AuditError> {
        let budget = attempts.max(1);
        let mut attempt: u32 = 1;
        loop {
            match self.append_once(&event) {
                Ok(stored) => return Ok(stored),
                Err(error) => {
                    if attempt >= budget || !is_lock_error(&error) {
                        return Err(error);
                    }
                    // 20 / 40 / 80 / 160 ms; the busy timeout covers the bulk of
                    // the wait, this only spaces the attempts out.
                    std::thread::sleep(backoff_base * 2u32.pow(attempt - 1));
                    attempt += 1;
                }
            }
        }
    }

    /// One append attempt: take the write lock, read the head, hash, insert.
    ///
    /// The whole sequence runs inside `BEGIN IMMEDIATE`, so the write lock is
    /// held **before** the head is read. That matters for more than tidiness: with
    /// a plain read-then-insert, two processes can both read the same head and
    /// both chain onto it, which forks the chain — WAL and a busy timeout make the
    /// writers wait for each other, but only an immediate transaction makes the
    /// second one read the head the first one wrote. (The concurrency test caught
    /// exactly that fork before this was here.)
    fn append_once(&mut self, event: &AuditEvent) -> Result<StoredEvent, AuditError> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let prev_hash: String = tx
            .query_row(
                "SELECT hash FROM audit_events ORDER BY id DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .optional()?
            .unwrap_or_else(|| GENESIS_PREV_HASH.to_string());
        let detail_json = serde_json::to_string(&event.detail)?;
        let hash = compute_hash(&prev_hash, event, &detail_json);

        tx.execute(
            "INSERT INTO audit_events \
             (timestamp_ms, actor, action, detail_json, prev_hash, hash, agent_id) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                event.timestamp_ms,
                event.actor,
                event.action,
                detail_json,
                prev_hash,
                hash,
                event.agent_id
            ],
        )?;

        let id = tx.last_insert_rowid();
        tx.commit()?;
        Ok(StoredEvent {
            id,
            event: event.clone(),
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
        if let Some(from) = filter.from_id {
            clauses.push("id >= ?");
            args.push(Value::Integer(from));
        }
        if let Some(to) = filter.to_id {
            clauses.push("id <= ?");
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
        write_events_jsonl(&events, path)
    }

    /// Write the chain **up to and including** `to_id`, from its first event, as JSONL.
    ///
    /// This is the export behind a run-scoped record (v0.5 batch 4): a file that
    /// claims to be a run's record has to stand on its own, so it starts at the
    /// chain's first event. Its first line's `prev_hash` is therefore
    /// [`GENESIS_PREV_HASH`], which is where `verify_chain` starts — the file can be
    /// verified in an empty database, with no prefix carried over from the one it was
    /// cut from.
    ///
    /// The **slice** form (`from_id`..`to_id`) is deliberately not offered. Two
    /// meanings for "export" is one meaning too many, and the one worth keeping is
    /// the file that can answer "is this record intact" by itself; a mid-chain slice
    /// cannot, because its first line links to an event it does not contain.
    ///
    /// Same line shape as [`Self::export_jsonl`] — an export is a prefix of the
    /// chain, not a different format. A `to_id` below the first event is not an
    /// error: it writes an empty file and returns `0`.
    pub fn export_self_contained_jsonl(
        &self,
        to_id: i64,
        path: &Path,
    ) -> Result<usize, AuditError> {
        let events = self.list(
            EventFilter {
                to_id: Some(to_id),
                ..EventFilter::default()
            },
            usize::MAX,
        )?;
        write_events_jsonl(&events, path)
    }

    /// The interval `record` occupies, resolved against **this store's chain**
    /// (v0.5 batch 2).
    ///
    /// Shorthand for [`crate::run::run_interval`]: a run the chain closed reads its
    /// interval off the record, and an abandoned one needs the chain to locate the
    /// `host.run.abandoned` event that ends it.
    pub fn run_interval(&self, record: &RunRecord) -> Result<(i64, i64), AuditError> {
        Ok((record.start_seq, self.run_end(record)?))
    }

    /// The id of the event that closes `record`, resolved against this store's chain
    /// (v0.5 batch 4). This is the boundary a run-scoped export cuts at.
    pub fn run_end(&self, record: &RunRecord) -> Result<i64, AuditError> {
        let chain = self.all()?;
        crate::run::run_end(record, &chain)
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

    // ----- Run index (v0.4 batch 1b) ----------------------------------------

    const RUN_COLUMNS: &str = "run_id, session_id, parent_run_id, fingerprint, \
         fingerprint_schema, started_at_ms, ended_at_ms, start_seq, end_seq, status, \
         resumed_from_snapshot";

    fn run_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<RunRecord> {
        Ok(RunRecord {
            run_id: row.get(0)?,
            session_id: row.get(1)?,
            parent_run_id: row.get(2)?,
            fingerprint: row.get(3)?,
            fingerprint_schema: row.get(4)?,
            started_at_ms: row.get(5)?,
            ended_at_ms: row.get(6)?,
            start_seq: row.get(7)?,
            end_seq: row.get(8)?,
            status: RunStatus::parse(&row.get::<_, String>(9)?),
            resumed_from_snapshot: row.get(10)?,
        })
    }

    /// Write (or overwrite) one derived index row.
    ///
    /// The index is derived, so `INSERT OR REPLACE` is safe: a rebuild produces
    /// the same row again from the chain.
    pub fn index_run_start(&mut self, record: &RunRecord) -> Result<(), AuditError> {
        self.conn.execute(
            "INSERT OR REPLACE INTO runs (run_id, session_id, parent_run_id, fingerprint, \
             fingerprint_schema, started_at_ms, ended_at_ms, start_seq, end_seq, status, \
             resumed_from_snapshot) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                record.run_id,
                record.session_id,
                record.parent_run_id,
                record.fingerprint,
                record.fingerprint_schema,
                record.started_at_ms,
                record.ended_at_ms,
                record.start_seq,
                record.end_seq,
                record.status.as_str(),
                record.resumed_from_snapshot,
            ],
        )?;
        Ok(())
    }

    /// Close one index row. Returns `false` when the run is not in the index
    /// (the caller may then [`Self::rebuild_run_index`]), never an error: a
    /// missing derived row must not be able to fail a run.
    pub fn index_run_end(
        &mut self,
        run_id: &str,
        end_seq: i64,
        ended_at_ms: i64,
        status: RunStatus,
    ) -> Result<bool, AuditError> {
        let changed = self.conn.execute(
            "UPDATE runs SET end_seq = ?2, ended_at_ms = ?3, status = ?4 WHERE run_id = ?1",
            params![run_id, end_seq, ended_at_ms, status.as_str()],
        )?;
        Ok(changed > 0)
    }

    /// Runs from the index, oldest first, capped at `limit`.
    pub fn list_runs(&self, limit: usize) -> Result<Vec<RunRecord>, AuditError> {
        let sql = format!(
            "SELECT {} FROM runs ORDER BY start_seq ASC LIMIT ?1",
            Self::RUN_COLUMNS
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map([limit as i64], Self::run_from_row)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// Every index row, oldest first.
    pub fn all_runs(&self) -> Result<Vec<RunRecord>, AuditError> {
        let sql = format!(
            "SELECT {} FROM runs ORDER BY start_seq ASC",
            Self::RUN_COLUMNS
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map([], Self::run_from_row)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// One run by id.
    pub fn get_run(&self, run_id: &str) -> Result<Option<RunRecord>, AuditError> {
        let sql = format!("SELECT {} FROM runs WHERE run_id = ?1", Self::RUN_COLUMNS);
        let mut stmt = self.conn.prepare(&sql)?;
        let mut rows = stmt.query([run_id])?;
        match rows.next()? {
            Some(row) => Ok(Some(Self::run_from_row(row)?)),
            None => Ok(None),
        }
    }

    /// The runs the **chain** describes, regardless of what the index says.
    pub fn derive_runs(&self) -> Result<(Vec<RunRecord>, RebuildReport), AuditError> {
        let events = self.all()?;
        Ok(crate::run::derive_runs_from(&events))
    }

    /// Rebuild the index from the chain alone. Writes only to `runs`.
    pub fn rebuild_run_index(&mut self) -> Result<RebuildReport, AuditError> {
        let (rows, report) = self.derive_runs()?;
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM runs", [])?;
        for row in &rows {
            tx.execute(
                "INSERT INTO runs (run_id, session_id, parent_run_id, fingerprint, \
                 fingerprint_schema, started_at_ms, ended_at_ms, start_seq, end_seq, status, \
                 resumed_from_snapshot) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![
                    row.run_id,
                    row.session_id,
                    row.parent_run_id,
                    row.fingerprint,
                    row.fingerprint_schema,
                    row.started_at_ms,
                    row.ended_at_ms,
                    row.start_seq,
                    row.end_seq,
                    row.status.as_str(),
                    row.resumed_from_snapshot,
                ],
            )?;
        }
        tx.commit()?;
        Ok(report)
    }

    /// Cross-check the index against the chain. An empty vector means they agree.
    pub fn check_run_index(&self) -> Result<Vec<String>, AuditError> {
        let (derived, _) = self.derive_runs()?;
        let stored = self.all_runs()?;
        let mut findings = Vec::new();

        for row in &derived {
            match stored.iter().find(|s| s.run_id == row.run_id) {
                None => findings.push(format!(
                    "missing in index: {} (chain has it at seq {})",
                    row.run_id, row.start_seq
                )),
                Some(s) if s != row => findings.push(format!(
                    "index row differs from the chain: {} (index {s:?}, chain {row:?})",
                    row.run_id
                )),
                Some(_) => {}
            }
        }
        for row in &stored {
            if !derived.iter().any(|d| d.run_id == row.run_id) {
                findings.push(format!(
                    "not in the chain: {} (index row at seq {})",
                    row.run_id, row.start_seq
                ));
            }
        }
        Ok(findings)
    }
}

/// Is this SQLite answering "another writer holds the lock"?
///
/// Only these two answers are worth retrying: any other failure (a broken
/// schema, a disk error) will fail again the same way.
fn is_lock_error(error: &AuditError) -> bool {
    matches!(
        error,
        AuditError::Sqlite(rusqlite::Error::SqliteFailure(err, _))
            if err.code == rusqlite::ErrorCode::DatabaseBusy
                || err.code == rusqlite::ErrorCode::DatabaseLocked
    )
}

/// Write events, in the order given, as JSONL.
///
/// One writer for both the whole-log export and the range export, so the two can
/// never drift apart: a range export is a slice of the chain, and its lines must
/// stay byte-for-byte the same shape as the whole-log ones.
fn write_events_jsonl(events: &[StoredEvent], path: &Path) -> Result<usize, AuditError> {
    let mut file = std::fs::File::create(path)?;
    for e in events {
        let line = serde_json::json!({
            "id": e.id,
            "timestamp_ms": e.event.timestamp_ms,
            "actor": e.event.actor,
            "action": e.event.action,
            "detail": e.event.detail,
            "prev_hash": e.prev_hash,
            "hash": e.hash,
            "agent_id": e.event.agent_id,
        });
        writeln!(file, "{}", serde_json::to_string(&line)?)?;
    }
    Ok(events.len())
}
