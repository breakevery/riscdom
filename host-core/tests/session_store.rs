//! Stage 17a — the session store (schema v1 since v1.0 M2b-2).

use host_core::session::{SessionError, SessionMessage, SessionStore, SESSION_SCHEMA_VERSION};

/// The executor these tests act as. The store itself does not know the node's
/// name — the host passes it in — so a store-level test picks one (v1.0 M2b-2).
const EXECUTOR: &str = "local";

fn unique_dir(tag: &str) -> std::path::PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir =
        std::env::temp_dir().join(format!("riscdom-sess-{tag}-{}-{nanos}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn user(session: &str, text: &str) -> SessionMessage {
    SessionMessage::new(session, "user", text)
}

#[test]
fn create_append_list_load() {
    let store = SessionStore::in_memory().expect("store");
    let id = store
        .create_session("hello world", EXECUTOR)
        .expect("create");

    store.append_message(&id, user(&id, "hi")).expect("append");
    store
        .append_message(&id, SessionMessage::new(&id, "assistant", "hello!"))
        .expect("append");

    let sessions = store.list_sessions(10, EXECUTOR, true).expect("list");
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].id, id);
    assert_eq!(sessions[0].title, "hello world");
    assert_eq!(sessions[0].message_count, 2);
    assert_eq!(
        sessions[0].executor_id.as_deref(),
        Some(EXECUTOR),
        "a new row says whose it is"
    );

    let messages = store.load_messages(&id, 100).expect("load");
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].role, "user");
    assert_eq!(messages[0].content, "hi");
    assert_eq!(messages[1].role, "assistant");
    assert_eq!(messages[1].content, "hello!");
    assert_eq!(store.message_count(&id).unwrap(), 2);
}

#[test]
fn load_messages_respects_limit_and_stays_ordered() {
    let store = SessionStore::in_memory().expect("store");
    let id = store.create_session("t", EXECUTOR).expect("create");
    for i in 0..5 {
        store
            .append_message(&id, user(&id, &format!("m{i}")))
            .expect("append");
    }
    let last_two = store.load_messages(&id, 2).expect("load");
    assert_eq!(
        last_two
            .iter()
            .map(|m| m.content.as_str())
            .collect::<Vec<_>>(),
        vec!["m3", "m4"]
    );
}

#[test]
fn rename_and_delete_work() {
    let store = SessionStore::in_memory().expect("store");
    let id = store.create_session("old", EXECUTOR).expect("create");
    store.append_message(&id, user(&id, "hi")).expect("append");

    store.rename_session(&id, "new").expect("rename");
    assert_eq!(
        store.list_sessions(10, EXECUTOR, true).unwrap()[0].title,
        "new"
    );

    store.delete_session(&id).expect("delete");
    assert!(store.list_sessions(10, EXECUTOR, true).unwrap().is_empty());
}

#[test]
fn deleting_a_session_cascades_to_its_messages() {
    let path = unique_dir("cascade").join("sessions.db");
    let store = SessionStore::open(&path).expect("open");
    let keep = store.create_session("keep", EXECUTOR).expect("create");
    let drop_me = store.create_session("drop", EXECUTOR).expect("create");
    store.append_message(&keep, user(&keep, "a")).expect("a");
    store
        .append_message(&drop_me, user(&drop_me, "b"))
        .expect("b");

    store.delete_session(&drop_me).expect("delete");

    // Reopen to prove the rows are really gone (cascade, not just filtering).
    let reopened = SessionStore::open(&path).expect("reopen");
    assert!(reopened.load_messages(&drop_me, 100).unwrap().is_empty());
    assert_eq!(reopened.load_messages(&keep, 100).unwrap().len(), 1);
}

#[test]
fn data_survives_across_store_instances() {
    let path = unique_dir("persist").join("sessions.db");

    let id = {
        let store = SessionStore::open(&path).expect("open");
        let id = store.create_session("persisted", EXECUTOR).expect("create");
        store
            .append_message(&id, user(&id, "remember me"))
            .expect("append");
        store
            .append_message(&id, SessionMessage::new(&id, "assistant", "ok"))
            .expect("append");
        id
    };

    let store = SessionStore::open(&path).expect("reopen");
    let sessions = store.list_sessions(10, EXECUTOR, true).expect("list");
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].id, id);
    assert_eq!(sessions[0].message_count, 2);
    let messages = store.load_messages(&id, 100).expect("load");
    assert_eq!(messages[0].content, "remember me");
}

