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
//!
//! **Not here yet**: discovery (§4), rooms (§5) and the cross-region server (§6) —
//! including the relay's routing, which is why [`NoRelay`] exists. Each piece lands only
//! after the section it implements is frozen.
//!
//! **Dependency direction.** `net` depends on [`audit`] and nothing else in this
//! workspace. The chain's canonical JSON ([`audit::canonical_json`]) is what a
//! signature is computed over and what a key or payload fingerprint is taken of, so
//! there is one description of those bytes instead of two; and `host-core` is what will
//! depend on *this* crate, never the other way round — the workspace's direction is
//! `audit ← net ← host-core ← server`, so nothing here may reach upward.

mod error;
mod identity;
mod message;
mod replay;
mod sign;
mod transport;
mod versioned;

pub use error::Category;
pub use identity::{NodeKey, NodeKeyError, NODE_KEY_FILE};
pub use message::{
    body_hash, now_ms, MessageError, SignedMessage, PROTOCOL_VERSION, SIGNATURE_BYTES,
};
pub use replay::{ReplayError, ReplayGuard, Window, REPLAY_WINDOW_AHEAD_MS, REPLAY_WINDOW_BACK_MS};
pub use sign::{verify, verify_at, PeerKeys, VerifiedMessage, VerifyError};
pub use transport::{
    deliver, send_direct, Connection, Listener, NoRelay, Op, Path, Relay, TransportConfig,
    TransportError, DEFAULT_CONNECT_TIMEOUT, DEFAULT_MAX_FRAME_BYTES, DEFAULT_READ_TIMEOUT,
    DEFAULT_WRITE_TIMEOUT, FRAME_TERMINATOR,
};
pub use versioned::{
    save, save_new_private, Versioned, VersionedError, VersionedLoad, FIRST_SCHEMA_VERSION,
};
