//! A node's identity: an Ed25519 key pair in `<data-dir>/node.key` (v1.0 M4a).
//!
//! [connection.md §2](../../docs/connection.md) freezes this: a node's cryptographic
//! identity is one Ed25519 key pair, kept as **one JWK** (RFC 7517 / 8037
//! `OKP`/`Ed25519`) whose **first member is `schema_version`**, written with mode
//! `600` (an owner-only ACL on Windows), minted on the first start that has
//! networking configured, and **never minted by a read**.
//!
//! Three things this module deliberately does *not* do:
//!
//! - **It does not put the node's name in the file.** §2 separates the key pair from
//!   the device name: `node_id` is the *device name* ([`agent::identity`]'s, and the
//!   connection layer is what sets it), and the file holds key material only.
//! - **It does not sign anything.** That is §3, and the next piece of this crate; the
//!   only thing here that touches the chain is the **fingerprint**, which borrows
//!   [`audit`]'s canonical JSON and hashing rather than re-implementing them.
//! - **It does not read the keyring yet.** §2 allows the key optionally to live in the
//!   OS keyring; the file path lands first, and [`NodeKey`] is a value a caller can
//!   hand to whichever store it prefers.

use crate::versioned::{self, Versioned, VersionedError, VersionedLoad};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use ed25519_dalek::{SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// The file name, inside the node's data directory.
pub const NODE_KEY_FILE: &str = "node.key";

/// The JWK key type and curve: `{"kty":"OKP","crv":"Ed25519"}` (RFC 8037).
pub const KEY_TYPE: &str = "OKP";
pub const CURVE: &str = "Ed25519";

/// An Ed25519 key is 32 bytes, public and private alike.
pub const KEY_BYTES: usize = 32;

/// A node's identity, as it is written to disk: one JWK.
///
/// The member order is the file's member order, and `schema_version` is first because
/// [`serde`] writes struct fields in declaration order — which is how
/// [decisions §11](../../docs/decisions.md)'s "first field" rule is kept.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeKey {
    /// Always [`NodeKey::SCHEMA_VERSION`] when this build writes it.
    pub schema_version: u32,
    /// [`KEY_TYPE`].
    pub kty: String,
    /// [`CURVE`].
    pub crv: String,
    /// The **public** key: 32 bytes, base64url without padding.
    pub x: String,
    /// The **private** key: the 32-byte seed, base64url without padding. Never
    /// logged, never sent, never in an argument.
    pub d: String,
}

/// Why a node key could not be used.
#[derive(Debug)]
pub enum NodeKeyError {
    /// The file could not be read, written, versioned or parsed.
    Versioned(VersionedError),
    /// The document is not a usable JWK: a wrong `kty`/`crv`, a member that is not
    /// 32 bytes of base64url, or two halves that do not belong together.
    Shape(String),
    /// The OS random source refused.
    Random(String),
}

impl std::fmt::Display for NodeKeyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NodeKeyError::Versioned(e) => write!(f, "{e}"),
            NodeKeyError::Shape(why) => write!(f, "the node key is not a usable JWK: {why}"),
            NodeKeyError::Random(e) => write!(f, "the OS random source failed: {e}"),
        }
    }
}

impl std::error::Error for NodeKeyError {}

impl From<VersionedError> for NodeKeyError {
    fn from(error: VersionedError) -> Self {
        NodeKeyError::Versioned(error)
    }
}

impl From<std::io::Error> for NodeKeyError {
    fn from(error: std::io::Error) -> Self {
        NodeKeyError::Versioned(VersionedError::Io(error))
    }
}

impl NodeKey {
    /// Mint a key pair from the OS random source.
    ///
    /// The seed comes from `getrandom`, the same call `server` mints its bearer token
    /// with, and the key is built from it directly: `ed25519-dalek` takes the 32-byte
    /// seed, so no second random-number abstraction is pulled in.
    pub fn generate() -> Result<Self, NodeKeyError> {
        let mut seed = [0u8; KEY_BYTES];
        getrandom::fill(&mut seed).map_err(|e| NodeKeyError::Random(e.to_string()))?;
        Ok(Self::from_signing_key(&SigningKey::from_bytes(&seed)))
    }

    /// The JWK for a live signing key.
    pub fn from_signing_key(key: &SigningKey) -> Self {
        Self {
            schema_version: Self::SCHEMA_VERSION,
            kty: KEY_TYPE.to_string(),
            crv: CURVE.to_string(),
            x: URL_SAFE_NO_PAD.encode(key.verifying_key().to_bytes()),
            d: URL_SAFE_NO_PAD.encode(key.to_bytes()),
        }
    }

    /// The signing key, after checking that the document is one.
    pub fn signing_key(&self) -> Result<SigningKey, NodeKeyError> {
        Ok(self.pair()?.0)
    }

    /// The public key on its own.
    pub fn verifying_key(&self) -> Result<VerifyingKey, NodeKeyError> {
        Ok(self.pair()?.1)
    }

    /// Both halves, checked against each other.
    ///
    /// The public half is **re-derived from the private one and compared**, so a file
    /// whose `x` and `d` disagree — a hand-edit, a truncated write, two files spliced —
    /// is refused instead of producing a node that signs as one identity and is
    /// verified as another. Both halves are returned so the callers above do not each
    /// re-derive one.
    fn pair(&self) -> Result<(SigningKey, VerifyingKey), NodeKeyError> {
        self.check_labels()?;
        let seed = decode(KEY_BYTES, "d", &self.d)?;
        let mut bytes = [0u8; KEY_BYTES];
        bytes.copy_from_slice(&seed);
        let signing = SigningKey::from_bytes(&bytes);
        if URL_SAFE_NO_PAD.encode(signing.verifying_key().to_bytes()) != self.x {
            return Err(NodeKeyError::Shape(
                "the public half does not match the private half".into(),
            ));
        }
        let public = decode(KEY_BYTES, "x", &self.x)?;
        let mut array = [0u8; KEY_BYTES];
        array.copy_from_slice(&public);
        let verifying = VerifyingKey::from_bytes(&array)
            .map_err(|e| NodeKeyError::Shape(format!("`x` is not a public key: {e}")))?;
        Ok((signing, verifying))
    }

