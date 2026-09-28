//! The management plane: the registry and the room definitions a server may publish (v1.0 M4d).
//!
//! [connection.md §6.2](../../docs/connection.md) gives the cross-region server a
//! **management** role: it "**may** publish a node list and room definitions", and "**it is
//! a source, not an authority** — that is §4.1's rule applied one level out: a node merges
//! what it is handed, its own `peers.json` and `rooms.json` stay authoritative for itself,
//! and a conflict is **reported, never silently resolved**".
//!
//! This module is therefore exactly two things. A **document shape** — [`Registry`]: §4.1's
//! hand-down table, with the room definitions beside it — carried as an ordinary signed
//! frame (§3.1, and §6.4 makes the server a peer whose key the node holds). And the
//! **merge** that applies §4.1 to both halves ([`Registry::merge`]).
//!
//! **The shapes are borrowed, not re-invented.** The table half is a
//! [`NodeTable`](crate::discovery::NodeTable) — the same `{generation, peers}` a handed-down
//! table already is, read back with [`NodeTable::from_body`] — and each room is a
//! [`Room`](crate::rooms::Room) held to `rooms.json`'s own rules
//! ([`RoomsFile::check`]), so a source cannot carry a room a file would refuse. The rooms are
//! optional, because §6.2 says a server **may** publish them: a bare `NodeTable` frame is a
//! legal registry with no rooms.
//!
//! **Nothing here is authority, and that is visible in the types.** [`Merged`] hands back the
//! merged view *and* the reports: a peer the table disagrees about keeps the local entry, a
//! room that differs keeps the local definition, and neither is resolved in silence. The local
//! file is what a node believes; a registry is what somebody told it.
//!
//! **The request is this batch's, and §6 left it so.** §6 freezes the roles and the routing
//! rule, not the bytes: [`registry_request_body`] is the frame a node sends to be handed the
//! registry, recorded in [decisions §99](../../docs/decisions.md).
//!
//! **Dependency direction.** Publishing is a connection-layer act, so nothing here names a
//! capability, writes an audit row, or reaches above `net`; and the registry is **not** a
//! place the truth lives — that is what keeps the role from reading as a service
//! ([§6.5](../../docs/connection.md)).

use crate::discovery::{merge_table, DiscoveryError, MergeReport, NodeTable};
use crate::error::Category;
use crate::identity::NodeKey;
use crate::message::{MessageError, SignedMessage, PROTOCOL_VERSION};
use crate::peers::PeersFile;
use crate::rooms::{merge_rooms, Room, RoomMergeReport, RoomsError, RoomsFile};
use crate::sign::VerifiedMessage;
use serde_json::Value;

/// The body a node sends to be handed the registry (§6.2's management).
///
/// An ordinary signed frame addressed to the server itself, carrying the protocol version so
/// a server can refuse a request it does not speak. §6 freezes the roles and leaves the wire
/// syntax to the implementation, so this shape is this batch's, recorded in
/// [decisions §99](../../docs/decisions.md).
pub fn registry_request_body() -> Value {
    serde_json::json!({ "registry": PROTOCOL_VERSION })
}

/// Is this body a registry request?
pub fn is_registry_request(body: &Value) -> bool {
    body.get("registry").and_then(Value::as_u64) == Some(u64::from(PROTOCOL_VERSION))
}

/// What a server published: §4.1's table, and the room definitions.
#[derive(Debug, Clone, PartialEq)]
pub struct Registry {
    table: NodeTable,
    rooms: RoomsFile,
}

impl Registry {
    /// A registry from a table and a room set.
    pub fn new(table: NodeTable, rooms: RoomsFile) -> Self {
        Self { table, rooms }
    }

    /// The node table half.
    pub fn table(&self) -> &NodeTable {
        &self.table
    }

    /// The room definitions half.
    pub fn rooms(&self) -> &RoomsFile {
        &self.rooms
    }

    /// The table's generation, which is what a node compares against its own copy (§4.1).
    pub fn generation(&self) -> i64 {
        self.table.generation()
    }

    /// The body it travels as: the table's own two members, with the rooms beside them.
    pub fn to_body(&self) -> Value {
        let mut body = self.table.to_body();
        if let Some(object) = body.as_object_mut() {
            object.insert(
                "rooms".to_string(),
                serde_json::to_value(&self.rooms.rooms).expect("rooms are JSON"),
            );
        }
        body
    }

