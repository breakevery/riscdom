//! `peers.json`: who this node knows, and the keys it verifies them with (v1.0 M4b).
//!
//! [connection.md §4.1](../../docs/connection.md) freezes the **entry shape** — the same one a
//! handed-down node table carries, so nothing has to translate between "a peer I was
//! configured with" and "a peer I was told about": `{node_id, addresses[], public_key,
//! capabilities, rooms[]}`. The file around those entries is the versioned-JSON shape
//! [§2](../../docs/connection.md) requires: `schema_version` first, migrate or refuse.
//!
//! **Public keys only.** An entry carrying a `d` (a JWK private half) is refused rather
//! than read: a peer table is a file that gets copied between machines, and a private key
//! in one is a leak that arrived by accident. [§2](../../docs/connection.md) says the private
//! key never leaves its node.
//!
//! The file is **authoritative for the node that owns it** — a handed-down table is a
//! source, never an authority ([`crate::discovery`]) — which is why this module's job is
//! only to read it faithfully.

use crate::error::Category;
use crate::sign::PeerKeys;
use crate::versioned::{self, Versioned, VersionedError, VersionedLoad};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use ed25519_dalek::VerifyingKey;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// The file name, inside the node's data directory.
pub const PEERS_FILE: &str = "peers.json";

/// The claim a node declares when it serves its workgroup as the network's server
/// ([connection.md §6.7](../../docs/connection.md); v1.0 batch AH froze it, batch AJ reads it).
///
/// It is an ordinary string in a `capabilities` claim list — the same list §6.6's registration and
/// the `peers.json` entry above carry — and **not** a member of the control plane's capability
/// vocabulary: declaring it grants nothing. What it buys is exactly one thing: the node is
/// **probed as a sibling** (§6.7), and a prober can only probe peers whose key it holds, which is
/// why the claim is read here, from `peers.json`, and not from a frame.
pub const SERVER_CLAIM: &str = "server";

/// One peer, as both `peers.json` and a handed-down table carry it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PeerEntry {
    /// The **device name** — the same identity a signed message's `from` uses
    /// ([§2](../../docs/connection.md)).
    pub node_id: String,
    /// Where it can be dialled. A node may have several.
    #[serde(default)]
    pub addresses: Vec<String>,
    /// The public half of its identity: a JWK, never a private key.
    pub public_key: serde_json::Value,
    /// What it declared it may do. Carried for the capability model; not checked here.
    #[serde(default)]
    pub capabilities: Vec<String>,
    /// The rooms it says it is in. **A claim about itself**, not membership: membership is
    /// the local `rooms.json` ([§5.1](../../docs/connection.md)).
    #[serde(default)]
    pub rooms: Vec<String>,
}

/// The file: a version and a list of entries.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PeersFile {
    /// Always [`PeersFile::SCHEMA_VERSION`] when this build writes it, and first on the wire.
    pub schema_version: u32,
    #[serde(default)]
    pub peers: Vec<PeerEntry>,
}

/// Why a peer table could not be read.
#[derive(Debug)]
pub enum PeersError {
    /// The file could not be versioned, read or parsed.
    Versioned(VersionedError),
    /// An entry is not usable: no `node_id`, a key that is not an Ed25519 public JWK, or —
    /// the one worth naming — a **private** key where a public one belongs.
    Entry(String),
}

impl std::fmt::Display for PeersError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PeersError::Versioned(e) => write!(f, "{e}"),
            PeersError::Entry(why) => write!(f, "the peer table is not usable: {why}"),
        }
    }
}

impl std::error::Error for PeersError {}

impl From<VersionedError> for PeersError {
    fn from(error: VersionedError) -> Self {
        PeersError::Versioned(error)
    }
}

impl From<std::io::Error> for PeersError {
    fn from(error: std::io::Error) -> Self {
        PeersError::Versioned(VersionedError::Io(error))
    }
}

impl PeersFile {
    /// The version this build writes.
    ///
    /// An inherent constant as well as the trait's, so `PeersFile::SCHEMA_VERSION` reads
    /// the same everywhere without the trait needing to be in scope.
    pub const SCHEMA_VERSION: u32 = 1;
    /// An empty table — a node that knows nobody, which is the default the rest of the
    /// project uses.
    pub fn empty() -> Self {
        Self {
            schema_version: Self::SCHEMA_VERSION,
            peers: Vec::new(),
        }
    }

