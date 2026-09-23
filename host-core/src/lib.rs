//! host-core — the portable half of the RiscDom host.
//!
//! Everything the kernel facade does that does **not** need a webview: the
//! audit store wiring, the VM slot, snapshots, sessions, the toolchain and QEMU
//! download paths, the preflight, the local-model probe, workspace paths and
//! the one event envelope. It depends on `agent` / `sandbox` / `audit` and on
//! no Tauri crate.
//!
//! The Tauri-facing half is the `host-tauri` crate, which depends on this one and adds
//! `commands` plus the Tauri transport for the envelope. The dependency runs one
//! way only — `host → host-core` — so the portable half can be driven by the
//! CLI, by `worker` and by the control plane without linking a GUI toolkit.
//!
//! Security constraints (unchanged by the move):
//! - The API key exists in memory only (`AppState::llm_config`): never on disk,
//!   never in the audit chain, never in a log.
//! - File reads and writes go through `agent::WorkspacePolicy`.
//! - Nothing outside this crate reaches `sandbox` / `agent` directly.

pub mod dispatch;
pub mod error;
pub mod events;
pub mod executor;
pub mod keyring;
pub mod paths;
pub mod preflight;
pub mod qemu_download;
pub mod run_diff;
pub mod sandbox_def;
pub mod sandbox_request;
pub mod session;
pub mod settings;
pub mod state;
pub mod toolchain_download;

pub use dispatch::{local_dispatcher, HostAgentHandle};
pub use error::HostError;
pub use events::{EventSink, EV_PREFLIGHT, EV_SANDBOX_REQUEST, EV_SANDBOX_SWITCH};
pub use executor::{StdioExecutorHandle, DEFAULT_EXECUTOR_TIMEOUT};
pub use keyring::{KeyringBackend, OsKeyring, SERVICE};
pub use preflight::{PreflightCache, PreflightRow, PreflightView};
pub use run_diff::{diff_fingerprints, FingerprintFieldDiff, FINGERPRINT_FIELDS};
pub use sandbox_def::{
    CandidateView, CandidatesView, SandboxDef, SandboxSource, SandboxView, DEFAULT_SANDBOX_NAME,
    NO_VERSION,
};
pub use sandbox_request::{
    SandboxAction, SandboxRequest, SandboxRequestService, SandboxRequestStatus, SandboxRequestView,
    SandboxRequests,
};
pub use session::{SessionMessage, SessionMeta, SessionStore};
pub use state::{
    AgentOutcomeView, AppState, AuditStatusView, ChainStatusView, LlmConfigInput, LlmConfigStatus,
    LlmReadiness, LocalProbeResult, LocalProviderInfo, ProviderPresetView, QemuDownloadStatus,
    QemuView, RunView, SessionDetailView, SnapshotMetaView, StoredEventView,
    ToolchainDownloadStatus, ToolchainView, VmStatusView,
};
