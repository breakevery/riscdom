//! Discovery: the static node table, the UDP beacon, and the room filter (v1.0 M4b).
//!
//! [connection.md §4](../../docs/connection.md) freezes two sources of addresses, in this
//! order of authority:
//!
//! 1. **The in-network server hands down a table** (§4.1). Its entries are exactly
//!    [`PeerEntry`]s, the hand-down is an ordinary signed frame (§3.1), and the table is a
//!    **source, not an authority**: a node merges it into its own view, its own
//!    `peers.json` still wins, and a disagreement is **reported, never silently resolved**.
//! 2. **A UDP broadcast is the supplement** (§4.2): one datagram, one signed frame, whose
//!    only permitted effect is to offer an address. The *only* thing in this protocol that
//!    travels over UDP — no request, no answer, no hand-down uses it.
//!
//! Two rules from §4.2 are what keep the beacon from becoming a side door for trust, and
//! both are implemented here rather than described: **room isolation is a filter with a
//! safe default** (adopt only when the announced rooms intersect the ones this node is
//! configured for; a node with none configured adopts nothing), and **an announcement
//! refreshes an address but cannot introduce a key** (a node the receiver does not already
//! know is *reported*, never adopted on the announcement's own say-so).
//!
//! **What this batch does not do.** The membership list itself is `rooms.json`, whose shape
//! is M4c's — so the filter here reads a set of room names handed to it (in memory, for
//! now) and fixes the **filter**, not the file. The relay, the cross-region server and the
//! wide area are M4d's.

use crate::error::Category;
use crate::message::{MessageError, SignedMessage};
use crate::peers::{PeerEntry, PeersError, PeersFile};
use crate::sign::VerifiedMessage;
use crate::transport::TransportError;
use serde_json::Value;
use std::collections::BTreeSet;
use std::net::{SocketAddr, UdpSocket};

/// The port a broadcast is sent to and received on.
///
/// **A protocol constant, not a setting** (§4.3): a broadcast must reach a node that knows
/// nothing yet, so it cannot itself be discovered, and a *configurable* port would let two
/// nodes on one link fail to see each other in silence.
pub const BROADCAST_PORT: u16 = 47821;

/// The most a single datagram may carry. An implementation limit, like the frame cap.
pub const MAX_DATAGRAM_BYTES: usize = 65_507;

/// The rooms **this** node is configured for.
///
/// [§5.3](../../docs/connection.md) reads "configured for a room" as two things at once: the
/// file names the room **and** the room's `members[]` lists this node. The file is M4c's,
/// so this type takes the result — the set of rooms this node is in — and the **filter** is
/// what is frozen here. An empty set is the default-deny case: nothing is adopted.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RoomFilter {
    rooms: BTreeSet<String>,
}

impl RoomFilter {
    /// Not in any room: adopts nothing.
    pub fn none() -> Self {
        Self::default()
    }

    /// The rooms this node is in.
    pub fn of<I, S>(rooms: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            rooms: rooms.into_iter().map(Into::into).collect(),
        }
    }

    /// Is this node in no room at all?
    pub fn is_empty(&self) -> bool {
        self.rooms.is_empty()
    }

    /// How many rooms this node is in.
    pub fn len(&self) -> usize {
        self.rooms.len()
    }

    /// Is this node in `room`?
    pub fn contains(&self, room: &str) -> bool {
        self.rooms.contains(room)
    }

    /// The rooms, in order.
    pub fn rooms(&self) -> impl Iterator<Item = &String> {
        self.rooms.iter()
    }

    /// Does an announcement naming `announced` pass the filter?
    ///
    /// The intersection rule, with **default deny** at the empty end: an announcement that
    /// names no room is admitted only by a receiver that is in no room either — which is no
    /// receiver at all, since a node in no room adopts nothing. (The filter is checked
    /// before that case is reached, in [`consider_announcement`].)
    pub fn admits(&self, announced: &[String]) -> bool {
        if self.rooms.is_empty() {
            return false;
        }
        announced.iter().any(|room| self.rooms.contains(room))
    }
}

/// The table an in-network server hands down (§4.1).
#[derive(Debug, Clone, PartialEq)]
pub struct NodeTable {
    generation: i64,
    entries: Vec<PeerEntry>,
}

impl NodeTable {
    /// A table with a generation and its entries.
    pub fn new(generation: i64, entries: Vec<PeerEntry>) -> Self {
        Self {
            generation,
            entries,
        }
    }

    /// The monotone integer a node compares against the copy it holds.
    pub fn generation(&self) -> i64 {
        self.generation
    }

    /// Is this newer than the generation a node last saw?
    pub fn is_newer_than(&self, seen: i64) -> bool {
        self.generation > seen
    }

    /// The entries.
    pub fn entries(&self) -> &[PeerEntry] {
        &self.entries
    }

    /// One node's entry.
    pub fn entry(&self, node_id: &str) -> Option<&PeerEntry> {
        self.entries.iter().find(|entry| entry.node_id == node_id)
    }

    /// The body a signed frame carries: the generation and the entries.
    pub fn to_body(&self) -> Value {
        serde_json::json!({
            "generation": self.generation,
            "peers": self.entries,
        })
    }

