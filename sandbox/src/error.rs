//! Sandbox error type.

use std::fmt;

/// Errors produced by the sandbox layer.
#[derive(Debug)]
pub enum SandboxError {
    /// Failed to spawn the QEMU process.
    Spawn(String),
    /// Filesystem / IO error.
    Io(String),
    /// Serial transport error.
    Serial(String),
    /// QMP protocol error.
    Qmp(String),
    /// Operation timed out.
    Timeout(String),
    /// Invalid configuration.
    Config(String),
    /// The VM is not running.
    NotRunning,
    /// The VM is already running.
    AlreadyRunning,
    /// Snapshot save/load error.
    Snapshot(String),
    /// Feature not supported on this platform / in MVP.
    Unsupported(String),
}

impl fmt::Display for SandboxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SandboxError::Spawn(m) => write!(f, "failed to spawn QEMU: {m}"),
            SandboxError::Io(m) => write!(f, "io error: {m}"),
            SandboxError::Serial(m) => write!(f, "serial error: {m}"),
            SandboxError::Qmp(m) => write!(f, "qmp error: {m}"),
            SandboxError::Timeout(m) => write!(f, "timed out: {m}"),
            SandboxError::Config(m) => write!(f, "invalid config: {m}"),
            SandboxError::NotRunning => write!(f, "the virtual machine is not running"),
            SandboxError::AlreadyRunning => write!(f, "the virtual machine is already running"),
            SandboxError::Snapshot(m) => write!(f, "snapshot error: {m}"),
            SandboxError::Unsupported(m) => write!(f, "unsupported: {m}"),
        }
    }
}

impl std::error::Error for SandboxError {}

impl From<std::io::Error> for SandboxError {
    fn from(e: std::io::Error) -> Self {
        SandboxError::Io(e.to_string())
    }
}
