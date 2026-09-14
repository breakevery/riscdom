//! Audit sinks: the trait every producer uses, plus the SQLite-backed sink.

use crate::error::AuditError;
use crate::event::AuditEvent;
use crate::hash::{verify_chain, ChainStatus};
use crate::store::AuditStore;
use std::sync::{Arc, Mutex};

/// Anything that can record audit events.
///
/// `record` takes `&mut self` so implementations may hold a non-synchronised
/// resource; cross-thread sharing is handled by wrapping the sink in a `Mutex`.
pub trait AuditSink: Send + Sync {
    fn record(&mut self, event: AuditEvent);
}

/// An [`AuditSink`] that writes to an [`AuditStore`].
///
/// The store is held behind an `Arc<Mutex<_>>` so that the same store can be
/// shared with an independent verifier / reader.
pub struct SqliteAuditSink {
    store: Arc<Mutex<AuditStore>>,
}

impl SqliteAuditSink {
    /// Wrap an owned store.
    pub fn new(store: AuditStore) -> Self {
        Self {
            store: Arc::new(Mutex::new(store)),
        }
    }

    /// Wrap an already-shared store.
    pub fn from_shared(store: Arc<Mutex<AuditStore>>) -> Self {
        Self { store }
    }

    /// A handle to the underlying store (for reading / verification).
    pub fn shared_store(&self) -> Arc<Mutex<AuditStore>> {
        Arc::clone(&self.store)
    }

    /// Verify the underlying chain.
    pub fn verify(&self) -> Result<ChainStatus, AuditError> {
        let store = self
            .store
            .lock()
            .map_err(|_| AuditError::Other("audit store mutex poisoned".into()))?;
        verify_chain(&store)
    }
}

impl AuditSink for SqliteAuditSink {
    fn record(&mut self, event: AuditEvent) {
        // Auditing must never take the caller down; a poisoned/locked store
        // simply drops the event (in practice impossible: single writer).
        if let Ok(mut store) = self.store.lock() {
            let _ = store.append(event);
        }
    }
}
