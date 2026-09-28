//! A signed message: the five-member preamble, and the signature beside it (v1.0 M4a).
//!
//! [connection.md §3](../../docs/connection.md) freezes this shape: `@` means **address
//! *and* signature**, what is signed is the **canonical JSON of a five-member preamble**
//! — `{v, from, to, ts, body}` — and the signature travels beside it in a member named
//! `sig`. The canonical bytes are [`audit::canonical_json`]'s, borrowed rather than
//! re-implemented: the chain already defines what "the same bytes on both sides" means,
//! and a second definition is a second thing that can drift.
//!
//! The wire form is **one JSON object per line** (§3.1, the transport in M4a-impl-3).
//! This module owns the object; the transport owns moving it.

use crate::identity::NodeKey;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};

/// The protocol major this build speaks ([connection.md §3.1](../../docs/connection.md)).
///
/// It is checked **per message**, so it lives in the message and not in a handshake: a
/// connection carries messages and each one stands on its own.
pub const PROTOCOL_VERSION: u32 = 1;

/// An Ed25519 signature is 64 bytes.
pub const SIGNATURE_BYTES: usize = 64;

/// One signed message, as it travels.
///
/// The member order is the wire order, and it is the preamble's order followed by
/// `sig`: `serde` writes struct fields in declaration order, so a reader sees the
/// message before the signature that covers it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SignedMessage {
    /// [`PROTOCOL_VERSION`] at the time it was signed.
    pub v: u32,
    /// The sender's `node_id` — its device name ([connection.md §2](../../docs/connection.md)).
    pub from: String,
    /// The recipient's `node_id`, or the room when one is addressed (§3, §5).
    pub to: String,
    /// Epoch milliseconds, the **sender's** clock.
    pub ts: i64,
    /// The payload. Opaque to this layer.
    pub body: serde_json::Value,
    /// The signature over [`Self::canonical_bytes`], base64url without padding.
    pub sig: String,
}

/// Why a message could not be signed, or could not be read off the wire.
#[derive(Debug)]
pub enum MessageError {
    /// The key could not be used.
    Key(crate::identity::NodeKeyError),
    /// A signature that does not decode as 64 bytes of base64url.
    Signature(String),
    /// The line is not one JSON object.
    Json(String),
}

impl std::fmt::Display for MessageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MessageError::Key(e) => write!(f, "{e}"),
            MessageError::Signature(why) => write!(f, "the signature is not usable: {why}"),
            MessageError::Json(why) => write!(f, "the message is not a JSON object: {why}"),
        }
    }
}

impl std::error::Error for MessageError {}

impl From<crate::identity::NodeKeyError> for MessageError {
    fn from(error: crate::identity::NodeKeyError) -> Self {
        MessageError::Key(error)
    }
}

impl SignedMessage {
    /// Sign `body`, addressed from `from` to `to`, stamped `ts`.
    ///
    /// `from` is passed in rather than read from the key: §2 keeps the key pair and the
    /// device name separate, so which name a node signs as is the caller's to state.
    pub fn sign(
        key: &NodeKey,
        from: &str,
        to: &str,
        ts: i64,
        body: serde_json::Value,
    ) -> Result<Self, MessageError> {
        Ok(Self::sign_with_version(
            PROTOCOL_VERSION,
            &key.signing_key()?,
            from,
            to,
            ts,
            body,
        ))
    }

    /// Sign with a bare signing key, for code that holds one without a `node.key`.
    pub fn sign_with(
        signing: &SigningKey,
        from: &str,
        to: &str,
        ts: i64,
        body: serde_json::Value,
    ) -> Self {
        Self::sign_with_version(PROTOCOL_VERSION, signing, from, to, ts, body)
    }

    /// Sign as a **specific** protocol version.
    ///
    /// The version is inside the preamble, so it is signed like anything else: a
    /// verifier can refuse a version it does not speak *after* the signature checks out,
    /// which is the only order that tells "wrong version" apart from "forged". This is
    /// what a test uses to reach step 3, and what a future bump would sign at.
    pub fn sign_with_version(
        v: u32,
        signing: &SigningKey,
        from: &str,
        to: &str,
        ts: i64,
        body: serde_json::Value,
    ) -> Self {
        let preamble = Self::preamble(v, from, to, ts, &body);
        let bytes = audit::canonical_json(&preamble);
        Self {
            v,
            from: from.to_string(),
            to: to.to_string(),
            ts,
            body,
            sig: URL_SAFE_NO_PAD.encode(signing.sign(bytes.as_bytes()).to_bytes()),
        }
    }

    /// The five members the signature covers.
    ///
    /// Key order in a [`serde_json::Value`] is not preserved, which is exactly why the
    /// bytes come from [`audit::canonical_json`]: both sides sort and both sides get
    /// the same string, whatever order anything happened to be written in.
    pub fn preamble(
        v: u32,
        from: &str,
        to: &str,
        ts: i64,
        body: &serde_json::Value,
    ) -> serde_json::Value {
        serde_json::json!({ "v": v, "from": from, "to": to, "ts": ts, "body": body })
    }

    /// The bytes the signature is computed over.
    pub fn canonical_bytes(&self) -> String {
        audit::canonical_json(&Self::preamble(
            self.v, &self.from, &self.to, self.ts, &self.body,
        ))
    }

    /// The signature, decoded.
    pub fn signature(&self) -> Result<Signature, MessageError> {
        let bytes = URL_SAFE_NO_PAD
            .decode(&self.sig)
            .map_err(|e| MessageError::Signature(format!("not base64url: {e}")))?;
        let array: [u8; SIGNATURE_BYTES] = bytes.as_slice().try_into().map_err(|_| {
            MessageError::Signature(format!("{} bytes, expected {SIGNATURE_BYTES}", bytes.len()))
        })?;
        Ok(Signature::from_bytes(&array))
    }

    /// One JSON line, newline included: the wire form §3.1 froze.
    pub fn to_line(&self) -> Result<String, MessageError> {
        let mut line =
            serde_json::to_string(self).map_err(|e| MessageError::Json(e.to_string()))?;
        line.push('\n');
        Ok(line)
    }

    /// Read one JSON line back.
    ///
    /// Only the **shape** is checked: a `sig` that does not decode as 64 bytes is
    /// refused here, because a message whose signature cannot even be read is not a
    /// message a verifier should have to think about.
    pub fn parse_line(line: &str) -> Result<Self, MessageError> {
        let message: Self =
            serde_json::from_str(line.trim()).map_err(|e| MessageError::Json(e.to_string()))?;
        message.signature()?;
        Ok(message)
    }

    /// The `from` half of the signature check, kept next to the rest of the shape.
    pub(crate) fn verify_with(&self, key: &VerifyingKey) -> bool {
        let Ok(signature) = self.signature() else {
            return false;
        };
        // `verify_strict` rather than `verify`: it rejects small-order and
        // non-canonical keys and signatures, which is the difference between
        // "this signature maps to this message" and "this key can be made to verify".
        key.verify_strict(self.canonical_bytes().as_bytes(), &signature)
            .is_ok()
    }
}

/// The current time in epoch milliseconds, the unit `ts` is in.
pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// A payload's identity for replay protection: the chain's fingerprint of it.
///
/// Canonical, so two encodings of the same body hash the same; borrowed from [`audit`]
/// for the same reason the signature is.
pub fn body_hash(body: &serde_json::Value) -> String {
    audit::fingerprint(body)
}
