//! Audit sinks: the trait every producer uses, plus the SQLite-backed sink
//! and a simple file-backed example sink.

use crate::error::AuditError;
use crate::event::AuditEvent;
use crate::hash::{verify_chain, ChainStatus};
use crate::store::AuditStore;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// Anything that can record audit events.
///
/// `record` takes `&mut self` so implementations may hold a non-synchronised
/// resource; cross-thread sharing is handled by wrapping the sink in a `Mutex`.
///
/// It **returns the outcome** (v0.8): a failed write is never silently dropped,
/// and the sink retries a locked database before giving up.
pub trait AuditSink: Send + Sync {
    fn record(&mut self, event: AuditEvent) -> Result<(), AuditError>;
}

/// Called when a write failed after the retries, so a host can surface it (v0.8).
///
/// It is deliberately a callback on the *sink* rather than a return value each
/// producer would have to handle: the sandbox and the agent loop share the very
/// same sink object as the host, and they have nowhere to put an error. The
/// reporter is what makes "no silent loss" hold for every producer at once.
pub type AuditFailureReporter = Arc<dyn Fn(&AuditError) + Send + Sync>;

/// An [`AuditSink`] that writes to an [`AuditStore`].
///
/// The store is held behind an `Arc<Mutex<_>>` so that the same store can be
/// shared with an independent verifier / reader.
pub struct SqliteAuditSink {
    store: Arc<Mutex<AuditStore>>,
    /// Told about a write that failed after the retries (v0.8). Never invoked
    /// while the store lock is held.
    reporter: Option<AuditFailureReporter>,
}

impl SqliteAuditSink {
    /// Wrap an owned store.
    pub fn new(store: AuditStore) -> Self {
        Self {
            store: Arc::new(Mutex::new(store)),
            reporter: None,
        }
    }

    /// Wrap an already-shared store.
    pub fn from_shared(store: Arc<Mutex<AuditStore>>) -> Self {
        Self {
            store,
            reporter: None,
        }
    }

    /// Report write failures to `reporter` (the host queues them for its alert).
    pub fn with_reporter(mut self, reporter: AuditFailureReporter) -> Self {
        self.reporter = Some(reporter);
        self
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
    fn record(&mut self, event: AuditEvent) -> Result<(), AuditError> {
        // The store lock is released before the reporter runs, so a reporter that
        // touches any host-side queue cannot deadlock against a writer.
        let result = match self.store.lock() {
            Ok(mut store) => store.append(event).map(|_| ()),
            Err(_) => Err(AuditError::Other("audit store mutex poisoned".into())),
        };
        if let Err(error) = &result {
            if let Some(reporter) = &self.reporter {
                reporter(error);
            }
        }
        result
    }
}

/// File-backed example sink: appends one JSON object per line (JSONL).
///
/// This is the **example implementation** kept from `sandbox`'s original
/// placeholder. It carries no chain / tamper-evidence of its own — prefer
/// [`SqliteAuditSink`] in production.
pub struct FileAuditSink {
    path: PathBuf,
}

impl FileAuditSink {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl AuditSink for FileAuditSink {
    fn record(&mut self, event: AuditEvent) -> Result<(), AuditError> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let line = serde_json::json!({
            "timestamp_ms": event.timestamp_ms,
            "actor": event.actor,
            "action": event.action,
            "detail": event.detail,
        });
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        writeln!(file, "{}", serde_json::to_string(&line)?)?;
        Ok(())
    }
}
