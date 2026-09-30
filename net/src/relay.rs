//! The cross-region server: the relay, and the session both sides dial out to (v1.0 M4d).
//!
//! [connection.md §6](../../docs/connection.md) freezes what this is: **a dedicated
//! deployment of the same software, run by a deployer**, that carries the traffic two
//! nodes cannot carry themselves. This module implements the **relay** role, the session it
//! needs, and §6.2's **signalling** and **management** roles — the address query, answered
//! from what the server knows, and the registry it publishes as a **source, not an
//! authority** ([`crate::registry`]). **Audit aggregation** is §7's, and waits on M5.
//!
//! **Why a session, and not a dial-back.** §6.3 is explicit: *the server never dials a
//! node*. It listens and waits to be dialled, "both sides dial **out**", and that is
//! what removes the need for hole punching. So the relay cannot reach a destination at
//! its `addresses[]`; it hands a frame down **the connection that destination already
//! holds open**. [`SessionTable`] is that table: who is dialled in right now — §6.3's
//! "who is where", a transport fact rather than a copy of anything.
//!
//! **What is routed, and on what.** On the signed `to` and nothing else (§6.3). A frame
//! is parsed, its sender is authenticated — steps 1–3 of §3 and §3.2's record, and *not*
//! step 4, because a relayed frame is addressed to somebody else by design
//! ([`crate::sign::authenticate_forwarded`]) — and then it either belongs to the server
//! itself, where [`Local::of`] says which role it is (a hello, an address query, a registry
//! request) and the server answers it, or it is handed down the destination's session,
//! **only if the server knows that destination**; an
//! unknown one is refused rather than broadcast. A room name is not a node the server
//! knows, so a frame addressed to a room is refused here too: §5 keeps membership local
//! and nothing in §6 asks the server to expand a room.
//!
//! The bytes handed on are the frame that arrived — [`frame_bytes`] is the one framer —
//! so §3.1's "byte-identical on both paths" survives the relay leg and the signature
//! still covers what the sender signed.
//!
//! **Stateless about the content.** No message is stored and a `body` is never opened:
//! [`route`] reads the preamble and `to`, and nothing else. What is kept — the session
//! table and the per-peer replay record — is transport state, exactly as §6.3 says, and
//! it is memory-only, like the record §3.2 gives every receiver.
//!
//! **The local wire syntax is this batch's, and §6 left it so.** §6 freezes the roles, the
//! routing rule, and what each role may know — not the bytes: [`hello_body`] opens a
//! session, [`address_query_body`] asks where a node is, and
//! [`registry_request_body`](crate::registry::registry_request_body) asks for the registry.
//! [Decisions §98](../../docs/decisions.md) records the first and [§99](../../docs/decisions.md)
//! the other two.
//!
//! **Dependency direction, and what is deliberately absent.** Relaying is a transport
//! concern and the authorisation is §3's, so nothing here names a capability, writes an
//! audit row, or reaches above `net`. [`NoRelay`](crate::transport::NoRelay) stays the
//! honest answer for a deployment that configures no server.

use crate::discovery::NodeTable;
use crate::error::Category;
use crate::identity::NodeKey;
use crate::liveness::{RecoverMethod, Report, Transition, TransitionSink, WitnessTable};
use crate::message::{body_hash, now_ms, SignedMessage, PROTOCOL_VERSION};
use crate::peers::{PeerEntry, PeersError, PeersFile};
use crate::registry::{Registry, RegistryError};
use crate::replay::{ReplayError, ReplayGuard};
use crate::rooms::{RoomsError, RoomsFile};
use crate::sign::{authenticate_forwarded, check_identity, PeerKeys, VerifiedMessage, VerifyError};
use crate::transport::{
    frame_bytes, Connection, Listener, Op, Relay, TransportConfig, TransportError,
};
use serde::Serialize;
use serde_json::Value;
use std::collections::{HashMap, VecDeque};
use std::io::Write;
use std::net::{SocketAddr, TcpStream};
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};

/// The generation a server's registry starts at.
///
/// It advances whenever the registry it publishes changes ([`RelayServer::set_peers`],
/// [`RelayServer::set_rooms`]), which is what §4.1's generation is for: a node compares the
/// number it is handed against the copy it holds.
pub const FIRST_GENERATION: i64 = 1;

/// The body a node opens its session with (§6.4: it uses a server because its own
/// deployer configured one).
///
/// It is an **ordinary signed frame addressed to the server itself**, carrying the
/// protocol version so a server can refuse a session it does not speak. §6 freezes the
/// routing rules and leaves the wire syntax to the implementation, so this shape is this
/// batch's, recorded in [decisions §98](../../docs/decisions.md). Any frame the server
/// authenticates on a connection binds that connection to its sender; the hello is what
/// lets a node that has nothing to send yet still be *found*.
pub fn hello_body() -> Value {
    serde_json::json!({ "hello": PROTOCOL_VERSION })
}

/// Is this body the session-opening [`hello_body`]?
pub fn is_hello(body: &Value) -> bool {
    body.get("hello").and_then(Value::as_u64) == Some(u64::from(PROTOCOL_VERSION))
}

/// The body a node asks "where can `node_id` be reached?" with (§6.2's signalling).
///
/// An ordinary signed frame addressed to the server itself. §6 leaves the wire syntax to the
/// implementation, so this shape is this batch's, recorded in
/// [decisions §99](../../docs/decisions.md).
///
pub fn address_query_body(node_id: &str) -> Value {
    serde_json::json!({ "query": node_id })
}

/// The node an address query asks about, when the body is one.
pub fn address_query(body: &Value) -> Option<&str> {
    body.get("query").and_then(Value::as_str)
}

/// The body the server answers an address query with: **addresses, and nothing else**.
///
/// §6.2 is explicit that signalling "knows **addresses**, never payloads", so the answer is
/// one member and nothing in it is a body, a room or a name a message was sent under. A node
/// the server cannot place is answered with an **empty list** rather than refused: the
/// question was well-formed, and "nowhere I know" is the honest answer to it.
pub fn address_answer_body(addresses: &[String]) -> Value {
    serde_json::json!({ "addresses": addresses })
}

/// The addresses an answer carries, when the body is one.
pub fn answered_addresses(body: &Value) -> Option<Vec<String>> {
    let addresses = body.get("addresses")?;
    serde_json::from_value(addresses.clone()).ok()
}

/// How often a node beats ([connection.md §6.6](../../docs/connection.md)): 15 seconds.
pub const HEARTBEAT_INTERVAL: std::time::Duration = std::time::Duration::from_secs(15);

/// How long a row reads `online` after the last beat (§6.6): 45 seconds — three intervals.
pub const ONLINE_WINDOW_MS: i64 = 3 * 15_000;

/// How often a node reports its chain's digest to its cross-region server (§7, v1.0 M4e-1): 30
/// seconds. The interval is §7's, decided there and not in the implementation; it is a **default**
/// in the same sense §6.6's 15 s is — the loop takes whatever interval its caller passes.
pub const DIGEST_INTERVAL: std::time::Duration = std::time::Duration::from_secs(30);

/// The body a node **registers** with (§6.6).
pub fn register_body(addresses: &[String], capabilities: &[String], rooms: &[String]) -> Value {
    serde_json::json!({
        "register": PROTOCOL_VERSION,
        "addresses": addresses,
        "capabilities": capabilities,
        "rooms": rooms,
    })
}

/// Is this body a registration?
///
/// **`register`, not `registry`** — one letter apart, and two different frames: the first is what a
/// node *tells* its server (§6.6), the second is what it *asks* it for (§6.2's management).
pub fn is_register(body: &Value) -> bool {
    body.get("register").and_then(Value::as_u64) == Some(u64::from(PROTOCOL_VERSION))
}

/// The body a node **beats** with (§6.6): the smallest thing it can say.
pub fn heartbeat_body() -> Value {
    serde_json::json!({ "heartbeat": PROTOCOL_VERSION })
}

/// Is this body a heartbeat?
pub fn is_heartbeat(body: &Value) -> bool {
    body.get("heartbeat").and_then(Value::as_u64) == Some(u64::from(PROTOCOL_VERSION))
}

/// The body the server answers a registration with: taken, and a row exists.
pub fn registered_body() -> Value {
    serde_json::json!({ "registered": PROTOCOL_VERSION })
}

/// Is this body the server's answer to a registration?
pub fn is_registered(body: &Value) -> bool {
    body.get("registered").and_then(Value::as_u64) == Some(u64::from(PROTOCOL_VERSION))
}

/// A chain digest: the chain's head and how many events lead to it (§7, v1.0
/// M4e-1).
///
/// **This is not a second hash and not the whole chain.** It is a **commitment to a point**: the
/// head hash a verifier can compare against, and the length that says which point it is. Both come
/// from reads the store already exposes ([`audit::AuditStore::last_hash`] and
/// [`audit::AuditStore::count`]), so the digest adds no formula — `audit`'s
/// [`compute_hash`](audit::compute_hash) is not called here and is not changed by §7 (decisions
/// §127 point 2: the chain's semantics extend, its formula does not).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChainDigest {
    /// The head hash (`None` for an empty chain), lowercase hex.
    pub chain: Option<String>,
    /// How many events lead to that head. `0` when the chain is empty.
    pub length: u64,
}

impl ChainDigest {
    /// Read the digest off a store's own head — the chain's last hash and its event count.
    pub fn of(store: &audit::AuditStore) -> Result<Self, audit::AuditError> {
        Ok(Self {
            chain: store.last_hash()?,
            length: store.count()? as u64,
        })
    }

    /// The body a node reports it with (§7): an ordinary signed frame addressed to the server.
    pub fn to_body(&self) -> Value {
        digest_body(self.chain.as_deref(), self.length)
    }

    /// Read a digest out of a body, when the body is one.
    pub fn of_body(body: &Value) -> Option<Self> {
        if !is_digest(body) {
            return None;
        }
        Some(Self {
            chain: body
                .get("chain")
                .and_then(Value::as_str)
                .map(str::to_string),
            length: body.get("length").and_then(Value::as_u64)?,
        })
    }
}

/// The body a node **reports its chain's digest** with (§7).
///
/// Like a registration and a heartbeat, it is an **ordinary §3 frame addressed to the server**, not a
/// new frame *type*: §6 left the wire syntax to the implementation (§6.6's rule, applied here). The
/// identity travels in the preamble's `from` (§3), so the body names nobody. `chain` is `null` on an
/// empty chain — `length` is then `0`, and both facts are reported rather than suppressed.
pub fn digest_body(chain: Option<&str>, length: u64) -> Value {
    serde_json::json!({
        "digest": PROTOCOL_VERSION,
        "chain": chain,
        "length": length,
    })
}