    /// Read `<data_dir>/peers.json`. Never creates anything.
    pub fn load_in(data_dir: &Path) -> Result<VersionedLoad<Self>, PeersError> {
        Self::load(&data_dir.join(PEERS_FILE))
    }

    /// Read a peer file.
    pub fn load(path: &Path) -> Result<VersionedLoad<Self>, PeersError> {
        versioned::load(path).map_err(PeersError::from)
    }

    /// Write a peer file (a **config** file: not owner-only, and it may be replaced).
    pub fn save_in(data_dir: &Path, file: &Self) -> Result<PathBuf, PeersError> {
        let path = data_dir.join(PEERS_FILE);
        std::fs::create_dir_all(data_dir)?;
        versioned::save(&path, file)?;
        Ok(path)
    }

    /// The entry for one node, if it is here.
    pub fn entry(&self, node_id: &str) -> Option<&PeerEntry> {
        self.peers.iter().find(|entry| entry.node_id == node_id)
    }

    /// The entries that declare the [`SERVER_CLAIM`] — this node's **siblings** (§6.7).
    ///
    /// The claim is configuration here, not a fact: what marks a peer as a server is what the
    /// deployer wrote in `peers.json`, which is also where that peer's public key is — and a key
    /// arrives through configuration and never through a frame (§4.2). A prober that lacks the
    /// key cannot verify an answer, which is why the same file is both the sibling set and its
    /// [`PeerKeys`].
    pub fn servers(&self) -> Vec<&PeerEntry> {
        self.peers
            .iter()
            .filter(|entry| entry.is_server())
            .collect()
    }

    /// The keys of the entries that declare the [`SERVER_CLAIM`], ready for verifying their answers.
    pub fn server_keys(&self) -> Result<PeerKeys, PeersError> {
        let mut keys = PeerKeys::new();
        for entry in self.peers.iter().filter(|entry| entry.is_server()) {
            keys.insert(&entry.node_id, [entry.verifying_key()?]);
        }
        Ok(keys)
    }

    /// The entries as a [`PeerKeys`] table, ready for verification.
    ///
    /// A peer whose key does not parse is **reported**, not skipped quietly: a table where
    /// one entry is broken is a table a caller should hear about.
    pub fn peer_keys(&self) -> Result<PeerKeys, PeersError> {
        let mut keys = PeerKeys::new();
        for entry in &self.peers {
            keys.insert(&entry.node_id, [entry.verifying_key()?]);
        }
        Ok(keys)
    }
}

impl PeerEntry {
    /// A minimal entry: an id, an address and a public JWK.
    pub fn new(node_id: &str, address: &str, public_key: serde_json::Value) -> Self {
        Self {
            node_id: node_id.to_string(),
            addresses: vec![address.to_string()],
            public_key,
            capabilities: Vec::new(),
            rooms: Vec::new(),
        }
    }

    /// The public key this entry carries.
    pub fn verifying_key(&self) -> Result<VerifyingKey, PeersError> {
        public_key_from_jwk(&self.public_key)
    }

    /// Does this entry declare that its node serves as the network's server? (§6.7, [`SERVER_CLAIM`])
    pub fn is_server(&self) -> bool {
        self.capabilities.iter().any(|claim| claim == SERVER_CLAIM)
    }

    /// Does `other` say the same thing about this node? Used by the merge to tell an
    /// update from a conflict ([`crate::discovery`]).
    pub fn same_as(&self, other: &PeerEntry) -> bool {
        self.node_id == other.node_id
            && self.public_key == other.public_key
            && self.addresses == other.addresses
    }

    /// A one-line description for a conflict report.
    pub fn summary(&self) -> String {
        format!(
            "{} at {:?} with rooms {:?}",
            self.node_id, self.addresses, self.rooms
        )
    }
}

