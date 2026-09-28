//! Verification: the six steps, in the order [connection.md §3](../../docs/connection.md)
//! freezes, and the error category each failure maps to.
//!
//! ```text
//! 1. the receiver knows the sender        → Refused
//! 2. the signature verifies               → Invalid
//! 3. `v` is a protocol version we speak   → Invalid
//! 4. `to` is this node                    → Invalid
//! 5. `ts` is inside the window            → Network
//! 6. the message is not a replay          → Refused
//! ```
//!
//! The first three categories come straight from §3's own sentence; the fourth is a
//! filled gap (see [`VerifyError::NotAddressedToUs`]), and the last two are §3.2's
//! window and record, checked in that order so a stale message never advances a mark.
//!
//! **Authentication, not authorisation.** This module answers *who sent this*. Whether
//! that node may do the thing is the capability question, answered by the existing model
//! ([security-model.md §4](../../docs/security-model.md)) — deliberately not this crate's, and
//! deliberately not this batch's.

use crate::message::{body_hash, now_ms, SignedMessage, PROTOCOL_VERSION};
use crate::replay::{ReplayError, ReplayGuard};
use ed25519_dalek::VerifyingKey;
use std::collections::HashMap;

/// The error model's categories ([error-model.md](../../docs/error-model.md) §3).
///
/// All five are here so the mapping is total and stable for a caller that branches on
/// it; verification itself can only ever produce three of them — `Refused`, `Invalid`
/// and `Network`. `Crashed` is a dead process and `Partial` is a batch that half
/// finished, and neither is something a single verified frame can be.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    Network,
    Refused,
    Crashed,
    Partial,
    Invalid,
}

impl Category {
    /// The wire word the error model uses.
    pub fn as_str(self) -> &'static str {
        match self {
            Category::Network => "network",
            Category::Refused => "refused",
            Category::Crashed => "crashed",
            Category::Partial => "partial",
            Category::Invalid => "invalid",
        }
    }
}

/// Why a message was not accepted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerifyError {
    /// Step 1: the sender is not a peer this node knows.
    ///
    /// §9 of the frozen document is the reason this is a refusal rather than an
    /// invitation: a peer is untrusted until it is known, and there is no trust on
    /// first use.
    UnknownSender(String),
    /// Step 2: no public key of that peer verifies the signature.
    BadSignature(String),
    /// Step 3: a protocol version this build does not speak.
    UnsupportedVersion { found: u32, supported: u32 },
    /// Step 4: addressed to somebody else.
    ///
    /// §3 does not name this one; it is [`Category::Invalid`] because the sender
    /// addressed the wrong node, which is a wrong input rather than a policy refusal.
    NotAddressedToUs { to: String, us: String },
    /// Step 5: older than the window.
    Stale { ts: i64, now: i64, back_ms: i64 },
    /// Step 5: further into the future than the window allows.
    Future { ts: i64, now: i64, ahead_ms: i64 },
    /// Step 6: this peer has already been seen at this `ts` with this payload.
    Replay { from: String, ts: i64 },
}

impl VerifyError {
    /// The error model's category for this failure.
    pub fn category(&self) -> Category {
        match self {
            VerifyError::UnknownSender(_) | VerifyError::Replay { .. } => Category::Refused,
            VerifyError::BadSignature(_)
            | VerifyError::UnsupportedVersion { .. }
            | VerifyError::NotAddressedToUs { .. } => Category::Invalid,
            VerifyError::Stale { .. } | VerifyError::Future { .. } => Category::Network,
        }
    }
}

impl std::fmt::Display for VerifyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VerifyError::UnknownSender(from) => write!(f, "{from} is not a known peer"),
            VerifyError::BadSignature(from) => {
                write!(f, "the signature from {from} does not verify")
            }
            VerifyError::UnsupportedVersion { found, supported } => write!(
                f,
                "protocol version {found} is not one this build speaks ({supported})"
            ),
            VerifyError::NotAddressedToUs { to, us } => {
                write!(f, "addressed to {to}, and this node is {us}")
            }
            VerifyError::Stale { ts, now, back_ms } => write!(
                f,
                "the timestamp is {} ms old; the window allows {back_ms}",
                now - ts
            ),
            VerifyError::Future { ts, now, ahead_ms } => write!(
                f,
                "the timestamp is {} ms in the future; the window allows {ahead_ms}",
                ts - now
            ),
            VerifyError::Replay { from, ts } => {
                write!(f, "{from} has already been seen at {ts}")
            }
        }
    }
}

impl std::error::Error for VerifyError {}