/// Is this body a chain digest (§7)?
pub fn is_digest(body: &Value) -> bool {
    body.get("digest").and_then(Value::as_u64) == Some(u64::from(PROTOCOL_VERSION))
}

/// How many key events the server keeps **per node** (v1.0 M4e-2).
///
/// The log is for "what just happened", not for history: the chain holds history, and the aggregation
/// role holders *reports*. A fork needs a conflicting segment and a takeover needs a 60 s stand-in, so
/// 256 is far more than any node produces in a session, and 256 × a few hundred bytes is a bounded,
/// trivial amount per node however many nodes report.
pub const KEY_EVENT_LOG: usize = 256;

/// One **key event**, on its way to the server the moment it happens (v1.0 M4e-2).
///
/// A digest is a *commitment* to a point on a chain, and idempotent: the newest one replaces the last. A
/// key event is a *fact* ([roadmap §4](../../docs/roadmap-v1.0.md): an ejection, a fork, a temporary
/// centre's takeover), so the server keeps a bounded log of them rather than a latest value. The identity
/// travels in the preamble's `from` (§3), so the body names nobody — the same as a digest or a beat.
#[derive(Debug, Clone, PartialEq)]
pub struct KeyEvent {
    /// When the node says it happened (its own clock).
    pub at_ms: i64,
    /// The action, spelled as the chain spells it, so a reader can find the row it describes.
    pub action: String,
    /// The event's detail, verbatim.
    pub detail: Value,
}

impl KeyEvent {
    /// The body it is pushed with (§7): an ordinary signed frame addressed to the server.
    pub fn to_body(&self) -> Value {
        key_event_body(self.at_ms, &self.action, &self.detail)
    }

    /// Read a key event out of a body, when the body is one.
    pub fn of_body(body: &Value) -> Option<Self> {
        if !is_key_event(body) {
            return None;
        }
        Some(Self {
            at_ms: body.get("at_ms").and_then(Value::as_i64)?,
            action: body.get("action").and_then(Value::as_str)?.to_string(),
            detail: body.get("detail")?.clone(),
        })
    }
}

/// The body a node **pushes a key event** with (§7, v1.0 M4e-2).
///
/// Like a digest, an ordinary §3 frame addressed to the server — the shape §6.6 left to the
/// implementation. One frame per event, the moment it happens: unlike the digest's 30-second batch,
/// there is nothing to batch, because the event *is* the news.
pub fn key_event_body(at_ms: i64, action: &str, detail: &Value) -> Value {
    serde_json::json!({
        "key_event": PROTOCOL_VERSION,
        "at_ms": at_ms,
        "action": action,
        "detail": detail,
    })
}

/// Is this body a key event (§7)?
pub fn is_key_event(body: &Value) -> bool {
    body.get("key_event").and_then(Value::as_u64) == Some(u64::from(PROTOCOL_VERSION))
}

/// What a registration **claims** ([connection.md §6.6](../../docs/connection.md)).
///
/// Claims, not facts: the server's own `peers.json` and `rooms.json` stay authoritative, and what a
/// node reports is a **source** (§4.1's rule one level out). Three lists and nothing else — in
/// particular **no key**, which arrives through configuration and never through a frame.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Registration {
    /// Where the node says it can be dialled. **Empty is legal** (§6.6): a node reachable only
    /// through the relay reports none.
    pub addresses: Vec<String>,
    /// What it says it can do.
    pub capabilities: Vec<String>,
    /// The rooms it says it is in.
    pub rooms: Vec<String>,
}

impl Registration {
    /// A registration of these rooms, with no addresses and no capabilities.
    pub fn in_rooms<S: Into<String>>(rooms: impl IntoIterator<Item = S>) -> Self {
        Self {
            addresses: Vec::new(),
            capabilities: Vec::new(),
            rooms: rooms.into_iter().map(Into::into).collect(),
        }
    }

    /// Read the claims out of a registration body (§6.6). A list the frame does not carry reads as
    /// empty: the shape says a node may report nothing.
    pub fn of(body: &Value) -> Option<Registration> {
        if !is_register(body) {
            return None;
        }
        let list = |key: &str| -> Vec<String> {
            body.get(key)
                .and_then(|value| serde_json::from_value(value.clone()).ok())
                .unwrap_or_default()
        };
        Some(Registration {
            addresses: list("addresses"),
            capabilities: list("capabilities"),
            rooms: list("rooms"),
        })
    }
}

/// Whether a row is inside or outside §6.6's window.
///
/// Serialised as the same word [`Self::as_str`] returns (`"online"` / `"offline"`), because that
/// word is what the API document and the CLI already print (v1.0 M6-2b-2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Online {
    /// A beat arrived within [`ONLINE_WINDOW_MS`].
    Online,
    /// None did.
    Offline,
}

impl Online {
    /// The word.
    pub fn as_str(self) -> &'static str {
        match self {
            Online::Online => "online",
            Online::Offline => "offline",
        }
    }
}

/// One row of the online-status table, as it reads at a given moment (§6.6).
///
/// Serialised by `GET /v0/online` (v1.0 M6-2b-2) as it stands: there is **no key material here**,
/// `addresses` is empty while no peer port is claimed, `capabilities`/`rooms` are what the node
/// declared, and `state`/`judged_at_ms` are this server's **opinion** of a peer rather than a fact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OnlineEntry {
    /// The key, and the row's only identity.
    pub node_id: String,
    /// The addresses the node last reported.
    pub addresses: Vec<String>,
    /// What it last said it can do.
    pub capabilities: Vec<String>,
    /// The rooms it last said it is in.
    pub rooms: Vec<String>,
    /// When the last beat arrived — or the registration, which counts as one (§6.6).
    pub last_heartbeat_ms: i64,
    /// When the collective judgement first held (§6.7), or `None`. Kept apart from [`Self::state`]
    /// on purpose: `state` is one observer's silence, this is the agreement of the witnesses.
    pub judged_at_ms: Option<i64>,
    /// Inside [`ONLINE_WINDOW_MS`] or not.
    pub state: Online,
}

/// The server's online-status table ([connection.md §6.6](../../docs/connection.md)).
///
/// **Runtime state, in memory** — the same trade §3.2's record makes: a restart forgets the whole
/// table (a node re-registers when it reconnects), and **a row is never deleted by going offline**,
/// because "offline" and "never registered" have to stay distinguishable. Nothing in it is a
/// message and nothing in it is written to a chain: it is a transport fact, and this batch records
/// nothing about it ([decisions §103](../../docs/decisions.md)).
#[derive(Clone, Default)]
pub struct OnlineTable {
    inner: Arc<OnlineInner>,
}

#[derive(Default)]
struct OnlineInner {
    rows: Mutex<HashMap<String, Row>>,
}

#[derive(Debug, Clone)]
struct Row {
    addresses: Vec<String>,
    capabilities: Vec<String>,
    rooms: Vec<String>,
    last_heartbeat_ms: i64,
    judged_at_ms: Option<i64>,
}

impl OnlineTable {
    /// Nobody has registered.
    pub fn new() -> Self {
        Self::default()
    }

    /// Take a registration (§6.6): a row is created, or the claims of one already there are
    /// replaced. **Idempotent** — registering twice is not two rows — and the beat advances either
    /// way, because §6.6 counts a registration as a beat.
    ///
    /// Returns whether the row is **new**.
    pub fn register(&self, node_id: &str, claims: &Registration, now: i64) -> bool {
        let mut rows = self
            .inner
            .rows
            .lock()
            .expect("the online table is not poisoned");
        match rows.get_mut(node_id) {
            Some(row) => {
                row.addresses = claims.addresses.clone();
                row.capabilities = claims.capabilities.clone();
                row.rooms = claims.rooms.clone();
                row.last_heartbeat_ms = now;
                false
            }
            None => {
                rows.insert(
                    node_id.to_string(),
                    Row {
                        addresses: claims.addresses.clone(),
                        capabilities: claims.capabilities.clone(),
                        rooms: claims.rooms.clone(),
                        last_heartbeat_ms: now,
                        judged_at_ms: None,
                    },
                );
                true
            }
        }
    }

    /// Take a beat (§6.6). A row that is not there is **not** created by a beat — a row is created
    /// by a registration — so this answers whether the beat landed.
    pub fn beat(&self, node_id: &str, now: i64) -> bool {
        match self
            .inner
            .rows
            .lock()
            .expect("the online table is not poisoned")
            .get_mut(node_id)
        {
            Some(row) => {
                row.last_heartbeat_ms = now;
                true
            }
            None => false,
        }
    }

    /// Every row, by `node_id`, as it reads at `now`.
    pub fn rows(&self, now: i64) -> Vec<OnlineEntry> {
        let rows = self
            .inner
            .rows
            .lock()
            .expect("the online table is not poisoned");
        let mut entries: Vec<OnlineEntry> = rows
            .iter()
            .map(|(node_id, row)| OnlineEntry {
                node_id: node_id.clone(),
                addresses: row.addresses.clone(),
                capabilities: row.capabilities.clone(),
                rooms: row.rooms.clone(),
                last_heartbeat_ms: row.last_heartbeat_ms,
                judged_at_ms: row.judged_at_ms,
                state: state_of(row.last_heartbeat_ms, now),
            })
            .collect();
        entries.sort_by(|a, b| a.node_id.cmp(&b.node_id));
        entries
    }

    /// One row, as it reads at `now`.
    pub fn row(&self, node_id: &str, now: i64) -> Option<OnlineEntry> {
        self.rows(now)
            .into_iter()
            .find(|entry| entry.node_id == node_id)
    }

    /// How many rows there are, online or not.
    pub fn len(&self) -> usize {
        self.inner
            .rows
            .lock()
            .expect("the online table is not poisoned")
            .len()
    }

    /// Has nobody registered?
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// How many rows read `online` at `now`.
    pub fn online_at(&self, now: i64) -> usize {
        self.rows(now)
            .iter()
            .filter(|entry| entry.state == Online::Online)
            .count()
    }

    /// Is `node_id` known **and** `online` at `now`? §6.7's test for whether a node may testify.
    pub fn is_online(&self, node_id: &str, now: i64) -> bool {
        self.row(node_id, now)
            .map(|entry| entry.state == Online::Online)
            .unwrap_or(false)
    }

