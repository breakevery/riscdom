//! The cross-region server: the relay, and the session both sides dial out to (v1.0 M4d).
//!
//! [connection.md §6](../../docs/connection.md) freezes what this is: **a dedicated
//! deployment of the same software, run by a deployer**, that carries the traffic two
//! nodes cannot carry themselves. This module implements the **relay** role and the
//! session it needs. Signalling and management — the address query and the publishing
//! half — are the next piece; audit aggregation is §7's, and waits on M5.
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
//! itself (a signalling query, the [`hello_body`] a session opens with) or is handed
//! down the destination's session, **only if the server knows that destination**; an
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
//! **The wire syntax is this batch's, and §6 left it so.** §6 freezes the roles and the
//! routing rule, not the bytes; the one thing invented here is [`hello_body`], the frame
//! a node opens its session with. It is recorded as such in [decisions §98](../../docs/decisions.md).
//!
//! **Dependency direction, and what is deliberately absent.** Relaying is a transport
//! concern and the authorisation is §3's, so nothing here names a capability, writes an
//! audit row, or reaches above `net`. [`NoRelay`](crate::transport::NoRelay) stays the
//! honest answer for a deployment that configures no server.

use crate::error::Category;
use crate::identity::NodeKey;
use crate::message::{body_hash, now_ms, SignedMessage, PROTOCOL_VERSION};
use crate::peers::{PeerEntry, PeersError, PeersFile};
use crate::replay::{ReplayError, ReplayGuard};
use crate::sign::{authenticate_forwarded, check_identity, PeerKeys, VerifyError};
use crate::transport::{
    frame_bytes, Connection, Listener, Op, Relay, TransportConfig, TransportError,
};
use serde_json::Value;
use std::collections::HashMap;
use std::io::Write;
use std::net::TcpStream;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};

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

/// What the server did with one frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Routed {
    /// Addressed to the server itself — the signalling and management roles' (§6.2).
    Local { from: String, to: String },
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
    writer: Mutex<TcpStream>,
}

impl SessionTable {
    /// Nobody is dialled in.
    pub fn new() -> Self {
        Self::default()
    }

    /// Bind `node_id` to `writer`, replacing whatever session it had. The returned id is
    /// what that connection retires itself with.
    pub fn bind(&self, node_id: &str, writer: TcpStream) -> u64 {
        let id = self.inner.next_id.fetch_add(1, Ordering::Relaxed);
        let live = Arc::new(Live {
            id,
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
        if !is_hello(&message.body) {
            guard.accept(&message.from, message.ts, &body_hash(&message.body), now)?;
        }
        return Ok(Routed::Local {
            from: message.from,
            to: message.to,
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
    config: TransportConfig,
    peers: RwLock<PeersFile>,
    keys: RwLock<PeerKeys>,
    guard: Mutex<ReplayGuard>,
    sessions: SessionTable,
}

impl RelayServer {
    /// A server that knows `peers` — the same entry shape every node's table uses, which
    /// is what makes the server a peer (§6.4).
    pub fn new(
        node_id: &str,
        peers: PeersFile,
        config: TransportConfig,
    ) -> Result<Self, PeersError> {
        let keys = peers.peer_keys()?;
        Ok(Self {
            inner: Arc::new(ServerInner {
                node_id: node_id.to_string(),
                config,
                peers: RwLock::new(peers),
                keys: RwLock::new(keys),
                guard: Mutex::new(ReplayGuard::new()),
                sessions: SessionTable::new(),
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

    /// The transport configuration sessions inherit.
    pub fn config(&self) -> TransportConfig {
        self.inner.config
    }

    /// The nodes it knows — its view, and the boundary of what it will forward to.
    pub fn peers(&self) -> PeersFile {
        self.inner.peers.read().expect("peers").clone()
    }

    /// Replace the view.
    ///
    /// This is where the **management** role lands when it is implemented (§6.2): a
    /// deployer republishing its registry rewrites the table the server routes against.
    /// The keys are re-derived here so a table can never disagree with the keys in force.
    pub fn set_peers(&self, peers: PeersFile) -> Result<(), PeersError> {
        let keys = peers.peer_keys()?;
        *self.inner.peers.write().expect("peers") = peers;
        *self.inner.keys.write().expect("keys") = keys;
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
            let routed = match self.route(&frame, now_ms()) {
                Ok(routed) => routed,
                Err(_) => continue,
            };
            if bound.is_none() {
                if let Ok(writer) = connection.writer_clone() {
                    let node_id = routed.from().to_string();
                    let id = self.inner.sessions.bind(&node_id, writer);
                    bound = Some((node_id, id));
                }
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
    fn the_session_table_retires_by_id() {
        let table = SessionTable::new();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("addr");
        let first = TcpStream::connect(addr).expect("connect");
        let second = TcpStream::connect(addr).expect("connect");
        let old = table.bind("dev-a", first);
        let new = table.bind("dev-a", second);
        assert!(table.is_present("dev-a"));
        assert_eq!(table.len(), 1);
        // The older connection's thread must not evict the session that replaced it.
        assert!(!table.unbind("dev-a", old));
        assert!(table.is_present("dev-a"));
        assert!(table.unbind("dev-a", new));
        assert!(table.is_empty());
    }
}