impl From<ReplayError> for VerifyError {
    fn from(error: ReplayError) -> Self {
        match error {
            ReplayError::Stale { ts, now, back_ms } => VerifyError::Stale { ts, now, back_ms },
            ReplayError::Future { ts, now, ahead_ms } => VerifyError::Future { ts, now, ahead_ms },
            ReplayError::Replay { from, ts } => VerifyError::Replay { from, ts },
        }
    }
}

/// The peers this node knows, and the public keys each currently carries.
///
/// A **set** of keys per peer rather than one, because [decisions §13](../../docs/decisions.md)
/// runs several in parallel during a rotation: §3.2 verifies against *all* the keys the
/// entry currently carries, and a rotation must not make yesterday's message unverifiable
/// during the grey period.
#[derive(Debug, Clone, Default)]
pub struct PeerKeys {
    by_node: HashMap<String, Vec<VerifyingKey>>,
}

impl PeerKeys {
    /// Nobody is known yet — and an empty set refuses everything, which is the default
    /// the rest of the project uses.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record (or replace) the keys a peer currently carries.
    pub fn insert(&mut self, node_id: &str, keys: impl IntoIterator<Item = VerifyingKey>) {
        self.by_node
            .insert(node_id.to_string(), keys.into_iter().collect());
    }

    /// Add one key to a peer's set, keeping the ones already there — the grey period's
    /// state, where old and new are both valid.
    pub fn add_key(&mut self, node_id: &str, key: VerifyingKey) {
        self.by_node
            .entry(node_id.to_string())
            .or_default()
            .push(key);
    }

    /// Is this `node_id` known at all?
    pub fn knows(&self, node_id: &str) -> bool {
        self.by_node.contains_key(node_id)
    }

    /// The keys the peer currently carries (empty when it is unknown).
    pub fn keys_of(&self, node_id: &str) -> &[VerifyingKey] {
        self.by_node.get(node_id).map(Vec::as_slice).unwrap_or(&[])
    }

    /// How many peers are known.
    pub fn len(&self) -> usize {
        self.by_node.len()
    }

    /// Is nobody known?
    pub fn is_empty(&self) -> bool {
        self.by_node.is_empty()
    }
}

/// A message that passed all six steps: authenticated, addressed here, fresh.
#[derive(Debug, Clone, PartialEq)]
pub struct VerifiedMessage {
    /// The sender, as the signature named it.
    pub from: String,
    /// Who it was addressed to (this node, since it verified).
    pub to: String,
    /// The sender's timestamp, as accepted.
    pub ts: i64,
    /// The payload, untouched.
    pub body: serde_json::Value,
}

/// Verify a message against `now`'s clock.
///
/// The clock is a parameter rather than read inside, so a test can place a message
/// anywhere in the window without waiting for one.
pub fn verify_at(
    message: &SignedMessage,
    this_node: &str,
    peers: &PeerKeys,
    guard: &mut ReplayGuard,
    now: i64,
) -> Result<VerifiedMessage, VerifyError> {
    // 1. we know the sender.
    if !peers.knows(&message.from) {
        return Err(VerifyError::UnknownSender(message.from.clone()));
    }
    // 2. one of that peer's keys verifies the signature over the canonical bytes.
    if !peers
        .keys_of(&message.from)
        .iter()
        .any(|key| message.verify_with(key))
    {
        return Err(VerifyError::BadSignature(message.from.clone()));
    }
    // 3. the protocol version is one we speak. A newer major is refused, not guessed at;
    //    an older one is refused for the same reason — we do not speak it either.
    if message.v != PROTOCOL_VERSION {
        return Err(VerifyError::UnsupportedVersion {
            found: message.v,
            supported: PROTOCOL_VERSION,
        });
    }
    // 4. it is addressed here.
    if message.to != this_node {
        return Err(VerifyError::NotAddressedToUs {
            to: message.to.clone(),
            us: this_node.to_string(),
        });
    }
    // 5 and 6. inside the window, and not a replay — the guard checks the window first
    // so a stale message cannot advance the peer's mark.
    guard.accept(&message.from, message.ts, &body_hash(&message.body), now)?;
    Ok(VerifiedMessage {
        from: message.from.clone(),
        to: message.to.clone(),
        ts: message.ts,
        body: message.body.clone(),
    })
}

/// Verify a message against this machine's clock.
pub fn verify(
    message: &SignedMessage,
    this_node: &str,
    peers: &PeerKeys,
    guard: &mut ReplayGuard,
) -> Result<VerifiedMessage, VerifyError> {
    verify_at(message, this_node, peers, guard, now_ms())
}
