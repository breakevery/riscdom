//! Hash chain computation and verification.

use crate::error::AuditError;
use crate::event::AuditEvent;
use crate::store::AuditStore;
use sha2::{Digest, Sha256};

/// `prev_hash` of the very first event in the log.
pub const GENESIS_PREV_HASH: &str =
    "0000000000000000000000000000000000000000000000000000000000000000";

/// Compute the chain hash for an event.
///
/// `sha256(prev_hash | "|" | timestamp_ms | "|" | actor | "|" | action | "|" | detail_json)`
/// returned as lowercase hex.
pub fn compute_hash(prev_hash: &str, event: &AuditEvent, detail_json: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(prev_hash.as_bytes());
    hasher.update(b"|");
    hasher.update(event.timestamp_ms.to_string().as_bytes());
    hasher.update(b"|");
    hasher.update(event.actor.as_bytes());
    hasher.update(b"|");
    hasher.update(event.action.as_bytes());
    hasher.update(b"|");
    hasher.update(detail_json.as_bytes());
    hex::encode(hasher.finalize())
}

/// Result of verifying a chain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChainStatus {
    /// The chain is intact and has `length` events.
    Intact { length: usize },
    /// The chain is broken at event `at_id`.
    Broken { at_id: i64, reason: String },
}

/// Verify the whole chain: linkage (`prev_hash` continuity) **and** hash
/// integrity. Returns the first broken event, if any.
pub fn verify_chain(store: &AuditStore) -> Result<ChainStatus, AuditError> {
    let rows = store.scan()?;
    let mut expected_prev = GENESIS_PREV_HASH.to_string();

    for row in &rows {
        if row.prev_hash != expected_prev {
            return Ok(ChainStatus::Broken {
                at_id: row.id,
                reason: format!(
                    "prev_hash mismatch: expected {}, found {}",
                    expected_prev, row.prev_hash
                ),
            });
        }

        let recomputed = compute_hash(
            &row.prev_hash,
            &AuditEvent {
                timestamp_ms: row.timestamp_ms,
                actor: row.actor.clone(),
                action: row.action.clone(),
                detail: serde_json::Value::Null, // unused by compute_hash
                agent_id: None,                  // unused by compute_hash
            },
            &row.detail_json,
        );

        if recomputed != row.hash {
            return Ok(ChainStatus::Broken {
                at_id: row.id,
                reason: format!("hash mismatch: expected {}, found {}", recomputed, row.hash),
            });
        }

        expected_prev = row.hash.clone();
    }

    Ok(ChainStatus::Intact { length: rows.len() })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(ts: i64, actor: &str, action: &str) -> AuditEvent {
        AuditEvent {
            timestamp_ms: ts,
            actor: actor.into(),
            action: action.into(),
            detail: serde_json::json!({ "n": ts }),
            agent_id: None,
        }
    }

    #[test]
    fn hash_is_sha256_lowercase_hex() {
        let h = compute_hash(
            GENESIS_PREV_HASH,
            &ev(1, "sandbox", "vm.start"),
            "{\"n\":1}",
        );
        assert_eq!(h.len(), 64);
        assert!(h
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
    }

    #[test]
    fn hash_changes_with_input() {
        let a = compute_hash(
            GENESIS_PREV_HASH,
            &ev(1, "sandbox", "vm.start"),
            "{\"n\":1}",
        );
        let b = compute_hash(GENESIS_PREV_HASH, &ev(1, "sandbox", "vm.stop"), "{\"n\":1}");
        assert_ne!(a, b);
    }
}
