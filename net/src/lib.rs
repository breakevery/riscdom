//! net — the connection layer (v1.0, roadmap §4).
//!
//! Layer two, as [`docs/connection.md`](../docs/connection.md) freezes it: a node's
//! **identity** (§2), **signing** (§3), **discovery** (§4), **rooms** (§5) and the
//! **cross-region server** (§6). This crate is where that protocol becomes code.
//!
//! **It is built in pieces, in the order the frozen document lays them out**, and
//! this is the first one: the crate's skeleton and §2's `node.key`. There is no
//! network code here yet, no transport and no signing — those are the next pieces,
//! and each lands only after the section it implements is frozen.
//!
//! **Dependency direction.** `net` depends on [`audit`] and nothing else in this
//! workspace. The chain's canonical JSON ([`audit::canonical_json`]) is what a
//! signature is computed over and what a fingerprint is taken of, so there is one
//! description of those bytes instead of two; and `host-core` is what will depend
//! on *this* crate, never the other way round — the workspace's direction is
//! `audit ← net ← host-core ← server`, so nothing here may reach upward.

mod identity;
mod versioned;

pub use identity::{NodeKey, NodeKeyError, NODE_KEY_FILE};
pub use versioned::{
    save, save_new_private, Versioned, VersionedError, VersionedLoad, FIRST_SCHEMA_VERSION,
};
