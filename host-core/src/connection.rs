//! The connection layer's files, loaded into the host (v1.0 batch W).
//!
//! `net` owns the formats — the node's key ([connection.md §2](../../docs/connection.md)),
//! the peer table (§4) and the rooms (§5) — and this module owns the **policy**: when they
//! are read at all, what a refusal means, and how it is reported. Nothing here parses a file
//! itself; every read goes through `net`, so the version rules have exactly one
//! implementation ([decisions §93](../../docs/decisions.md)).
//!
//! **The condition is the network settings.** §2 gives a node a key on the first start that
//! has networking configured, and a node with none is a node that never joins a network: it
//! grows **no key** and reads nothing. [`AppState`](crate::AppState) passes
//! `settings.network.is_some()`, and an unconfigured node gets three `None`s — the behaviour
//! every version before this one had.
//!
//! **Nothing here stops the host.** A missing file is normal (a node may know nobody and be
//! in no room), a file from a newer build is refused **without being written**, and anything
//! else is reported — the way the keyring degrades silently ([`crate::keyring`]). What it does
//! not do is *hide*: every refusal comes back as a [`ConnectionProblem`] for the host to log,
//! record and write to the chain.

use net::{
    NodeKey, PeersFile, RoomsFile, Versioned, VersionedLoad, NODE_KEY_FILE, PEERS_FILE, ROOMS_FILE,
};
use std::path::Path;

/// One of the three files the connection layer keeps in the data directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionFile {
    /// `<data-dir>/node.key` — the node's Ed25519 identity (connection.md §2).
    NodeKey,
    /// `<data-dir>/peers.json` — who this node knows (connection.md §4).
    Peers,
    /// `<data-dir>/rooms.json` — membership and the room rules (connection.md §5).
    Rooms,
}

impl ConnectionFile {
    /// The file's name, as it is on disk.
    pub fn name(self) -> &'static str {
        match self {
            ConnectionFile::NodeKey => NODE_KEY_FILE,
            ConnectionFile::Peers => PEERS_FILE,
            ConnectionFile::Rooms => ROOMS_FILE,
        }
    }
}

/// What went wrong with one file — reported, never hidden, and never fatal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionProblem {
    /// The file is from a **newer** build: refused, and **nothing was written**. This is
    /// [docs/api-compatibility.md §6](../../docs/api-compatibility.md)'s rule, and the reason
    /// a newer `node.key` is not minted over.
    TooNew {
        file: ConnectionFile,
        found: u32,
        supported: u32,
    },
    /// The file is there but not usable — a spliced JWK, a peer entry carrying a private key,
    /// a room that lowers §3's floor. The format's own checks, reported not repaired.
    Unusable { file: ConnectionFile, why: String },
    /// There is no key and one could not be minted or written.
    KeyUnavailable { why: String },
}

impl ConnectionProblem {
    /// The file it is about. Every problem is about one: `node.key` is the file a missing key
    /// belongs to.
    pub fn file(&self) -> ConnectionFile {
        match self {
            ConnectionProblem::TooNew { file, .. } | ConnectionProblem::Unusable { file, .. } => {
                *file
            }
            ConnectionProblem::KeyUnavailable { .. } => ConnectionFile::NodeKey,
        }
    }

    /// Is this the "from a newer build" refusal? (The one the host has an event name for.)
    pub fn is_too_new(&self) -> bool {
        matches!(self, ConnectionProblem::TooNew { .. })
    }

    /// One line, for a log and for [`crate::AppState::connection_problem`].
    pub fn message(&self) -> String {
        match self {
            ConnectionProblem::TooNew {
                file,
                found,
                supported,
            } => format!(
                "{} is version {found}, this build reads {supported}; nothing was applied and \
                 nothing was written",
                file.name()
            ),
            ConnectionProblem::Unusable { file, why } => {
                format!("{} is not usable: {why}", file.name())
            }
            ConnectionProblem::KeyUnavailable { why } => {
                format!("{NODE_KEY_FILE} could not be minted: {why}")
            }
        }
    }
}

/// The connection layer's three files, as they were loaded, and what went wrong.
///
/// All three are `None` when no network wiring is configured: nothing was read and nothing was
/// written, so a node that never joins a network never grows a key (connection.md §2).
#[derive(Debug, Clone, Default)]
pub struct ConnectionFiles {
    /// The node's key, minted or read.
    pub node_key: Option<NodeKey>,
    /// The peer table, when there is one.
    pub peers: Option<PeersFile>,
    /// The room definitions, when there is a file.
    pub rooms: Option<RoomsFile>,
    /// Did **this** call mint the key? The one thing the caller cannot infer from the value —
    /// `net`'s loader answers the key, not which of "read" and "minted" happened — and what the
    /// `host.connection.key_minted` row is for.
    pub minted_key: bool,
    /// Every file that was refused or could not be used.
    pub problems: Vec<ConnectionProblem>,
}

impl ConnectionFiles {
    /// Did anything need a human's attention?
    pub fn has_problems(&self) -> bool {
        !self.problems.is_empty()
    }
}

/// Read the three files, or read nothing at all (v1.0 batch W).
///
/// `configured == false` is the answer every version before v1.0 gave: **nothing is read and
/// nothing is written**, which is what keeps a node that never joins a network from growing a
/// key (connection.md §2).
///
/// `data_dir` is the instance's own directory, so two `AppState`s in one process keep their own
/// connection files, exactly as they keep their own settings and sessions.
pub fn load(data_dir: &Path, configured: bool) -> ConnectionFiles {
    let mut files = ConnectionFiles::default();
    if !configured {
        return files;
    }
    load_node_key(data_dir, &mut files);
    load_peers(data_dir, &mut files);
    load_rooms(data_dir, &mut files);
    files
}