    /// Record a judgement (§6.7): set `judged_at_ms` once, and say whether **this** call set it.
    /// A row must exist — a node the server never knew is not judged.
    pub fn mark_judged(&self, node_id: &str, now: i64) -> bool {
        let mut rows = self
            .inner
            .rows
            .lock()
            .expect("the online table is not poisoned");
        match rows.get_mut(node_id) {
            Some(row) if row.judged_at_ms.is_none() => {
                row.judged_at_ms = Some(now);
                true
            }
            _ => false,
        }
    }

    /// Clear a judgement, answering the moment it had held (§6.7's recovery).
    pub fn clear_judged(&self, node_id: &str) -> Option<i64> {
        let mut rows = self
            .inner
            .rows
            .lock()
            .expect("the online table is not poisoned");
        rows.get_mut(node_id)
            .and_then(|row| row.judged_at_ms.take())
    }

    /// The node was heard from again: refresh the row, so a `state` read is `online` (§6.7's
    /// recovery, for a report rather than a beat).
    pub fn heard_from(&self, node_id: &str, now: i64) -> bool {
        let mut rows = self
            .inner
            .rows
            .lock()
            .expect("the online table is not poisoned");
        match rows.get_mut(node_id) {
            Some(row) => {
                row.last_heartbeat_ms = now;
                true
            }
            None => false,
        }
    }
}

/// §6.6's rule, in one place: inside the window is `online`, past it is `offline`.
fn state_of(last_heartbeat_ms: i64, now: i64) -> Online {
    if now - last_heartbeat_ms <= ONLINE_WINDOW_MS {
        Online::Online
    } else {
        Online::Offline
    }
}

/// What a frame addressed to the server **itself** is, once it has authenticated (§6.2).
///
/// The local half of the four roles, plus the honest fifth answer: a local frame the server
/// has nothing to do with is **reported, never guessed at**. An unrecognised local frame is
/// still held to §3's identity checks — it simply earns no answer.
#[derive(Debug, Clone, PartialEq)]
pub enum Local {
    /// A session opener: the socket is bound to its sender, and nothing is answered.
    Hello,
    /// A registration: the claims a node makes about itself (§6.6).
    Register(Registration),
    /// A heartbeat: the smallest thing a node can say (§6.6).
    Heartbeat,
    /// A chain digest, reported on §7's timer (v1.0 M4e-1).
    Digest { chain: Option<String>, length: u64 },
    /// A key event, pushed the moment it happens (v1.0 M4e-2).
    KeyEvent {
        at_ms: i64,
        action: String,
        detail: Value,
    },
    /// A prober's report that a peer is unreachable (§6.7).
    UnreachableReport { node_id: String },
    /// A prober's report that a peer is reachable again (§6.7).
    ReachableReport { node_id: String },
    /// An address query: where the named node can be reached.
    Addresses { node_id: String },
    /// A registry request: the node table and the room definitions, as they stand.
    Registry,
    /// A local frame with nothing to do.
    Unrecognised { body: Value },
}

impl Local {
    /// Read what a body is asking the server for.
    ///
    /// The order matters once: `register` is tested **before** `registry`, because the two bodies
    /// differ by one letter and mean opposite directions (§6.6's reporting up, §6.2's handing down).
    pub fn of(body: &Value) -> Self {
        if is_hello(body) {
            return Local::Hello;
        }
        if let Some(claims) = Registration::of(body) {
            return Local::Register(claims);
        }
        if is_heartbeat(body) {
            return Local::Heartbeat;
        }
        if let Some(digest) = ChainDigest::of_body(body) {
            return Local::Digest {
                chain: digest.chain,
                length: digest.length,
            };
        }
        if let Some(event) = KeyEvent::of_body(body) {
            return Local::KeyEvent {
                at_ms: event.at_ms,
                action: event.action,
                detail: event.detail,
            };
        }
        if let Some(report) = crate::liveness::report_of(body) {
            return match report {
                Report::Unreachable(node_id) => Local::UnreachableReport { node_id },
                Report::Reachable(node_id) => Local::ReachableReport { node_id },
            };
        }
        if crate::registry::is_registry_request(body) {
            return Local::Registry;
        }
        if let Some(node_id) = address_query(body) {
            return Local::Addresses {
                node_id: node_id.to_string(),
            };
        }
        Local::Unrecognised { body: body.clone() }
    }
}

/// What the server did with a frame addressed to it, so the act is visible rather than
/// inferred.
#[derive(Debug, Clone, PartialEq)]
pub enum LocalReply {
    /// A session opener: the socket was bound, and nothing is answered.
    Silent,
    /// A registration was taken and answered: the row exists (`true`) or was refreshed (`false`).
    Registered { node_id: String, fresh: bool },
    /// A heartbeat was taken. Nothing is answered — a beat is a statement, not a question.
    Beat { node_id: String },
    /// A chain digest was taken as this node's latest report (§7). Nothing is answered, for the
    /// same reason a beat is not: a report is a statement.
    DigestTaken { node_id: String, length: u64 },
    /// A key event was taken into this node's log (v1.0 M4e-2). Nothing is answered, for the same
    /// reason a digest is not: a fact is a statement.
    KeyEventTaken { node_id: String, action: String },
    /// A heartbeat arrived for a node with no row: §6.6 creates a row by a **registration**, so a
    /// beat alone places nobody. Nothing is answered for it either.
    Unplaced { node_id: String },
    /// A prober's report that a peer is unreachable was taken (§6.7).
    UnreachableReported { peer: String },
    /// A prober's report of life was taken (§6.7).
    ReachableReported { peer: String },
    /// An address answer went down the session.
    Addresses { addresses: Vec<String> },
    /// The registry went down the session.
    Registry { generation: i64 },
    /// A local frame with nothing to do.
    Unrecognised,
}

/// What the server did with one frame.
#[derive(Debug, Clone, PartialEq)]
pub enum Routed {
    /// Addressed to the server itself — the signalling and management roles' (§6.2).
    Local {
        from: String,
        to: String,
        /// Which role it is: [`Local::of`]'s reading of the body.
        action: Local,
    },
    /// Handed down the destination's live session.
    Forwarded { from: String, to: String },
}

impl Routed {
    /// The sender the frame authenticated as.
    pub fn from(&self) -> &str {
        match self {
            Routed::Local { from, .. } | Routed::Forwarded { from, .. } => from,
        }
    }

    /// Who the frame named.
    pub fn to(&self) -> &str {
        match self {
            Routed::Local { to, .. } | Routed::Forwarded { to, .. } => to,
        }
    }

    /// What the server itself was asked, when the frame was for it.
    pub fn local(&self) -> Option<&Local> {
        match self {
            Routed::Local { action, .. } => Some(action),
            Routed::Forwarded { .. } => None,
        }
    }

    /// Did it leave the server?
    pub fn was_forwarded(&self) -> bool {
        matches!(self, Routed::Forwarded { .. })
    }
}

/// Why a frame was not carried.
#[derive(Debug)]
pub enum RelayError {
    /// The line is not a message this layer can read. §3.1's `invalid`.
    Malformed(String),
    /// The §3 checks said no — an unknown sender, a signature that does not verify, a
    /// version we do not speak, a stale or replayed timestamp. The category is §3's own,
    /// because the refusal is §3's ([`VerifyError::category`]).
    Refused(VerifyError),
    /// §6.3: the destination is not a node the server knows. Refused rather than
    /// broadcast, so one sender's mistake does not become everybody's traffic.
    UnknownDestination(String),
    /// §6.3: the destination is known but is not dialled in — and the server never dials
    /// out, so there is nothing to hand the frame to.
    NoSession(String),
    /// The push down a session failed: the socket, not the policy.
    Transport(TransportError),
}

impl RelayError {
    /// The error model's category for this refusal.
    pub fn category(&self) -> Category {
        match self {
            RelayError::Malformed(_) => Category::Invalid,
            RelayError::Refused(error) => error.category(),
            RelayError::UnknownDestination(_) | RelayError::NoSession(_) => Category::Refused,
            RelayError::Transport(error) => error.category(),
        }
    }
}

impl std::fmt::Display for RelayError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RelayError::Malformed(why) => write!(f, "the frame does not parse: {why}"),
            RelayError::Refused(error) => write!(f, "{error}"),
            RelayError::UnknownDestination(to) => {
                write!(f, "{to} is not a node this server knows")
            }
            RelayError::NoSession(to) => {
                write!(
                    f,
                    "{to} is known but not dialled in, and the server never dials"
                )
            }
            RelayError::Transport(error) => write!(f, "the frame did not go out: {error}"),
        }
    }
}

impl std::error::Error for RelayError {}

impl From<VerifyError> for RelayError {
    fn from(error: VerifyError) -> Self {
        RelayError::Refused(error)
    }
}

impl From<ReplayError> for RelayError {
    /// §3.2's refusals are §3's, so they take the same category a verifier would give
    /// them ([`VerifyError::category`]) rather than a second opinion here.
    fn from(error: ReplayError) -> Self {
        RelayError::Refused(VerifyError::from(error))
    }
}

impl From<TransportError> for RelayError {
    fn from(error: TransportError) -> Self {
        RelayError::Transport(error)
    }
}

/// Why a server could not be built.
///
/// Both of its files are checked at construction, so a deployer learns that a table or a room
/// set is unusable when the server starts rather than when the first node asks for it.
#[derive(Debug)]
pub enum RelayServerError {
    /// The peer table is not usable.
    Peers(PeersError),
    /// The room definitions are not usable.
    Rooms(RoomsError),
}

