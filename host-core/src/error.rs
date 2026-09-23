//! Host error type.
//!
//! `Display` output is what the frontend sees through Tauri commands, so it
//! must never contain the API key or other secrets.

use thiserror::Error;

/// Errors produced by the host layer.
#[derive(Debug, Error)]
pub enum HostError {
    #[error("not configured: {0}")]
    NotConfigured(String),

    #[error("policy denied: {0}")]
    Policy(String),

    #[error("agent error: {0}")]
    Agent(#[from] agent::AgentError),

    #[error("audit error: {0}")]
    Audit(#[from] audit::AuditError),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// No sandbox definition carries that name (v0.9 sandbox F2b).
    ///
    /// The sandbox variants carry a machine-readable code in their `Display`
    /// (`sandbox_not_found`, `sandbox_qemu_missing`, `sandbox_toolchain_missing`,
    /// `sandbox_kernel_missing`, `sandbox_request_not_found`,
    /// `sandbox_request_decided`), so a caller can branch on the reason without
    /// matching prose, and `user_message` is still what the interface shows.
    #[error("sandbox_not_found: {0}")]
    SandboxNotFound(String),

    /// The definition's QEMU is missing, or is not a QEMU that runs.
    #[error("sandbox_qemu_missing: {0}")]
    SandboxQemuMissing(String),

    /// The definition's RISC-V toolchain is missing.
    #[error("sandbox_toolchain_missing: {0}")]
    SandboxToolchainMissing(String),

    /// The definition pins no kernel and the workspace has none either.
    #[error("sandbox_kernel_missing: {0}")]
    SandboxKernelMissing(String),

    /// Every check passed and the VM still would not start (v0.9 sandbox F2b).
    ///
    /// The node is **stopped**, not half-switched: the failed handle's `Drop`
    /// killed whatever it spawned. The message carries the attempts and the last
    /// reason.
    #[error("sandbox_start_failed: {0}")]
    SandboxStart(String),

    /// No sandbox request carries that id (v0.9 sandbox F2c).
    #[error("sandbox_request_not_found: {0}")]
    SandboxRequestNotFound(String),

    /// The request was already decided (v0.9 sandbox F2c).
    ///
    /// A decision is not reversible (F2c decision 5): the second one is a `409`,
    /// not a silent overwrite of who decided what.
    #[error("sandbox_request_decided: {0}")]
    SandboxRequestDecided(String),

    /// An archive an import cannot accept (v0.9 project in/out).
    ///
    /// One variant for every way an archive can be unusable or hostile —
    /// unreadable, a traversing entry, a link, an absolute path, the host's own
    /// state directory, too many entries, too much decompressed content. The
    /// message names which.
    #[error("archive: {0}")]
    Archive(String),

    /// An import would replace a file that is already in the workspace, and
    /// `force` was not asked for (v0.9 project in/out). The message is the file's
    /// name.
    #[error("workspace entry exists: {0}")]
    WorkspaceEntryExists(String),

    #[error("{0}")]
    Other(String),
}

impl HostError {
    /// A message safe to hand to the frontend (currently just `Display`).
    pub fn user_message(&self) -> String {
        self.to_string()
    }
}