/// Read a **public** JWK (`OKP`/`Ed25519`).
///
/// Refuses a `d` member by name: that is a private key, and this crate has exactly one
/// place a private key lives ([`crate::NodeKey`]).
pub fn public_key_from_jwk(value: &serde_json::Value) -> Result<VerifyingKey, PeersError> {
    let object = value
        .as_object()
        .ok_or_else(|| PeersError::Entry("the public key is not a JSON object".into()))?;
    let kty = object
        .get("kty")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    let crv = object
        .get("crv")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    if kty != "OKP" || crv != "Ed25519" {
        return Err(PeersError::Entry(format!(
            "the public key is {kty:?}/{crv:?}, expected \"OKP\"/\"Ed25519\""
        )));
    }
    if object.contains_key("d") {
        return Err(PeersError::Entry(
            "the entry carries a private key (`d`); a peer table holds public keys only".into(),
        ));
    }
    let x = object
        .get("x")
        .and_then(|v| v.as_str())
        .ok_or_else(|| PeersError::Entry("the public key carries no `x`".into()))?;
    let bytes = URL_SAFE_NO_PAD
        .decode(x)
        .map_err(|e| PeersError::Entry(format!("`x` is not base64url: {e}")))?;
    let array: [u8; 32] = bytes
        .as_slice()
        .try_into()
        .map_err(|_| PeersError::Entry(format!("`x` is {} bytes, expected 32", bytes.len())))?;
    VerifyingKey::from_bytes(&array)
        .map_err(|e| PeersError::Entry(format!("`x` is not a public key: {e}")))
}

impl Versioned for PeersFile {
    const SCHEMA_VERSION: u32 = PeersFile::SCHEMA_VERSION;

    fn from_value(value: serde_json::Value, _from: u32) -> Result<Self, VersionedError> {
        let file: PeersFile = serde_json::from_value(value)
            .map_err(|e| VersionedError::Shape(format!("the peer table is malformed: {e}")))?;
        for entry in &file.peers {
            if entry.node_id.trim().is_empty() {
                return Err(VersionedError::Shape(
                    "a peer entry has an empty node_id".into(),
                ));
            }
            entry
                .verifying_key()
                .map_err(|e| VersionedError::Shape(e.to_string()))?;
        }
        Ok(file)
    }
}

/// The category a peer-table failure maps to: the input is wrong, not the transport.
pub fn peers_category(error: &PeersError) -> Category {
    match error {
        PeersError::Versioned(VersionedError::Io(_)) => Category::Network,
        _ => Category::Invalid,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NodeKey;

    #[test]
    fn a_public_entry_reads_back_and_a_private_half_is_refused() {
        let key = NodeKey::generate().expect("key");
        let public = key.public_jwk();
        let entry = PeerEntry::new("dev-a", "127.0.0.1:47821", public.clone());
        assert!(entry.verifying_key().is_ok());
        assert_eq!(
            entry.verifying_key().expect("key").to_bytes(),
            key.verifying_key().expect("key").to_bytes()
        );

        // The private half, spelled out: refused by name.
        let mut with_secret = public;
        with_secret["d"] = serde_json::json!(key.d);
        let error = public_key_from_jwk(&with_secret).expect_err("private key");
        assert!(
            error.to_string().contains("private key"),
            "the refusal says why: {error}"
        );
        assert_eq!(peers_category(&error), Category::Invalid);
    }

    #[test]
    fn only_the_entries_that_declare_the_server_claim_are_siblings() {
        // v1.0 batch AJ: the sibling set is configuration, and the same file carries the keys.
        let server = NodeKey::generate().expect("key");
        let node = NodeKey::generate().expect("key");
        let mut entry = PeerEntry::new("dev-server", "127.0.0.1:1", server.public_jwk());
        assert!(!entry.is_server(), "no claim, no sibling");
        entry.capabilities = vec!["server".to_string()];
        assert!(entry.is_server());
        assert!(entry.verifying_key().is_ok(), "the key is right beside it");

        let file = PeersFile {
            schema_version: PeersFile::SCHEMA_VERSION,
            peers: vec![
                entry,
                PeerEntry::new("dev-plain", "127.0.0.1:2", node.public_jwk()),
            ],
        };
        let siblings = file.servers();
        assert_eq!(siblings.len(), 1);
        assert_eq!(siblings[0].node_id, "dev-server");
        let keys = file.server_keys().expect("keys");
        assert!(!keys.keys_of("dev-server").is_empty(), "the sibling's key");
        assert!(keys.keys_of("dev-plain").is_empty(), "and nobody else's");

        // A claim is a list, not a flag: it may sit beside others.
        let mut mixed = PeerEntry::new("dev-mixed", "127.0.0.1:3", server.public_jwk());
        mixed.capabilities = vec!["audit.read".to_string(), "server".to_string()];
        assert!(mixed.is_server());
        // A different claim is not one: the match is exact.
        let mut not_it = PeerEntry::new("dev-trick", "127.0.0.1:4", server.public_jwk());
        not_it.capabilities = vec!["servers".to_string(), "SERVER".to_string()];
        assert!(!not_it.is_server());
    }
}