#[test]
fn list_is_most_recently_updated_first() {
    let store = SessionStore::in_memory().expect("store");
    let first = store.create_session("first", EXECUTOR).expect("create");
    let second = store.create_session("second", EXECUTOR).expect("create");
    // Ensure a distinct millisecond so the ordering is deterministic.
    std::thread::sleep(std::time::Duration::from_millis(5));
    store.touch_session(&first).expect("touch");
    let ids: Vec<String> = store
        .list_sessions(10, EXECUTOR, true)
        .unwrap()
        .into_iter()
        .map(|s| s.id)
        .collect();
    assert_eq!(ids, vec![first, second]);
}

/// The connection waits for another process's lock instead of failing at once.
///
/// SQLite's default is zero, and the sessions DB can be met by two processes
/// (two default-path CLI/server processes, or a shared `--data-dir`), so the
/// value is pinned here: five seconds, the same the audit store waits with.
#[test]
fn a_file_database_gets_a_busy_timeout() {
    let path = unique_dir("busy").join("sessions.db");
    let store = SessionStore::open(&path).expect("open");
    assert_eq!(store.busy_timeout_ms().expect("busy_timeout"), 5_000);
    assert_eq!(
        store.busy_timeout_ms().expect("busy_timeout"),
        host_core::session::BUSY_TIMEOUT.as_millis() as i64
    );

    // The timeout is per connection, so a second store opened on the same file
    // has to set its own — an existing database is not enough.
    let second = SessionStore::open(&path).expect("reopen");
    assert_eq!(second.busy_timeout_ms().expect("busy_timeout"), 5_000);

    // The in-memory store goes through the same `init`.
    let memory = SessionStore::in_memory().expect("memory");
    assert_eq!(memory.busy_timeout_ms().expect("busy_timeout"), 5_000);
}

/// A failure **between** the insert and the timestamp bump leaves neither
/// behind.
///
/// The failure is injected where the second statement runs — a trigger that
/// refuses every `UPDATE` on `sessions` — because that is the only place a
/// half-done append could hide: without the transaction the insert has already
/// committed and the message outlives its failed append. This is the test that
/// fails before `append_message` was wrapped.
#[test]
fn a_failed_append_leaves_no_message_behind() {
    let path = unique_dir("atomic").join("sessions.db");
    let store = SessionStore::open(&path).expect("open");
    let id = store.create_session("atomic", EXECUTOR).expect("create");

    {
        let blocker = rusqlite::Connection::open(&path).expect("second connection");
        blocker
            .execute_batch(
                "CREATE TRIGGER no_touch BEFORE UPDATE ON sessions \
                 BEGIN SELECT RAISE(ABORT, 'sessions are frozen'); END;",
            )
            .expect("trigger");
    }

    let error = store
        .append_message(&id, user(&id, "should not survive"))
        .expect_err("the append must fail");
    assert!(matches!(error, SessionError::Sqlite(_)), "{error:?}");

    assert_eq!(store.message_count(&id).expect("count"), 0);
    let listed = store.list_sessions(10, EXECUTOR, true).expect("list");
    assert_eq!(listed[0].message_count, 0);
    assert_eq!(
        listed[0].updated_at_ms, listed[0].created_at_ms,
        "the timestamp bump must roll back with the insert"
    );

    // And the store is still usable afterwards: the aborted trigger only ever
    // refused writes, it did not poison the connection.
    let other = store
        .create_session("still works", EXECUTOR)
        .expect("create");
    assert_eq!(
        store.list_sessions(10, EXECUTOR, true).expect("list").len(),
        2
    );
    assert!(!other.is_empty());
}

// ---------------------------------------------------------------------------
// v1.0 M2b-2 — the schema's own version, in SQLite's `PRAGMA user_version`
// ---------------------------------------------------------------------------

#[test]
fn a_new_database_is_stamped_with_the_current_version() {
    let path = unique_dir("stamped").join("sessions.db");
    let store = SessionStore::open(&path).expect("open");
    assert_eq!(
        store.schema_version().expect("version"),
        SESSION_SCHEMA_VERSION
    );
    assert_eq!(SESSION_SCHEMA_VERSION, 1);
    // A brand-new file has nothing to back up.
    assert!(!path.with_extension("db.bak").exists());
}