    /// Read a published registry out of a frame's body.
    ///
    /// The table half is read with [`NodeTable::from_body`], so a bare hand-down is a legal
    /// registry with no rooms (§6.2 says a server **may** publish room definitions); the room
    /// half, when present, is held to [`RoomsFile::check`].
    pub fn from_body(body: &Value) -> Result<Self, RegistryError> {
        let table = NodeTable::from_body(body).map_err(RegistryError::Table)?;
        let rooms = match body.get("rooms") {
            None => RoomsFile::empty(),
            Some(value) => {
                let rooms: Vec<Room> = serde_json::from_value(value.clone()).map_err(|e| {
                    RegistryError::Shape(format!("the published rooms are malformed: {e}"))
                })?;
                let rooms = RoomsFile {
                    schema_version: RoomsFile::SCHEMA_VERSION,
                    rooms,
                };
                rooms.check()?;
                rooms
            }
        };
        Ok(Self { table, rooms })
    }

    /// Read a published registry out of a message that **already passed verification**.
    pub fn from_verified(message: &VerifiedMessage) -> Result<Self, RegistryError> {
        Self::from_body(&message.body)
    }

    /// Sign the publish as an ordinary frame (§6.4: the server is a peer whose key the node
    /// knows).
    pub fn sign(
        &self,
        key: &NodeKey,
        from: &str,
        to: &str,
        ts: i64,
    ) -> Result<SignedMessage, RegistryError> {
        Ok(SignedMessage::sign(key, from, to, ts, self.to_body())?)
    }

    /// Apply §4.1 to both halves: the local files win, and every disagreement is reported.
    ///
    /// This is the whole of "a source, not an authority" in one call — the returned
    /// [`Merged`] view has the local entries first and the additions after them, and the two
    /// reports say what was added and what disagreed. Nothing local is replaced.
    pub fn merge(&self, local_peers: &PeersFile, local_rooms: &RoomsFile) -> Merged {
        let (peers, peers_report) = merge_table(local_peers, &self.table);
        let (rooms, rooms_report) = merge_rooms(local_rooms, &self.rooms);
        Merged {
            peers,
            peers_report,
            rooms,
            rooms_report,
        }
    }
}

/// What merging a published registry did.
#[derive(Debug, Clone, PartialEq)]
pub struct Merged {
    /// The merged peer view: the local entries first, then what the table added.
    pub peers: PeersFile,
    /// What the table merge found — additions, and every disagreement.
    pub peers_report: MergeReport,
    /// The merged room set: the local rooms first, then what the registry added.
    pub rooms: RoomsFile,
    /// What the room merge found.
    pub rooms_report: RoomMergeReport,
}

impl Merged {
    /// Did anything need a human's attention?
    pub fn has_conflicts(&self) -> bool {
        self.peers_report.has_conflicts() || self.rooms_report.has_conflicts()
    }
}

/// Why a published registry could not be read.
#[derive(Debug)]
pub enum RegistryError {
    /// The table half is not usable.
    Table(DiscoveryError),
    /// The room half is not usable — `rooms.json`'s own rules, and no others.
    Rooms(RoomsError),
    /// The body is not a registry.
    Shape(String),
    /// Signing refused.
    Message(MessageError),
}