impl std::fmt::Display for RelayServerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RelayServerError::Peers(e) => write!(f, "{e}"),
            RelayServerError::Rooms(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for RelayServerError {}

impl From<PeersError> for RelayServerError {
    fn from(error: PeersError) -> Self {
        RelayServerError::Peers(error)
    }
}

impl From<RoomsError> for RelayServerError {
    fn from(error: RoomsError) -> Self {
        RelayServerError::Rooms(error)
    }
}

/// Where the relay hands a frame on (§6.3's "hands the frame on").
///
/// The seam is one method, so the routing rule can be read — and tested — with no socket
/// anywhere near it: [`SessionTable`] is what a deployment uses, and a test's is a list.
pub trait Forwarder: Send + Sync {
    /// Hand `frame` down `node_id`'s live session.
    ///
    /// The bytes must be the frame that arrived: a relay that re-encoded it would break a
    /// signature it is not able to read.
    fn forward_to(&self, node_id: &str, frame: &str) -> Result<(), RelayError>;
}

/// Who is dialled in right now, and the socket to reach them on.
///
/// One entry per node: the newest session that authenticated as it. A node that dials in
/// twice has its older connection retired **by id**, so a disconnect cannot evict a
/// successor that replaced it.
#[derive(Clone, Default)]
pub struct SessionTable {
    inner: Arc<SessionInner>,
}

#[derive(Default)]
struct SessionInner {
    sessions: Mutex<HashMap<String, Arc<Live>>>,
    next_id: AtomicU64,
}

struct Live {
    id: u64,
    addr: SocketAddr,
    writer: Mutex<TcpStream>,
}

impl SessionTable {
    /// Nobody is dialled in.
    pub fn new() -> Self {
        Self::default()
    }

    /// Bind `node_id` to `writer` — the socket it dialled in on — and to `addr`, where it
    /// dialled in from. Replaces whatever session it had; the returned id is what that
    /// connection retires itself with.
    pub fn bind(&self, node_id: &str, writer: TcpStream, addr: SocketAddr) -> u64 {
        let id = self.inner.next_id.fetch_add(1, Ordering::Relaxed);
        let live = Arc::new(Live {
            id,
            addr,
            writer: Mutex::new(writer),
        });
        self.inner
            .sessions
            .lock()
            .expect("the session table is not poisoned")
            .insert(node_id.to_string(), live);
        id
    }

    /// Retire `node_id`'s session **if it is still `id`**.
    ///
    /// A connection that has ended must take its own session away and nothing else: the
    /// id is what stops an older socket's thread from removing the newer session that
    /// took its place.
    pub fn unbind(&self, node_id: &str, id: u64) -> bool {
        let mut sessions = self.inner.sessions.lock().expect("sessions");
        match sessions.get(node_id) {
            Some(live) if live.id == id => {
                sessions.remove(node_id);
                true
            }
            _ => false,
        }
    }

    /// Is `node_id` dialled in?
    pub fn is_present(&self, node_id: &str) -> bool {
        self.inner
            .sessions
            .lock()
            .expect("sessions")
            .contains_key(node_id)
    }

    /// Where `node_id` dialled in from, when it is dialled in.
    ///
    /// This is the signalling role's most current answer: §6.2 has signalling say where a
    /// `node_id` "can be reached", and a live session is where it just reached the server
    /// from. Nothing about the *content* of that session is here, which is the other half of
    /// the same sentence.
    pub fn address_of(&self, node_id: &str) -> Option<SocketAddr> {
        self.inner
            .sessions
            .lock()
            .expect("sessions")
            .get(node_id)
            .map(|live| live.addr)
    }

    /// How many sessions are live.
    pub fn len(&self) -> usize {
        self.inner.sessions.lock().expect("sessions").len()
    }

    /// Is nobody dialled in?
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Forwarder for SessionTable {
    fn forward_to(&self, node_id: &str, frame: &str) -> Result<(), RelayError> {
        // Take the entry and let the map go before writing: a slow push down one session
        // must not hold up a bind or an unbind on another.
        let live = self
            .inner
            .sessions
            .lock()
            .expect("sessions")
            .get(node_id)
            .cloned()
            .ok_or_else(|| RelayError::NoSession(node_id.to_string()))?;
        let mut writer = live.writer.lock().expect("session writer");
        let bytes = frame_bytes(frame);
        writer
            .write_all(&bytes)
            .and_then(|()| writer.flush())
            .map_err(|e| TransportError::Io {
                op: Op::Write,
                why: e.to_string(),
            })?;
        Ok(())
    }
}

/// Route one frame (§6.3), against explicit state so the rule can be read on its own.
///
/// The order is the whole of the rule: the frame must parse; its **sender** must be a
/// node the server knows with a signature that verifies (§6.3's authorisation, which is
/// §3's model and no new credential); a frame addressed to the server itself is the local
/// roles' business; a frame naming a destination the server cannot place is refused; and
/// otherwise the frame — the same bytes — goes down the destination's session.
///
/// **A hello does not consume the sender's record.** §3.2's record is per peer, and a
/// session is opened *after* the frame that failed the direct path was signed — so a
/// record that the hello advanced would refuse that frame for being older than the hello
/// that carried it there. The hello is idempotent (it says "this socket is mine" and
/// nothing else), so it is the one local frame the record does not see; every other frame,
/// carried or not, passes through it.
pub fn route(
    frame: &str,
    server_node_id: &str,
    peers: &PeersFile,
    keys: &PeerKeys,
    guard: &mut ReplayGuard,
    forwarder: &dyn Forwarder,
    now: i64,
) -> Result<Routed, RelayError> {
    let message =
        SignedMessage::parse_line(frame).map_err(|e| RelayError::Malformed(e.to_string()))?;
    // §6.3: "a node the server knows, with a signature that verifies" — steps 1–3 of §3.
    // Step 4 does not apply to either branch: a relayed frame is addressed elsewhere by
    // design, and a frame for the server is not a message it must be the addressee of to
    // read.
    if message.to == server_node_id {
        // The signalling and management roles' business (§6.2); the hello is the one the
        // record is not allowed to see.
        check_identity(&message, keys)?;
        let action = Local::of(&message.body);
        if !matches!(action, Local::Hello) {
            guard.accept(&message.from, message.ts, &body_hash(&message.body), now)?;
        }
        return Ok(Routed::Local {
            from: message.from,
            to: message.to,
            action,
        });
    }
    // Everything else is carried, and §6.3's authorisation is §3's model minus the
    // addressee step.
    authenticate_forwarded(&message, keys, guard, now)?;
    if peers.entry(&message.to).is_none() {
        return Err(RelayError::UnknownDestination(message.to));
    }
    forwarder.forward_to(&message.to, frame)?;
    Ok(Routed::Forwarded {
        from: message.from,
        to: message.to,
    })
}

/// The server: a peer the deployer runs, whose knowledge is its own `peers.json` (§6.4).
#[derive(Clone)]
pub struct RelayServer {
    inner: Arc<ServerInner>,
}

struct ServerInner {
    node_id: String,
    key: NodeKey,
    config: TransportConfig,
    peers: RwLock<PeersFile>,
    keys: RwLock<PeerKeys>,
    rooms: RwLock<RoomsFile>,
    generation: AtomicI64,
    guard: Mutex<ReplayGuard>,
    sessions: SessionTable,
    online: OnlineTable,
    witnesses: WitnessTable,
    /// The latest chain digest each node has reported (§7, v1.0 M4e-1). Memory-only: a digest is
    /// transport state, like the session table and the replay record, and §6.2's aggregation role
    /// "holds digests rather than messages" — nothing here becomes a second copy of the history.
    digests: Mutex<HashMap<String, ChainDigest>>,
    /// The **key events** each node has pushed, newest last, at most [`KEY_EVENT_LOG`] per node (v1.0
    /// M4e-2). Memory-only, like the digest table, and for the same reason: this is what the aggregation
    /// role holds, not a second copy of anyone's history.
    key_events: Mutex<HashMap<String, VecDeque<KeyEvent>>>,
    transition_sink: RwLock<Option<TransitionSink>>,
}

impl RelayServer {
    /// A server that knows `peers`, publishes `rooms`, and signs its answers with `key`.
    ///
    /// §6.4 makes it a peer, so it has a key pair like any other node and the nodes that use
    /// it hold the public half in their own `peers.json`. Both files are checked here, so a
    /// deployment cannot start holding something a node would refuse.
    pub fn new(
        node_id: &str,
        key: NodeKey,
        peers: PeersFile,
        rooms: RoomsFile,
        config: TransportConfig,
    ) -> Result<Self, RelayServerError> {
        let keys = peers.peer_keys()?;
        rooms.check()?;
        Ok(Self {
            inner: Arc::new(ServerInner {
                node_id: node_id.to_string(),
                key,
                config,
                peers: RwLock::new(peers),
                keys: RwLock::new(keys),
                rooms: RwLock::new(rooms),
                generation: AtomicI64::new(FIRST_GENERATION),
                guard: Mutex::new(ReplayGuard::new()),
                sessions: SessionTable::new(),
                online: OnlineTable::new(),
                witnesses: WitnessTable::new(),
                digests: Mutex::new(HashMap::new()),
                key_events: Mutex::new(HashMap::new()),
                transition_sink: RwLock::new(None),
            }),
        })
    }

    /// The name it answers to — the `to` that means "this is the server's own business".
    pub fn node_id(&self) -> &str {
        &self.inner.node_id
    }

    /// The live sessions.
    pub fn sessions(&self) -> &SessionTable {
        &self.inner.sessions
    }

    /// The online-status table as it reads **now** (§6.6).
    pub fn online(&self) -> Vec<OnlineEntry> {
        self.online_at(now_ms())
    }

    /// The online-status table as it reads at `now`.
    ///
    /// The clock is a parameter for the same reason [`Self::route`]'s is: §6.6's 45 seconds is a
    /// rule a test must be able to age without waiting them out.
    pub fn online_at(&self, now: i64) -> Vec<OnlineEntry> {
        self.inner.online.rows(now)
    }

    /// The online-status table itself, for a caller that wants to read it more than once.
    pub fn online_table(&self) -> &OnlineTable {
        &self.inner.online
    }

    /// Wire where a judgement goes (§6.7). The chain is host-core's, so the server **hands the
    /// transition out** rather than writing a row itself: a deployment that runs the server in a
    /// process with a chain installs a sink, and the standalone relay installs none.
    /// The latest digest each node has reported (§7), in the order their names sort.
    pub fn digests(&self) -> Vec<(String, ChainDigest)> {
        let digests = self
            .inner
            .digests
            .lock()
            .expect("the digest table is not poisoned");
        let mut out: Vec<(String, ChainDigest)> = digests
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        out.sort_by(|a, b| a.0.cmp(&b.0));
        out
    }

    /// What `node_id` last reported, if it reported at all (§7).
    pub fn digest_of(&self, node_id: &str) -> Option<ChainDigest> {
        self.inner
            .digests
            .lock()
            .expect("the digest table is not poisoned")
            .get(node_id)
            .cloned()
    }

    /// The key events a node has pushed, oldest first (v1.0 M4e-2).
    ///
    /// At most [`KEY_EVENT_LOG`] of them, and only the ones that node pushed over its own session.
    pub fn key_events_of(&self, node_id: &str) -> Vec<KeyEvent> {
        self.inner
            .key_events
            .lock()
            .expect("the key-event log is not poisoned")
            .get(node_id)
            .map(|log| log.iter().cloned().collect())
            .unwrap_or_default()
    }

    pub fn set_transition_sink(&self, sink: TransitionSink) {
        *self
            .inner
            .transition_sink
            .write()
            .expect("the transition sink is not poisoned") = Some(sink);
    }

    /// Hand a transition to the sink, when one is wired.
    fn fire(&self, transition: &Transition) {
        let sink = self
            .inner
            .transition_sink
            .read()
            .expect("the transition sink is not poisoned")
            .clone();
        if let Some(sink) = sink {
            sink(transition);
        }
    }

    /// Re-read §6.7's threshold for one subject, and record a **new** judgement.
    ///
    /// The row's `judged_at_ms` is set once, so a report that keeps arriving while the row is
    /// already judged is not a second transition.
    fn judge(&self, subject: &str, now: i64) {
        let judged = self.inner.witnesses.judge(subject, now, |witness| {
            self.inner.online.is_online(witness, now)
        });
        if let Some(judgement) = judged {
            if self.inner.online.mark_judged(&judgement.peer, now) {
                self.fire(&Transition::Judged(judgement));
            }
        }
    }

    /// The transport configuration sessions inherit.
    pub fn config(&self) -> TransportConfig {
        self.inner.config
    }

    /// The nodes it knows — its view, the boundary of what it will forward to, and the table
    /// half of what it publishes.
    pub fn peers(&self) -> PeersFile {
        self.inner.peers.read().expect("peers").clone()
    }

    /// The room definitions it publishes.
    pub fn rooms(&self) -> RoomsFile {
        self.inner.rooms.read().expect("rooms").clone()
    }

    /// The generation its registry currently carries.
    pub fn generation(&self) -> i64 {
        self.inner.generation.load(Ordering::Relaxed)
    }

    /// The registry as it stands: the nodes it knows, and the rooms its deployer published.
    ///
    /// This **is** §6.2's management role — what the server hands a node that asks, and what
    /// the node merges by §4.1's rule. It is assembled per request rather than stored, so what
    /// is published cannot drift from the table the server routes against.
    pub fn registry(&self) -> Registry {
        let peers = self.inner.peers.read().expect("peers");
        Registry::new(
            NodeTable::new(self.generation(), peers.peers.clone()),
            self.rooms(),
        )
    }

    /// Replace the view — a deployer republishing its registry.
    ///
    /// The keys are re-derived here so a table can never disagree with the keys in force, and
    /// the **generation advances**: the registry a node is handed has changed, which is exactly
    /// what §4.1's number is for.
    pub fn set_peers(&self, peers: PeersFile) -> Result<(), PeersError> {
        let keys = peers.peer_keys()?;
        *self.inner.peers.write().expect("peers") = peers;
        *self.inner.keys.write().expect("keys") = keys;
        self.inner.generation.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    /// Replace the room definitions.
    ///
    /// Held to `rooms.json`'s own checks, so a deployment cannot publish a room a file would
    /// refuse; the generation advances for the same reason as [`Self::set_peers`]'s.
    pub fn set_rooms(&self, rooms: RoomsFile) -> Result<(), RoomsError> {
        rooms.check()?;
        *self.inner.rooms.write().expect("rooms") = rooms;
        self.inner.generation.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    /// Route one frame against this server's own state.
    pub fn route(&self, frame: &str, now: i64) -> Result<Routed, RelayError> {
        let peers = self.inner.peers.read().expect("peers");
        let keys = self.inner.keys.read().expect("keys");
        let mut guard = self.inner.guard.lock().expect("guard");
        route(
            frame,
            &self.inner.node_id,
            &peers,
            &keys,
            &mut guard,
            &self.inner.sessions,
            now,
        )
    }

    /// Where `node_id` can be reached, from what the server knows (§6.2).
    ///
    /// Two sources, and both are facts about the **transport**: the session it dialled in on,
    /// which is where it *is* right now, and its `peers.json` entry, which is what its deployer
    /// wrote down. The live address comes first because it is the more current of the two, and
    /// the list is de-duplicated. A node the server cannot place is an **empty list** — the
    /// honest answer to a well-formed question, not a refusal.
    ///
    /// Nothing here is a payload: a session contributes an address and nothing else.
    fn addresses_for(&self, node_id: &str) -> Vec<String> {
        let mut addresses: Vec<String> = Vec::new();
        if let Some(address) = self.inner.sessions.address_of(node_id) {
            addresses.push(address.to_string());
        }
        let peers = self.inner.peers.read().expect("peers");
        if let Some(entry) = peers.entry(node_id) {
            for address in &entry.addresses {
                if !addresses.iter().any(|known| known == address) {
                    addresses.push(address.clone());
                }
            }
        }
        addresses
    }

    /// Sign `body` as the server and hand it down `node_id`'s session.
    ///
    /// The server is a peer (§6.4), so an answer is an ordinary §3 frame: the node verifies it
    /// against the server's public key and runs §3's six steps on it like anything else. The
    /// answer travels the session the question arrived on — which is why an answer is only ever
    /// sent to a node that is dialled in.
    fn send_to(&self, node_id: &str, body: Value) -> Result<(), RelayError> {
        let message = SignedMessage::sign(
            &self.inner.key,
            &self.inner.node_id,
            node_id,
            now_ms(),
            body,
        )
        .map_err(|e| RelayError::Malformed(e.to_string()))?;
        let frame = message
            .to_line()
            .map_err(|e| RelayError::Malformed(e.to_string()))?;
        self.inner.sessions.forward_to(node_id, &frame)
    }

    /// Answer a frame addressed to the server itself (§6.2's signalling and management).
    ///
    /// Returns what it did, so a caller — a test, or the deployer's banner — can see the act
    /// rather than infer it. A failed write is the socket's business rather than the frame's:
    /// the caller keeps reading, and the next read says whether the session is still there.
    pub fn answer_local(
        &self,
        from: &str,
        action: &Local,
        now: i64,
    ) -> Result<LocalReply, RelayError> {
        match action {
            // The socket was bound when the frame authenticated; a hello asks for nothing else.
            Local::Hello => Ok(LocalReply::Silent),
            // A local frame with nothing to do is reported, and nothing is invented for it.
            Local::Unrecognised { .. } => Ok(LocalReply::Unrecognised),
            // §6.6's reporting up: the claims are taken as a **source** — the server's own files stay
            // authoritative — and the registration is answered so the node knows it landed. A node the
            // server does not know never reaches here: §6.3's authorisation refused it already, which is
            // what makes "a key cannot arrive by frame" structural rather than promised.
            Local::Register(claims) => {
                let fresh = self.inner.online.register(from, claims, now);
                self.send_to(from, registered_body())?;
                Ok(LocalReply::Registered {
                    node_id: from.to_string(),
                    fresh,
                })
            }
            // A beat refreshes the row a registration made and is answered with nothing (§6.6): the
            // smallest thing a node can say does not earn a frame back. A beat for a node with no row
            // places nobody — §6.6 creates a row by a registration — and is just as silent.
            Local::Heartbeat => {
                if self.inner.online.beat(from, now) {
                    // §6.7's recovery: a beat is being heard from, so a judgement clears and the
                    // transition is named for what was heard.
                    if self.inner.online.clear_judged(from).is_some() {
                        self.fire(&Transition::Recovered {
                            peer: from.to_string(),
                            method: RecoverMethod::Heartbeat,
                        });
                    }
                    Ok(LocalReply::Beat {
                        node_id: from.to_string(),
                    })
                } else {
                    Ok(LocalReply::Unplaced {
                        node_id: from.to_string(),
                    })
                }
            }
            // §7: the chain's digest, taken as this node's latest report. A statement, not a
            // question, so nothing is answered back — the same reason a beat is silent.
            Local::Digest { chain, length } => {
                let digest = ChainDigest {
                    chain: chain.clone(),
                    length: *length,
                };
                self.inner
                    .digests
                    .lock()
                    .expect("the digest table is not poisoned")
                    .insert(from.to_string(), digest);
                Ok(LocalReply::DigestTaken {
                    node_id: from.to_string(),
                    length: *length,
                })
            }
            // §7: a key event, taken the moment it happened. A statement like a beat or a digest, so
            // nothing is answered back — and, unlike a digest, it is **kept**: the newest
            // [`KEY_EVENT_LOG`] events per node, deduplicated by `(action, at_ms)`, because a re-pushed
            // fact is the same fact.
            Local::KeyEvent {
                at_ms,
                action,
                detail,
            } => {
                let event = KeyEvent {
                    at_ms: *at_ms,
                    action: action.clone(),
                    detail: detail.clone(),
                };
                let mut logs = self
                    .inner
                    .key_events
                    .lock()
                    .expect("the key-event log is not poisoned");
                let log = logs.entry(from.to_string()).or_default();
                let seen = log
                    .iter()
                    .any(|held| held.action == event.action && held.at_ms == event.at_ms);
                if !seen {
                    log.push_back(event);
                    while log.len() > KEY_EVENT_LOG {
                        log.pop_front();
                    }
                }
                Ok(LocalReply::KeyEventTaken {
                    node_id: from.to_string(),
                    action: action.clone(),
                })
            }
            // §6.7: a prober's view, recorded as a witness report about `node_id`, and then the
            // threshold re-read. The report is a **view**, so the latest one replaces an earlier
            // one from the same witness.
            Local::UnreachableReport { node_id } => {
                self.inner.witnesses.record(from, node_id, false, now);
                self.judge(node_id, now);
                Ok(LocalReply::UnreachableReported {
                    peer: node_id.clone(),
                })
            }
            // A report of life is a veto, and — when the subject had been judged — the recovery:
            // the node is heard from, so the judgement clears and the row reads `online` again.
            Local::ReachableReport { node_id } => {
                self.inner.witnesses.record(from, node_id, true, now);
                if self.inner.online.clear_judged(node_id).is_some() {
                    self.inner.online.heard_from(node_id, now);
                    self.fire(&Transition::Recovered {
                        peer: node_id.clone(),
                        method: RecoverMethod::Probe,
                    });
                }
                Ok(LocalReply::ReachableReported {
                    peer: node_id.clone(),
                })
            }
            Local::Addresses { node_id } => {
                let addresses = self.addresses_for(node_id);
                self.send_to(from, address_answer_body(&addresses))?;
                Ok(LocalReply::Addresses { addresses })
            }
            Local::Registry => {
                let registry = self.registry();
                let generation = registry.generation();
                self.send_to(from, registry.to_body())?;
                Ok(LocalReply::Registry { generation })
            }
        }
    }

    /// Serve one connection to its end: read frames, route each, and bind the socket to
    /// whoever authenticated on it.
    ///
    /// The read has **no timeout** — a session is idle between messages by design, and a
    /// session that timed out while quiet could never be reached (§6.3: the server waits
    /// to be dialled). A frame that is refused does not end the connection: the peer may
    /// have others to send.
    pub fn serve_connection(&self, mut connection: Connection) {
        let _ = connection.set_read_timeout(None);
        let mut bound: Option<(String, u64)> = None;
        // A closed connection, or a half-written line, ends the session: the loop's
        // condition is the read.
        while let Ok(frame) = connection.receive_frame() {
            let now = now_ms();
            let routed = match self.route(&frame, now) {
                Ok(routed) => routed,
                Err(_) => continue,
            };
            if bound.is_none() {
                if let Ok(writer) = connection.writer_clone() {
                    let node_id = routed.from().to_string();
                    let id = self
                        .inner
                        .sessions
                        .bind(&node_id, writer, connection.peer());
                    bound = Some((node_id, id));
                }
            }
            // The local roles answer **after** the bind: an answer goes down the session, and
            // a session that is not bound yet is a session with nowhere to put it.
            if let Routed::Local { from, action, .. } = &routed {
                let _ = self.answer_local(from, action, now);
            }
        }
        if let Some((node_id, id)) = bound {
            self.inner.sessions.unbind(&node_id, id);
        }
    }

    /// Accept connections and serve each on its own thread, until the listener fails.
    pub fn serve(&self, listener: Listener) -> Result<(), TransportError> {
        loop {
            let connection = listener.accept()?;
            let server = self.clone();
            std::thread::spawn(move || server.serve_connection(connection));
        }
    }
}

/// A node's session with its cross-region server: the half §6.3 says is dialled **out**.
///
/// Opening one sends a [`hello_body`] frame addressed to the server, which is what puts
/// this node into the server's knowledge; from then on the server hands frames down this
/// socket. The two halves are separately locked, so a reader can sit in a read while a
/// send goes out behind it — a session that could only do one at a time would stall the
/// first node that had something to say while it was waiting to hear something.
pub struct RelaySession {
    reader: Mutex<Connection>,
    writer: Mutex<TcpStream>,
    node_id: String,
    key: NodeKey,
    server_node_id: String,
}

impl RelaySession {
    /// Dial the server and open a session with it.
    pub fn open(
        addr: &str,
        server_node_id: &str,
        key: &NodeKey,
        node_id: &str,
        config: TransportConfig,
    ) -> Result<Self, TransportError> {
        let mut connection = Connection::connect_with(addr, config)?;
        let hello = SignedMessage::sign(key, node_id, server_node_id, now_ms(), hello_body())
            .map_err(|e| TransportError::Malformed { why: e.to_string() })?;
        connection.send(&hello)?;
        let writer = connection.writer_clone()?;
        Ok(Self {
            reader: Mutex::new(connection),
            writer: Mutex::new(writer),
            node_id: node_id.to_string(),
            key: key.clone(),
            server_node_id: server_node_id.to_string(),
        })
    }

    /// The server this session is with.
    pub fn server_node_id(&self) -> &str {
        &self.server_node_id
    }

    /// Hand one already-serialised frame to the server.
    pub fn send_frame(&self, frame: &str) -> Result<(), TransportError> {
        let bytes = frame_bytes(frame);
        let mut writer = self.writer.lock().expect("session writer");
        writer
            .write_all(&bytes)
            .and_then(|()| writer.flush())
            .map_err(|e| TransportError::Io {
                op: Op::Write,
                why: e.to_string(),
            })
    }

    /// Hand one message to the server.
    pub fn send(&self, message: &SignedMessage) -> Result<(), TransportError> {
        let frame = message
            .to_line()
            .map_err(|e| TransportError::Malformed { why: e.to_string() })?;
        self.send_frame(&frame)
    }

    /// Ask the server where `node_id` can be reached (§6.2's signalling).
    ///
    /// The answer arrives like anything else the server hands down: [`Self::receive`].
    pub fn query_addresses(&self, node_id: &str) -> Result<(), TransportError> {
        self.ask(address_query_body(node_id))
    }

    /// Ask the server to publish its registry (§6.2's management).
    pub fn request_registry(&self) -> Result<(), TransportError> {
        self.ask(crate::registry::registry_request_body())
    }

    /// Register with the server (§6.6).
    ///
    /// The acknowledgement the server answers with is left on the socket for whoever reads next —
    /// this call is the *sending* half, like every other question here.
    pub fn register(&self, claims: &Registration) -> Result<(), TransportError> {
        self.ask(register_body(
            &claims.addresses,
            &claims.capabilities,
            &claims.rooms,
        ))
    }

    /// Beat (§6.6). The server answers nothing: a heartbeat is a statement, not a question.
    pub fn heartbeat(&self) -> Result<(), TransportError> {
        self.ask(heartbeat_body())
    }

    /// Report this node's chain digest to the server (§7, v1.0 M4e-1). A statement, like a beat:
    /// nothing comes back.
    pub fn digest(&self, digest: &ChainDigest) -> Result<(), TransportError> {
        self.ask(digest.to_body())
    }

    /// Push a **key event** the moment it happens (§7, v1.0 M4e-2). A statement like a digest:
    /// nothing comes back.
    pub fn key_event(&self, event: &KeyEvent) -> Result<(), TransportError> {
        self.ask(event.to_body())
    }

    /// Ask a peer whether it is alive (§6.7): an ordinary §3 frame addressed to the peer.
    pub fn probe(&self, peer: &str) -> Result<(), TransportError> {
        self.send_body_to(peer, crate::liveness::probe_body())
    }

    /// Tell a **peer** what this node's own probe sees (v1.0 M5-3b-1): the same [`Report`] body §6.7 sends
    /// upward, addressed **sideways**. The centre is the node that is not answering, so its peers are the
    /// only ones who can be told about it — and §33's suppression reads their reports locally.
    pub fn report_to(&self, peer: &str, report: &Report) -> Result<(), TransportError> {
        self.send_body_to(peer, report.to_body())
    }

    /// Announce a **takeover** to a peer (v1.0 M5-3b-2): what §33's third layer reacts to.
    pub fn takeover_to(
        &self,
        peer: &str,
        centre: &str,
        by: &str,
        at_ms: i64,
    ) -> Result<(), TransportError> {
        self.send_body_to(peer, crate::suppression::takeover_body(centre, by, at_ms))
    }

    /// Hand a peer one event of a closed segment (v1.0 M5-3c-2).
    pub fn segment_event_to(
        &self,
        peer: &str,
        event: &crate::suppression::SegmentEvent,
    ) -> Result<(), TransportError> {
        self.send_body_to(peer, crate::suppression::segment_event_body(event))
    }

    /// Tell a peer a segment's stream is complete (v1.0 M5-3c-2).
    pub fn segment_done_to(
        &self,
        peer: &str,
        done: &crate::suppression::SegmentDone,
    ) -> Result<(), TransportError> {
        self.send_body_to(peer, crate::suppression::segment_done_body(done))
    }

    /// Hand a **peer** one task (v1.0 M6-1a). §14.6: the cross-device protocol, no protocol of M's own.
    pub fn task_to(&self, peer: &str, task: &crate::task::TaskFrame) -> Result<(), TransportError> {
        self.send_body_to(peer, crate::task::task_body(task))
    }

    /// Answer a peer's task (v1.0 M6-1a).
    pub fn task_reply_to(
        &self,
        peer: &str,
        reply: &crate::task::TaskReply,
    ) -> Result<(), TransportError> {
        self.send_body_to(peer, crate::task::task_reply_body(reply))
    }

    /// Answer a probe (§6.7): the smallest evidence that the node behind this session works.
    pub fn answer_alive(&self, to: &str) -> Result<(), TransportError> {
        self.send_body_to(to, crate::liveness::alive_body())
    }

    /// Report a peer's reachability to the server (§6.7).
    pub fn report(&self, report: &Report) -> Result<(), TransportError> {
        self.send_body_to(&self.server_node_id, report.to_body())
    }

    /// Sign one question to the server and hand it over.
    ///
    /// A question is an ordinary §3 frame addressed to the server itself, so the server
    /// authenticates it exactly as it authenticates every other frame: no new credential, and
    /// nothing the capability model has to answer for.
    fn ask(&self, body: Value) -> Result<(), TransportError> {
        self.send_body_to(&self.server_node_id, body)
    }

    /// Sign `body` to `to` and hand it over — the general form [`Self::ask`] specialises, and
    /// what §6.7's probes, answers and reports are built from.
    fn send_body_to(&self, to: &str, body: Value) -> Result<(), TransportError> {
        let message = SignedMessage::sign(&self.key, &self.node_id, to, now_ms(), body)
            .map_err(|e| TransportError::Malformed { why: e.to_string() })?;
        self.send(&message)
    }

    /// Wait for one frame the server pushed down this session.
    ///
    /// `Ok(None)` is the read timeout — nothing yet, which on an idle session is the
    /// ordinary case. Anything else is the socket's answer, and ends the session.
    pub fn receive(&self) -> Result<Option<SignedMessage>, TransportError> {
        match self.reader.lock().expect("session reader").receive() {
            Ok(message) => Ok(Some(message)),
            Err(TransportError::Timeout { .. }) => Ok(None),
            Err(error) => Err(error),
        }
    }
}

/// The node's [`Relay`], wired to one cross-region server.
///
/// §6.4 makes the server a peer and says the node's settings name which peer it is; this
/// is the client end of that. It keeps **one** session and reuses it, and drops it on a
/// failure so the next frame dials again — a node that has a server is expected to keep
/// the connection a server can hand frames down.
pub struct RelayClient {
    node_id: String,
    key: NodeKey,
    server: PeerEntry,
    config: TransportConfig,
    session: Mutex<Option<RelaySession>>,
}

impl RelayClient {
    /// A client for `server`, which must be a peer entry carrying a usable public key.
    pub fn new(
        node_id: &str,
        key: NodeKey,
        server: &PeerEntry,
        config: TransportConfig,
    ) -> Result<Self, PeersError> {
        // The server is a peer like any other: its entry is held to the same rule.
        server.verifying_key()?;
        Ok(Self {
            node_id: node_id.to_string(),
            key,
            server: server.clone(),
            config,
            session: Mutex::new(None),
        })
    }

    /// The client for the peer `server_node_id` (§6.4: the settings name one, and its key
    /// and addresses are that entry's).
    pub fn for_server(
        node_id: &str,
        key: NodeKey,
        peers: &PeersFile,
        server_node_id: &str,
        config: TransportConfig,
    ) -> Result<Self, PeersError> {
        let entry = peers.entry(server_node_id).ok_or_else(|| {
            PeersError::Entry(format!("{server_node_id} is not in the peer table"))
        })?;
        Self::new(node_id, key, entry, config)
    }

    /// The server's `node_id`.
    pub fn server_node_id(&self) -> &str {
        &self.server.node_id
    }

    /// The addresses the session is dialled at.
    pub fn server_addresses(&self) -> &[String] {
        &self.server.addresses
    }

    /// The server's own public key, as a [`PeerKeys`] a node verifies its **answers** with.
    ///
    /// §6.4 makes the server a peer, so verifying what it publishes is §3's ordinary check
    /// against its `peers.json` entry — the same entry that pointed the node at it.
    pub fn server_keys(&self) -> Result<PeerKeys, PeersError> {
        let mut keys = PeerKeys::new();
        keys.insert(&self.server.node_id, [self.server.verifying_key()?]);
        Ok(keys)
    }

    /// Ask the server where `node_id` can be reached (§6.2's signalling).
    ///
    /// The answer comes back from [`Self::receive`]: a question and its answer are two
    /// frames, and the answer is the server's to sign.
    pub fn query_addresses(&self, node_id: &str) -> Result<(), TransportError> {
        self.with_session(|session| session.query_addresses(node_id))
    }

    /// Ask the server to publish its registry (§6.2's management).
    pub fn request_registry(&self) -> Result<(), TransportError> {
        self.with_session(|session| session.request_registry())
    }

    /// Open the session if it is not open, and say so when it cannot be.
    ///
    /// A client is built **without dialling** (v1.0 V-2): the pointer in a node's settings makes one,
    /// and this is the call — or the first [`Relay::forward`] — that reaches the server. Idempotent.
    pub fn connect(&self) -> Result<(), TransportError> {
        self.with_session(|_| Ok(()))
    }

    /// Register with the server (§6.6).
    pub fn register(&self, claims: &Registration) -> Result<(), TransportError> {
        self.with_session(|session| session.register(claims))
    }

    /// Beat (§6.6).
    pub fn heartbeat(&self) -> Result<(), TransportError> {
        self.with_session(|session| session.heartbeat())
    }

    /// Report this node's chain digest to the server (§7), opening the session if needed.
    pub fn digest(&self, digest: &ChainDigest) -> Result<(), TransportError> {
        self.with_session(|session| session.digest(digest))
    }

    /// Push a **key event** the moment it happens (§7, v1.0 M4e-2), opening the session if needed.
    pub fn key_event(&self, event: &KeyEvent) -> Result<(), TransportError> {
        self.with_session(|session| session.key_event(event))
    }

    /// Probe a peer (§6.7), opening the session if needed.
    pub fn probe(&self, peer: &str) -> Result<(), TransportError> {
        self.with_session(|session| session.probe(peer))
    }

    /// Tell a **peer** what this node's own probe sees (v1.0 M5-3b-1).
    pub fn report_to(&self, peer: &str, report: &Report) -> Result<(), TransportError> {
        self.with_session(|session| session.report_to(peer, report))
    }

    /// Announce a **takeover** to a peer (v1.0 M5-3b-2).
    pub fn takeover_to(
        &self,
        peer: &str,
        centre: &str,
        by: &str,
        at_ms: i64,
    ) -> Result<(), TransportError> {
        self.with_session(|session| session.takeover_to(peer, centre, by, at_ms))
    }

    /// Hand a peer one event of a closed segment (v1.0 M5-3c-2).
    pub fn segment_event_to(
        &self,
        peer: &str,
        event: &crate::suppression::SegmentEvent,
    ) -> Result<(), TransportError> {
        self.with_session(|session| session.segment_event_to(peer, event))
    }

    /// Tell a peer a segment's stream is complete (v1.0 M5-3c-2).
    pub fn segment_done_to(
        &self,
        peer: &str,
        done: &crate::suppression::SegmentDone,
    ) -> Result<(), TransportError> {
        self.with_session(|session| session.segment_done_to(peer, done))
    }

    /// Hand a **peer** one task (v1.0 M6-1a), opening the session if needed.
    pub fn task_to(&self, peer: &str, task: &crate::task::TaskFrame) -> Result<(), TransportError> {
        self.with_session(|session| session.task_to(peer, task))
    }

    /// Answer a peer's task (v1.0 M6-1a), opening the session if needed.
    pub fn task_reply_to(
        &self,
        peer: &str,
        reply: &crate::task::TaskReply,
    ) -> Result<(), TransportError> {
        self.with_session(|session| session.task_reply_to(peer, reply))
    }

    /// Answer a probe (§6.7).
    pub fn answer_alive(&self, to: &str) -> Result<(), TransportError> {
        self.with_session(|session| session.answer_alive(to))
    }

    /// Report a peer's reachability to the server (§6.7).
    pub fn report(&self, report: &Report) -> Result<(), TransportError> {
        self.with_session(|session| session.report(report))
    }

    /// Hand one already-serialised frame to the server, opening the session if needed.
    pub fn send_frame(&self, frame: &str) -> Result<(), TransportError> {
        self.with_session(|session| session.send_frame(frame))
    }

    /// Hand one message to the server.
    pub fn send(&self, message: &SignedMessage) -> Result<(), TransportError> {
        self.with_session(|session| session.send(message))
    }

    /// Wait for one frame the server pushed down. Opens the session if needed.
    pub fn receive(&self) -> Result<Option<SignedMessage>, TransportError> {
        self.with_session(|session| session.receive())
    }

    /// Is a session open right now?
    pub fn is_connected(&self) -> bool {
        self.session.lock().expect("relay session").is_some()
    }

    /// Drop the session, so the next call dials again.
    pub fn disconnect(&self) {
        *self.session.lock().expect("relay session") = None;
    }

    fn with_session<T>(
        &self,
        work: impl FnOnce(&RelaySession) -> Result<T, TransportError>,
    ) -> Result<T, TransportError> {
        let mut slot = self.session.lock().expect("relay session");
        if slot.is_none() {
            *slot = Some(self.open_session()?);
        }
        let session = slot.as_ref().expect("just opened");
        match work(session) {
            Ok(value) => Ok(value),
            // A session that failed is not a session: forget it so the next call dials.
            Err(error) => {
                *slot = None;
                Err(error)
            }
        }
    }

    fn open_session(&self) -> Result<RelaySession, TransportError> {
        let mut last = TransportError::Io {
            op: Op::Connect,
            why: format!("{} carries no address", self.server.node_id),
        };
        for address in &self.server.addresses {
            match RelaySession::open(
                address,
                &self.server.node_id,
                &self.key,
                &self.node_id,
                self.config,
            ) {
                Ok(session) => return Ok(session),
                Err(error) => last = error,
            }
        }
        Err(last)
    }
}

impl Relay for RelayClient {
    /// §3.1's relay leg: the frame that failed the direct path, in the bytes the direct
    /// path would have written.
    fn forward(&self, frame: &str) -> Result<(), TransportError> {
        self.send_frame(frame)
    }

    fn describe(&self) -> String {
        format!("the cross-region server {}", self.server.node_id)
    }
}

/// The client for the peer the settings named as the cross-region server (§6.4).
///
/// A node with no server configured has no wide-area lane, and that is the honest state:
/// [`NoRelay`](crate::transport::NoRelay) is what a caller wires when the settings name
/// nobody.
pub fn client_for_server(
    node_id: &str,
    key: NodeKey,
    peers: &PeersFile,
    server_node_id: &str,
    config: TransportConfig,
) -> Result<RelayClient, PeersError> {
    RelayClient::for_server(node_id, key, peers, server_node_id, config)
}

/// What a server answered a node (§6.2's signalling and management).
#[derive(Debug, Clone, PartialEq)]
pub enum Answer {
    /// Where a node can be reached — **addresses, and nothing else** (§6.2).
    Addresses { addresses: Vec<String> },
    /// The registry and the room definitions as they stood.
    Registry(Registry),
}

impl Answer {
    /// Read an answer out of a message that **already passed verification** at the node.
    ///
    /// `server_node_id` is the node's own cross-region server, and it is required: only that
    /// peer answers, so a relayed frame that happens to carry an `addresses` member is not
    /// mistaken for one. `Ok(None)` is then "not an answer" — traffic the node should read
    /// again rather than fail on.
    pub fn from_verified(
        message: &VerifiedMessage,
        server_node_id: &str,
    ) -> Result<Option<Answer>, RegistryError> {
        if message.from != server_node_id {
            return Ok(None);
        }
        if let Some(addresses) = answered_addresses(&message.body) {
            return Ok(Some(Answer::Addresses { addresses }));
        }
        // A registry is anything else that carries a table; the shared shape is the test.
        if message.body.get("generation").is_none() {
            return Ok(None);
        }
        Registry::from_verified(message).map(|registry| Some(Answer::Registry(registry)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::SignedMessage;
    use crate::peers::PeerEntry;

    const NOW: i64 = 1_700_000_000_000;

    fn entry(node_id: &str, key: &NodeKey, address: &str) -> PeerEntry {
        PeerEntry::new(node_id, address, key.public_jwk())
    }

    /// A forwarder that remembers what it was handed, and can be told to refuse.
    #[derive(Default)]
    struct Recording {
        handed: Mutex<Vec<(String, String)>>,
        present: Vec<String>,
    }

    impl Forwarder for Recording {
        fn forward_to(&self, node_id: &str, frame: &str) -> Result<(), RelayError> {
            if !self.present.iter().any(|known| known == node_id) {
                return Err(RelayError::NoSession(node_id.to_string()));
            }
            self.handed
                .lock()
                .expect("handed")
                .push((node_id.to_string(), frame.to_string()));
            Ok(())
        }
    }

    fn state() -> (NodeKey, NodeKey, PeersFile, PeerKeys) {
        let sender = NodeKey::generate().expect("key");
        let destination = NodeKey::generate().expect("key");
        let mut peers = PeersFile::empty();
        peers.peers.push(entry("dev-a", &sender, "127.0.0.1:1"));
        peers
            .peers
            .push(entry("dev-b", &destination, "127.0.0.1:2"));
        let keys = peers.peer_keys().expect("keys");
        (sender, destination, peers, keys)
    }

    #[test]
    fn a_frame_to_a_known_destination_is_handed_on_byte_for_byte() {
        let (sender, _destination, peers, keys) = state();
        let message =
            SignedMessage::sign(&sender, "dev-a", "dev-b", NOW, serde_json::json!({"hi": 1}))
                .expect("sign");
        let frame = message.to_line().expect("line");
        let forwarder = Recording {
            handed: Mutex::new(Vec::new()),
            present: vec!["dev-b".to_string()],
        };
        let routed = route(
            &frame,
            "server",
            &peers,
            &keys,
            &mut ReplayGuard::new(),
            &forwarder,
            NOW,
        )
        .expect("routed");
        assert_eq!(
            routed,
            Routed::Forwarded {
                from: "dev-a".to_string(),
                to: "dev-b".to_string()
            }
        );
        let handed = forwarder.handed.lock().expect("handed");
        assert_eq!(handed.len(), 1);
        assert_eq!(handed[0].0, "dev-b");
        assert_eq!(
            handed[0].1, frame,
            "the relay hands on the bytes it received"
        );
    }

    #[test]
    fn a_frame_addressed_to_the_server_is_local_and_never_forwarded() {
        let (sender, _destination, peers, keys) = state();
        let message =
            SignedMessage::sign(&sender, "dev-a", "server", NOW, hello_body()).expect("sign");
        let frame = message.to_line().expect("line");
        let forwarder = Recording {
            handed: Mutex::new(Vec::new()),
            present: vec!["dev-b".to_string()],
        };
        let routed = route(
            &frame,
            "server",
            &peers,
            &keys,
            &mut ReplayGuard::new(),
            &forwarder,
            NOW,
        )
        .expect("routed");
        assert!(matches!(routed, Routed::Local { .. }));
        assert!(!routed.was_forwarded());
        assert!(forwarder.handed.lock().expect("handed").is_empty());
        assert!(is_hello(&hello_body()));
    }

    #[test]
    fn an_unknown_sender_is_refused_as_a_peer() {
        let (_sender, destination, peers, keys) = state();
        let stranger = NodeKey::generate().expect("key");
        let message =
            SignedMessage::sign(&stranger, "dev-x", "dev-b", NOW, serde_json::json!(null))
                .expect("sign");
        let frame = message.to_line().expect("line");
        let error = route(
            &frame,
            "server",
            &peers,
            &keys,
            &mut ReplayGuard::new(),
            &Recording::default(),
            NOW,
        )
        .expect_err("unknown sender");
        assert_eq!(error.category(), Category::Refused);
        let _ = destination;
    }

    #[test]
    fn an_unknown_destination_is_refused_rather_than_broadcast() {
        let (sender, _destination, peers, keys) = state();
        let message = SignedMessage::sign(&sender, "dev-a", "dev-z", NOW, serde_json::json!(null))
            .expect("sign");
        let frame = message.to_line().expect("line");
        let error = route(
            &frame,
            "server",
            &peers,
            &keys,
            &mut ReplayGuard::new(),
            &Recording::default(),
            NOW,
        )
        .expect_err("unknown destination");
        assert!(matches!(error, RelayError::UnknownDestination(_)));
        assert_eq!(error.category(), Category::Refused);
    }

    #[test]
    fn a_known_destination_that_is_not_dialled_in_is_refused() {
        let (sender, _destination, peers, keys) = state();
        let message = SignedMessage::sign(&sender, "dev-a", "dev-b", NOW, serde_json::json!(null))
            .expect("sign");
        let frame = message.to_line().expect("line");
        // `Recording::default()` has nobody present.
        let error = route(
            &frame,
            "server",
            &peers,
            &keys,
            &mut ReplayGuard::new(),
            &Recording::default(),
            NOW,
        )
        .expect_err("no session");
        assert!(matches!(error, RelayError::NoSession(_)));
    }

    #[test]
    fn the_same_frame_twice_is_a_replay_at_the_server() {
        let (sender, _destination, peers, keys) = state();
        let message =
            SignedMessage::sign(&sender, "dev-a", "dev-b", NOW, serde_json::json!({"n": 1}))
                .expect("sign");
        let frame = message.to_line().expect("line");
        let forwarder = Recording {
            handed: Mutex::new(Vec::new()),
            present: vec!["dev-b".to_string()],
        };
        let mut guard = ReplayGuard::new();
        assert!(route(&frame, "server", &peers, &keys, &mut guard, &forwarder, NOW).is_ok());
        let error = route(&frame, "server", &peers, &keys, &mut guard, &forwarder, NOW)
            .expect_err("replay");
        assert!(matches!(
            error,
            RelayError::Refused(VerifyError::Replay { .. })
        ));
        assert_eq!(forwarder.handed.lock().expect("handed").len(), 1);
    }

    #[test]
    fn a_line_that_is_not_a_message_is_invalid() {
        let (_sender, _destination, peers, keys) = state();
        let error = route(
            "not json at all",
            "server",
            &peers,
            &keys,
            &mut ReplayGuard::new(),
            &Recording::default(),
            NOW,
        )
        .expect_err("malformed");
        assert!(matches!(error, RelayError::Malformed(_)));
        assert_eq!(error.category(), Category::Invalid);
    }

    #[test]
    fn the_body_is_never_opened() {
        // A body that is not a table, an announcement or a hello: the relay carries it
        // because routing reads the preamble and nothing else.
        let (sender, _destination, peers, keys) = state();
        let message = SignedMessage::sign(
            &sender,
            "dev-a",
            "dev-b",
            NOW,
            serde_json::json!({"opaque": [1, 2, {"deep": true}]}),
        )
        .expect("sign");
        let frame = message.to_line().expect("line");
        let forwarder = Recording {
            handed: Mutex::new(Vec::new()),
            present: vec!["dev-b".to_string()],
        };
        route(
            &frame,
            "server",
            &peers,
            &keys,
            &mut ReplayGuard::new(),
            &forwarder,
            NOW,
        )
        .expect("routed");
        assert_eq!(forwarder.handed.lock().expect("handed")[0].1, frame);
    }

    #[test]
    fn a_session_opener_does_not_consume_the_sender_s_record() {
        // A session is opened *after* the frame it is about to carry was signed, so a
        // hello that advanced the record would make the relay refuse the very frame it
        // was handed — for being older than the hello that carried it there.
        let (sender, _destination, peers, keys) = state();
        let mut guard = ReplayGuard::new();
        let hello =
            SignedMessage::sign(&sender, "dev-a", "server", NOW, hello_body()).expect("sign");
        route(
            &hello.to_line().expect("line"),
            "server",
            &peers,
            &keys,
            &mut guard,
            &Recording::default(),
            NOW,
        )
        .expect("hello");
        assert_eq!(guard.tracked_peers(), 0, "the hello left a record");

        // Signed a second *before* the hello, and still carried.
        let message = SignedMessage::sign(
            &sender,
            "dev-a",
            "dev-b",
            NOW - 1_000,
            serde_json::json!({ "n": 1 }),
        )
        .expect("sign");
        let forwarder = Recording {
            handed: Mutex::new(Vec::new()),
            present: vec!["dev-b".to_string()],
        };
        route(
            &message.to_line().expect("line"),
            "server",
            &peers,
            &keys,
            &mut guard,
            &forwarder,
            NOW,
        )
        .expect("carried");
        assert_eq!(forwarder.handed.lock().expect("handed").len(), 1);
    }

    #[test]
    fn the_local_branch_tells_the_three_questions_apart() {
        assert_eq!(Local::of(&hello_body()), Local::Hello);
        assert_eq!(
            Local::of(&address_query_body("dev-b")),
            Local::Addresses {
                node_id: "dev-b".to_string()
            }
        );
        assert_eq!(
            Local::of(&crate::registry::registry_request_body()),
            Local::Registry
        );
        assert!(matches!(
            Local::of(&serde_json::json!({ "what": "ever" })),
            Local::Unrecognised { .. }
        ));
        // The query shape reads back both ways, and is not confused with anything else.
        assert_eq!(address_query(&address_query_body("dev-c")), Some("dev-c"));
        assert_eq!(address_query(&hello_body()), None);
    }

    #[test]
    fn an_address_answer_carries_addresses_and_nothing_else() {
        let body = address_answer_body(&["127.0.0.1:1".to_string()]);
        assert_eq!(
            answered_addresses(&body),
            Some(vec!["127.0.0.1:1".to_string()])
        );
        // §6.2: signalling knows addresses, never payloads — one member, and no room for
        // anything else to ride along.
        assert_eq!(body.as_object().expect("object").len(), 1);
        assert_eq!(answered_addresses(&serde_json::json!({})), None);
    }

    #[test]
    fn a_frame_for_the_server_is_local_and_carries_what_was_asked() {
        let (sender, _destination, peers, keys) = state();
        let message =
            SignedMessage::sign(&sender, "dev-a", "server", NOW, address_query_body("dev-b"))
                .expect("sign");
        let routed = route(
            &message.to_line().expect("line"),
            "server",
            &peers,
            &keys,
            &mut ReplayGuard::new(),
            &Recording::default(),
            NOW,
        )
        .expect("routed");
        assert_eq!(
            routed.local(),
            Some(&Local::Addresses {
                node_id: "dev-b".to_string()
            })
        );
        assert!(!routed.was_forwarded());
    }

    #[test]
    fn a_digest_body_round_trips_and_is_read_as_one() {
        // The digest reports a **point**: a head hash and a length (§7).
        let body = digest_body(Some("abc"), 7);
        assert!(is_digest(&body));
        assert_eq!(
            ChainDigest::of_body(&body),
            Some(ChainDigest {
                chain: Some("abc".to_string()),
                length: 7
            })
        );
        // An empty chain reports both halves honestly: no head, and a length of zero.
        assert_eq!(
            ChainDigest::of_body(&digest_body(None, 0)),
            Some(ChainDigest {
                chain: None,
                length: 0
            })
        );
        // It is not confused with anything else on the wire, and it is read as its own local frame.
        assert_eq!(ChainDigest::of_body(&heartbeat_body()), None);
        assert_eq!(ChainDigest::of_body(&hello_body()), None);
        assert_eq!(
            Local::of(&body),
            Local::Digest {
                chain: Some("abc".to_string()),
                length: 7
            }
        );
    }

    #[test]
    fn a_session_table_retires_by_id() {
        let table = SessionTable::new();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("addr");
        let first = TcpStream::connect(addr).expect("connect");
        let second = TcpStream::connect(addr).expect("connect");
        let what = "127.0.0.1:9001".parse::<SocketAddr>().expect("addr");
        let old = table.bind("dev-a", first, what);
        let new = table.bind("dev-a", second, what);
        assert!(table.is_present("dev-a"));
        assert_eq!(table.address_of("dev-a"), Some(what));
        assert_eq!(table.len(), 1);
        // The older connection's thread must not evict the session that replaced it.
        assert!(!table.unbind("dev-a", old));
        assert!(table.is_present("dev-a"));
        assert!(table.unbind("dev-a", new));
        assert!(table.is_empty());
        assert_eq!(table.address_of("dev-a"), None);
    }
}