/// The node's key: read it, or mint it once (connection.md §2).
///
/// `net`'s `load_or_create_in` owns the policy — mint on the first start with networking
/// configured, never by a read, refuse a newer file — and answers the key. Whether the file was
/// **there first** is therefore what tells the host that a mint happened, and that is exactly
/// the event it writes. A too-new file reaches the same error channel as a broken one, so the
/// refusal is read once more to name the case: "this file is from a newer build" is a different
/// report from "this file is broken", and only the first has an event name.
fn load_node_key(data_dir: &Path, files: &mut ConnectionFiles) {
    let existed = data_dir.join(NODE_KEY_FILE).exists();
    match NodeKey::load_or_create_in(data_dir, true) {
        Ok(Some(key)) => {
            files.minted_key = !existed;
            files.node_key = Some(key);
        }
        Ok(None) => files.problems.push(ConnectionProblem::KeyUnavailable {
            why: "there is no key although networking is configured".to_string(),
        }),
        Err(error) => match NodeKey::load_in(data_dir) {
            Ok(VersionedLoad::TooNew { found }) => files.problems.push(ConnectionProblem::TooNew {
                file: ConnectionFile::NodeKey,
                found,
                supported: <NodeKey as Versioned>::SCHEMA_VERSION,
            }),
            _ => files.problems.push(ConnectionProblem::Unusable {
                file: ConnectionFile::NodeKey,
                why: error.to_string(),
            }),
        },
    }
}

/// The peer table (connection.md §4).
///
/// A **missing** file is not a problem: a node may know nobody, and that is a working
/// configuration — the same answer `settings.executors`' empty list gives.
fn load_peers(data_dir: &Path, files: &mut ConnectionFiles) {
    match PeersFile::load_in(data_dir) {
        Ok(VersionedLoad::Missing) => {}
        Ok(VersionedLoad::Current(file)) | Ok(VersionedLoad::Migrated { value: file, .. }) => {
            files.peers = Some(file);
        }
        Ok(VersionedLoad::TooNew { found }) => files.problems.push(ConnectionProblem::TooNew {
            file: ConnectionFile::Peers,
            found,
            supported: PeersFile::SCHEMA_VERSION,
        }),
        Err(error) => files.problems.push(ConnectionProblem::Unusable {
            file: ConnectionFile::Peers,
            why: error.to_string(),
        }),
    }
}

/// The room definitions (connection.md §5); the same four outcomes as the peer table.
fn load_rooms(data_dir: &Path, files: &mut ConnectionFiles) {
    match RoomsFile::load_in(data_dir) {
        Ok(VersionedLoad::Missing) => {}
        Ok(VersionedLoad::Current(file)) | Ok(VersionedLoad::Migrated { value: file, .. }) => {
            files.rooms = Some(file);
        }
        Ok(VersionedLoad::TooNew { found }) => files.problems.push(ConnectionProblem::TooNew {
            file: ConnectionFile::Rooms,
            found,
            supported: RoomsFile::SCHEMA_VERSION,
        }),
        Err(error) => files.problems.push(ConnectionProblem::Unusable {
            file: ConnectionFile::Rooms,
            why: error.to_string(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> std::path::PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!(
            "riscdom-connection-unit-{tag}-{}-{nanos}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch dir");
        dir
    }

    #[test]
    fn an_unconfigured_node_reads_nothing_and_writes_nothing() {
        let dir = scratch("unconfigured");
        let files = load(&dir, false);
        assert!(files.node_key.is_none());
        assert!(files.peers.is_none());
        assert!(files.rooms.is_none());
        assert!(!files.minted_key);
        assert!(!files.has_problems());
        // Nothing was created: not the key, and not a directory.
        assert!(!dir.join(NODE_KEY_FILE).exists());
    }

    #[test]
    fn a_configured_node_mints_a_key_and_an_empty_directory_is_not_a_problem() {
        let dir = scratch("configured");
        let files = load(&dir, true);
        assert!(
            files.minted_key,
            "the first start with networking mints one"
        );
        assert!(files.node_key.is_some());
        assert!(dir.join(NODE_KEY_FILE).is_file());
        // A node may know nobody and be in no room; neither is missing.
        assert!(files.peers.is_none());
        assert!(files.rooms.is_none());
        assert!(!files.has_problems(), "{:?}", files.problems);

        // A second read takes the key as it stands and mints nothing.
        let again = load(&dir, true);
        assert!(!again.minted_key, "not minted twice");
        assert_eq!(again.node_key, files.node_key);
    }

    #[test]
    fn every_problem_names_its_file_and_says_what_happened() {
        let dir = scratch("problems");
        std::fs::write(
            dir.join(PEERS_FILE),
            br#"{"schema_version": 9, "peers": []}"#,
        )
        .expect("write");
        let files = load(&dir, true);
        assert!(files.peers.is_none(), "a newer file is refused, not read");
        assert_eq!(files.problems.len(), 1);
        let problem = &files.problems[0];
        assert_eq!(problem.file(), ConnectionFile::Peers);
        assert!(problem.is_too_new());
        let message = problem.message();
        assert!(message.contains(PEERS_FILE), "{message}");
        assert!(message.contains('9'), "{message}");
        // And nothing was written over it.
        assert_eq!(
            std::fs::read_to_string(dir.join(PEERS_FILE)).expect("read"),
            r#"{"schema_version": 9, "peers": []}"#
        );
    }
}