    /// Read a hand-down out of a frame's body.
    pub fn from_body(body: &Value) -> Result<Self, DiscoveryError> {
        let generation = body
            .get("generation")
            .and_then(Value::as_i64)
            .ok_or_else(|| DiscoveryError::Shape("the table carries no generation".into()))?;
        let entries: Vec<PeerEntry> = serde_json::from_value(
            body.get("peers")
                .cloned()
                .ok_or_else(|| DiscoveryError::Shape("the table carries no `peers`".into()))?,
        )
        .map_err(|e| DiscoveryError::Shape(format!("the table's entries are malformed: {e}")))?;
        for entry in &entries {
            entry
                .verifying_key()
                .map_err(|e| DiscoveryError::Shape(e.to_string()))?;
        }
        Ok(Self {
            generation,
            entries,
        })
    }

    /// Sign the hand-down as an ordinary frame (§4.1: the server is a peer whose key the
    /// node knows).
    pub fn sign(
        &self,
        key: &crate::NodeKey,
        from: &str,
        to: &str,
        ts: i64,
    ) -> Result<SignedMessage, DiscoveryError> {
        Ok(SignedMessage::sign(key, from, to, ts, self.to_body())?)
    }

    /// Read a hand-down out of a message that **already passed verification**.
    pub fn from_verified(message: &VerifiedMessage) -> Result<Self, DiscoveryError> {
        Self::from_body(&message.body)
    }
}

/// A disagreement between the local file and a handed-down table.
///
/// §4.1: the local file wins and the conflict is **reported**. This is the report.
#[derive(Debug, Clone, PartialEq)]
pub struct Conflict {
    pub node_id: String,
    /// What the local `peers.json` says.
    pub local: String,
    /// What the handed-down table says.
    pub from_table: String,
}

/// What merging a table into the local view did.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MergeReport {
    /// Entries for nodes the local file did not know.
    pub added: usize,
    /// Entries identical to what the local file already had.
    pub unchanged: usize,
    /// Entries the local file disagrees with. Local wins; nothing was changed.
    pub conflicts: Vec<Conflict>,
}

impl MergeReport {
    /// Did anything need a human's attention?
    pub fn has_conflicts(&self) -> bool {
        !self.conflicts.is_empty()
    }
}

/// Merge a handed-down table into the local view (§4.1).
///
/// The result is the **view**: the local entries first, then the ones the table added.
/// Nothing local is ever replaced — a conflict keeps the local entry and lands in
/// [`MergeReport::conflicts`].
pub fn merge_table(local: &PeersFile, table: &NodeTable) -> (PeersFile, MergeReport) {
    let mut merged = local.clone();
    merged.schema_version = PeersFile::SCHEMA_VERSION;
    let mut report = MergeReport::default();
    for entry in table.entries() {
        match local.entry(&entry.node_id) {
            Some(known) if known.same_as(entry) => report.unchanged += 1,
            Some(known) => report.conflicts.push(Conflict {
                node_id: entry.node_id.clone(),
                local: known.summary(),
                from_table: entry.summary(),
            }),
            None => {
                merged.peers.push(entry.clone());
                report.added += 1;
            }
        }
    }
    (merged, report)
}

/// The body of an announcement: the sender's own entry, plus the rooms it announces for
/// (§4.2).
pub fn announcement_body(entry: &PeerEntry, rooms: &[String]) -> Value {
    serde_json::json!({ "entry": entry, "rooms": rooms })
}

/// Sign a beacon (§4.2): one datagram, one signed frame.
pub fn sign_announcement(
    key: &crate::NodeKey,
    from: &str,
    ts: i64,
    entry: &PeerEntry,
    rooms: &[String],
) -> Result<SignedMessage, DiscoveryError> {
    Ok(SignedMessage::sign(
        key,
        from,
        from,
        ts,
        announcement_body(entry, rooms),
    )?)
}

/// Send one frame as one datagram.
pub fn send_datagram(
    socket: &UdpSocket,
    to: SocketAddr,
    message: &SignedMessage,
) -> Result<usize, TransportError> {
    let frame = message.to_line()?;
    socket
        .send_to(frame.as_bytes(), to)
        .map_err(|e| TransportError::Io {
            op: crate::transport::Op::Write,
            why: e.to_string(),
        })
}

/// Read one datagram as one frame.
///
/// A datagram is already a message boundary, so there is no framing to do here — only a
/// parse. A socket with a read timeout answers [`TransportError::Timeout`] when nothing
/// arrives, exactly as the TCP side does.
pub fn receive_datagram(
    socket: &UdpSocket,
    buffer: &mut [u8],
) -> Result<(SignedMessage, SocketAddr), TransportError> {
    let (read, from) = socket.recv_from(buffer).map_err(|e| match e.kind() {
        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut => TransportError::Timeout {
            op: crate::transport::Op::Read,
            after: std::time::Duration::ZERO,
        },
        _ => TransportError::Io {
            op: crate::transport::Op::Read,
            why: e.to_string(),
        },
    })?;
    let text = std::str::from_utf8(&buffer[..read])
        .map_err(|e| TransportError::Malformed { why: e.to_string() })?;
    let message = SignedMessage::parse_line(text)?;
    Ok((message, from))
}