impl std::fmt::Display for RegistryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RegistryError::Table(e) => write!(f, "{e}"),
            RegistryError::Rooms(e) => write!(f, "{e}"),
            RegistryError::Shape(why) => write!(f, "the registry is not usable: {why}"),
            RegistryError::Message(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for RegistryError {}

impl From<DiscoveryError> for RegistryError {
    fn from(error: DiscoveryError) -> Self {
        RegistryError::Table(error)
    }
}

impl From<RoomsError> for RegistryError {
    fn from(error: RoomsError) -> Self {
        RegistryError::Rooms(error)
    }
}

impl From<MessageError> for RegistryError {
    fn from(error: MessageError) -> Self {
        RegistryError::Message(error)
    }
}

/// A bad registry is the input being wrong, so the category is the half that failed —
/// [`crate::discovery::discovery_category`]'s answer for the table and
/// [`crate::rooms::rooms_category`]'s for the rooms, and `invalid` for a body that is neither.
pub fn registry_category(error: &RegistryError) -> Category {
    match error {
        RegistryError::Table(e) => crate::discovery::discovery_category(e),
        RegistryError::Rooms(e) => crate::rooms::rooms_category(e),
        _ => Category::Invalid,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::peers::PeerEntry;
    use crate::rooms::{Mention, RateRule, RoomRules};

    const NOW: i64 = 1_700_000_000_000;

    fn rules(messages: u32, window_seconds: u32) -> RoomRules {
        RoomRules {
            rate: RateRule {
                messages,
                window_seconds,
            },
            mention: Mention::Nobody,
            require_signature: true,
        }
    }

    fn room(name: &str, member: &str) -> Room {
        Room {
            name: name.to_string(),
            members: vec![member.to_string()],
            rules: rules(10, 60),
        }
    }

    #[test]
    fn a_registry_travels_as_a_signed_frame_and_reads_back() {
        let server = NodeKey::generate().expect("key");
        let node = NodeKey::generate().expect("key");
        let mut rooms = RoomsFile::empty();
        rooms.rooms.push(room("lab", "dev-a"));
        let table = NodeTable::new(
            4,
            vec![PeerEntry::new("dev-b", "127.0.0.1:2", node.public_jwk())],
        );
        let registry = Registry::new(table, rooms.clone());

        let signed = registry
            .sign(&server, "server", "dev-a", NOW)
            .expect("sign");
        let line = signed.to_line().expect("line");
        let parsed = SignedMessage::parse_line(&line).expect("parse");

        // The table half is M4b's own shape, so it reads with M4b's reader.
        let table = NodeTable::from_body(&parsed.body).expect("table");
        assert_eq!(table.generation(), 4);
        assert_eq!(table.entries().len(), 1);

        let read = Registry::from_body(&parsed.body).expect("registry");
        assert_eq!(read.generation(), 4);
        assert_eq!(read.rooms(), &rooms);
        assert_eq!(read, registry);
    }

    #[test]
    fn a_bare_hand_down_is_a_registry_with_no_rooms() {
        let node = NodeKey::generate().expect("key");
        let table = NodeTable::new(
            1,
            vec![PeerEntry::new("dev-b", "127.0.0.1:2", node.public_jwk())],
        );
        let read = Registry::from_body(&table.to_body()).expect("registry");
        assert_eq!(read.rooms(), &RoomsFile::empty());
    }

    #[test]
    fn a_published_room_may_not_drop_the_signature_floor() {
        let node = NodeKey::generate().expect("key");
        let table = NodeTable::new(
            1,
            vec![PeerEntry::new("dev-b", "127.0.0.1:2", node.public_jwk())],
        );
        let mut body = table.to_body();
        let mut bad = room("lab", "dev-a");
        bad.rules.require_signature = false;
        body["rooms"] = serde_json::json!([bad]);

        let error = Registry::from_body(&body).expect_err("no source may lower the floor");
        assert!(error.to_string().contains("require_signature"), "{error}");
        assert_eq!(registry_category(&error), Category::Invalid);
    }

    #[test]
    fn merging_reports_the_conflict_and_keeps_the_local_side() {
        let key = NodeKey::generate().expect("key");
        let local_peers = {
            let mut peers = PeersFile::empty();
            peers
                .peers
                .push(PeerEntry::new("dev-b", "127.0.0.1:99", key.public_jwk()));
            peers
        };
        let local_rooms = {
            let mut rooms = RoomsFile::empty();
            rooms.rooms.push(room("lab", "dev-a"));
            rooms
        };

        // The published side disagrees about dev-b's address and about `lab`, and knows a
        // node and a room the local side does not.
        let published = Registry::new(
            NodeTable::new(
                3,
                vec![
                    PeerEntry::new("dev-b", "127.0.0.1:2", key.public_jwk()),
                    PeerEntry::new("dev-c", "127.0.0.1:3", key.public_jwk()),
                ],
            ),
            {
                let mut rooms = RoomsFile::empty();
                rooms.rooms.push(room("lab", "dev-b"));
                rooms.rooms.push(room("quiet", "dev-a"));
                rooms
            },
        );

        let merged = published.merge(&local_peers, &local_rooms);
        assert!(merged.has_conflicts());
        assert_eq!(merged.peers_report.conflicts.len(), 1);
        assert_eq!(merged.rooms_report.conflicts.len(), 1);
        assert_eq!(merged.peers_report.added, 1);
        assert_eq!(merged.rooms_report.added, 1);

        // Local wins, on both halves.
        assert_eq!(
            merged.peers.entry("dev-b").expect("dev-b").addresses,
            vec!["127.0.0.1:99".to_string()]
        );
        assert_eq!(
            merged.rooms.room("lab").expect("lab").members,
            vec!["dev-a".to_string()]
        );
        // And the additions are there.
        assert!(merged.peers.entry("dev-c").is_some());
        assert!(merged.rooms.room("quiet").is_some());
    }

    #[test]
    fn the_request_body_is_the_documented_one() {
        assert!(is_registry_request(&registry_request_body()));
        assert!(!is_registry_request(&serde_json::json!({ "registry": 2 })));
        assert!(!is_registry_request(&serde_json::json!({})));
    }
}