#[test]
fn an_old_database_is_migrated_and_backed_up() {
    let path = unique_dir("migrate").join("sessions.db");
    // A database from before the column: the same DDL, no `executor_id`, and
    // `user_version` at SQLite's default of 0.
    {
        let conn = rusqlite::Connection::open(&path).expect("old db");
        conn.execute_batch(
            "CREATE TABLE sessions (id TEXT PRIMARY KEY, title TEXT NOT NULL, \
             created_at_ms INTEGER NOT NULL, updated_at_ms INTEGER NOT NULL); \
             CREATE TABLE session_messages (id INTEGER PRIMARY KEY AUTOINCREMENT, \
             session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE, \
             role TEXT NOT NULL, content TEXT NOT NULL, tool_call_json TEXT, \
             tool_call_id TEXT, created_at_ms INTEGER NOT NULL);",
        )
        .expect("old schema");
        conn.execute(
            "INSERT INTO sessions (id, title, created_at_ms, updated_at_ms) \
             VALUES ('sess-old', 'old title', 1, 1)",
            [],
        )
        .expect("old row");
    }
    let before = std::fs::read(&path).expect("read");

    let store = SessionStore::open(&path).expect("open migrates");
    assert_eq!(
        store.schema_version().expect("version"),
        SESSION_SCHEMA_VERSION
    );
    // The pre-migration bytes are kept, byte for byte.
    assert_eq!(
        std::fs::read(path.with_extension("db.bak")).expect("backup"),
        before,
        "the backup is what was there before the migration"
    );

    // The old row survives, **unnamed**: it belongs to the node itself, so the
    // node's query sees it…
    let listed = store.list_sessions(10, EXECUTOR, true).expect("list");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, "sess-old");
    assert_eq!(listed[0].executor_id, None, "an old row stays unnamed");
    // …and another executor's does not.
    let foreign = store.list_sessions(10, "executor-0", false).expect("list");
    assert!(
        foreign.is_empty(),
        "another executor must not see it: {foreign:?}"
    );

    // New rows carry the executor explicitly, and renaming an old one does not
    // claim it.
    store.rename_session("sess-old", "renamed").expect("rename");
    let mine = store.create_session("mine", EXECUTOR).expect("create");
    assert_eq!(
        store
            .session(&mine)
            .unwrap()
            .unwrap()
            .executor_id
            .as_deref(),
        Some(EXECUTOR)
    );
    assert_eq!(
        store.session("sess-old").unwrap().unwrap().executor_id,
        None,
        "renaming did not give the old row an owner"
    );
}

#[test]
fn a_newer_database_is_refused() {
    let path = unique_dir("too-new").join("sessions.db");
    {
        let conn = rusqlite::Connection::open(&path).expect("db");
        conn.execute_batch("PRAGMA user_version = 99;")
            .expect("stamp");
    }
    let refused = SessionStore::open(&path);
    assert!(
        matches!(
            refused.as_ref(),
            Err(SessionError::DataTooNew { found: 99, .. })
        ),
        "{:?}",
        refused.err()
    );
    // A refusal is not a migration: nothing was written, and no backup taken.
    assert!(!path.with_extension("db.bak").exists());
}

#[test]
fn a_database_that_already_has_the_column_is_current() {
    let path = unique_dir("idempotent").join("sessions.db");
    {
        // The state an interrupted migration leaves: the column is there, the
        // version stamp is not.
        let conn = rusqlite::Connection::open(&path).expect("db");
        conn.execute_batch(
            "CREATE TABLE sessions (id TEXT PRIMARY KEY, title TEXT NOT NULL, \
             created_at_ms INTEGER NOT NULL, updated_at_ms INTEGER NOT NULL, \
             executor_id TEXT);",
        )
        .expect("half-migrated schema");
    }
    let store = SessionStore::open(&path).expect("open must not re-add the column");
    assert_eq!(
        store.schema_version().expect("version"),
        SESSION_SCHEMA_VERSION
    );
}

#[test]
fn clearing_is_scoped_to_the_executor() {
    let store = SessionStore::in_memory().expect("store");
    let mine = store.create_session("mine", EXECUTOR).expect("create");
    let theirs = store
        .create_session("theirs", "executor-0")
        .expect("create");

    store.clear_all(EXECUTOR, true).expect("clear");
    let left: Vec<String> = store
        .list_sessions(10, "executor-0", false)
        .expect("list")
        .into_iter()
        .map(|s| s.id)
        .collect();
    assert_eq!(left, vec![theirs], "another executor's session stays");
    assert!(store.session(&mine).expect("session").is_none());
}