    /// The **public** JWK: what a peer stores in its `peers.json` entry.
    ///
    /// No `d`, on purpose — this is the shape that leaves the machine
    /// ([connection.md §3](../../docs/connection.md): the private key never does).
    pub fn public_jwk(&self) -> serde_json::Value {
        serde_json::json!({ "crv": self.crv, "kty": self.kty, "x": self.x })
    }

    /// A stable fingerprint of the **public** half: SHA-256 of its canonical JSON.
    ///
    /// Borrowed from [`audit`] rather than written here — the chain already defines
    /// what canonical bytes and a fingerprint are, and a second definition is a second
    /// thing that can drift. It is the same value two nodes would compute for this key.
    pub fn fingerprint(&self) -> String {
        audit::fingerprint(&self.public_jwk())
    }

    /// The display form of [`Self::fingerprint`].
    pub fn short_fingerprint(&self) -> String {
        audit::short_fingerprint(&self.fingerprint()).to_string()
    }

    /// Read `<data_dir>/node.key`. Never mints anything.
    pub fn load_in(data_dir: &Path) -> Result<VersionedLoad<Self>, NodeKeyError> {
        Self::load(&data_dir.join(NODE_KEY_FILE))
    }

    /// Read a key file. Never mints anything — [`Self::load_or_create_in`] is the one
    /// that does, and only when networking is configured.
    pub fn load(path: &Path) -> Result<VersionedLoad<Self>, NodeKeyError> {
        versioned::load(path).map_err(NodeKeyError::from)
    }

    /// Write a **new** key file, owner-only, refusing to overwrite an existing one.
    pub fn save_new_in(data_dir: &Path, key: &Self) -> Result<PathBuf, NodeKeyError> {
        let path = data_dir.join(NODE_KEY_FILE);
        std::fs::create_dir_all(data_dir)?;
        versioned::save_new_private(&path, key)?;
        Ok(path)
    }

    /// The node's key, minting one the first time it is asked for **with networking
    /// configured** ([connection.md §2](../../docs/connection.md)).
    ///
    /// - `enabled == false` — this node is not on a network: `Ok(None)`, and nothing is
    ///   read or written. A node that never joins a network never has a key.
    /// - the file is missing — mint one and write it, once.
    /// - the file is current (or older and migratable) — answer it as it stands.
    /// - the file is newer — refuse, and write nothing.
    pub fn load_or_create_in(data_dir: &Path, enabled: bool) -> Result<Option<Self>, NodeKeyError> {
        if !enabled {
            return Ok(None);
        }
        match Self::load_in(data_dir)? {
            VersionedLoad::Missing => {
                let key = Self::generate()?;
                Self::save_new_in(data_dir, &key)?;
                Ok(Some(key))
            }
            // A migrated file is used as it stands; this build writes version 1, so
            // there is nothing to migrate today and nothing to write back. The arm is
            // here so the first format that does migrate cannot forget its caller.
            VersionedLoad::Current(key) | VersionedLoad::Migrated { value: key, .. } => {
                Ok(Some(key))
            }
            VersionedLoad::TooNew { found } => {
                Err(NodeKeyError::Versioned(VersionedError::Shape(format!(
                    "{NODE_KEY_FILE} is version {found}; this build reads {}",
                    <Self as Versioned>::SCHEMA_VERSION
                ))))
            }
        }
    }

    /// Every member is present and the pair is coherent.
    ///
    /// This is the definition of "a usable JWK" for this format, and it is what a
    /// **read** checks: a key file that is wrong is refused when it is read, not the
    /// first time somebody tries to sign with it.
    fn check_shape(&self) -> Result<(), NodeKeyError> {
        self.pair().map(|_| ())
    }

    /// The two labels that say which algorithm this key is.
    fn check_labels(&self) -> Result<(), NodeKeyError> {
        if self.kty != KEY_TYPE {
            return Err(NodeKeyError::Shape(format!(
                "kty is {:?}, expected {KEY_TYPE:?}",
                self.kty
            )));
        }
        if self.crv != CURVE {
            return Err(NodeKeyError::Shape(format!(
                "crv is {:?}, expected {CURVE:?}",
                self.crv
            )));
        }
        Ok(())
    }
}

impl Versioned for NodeKey {
    const SCHEMA_VERSION: u32 = 1;

    fn from_value(value: serde_json::Value, _from: u32) -> Result<Self, VersionedError> {
        let key: NodeKey = serde_json::from_value(value)
            .map_err(|e| VersionedError::Shape(format!("the node key is malformed: {e}")))?;
        key.check_shape()
            .map_err(|e| VersionedError::Shape(e.to_string()))?;
        Ok(key)
    }
}

/// Decode one base64url member, insisting it is exactly `wanted` bytes.
fn decode(wanted: usize, field: &'static str, text: &str) -> Result<Vec<u8>, NodeKeyError> {
    let bytes = URL_SAFE_NO_PAD
        .decode(text)
        .map_err(|e| NodeKeyError::Shape(format!("`{field}` is not base64url: {e}")))?;
    if bytes.len() != wanted {
        return Err(NodeKeyError::Shape(format!(
            "`{field}` is {} bytes, expected {wanted}",
            bytes.len()
        )));
    }
    Ok(bytes)
}