/// The rooms an announcement names.
pub fn announced_rooms(body: &Value) -> Result<Vec<String>, DiscoveryError> {
    serde_json::from_value(
        body.get("rooms")
            .cloned()
            .ok_or_else(|| DiscoveryError::Shape("the announcement names no rooms".into()))?,
    )
    .map_err(|e| DiscoveryError::Shape(format!("the announced rooms are malformed: {e}")))
}

/// The entry an announcement carries.
pub fn announced_entry(body: &Value) -> Result<PeerEntry, DiscoveryError> {
    let entry: PeerEntry = serde_json::from_value(
        body.get("entry")
            .cloned()
            .ok_or_else(|| DiscoveryError::Shape("the announcement carries no entry".into()))?,
    )
    .map_err(|e| DiscoveryError::Shape(format!("the announced entry is malformed: {e}")))?;
    entry
        .verifying_key()
        .map_err(|e| DiscoveryError::Shape(e.to_string()))?;
    Ok(entry)
}

/// What a verified announcement was allowed to do (§4.2).
#[derive(Debug, Clone, PartialEq)]
pub enum Adoption {
    /// The node was already known: its addresses are on offer.
    RefreshedAddresses {
        node_id: String,
        addresses: Vec<String>,
    },
    /// The node is **not** known. It is reported for the deployer to add — never adopted
    /// on the announcement's own authority.
    ReportedUnknown {
        node_id: String,
        addresses: Vec<String>,
        rooms: Vec<String>,
    },
    /// The announcement names no room this node is in.
    IgnoredOutOfRoom { rooms: Vec<String> },
}

impl Adoption {
    /// Was the announcement out of room?
    pub fn ignored(&self) -> bool {
        matches!(self, Adoption::IgnoredOutOfRoom { .. })
    }

    /// Did it introduce a key? Never: that is the rule this type exists to make visible.
    pub fn introduced_a_key(&self) -> bool {
        false
    }

    /// A `peers.json` entry, when the addresses are worth taking.
    pub fn refreshed(&self) -> Option<(&str, &[String])> {
        match self {
            Adoption::RefreshedAddresses { node_id, addresses } => {
                Some((node_id.as_str(), addresses.as_slice()))
            }
            _ => None,
        }
    }
}

/// Decide what one **verified** announcement may do.
///
/// The order is §4.2's: the room filter first (default deny), then "refresh an address,
/// never introduce a key".
pub fn consider_announcement(
    body: &Value,
    local: &PeersFile,
    filter: &RoomFilter,
) -> Result<Adoption, DiscoveryError> {
    let rooms = announced_rooms(body)?;
    if !filter.admits(&rooms) {
        return Ok(Adoption::IgnoredOutOfRoom { rooms });
    }
    let entry = announced_entry(body)?;
    match local.entry(&entry.node_id) {
        Some(_) => Ok(Adoption::RefreshedAddresses {
            node_id: entry.node_id,
            addresses: entry.addresses,
        }),
        None => Ok(Adoption::ReportedUnknown {
            node_id: entry.node_id,
            addresses: entry.addresses,
            rooms,
        }),
    }
}

/// Why discovery refused something.
#[derive(Debug)]
pub enum DiscoveryError {
    /// The peer table itself is unusable.
    Peers(PeersError),
    /// A body is not a table or not an announcement.
    Shape(String),
    /// Signing refused.
    Message(MessageError),
}

impl std::fmt::Display for DiscoveryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DiscoveryError::Peers(e) => write!(f, "{e}"),
            DiscoveryError::Shape(why) => write!(f, "{why}"),
            DiscoveryError::Message(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for DiscoveryError {}

impl From<PeersError> for DiscoveryError {
    fn from(error: PeersError) -> Self {
        DiscoveryError::Peers(error)
    }
}

impl From<MessageError> for DiscoveryError {
    fn from(error: MessageError) -> Self {
        DiscoveryError::Message(error)
    }
}

/// Discovery refusals are the input being wrong — a malformed table, a body that is not an
/// announcement — never a transport failure: the frame that carried it already arrived.
pub fn discovery_category(error: &DiscoveryError) -> Category {
    match error {
        DiscoveryError::Peers(e) => crate::peers::peers_category(e),
        _ => Category::Invalid,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_filter_adopts_nothing_and_a_matching_one_admits() {
        let empty = RoomFilter::none();
        assert!(!empty.admits(&["lab".to_string()]));
        assert!(empty.is_empty());

        let lab = RoomFilter::of(["lab"]);
        assert!(lab.admits(&["lab".to_string()]));
        assert!(lab.admits(&["other".to_string(), "lab".to_string()]));
        assert!(!lab.admits(&["other".to_string()]));
        // An announcement that names nothing is not admitted by a filter that names
        // something.
        assert!(!lab.admits(&[]));
        assert_eq!(lab.len(), 1);
        assert!(lab.contains("lab"));
    }
}
