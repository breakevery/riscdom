//! net — the connection layer (v1.0, roadmap §4).
//!
//! Layer two, as [`docs/connection.md`](../docs/connection.md) freezes it: a node's
//! **identity** (§2), **signing** (§3), **discovery** (§4), **rooms** (§5) and the
//! **cross-region server** (§6). This crate is where that protocol becomes code.
//!
//! **It is built in pieces, in the order the frozen document lays them out.** What is
//! here now:
//!
//! - **§2, the node's identity** — [`NodeKey`]: an Ed25519 key pair in
//!   `<data-dir>/node.key`, one JWK whose first member is `schema_version`, minted on
//!   the first start with networking configured and never minted by a read.
//! - **§3, signing** — [`SignedMessage`]: the five-member preamble
//!   `{v, from, to, ts, body}` signed over its canonical JSON, with `sig` beside it;
//!   [`verify`] runs the six steps in the frozen order and answers with a
//!   [`Category`] from the error model.
//! - **§3.2, replay protection** — [`ReplayGuard`]: a per-peer, in-memory high-water
//!   mark over a −5 min / +1 min window, where advancing the mark discards the set.
//! - **§3.1, the transport** — [`Connection`] / [`Listener`]: one JSON line per message
//!   over a TCP socket (std, no async runtime), sent **direct first** and, when that
//!   fails, through the [`Relay`] seam M4d fills in. The frame is serialised once, so the
//!   two paths carry identical bytes.
//! - **§4, discovery** — [`PeersFile`] (the local, authoritative peer table),
//!   [`NodeTable`] (what an in-network server hands down, merged as a **source** with
//!   conflicts reported), the UDP beacon ([`sign_announcement`] / [`receive_datagram`]) and
//!   [`RoomFilter`] — the default-deny filter that keeps a beacon from introducing a key.
//! - **§5, rooms** — [`RoomsFile`]: membership plus the three rules ([`RateRule`] and
//!   [`RateCounters`], [`Mention`], `require_signature`). [`RoomFilter::from_rooms`] is
//!   where §4's filter meets §5's file.
//! - **§6, the cross-region server** — [`RelayServer`] and [`route`]: a frame is
//!   authenticated (§3's model, and no new credential), routed on its signed `to`, and
//!   handed down the destination's session — [`SessionTable`] — while a frame addressed to
//!   the server itself is [`Local`]'s business: a hello, a **registration** and its
//!   **heartbeat** (§6.6, with [`OnlineTable`] on the server's side), an **address query**
//!   ([`address_query_body`], answered from what the server knows), or a **registry request**
//!   ([`registry_request_body`], answered with [`Registry`] — the table plus the room
//!   definitions, a **source and not an authority**). [`RelayClient`] and [`RelaySession`] are
//!   the node's half: the session **both sides dial out** (§6.3), which is why the server never
//!   dials and this project needs no hole punching.
//!
//! **Not here yet**: §7 — the **audit digests** a server aggregates on a timer — which waits
//! on M5's authorisation. Each piece lands only after the section it implements is frozen.
//!
//! **Dependency direction.** `net` depends on [`audit`] and nothing else in this
//! workspace. The chain's canonical JSON ([`audit::canonical_json`]) is what a
//! signature is computed over and what a key or payload fingerprint is taken of, so
//! there is one description of those bytes instead of two; and `host-core` is what will
//! depend on *this* crate, never the other way round — the workspace's direction is
//! `audit ← net ← host-core ← server`, so nothing here may reach upward.

mod discovery;
mod error;
mod identity;
mod liveness;
mod message;
mod peers;
mod registry;
mod relay;
mod replay;
mod rooms;
mod sign;
mod suppression;
mod transport;
mod versioned;

pub use discovery::{
    announced_entry, announced_rooms, announcement_body, consider_announcement, discovery_category,
    merge_table, receive_datagram, send_datagram, sign_announcement, Adoption, Conflict,
    DiscoveryError, MergeReport, NodeTable, RoomFilter, BROADCAST_PORT, MAX_DATAGRAM_BYTES,
};
pub use error::Category;
pub use identity::{NodeKey, NodeKeyError, NODE_KEY_FILE};
pub use liveness::{
    alive_body, is_alive, is_probe, probe_body, reachable_body, report_of, unreachable_body,
    Judgement, PeerView, Prober, RecoverMethod, Report, Transition, TransitionSink, WitnessTable,
    PROBE_INTERVAL, PROBE_MISSES, REPORT_WINDOW_MS,
};
pub use message::{
    body_hash, now_ms, MessageError, SignedMessage, PROTOCOL_VERSION, SIGNATURE_BYTES,
};
pub use peers::{
    peers_category, public_key_from_jwk, PeerEntry, PeersError, PeersFile, PEERS_FILE, SERVER_CLAIM,
};
pub use registry::{
    is_registry_request, registry_category, registry_request_body, Merged, Registry, RegistryError,
};
pub use relay::{
    address_answer_body, address_query, address_query_body, answered_addresses, client_for_server,
    digest_body, heartbeat_body, hello_body, is_digest, is_heartbeat, is_hello, is_register,
    is_registered, register_body, registered_body, route, Answer, ChainDigest, Forwarder, Local,
    LocalReply, Online, OnlineEntry, OnlineTable, Registration, RelayClient, RelayError,
    RelayServer, RelayServerError, RelaySession, Routed, SessionTable, DIGEST_INTERVAL,
    FIRST_GENERATION, HEARTBEAT_INTERVAL, ONLINE_WINDOW_MS,
};
pub use replay::{ReplayError, ReplayGuard, Window, REPLAY_WINDOW_AHEAD_MS, REPLAY_WINDOW_BACK_MS};
pub use rooms::{
    merge_rooms, rooms_category, Mention, RateCounters, RateError, RateRule, Room, RoomConflict,
    RoomMergeReport, RoomRules, RoomsError, RoomsFile, ROOMS_FILE,
};
pub use sign::{verify, verify_at, PeerKeys, VerifiedMessage, VerifyError};
pub use suppression::{
    backoff_delay_ms, first_in_line, is_first_in_line, Suppression, SuppressionPhase,
    SUPPRESSION_BACKOFF_MAX, SUPPRESSION_WAIT,
};
pub use transport::{
    deliver, frame_bytes, send_direct, Connection, Listener, NoRelay, Op, Path, Relay,
    TransportConfig, TransportError, DEFAULT_CONNECT_TIMEOUT, DEFAULT_MAX_FRAME_BYTES,
    DEFAULT_READ_TIMEOUT, DEFAULT_WRITE_TIMEOUT, FRAME_TERMINATOR,
};
pub use versioned::{
    save, save_new_private, Versioned, VersionedError, VersionedLoad, FIRST_SCHEMA_VERSION,
};
