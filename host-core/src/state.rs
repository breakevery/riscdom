//! Application state shared by all Tauri commands.

use crate::error::HostError;
use crate::events::{
    EventSink, EV_AGENT_FINAL, EV_AGENT_ITERATION, EV_AGENT_STREAM_DELTA, EV_AGENT_STREAM_DONE,
    EV_AGENT_TOOL_CALL, EV_AGENT_TOOL_RESULT, EV_AUDIT_FAILED, EV_SANDBOX_SWITCH, EV_SERIAL_CHUNK,
    EV_VM_STATE,
};
use crate::keyring::{user_for_provider, InMemoryKeyring, KeyringBackend, OsKeyring, SERVICE};
use crate::run_diff::{self, FingerprintFieldDiff};
use crate::sandbox_def::{
    CandidatesView, SandboxDef, SandboxSource, SandboxView, DEFAULT_SANDBOX_NAME,
};
use crate::sandbox_request::{
    SandboxAction, SandboxRequestService, SandboxRequestStatus, SandboxRequestView, SandboxRequests,
};
use crate::session::{SessionMessage, SessionMeta, SessionStore};
use crate::settings::LocalSettings;
use crate::toolchain_download::DownloadEvent;
use agent::llm::{DeepSeekClient, LlmClient};
use agent::message::{ChatMessage, ChatRequest, ChatResponse, StreamEvent};
use agent::policy::WorkspacePolicy;
use agent::presets::{builtin_presets, find_preset, ProviderPreset, DEFAULT_PRESET_ID};
use agent::{
    AgentConfig, AgentHandle, AgentId, AgentLoop, AgentOutcome, DispatchError, Dispatcher,
    LocalDispatcher, Task, TaskId, TaskOutcome,
};
use audit::{AuditSink, AuditStore, ChainStatus, SqliteAuditSink, StoredEvent};
use sandbox::platform::{QmpEndpoint, SerialEndpoint};
use sandbox::vm::{RiscVVirtualMachine, VMConfig};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// How many audit-write failures the host keeps queued for the alert (v0.8).
///
/// The newest failures are the ones that describe the current state, so an older
/// one is dropped once the queue is full.
pub const AUDIT_FAILURE_QUEUE_CAP: usize = 32;

/// The constitution is embedded so the desktop app works without the repo.
const CONSTITUTION: &str = include_str!("../../AGENTS.md");

/// Poll interval for the audit/serial bridge.
pub const POLL_INTERVAL: Duration = Duration::from_millis(200);

/// The snapshot mechanism the host uses, as recorded in a run's fingerprint and
/// in the snapshot listing.
const SNAPSHOT_MODE: &str = "tcp-relay";

/// The older reboot-fallback snapshots the host still lists and deletes.
const SNAPSHOT_FALLBACK_MODE: &str = "reboot-fallback";

/// The preflight guest's file names, inside this agent's preflight directory.
const PREFLIGHT_GUEST_SRC: &str = "guest.c";
const PREFLIGHT_GUEST_ELF: &str = "guest.elf";

/// Epoch milliseconds (VM start bookkeeping).
fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// In-flight download bookkeeping (v0.3 #3b).
/// VM status for the top-bar badge (v0.3 #4c).
#[derive(Debug, Clone, serde::Serialize)]
pub struct VmStatusView {
    /// Is a VM held by the host right now?
    pub running: bool,
    /// When the VM started (epoch ms), while it is running.
    pub since_ms: Option<i64>,
}

/// In-flight download bookkeeping (v0.3 #3b).
#[derive(Debug)]
pub struct ToolchainDownloadState {
    pub cancel: Arc<AtomicBool>,
    pub started_at: std::time::Instant,
    /// Which toolchain is being installed (v0.9 F3a-download-apply). One slot serves both,
    /// so this is what lets the status say which one is running.
    pub toolchain: crate::toolchain_download::Toolchain,
}

/// In-flight sandbox switch (v0.9 sandbox F2b).
///
/// The same shape as the download slots, minus the cancel flag: a switch is a
/// synchronous call, and nothing it does — a `--version` probe, a stop, a start —
/// can be interrupted part-way without leaving the node in the state the switch
/// was trying to leave. What a caller needs is the **target**, so a refusal can
/// name what is already being switched to.
#[derive(Debug)]
pub struct SandboxSwitchState {
    pub target: String,
    pub started_at: std::time::Instant,
}

/// Download status for the UI / polling clients.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ToolchainDownloadStatus {
    pub in_progress: bool,
    /// Which toolchain is downloading; `None` while idle (v0.9 F3a-download-apply).
    pub toolchain: Option<crate::toolchain_download::Toolchain>,
    pub last_event: Option<DownloadEvent>,
}

/// In-flight QEMU download bookkeeping (v0.9 sandbox F1).
///
/// The mirror of [`ToolchainDownloadState`]: two assemblies, one shape.
#[derive(Debug)]
pub struct QemuDownloadState {
    pub cancel: Arc<AtomicBool>,
    pub started_at: std::time::Instant,
}

/// QEMU download status for the UI / polling clients.
#[derive(Debug, Clone, serde::Serialize)]
pub struct QemuDownloadStatus {
    pub in_progress: bool,
    pub last_event: Option<crate::qemu_download::QemuDownloadEvent>,
}

/// QEMU status shown in the UI (v0.3 5b-1a).
#[derive(Debug, Clone, serde::Serialize)]
pub struct QemuView {
    /// Is a usable `qemu-system-riscv64` available?
    pub found: bool,
    /// Resolved path (absent when not found).
    pub path: Option<String>,
    /// `EnvVar` / `KnownPath` / `Path` / `Manual`.
    pub source: String,
    /// Human-readable search record.
    pub diagnostics: String,
}

/// Toolchain status shown in the UI (stage 24b).
#[derive(Debug, Clone, serde::Serialize)]
pub struct ToolchainView {
    /// Is a usable RISC-V GCC available?
    pub found: bool,
    /// Resolved path (absent when not found).
    pub path: Option<String>,
    /// `EnvVar` / `KnownPath` / `Path` / `Manual`.
    pub source: String,
    /// Human-readable search record.
    pub diagnostics: String,
}

/// How many times a tool probe re-runs a command the kernel refused to `exec` (v0.9).
///
/// The same five attempts the audit store's write path allows, for the same reason:
/// a refusal that survives the whole budget is reported, never swallowed.
const EXEC_MAX_ATTEMPTS: u32 = 5;

/// The pause between those attempts.
///
/// Fixed at ten milliseconds rather than the audit path's doubling schedule: the
/// condition being waited out (`ETXTBSY`) is a fork-that-has-not-exec'd-yet window,
/// which is microseconds long, so the whole budget is fifty milliseconds — short
/// enough that a genuinely unusable binary is still reported promptly.
const EXEC_RETRY_DELAY: Duration = Duration::from_millis(10);

/// Run a tool probe, retrying the one refusal that says nothing about the tool.
///
/// `ETXTBSY` (`ErrorKind::ExecutableFileBusy`) means "the kernel will not `exec` this
/// file *right now* because some process has it open for writing". On Unix that
/// includes a **process that has forked but not yet exec'd**: `CLOEXEC` closes an
/// inherited descriptor only *at* `exec`, so when this host runs inside a process
/// with other threads (a test binary, and any future multi-threaded host), a sibling
/// thread's `spawn` can hold the write reference for microseconds after this very
/// process has closed its own. The file is fine; the moment is not.
///
/// Only that one error is retried, and only because it is provably transient: every
/// other failure (a missing file, a wrong architecture, a permission that will not
/// change) is returned at once, and a busy refusal that outlives the budget is
/// returned too. Matching on `kind()` rather than `raw_os_error() == 26` is what keeps
/// this honest across platforms: 26 is `ETXTBSY` on Unix and an unrelated Windows error
/// code elsewhere, and only the former maps to this kind.
fn exec_with_busy_retry(
    command: &mut std::process::Command,
) -> std::io::Result<std::process::Output> {
    let mut attempts = 0;
    exec_retrying(command, &mut attempts, EXEC_MAX_ATTEMPTS, EXEC_RETRY_DELAY)
}

/// [`exec_with_busy_retry`]'s loop, with its budget, its pause and its attempt count
/// handed in.
///
/// Split out so a test can read the count: "a missing file is not retried" is only
/// checkable if the number of attempts is observable, and waiting out the real budget
/// to prove a *different* behaviour is what the split avoids.
fn exec_retrying(
    command: &mut std::process::Command,
    attempts: &mut u32,
    max_attempts: u32,
    delay: Duration,
) -> std::io::Result<std::process::Output> {
    loop {
        *attempts += 1;
        match command.output() {
            Ok(output) => return Ok(output),
            Err(e)
                if e.kind() == std::io::ErrorKind::ExecutableFileBusy
                    && *attempts < max_attempts =>
            {
                std::thread::sleep(delay);
            }
            Err(e) => return Err(e),
        }
    }
}

/// Run `<path> version`; returns its first output line, or the raw error text.
///
/// Zig spells this as a subcommand (`zig version`), not as a `--version` flag.
fn zig_runs(path: &Path) -> Result<String, String> {
    match exec_with_busy_retry(std::process::Command::new(path).arg("version")) {
        Ok(out) if out.status.success() => {
            let text = String::from_utf8_lossy(&out.stdout);
            let first = text.lines().next().unwrap_or_default().trim().to_string();
            Ok(if first.is_empty() {
                "ok".to_string()
            } else {
                first
            })
        }
        Ok(out) => Err(format!("`version` exited with {}", out.status)),
        Err(e) => Err(e.to_string()),
    }
}

/// The `release:` field of `rustc -vV` (`1.98.1`), or the raw error text.
///
/// `--version` is for humans; `-vV` is the machine-readable form, and its `release` field is what
/// a `rust-std` sysroot has to match (v0.9 F3b-2).
fn rustc_release(path: &Path) -> Result<String, String> {
    match exec_with_busy_retry(std::process::Command::new(path).arg("-vV")) {
        Ok(out) if out.status.success() => {
            let text = String::from_utf8_lossy(&out.stdout);
            text.lines()
                .find_map(|line| {
                    line.strip_prefix("release:")
                        .map(|value| value.trim().to_string())
                })
                .ok_or_else(|| "`rustc -vV` printed no `release:` line".to_string())
        }
        Ok(out) => Err(format!("`rustc -vV` exited with {}", out.status)),
        Err(e) => Err(e.to_string()),
    }
}

/// Does a machine release satisfy a pinned one?
///
/// Pure, so the rule is testable on a machine that has no `rustc` at all — which is exactly the
/// machine the first arm is for.
fn rust_release_matches(release: Option<&str>, pinned: &str) -> Result<(), String> {
    match release {
        None => Err(format!(
            "no rustc was found, and a rust-std {pinned} sysroot is only usable by the rustc that \
             produced it: install Rust from https://rustup.rs/ and try again"
        )),
        Some(found) if found == pinned => Ok(()),
        Some(found) => Err(format!(
            "the Rust sysroot offered is {pinned}, but this machine's rustc is {found}; a sysroot \
             is only usable by the release that produced it — install rustc {pinned}, or wait for a \
             pinned {found} sysroot"
        )),
    }
}

/// Run `<path> --version` for `rustc`; returns its first output line, or the raw error text.
///
/// The version matters more here than anywhere else: a Rust sysroot is only usable by the
/// `rustc` release that produced it, so the host reports both together (v0.9 F3b-1).
fn rust_runs(path: &Path) -> Result<String, String> {
    match exec_with_busy_retry(std::process::Command::new(path).arg("--version")) {
        Ok(out) if out.status.success() => {
            let text = String::from_utf8_lossy(&out.stdout);
            let first = text.lines().next().unwrap_or_default().trim().to_string();
            Ok(if first.is_empty() {
                "ok".to_string()
            } else {
                first
            })
        }
        Ok(out) => Err(format!("`--version` exited with {}", out.status)),
        Err(e) => Err(e.to_string()),
    }
}

/// Run `<path> --version`; returns its first output line, or the raw error text
/// (callers add their own, single, `not runnable:` prefix).
fn toolchain_runs(path: &Path) -> Result<String, String> {
    match exec_with_busy_retry(std::process::Command::new(path).arg("--version")) {
        Ok(out) if out.status.success() => {
            let text = String::from_utf8_lossy(&out.stdout);
            let first = text.lines().next().unwrap_or_default().trim().to_string();
            Ok(if first.is_empty() {
                "ok".to_string()
            } else {
                first
            })
        }
        Ok(out) => Err(format!("`--version` exited with {}", out.status)),
        Err(e) => Err(e.to_string()),
    }
}

/// LLM configuration (in memory only — never persisted).
#[derive(Clone)]
pub struct LlmConfigInput {
    pub provider_id: String,
    pub api_key: String,
    pub base_url: String,
    pub model: String,
}

impl std::fmt::Debug for LlmConfigInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LlmConfigInput")
            .field("provider_id", &self.provider_id)
            .field("api_key", &agent::config::mask_key(&self.api_key))
            .field("base_url", &self.base_url)
            .field("model", &self.model)
            .finish()
    }
}

/// Status of the LLM configuration (never includes the key).
#[derive(Debug, Clone, Serialize)]
pub struct LlmConfigStatus {
    pub configured: bool,
    pub provider_id: String,
    pub base_url: String,
    pub model: String,
    /// Whether the current key is stored in the OS keyring.
    pub persisted: bool,
}

/// A provider preset, shaped for the frontend.
#[derive(Debug, Clone, Serialize)]
pub struct ProviderPresetView {
    pub id: String,
    pub display_name: String,
    pub base_url: String,
    pub default_model: String,
    pub requires_key: bool,
    pub is_local: bool,
}

impl From<ProviderPreset> for ProviderPresetView {
    fn from(p: ProviderPreset) -> Self {
        Self {
            id: p.id,
            display_name: p.display_name,
            base_url: p.base_url,
            default_model: p.default_model,
            requires_key: p.requires_key,
            is_local: p.is_local,
        }
    }
}

/// Whether the LLM is ready to run, and why not (never includes the key).
#[derive(Debug, Clone, Serialize)]
pub struct LlmReadiness {
    pub ready: bool,
    /// `"no_config"` / `"missing_api_key"` / `"invalid_base_url"` / `"invalid_config"`.
    pub reason: Option<String>,
    /// Human-readable next step.
    pub suggestion: Option<String>,
}

/// A locally-detected OpenAI-compatible provider.
#[derive(Debug, Clone, Serialize)]
pub struct LocalProviderInfo {
    pub id: String,
    pub display_name: String,
    pub base_url: String,
    pub models: Vec<String>,
}

/// Result of probing localhost for local LLM servers.
#[derive(Debug, Clone, Serialize)]
pub struct LocalProbeResult {
    pub found: bool,
    pub providers: Vec<LocalProviderInfo>,
    pub probed: Vec<String>,
}

/// Timeout for each local probe request (must stay short).
pub const PROBE_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(1500);

/// Candidate local endpoints (id, display name, `/v1/models` URL).
const PROBE_TARGETS: &[(&str, &str, &str)] = &[
    (
        "ollama",
        "Ollama（本地）",
        "http://localhost:11434/v1/models",
    ),
    (
        "ollama",
        "Ollama（本地）",
        "http://127.0.0.1:11434/v1/models",
    ),
    (
        "lmstudio",
        "LM Studio（本地）",
        "http://localhost:1234/v1/models",
    ),
    (
        "lmstudio",
        "LM Studio（本地）",
        "http://127.0.0.1:1234/v1/models",
    ),
];

/// Build an [`AgentConfig`] view of a stored input (for validation only).
fn readiness_of(input: &LlmConfigInput) -> LlmReadiness {
    let config = AgentConfig {
        api_key: input.api_key.clone(),
        base_url: input.base_url.clone(),
        model: input.model.clone(),
        provider_id: input.provider_id.clone(),
        max_iterations: 10,
        request_timeout_secs: 120,
    };
    match config.validate() {
        Ok(()) => LlmReadiness {
            ready: true,
            reason: None,
            suggestion: None,
        },
        Err(e) => {
            let message = e.to_string();
            let (reason, suggestion) = if message.contains("api_key") {
                (
                    "missing_api_key",
                    "缺少 API Key。请填写，或切换到本地模型预设（如 Ollama）",
                )
            } else if message.contains("base_url") {
                (
                    "invalid_base_url",
                    "Base URL 无效，请检查（需以 http:// 或 https:// 开头）",
                )
            } else {
                ("invalid_config", "配置无效，请检查 Model 与服务商")
            };
            LlmReadiness {
                ready: false,
                reason: Some(reason.into()),
                suggestion: Some(suggestion.into()),
            }
        }
    }
}

/// `"<code>|<human message>"` — machine-readable code + readable hint.
pub fn readiness_error(r: &LlmReadiness) -> String {
    let code = r.reason.clone().unwrap_or_else(|| "not_ready".into());
    match &r.suggestion {
        Some(msg) if !msg.is_empty() => format!("{code}|{msg}"),
        _ => code,
    }
}

/// Audit chain status (serialisable view).
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "status")]
pub enum ChainStatusView {
    Intact { length: usize },
    Broken { at_id: i64, reason: String },
}

impl From<ChainStatus> for ChainStatusView {
    fn from(value: ChainStatus) -> Self {
        match value {
            ChainStatus::Intact { length } => ChainStatusView::Intact { length },
            ChainStatus::Broken { at_id, reason } => ChainStatusView::Broken { at_id, reason },
        }
    }
}

/// Audit status returned by `get_audit_status`.
#[derive(Debug, Clone, Serialize)]
pub struct AuditStatusView {
    pub count: usize,
    pub chain: ChainStatusView,
    /// Whether the audit-failure alert is on (v0.8). The setting, not the state:
    /// the event and the log line are sent either way.
    pub alert_on_failure: bool,
    /// Audit writes that failed and have not been shown yet (v0.8).
    /// `get_audit_status` takes them, so the panel is told once.
    pub failures: Vec<String>,
}

/// A stored audit event, shaped for the frontend.
#[derive(Debug, Clone, Serialize)]
pub struct StoredEventView {
    pub id: i64,
    pub timestamp_ms: i64,
    pub actor: String,
    pub action: String,
    pub detail: serde_json::Value,
    pub prev_hash: String,
    pub hash: String,
    /// Which agent caused the event (v0.8); `null` on rows written before it.
    pub agent_id: Option<String>,
}

impl From<StoredEvent> for StoredEventView {
    fn from(e: StoredEvent) -> Self {
        Self {
            id: e.id,
            timestamp_ms: e.event.timestamp_ms,
            actor: e.event.actor,
            action: e.event.action,
            detail: e.event.detail,
            prev_hash: e.prev_hash,
            hash: e.hash,
            agent_id: e.event.agent_id,
        }
    }
}

/// Result of a `run_agent` call.
#[derive(Debug, Clone, Serialize)]
pub struct AgentOutcomeView {
    pub kind: String,
    pub content: Option<String>,
    pub reason: Option<String>,
    pub iterations: u32,
}

impl From<AgentOutcome> for AgentOutcomeView {
    fn from(value: AgentOutcome) -> Self {
        match value {
            AgentOutcome::Final {
                content,
                iterations,
            } => Self {
                kind: "final".into(),
                content: Some(content),
                reason: None,
                iterations,
            },
            AgentOutcome::MaxIterations {
                last_content,
                iterations,
            } => Self {
                kind: "max_iterations".into(),
                content: Some(last_content),
                reason: None,
                iterations,
            },
            AgentOutcome::Failed { reason, iterations } => Self {
                kind: "failed".into(),
                content: None,
                reason: Some(reason),
                iterations,
            },
        }
    }
}

/// Shared, thread-safe application state.
pub struct AppState {
    pub audit: Arc<Mutex<AuditStore>>,
    pub sink: Arc<Mutex<dyn AuditSink>>,
    /// Audit writes that failed after the retries, waiting to be surfaced
    /// (v0.8). The sink's reporter pushes here, so a failure raised inside the
    /// sandbox or the agent loop — not only inside the host — reaches the UI.
    audit_failures: Arc<Mutex<Vec<String>>>,
    /// How many of them an [`EventSink`] has already been told about, so the
    /// `audit:failed` event is not repeated on every refresh.
    audit_failures_emitted: Arc<AtomicUsize>,
    /// This instance's agent identity (v0.8 batch B): `<device>-<pid>-<seq>`.
    ///
    /// Every event this instance writes carries it, so one chain can say which
    /// agent caused what once several agents write to it. Minted once, at
    /// construction: the host is the agent, not the run.
    agent_id: String,
    /// Host-owned VM slot: a VM started during a run stays here after the run
    /// ends, so later runs reuse the same guest (v0.2 host-owned lifecycle).
    pub vm_slot: Arc<Mutex<Option<RiscVVirtualMachine>>>,
    /// User-chosen RISC-V GCC (`set_toolchain_path`); `None` means auto-discovery.
    pub toolchain_path: Mutex<Option<PathBuf>>,
    /// User-chosen Zig executable (`set_zig_path`, v0.9 F3a); `None` means
    /// auto-discovery. Parallel to [`Self::toolchain_path`] rather than a map: the
    /// language follows the source extension, so one sandbox pins **two** compilers.
    pub zig_path: Mutex<Option<PathBuf>>,
    /// User-chosen Rust sysroot (`set_rust_sysroot`, v0.9 F3b-1); `None` means the
    /// environment. The third single value — and the only one that names a *directory*, since
    /// what Rust needs from us is the target's `core`, not an executable.
    pub rust_sysroot: Mutex<Option<PathBuf>>,
    /// User-chosen QEMU (`set_qemu_path`); `None` means auto-discovery.
    pub qemu_path: Mutex<Option<PathBuf>>,
    /// In-flight toolchain download (v0.3 #3b). `Arc` so the worker thread can
    /// be handed the cancel flag and clear the slot when it finishes.
    pub toolchain_download: Arc<Mutex<Option<ToolchainDownloadState>>>,
    /// In-flight QEMU download (v0.9 sandbox F1); the same shape, including the
    /// `Arc` for the same reason.
    pub qemu_download: Arc<Mutex<Option<QemuDownloadState>>>,
    /// In-flight sandbox switch (v0.9 sandbox F2b). One at a time, world-wide:
    /// two switches would race for the same VM slot.
    switch_slot: Mutex<Option<SandboxSwitchState>>,
    /// The sandbox this node is **running now** (v0.9 sandbox F2b).
    ///
    /// Runtime state, never written to `settings.json`: `default_sandbox` is what
    /// a restart starts from, this is what a switch changed. `None` means nothing
    /// was switched, so the default — or the fallback — is what a run would use.
    ///
    /// `Arc` since F2c: the request queue reads it through the same handle, to
    /// answer the AI's `sandbox_status` tool without an `Arc<AppState>` cycle.
    current_sandbox: Arc<Mutex<Option<String>>>,
    /// The sandbox requests waiting for a decision (v0.9 sandbox F2c).
    ///
    /// A queue, not a slot: several asks may wait at once. Cloned into the agent
    /// loop's tool gateway, which is why it is its own (cloneable) struct.
    sandbox_requests: SandboxRequests,
    /// The definition the **running VM** came from (v0.9 sandbox F2d).
    ///
    /// Distinct from `current_sandbox`, which is what a *switch* put in charge and
    /// which a task-declared run must not change. This one answers "if a VM is in
    /// the slot, where did it come from", so a task declaring a different sandbox
    /// can be refused instead of silently running against the wrong guest. `None`
    /// means nothing is running, or what is running was not started from a
    /// definition (a restored snapshot on a node with no current sandbox).
    active_sandbox: Mutex<Option<String>>,
    /// When the host-owned VM started (epoch ms); shared with the audit bridge,
    /// which learns about VM starts/stops from the sandbox's audit events.
    vm_started_at_ms: Arc<Mutex<Option<i64>>>,
    /// Last download event seen, kept after the download ends (for polling).
    toolchain_download_last: Mutex<Option<DownloadEvent>>,
    /// The same for QEMU.
    qemu_download_last: Mutex<Option<crate::qemu_download::QemuDownloadEvent>>,
    /// Non-secret local settings mirrored to `settings.json`.
    settings: Mutex<LocalSettings>,
    /// The executor handles a task is routed to (v0.9 interface E0).
    ///
    /// Built from `settings.executors` after the settings file is read. The
    /// handles are pure data until a task runs — `StdioExecutorHandle::new` spawns
    /// nothing, the child appears in `run` — so registration has **no side
    /// effect** and needs no lazy init. The node itself is deliberately **not**
    /// here: `/v0/agent/run` is how a caller runs on this node, and `/v0/tasks`
    /// reaches only the fleet this node was configured with.
    executors: Mutex<Vec<Arc<dyn AgentHandle>>>,
    /// Where `settings.json` lives.
    settings_path: PathBuf,
    /// The data directory this instance owns (v0.8). The sessions DB and the
    /// downloaded toolchains live here too. Injecting it is what lets two
    /// `AppState`s in one process keep their own data — before v0.8 it was
    /// resolved from a process-wide `OnceLock`, so only the first instance's
    /// directory ever took effect.
    data_dir: PathBuf,
    pub llm_config: Mutex<Option<LlmConfigInput>>,
    pub workspace_root: PathBuf,
    pub compiler: agent::CompilerConfig,
    /// Test seam: an injected LLM client (bypasses HTTP).
    pub llm_override: Mutex<Option<Arc<dyn LlmClient>>>,
    /// OS keyring backend (or an in-memory one in tests).
    pub keyring: Arc<dyn KeyringBackend>,
    /// Whether the current key has been persisted to the keyring.
    persisted: Mutex<bool>,
    /// Serial broadcast list shared by the long-lived forwarder and every run.
    /// The forwarder owns the matching `Receiver`.
    pub serial_senders: Arc<Mutex<Vec<std::sync::mpsc::Sender<Vec<u8>>>>>,
    /// Accumulated serial text from the push stream (for `get_serial_buffer`).
    pub serial_accum: Arc<Mutex<String>>,
    /// Live LLM stream receiver for the current run.
    pub stream_receiver: Arc<Mutex<Option<Receiver<StreamEvent>>>>,
    /// Persisted conversations.
    pub sessions: Arc<Mutex<SessionStore>>,
    /// The session the next run appends to (created on demand).
    pub current_session_id: Mutex<Option<String>>,
    /// The run currently in progress, if any (host-owned provenance, v0.4 1c).
    current_run_id: Mutex<Option<String>>,
    /// The most recent run, finished or not. The VM outlives a run, so a
    /// snapshot is usually saved between runs: this is the run a later restore
    /// links to as its parent.
    last_run_id: Mutex<Option<String>>,
    /// Which run produced each saved snapshot (name -> run id), for the lifetime
    /// of this process. A restore after a restart cannot recover the producing
    /// run from the migration stream and records no parent (v0.4 1c).
    snapshot_producers: Mutex<HashMap<String, String>>,
    /// How far the chain had grown when this process opened the store (v0.4 1e).
    /// Runs starting beyond it belong to this process and may still be running,
    /// so the abandoned-run hook never touches them.
    startup_seq: i64,
}

/// Messages restored into a fresh `AgentLoop` (never the system prompt).
const HISTORY_LIMIT: usize = 100;

/// A snapshot on disk, shaped for the frontend.
#[derive(Debug, Clone, Serialize)]
pub struct SnapshotMetaView {
    pub name: String,
    pub size_bytes: u64,
    pub created_at_ms: i64,
    /// `"tcp-relay"` (real, migration stream) or `"reboot-fallback"` (JSON).
    pub mode: String,
}

/// One run from the derived index, shaped for the frontend (v0.4 1d).
///
/// Read-only: the run's history lives in the audit chain, and this is what the
/// chain says about it.
#[derive(Debug, Clone, Serialize)]
pub struct RunView {
    pub run_id: String,
    /// `open` / `ok` / `failed` / `interrupted` / `abandoned`.
    pub status: String,
    /// The full configuration digest (64 hex characters).
    pub fingerprint: String,
    /// The first 16 hex characters, for display.
    pub fingerprint_short: String,
    pub parent_run_id: Option<String>,
    pub session_id: Option<String>,
    /// The snapshot this run was restored from, when it was a restore
    /// (v0.5 batch 3). `None` for a run that started from scratch.
    pub resumed_from_snapshot: Option<String>,
    pub started_at_ms: i64,
    pub ended_at_ms: Option<i64>,
}

impl From<&audit::RunRecord> for RunView {
    fn from(record: &audit::RunRecord) -> Self {
        Self {
            run_id: record.run_id.clone(),
            status: record.status.as_str().to_string(),
            fingerprint: record.fingerprint.clone(),
            fingerprint_short: audit::short_fingerprint(&record.fingerprint).to_string(),
            parent_run_id: record.parent_run_id.clone(),
            session_id: record.session_id.clone(),
            resumed_from_snapshot: record.resumed_from_snapshot.clone(),
            started_at_ms: record.started_at_ms,
            ended_at_ms: record.ended_at_ms,
        }
    }
}

/// A session plus its messages, for `open_session`.
#[derive(Debug, Clone, Serialize)]
pub struct SessionDetailView {
    pub meta: SessionMeta,
    pub messages: Vec<SessionMessage>,
}

/// Title derived from the first user input (first 60 characters).
fn title_from(user_input: &str) -> String {
    let trimmed = user_input.trim();
    let title: String = trimmed.chars().take(60).collect();
    if title.is_empty() {
        "新会话".to_string()
    } else {
        title
    }
}

/// Convert a persisted row back into a chat message (system rows are skipped).
fn history_to_chat(row: &SessionMessage) -> Option<ChatMessage> {
    match row.role.as_str() {
        "user" => Some(ChatMessage::text("user", row.content.clone())),
        "assistant" => Some(ChatMessage {
            role: "assistant".to_string(),
            content: if row.content.is_empty() {
                None
            } else {
                Some(row.content.clone())
            },
            tool_calls: row
                .tool_call_json
                .as_ref()
                .and_then(|json| serde_json::from_str(json).ok()),
            tool_call_id: None,
        }),
        "tool" => Some(ChatMessage::tool_result(
            row.tool_call_id.clone().unwrap_or_default(),
            row.content.clone(),
        )),
        _ => None,
    }
}

impl AppState {
    /// Build state backed by an on-disk audit DB under `<workspace>/.riscdom`.
    pub fn new(workspace_root: impl Into<PathBuf>) -> Result<Self, HostError> {
        let root = workspace_root.into();
        std::fs::create_dir_all(&root)?;
        let db_dir = root.join(".riscdom");
        std::fs::create_dir_all(&db_dir)?;
        let store = AuditStore::open(&db_dir.join("audit.db"))?;
        let state = Self::from_store(
            root,
            store,
            Arc::new(OsKeyring::new()),
            SessionStore::open(&crate::paths::sessions_db_path())
                .map_err(|e| HostError::Other(format!("session store: {e}")))?,
        );
        state.init_from_env();
        state.load_settings();
        state.register_executors();
        Ok(state)
    }

    /// Build state whose data directory is **injected** rather than taken from
    /// the process-wide default (v0.8 technical debt).
    ///
    /// `settings.json`, the sessions DB and the toolchain download directory all
    /// resolve inside `data_dir`, so two instances built this way never share
    /// state — the multi-agent runtime's precondition. The audit DB still lives
    /// under the workspace (`<workspace>/.riscdom/audit.db`), as it always has.
    pub fn with_data_dir(
        workspace_root: impl Into<PathBuf>,
        data_dir: impl Into<PathBuf>,
    ) -> Result<Self, HostError> {
        let root = workspace_root.into();
        let data_dir = data_dir.into();
        std::fs::create_dir_all(&root)?;
        let db_dir = root.join(".riscdom");
        std::fs::create_dir_all(&db_dir)?;
        std::fs::create_dir_all(&data_dir)?;
        let store = AuditStore::open(&db_dir.join("audit.db"))?;
        let sessions = SessionStore::open(&crate::paths::sessions_db_path_in(&data_dir))
            .map_err(|e| HostError::Other(format!("session store: {e}")))?;
        let mut state = Self::from_store(root, store, Arc::new(OsKeyring::new()), sessions);
        state.data_dir = data_dir;
        state.settings_path = crate::paths::settings_path_in(&state.data_dir);
        state.init_from_env();
        state.load_settings();
        state.register_executors();
        Ok(state)
    }

    /// Build state with a private in-memory audit DB and keyring (tests).
    pub fn in_memory(workspace_root: impl Into<PathBuf>) -> Result<Self, HostError> {
        let root = workspace_root.into();
        std::fs::create_dir_all(&root)?;
        let store = AuditStore::in_memory()?;
        let mut state = Self::from_store(
            root.clone(),
            store,
            Arc::new(InMemoryKeyring::new()),
            SessionStore::in_memory()
                .map_err(|e| HostError::Other(format!("session store: {e}")))?,
        );
        // Tests stay hermetic: settings live inside the temp workspace.
        state.settings_path = root.join(".riscdom").join("settings.json");
        state.load_settings();
        state.register_executors();
        Ok(state)
    }

    /// Turn `settings.executors` into the handles a task is routed to (v0.9
    /// interface E0).
    ///
    /// Called once per constructor, after `load_settings`. Nothing is spawned
    /// here: `StdioExecutorHandle::new` only records what to run, so a node with a
    /// fleet configured starts no children until a task actually arrives. A
    /// program that does not exist is therefore **not** an error at startup — it
    /// is the first task's failure, which is the honest place for it (a path may
    /// be replaced between the two moments, and a discovery scan is not what this
    /// is).
    fn register_executors(&self) {
        let specs = match self.settings.lock() {
            Ok(settings) => settings.executors.clone(),
            Err(_) => return,
        };
        // `LocalDispatcher` is the same holder the worker's supervisor builds;
        // the node reuses it rather than keeping a second routing implementation.
        // The handles are kept, not a dispatcher: `dispatch_task_value` builds one
        // on demand, which is the single place a future runtime registration
        // (not in v0.9) would have to be visible to.
        let handles: Vec<Arc<dyn AgentHandle>> = specs
            .iter()
            .map(|spec| {
                Arc::new(crate::executor::StdioExecutorHandle::new(
                    AgentId::new(spec.label.clone()),
                    spec.program.clone(),
                    spec.args.clone(),
                )) as Arc<dyn AgentHandle>
            })
            .collect();
        if let Ok(mut held) = self.executors.lock() {
            *held = handles;
        }
    }

    /// The identities `POST /v0/tasks` can reach — the configured fleet, in
    /// configuration order.
    pub fn executors(&self) -> Vec<String> {
        match self.executors.lock() {
            Ok(held) => held
                .iter()
                .map(|handle| handle.agent_id().as_str().to_string())
                .collect(),
            Err(_) => Vec::new(),
        }
    }

    /// Dispatch one task to the executor its `target` names (v0.9 interface E0).
    ///
    /// `id` is the caller's `Task.id` when it sent one; a task without one gets a
    /// fresh [`TaskId`], because the id is what the executor echoes back and what
    /// a reader uses to match the two halves.
    ///
    /// The refusal ladder is the dispatcher's, translated to the host's vocabulary
    /// so the endpoint can answer `404 cause "target"` for a target nobody owns and
    /// `500 cause "task"` for a dispatch that broke — two different things, and a
    /// run that merely *failed* is neither (it comes back as an `Ok` outcome whose
    /// `outcome` is `failed`).
    pub fn dispatch_task(
        &self,
        target: &str,
        input: &str,
        sandbox: Option<&str>,
        id: Option<&str>,
    ) -> Result<TaskOutcome, HostError> {
        let task = Task {
            id: id.map(TaskId::new).unwrap_or_else(TaskId::next),
            target: AgentId::new(target),
            input: input.to_string(),
            sandbox: sandbox.map(str::to_string),
        };
        self.dispatch_task_value(task)
    }

    /// [`dispatch_task`](Self::dispatch_task) for a task a caller already built.
    pub fn dispatch_task_value(&self, task: Task) -> Result<TaskOutcome, HostError> {
        let dispatcher = match self.executors.lock() {
            Ok(held) => LocalDispatcher::new(held.clone()),
            Err(_) => {
                return Err(HostError::TaskFailed(
                    "the executor list is poisoned".into(),
                ))
            }
        };
        match dispatcher.dispatch(task) {
            Ok(outcome) => Ok(outcome),
            Err(DispatchError::NoSuchAgent(agent)) => {
                Err(HostError::NoSuchExecutor(agent.as_str().to_string()))
            }
            Err(error) => Err(HostError::TaskFailed(error.to_string())),
        }
    }

    /// Dev convenience: adopt `DEEPSEEK_API_KEY` into **memory only**.
    ///
    /// Never writes the keyring (so a user's environment variable is never
    /// persisted by surprise) and never marks the config as `persisted`.
    fn init_from_env(&self) {
        let api_key = match std::env::var("DEEPSEEK_API_KEY") {
            Ok(v) if !v.trim().is_empty() => v,
            _ => return,
        };
        let base_url = std::env::var("DEEPSEEK_BASE_URL")
            .unwrap_or_else(|_| "https://api.deepseek.com".to_string());
        let model = std::env::var("DEEPSEEK_MODEL").unwrap_or_else(|_| "deepseek-chat".to_string());

        let input = LlmConfigInput {
            provider_id: DEFAULT_PRESET_ID.to_string(),
            api_key,
            base_url,
            model,
        };
        if readiness_of(&input).ready {
            self.set_llm_config(input);
        }
    }

    /// Override the keyring backend.
    pub fn with_keyring(mut self, keyring: Arc<dyn KeyringBackend>) -> Self {
        self.keyring = keyring;
        self
    }

    fn from_store(
        root: PathBuf,
        store: AuditStore,
        keyring: Arc<dyn KeyringBackend>,
        sessions: SessionStore,
    ) -> Self {
        let shared = Arc::new(Mutex::new(store));
        // Everything the chain holds right now belongs to earlier processes (or
        // to nothing at all): this is the boundary the abandoned-run hook uses.
        let startup_seq = shared
            .lock()
            .ok()
            .and_then(|store| store.all().ok())
            .and_then(|events| events.last().map(|e| e.id))
            .unwrap_or(0);
        let audit_failures: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let sink: Arc<Mutex<dyn AuditSink>> = {
            // v0.8: a write that fails after the retries is queued in
            // `audit_failures` and surfaced by the host (log + `audit:failed`
            // event + the audit tab's alert). The reporter runs with the store
            // lock released.
            let reporter_queue = Arc::clone(&audit_failures);
            let reporter: audit::AuditFailureReporter =
                Arc::new(move |error: &audit::AuditError| {
                    audit::report_failure(error);
                    if let Ok(mut failures) = reporter_queue.lock() {
                        failures.push(error.to_string());
                        if failures.len() > AUDIT_FAILURE_QUEUE_CAP {
                            let excess = failures.len() - AUDIT_FAILURE_QUEUE_CAP;
                            failures.drain(0..excess);
                        }
                    }
                });
            Arc::new(Mutex::new(
                SqliteAuditSink::from_shared(Arc::clone(&shared)).with_reporter(reporter),
            ))
        };
        // The running sandbox is read by two places that must not see different
        // answers (the switch, and the request queue's status text), so they share
        // one handle (v0.9 sandbox F2c).
        let current_sandbox: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
        let state = Self {
            audit: shared,
            sink,
            audit_failures,
            audit_failures_emitted: Arc::new(AtomicUsize::new(0)),
            agent_id: agent::next_agent_id(),
            vm_slot: Arc::new(Mutex::new(None)),
            toolchain_path: Mutex::new(None),
            zig_path: Mutex::new(None),
            rust_sysroot: Mutex::new(None),
            qemu_path: Mutex::new(None),
            toolchain_download: Arc::new(Mutex::new(None)),
            qemu_download: Arc::new(Mutex::new(None)),
            switch_slot: Mutex::new(None),
            current_sandbox: Arc::clone(&current_sandbox),
            sandbox_requests: SandboxRequests::new(current_sandbox),
            active_sandbox: Mutex::new(None),
            vm_started_at_ms: Arc::new(Mutex::new(None)),
            toolchain_download_last: Mutex::new(None),
            qemu_download_last: Mutex::new(None),
            settings: Mutex::new(LocalSettings::default()),
            executors: Mutex::new(Vec::new()),
            settings_path: crate::paths::settings_path(),
            data_dir: crate::paths::default_data_dir(),
            llm_config: Mutex::new(None),
            workspace_root: root,
            compiler: agent::CompilerConfig::from_env(),
            llm_override: Mutex::new(None),
            keyring,
            persisted: Mutex::new(false),
            serial_senders: Arc::new(Mutex::new(Vec::new())),
            serial_accum: Arc::new(Mutex::new(String::new())),
            stream_receiver: Arc::new(Mutex::new(None)),
            sessions: Arc::new(Mutex::new(sessions)),
            current_session_id: Mutex::new(None),
            current_run_id: Mutex::new(None),
            last_run_id: Mutex::new(None),
            snapshot_producers: Mutex::new(HashMap::new()),
            startup_seq,
        };
        // Startup hook (v0.4 1e): make runs left open by a previous process
        // legible. Best effort — it must never stop the app from starting.
        let _ = state.abandon_stale_runs();
        // Startup hygiene (v0.4 batch 5/6): whatever a killed process left in the
        // temp directory is removed once it is old enough that nothing can still
        // own it. `<temp>/riscdom` — the fallback data directory — is never a
        // target, and only directories are ever removed.
        let swept = agent::sweep_stale_temp_dirs_with_age(agent::TEMP_DIR_MAX_AGE);
        if swept > 0 {
            eprintln!("startup hygiene: removed {swept} stale temp director(ies)");
        }
        state
    }

    // ----- Snapshots --------------------------------------------------------

    /// This instance's snapshot directory: `<workspace>/.riscdom/snapshots/<agent_id>`.
    ///
    /// Per agent (v0.8 batch B): several agents may share one workspace, and a
    /// shared directory meant two agents saving `snap1` overwrote each other. New
    /// writes always land here; reads fall back to [`Self::snapshot_root`] so
    /// snapshots taken before this change stay usable.
    pub fn snapshot_dir(&self) -> PathBuf {
        self.snapshot_root().join(&self.agent_id)
    }

    /// The directory every agent's snapshot subdirectory lives under
    /// (`<workspace>/.riscdom/snapshots`) — where an older version wrote them.
    pub fn snapshot_root(&self) -> PathBuf {
        self.workspace_root.join(".riscdom").join("snapshots")
    }

    /// Find `name` on disk: this agent's directory first, then the shared root
    /// (a snapshot written before the per-agent layout).
    fn find_snapshot(&self, name: &str) -> Option<PathBuf> {
        for dir in [self.snapshot_dir(), self.snapshot_root()] {
            for ext in [sandbox::SNAPSHOT_MIG_EXT, sandbox::SNAPSHOT_JSON_EXT] {
                let path = dir.join(format!("{name}.{ext}"));
                if path.is_file() {
                    return Some(path);
                }
            }
        }
        None
    }

    /// Snapshots present on disk: real (`.mig`) and reboot-fallback (`.json`).
    ///
    /// Both this agent's directory and the shared root are listed — the root so a
    /// pre-v0.8 snapshot stays visible, this agent's directory first so it wins on
    /// a name collision.
    pub fn list_snapshots(&self) -> Result<Vec<SnapshotMetaView>, HostError> {
        let mut out = Vec::new();
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        for dir in [self.snapshot_dir(), self.snapshot_root()] {
            Self::list_snapshot_dir(&dir, &mut seen, &mut out)?;
        }
        out.sort_by_key(|s| std::cmp::Reverse(s.created_at_ms));
        Ok(out)
    }

    /// Collect the snapshots of one directory into `out`, skipping names already
    /// seen (so the per-agent entry shadows the shared one).
    fn list_snapshot_dir(
        dir: &Path,
        seen: &mut std::collections::HashSet<String>,
        out: &mut Vec<SnapshotMetaView>,
    ) -> Result<(), HostError> {
        if !dir.exists() {
            return Ok(());
        }
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let (name, mode) = match path.extension().and_then(|e| e.to_str()) {
                Some(sandbox::SNAPSHOT_MIG_EXT) => (
                    path.file_stem()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string(),
                    SNAPSHOT_MODE,
                ),
                Some(sandbox::SNAPSHOT_JSON_EXT) => (
                    path.file_stem()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string(),
                    SNAPSHOT_FALLBACK_MODE,
                ),
                _ => continue,
            };
            if !seen.insert(name.clone()) {
                continue;
            }
            let meta = entry.metadata()?;
            let created_at_ms = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0);
            out.push(SnapshotMetaView {
                name,
                size_bytes: meta.len(),
                created_at_ms,
                mode: mode.to_string(),
            });
        }
        Ok(())
    }

    /// Delete a snapshot (either mode) from this agent's directory and from the
    /// shared root. Returns whether a file was removed.
    pub fn delete_snapshot(&self, name: &str) -> Result<bool, HostError> {
        let mut removed = false;
        for dir in [self.snapshot_dir(), self.snapshot_root()] {
            for ext in [sandbox::SNAPSHOT_MIG_EXT, sandbox::SNAPSHOT_JSON_EXT] {
                let path = dir.join(format!("{name}.{ext}"));
                if path.is_file() {
                    std::fs::remove_file(&path)?;
                    removed = true;
                }
            }
        }
        self.emit_host(
            "host.snapshot.delete",
            serde_json::json!({ "name": name, "removed": removed }),
        );
        Ok(removed)
    }

    // ----- Real snapshots (tcp relay, stage 20c) ----------------------------

    /// Reject names that could escape the snapshot directory.
    fn validate_snapshot_name(name: &str) -> Result<(), HostError> {
        let ok = !name.is_empty()
            && name.len() <= 64
            && name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
        if ok {
            Ok(())
        } else {
            Err(HostError::Other(format!("invalid snapshot name: {name}")))
        }
    }

    /// Kernel ELF for a live restore: the newest `*.elf` in the workspace.
    fn resume_kernel(&self) -> Result<PathBuf, HostError> {
        let mut best: Option<(std::time::SystemTime, PathBuf)> = None;
        for entry in std::fs::read_dir(&self.workspace_root)? {
            let entry = entry?;
            let path = entry.path();
            if !path.is_file() || path.extension().and_then(|e| e.to_str()) != Some("elf") {
                continue;
            }
            let Ok(modified) = entry.metadata().and_then(|m| m.modified()) else {
                continue;
            };
            if best.as_ref().map(|(t, _)| modified > *t).unwrap_or(true) {
                best = Some((modified, path));
            }
        }
        best.map(|(_, p)| p).ok_or_else(|| {
            HostError::Other("no compiled ELF in the workspace; compile one first".into())
        })
    }

    /// Save a **real** (tcp-relay) snapshot of the host-owned VM.
    ///
    /// Returns the snapshot size in bytes. Errors when no VM is running.
    pub fn save_snapshot_real(&self, name: &str) -> Result<u64, HostError> {
        Self::validate_snapshot_name(name)?;
        {
            let mut slot = self
                .vm_slot
                .lock()
                .map_err(|_| HostError::Other("vm slot poisoned".into()))?;
            let vm = slot
                .as_mut()
                .ok_or_else(|| HostError::Other("no running vm".into()))?;
            vm.save_snapshot_real(name)
                .map_err(|e| HostError::Other(e.to_string()))?;
        }
        let bytes = self
            .find_snapshot(name)
            .and_then(|path| std::fs::metadata(path).ok())
            .map(|m| m.len())
            .unwrap_or(0);
        // Remember which run produced this snapshot, so a restore later in this
        // process can link to it (v0.4 1c). The VM outlives a run, so the run to
        // link to is the most recent one, not necessarily an in-flight run.
        let producer = self
            .current_run_id
            .lock()
            .ok()
            .and_then(|slot| slot.clone())
            .or_else(|| self.last_run_id.lock().ok().and_then(|slot| slot.clone()));
        if let Some(run_id) = producer {
            if let Ok(mut producers) = self.snapshot_producers.lock() {
                producers.insert(name.to_string(), run_id);
            }
        }
        self.emit_host(
            "host.snapshot.save",
            serde_json::json!({ "name": name, "bytes": bytes, "mode": SNAPSHOT_MODE }),
        );
        Ok(bytes)
    }

    /// Restore the host-owned VM from a real snapshot (`-incoming` + relay).
    ///
    /// The current VM (if any) is stopped first; the restored VM stays in the
    /// slot so the next run keeps using it.
    pub fn resume_from_snapshot_real(&self, name: &str) -> Result<(), HostError> {
        Self::validate_snapshot_name(name)?;
        // This agent's directory first, then the shared root: a snapshot taken
        // before the per-agent layout is still restorable (v0.8 batch B).
        let path = self
            .find_snapshot(name)
            .ok_or_else(|| HostError::Other(format!("snapshot not found: {name}")))?;

        // Run provenance (v0.4 1c): a restore is its own run (§4.3) — it is a
        // materially different execution — linked to the run that produced the
        // snapshot whenever this process still knows it.
        let parent = self
            .snapshot_producers
            .lock()
            .ok()
            .and_then(|producers| producers.get(name).cloned());
        let run_id = self.begin_run(None, parent.as_deref(), Some(name));

        let restored = (|| -> Result<(), HostError> {
            // Both ports come from the process-wide lease (v0.4 #1) and are handed
            // to QEMU only right before it starts.
            let mut leases = sandbox::relay::lease_local_ports(2)
                .map_err(|e| HostError::Other(e.to_string()))?;
            let mut serial_lease = leases.pop().expect("two leases were requested");
            let mut qmp_lease = leases.pop().expect("two leases were requested");
            let config = VMConfig {
                kernel: self.resume_kernel()?,
                memory_mb: agent::VM_MEMORY_MB,
                qmp: QmpEndpoint::tcp("127.0.0.1", qmp_lease.port()),
                serial: SerialEndpoint::tcp("127.0.0.1", serial_lease.port()),
                snapshot_dir: self.snapshot_dir(),
                serial_observer: Some(agent::tools::serial_observer_for(Arc::clone(
                    &self.serial_senders,
                ))),
                incoming_snapshot: Some(path.clone()),
                incoming_relay_addr: None,
                // Honour the user's manual QEMU here too (v0.3.1 #1): a restore
                // used to fall back to auto-discovery and could boot with a
                // different binary than the one configured in *Settings → Toolchain*.
                qemu_exe: self.manual_qemu_path(),
            };
            self.stop_current_vm()?;
            qmp_lease.hand_off();
            serial_lease.hand_off();
            let vm = RiscVVirtualMachine::resume_from_snapshot_real(
                config,
                &path,
                Arc::clone(&self.sink),
            )
            .map_err(|e| HostError::Other(e.to_string()))?;
            *self
                .vm_slot
                .lock()
                .map_err(|_| HostError::Other("vm slot poisoned".into()))? = Some(vm);
            // A restored VM is a "fresh" one for the status badge.
            self.clear_vm_started();
            self.mark_vm_started();
            // Its provenance is the node's own sandbox: the snapshot came from this
            // node's VM, so whichever definition the node is in charge of is the
            // honest answer (v0.9 sandbox F2d). `stop_current_vm` cleared the slot
            // above, so this is not left over from the VM that was replaced.
            self.mark_active_sandbox(self.current_sandbox().as_deref());
            Ok(())
        })();

        if let Some(run_id) = &run_id {
            let (status, reason) = match &restored {
                Ok(()) => (audit::RunStatus::Ok, "snapshot restored".to_string()),
                Err(e) => (
                    audit::RunStatus::Failed,
                    format!("snapshot restore failed: {e}"),
                ),
            };
            self.finish_run(run_id, status, &reason);
        }
        restored?;

        self.emit_host(
            "host.snapshot.resume",
            serde_json::json!({ "name": name, "mode": SNAPSHOT_MODE }),
        );
        Ok(())
    }

    // ----- Sessions ---------------------------------------------------------

    // ----- Toolchain (stage 24b) --------------------------------------------

    // ----- Toolchain download (v0.3 #3b) ------------------------------------

    /// Claim the download slot. Errors when a download is already running.
    ///
    /// Also the **gate** for the one pin whose product is version-coupled: a `rust-std` sysroot
    /// is only usable by the `rustc` that produced it, so a Rust download is refused here — before
    /// any bytes move — when this machine's `rustc` reports a different release (v0.9 F3b-2,
    /// decision §52). Every edge goes through this method, so the check cannot be bypassed.
    pub fn begin_toolchain_download(
        &self,
        spec: &crate::toolchain_download::DownloadSpec,
    ) -> Result<Arc<AtomicBool>, HostError> {
        if spec.toolchain == crate::toolchain_download::Toolchain::Rust {
            rust_release_matches(self.rust_release().as_deref(), &spec.version)
                .map_err(HostError::Other)?;
        }

        let mut slot = self
            .toolchain_download
            .lock()
            .map_err(|_| HostError::Other("download lock poisoned".into()))?;
        if slot.is_some() {
            return Err(HostError::Other("download already in progress".into()));
        }
        let cancel = Arc::new(AtomicBool::new(false));
        *slot = Some(ToolchainDownloadState {
            cancel: Arc::clone(&cancel),
            started_at: std::time::Instant::now(),
            toolchain: spec.toolchain,
        });
        drop(slot);
        self.emit_host(
            "host.toolchain.download.start",
            serde_json::json!({ "version": spec.version, "toolchain": spec.toolchain.label() }),
        );
        Ok(cancel)
    }

    /// Ask an in-flight download to stop.
    pub fn cancel_toolchain_download(&self) -> Result<(), HostError> {
        let slot = self
            .toolchain_download
            .lock()
            .map_err(|_| HostError::Other("download lock poisoned".into()))?;
        match slot.as_ref() {
            Some(state) => {
                state.cancel.store(true, Ordering::Relaxed);
                Ok(())
            }
            None => Err(HostError::Other("no download in progress".into())),
        }
    }

    /// Current download status (also valid when idle: the last event is kept).
    pub fn toolchain_download_status(&self) -> ToolchainDownloadStatus {
        let running = self
            .toolchain_download
            .lock()
            .ok()
            .and_then(|slot| slot.as_ref().map(|state| state.toolchain));
        let last_event = self
            .toolchain_download_last
            .lock()
            .ok()
            .and_then(|event| event.clone());
        ToolchainDownloadStatus {
            in_progress: running.is_some(),
            toolchain: running,
            last_event,
        }
    }

    /// Record one download event (progress reporting + polling).
    pub fn record_download_event(&self, event: DownloadEvent) {
        if let Ok(mut last) = self.toolchain_download_last.lock() {
            *last = Some(event);
        }
    }

    /// Release the download slot (called when the worker finishes).
    pub fn finish_toolchain_download(&self) {
        if let Ok(mut slot) = self.toolchain_download.lock() {
            *slot = None;
        }
    }

    /// Download, verify and install, then adopt the compiler as the active one.
    ///
    /// The caller owns the Tauri side (spawning + event emission); tests call
    /// this directly. `on_event` is invoked for every progress event in addition
    /// to the internal bookkeeping.
    pub fn download_toolchain_now(
        &self,
        spec: &crate::toolchain_download::DownloadSpec,
        dest_root: &Path,
        cancel: Arc<AtomicBool>,
        on_event: &mut dyn FnMut(DownloadEvent),
    ) -> Result<PathBuf, HostError> {
        let mut forward = |event: DownloadEvent| {
            self.record_download_event(event.clone());
            on_event(event);
        };
        let result =
            crate::toolchain_download::download_and_install(spec, dest_root, &cancel, &mut forward);

        match result {
            Ok(compiler) => {
                let path = compiler.display().to_string();
                // Which "adopt" call this is follows the spec (v0.9 F3a-download-apply): the C
                // toolchain replaces `toolchain_path`, Zig replaces `zig_path`, and Rust replaces
                // `rust_sysroot` — with a **directory** instead of an executable, because that is
                // what a Rust sysroot is (v0.9 F3b-2). All three are single values, so none can
                // disturb the others.
                let adopted = match spec.toolchain {
                    crate::toolchain_download::Toolchain::C => self.set_toolchain_path(&path),
                    crate::toolchain_download::Toolchain::Zig => self.set_zig_path(&path),
                    crate::toolchain_download::Toolchain::Rust => self.set_rust_sysroot(&path),
                };
                self.finish_toolchain_download();
                match adopted {
                    Ok(()) => {
                        self.emit_host(
                            "host.toolchain.download.done",
                            serde_json::json!({ "version": spec.version, "path": path }),
                        );
                        Ok(compiler)
                    }
                    Err(e) => {
                        self.emit_host(
                            "host.toolchain.download.failed",
                            serde_json::json!({
                                "version": spec.version,
                                "code": "not_runnable",
                                "error": e.to_string(),
                            }),
                        );
                        Err(e)
                    }
                }
            }
            Err(e) => {
                let code = e.code();
                self.finish_toolchain_download();
                if matches!(
                    e,
                    crate::toolchain_download::ToolchainDownloadError::Cancelled
                ) {
                    self.emit_host(
                        "host.toolchain.download.cancelled",
                        serde_json::json!({ "version": spec.version }),
                    );
                } else {
                    self.emit_host(
                        "host.toolchain.download.failed",
                        serde_json::json!({
                            "version": spec.version,
                            "code": code,
                            "error": e.to_string(),
                        }),
                    );
                }
                Err(HostError::Other(format!("{code}: {e}")))
            }
        }
    }

    // ----- QEMU download (v0.9 sandbox F1) ----------------------------------

    /// Where downloaded QEMU builds live (`<data-dir>/qemu`).
    ///
    /// One directory per resource, one versioned subdirectory inside it: the same
    /// layout as [`Self::toolchain_dir`].
    pub fn qemu_dir(&self) -> PathBuf {
        crate::paths::qemu_dir_in(&self.data_dir)
    }

    /// Claim the QEMU download slot. Errors when one is already running.
    ///
    /// The mirror of [`Self::begin_toolchain_download`]. Today no QEMU release is
    /// pinned, so a caller refuses (`unpinned_platform`) **before** claiming this
    /// slot; the slot exists so that pinning one is a data change, not a rewrite.
    pub fn begin_qemu_download(
        &self,
        spec: &crate::qemu_download::QemuDownloadSpec,
    ) -> Result<Arc<AtomicBool>, HostError> {
        let mut slot = self
            .qemu_download
            .lock()
            .map_err(|_| HostError::Other("download lock poisoned".into()))?;
        if slot.is_some() {
            return Err(HostError::Other("download already in progress".into()));
        }
        let cancel = Arc::new(AtomicBool::new(false));
        *slot = Some(QemuDownloadState {
            cancel: Arc::clone(&cancel),
            started_at: std::time::Instant::now(),
        });
        drop(slot);
        self.emit_host(
            "host.qemu.download.start",
            serde_json::json!({ "version": spec.version }),
        );
        Ok(cancel)
    }

    /// Ask an in-flight QEMU download to stop.
    pub fn cancel_qemu_download(&self) -> Result<(), HostError> {
        let slot = self
            .qemu_download
            .lock()
            .map_err(|_| HostError::Other("download lock poisoned".into()))?;
        match slot.as_ref() {
            Some(state) => {
                state.cancel.store(true, Ordering::Relaxed);
                Ok(())
            }
            None => Err(HostError::Other("no download in progress".into())),
        }
    }

    /// Current QEMU download status (also valid when idle: the last event is kept).
    pub fn qemu_download_status(&self) -> QemuDownloadStatus {
        let in_progress = self
            .qemu_download
            .lock()
            .map(|slot| slot.is_some())
            .unwrap_or(false);
        let last_event = self
            .qemu_download_last
            .lock()
            .ok()
            .and_then(|event| event.clone());
        QemuDownloadStatus {
            in_progress,
            last_event,
        }
    }

    /// Record one QEMU download event (progress reporting + polling).
    pub fn record_qemu_download_event(&self, event: crate::qemu_download::QemuDownloadEvent) {
        if let Ok(mut last) = self.qemu_download_last.lock() {
            *last = Some(event);
        }
    }

    /// Release the QEMU download slot (called when the worker finishes).
    pub fn finish_qemu_download(&self) {
        if let Ok(mut slot) = self.qemu_download.lock() {
            *slot = None;
        }
    }

    /// Download, verify and install, then adopt the emulator as the active one.
    ///
    /// The mirror of [`Self::download_toolchain_now`], including the adoption step:
    /// a QEMU that installs but does not run is `not_runnable`, not a success.
    pub fn download_qemu_now(
        &self,
        spec: &crate::qemu_download::QemuDownloadSpec,
        dest_root: &Path,
        cancel: Arc<AtomicBool>,
        on_event: &mut dyn FnMut(crate::qemu_download::QemuDownloadEvent),
    ) -> Result<PathBuf, HostError> {
        let mut forward = |event: crate::qemu_download::QemuDownloadEvent| {
            self.record_qemu_download_event(event.clone());
            on_event(event);
        };
        let result =
            crate::qemu_download::download_and_install(spec, dest_root, &cancel, &mut forward);

        match result {
            Ok(emulator) => {
                let path = emulator.display().to_string();
                let adopted = self.set_qemu_path(&path);
                self.finish_qemu_download();
                match adopted {
                    Ok(()) => {
                        self.emit_host(
                            "host.qemu.download.done",
                            serde_json::json!({ "version": spec.version, "path": path }),
                        );
                        Ok(emulator)
                    }
                    Err(e) => {
                        self.emit_host(
                            "host.qemu.download.failed",
                            serde_json::json!({
                                "version": spec.version,
                                "code": "not_runnable",
                                "error": e.to_string(),
                            }),
                        );
                        Err(e)
                    }
                }
            }
            Err(e) => {
                let code = e.code();
                self.finish_qemu_download();
                if matches!(e, crate::qemu_download::QemuDownloadError::Cancelled) {
                    self.emit_host(
                        "host.qemu.download.cancelled",
                        serde_json::json!({ "version": spec.version }),
                    );
                } else {
                    self.emit_host(
                        "host.qemu.download.failed",
                        serde_json::json!({
                            "version": spec.version,
                            "code": code,
                            "error": e.to_string(),
                        }),
                    );
                }
                Err(HostError::Other(format!("{code}: {e}")))
            }
        }
    }

    /// Effective compiler config: an explicit user path wins over discovery.
    fn toolchain_config(&self) -> agent::CompilerConfig {
        let mut cfg = match self.toolchain_path.lock().ok().and_then(|g| g.clone()) {
            Some(path) => agent::CompilerConfig::manual(path),
            None => agent::CompilerConfig::from_env(),
        };
        // v0.9 F3a: the second language is pinned in the same struct, so a `.zig`
        // compile sees the user's choice without a second plumbing path to the model.
        if let Some(zig) = self.zig_path.lock().ok().and_then(|g| g.clone()) {
            cfg.zig = agent::ZigConfig::manual(zig);
        }
        // v0.9 F3b-1: same for Rust's sysroot (the Rust compiler itself is the machine's).
        cfg.rust = self.rust_config();
        cfg
    }

    /// Effective Rust config: an explicit user sysroot wins over the environment
    /// (v0.9 F3b-1).
    pub fn rust_config(&self) -> agent::RustConfig {
        let mut cfg = agent::RustConfig::from_env();
        if let Some(sysroot) = self.rust_sysroot.lock().ok().and_then(|g| g.clone()) {
            cfg.sysroot = Some(sysroot);
        }
        cfg
    }

    /// The machine's `rustc` release (`1.98.1`), when there is one (v0.9 F3b-2).
    ///
    /// The value a `rust-std` download is checked against before it starts — and the value a UI
    /// would show beside a sysroot, since the two have to agree.
    pub fn rust_release(&self) -> Option<String> {
        let rustc = self.rust_config().rustc?;
        rustc_release(&rustc).ok()
    }

    /// Effective Zig config: an explicit user path wins over discovery (v0.9 F3a).
    pub fn zig_config(&self) -> agent::ZigConfig {
        match self.zig_path.lock().ok().and_then(|g| g.clone()) {
            Some(path) => agent::ZigConfig::manual(path),
            None => agent::ZigConfig::from_env(),
        }
    }

    /// Current toolchain status for the UI.
    pub fn probe_toolchain(&self) -> ToolchainView {
        if let Some(path) = self.toolchain_path.lock().ok().and_then(|g| g.clone()) {
            let runnable = toolchain_runs(&path);
            return ToolchainView {
                found: runnable.is_ok(),
                path: Some(path.display().to_string()),
                source: "Manual".to_string(),
                diagnostics: match &runnable {
                    Ok(version) => format!("manual path: {}\n{version}", path.display()),
                    Err(e) => format!("manual path: {} is not runnable: {e}", path.display()),
                },
            };
        }
        let cfg = agent::CompilerConfig::from_env();
        let found = cfg.gcc.is_file();
        ToolchainView {
            found,
            path: found.then(|| cfg.gcc.display().to_string()),
            source: cfg.source.as_str().to_string(),
            diagnostics: agent::CompilerConfig::diagnostics(),
        }
    }

    /// Store a user-chosen RISC-V GCC after checking that it really runs.
    pub fn set_toolchain_path(&self, path: &str) -> Result<(), HostError> {
        let p = PathBuf::from(path.trim());
        if !p.is_file() {
            return Err(HostError::Other(format!("not a file: {}", p.display())));
        }
        let version =
            toolchain_runs(&p).map_err(|e| HostError::Other(format!("not runnable: {e}")))?;
        *self
            .toolchain_path
            .lock()
            .map_err(|_| HostError::Other("toolchain lock poisoned".into()))? = Some(p.clone());
        if let Ok(mut g) = self.settings.lock() {
            g.toolchain_path = Some(p.display().to_string());
        }
        self.save_settings();
        // The environment changed: the cached preflight no longer describes it.
        self.clear_preflight_cache();
        self.emit_host(
            "host.toolchain.set",
            serde_json::json!({ "path": p.display().to_string(), "version": version }),
        );
        Ok(())
    }

    /// Drop the manual toolchain and fall back to auto-discovery.
    pub fn clear_toolchain_path(&self) -> Result<(), HostError> {
        *self
            .toolchain_path
            .lock()
            .map_err(|_| HostError::Other("toolchain lock poisoned".into()))? = None;
        if let Ok(mut g) = self.settings.lock() {
            g.toolchain_path = None;
        }
        self.save_settings();
        self.emit_host("host.toolchain.clear", serde_json::json!({}));
        Ok(())
    }

    /// Store a user-chosen Zig executable after checking that it really runs (v0.9 F3a).
    ///
    /// The preflight cache is deliberately **not** invalidated: it describes the C guest
    /// build and the emulator, which a Zig path cannot change.
    pub fn set_zig_path(&self, path: &str) -> Result<(), HostError> {
        let p = PathBuf::from(path.trim());
        if !p.is_file() {
            return Err(HostError::Other(format!("not a file: {}", p.display())));
        }
        let version = zig_runs(&p).map_err(|e| HostError::Other(format!("not runnable: {e}")))?;
        *self
            .zig_path
            .lock()
            .map_err(|_| HostError::Other("zig lock poisoned".into()))? = Some(p.clone());
        if let Ok(mut g) = self.settings.lock() {
            g.zig_path = Some(p.display().to_string());
        }
        self.save_settings();
        self.emit_host(
            "host.zig.set",
            serde_json::json!({ "path": p.display().to_string(), "version": version }),
        );
        Ok(())
    }

    /// Store a user-chosen Rust sysroot (v0.9 F3b-1).
    ///
    /// A sysroot has no `--version` to run, so the check is about **shape**: the target's
    /// library directory has to be inside it. Refusing here turns "the compile fails later
    /// with a rustc message" into "this directory is not a sysroot". The `rustc` version is
    /// reported next to it because the two must match.
    pub fn set_rust_sysroot(&self, path: &str) -> Result<(), HostError> {
        let p = PathBuf::from(path.trim());
        if !p.is_dir() {
            return Err(HostError::Other(format!(
                "not a directory: {}",
                p.display()
            )));
        }
        let target = agent::RustConfig::from_env().target;
        let libs = p.join("lib").join("rustlib").join(&target).join("lib");
        if !libs.is_dir() {
            return Err(HostError::Other(format!(
                "no Rust libraries for {target} under {}: expected {}",
                p.display(),
                libs.display()
            )));
        }
        *self
            .rust_sysroot
            .lock()
            .map_err(|_| HostError::Other("rust lock poisoned".into()))? = Some(p.clone());
        if let Ok(mut g) = self.settings.lock() {
            g.rust_sysroot = Some(p.display().to_string());
        }
        self.save_settings();
        let version = agent::RustConfig::discover()
            .ok()
            .and_then(|rustc| rust_runs(&rustc).ok());
        self.emit_host(
            "host.rust.set",
            serde_json::json!({ "path": p.display().to_string(), "version": version }),
        );
        Ok(())
    }

    /// Drop the manual Rust sysroot and fall back to the environment (v0.9 F3b-1).
    pub fn clear_rust_sysroot(&self) -> Result<(), HostError> {
        *self
            .rust_sysroot
            .lock()
            .map_err(|_| HostError::Other("rust lock poisoned".into()))? = None;
        if let Ok(mut g) = self.settings.lock() {
            g.rust_sysroot = None;
        }
        self.save_settings();
        self.emit_host("host.rust.clear", serde_json::json!({}));
        Ok(())
    }

    /// Drop the manual Zig executable and fall back to auto-discovery (v0.9 F3a).
    pub fn clear_zig_path(&self) -> Result<(), HostError> {
        *self
            .zig_path
            .lock()
            .map_err(|_| HostError::Other("zig lock poisoned".into()))? = None;
        if let Ok(mut g) = self.settings.lock() {
            g.zig_path = None;
        }
        self.save_settings();
        self.emit_host("host.zig.clear", serde_json::json!({}));
        Ok(())
    }

    /// Where the local settings file lives (tests / diagnostics).
    /// Current QEMU status for the UI (v0.3 5b-1a).
    pub fn probe_qemu(&self) -> QemuView {
        if let Some(path) = self.qemu_path.lock().ok().and_then(|g| g.clone()) {
            let runnable = toolchain_runs(&path);
            return QemuView {
                found: runnable.is_ok(),
                path: Some(path.display().to_string()),
                source: "Manual".to_string(),
                diagnostics: match &runnable {
                    Ok(version) => format!("manual path: {}\n{version}", path.display()),
                    Err(e) => format!("manual path: {} is not runnable: {e}", path.display()),
                },
            };
        }
        match sandbox::qemu_discover::discover() {
            Ok(location) => QemuView {
                found: true,
                path: Some(location.exe.display().to_string()),
                source: location.source.as_str().to_string(),
                diagnostics: sandbox::qemu_discover::diagnostics(),
            },
            Err(_) => QemuView {
                found: false,
                path: None,
                source: "Path".to_string(),
                diagnostics: sandbox::qemu_discover::diagnostics(),
            },
        }
    }

    /// Store a user-chosen QEMU after checking that it runs.
    pub fn set_qemu_path(&self, path: &str) -> Result<(), HostError> {
        let p = PathBuf::from(path.trim());
        if !p.is_file() {
            return Err(HostError::Other(format!("not a file: {}", p.display())));
        }
        let version =
            toolchain_runs(&p).map_err(|e| HostError::Other(format!("not runnable: {e}")))?;
        *self
            .qemu_path
            .lock()
            .map_err(|_| HostError::Other("qemu lock poisoned".into()))? = Some(p.clone());
        if let Ok(mut g) = self.settings.lock() {
            g.qemu_path = Some(p.display().to_string());
        }
        self.save_settings();
        // The environment changed: the cached preflight no longer describes it.
        self.clear_preflight_cache();
        self.emit_host(
            "host.qemu.set",
            serde_json::json!({ "path": p.display().to_string(), "version": version }),
        );
        Ok(())
    }

    /// Drop the manual QEMU path and fall back to auto-discovery.
    pub fn clear_qemu_path(&self) -> Result<(), HostError> {
        *self
            .qemu_path
            .lock()
            .map_err(|_| HostError::Other("qemu lock poisoned".into()))? = None;
        if let Ok(mut g) = self.settings.lock() {
            g.qemu_path = None;
        }
        self.save_settings();
        self.emit_host("host.qemu.clear", serde_json::json!({}));
        Ok(())
    }

    /// Where the local settings file lives (tests / diagnostics).
    pub fn settings_path(&self) -> &Path {
        &self.settings_path
    }

    /// The data directory this instance owns (v0.8).
    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    /// Directory for downloaded toolchains, resolved against **this instance's**
    /// data directory (v0.8). Commands that install a toolchain use this rather
    /// than the process-wide default, so two hosts in one process cannot
    /// overwrite each other's downloads.
    pub fn toolchain_dir(&self) -> PathBuf {
        crate::paths::toolchain_dir_in(&self.data_dir)
    }

    // ----- Sandboxes (v0.9 sandbox F2a) -------------------------------------

    /// The definitions written by hand in `settings.json`.
    fn manual_sandbox_defs(&self) -> Vec<SandboxDef> {
        self.settings
            .lock()
            .ok()
            .map(|settings| settings.sandboxes.clone())
            .unwrap_or_default()
    }

    /// The raw scan: what is installed under **this instance's** data directory,
    /// plus the QEMU this machine already has.
    ///
    /// Read-only by contract: the result is a candidate list, and it is never
    /// written back to `settings.json` (F2a decision 3).
    pub fn sandbox_candidates(&self) -> CandidatesView {
        crate::sandbox_def::discover_in(&self.data_dir)
    }

    /// Could this definition run **right now** — and if not, why not? (F2b.)
    ///
    /// The three parts of [`Self::sandbox_runnable`], each with the reason it
    /// failed, so a caller that has to act on the answer (a switch, which must
    /// decide *before* it stops anything) can name what to fix. Everything the
    /// definition does not pin falls back to what the host would use anyway.
    fn sandbox_check(&self, def: &SandboxDef) -> Result<(), HostError> {
        // QEMU: the definition's own must be a file that answers `--version`;
        // otherwise the host's own QEMU has to be discoverable.
        match &def.qemu_exe {
            Some(path) if !path.is_file() => {
                return Err(HostError::SandboxQemuMissing(format!(
                    "not a file: {}",
                    path.display()
                )))
            }
            Some(path) => {
                if let Err(e) = toolchain_runs(path) {
                    return Err(HostError::SandboxQemuMissing(format!(
                        "{} is not runnable: {e}",
                        path.display()
                    )));
                }
            }
            None => {
                let qemu = self.probe_qemu();
                if !qemu.found {
                    return Err(HostError::SandboxQemuMissing(qemu.diagnostics));
                }
            }
        }

        // Toolchain: the definition's own must exist; otherwise the host's.
        match &def.toolchain_path {
            Some(path) if !path.is_file() => {
                return Err(HostError::SandboxToolchainMissing(format!(
                    "not a file: {}",
                    path.display()
                )))
            }
            Some(_) => {}
            None => {
                let toolchain = self.probe_toolchain();
                if !toolchain.found {
                    return Err(HostError::SandboxToolchainMissing(toolchain.diagnostics));
                }
            }
        }

        // Kernel: a pinned one must exist; an unpinned one is the toolchain's to
        // compile, which is what the check just confirmed.
        if let Some(path) = &def.kernel {
            if !path.is_file() {
                return Err(HostError::SandboxKernelMissing(format!(
                    "not a file: {}",
                    path.display()
                )));
            }
        }
        Ok(())
    }

    /// Could this definition run **right now**? (F2a decision 7.)
    ///
    /// The boolean face of [`Self::sandbox_check`] — one rule, two shapes: the
    /// registry wants a flag, a switch wants the reason. Three parts, all
    /// required: a QEMU that exists and answers `--version`, a toolchain that
    /// exists, and a kernel that exists — or, when the definition pins none, one
    /// this toolchain can compile. A definition whose QEMU was uninstalled stays a
    /// valid definition; it is simply not runnable.
    fn sandbox_runnable(&self, def: &SandboxDef) -> bool {
        self.sandbox_check(def).is_ok()
    }

    /// One definition as the API serves it: the stored fields plus the three the
    /// host answers at the moment of the question (`source`, `runnable`,
    /// `shadowed`).
    fn sandbox_view(&self, def: &SandboxDef, source: SandboxSource, shadowed: bool) -> SandboxView {
        SandboxView {
            name: def.name.clone(),
            display_name: def.display_name.clone(),
            memory_mb: def.memory_mb,
            qemu_exe: def.qemu_exe.as_ref().map(|p| p.display().to_string()),
            toolchain_path: def.toolchain_path.as_ref().map(|p| p.display().to_string()),
            kernel: def.kernel.as_ref().map(|p| p.display().to_string()),
            notes: def.notes.clone(),
            source,
            runnable: self.sandbox_runnable(def),
            shadowed,
        }
    }

    /// The merged registry as **definitions**: `(definition, source, shadowed)`, in
    /// the order [`Self::sandboxes`] serves — hand-written first, then what the
    /// scan found, then the built-in fallback (F2a decision 4).
    ///
    /// One merge, two readers: the registry wants to serve them, a switch wants to
    /// use the winner (which is why the precedence lives here and not in either
    /// caller).
    fn merged_sandbox_defs(&self) -> Vec<(SandboxDef, SandboxSource, bool)> {
        let manual = self.manual_sandbox_defs();
        let taken: std::collections::HashSet<&str> =
            manual.iter().map(|def| def.name.as_str()).collect();
        let mut merged: Vec<(SandboxDef, SandboxSource, bool)> = manual
            .iter()
            .map(|def| (def.clone(), SandboxSource::Manual, false))
            .collect();

        // One entry per scanned resource, named `<kind>-<version>`; the toolchain
        // and QEMU lists stay independent (F2a decision 6 — no cartesian product).
        let candidates = self.sandbox_candidates();
        let mut discovered: Vec<SandboxDef> = Vec::new();
        for candidate in candidates.toolchains.iter().chain(candidates.qemus.iter()) {
            discovered.push(SandboxDef::for_resource(
                &candidate.kind,
                &candidate.version,
                PathBuf::from(&candidate.path),
            ));
        }
        // A registry needs unique keys; the scan cannot name two resources alike,
        // but a duplicated version directory could.
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        discovered.retain(|def| seen.insert(def.name.clone()));
        discovered.sort_by(|a, b| a.name.cmp(&b.name));
        for def in discovered {
            let shadowed = taken.contains(def.name.as_str());
            merged.push((def, SandboxSource::Discovered, shadowed));
        }

        let fallback = SandboxDef::fallback();
        let shadowed = taken.contains(fallback.name.as_str());
        merged.push((fallback, SandboxSource::Discovered, shadowed));
        merged
    }

    /// The merged registry: the hand-written definitions first, then what the
    /// scan found, then the built-in fallback (F2a decision 4).
    ///
    /// A hand-written definition wins on a name collision, and the shadowed entry
    /// **stays in the list, marked**, so the merge is visible instead of silent.
    /// Nothing here is persisted.
    pub fn sandboxes(&self) -> Vec<SandboxView> {
        self.merged_sandbox_defs()
            .iter()
            .map(|(def, source, shadowed)| self.sandbox_view(def, *source, *shadowed))
            .collect()
    }

    /// One merged definition by name, as the API serves it.
    pub fn sandbox(&self, name: &str) -> Option<SandboxView> {
        self.sandboxes().into_iter().find(|view| view.name == name)
    }

    /// The definition the registry resolves `name` to, as the switch uses it.
    ///
    /// The winner of the same merge, so a hand-written definition shadows a scanned
    /// one here exactly as it does in the list.
    fn sandbox_def_by_name(&self, name: &str) -> Option<SandboxDef> {
        self.merged_sandbox_defs()
            .into_iter()
            .find(|(def, _, _)| def.name == name)
            .map(|(def, _, _)| def)
    }

    /// The sandbox this node is running **now**, if a switch chose one.
    ///
    /// Runtime state (F2b decision 1): `None` until a switch succeeds, and never
    /// written to `settings.json` — [`Self::sandbox_default_name`] is what a
    /// restart starts from.
    pub fn current_sandbox(&self) -> Option<String> {
        self.current_sandbox
            .lock()
            .ok()
            .and_then(|slot| slot.clone())
    }

    /// The default definition's name: the stored `default_sandbox`, else the
    /// fallback's.
    ///
    /// This is the *configured* default (settings), not [`Self::current_sandbox`]:
    /// a switch changes what is running, not what the configuration says.
    pub fn sandbox_default_name(&self) -> String {
        self.settings
            .lock()
            .ok()
            .and_then(|settings| settings.default_sandbox.clone())
            .unwrap_or_else(|| DEFAULT_SANDBOX_NAME.to_string())
    }

    /// Is a run in flight right now? (v0.9 sandbox F2b.)
    ///
    /// Read from the run bookkeeping [`Self::begin_run`] sets and
    /// [`Self::finish_run`] clears — the host's runs are synchronous, so this
    /// answers "a call is inside `run_agent` right now", which is the question a
    /// switch has to ask before it takes the VM out from under a loop.
    ///
    /// Two paths leave it stale, both of them the audit already having failed: a
    /// run-start marker the sink refused (the run proceeds, this says `false`), and
    /// a process that died mid-run (a restart clears it, and `abandon_stale_runs`
    /// closes the ledger row).
    pub fn run_in_flight(&self) -> bool {
        self.current_run_id
            .lock()
            .map(|slot| slot.is_some())
            .unwrap_or(false)
    }

    // ----- Sandbox switch (v0.9 sandbox F2b) --------------------------------

    /// Claim the switch slot. Errors when a switch is already in progress.
    ///
    /// The same shape as the download slots' `begin` (F2b decision 3): one switch
    /// at a time, so two of them cannot race for the same VM slot.
    pub fn begin_sandbox_switch(&self, target: &str) -> Result<(), HostError> {
        let mut slot = self
            .switch_slot
            .lock()
            .map_err(|_| HostError::Other("sandbox switch lock poisoned".into()))?;
        if slot.is_some() {
            return Err(HostError::Other(
                "sandbox switch already in progress".into(),
            ));
        }
        *slot = Some(SandboxSwitchState {
            target: target.to_string(),
            started_at: std::time::Instant::now(),
        });
        Ok(())
    }

    /// Give the switch slot back without switching.
    ///
    /// The download slots' `cancel` shape: it errors when nothing is in progress,
    /// so a caller that cancels a switch that already finished hears about it
    /// instead of silently succeeding.
    pub fn cancel_sandbox_switch(&self) -> Result<(), HostError> {
        let mut slot = self
            .switch_slot
            .lock()
            .map_err(|_| HostError::Other("sandbox switch lock poisoned".into()))?;
        match slot.take() {
            Some(_) => Ok(()),
            None => Err(HostError::Other("no sandbox switch in progress".into())),
        }
    }

    /// Release the switch slot (called when the switch is over, either way).
    pub fn finish_sandbox_switch(&self) {
        if let Ok(mut slot) = self.switch_slot.lock() {
            *slot = None;
        }
    }

    /// Is a sandbox switch in progress right now? (v0.9 sandbox F2b-2.)
    ///
    /// The probe the control plane answers `409` from *before* it calls
    /// [`Self::switch_sandbox`], the same shape as
    /// [`Self::toolchain_download_status`]'s `in_progress`. The switch refuses a
    /// second caller itself; this exists so the refusal can carry a status and a
    /// `cause` instead of a message nobody can branch on.
    pub fn sandbox_switch_in_progress(&self) -> bool {
        self.switch_slot
            .lock()
            .map(|slot| slot.is_some())
            .unwrap_or(false)
    }

    // ----- Task-level sandbox (v0.9 sandbox F2d) -----------------------------

    /// The definition the running VM came from, if a definition started it.
    pub fn active_sandbox(&self) -> Option<String> {
        self.active_sandbox
            .lock()
            .ok()
            .and_then(|slot| slot.clone())
    }

    /// Which definition a run should use, in order of authority (F2d).
    ///
    /// A **task** outranks the node: a task that names a sandbox gets that one, and
    /// an unknown name is a refusal (`404`, `cause: "name"` — never a silent
    /// fallback, or a typo would look like a successful run). A task that names none
    /// gets what the node is running (`current_sandbox`), then the configured
    /// default, then the registry's built-in fallback (whose empty definition means
    /// "discover the host's own QEMU and toolchain"); if even that is gone the answer
    /// is `None`, the behaviour before this batch.
    ///
    /// The answer carries the name as well as the definition: the name is what the
    /// running VM's provenance is recorded under.
    pub fn resolve_task_sandbox(
        &self,
        task_sandbox: Option<&str>,
    ) -> Result<Option<(SandboxDef, String)>, HostError> {
        if let Some(name) = task_sandbox {
            return match self.sandbox_def_by_name(name) {
                Some(def) => Ok(Some((def, name.to_string()))),
                None => Err(HostError::SandboxNotFound(name.to_string())),
            };
        }
        let names = [
            self.current_sandbox(),
            Some(self.sandbox_default_name()),
            // And last, the registry's built-in fallback, by name: a node whose
            // configured default was removed still runs, on discovery.
            Some(DEFAULT_SANDBOX_NAME.to_string()),
        ];
        for name in names.into_iter().flatten() {
            // A name this node *was* using can stop resolving (a hand-written
            // definition removed from settings); falling through is better than
            // refusing a run over a stale name nobody asked for.
            if let Some(def) = self.sandbox_def_by_name(&name) {
                return Ok(Some((def, name)));
            }
        }
        Ok(None)
    }

    /// The refusal a run gets when it asked for a sandbox that is not the one the
    /// running VM came from (F2d).
    ///
    /// A VM cannot simply be replaced inside a run: the switch exists for that, it
    /// needs `sandbox.switch`, and it is an explicit node change. So the run says
    /// why it will not start instead.
    fn check_task_sandbox(&self, requested: &str) -> Result<(), HostError> {
        match task_sandbox_conflict(
            self.vm_is_running(),
            self.active_sandbox().as_deref(),
            requested,
        ) {
            None => Ok(()),
            Some(message) => Err(HostError::SandboxConflict(message)),
        }
    }

    /// Note which definition a VM put into the slot came from (F2d).
    fn mark_active_sandbox(&self, name: Option<&str>) {
        if let Ok(mut slot) = self.active_sandbox.lock() {
            *slot = name.map(str::to_string);
        }
    }

    // ----- Sandbox requests (v0.9 sandbox F2c) -------------------------------

    /// The request queue, for callers that hold their own sink (the agent loop's
    /// tool gateway). The methods below are the same queue with the sink filled in.
    pub fn sandbox_requests(&self) -> SandboxRequests {
        self.sandbox_requests.clone()
    }

    /// A queue plus the sink a change is announced on.
    fn sandbox_request_service(&self, emitter: Arc<dyn EventSink>) -> SandboxRequestService {
        SandboxRequestService::new(self.sandbox_requests(), emitter)
    }

    /// Leave a request for a sandbox change (v0.9 sandbox F2c).
    ///
    /// Nothing is switched here: the ask waits for an actor that holds the
    /// capability its `action` implies, and approving it is a second, separate
    /// call (F2c decision 4). Answers the new request, and announces it on
    /// `sandbox:request` with `status: "pending"`.
    pub fn request_sandbox(
        &self,
        requester_agent_id: &str,
        action: SandboxAction,
        sandbox: Option<String>,
        definition: Option<crate::sandbox_def::SandboxDef>,
        reason: Option<String>,
        emitter: Arc<dyn EventSink>,
    ) -> Result<SandboxRequestView, HostError> {
        self.sandbox_request_service(emitter).request(
            requester_agent_id,
            action,
            sandbox,
            definition,
            reason,
        )
    }

    /// The queue, newest first, optionally filtered by status.
    pub fn list_sandbox_requests(
        &self,
        status: Option<SandboxRequestStatus>,
    ) -> Vec<SandboxRequestView> {
        self.sandbox_requests.list(status)
    }

    /// What a request asks for. The route calls this **before** deciding, because
    /// the capability a decision needs follows from the action (F2c decision 1).
    pub fn sandbox_request_action(&self, id: &str) -> Result<SandboxAction, HostError> {
        self.sandbox_requests.action_of(id)
    }

    /// Approve a pending request. Changes the record and nothing else.
    pub fn approve_sandbox_request(
        &self,
        id: &str,
        decided_by: &str,
        emitter: Arc<dyn EventSink>,
    ) -> Result<SandboxRequestView, HostError> {
        self.sandbox_request_service(emitter)
            .decide(id, SandboxRequestStatus::Approved, decided_by)
    }

    /// Reject a pending request.
    pub fn reject_sandbox_request(
        &self,
        id: &str,
        decided_by: &str,
        emitter: Arc<dyn EventSink>,
    ) -> Result<SandboxRequestView, HostError> {
        self.sandbox_request_service(emitter)
            .decide(id, SandboxRequestStatus::Rejected, decided_by)
    }

    /// Switch this node to the sandbox `name` (v0.9 sandbox F2b).
    ///
    /// The order is the whole point (F2b decision 2): everything that can be
    /// checked is checked **before** the running VM is touched, so a definition
    /// that cannot run leaves the current sandbox alone; and everything after the
    /// stop is a failure that leaves the node **stopped** rather than
    /// half-switched — a handle whose `start` failed is dropped, and `Drop` kills
    /// whatever it spawned (`sandbox/src/vm.rs`).
    ///
    /// Refused while another switch is in progress, and while a run is in flight:
    /// the loop shares this VM slot and takes it per tool call, so a switch under a
    /// running agent would silently hand it a different guest (F2b decision 4).
    ///
    /// Emits exactly one [`EV_SANDBOX_SWITCH`] on every exit — with `ok: true` and
    /// a new sandbox running, or with `ok: false` and the reason it did not. The
    /// VM's own `vm.stop` / `vm.start` audit rows are the sandbox's, and they
    /// arrive as they always did.
    pub fn switch_sandbox(&self, name: &str, emitter: Arc<dyn EventSink>) -> Result<(), HostError> {
        // What was current before, for the event's `from` (F2b-2).
        let from = self.current_sandbox();
        let outcome = (|| -> Result<(), HostError> {
            // ① The definition, or the reason there is none.
            let def = self
                .sandbox_def_by_name(name)
                .ok_or_else(|| HostError::SandboxNotFound(name.to_string()))?;
            // ② One switch at a time.
            self.begin_sandbox_switch(name)?;

            let switched = (|| -> Result<(), HostError> {
                // F2b decision 4: a run in flight means the loop is holding this slot.
                if self.run_in_flight() {
                    return Err(HostError::Other(
                        "a run is in flight; a sandbox switch would take its VM away".into(),
                    ));
                }
                // ③④⑤ Everything checkable, before anything is stopped: the QEMU and
                // the toolchain (`sandbox_check`), then the kernel.
                self.sandbox_check(&def)?;
                let kernel = match &def.kernel {
                    Some(path) => path.clone(),
                    None => self.resume_kernel().map_err(|e| {
                        HostError::SandboxKernelMissing(format!(
                            "the definition pins none and the workspace has none: {}",
                            e.user_message()
                        ))
                    })?,
                };

                // ⑥ Only now is the running VM touched.
                self.stop_current_vm()?;

                // ⑦ A fresh VM on fresh ports, up to three times: the lease narrows
                // the window on a port race and the retry covers what is left — the
                // same shape `tool_start_vm` uses (`agent/src/tools.rs`).
                const START_ATTEMPTS: usize = 3;
                let mut last_error = String::from("unknown error");
                for attempt in 1..=START_ATTEMPTS {
                    let mut leases = sandbox::relay::lease_local_ports(2)
                        .map_err(|e| HostError::Other(e.to_string()))?;
                    let mut serial_lease = leases.pop().expect("two leases were requested");
                    let mut qmp_lease = leases.pop().expect("two leases were requested");
                    let config = VMConfig {
                        kernel: kernel.clone(),
                        memory_mb: def.memory_mb.unwrap_or(agent::VM_MEMORY_MB),
                        qmp: QmpEndpoint::tcp("127.0.0.1", qmp_lease.port()),
                        serial: SerialEndpoint::tcp("127.0.0.1", serial_lease.port()),
                        snapshot_dir: self.snapshot_dir(),
                        serial_observer: Some(agent::tools::serial_observer_for(Arc::clone(
                            &self.serial_senders,
                        ))),
                        // A switch boots fresh: it is not a restore (F2b decision 3 —
                        // resuming a snapshot stays its own action).
                        incoming_snapshot: None,
                        incoming_relay_addr: None,
                        // The definition's QEMU wins, then the host's manual one.
                        qemu_exe: def.qemu_exe.clone().or_else(|| self.manual_qemu_path()),
                    };
                    let mut vm = match RiscVVirtualMachine::new(config, Arc::clone(&self.sink)) {
                        Ok(vm) => vm,
                        Err(e) => {
                            last_error = format!("attempt {attempt}: {e}");
                            std::thread::sleep(std::time::Duration::from_millis(150));
                            continue;
                        }
                    };
                    // Last moment: hand the ports over to QEMU.
                    qmp_lease.hand_off();
                    serial_lease.hand_off();
                    match vm.start() {
                        Ok(()) => {
                            // ⑧ A VM that started *is* the switch; adopt it, then the name.
                            *self
                                .vm_slot
                                .lock()
                                .map_err(|_| HostError::Other("vm slot poisoned".into()))? =
                                Some(vm);
                            self.clear_vm_started();
                            self.mark_vm_started();
                            if let Ok(mut slot) = self.current_sandbox.lock() {
                                *slot = Some(name.to_string());
                            }
                            // The slot now holds a VM this definition started, which
                            // is what a later task-declared run is compared against
                            // (v0.9 sandbox F2d).
                            self.mark_active_sandbox(Some(name));
                            return Ok(());
                        }
                        Err(e) => {
                            last_error = format!("attempt {attempt}: {e}");
                            std::thread::sleep(std::time::Duration::from_millis(150));
                        }
                    }
                }
                // ⑨ The slot is released by the caller. The node is stopped, not
                // half-switched: the last handle was dropped and killed its child.
                Err(HostError::SandboxStart(format!(
                    "switch to {name:?} failed after {START_ATTEMPTS} attempts: {last_error}"
                )))
            })();

            self.finish_sandbox_switch();
            switched
        })();

        // One event per attempt, either way (F2b-2). A client that sees `ok: false`
        // reads `reason`; nothing else changes hands here.
        let reason = outcome.as_ref().err().map(|e| e.user_message());
        emitter.emit(
            EV_SANDBOX_SWITCH,
            crate::events::sandbox_switch_payload(
                from.as_deref(),
                name,
                outcome.is_ok(),
                reason.as_deref(),
            ),
        );
        outcome
    }

    /// Read settings from disk and apply them (missing/corrupt → defaults).
    fn load_settings(&self) {
        let loaded = LocalSettings::load(&self.settings_path);
        if let Ok(mut g) = self.settings.lock() {
            *g = loaded.clone();
        }
        if let Ok(mut g) = self.toolchain_path.lock() {
            *g = loaded.toolchain_path.map(PathBuf::from);
        }
        if let Ok(mut g) = self.zig_path.lock() {
            *g = loaded.zig_path.map(PathBuf::from);
        }
        if let Ok(mut g) = self.rust_sysroot.lock() {
            *g = loaded.rust_sysroot.map(PathBuf::from);
        }
        if let Ok(mut g) = self.qemu_path.lock() {
            *g = loaded.qemu_path.map(PathBuf::from);
        }
    }

    /// Persist settings. Best effort: a failure is audited, never fatal.
    fn save_settings(&self) {
        let snapshot = self.settings.lock().map(|g| g.clone()).unwrap_or_default();
        if let Err(e) = snapshot.save(&self.settings_path) {
            self.emit_host(
                "host.settings.save_failed",
                serde_json::json!({
                    "error": e,
                    "path": self.settings_path.display().to_string(),
                }),
            );
        }
    }

    /// The session the next run appends to.
    pub fn current_session_id(&self) -> Option<String> {
        self.current_session_id
            .lock()
            .ok()
            .and_then(|guard| guard.clone())
    }

    /// Create a session and make it current.
    pub fn create_session(&self, title: &str) -> Result<String, HostError> {
        let id = {
            let store = self
                .sessions
                .lock()
                .map_err(|_| HostError::Other("sessions lock poisoned".into()))?;
            store
                .create_session(title)
                .map_err(|e| HostError::Other(e.to_string()))?
        };
        if let Ok(mut slot) = self.current_session_id.lock() {
            *slot = Some(id.clone());
        }
        self.emit_host(
            "host.session.create",
            serde_json::json!({ "session_id": id }),
        );
        Ok(id)
    }

    /// Recent sessions, newest first.
    pub fn list_sessions(&self, limit: usize) -> Result<Vec<SessionMeta>, HostError> {
        let store = self
            .sessions
            .lock()
            .map_err(|_| HostError::Other("sessions lock poisoned".into()))?;
        store
            .list_sessions(limit)
            .map_err(|e| HostError::Other(e.to_string()))
    }

    /// Open a session (and make it current), returning its messages.
    pub fn open_session(&self, session_id: &str) -> Result<SessionDetailView, HostError> {
        let store = self
            .sessions
            .lock()
            .map_err(|_| HostError::Other("sessions lock poisoned".into()))?;
        let meta = store
            .list_sessions(1000)
            .map_err(|e| HostError::Other(e.to_string()))?
            .into_iter()
            .find(|s| s.id == session_id)
            .ok_or_else(|| HostError::Other("session not found".to_string()))?;
        let messages = store
            .load_messages(session_id, HISTORY_LIMIT)
            .map_err(|e| HostError::Other(e.to_string()))?;
        drop(store);

        if let Ok(mut slot) = self.current_session_id.lock() {
            *slot = Some(session_id.to_string());
        }
        self.emit_host(
            "host.session.open",
            serde_json::json!({ "session_id": session_id }),
        );
        Ok(SessionDetailView { meta, messages })
    }

    /// Rename a session.
    pub fn rename_session(&self, session_id: &str, title: &str) -> Result<(), HostError> {
        let store = self
            .sessions
            .lock()
            .map_err(|_| HostError::Other("sessions lock poisoned".into()))?;
        store
            .rename_session(session_id, title)
            .map_err(|e| HostError::Other(e.to_string()))?;
        drop(store);
        self.emit_host(
            "host.session.rename",
            serde_json::json!({ "session_id": session_id }),
        );
        Ok(())
    }

    /// Delete a session (its messages cascade) and forget it if current.
    pub fn delete_session(&self, session_id: &str) -> Result<(), HostError> {
        let store = self
            .sessions
            .lock()
            .map_err(|_| HostError::Other("sessions lock poisoned".into()))?;
        store
            .delete_session(session_id)
            .map_err(|e| HostError::Other(e.to_string()))?;
        drop(store);
        if let Ok(mut slot) = self.current_session_id.lock() {
            if slot.as_deref() == Some(session_id) {
                *slot = None;
            }
        }
        self.emit_host(
            "host.session.delete",
            serde_json::json!({ "session_id": session_id }),
        );
        Ok(())
    }

    /// Delete every session.
    pub fn clear_all_sessions(&self) -> Result<(), HostError> {
        let store = self
            .sessions
            .lock()
            .map_err(|_| HostError::Other("sessions lock poisoned".into()))?;
        store
            .clear_all()
            .map_err(|e| HostError::Other(e.to_string()))?;
        drop(store);
        if let Ok(mut slot) = self.current_session_id.lock() {
            *slot = None;
        }
        self.emit_host("host.session.delete", serde_json::json!({ "all": true }));
        Ok(())
    }

    /// The current session, creating one titled from `user_input` if needed.
    fn ensure_session(&self, user_input: &str) -> Result<String, HostError> {
        if let Some(id) = self.current_session_id() {
            return Ok(id);
        }
        self.create_session(&title_from(user_input))
    }

    /// Messages to rehydrate a fresh `AgentLoop` with (never the system prompt).
    fn load_session_history(&self, session_id: &str) -> Result<Vec<ChatMessage>, HostError> {
        let store = self
            .sessions
            .lock()
            .map_err(|_| HostError::Other("sessions lock poisoned".into()))?;
        let rows = store
            .load_messages(session_id, HISTORY_LIMIT)
            .map_err(|e| HostError::Other(e.to_string()))?;
        Ok(rows.iter().filter_map(history_to_chat).collect())
    }

    /// Persist everything the turn produced (system messages are never stored).
    fn persist_turn(
        &self,
        session_id: &str,
        messages: &[ChatMessage],
        from: usize,
    ) -> Result<(), HostError> {
        let store = self
            .sessions
            .lock()
            .map_err(|_| HostError::Other("sessions lock poisoned".into()))?;
        for message in messages.iter().skip(from) {
            if message.role == "system" {
                continue;
            }
            let mut row = SessionMessage::new(
                session_id,
                message.role.clone(),
                message.content.clone().unwrap_or_default(),
            );
            row.tool_call_json = message
                .tool_calls
                .as_ref()
                .and_then(|calls| serde_json::to_string(calls).ok());
            row.tool_call_id = message.tool_call_id.clone();
            store
                .append_message(session_id, row)
                .map_err(|e| HostError::Other(e.to_string()))?;
        }
        Ok(())
    }

    // ----- LLM config -------------------------------------------------------

    /// The built-in provider presets (pure data).
    pub fn provider_presets(&self) -> Vec<ProviderPresetView> {
        builtin_presets()
            .into_iter()
            .map(ProviderPresetView::from)
            .collect()
    }

    /// Store LLM config, filling base_url / model from a preset when omitted.
    ///
    /// `remember` (default `true`) also persists the key in the OS keyring;
    /// failures degrade silently to in-memory-only.
    pub fn set_llm_config_with(
        &self,
        provider_id: Option<String>,
        api_key: String,
        base_url: String,
        model: String,
        remember: Option<bool>,
    ) -> Result<(), HostError> {
        let pid = provider_id
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_PRESET_ID.to_string());

        let (base_url, model) = match find_preset(&pid) {
            Some(p) if p.id != "custom" => (
                if base_url.trim().is_empty() {
                    p.base_url
                } else {
                    base_url
                },
                if model.trim().is_empty() {
                    p.default_model
                } else {
                    model
                },
            ),
            // custom (or unknown id): take the user's values as-is.
            _ => (base_url, model),
        };

        if pid == "custom" && (base_url.trim().is_empty() || model.trim().is_empty()) {
            return Err(HostError::Other(
                "invalid_config|custom provider requires base_url and model".into(),
            ));
        }

        let input = LlmConfigInput {
            provider_id: pid,
            api_key,
            base_url,
            model,
        };

        // Validate before persisting; return a structured code the UI can map
        // to a field-level hint. The message never contains the key.
        let readiness = readiness_of(&input);
        if !readiness.ready {
            return Err(HostError::Other(readiness_error(&readiness)));
        }

        let provider_id = input.provider_id.clone();
        let key = input.api_key.clone();
        let remember = remember.unwrap_or(true);

        let persisted = if remember {
            self.keyring_save(&provider_id, &key)
        } else {
            // Never leave a stale entry behind when the user opts out.
            let _ = self.keyring_delete(&provider_id);
            false
        };

        self.set_llm_config(input);
        if let Ok(mut p) = self.persisted.lock() {
            *p = persisted;
        }
        Ok(())
    }

    /// Does the keyring hold a key for `provider_id`?
    pub fn has_stored_key(&self, provider_id: &str) -> bool {
        matches!(
            self.keyring.get(SERVICE, &user_for_provider(provider_id)),
            Ok(Some(_))
        )
    }

    /// Load a stored key from the keyring into memory (startup restore).
    pub fn load_stored_key(&self, provider_id: &str) -> Result<(), String> {
        let key = match self.keyring.get(SERVICE, &user_for_provider(provider_id)) {
            Ok(Some(key)) => key,
            Ok(None) => return Err("no_stored_key".to_string()),
            Err(e) => return Err(e),
        };

        let existing = self
            .llm_config
            .lock()
            .ok()
            .and_then(|g| g.as_ref().cloned());
        let input = match existing {
            Some(current) if current.provider_id == provider_id => LlmConfigInput {
                api_key: key,
                ..current
            },
            _ => {
                let preset =
                    find_preset(provider_id).ok_or_else(|| "unknown_provider".to_string())?;
                LlmConfigInput {
                    provider_id: preset.id.clone(),
                    api_key: key,
                    base_url: preset.base_url,
                    model: preset.default_model,
                }
            }
        };

        let readiness = readiness_of(&input);
        if !readiness.ready {
            return Err(readiness.reason.unwrap_or_else(|| "invalid_config".into()));
        }

        self.set_llm_config(input);
        if let Ok(mut p) = self.persisted.lock() {
            *p = true;
        }
        self.emit_host(
            "host.keyring.load",
            serde_json::json!({ "provider_id": provider_id }),
        );
        Ok(())
    }

    fn keyring_save(&self, provider_id: &str, api_key: &str) -> bool {
        if api_key.trim().is_empty() {
            return false;
        }
        match self
            .keyring
            .set(SERVICE, &user_for_provider(provider_id), api_key)
        {
            Ok(()) => {
                self.emit_host(
                    "host.keyring.save",
                    serde_json::json!({ "provider_id": provider_id }),
                );
                true
            }
            Err(_) => {
                self.emit_host(
                    "host.keyring.save_failed",
                    serde_json::json!({ "provider_id": provider_id }),
                );
                false
            }
        }
    }

    fn keyring_delete(&self, provider_id: &str) -> Result<(), String> {
        let result = self
            .keyring
            .delete(SERVICE, &user_for_provider(provider_id));
        if result.is_ok() {
            self.emit_host(
                "host.keyring.delete",
                serde_json::json!({ "provider_id": provider_id }),
            );
        }
        result
    }

    /// Store LLM config in memory (replaces any previous value).
    pub fn set_llm_config(&self, input: LlmConfigInput) {
        if let Ok(mut g) = self.llm_config.lock() {
            *g = Some(input);
        }
    }

    /// Forget LLM config: clears memory **and** the keyring entry for the
    /// currently configured provider (other providers are left untouched).
    pub fn clear_llm_config(&self) {
        let provider = self
            .llm_config
            .lock()
            .ok()
            .and_then(|g| g.as_ref().map(|c| c.provider_id.clone()));
        if let Ok(mut g) = self.llm_config.lock() {
            *g = None;
        }
        if let Some(provider_id) = provider {
            let _ = self.keyring_delete(&provider_id);
        }
        if let Ok(mut p) = self.persisted.lock() {
            *p = false;
        }
    }

    /// Status for the UI (never returns the key).
    pub fn llm_config_status(&self) -> LlmConfigStatus {
        let persisted = self.persisted.lock().map(|g| *g).unwrap_or(false);
        let unconfigured = || LlmConfigStatus {
            configured: false,
            provider_id: DEFAULT_PRESET_ID.to_string(),
            base_url: String::new(),
            model: String::new(),
            persisted: false,
        };
        match self.llm_config.lock() {
            Ok(g) => match g.as_ref() {
                Some(c) => LlmConfigStatus {
                    configured: true,
                    provider_id: c.provider_id.clone(),
                    base_url: c.base_url.clone(),
                    model: c.model.clone(),
                    persisted,
                },
                None => unconfigured(),
            },
            Err(_) => unconfigured(),
        }
    }

    /// Whether the LLM is ready to run (never includes the key).
    pub fn llm_readiness(&self) -> LlmReadiness {
        // Test seam: an injected client is always considered ready.
        if self
            .llm_override
            .lock()
            .map(|g| g.is_some())
            .unwrap_or(false)
        {
            return LlmReadiness {
                ready: true,
                reason: None,
                suggestion: None,
            };
        }
        let no_config = || LlmReadiness {
            ready: false,
            reason: Some("no_config".into()),
            suggestion: Some("请在设置中选择服务商并填写 API Key，或使用本地模型".to_string()),
        };
        match self.llm_config.lock() {
            Ok(g) => match g.as_ref() {
                Some(input) => readiness_of(input),
                None => no_config(),
            },
            Err(_) => no_config(),
        }
    }

    /// Probe localhost for local OpenAI-compatible LLM servers.
    ///
    /// Short timeout per request; all failures are silent.
    pub fn probe_local_llm(&self) -> LocalProbeResult {
        let client = match reqwest::blocking::Client::builder()
            .timeout(PROBE_TIMEOUT)
            .build()
        {
            Ok(c) => c,
            Err(_) => {
                return LocalProbeResult {
                    found: false,
                    providers: Vec::new(),
                    probed: Vec::new(),
                }
            }
        };

        let mut providers: Vec<LocalProviderInfo> = Vec::new();
        let mut probed: Vec<String> = Vec::new();

        for (id, name, url) in PROBE_TARGETS {
            probed.push((*url).to_string());
            let models = match fetch_models(&client, url) {
                Some(m) => m,
                None => continue,
            };
            if providers.iter().any(|p| p.id == *id) {
                continue;
            }
            providers.push(LocalProviderInfo {
                id: (*id).to_string(),
                display_name: (*name).to_string(),
                base_url: url.trim_end_matches("/models").to_string(),
                models,
            });
        }

        let found = !providers.is_empty();
        self.emit_host(
            "host.llm.probe",
            serde_json::json!({
                "found": found,
                "probed": probed,
                "providers": providers
                    .iter()
                    .map(|p| serde_json::json!({ "id": p.id, "models": p.models.len() }))
                    .collect::<Vec<_>>(),
            }),
        );

        LocalProbeResult {
            found,
            providers,
            probed,
        }
    }

    /// Record a host-originated audit event.
    ///
    /// The result is not handled here on purpose: the sink's reporter already
    /// queues a failure (and writes the log line), so a host event that could not
    /// be written still reaches the alert path (v0.8).
    fn emit_host(&self, action: &str, detail: serde_json::Value) {
        if let Ok(mut sink) = self.sink.lock() {
            let _ = sink
                .record(audit::AuditEvent::new("host", action, detail).with_agent(&self.agent_id));
        }
    }

    fn agent_config(&self) -> AgentConfig {
        let default = AgentConfig::deepseek_default();
        let using_override = self
            .llm_override
            .lock()
            .map(|g| g.is_some())
            .unwrap_or(false);
        let (mut api_key, base_url, model, provider_id) = match self.llm_config.lock() {
            Ok(g) => match g.as_ref() {
                Some(c) => (
                    c.api_key.clone(),
                    c.base_url.clone(),
                    c.model.clone(),
                    c.provider_id.clone(),
                ),
                None => (
                    String::new(),
                    default.base_url.clone(),
                    default.model.clone(),
                    default.provider_id.clone(),
                ),
            },
            Err(_) => (
                String::new(),
                default.base_url.clone(),
                default.model.clone(),
                default.provider_id.clone(),
            ),
        };
        // Test seam only: an injected client never hits the network, so satisfy
        // the agent's config gate with a placeholder when no key is configured.
        if using_override && api_key.trim().is_empty() {
            api_key = "override-test-client".to_string();
        }
        AgentConfig {
            api_key,
            base_url,
            model,
            provider_id,
            max_iterations: 10,
            request_timeout_secs: 120,
        }
    }

    fn build_llm(&self) -> Result<Box<dyn LlmClient>, HostError> {
        if let Ok(g) = self.llm_override.lock() {
            if let Some(over) = g.as_ref() {
                return Ok(Box::new(ArcLlm(Arc::clone(over))));
            }
        }
        let config = self.agent_config();
        if config.api_key.trim().is_empty() {
            return Err(HostError::NotConfigured(
                "LLM not configured; call set_llm_config first".into(),
            ));
        }
        Ok(Box::new(DeepSeekClient::new(config)?))
    }

    // ----- Environment preflight (v0.4 batch 3) -----------------------------

    /// Where this agent's preflight guest is built: `<workspace>/.riscdom/preflight/<agent_id>`.
    ///
    /// Per agent (v0.8 follow-up A2): several processes may share one workspace,
    /// and a shared directory meant two of them compiling the guest and booting
    /// their preflight VM into the same paths at the same time. New artifacts
    /// always land here; reads fall back to [`Self::preflight_root`] so a guest
    /// built before this change stays usable. Host-managed, never in the AI's
    /// area.
    pub fn preflight_dir(&self) -> PathBuf {
        self.preflight_root().join(&self.agent_id)
    }

    /// The directory every agent's preflight subdirectory lives under
    /// (`<workspace>/.riscdom/preflight`) — where an older version wrote them.
    pub fn preflight_root(&self) -> PathBuf {
        self.workspace_root.join(".riscdom").join("preflight")
    }

    /// Lay this agent's preflight guest down in its own directory and return the
    /// `(source, elf)` paths the compiler will use.
    ///
    /// This is the write half of the per-agent isolation; it is public so a test
    /// can pin the layout without compiling or booting anything.
    pub fn write_preflight_guest(&self) -> Result<(PathBuf, PathBuf), String> {
        let dir = self.preflight_dir();
        std::fs::create_dir_all(&dir)
            .map_err(|e| format!("preflight dir {}: {e}", dir.display()))?;
        let src = dir.join(PREFLIGHT_GUEST_SRC);
        let elf = dir.join(PREFLIGHT_GUEST_ELF);
        std::fs::write(&src, crate::preflight::GUEST_SRC)
            .map_err(|e| format!("{}: {e}", src.display()))?;
        Ok((src, elf))
    }

    /// The preflight guest to boot: this agent's own build first, then a guest an
    /// older version left in the shared root.
    ///
    /// The read half of the isolation — a pre-A2 artifact is still usable instead
    /// of being orphaned, and it never wins over this agent's own build.
    pub fn find_preflight_guest(&self) -> Option<PathBuf> {
        for dir in [self.preflight_dir(), self.preflight_root()] {
            let path = dir.join(PREFLIGHT_GUEST_ELF);
            if path.is_file() {
                return Some(path);
            }
        }
        None
    }

    fn preflight_cache(&self) -> Option<crate::preflight::PreflightCache> {
        self.settings.lock().ok().and_then(|s| s.preflight.clone())
    }

    /// The cached preflight for the **current** configuration, or an unchecked
    /// view when the configuration changed since it ran.
    pub fn preflight_status(&self) -> crate::preflight::PreflightView {
        let fingerprint = audit::fingerprint(&self.run_fingerprint());
        match self.preflight_cache() {
            Some(cache) if cache.fingerprint == fingerprint => {
                crate::preflight::PreflightView::from_cache(&cache, false)
            }
            _ => crate::preflight::PreflightView::unchecked(&fingerprint),
        }
    }

    /// Run the preflight when the cache has no result for this configuration
    /// (`force` runs it regardless). A cache hit does not touch the environment.
    pub fn ensure_preflight(
        &self,
        force: bool,
        emitter: Option<Arc<dyn EventSink>>,
    ) -> Result<crate::preflight::PreflightView, HostError> {
        self.ensure_preflight_with(
            force,
            emitter,
            crate::preflight::PreflightOptions::default(),
        )
    }

    /// [`Self::ensure_preflight`] with explicit options (the compile guard's
    /// budget is the interesting one).
    pub fn ensure_preflight_with(
        &self,
        force: bool,
        emitter: Option<Arc<dyn EventSink>>,
        options: crate::preflight::PreflightOptions,
    ) -> Result<crate::preflight::PreflightView, HostError> {
        let fingerprint = audit::fingerprint(&self.run_fingerprint());
        if !force {
            if let Some(cache) = self.preflight_cache() {
                if cache.fingerprint == fingerprint {
                    return Ok(crate::preflight::PreflightView::from_cache(&cache, false));
                }
            }
        }
        Ok(self.run_preflight_now(emitter, options))
    }

    /// Record the escape hatch: the user accepts this configuration as it is, so
    /// the warning stops until the configuration changes again.
    pub fn acknowledge_preflight(&self) -> Result<crate::preflight::PreflightView, HostError> {
        let fingerprint = audit::fingerprint(&self.run_fingerprint());
        let mut cache =
            self.preflight_cache()
                .unwrap_or_else(|| crate::preflight::PreflightCache {
                    fingerprint: fingerprint.clone(),
                    ok: false,
                    failed_step: None,
                    detail: None,
                    suggestion: None,
                    checked_at_ms: now_ms(),
                    overridden: false,
                });
        cache.fingerprint = fingerprint;
        cache.overridden = true;
        if let Ok(mut settings) = self.settings.lock() {
            settings.preflight = Some(cache.clone());
        }
        self.save_settings();
        Ok(crate::preflight::PreflightView::from_cache(&cache, false))
    }

    /// Forget the cached preflight (the environment changed).
    fn clear_preflight_cache(&self) {
        if let Ok(mut settings) = self.settings.lock() {
            settings.preflight = None;
        }
        self.save_settings();
    }

    /// Compile the preflight guest, but never wait longer than `budget`.
    ///
    /// `agent::compile_freestanding` owns its child process, so the guard runs it
    /// on a worker thread and, when the budget expires, stops the compiler
    /// processes **this** process started and reports the timeout (v0.4 batch
    /// 3-followup). The agent's compile path itself is untouched.
    fn compile_with_guard(
        &self,
        compiler: &agent::CompilerConfig,
        src: &Path,
        elf: &Path,
        budget: Duration,
    ) -> Result<(), String> {
        let gcc = compiler.gcc.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        let thread_compiler = compiler.clone();
        let thread_src = src.to_path_buf();
        let thread_elf = elf.to_path_buf();
        std::thread::spawn(move || {
            let _ = tx.send(agent::compile_freestanding(
                &thread_compiler,
                &thread_src,
                &thread_elf,
            ));
        });

        match rx.recv_timeout(budget) {
            Ok(Ok(out)) if out.ok && elf.is_file() => Ok(()),
            Ok(Ok(out)) => Err(format!(
                "compiler said:\n{}\n{}",
                out.stderr.trim(),
                out.stdout.trim()
            )),
            Ok(Err(e)) => Err(e.to_string()),
            Err(_) => {
                Self::kill_compiler_children(&gcc);
                Err(format!(
                    "编译器超过 {} 秒没有返回，已终止该进程（可能是二进制损坏、包装脚本卡住，或路径有问题）",
                    budget.as_secs()
                ))
            }
        }
    }

    /// Stop the compiler processes **this** process started.
    ///
    /// Scoped to direct children of the host whose image name is the configured
    /// compiler's (or `cmd.exe`, for a batch-file wrapper), so a QEMU guest the
    /// host also owns is never touched. Best effort: the guard's job is to report
    /// the timeout, not to guarantee an OS-level cleanup.
    fn kill_compiler_children(gcc: &Path) {
        let Some(name) = gcc.file_name().and_then(|n| n.to_str()) else {
            return;
        };
        #[cfg(target_os = "windows")]
        {
            let script = format!(
                "$names = @('{name}','cmd.exe'); Get-CimInstance Win32_Process -Filter \"ParentProcessId={pid}\" | Where-Object {{ $names -contains $_.Name }} | ForEach-Object {{ Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }}",
                pid = std::process::id()
            );
            let _ = std::process::Command::new("powershell")
                .args(["-NoProfile", "-NonInteractive", "-Command", &script])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = std::process::Command::new("pkill")
                .args(["-P", &std::process::id().to_string(), "-x", name])
                .status();
        }
    }

    /// The four checks, in order, fail-fast. Emits `preflight:progress` when an
    /// emitter is given. **Never touches the audit chain** and never fails: a
    /// broken environment is reported, not raised.
    fn run_preflight_now(
        &self,
        emitter: Option<Arc<dyn EventSink>>,
        options: crate::preflight::PreflightOptions,
    ) -> crate::preflight::PreflightView {
        use crate::preflight as pf;

        let fingerprint = audit::fingerprint(&self.run_fingerprint());
        let emit = |step: &str, state: &str, detail: Option<&str>| {
            if let Some(sink) = &emitter {
                sink.emit(
                    crate::events::EV_PREFLIGHT,
                    serde_json::json!({ "step": step, "state": state, "detail": detail }),
                );
            }
        };

        let outcome = (|| -> Result<(), (String, String, String)> {
            // 1. the configured compiler runs and reports a version
            emit(pf::STEP_GCC_RUNS, "running", None);
            let compiler = self.toolchain_config();
            let gcc = compiler.gcc.clone();
            match toolchain_runs(&gcc) {
                Ok(version) => emit(pf::STEP_GCC_RUNS, "ok", Some(&version)),
                Err(e) => {
                    let detail = format!("{} --version: {e}", gcc.display());
                    emit(pf::STEP_GCC_RUNS, "failed", Some(&detail));
                    return Err((
                        pf::STEP_GCC_RUNS.to_string(),
                        detail,
                        pf::SUGGEST_GCC_RUNS.to_string(),
                    ));
                }
            }

            // 2. it compiles the preflight guest, in the real environment (the
            //    host's own directory, the real toolchain path: long paths and
            //    spaces fail here rather than later inside a run)
            emit(pf::STEP_GCC_COMPILES, "running", None);
            let build = self.write_preflight_guest().and_then(|(src, elf)| {
                self.compile_with_guard(&compiler, &src, &elf, options.compile_timeout)
                    .map(|()| elf)
            });
            let elf = match build {
                Ok(elf) => {
                    emit(
                        pf::STEP_GCC_COMPILES,
                        "ok",
                        Some(&elf.display().to_string()),
                    );
                    elf
                }
                Err(detail) => {
                    emit(pf::STEP_GCC_COMPILES, "failed", Some(&detail));
                    return Err((
                        pf::STEP_GCC_COMPILES.to_string(),
                        detail,
                        pf::SUGGEST_GCC_COMPILES.to_string(),
                    ));
                }
            };

            // 3. the configured QEMU runs
            emit(pf::STEP_QEMU_RUNS, "running", None);
            let qemu = self.probe_qemu();
            let Some(qemu_path) = qemu.path.clone() else {
                let detail = qemu.diagnostics.clone();
                emit(pf::STEP_QEMU_RUNS, "failed", Some(&detail));
                return Err((
                    pf::STEP_QEMU_RUNS.to_string(),
                    detail,
                    pf::SUGGEST_QEMU_RUNS.to_string(),
                ));
            };
            match toolchain_runs(Path::new(&qemu_path)) {
                Ok(version) => emit(pf::STEP_QEMU_RUNS, "ok", Some(&version)),
                Err(e) => {
                    let detail = format!("{qemu_path} --version: {e}");
                    emit(pf::STEP_QEMU_RUNS, "failed", Some(&detail));
                    return Err((
                        pf::STEP_QEMU_RUNS.to_string(),
                        detail,
                        pf::SUGGEST_QEMU_RUNS.to_string(),
                    ));
                }
            }

            // 4. it boots that guest and the banner arrives
            emit(pf::STEP_GUEST_BOOTS, "running", None);
            // This agent's own build, or — if it is gone — a guest an older
            // version left in the shared root.
            let boot_guest = self.find_preflight_guest().unwrap_or_else(|| elf.clone());
            let booted = (|| -> Result<(), String> {
                let mut leases = sandbox::relay::lease_local_ports(2).map_err(|e| e.to_string())?;
                let mut serial_lease = leases.pop().expect("two leases were requested");
                let mut qmp_lease = leases.pop().expect("two leases were requested");
                let config = VMConfig {
                    kernel: boot_guest.clone(),
                    memory_mb: agent::VM_MEMORY_MB,
                    qmp: QmpEndpoint::tcp("127.0.0.1", qmp_lease.port()),
                    serial: SerialEndpoint::tcp("127.0.0.1", serial_lease.port()),
                    snapshot_dir: self.preflight_dir(),
                    // No observer: the preflight must not feed the run's serial
                    // stream, and must not touch the host-owned VM slot.
                    serial_observer: None,
                    incoming_snapshot: None,
                    incoming_relay_addr: None,
                    qemu_exe: Some(PathBuf::from(&qemu_path)),
                };
                let mut vm = RiscVVirtualMachine::new(config, Arc::clone(&self.sink))
                    .map_err(|e| e.to_string())?;
                // Last moment: hand the ports over to QEMU.
                qmp_lease.hand_off();
                serial_lease.hand_off();
                let result = match vm.start() {
                    Ok(()) => {
                        crate::preflight::wait_for_banner(crate::preflight::BANNER_TIMEOUT, || {
                            vm.serial_output()
                        })
                    }
                    Err(e) => Err(format!("QEMU 未能启动：{e}")),
                };
                // Always clean up, whatever happened.
                let _ = vm.stop();
                result
            })();
            match booted {
                Ok(()) => emit(pf::STEP_GUEST_BOOTS, "ok", Some(pf::BANNER)),
                Err(detail) => {
                    emit(pf::STEP_GUEST_BOOTS, "failed", Some(&detail));
                    return Err((
                        pf::STEP_GUEST_BOOTS.to_string(),
                        detail,
                        pf::SUGGEST_GUEST_BOOTS.to_string(),
                    ));
                }
            }
            Ok(())
        })();

        let cache = match outcome {
            Ok(()) => pf::PreflightCache {
                fingerprint,
                ok: true,
                failed_step: None,
                detail: None,
                suggestion: None,
                checked_at_ms: now_ms(),
                overridden: false,
            },
            Err((step, detail, suggestion)) => pf::PreflightCache {
                fingerprint,
                ok: false,
                failed_step: Some(step),
                detail: Some(detail),
                suggestion: Some(suggestion),
                checked_at_ms: now_ms(),
                overridden: false,
            },
        };
        if let Ok(mut settings) = self.settings.lock() {
            settings.preflight = Some(cache.clone());
        }
        self.save_settings();
        emit("done", if cache.ok { "ok" } else { "failed" }, None);
        pf::PreflightView::from_cache(&cache, true)
    }

    // ----- Appearance (v0.4 #11a) -------------------------------------------

    /// The stored UI theme preference; `system` when the user never chose one.
    pub fn theme(&self) -> String {
        self.settings
            .lock()
            .ok()
            .and_then(|settings| settings.theme.clone())
            .unwrap_or_else(|| "system".to_string())
    }

    /// Store the UI theme preference (`light` / `dark` / `system`).
    pub fn set_theme(&self, theme: &str) -> Result<(), HostError> {
        let theme = theme.trim().to_lowercase();
        if !matches!(theme.as_str(), "light" | "dark" | "system") {
            return Err(HostError::Other(format!("unknown theme: {theme}")));
        }
        if let Ok(mut settings) = self.settings.lock() {
            settings.theme = Some(theme.clone());
        }
        self.save_settings();
        self.emit_host("host.theme.set", serde_json::json!({ "theme": theme }));
        Ok(())
    }

    /// The stored UI language preference; `system` when the user never chose one
    /// (v0.7 batch 2).
    pub fn language(&self) -> String {
        self.settings
            .lock()
            .ok()
            .and_then(|settings| settings.language.clone())
            .unwrap_or_else(|| "system".to_string())
    }

    /// Store the UI language preference (`system` / `en` / `zh`).
    pub fn set_language(&self, language: &str) -> Result<(), HostError> {
        let language = language.trim().to_lowercase();
        if !matches!(language.as_str(), "system" | "en" | "zh") {
            return Err(HostError::Other(format!("unknown language: {language}")));
        }
        if let Ok(mut settings) = self.settings.lock() {
            settings.language = Some(language.clone());
        }
        self.save_settings();
        self.emit_host(
            "host.language.set",
            serde_json::json!({ "language": language }),
        );
        Ok(())
    }

    /// This instance's agent identity (v0.8 batch B): `local-<pid>-<seq>`.
    pub fn agent_id(&self) -> &str {
        &self.agent_id
    }

    /// Is the audit-failure alert on? `true` when the user never chose (v0.8).
    pub fn alert_on_audit_failure(&self) -> bool {
        self.settings
            .lock()
            .ok()
            .map(|settings| settings.alert_on_audit_failure)
            .unwrap_or(true)
    }

    /// Turn the audit-failure alert (banner + popup) on or off.
    ///
    /// The `audit:failed` event and the log line are **not** affected — they are
    /// always sent; this only decides whether the interface shouts.
    pub fn set_alert_on_audit_failure(&self, enabled: bool) -> Result<(), HostError> {
        if let Ok(mut settings) = self.settings.lock() {
            settings.alert_on_audit_failure = enabled;
        }
        self.save_settings();
        self.emit_host(
            "host.audit_alert.set",
            serde_json::json!({ "enabled": enabled }),
        );
        Ok(())
    }

    // ----- Audit ------------------------------------------------------------

    /// Event count + chain status.
    pub fn audit_status(&self) -> Result<AuditStatusView, HostError> {
        // Read the setting before taking the chain lock: the two mutexes are then
        // never held in opposite orders.
        let alert_on_failure = self.alert_on_audit_failure();
        let store = self
            .audit
            .lock()
            .map_err(|_| HostError::Other("audit store lock poisoned".into()))?;
        Ok(AuditStatusView {
            count: store.count()?,
            chain: ChainStatusView::from(audit::verify_chain(&store)?),
            alert_on_failure,
            failures: Vec::new(),
        })
    }

    // ----- Audit write failures (v0.8) -------------------------------------

    /// Audit-write failures queued by the sink's reporter and not taken yet.
    pub fn audit_failures(&self) -> Vec<String> {
        self.audit_failures
            .lock()
            .map(|guard| guard.clone())
            .unwrap_or_default()
    }

    /// Queue an audit-write failure.
    ///
    /// The sink's reporter calls this for the host. It is public so the
    /// surfacing path can be tested without arranging a real database lock.
    pub fn push_audit_failure(&self, message: &str) {
        if let Ok(mut failures) = self.audit_failures.lock() {
            failures.push(message.to_string());
            if failures.len() > AUDIT_FAILURE_QUEUE_CAP {
                let excess = failures.len() - AUDIT_FAILURE_QUEUE_CAP;
                failures.drain(0..excess);
            }
        }
    }

    /// Tell `emitter` about every failure it has not been told about yet.
    ///
    /// This is the **event half** of the failure channel and it is not optional:
    /// the log line is written by the reporter and this event is sent whatever
    /// the alert setting says. Returns how many were announced.
    pub fn emit_audit_failures(&self, emitter: &dyn EventSink) -> usize {
        let failures = match self.audit_failures.lock() {
            Ok(guard) => guard.clone(),
            Err(_) => return 0,
        };
        let already = self.audit_failures_emitted.load(Ordering::Relaxed);
        let mut announced = 0;
        for message in failures.iter().skip(already) {
            emitter.emit(
                EV_AUDIT_FAILED,
                crate::events::audit_failed_payload(message),
            );
            announced += 1;
        }
        self.audit_failures_emitted
            .store(failures.len(), Ordering::Relaxed);
        announced
    }

    /// Take the queued failures (the audit panel is told once).
    ///
    /// Clears the "already announced" cursor with them, so the next failure
    /// starts a fresh alert.
    pub fn take_audit_failures(&self) -> Vec<String> {
        let taken = match self.audit_failures.lock() {
            Ok(mut guard) => std::mem::take(&mut *guard),
            Err(_) => Vec::new(),
        };
        self.audit_failures_emitted.store(0, Ordering::Relaxed);
        taken
    }

    // ----- Runs (v0.4 1d) ---------------------------------------------------

    /// Recent runs from the derived index, oldest first. Read-only.
    pub fn list_runs(&self, limit: usize) -> Result<Vec<RunView>, HostError> {
        let store = self
            .audit
            .lock()
            .map_err(|_| HostError::Other("audit store lock poisoned".into()))?;
        let runs = store
            .list_runs(limit)
            .map_err(|e| HostError::Other(e.to_string()))?;
        Ok(runs.iter().map(RunView::from).collect())
    }

    /// One run by id, or `None` when this log has never seen it.
    pub fn get_run(&self, run_id: &str) -> Result<Option<RunView>, HostError> {
        let store = self
            .audit
            .lock()
            .map_err(|_| HostError::Other("audit store lock poisoned".into()))?;
        let run = store
            .get_run(run_id)
            .map_err(|e| HostError::Other(e.to_string()))?;
        Ok(run.as_ref().map(RunView::from))
    }

    /// Compare two runs' configuration fingerprints, field by field (v0.6 1).
    ///
    /// The documents come from the **chain**, not from the index: each run's
    /// `start_seq` is the id of its `run.start` event, and that event carries the
    /// canonical JSON that was hashed. Read-only; the rows are in
    /// [`run_diff::FINGERPRINT_FIELDS`] order.
    pub fn compare_run_fingerprints(
        &self,
        run_a: &str,
        run_b: &str,
    ) -> Result<Vec<FingerprintFieldDiff>, HostError> {
        let store = self
            .audit
            .lock()
            .map_err(|_| HostError::Other("audit store lock poisoned".into()))?;
        let left = Self::run_fingerprint_document(&store, run_a)?;
        let right = Self::run_fingerprint_document(&store, run_b)?;
        Ok(run_diff::diff_fingerprints(&left, &right))
    }

    /// The fingerprint document a run's `run.start` carries.
    fn run_fingerprint_document(
        store: &AuditStore,
        run_id: &str,
    ) -> Result<serde_json::Value, HostError> {
        let record = store
            .get_run(run_id)
            .map_err(|e| HostError::Other(e.to_string()))?
            .ok_or_else(|| HostError::Other(format!("no run {run_id} in this log")))?;
        let event = store
            .get(record.start_seq)
            .map_err(|e| HostError::Other(e.to_string()))?
            .ok_or_else(|| {
                HostError::Other(format!(
                    "run {run_id}: its `run.start` event ({}) is not in the chain",
                    record.start_seq
                ))
            })?;
        let payload = audit::parse_run_start(&event.event.detail)?;
        serde_json::from_str(&payload.fingerprint_json).map_err(|e| {
            HostError::Other(format!("run {run_id}: `fingerprint_json` is not JSON: {e}"))
        })
    }

    /// Recent events, newest last, filtered.
    pub fn list_events(
        &self,
        limit: usize,
        actor: Option<String>,
        action_prefix: Option<String>,
    ) -> Result<Vec<StoredEventView>, HostError> {
        let store = self
            .audit
            .lock()
            .map_err(|_| HostError::Other("audit store lock poisoned".into()))?;
        let filter = audit::EventFilter {
            actor,
            action_prefix,
            from_ms: None,
            to_ms: None,
            from_id: None,
            to_id: None,
        };
        let mut events = store.list(filter, limit)?;
        events.reverse(); // newest first for the UI
        Ok(events.into_iter().map(StoredEventView::from).collect())
    }

    /// Export the whole log as JSONL into the workspace.
    pub fn export_audit_jsonl(&self, path: String) -> Result<usize, HostError> {
        let policy = WorkspacePolicy::new(self.workspace_root.clone());
        let abs = policy
            .check_read(Path::new(&path))
            .map_err(|e| HostError::Policy(e.to_string()))?;
        let store = self
            .audit
            .lock()
            .map_err(|_| HostError::Other("audit store lock poisoned".into()))?;
        Ok(store.export_jsonl(&abs)?)
    }

    /// Export **one run's** audit interval as JSONL (v0.5 batch 1).
    ///
    /// The file is **self-contained** (v0.5 batch 4): it holds the chain from its
    /// first event up to the event that closes this run, so its first line's
    /// `prev_hash` is the genesis link and it can be verified in an empty database
    /// with nothing carried over from this one. Where the run's own events start is
    /// still recorded — that is the derived index's `start_seq`, not the file's
    /// first line.
    ///
    /// The end is this run's `run.end`; for a run the chain marked **abandoned** (its
    /// process disappeared) it is that run's `host.run.abandoned` event, so the last
    /// line says why it stopped. A run that is still open has nothing to close it and
    /// is refused rather than exported to wherever the chain happens to end.
    pub fn export_run_audit(&self, run_id: &str, path: String) -> Result<usize, HostError> {
        let policy = WorkspacePolicy::new(self.workspace_root.clone());
        let abs = policy
            .check_read(Path::new(&path))
            .map_err(|e| HostError::Policy(e.to_string()))?;
        let store = self
            .audit
            .lock()
            .map_err(|_| HostError::Other("audit store lock poisoned".into()))?;
        let record = store
            .get_run(run_id)?
            .ok_or_else(|| HostError::Other(format!("unknown run: {run_id}")))?;
        let to = store
            .run_end(&record)
            .map_err(|e| HostError::Other(e.to_string()))?;
        Ok(store.export_self_contained_jsonl(to, &abs)?)
    }

    /// The AI workspace root as an absolute path string (v0.5 batch 2).
    ///
    /// The host is the one that knows where the workspace is, so the UI asks for it
    /// instead of hard-coding a path: it is used to pre-fill the audit export's
    /// default file name, which must be a path the export is allowed to write to.
    pub fn workspace_root_display(&self) -> String {
        self.workspace_root.to_string_lossy().into_owned()
    }

    // ----- Workspace --------------------------------------------------------

    /// List workspace files (relative paths, forward slashes).
    pub fn workspace_files(&self) -> Result<Vec<String>, HostError> {
        let mut out = Vec::new();
        collect_files(&self.workspace_root, &self.workspace_root, &mut out)?;
        out.sort();
        Ok(out)
    }

    /// Read a workspace file (policy-checked).
    pub fn read_workspace_file(&self, path: String) -> Result<String, HostError> {
        let policy = WorkspacePolicy::new(self.workspace_root.clone());
        let abs = policy
            .check_read(Path::new(&path))
            .map_err(|e| HostError::Policy(e.to_string()))?;
        Ok(std::fs::read_to_string(abs)?)
    }

    /// Bring a project in: unpack `archive` (zip or tar.gz) into the workspace
    /// (v0.9 project in/out).
    ///
    /// Containment, not the write allow-list: an import carries a *project*, and a
    /// project is not only `.c` / `.h` / `.S` / `.s` (the same reasoning the
    /// exports already use). What the entries may not do is escape the workspace,
    /// arrive as a link, or touch the host's own state — see
    /// [`crate::workspace_io::unpack_archive`].
    ///
    /// `force` decides what happens when an entry names a file already here: by
    /// default the import refuses, and the caller hears which entry.
    pub fn import_workspace(
        &self,
        archive: &[u8],
        format: crate::workspace_io::ArchiveFormat,
        force: bool,
    ) -> Result<crate::workspace_io::UnpackReport, HostError> {
        crate::workspace_io::unpack_archive(archive, &self.workspace_root, format, force)
    }

    /// Take the project out: the workspace as a `tar.gz` (v0.9 project in/out).
    ///
    /// The host's own state directory is left out, and an empty workspace packs to
    /// a valid empty archive — "export this project" is never an error because the
    /// project is empty.
    pub fn export_workspace(&self) -> Result<Vec<u8>, HostError> {
        crate::workspace_io::pack_workspace(&self.workspace_root)
    }

    // ----- Serial -----------------------------------------------------------

    /// The accumulated serial text pushed by the sandbox so far.
    pub fn serial_buffer(&self) -> String {
        self.serial_accum
            .lock()
            .map(|g| g.clone())
            .unwrap_or_default()
    }

    /// Write the current serial buffer into the workspace. Returns bytes written.
    pub fn export_serial_log(&self, path: String) -> Result<usize, HostError> {
        let policy = WorkspacePolicy::new(self.workspace_root.clone());
        let abs = policy
            .check_read(Path::new(&path))
            .map_err(|e| HostError::Policy(e.to_string()))?;
        let text = self.serial_buffer();
        std::fs::write(&abs, &text)?;
        Ok(text.len())
    }

    // ----- Agent ------------------------------------------------------------

    /// Start the **long-lived** serial forwarder (call once at app startup).
    ///
    /// The forwarder owns the receiving end of a broadcast channel whose sender
    /// lives in [`Self::serial_senders`]. Every run attaches that same list, so
    /// serial output keeps flowing to the UI across runs.
    pub fn start_serial_forwarder(&self, emitter: Arc<dyn EventSink>) -> Result<(), HostError> {
        let (tx, rx) = std::sync::mpsc::channel::<Vec<u8>>();
        self.serial_senders
            .lock()
            .map_err(|_| HostError::Other("serial senders poisoned".into()))?
            .push(tx);

        let accum = Arc::clone(&self.serial_accum);
        std::thread::spawn(move || loop {
            match rx.recv_timeout(Duration::from_millis(200)) {
                Ok(bytes) => {
                    let text = append_serial_chunk(&accum, &bytes);
                    emitter.emit(EV_SERIAL_CHUNK, serde_json::json!({ "chunk": text }));
                }
                Err(RecvTimeoutError::Timeout) => continue,
                Err(RecvTimeoutError::Disconnected) => break,
            }
        });
        Ok(())
    }

    // ----- Run provenance (v0.4 batch 1c) -----------------------------------

    /// The configuration document a run's fingerprint hashes (design §1.2).
    ///
    /// Only values the host can actually resolve are included; anything else is
    /// recorded as `"unknown"` rather than omitted, so two fingerprints that
    /// differ only by an unreadable field never look identical. The API key is
    /// never part of it, in any form.
    pub fn run_fingerprint(&self) -> serde_json::Value {
        let agent_config = self.agent_config();
        let compiler = self.toolchain_config();
        let toolchain = self.probe_toolchain();
        let qemu = self.probe_qemu();
        let policy = WorkspacePolicy::new(self.workspace_root.clone());

        let hex_sha256 = |bytes: &[u8]| -> String {
            let mut hasher = Sha256::new();
            hasher.update(bytes);
            format!("{:x}", hasher.finalize())
        };
        // The tool set is hashed as the exact JSON the model is offered.
        let tools_sha256 = hex_sha256(
            serde_json::to_string(&agent::tools::tools_json())
                .unwrap_or_default()
                .as_bytes(),
        );
        let prompt = format!("{CONSTITUTION}\n\n{}", agent::prompt::OPERATING_RULES);

        // Versions come from running `--version`; an unreadable one is recorded
        // as unknown instead of being dropped.
        let version_of = |path: Option<&str>| -> String {
            path.map(|p| toolchain_runs(Path::new(p)).unwrap_or_else(|_| "unknown".into()))
                .unwrap_or_else(|| "unknown".into())
        };

        serde_json::json!({
            "schema": {
                "fingerprint_schema": audit::FINGERPRINT_SCHEMA_V1,
                "app_version": env!("CARGO_PKG_VERSION"),
            },
            "llm": {
                "provider_id": agent_config.provider_id,
                "base_url": agent_config.base_url,
                "model": agent_config.model,
            },
            "agent": {
                "max_iterations": agent_config.max_iterations,
                "request_timeout_secs": agent_config.request_timeout_secs,
                "tools_sha256": tools_sha256,
                "march": compiler.march,
                "mabi": compiler.mabi,
                "link_addr": compiler.link_addr,
                "crt0": agent::CRT0_INJECTED,
                "language_allowlist": policy.allowed_extensions,
            },
            "vm": {
                // Guest RAM comes from the agent (the crate that boots the guest),
                // so a change there cannot leave the fingerprint behind.
                "memory_mb": agent::VM_MEMORY_MB,
                // The machine and the cpu come from the sandbox itself, so a
                // change there cannot leave the fingerprint behind (v0.4 1e).
                "machine": sandbox::VM_MACHINE,
                "cpu": sandbox::VM_CPU,
                "qemu_path": qemu.path.clone().unwrap_or_else(|| "unknown".into()),
                "qemu_version": version_of(qemu.path.as_deref()),
                "snapshot_mode": SNAPSHOT_MODE,
            },
            "toolchain": {
                "path": toolchain.path.clone().unwrap_or_else(|| "unknown".into()),
                "version": version_of(toolchain.path.as_deref()),
                "source": toolchain.source,
            },
            "policy": {
                "allowed_extensions": policy.allowed_extensions,
                "traversal_guard": "normalize+containment",
            },
            "prompt": { "sha256": hex_sha256(prompt.as_bytes()) },
        })
    }

    /// Mint the id of one run. Host-side only: the id is never handed to the
    /// agent or the sandbox (an id the AI can influence is not provenance).
    fn new_run_id() -> String {
        format!("run_{}", uuid::Uuid::now_v7())
    }

    /// Append a host event and return its position in the chain (best effort).
    fn append_host_event(&self, action: &str, detail: serde_json::Value) -> Option<StoredEvent> {
        let mut store = self.audit.lock().ok()?;
        store
            .append(audit::AuditEvent::new("host", action, detail).with_agent(&self.agent_id))
            .ok()
    }

    /// Open run provenance: mint the id, append `run.start` and index it.
    ///
    /// Best effort by construction: provenance must never be able to fail a run,
    /// so every error path here degrades to `None`.
    fn begin_run(
        &self,
        session_id: Option<&str>,
        parent_run_id: Option<&str>,
        resumed_from_snapshot: Option<&str>,
    ) -> Option<String> {
        let run_id = Self::new_run_id();
        let config = self.run_fingerprint();
        let detail = audit::run_start_detail(
            &run_id,
            session_id,
            parent_run_id,
            resumed_from_snapshot,
            &config,
        );
        let stored = self.append_host_event(audit::ACTION_RUN_START, detail)?;
        if let Ok(mut slot) = self.current_run_id.lock() {
            *slot = Some(run_id.clone());
        }
        if let Ok(mut slot) = self.last_run_id.lock() {
            *slot = Some(run_id.clone());
        }
        let row = audit::RunRecord {
            run_id: run_id.clone(),
            session_id: session_id.map(str::to_string),
            parent_run_id: parent_run_id.map(str::to_string),
            resumed_from_snapshot: resumed_from_snapshot.map(str::to_string),
            fingerprint: audit::fingerprint(&config),
            fingerprint_schema: audit::FINGERPRINT_SCHEMA_V1.to_string(),
            started_at_ms: stored.event.timestamp_ms,
            ended_at_ms: None,
            start_seq: stored.id,
            end_seq: None,
            status: audit::RunStatus::Open,
        };
        if let Ok(mut store) = self.audit.lock() {
            let _ = store.index_run_start(&row);
        }
        Some(run_id)
    }

    /// Close run provenance: append `run.end` and close the index row.
    fn finish_run(&self, run_id: &str, status: audit::RunStatus, reason: &str) {
        let detail = audit::run_end_detail(run_id, status, reason);
        let Some(stored) = self.append_host_event(audit::ACTION_RUN_END, detail) else {
            return;
        };
        if let Ok(mut store) = self.audit.lock() {
            let _ = store.index_run_end(run_id, stored.id, stored.event.timestamp_ms, status);
        }
        if let Ok(mut slot) = self.current_run_id.lock() {
            if slot.as_deref() == Some(run_id) {
                *slot = None;
            }
        }
    }

    /// Abandon the runs a **previous** process left open (v0.4 1e).
    ///
    /// A run whose process disappeared keeps `end_seq = NULL` — nothing fabricates
    /// an end — so without this hook such a run stays `open` for ever and has to be
    /// recognised by hand. The hook appends one `host.run.abandoned` event per
    /// stale run (an ordinary chained event) and rebuilds the derived index.
    ///
    /// Boundaries: runs started by **this** process (`start_seq > startup_seq`) are
    /// never touched, because they may be running right now; a run the chain
    /// already marks `abandoned` is skipped, so repeated starts do not append a
    /// second marker; normally closed runs are skipped as well.
    ///
    /// Returns the ids it abandoned, oldest first.
    pub fn abandon_stale_runs(&self) -> Result<Vec<String>, HostError> {
        let runs = {
            let store = self
                .audit
                .lock()
                .map_err(|_| HostError::Other("audit mutex poisoned".into()))?;
            let (runs, _) = store
                .derive_runs()
                .map_err(|e| HostError::Other(e.to_string()))?;
            runs
        };

        let mut abandoned = Vec::new();
        for run in runs {
            if run.end_seq.is_some()
                || run.status != audit::RunStatus::Open
                || run.start_seq > self.startup_seq
            {
                continue;
            }
            let detail = serde_json::json!({
                "run_id": run.run_id,
                "detected_at_ms": now_ms(),
            });
            if self
                .append_host_event(audit::ACTION_RUN_ABANDONED, detail)
                .is_some()
            {
                abandoned.push(run.run_id);
            }
        }

        if !abandoned.is_empty() {
            // Keep the derived index in step with the chain we just extended.
            if let Ok(mut store) = self.audit.lock() {
                let _ = store.rebuild_run_index();
            }
        }
        Ok(abandoned)
    }

    // ----- VM lifecycle -----------------------------------------------------

    /// Is the host currently holding a **live** VM?
    ///
    /// `vm_slot` alone is not enough: QEMU can exit on its own (the guest shuts
    /// down, the process is killed, QEMU crashes) while its handle stays in the
    /// slot, which kept the badge on "VM 运行中" forever (v0.3.1 #3). The child
    /// process is checked too, and a dead handle is dropped so the next
    /// `start_vm` sees an empty slot.
    ///
    /// **Not a pure query — it performs lazy cleanup.** When the child process is
    /// gone this call drops the handle (the slot becomes `None`) and clears the
    /// VM start time. Call it only from callers that accept that side effect:
    /// never while another `vm_slot` guard is held (that would deadlock), and not
    /// from a path that needs the dead handle to survive. Use the slot directly
    /// when a side-effect-free check is required.
    pub fn vm_is_running(&self) -> bool {
        let (had_vm, alive) = {
            let mut slot = match self.vm_slot.lock() {
                Ok(g) => g,
                Err(_) => return false,
            };
            match slot.as_mut() {
                Some(vm) => (true, vm.is_running()),
                None => (false, false),
            }
        };
        if had_vm && !alive {
            // The lock is released before taking the timestamp lock again.
            if let Ok(mut slot) = self.vm_slot.lock() {
                *slot = None;
            }
            self.clear_vm_started();
        }
        alive
    }

    /// VM status for the top-bar badge (v0.3 #4c).
    ///
    /// Inherits the lazy cleanup of [`Self::vm_is_running`]: a stale handle is
    /// dropped here too.
    pub fn vm_status(&self) -> VmStatusView {
        let running = self.vm_is_running();
        let since_ms = self
            .vm_started_at_ms
            .lock()
            .ok()
            .and_then(|g| *g)
            .filter(|_| running);
        VmStatusView { running, since_ms }
    }

    /// The user's manually chosen QEMU, if one is configured (v0.3 #5b).
    ///
    /// Every path that builds a `VMConfig` (the agent loop **and** a snapshot
    /// restore) must go through this, so a manual path always wins over
    /// auto-discovery.
    fn manual_qemu_path(&self) -> Option<PathBuf> {
        self.qemu_path.lock().ok().and_then(|g| g.clone())
    }

    /// Remember when a VM (re)appeared in the slot.
    fn mark_vm_started(&self) {
        if let Ok(mut slot) = self.vm_started_at_ms.lock() {
            if slot.is_none() {
                *slot = Some(now_ms());
            }
        }
    }

    /// Forget the VM start time (the slot is empty again).
    fn clear_vm_started(&self) {
        if let Ok(mut slot) = self.vm_started_at_ms.lock() {
            *slot = None;
        }
    }

    /// Stop the host-owned VM and clear the slot (no-op when empty).
    pub fn stop_current_vm(&self) -> Result<(), HostError> {
        let taken = self
            .vm_slot
            .lock()
            .map_err(|_| HostError::Other("vm slot poisoned".into()))?
            .take();
        if let Some(mut vm) = taken {
            vm.stop().map_err(|e| HostError::Other(e.to_string()))?;
        }
        self.clear_vm_started();
        // Nothing is running, so nothing has a provenance (v0.9 sandbox F2d).
        self.mark_active_sandbox(None);
        Ok(())
    }

    /// Run one agent turn, emitting host events through `emitter`.
    ///
    /// The node's own sandbox: no task declared one, so the run uses whatever this
    /// node is running, or its default (v0.9 sandbox F2d keeps this signature — the
    /// three surfaces that *can* declare one call [`Self::run_agent_for`]).
    pub fn run_agent(
        &self,
        emitter: Arc<dyn EventSink>,
        user_input: &str,
    ) -> Result<AgentOutcomeView, HostError> {
        self.run_agent_for(emitter, user_input, None)
    }

    /// Run one agent turn under a sandbox the caller declares (v0.9 sandbox F2d).
    ///
    /// `sandbox` is a **declaration**, never a node change: it decides which
    /// definition this run's VM comes from (toolchain, QEMU, memory), and it leaves
    /// `current_sandbox` alone. Two refusals guard that:
    ///
    /// - an unknown name is `404` (`cause: "name"`) — a typo must not look like a
    ///   successful run under some other sandbox;
    /// - a name that is not the one the **running** VM came from is a `409`
    ///   (`cause: "sandbox"`): replacing a VM mid-run is what the switch is for, and
    ///   the switch needs `sandbox.switch` and is an explicit node change.
    ///
    /// The declaration reaches the loop as the compiler, the QEMU path and the
    /// guest's memory. It does **not** reach the kernel: which ELF to boot is still
    /// the model's `start_vm` argument (F2d decision 2 — `def.kernel` is what the
    /// *switch* boots, not what a run boots).
    pub fn run_agent_for(
        &self,
        emitter: Arc<dyn EventSink>,
        user_input: &str,
        sandbox: Option<&str>,
    ) -> Result<AgentOutcomeView, HostError> {
        // Which definition this run uses (F2d), and whether the node can honour the
        // declaration at all. Both are the **caller's** side of the question — a name
        // nobody has, or a clash with the running VM — so they are answered before
        // the environment is asked, the way a bad parameter is a `400` before a `503`.
        let resolved = self.resolve_task_sandbox(sandbox)?;
        if let Some(name) = sandbox {
            self.check_task_sandbox(name)?;
        }
        // Readiness gate: never enter the loop when the LLM is not usable. Typed,
        // so the HTTP surface can answer its documented `503 unavailable` with
        // `cause: "llm"` without checking readiness twice (the route used to; F2d
        // moved the whole refusal ladder into this one function, so the declaration
        // is answered the same way whichever surface asked).
        let readiness = self.llm_readiness();
        if !readiness.ready {
            return Err(HostError::NotConfigured(readiness_error(&readiness)));
        }
        // Toolchain pre-check: never enter the loop without a working compiler.
        // A resolved definition carries its own QEMU and toolchain questions, so it
        // is asked them (`sandbox_check`); with none, the node's own probes run —
        // which is also what the built-in fallback definition ends up asking.
        match &resolved {
            Some((def, _)) => self.sandbox_check(def)?,
            None => {
                let toolchain = self.probe_toolchain();
                if !toolchain.found {
                    return Err(HostError::Other(format!(
                        "toolchain_missing\n{}",
                        toolchain.diagnostics
                    )));
                }
                let qemu = self.probe_qemu();
                if !qemu.found {
                    return Err(HostError::Other(format!(
                        "qemu_missing\n{}",
                        qemu.diagnostics
                    )));
                }
            }
        }
        // Where the slot stood before this run, so a VM started *by* it can be
        // recorded as the definition's (F2d).
        let vm_before = self.vm_is_running();
        let resolved_name = resolved.as_ref().map(|(_, name)| name.clone());
        // Environment preflight (v0.4 batch 3): warn-only, and only when the cache
        // has no result for this configuration. It never fails the run — a broken
        // environment surfaces through the run's own errors — and the user can
        // inspect or bypass the warning in settings.
        if let Ok(view) = self.ensure_preflight(false, Some(Arc::clone(&emitter))) {
            if !view.ok && !view.overridden {
                eprintln!(
                    "preflight: {} — {}",
                    view.failed_step.clone().unwrap_or_default(),
                    view.detail.clone().unwrap_or_default()
                );
            }
        }
        let llm = self.build_llm()?;
        let policy = WorkspacePolicy::new(self.workspace_root.clone());
        let system_prompt = format!("{CONSTITUTION}\n\n{}", agent::prompt::OPERATING_RULES);
        let mut agent = AgentLoop::with_vm(
            llm,
            self.agent_config(),
            policy,
            Arc::clone(&self.sink),
            Arc::clone(&self.vm_slot),
            system_prompt,
            self.agent_id.clone(),
        )?;
        // Host-owned VM: the loop works on `vm_slot`; serial bytes go to the
        // same broadcast list that the long-lived forwarder reads from.
        agent.attach_serial(Arc::clone(&self.serial_senders));
        // Host-configured toolchain (falls back to auto-discovery).
        agent.set_compiler(self.toolchain_config());
        // The AI's sandbox tools: this agent may **ask** for a sandbox change, not
        // make one (F2c decision 4). The gateway holds the queue and this run's
        // emitter — never `Arc<AppState>`, which the loop already lives inside.
        agent.with_sandbox_requester(Some(Arc::new(ToolSandboxRequests {
            requests: self.sandbox_requests(),
            sink: Arc::clone(&emitter),
            agent_id: self.agent_id.clone(),
        })));
        // Host-configured QEMU (falls back to the sandbox's discovery).
        if let Some(path) = self.manual_qemu_path() {
            agent.set_qemu_path(path);
        }
        // The declared sandbox has the last word on how this run's VM is configured
        // (v0.9 sandbox F2d): its toolchain, its QEMU, its memory. A field it leaves
        // empty keeps whatever the node set above.
        if let Some((def, _)) = &resolved {
            if let Some(path) = &def.toolchain_path {
                agent.set_compiler(agent::CompilerConfig::manual(path.clone()));
            }
            if let Some(path) = &def.qemu_exe {
                agent.set_qemu_path(path.clone());
            }
            if let Some(memory_mb) = def.memory_mb {
                agent.set_memory_mb(memory_mb);
            }
        }

        // Sessions: restore prior turns, then persist whatever this turn adds.
        let session_id = self.ensure_session(user_input)?;
        let history = self.load_session_history(&session_id)?;
        if !history.is_empty() {
            agent.push_history(history);
        }
        let pre_len = agent.messages().len();

        // Run provenance (v0.4 1c): one run = one `run_agent` call. The markers
        // are ordinary chained events; the id itself stays host-side and is never
        // handed to the agent or the sandbox.
        let run_id = self.begin_run(Some(&session_id), None, None);

        // Stream subscription: forwarded as `agent:stream:delta` / `:done`.
        if let Ok(mut slot) = self.stream_receiver.lock() {
            *slot = Some(agent.subscribe_stream());
        }

        // Bridge: poll the audit log for agent:* / vm:state host events.
        let stop = Arc::new(AtomicBool::new(false));
        let stop_bridge = Arc::clone(&stop);
        let audit = Arc::clone(&self.audit);
        let vm_started_at = Arc::clone(&self.vm_started_at_ms);
        let bridge_emitter = Arc::clone(&emitter);
        let bridge = std::thread::spawn(move || {
            let mut bridge = AuditBridge::new(audit, bridge_emitter, Arc::clone(&vm_started_at));
            while !stop_bridge.load(Ordering::Relaxed) {
                bridge.tick();
                std::thread::sleep(POLL_INTERVAL);
            }
            bridge.tick(); // final flush
        });

        // Forwarder: LLM stream -> `agent:stream:delta` / `agent:stream:done`.
        let stop_stream = Arc::clone(&stop);
        let stream_slot = Arc::clone(&self.stream_receiver);
        let stream_emitter = Arc::clone(&emitter);
        let stream_thread = std::thread::spawn(move || {
            let rx = match stream_slot.lock() {
                Ok(mut slot) => slot.take(),
                Err(_) => None,
            };
            let Some(rx) = rx else { return };

            let forward = |event: StreamEvent| match event {
                StreamEvent::Delta(text) => {
                    stream_emitter.emit(EV_AGENT_STREAM_DELTA, serde_json::json!({ "text": text }));
                }
                // Tool calls are surfaced by the `agent:tool_call` event.
                StreamEvent::ToolCallDelta { .. } => {}
                StreamEvent::Done => {
                    stream_emitter.emit(EV_AGENT_STREAM_DONE, serde_json::json!({}));
                }
            };

            while !stop_stream.load(Ordering::Relaxed) {
                match rx.recv_timeout(Duration::from_millis(100)) {
                    Ok(event) => forward(event),
                    Err(RecvTimeoutError::Timeout) => continue,
                    Err(RecvTimeoutError::Disconnected) => break,
                }
            }

            // Grace drain: capture any tail events still in flight.
            let mut idle = 0;
            while idle < 10 {
                match rx.try_recv() {
                    Ok(event) => {
                        forward(event);
                        idle = 0;
                    }
                    Err(_) => {
                        idle += 1;
                        std::thread::sleep(Duration::from_millis(30));
                    }
                }
            }
        });

        let outcome = agent.run(user_input);

        stop.store(true, Ordering::Relaxed);
        let _ = bridge.join();
        let _ = stream_thread.join();

        // Keep the VM badge in sync with the host-owned slot: a VM that
        // survived the run keeps its start time, an empty slot clears it.
        if self.vm_is_running() {
            self.mark_vm_started();
            // A VM appeared during this run: it came from the definition this run
            // resolved to (v0.9 sandbox F2d). This is the host's best view of the
            // tool-started VM, which the tool itself cannot report from the agent
            // crate.
            if !vm_before {
                self.mark_active_sandbox(resolved_name.as_deref());
            }
        } else {
            self.clear_vm_started();
        }

        // Close run provenance on both paths: a failed run is `failed`, not
        // open. Only a run whose process never returns stays open (§4.2).
        if let Some(run_id) = &run_id {
            // Only a run that produced a final answer counts as `ok`. A truncated
            // run (`max_iterations`) did not complete either, so it is `failed`
            // with a reason that says which case it was; `interrupted` stays
            // reserved for a human stop.
            let (status, reason) = match &outcome {
                Ok(AgentOutcome::Final { iterations, .. }) => (
                    audit::RunStatus::Ok,
                    format!("final answer after {iterations} iterations"),
                ),
                Ok(AgentOutcome::MaxIterations { iterations, .. }) => (
                    audit::RunStatus::Failed,
                    format!("iteration cap reached ({iterations}) without a final answer"),
                ),
                Ok(AgentOutcome::Failed { reason, iterations }) => (
                    audit::RunStatus::Failed,
                    format!("agent run failed after {iterations} iterations: {reason}"),
                ),
                Err(e) => (audit::RunStatus::Failed, format!("host error: {e}")),
            };
            self.finish_run(run_id, status, &reason);
        }

        let outcome = outcome?;

        // Persist the turn (system messages are skipped inside).
        self.persist_turn(&session_id, agent.messages(), pre_len)?;

        let view = AgentOutcomeView::from(outcome);
        emitter.emit(
            EV_AGENT_FINAL,
            serde_json::to_value(&view).unwrap_or(serde_json::Value::Null),
        );
        // Anything the audit layer could not write during this run is announced
        // here (v0.8): after the run, while the emitter is still in hand.
        self.emit_audit_failures(emitter.as_ref());
        Ok(view)
    }
}

/// `Arc<dyn LlmClient>` as a `Box<dyn LlmClient>`.
struct ArcLlm(Arc<dyn LlmClient>);

impl LlmClient for ArcLlm {
    fn chat(&self, req: ChatRequest) -> Result<ChatResponse, agent::AgentError> {
        self.0.chat(req)
    }

    // Must forward streaming too, otherwise the default (single-delta)
    // implementation would be used instead of the wrapped client's.
    fn chat_stream(
        &self,
        req: ChatRequest,
        on_event: &mut dyn FnMut(StreamEvent),
    ) -> Result<ChatResponse, agent::AgentError> {
        self.0.chat_stream(req, on_event)
    }
}

/// The agent loop's view of the request queue (v0.9 sandbox F2c).
///
/// The loop is stored inside `AppState`, so it must not hold an `Arc<AppState>`:
/// that would be a cycle through the heap that never frees. It holds the queue
/// and this run's sink instead — the same shape as the VM slot it was already
/// handed. The rule the surface enforces stays the same either way: this may
/// **ask**, and only an actor holding the capability may decide.
struct ToolSandboxRequests {
    requests: SandboxRequests,
    sink: Arc<dyn EventSink>,
    agent_id: String,
}

impl agent::SandboxRequester for ToolSandboxRequests {
    fn request(
        &self,
        action: &str,
        sandbox: Option<&str>,
        reason: Option<&str>,
    ) -> Result<String, String> {
        let action = SandboxAction::parse(action).ok_or_else(|| {
            format!("unknown action {action:?}: expected switch, define or assemble")
        })?;
        SandboxRequestService::new(self.requests.clone(), Arc::clone(&self.sink))
            .request(
                // Who asked: the host's agent id, so the queue says which agent
                // wanted the change, not just "an agent".
                &self.agent_id,
                action,
                sandbox.map(str::to_string),
                // The tool carries no definition (F2c decision 4): a model does
                // not get to name a path, and `SandboxDef` carries paths.
                None,
                reason.map(str::to_string),
            )
            .map(|view| view.id)
            .map_err(|e| e.user_message())
    }

    fn status(&self) -> Result<String, String> {
        Ok(self.requests.summary())
    }
}

/// Turns new audit events into frontend events, and tracks serial increments.
struct AuditBridge {
    audit: Arc<Mutex<AuditStore>>,
    emitter: Arc<dyn EventSink>,
    vm_started_at: Arc<Mutex<Option<i64>>>,
    last_id: i64,
}

impl AuditBridge {
    fn new(
        audit: Arc<Mutex<AuditStore>>,
        emitter: Arc<dyn EventSink>,
        vm_started_at: Arc<Mutex<Option<i64>>>,
    ) -> Self {
        Self {
            audit,
            emitter,
            vm_started_at,
            last_id: 0,
        }
    }

    /// `vm:state` payload shared by every VM lifecycle event.
    ///
    /// `name` is always present (v0.9): `null` for a start/stop, the snapshot's
    /// name for a save. A client tests `state == "snapshot"`.
    fn vm_payload(&self, state: &str, name: Option<&str>) -> serde_json::Value {
        let since = self.vm_started_at.lock().ok().and_then(|g| *g);
        crate::events::vm_state_payload(state, since.is_some(), since, name)
    }

    /// Record/clear the VM start time as the sandbox reports it.
    fn set_vm_started(&self, started: bool) {
        if let Ok(mut slot) = self.vm_started_at.lock() {
            *slot = if started { Some(now_ms()) } else { None };
        }
    }

    fn tick(&mut self) {
        let events = match self.audit.lock() {
            Ok(store) => store.all().unwrap_or_default(),
            Err(_) => return,
        };

        let last = self.last_id;
        let mut new_max = self.last_id;
        for e in events.iter().filter(|e| e.id > last) {
            match e.event.action.as_str() {
                "agent.llm.request" => self.emitter.emit(
                    EV_AGENT_ITERATION,
                    serde_json::json!({
                        "model": e.event.detail.get("model"),
                        "messages": e.event.detail.get("messages"),
                    }),
                ),
                "agent.tool.call" => self.emitter.emit(
                    EV_AGENT_TOOL_CALL,
                    serde_json::json!({
                        "name": e.event.detail.get("name"),
                        "arguments": e.event.detail.get("arguments"),
                    }),
                ),
                "agent.tool.result" => self.emitter.emit(
                    EV_AGENT_TOOL_RESULT,
                    serde_json::json!({
                        "ok": e.event.detail.get("ok"),
                        "result": e.event.detail.get("result"),
                    }),
                ),
                "vm.start" => {
                    self.set_vm_started(true);
                    let payload = self.vm_payload("running", None);
                    self.emitter.emit(EV_VM_STATE, payload);
                }
                "vm.stop" => {
                    self.set_vm_started(false);
                    let payload = self.vm_payload("stopped", None);
                    self.emitter.emit(EV_VM_STATE, payload);
                }
                "vm.snapshot.save" => {
                    let name = e.event.detail.get("name").and_then(|v| v.as_str());
                    let payload = self.vm_payload("snapshot", name);
                    self.emitter.emit(EV_VM_STATE, payload);
                }
                _ => {}
            }
            new_max = new_max.max(e.id);
        }
        self.last_id = new_max;
    }
}

/// Append a serial chunk to the accumulated text (lossy UTF-8) and return the
/// text that was appended.
pub fn append_serial_chunk(accum: &Mutex<String>, bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes).to_string();
    if let Ok(mut guard) = accum.lock() {
        guard.push_str(&text);
    }
    text
}

/// Fetch `/v1/models` and return up to 20 model ids (silent on any failure).
fn fetch_models(client: &reqwest::blocking::Client, url: &str) -> Option<Vec<String>> {
    let response = client.get(url).send().ok()?;
    if !response.status().is_success() {
        return None;
    }
    let body: serde_json::Value = response.json().ok()?;
    let data = body.get("data")?.as_array()?;
    let models: Vec<String> = data
        .iter()
        .filter_map(|m| m.get("id").and_then(|v| v.as_str()).map(String::from))
        .take(20)
        .collect();
    if models.is_empty() {
        None
    } else {
        Some(models)
    }
}

fn collect_files(root: &Path, dir: &Path, out: &mut Vec<String>) -> Result<(), HostError> {
    if !dir.exists() {
        return Ok(());
    }
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') {
            continue;
        }
        let path = entry.path();
        if path.is_dir() {
            collect_files(root, &path, out)?;
        } else if let Ok(rel) = path.strip_prefix(root) {
            out.push(rel.to_string_lossy().replace('\\', "/"));
        }
    }
    Ok(())
}

/// Does a task's declared sandbox clash with what is running (v0.9 sandbox F2d)?
///
/// A function of what the node looks like right now, so the rule can be pinned
/// without a live guest: a declaration is honoured when nothing is running (the VM
/// this run starts will be the declared one), and when the running VM came from the
/// very definition the task named. Otherwise the run is refused, and the message
/// names both ways out — because a task **declares** and only a switch **changes**.
fn task_sandbox_conflict(
    vm_running: bool,
    active: Option<&str>,
    requested: &str,
) -> Option<String> {
    if !vm_running || active == Some(requested) {
        return None;
    }
    Some(format!(
        "a VM is already running ({}) and a task may not switch the node; stop it, or switch \
         to {requested:?} with POST /v0/sandboxes/switch",
        active.unwrap_or("from a snapshot or an earlier run")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A workspace with the given hand-written definitions already on disk.
    ///
    /// `AppState` reads its settings at construction, so the file has to exist
    /// first; `version` is not optional in the file (a malformed one would load as
    /// "no definitions", and the test would pass for the wrong reason).
    fn state_with(tag: &str, settings: serde_json::Value) -> AppState {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "riscdom-state-{tag}-{}-{nanos}",
            std::process::id()
        ));
        std::fs::create_dir_all(root.join(".riscdom")).unwrap();
        let mut settings = settings;
        settings["version"] = serde_json::json!(1);
        std::fs::write(
            root.join(".riscdom").join("settings.json"),
            serde_json::to_vec_pretty(&settings).unwrap(),
        )
        .unwrap();
        AppState::in_memory(&root).expect("state")
    }

    #[test]
    fn the_running_nodes_sandbox_outranks_the_configured_default() {
        // Nothing but a successful switch writes `current_sandbox`, and a switch
        // boots a guest — so the middle step of the order is pinned here, where the
        // field is reachable, and not through a switch.
        let state = state_with(
            "current-wins",
            serde_json::json!({
                "sandboxes": [
                    { "name": "running", "memory_mb": 256 },
                    { "name": "configured", "memory_mb": 512 },
                ],
                "default_sandbox": "configured",
            }),
        );
        *state.current_sandbox.lock().unwrap() = Some("running".to_string());
        let (def, name) = state
            .resolve_task_sandbox(None)
            .expect("resolved")
            .expect("a definition");
        assert_eq!(name, "running", "the node's own sandbox must win");
        assert_eq!(def.memory_mb, Some(256));
    }

    #[test]
    fn a_stale_current_name_falls_through_to_the_configured_default() {
        let state = state_with(
            "stale-current",
            serde_json::json!({
                "sandboxes": [{ "name": "configured", "memory_mb": 512 }],
                "default_sandbox": "configured",
            }),
        );
        *state.current_sandbox.lock().unwrap() = Some("removed-by-hand".to_string());
        let (_, name) = state
            .resolve_task_sandbox(None)
            .expect("resolved")
            .expect("a definition");
        assert_eq!(name, "configured");
    }

    #[test]
    fn a_declaration_clashes_only_with_a_running_vm_from_another_definition() {
        // The four cases of F2d decision 1, as a rule: this is what the run checks.
        assert!(task_sandbox_conflict(false, None, "blink").is_none());
        assert!(task_sandbox_conflict(false, Some("other"), "blink").is_none());
        assert!(task_sandbox_conflict(true, Some("blink"), "blink").is_none());

        let clash = task_sandbox_conflict(true, Some("other"), "blink").expect("a conflict");
        assert!(clash.contains("other"), "{clash}");
        assert!(clash.contains("blink"), "{clash}");
        assert!(
            clash.contains("POST /v0/sandboxes/switch"),
            "the refusal names the way out: {clash}"
        );

        // A VM nobody recorded a definition for (a restored snapshot on a node with
        // no current sandbox) is still not the declared one.
        let unknown = task_sandbox_conflict(true, None, "blink").expect("a conflict");
        assert!(
            unknown.contains("from a snapshot or an earlier run"),
            "{unknown}"
        );
    }

    #[test]
    fn a_vm_that_appears_during_a_run_is_attributed_to_that_run() {
        // The host's only view of a tool-started VM is the slot: `start_vm` runs
        // inside the agent crate and cannot report it. So the rule is "the slot was
        // empty before, occupied after" — and `stop_current_vm` clears it again.
        let state = state_with("provenance", serde_json::json!({}));
        assert_eq!(state.active_sandbox(), None);
        state.mark_active_sandbox(Some("blink"));
        assert_eq!(state.active_sandbox().as_deref(), Some("blink"));
        state.mark_active_sandbox(None);
        assert_eq!(state.active_sandbox(), None);
    }

    /// A refusal that has nothing to do with `exec` is not retried (v0.9).
    ///
    /// The attempt count is what makes this checkable: a path that does not exist fails
    /// on the first attempt, and the helper has to return there rather than spend the
    /// budget.
    #[test]
    fn a_probe_that_cannot_start_is_not_retried() {
        let mut command = std::process::Command::new("definitely-not-here-riscdom");
        let mut attempts = 0;
        let result = exec_retrying(
            &mut command,
            &mut attempts,
            EXEC_MAX_ATTEMPTS,
            EXEC_RETRY_DELAY,
        );
        assert_eq!(attempts, 1, "a missing file must not be retried");
        assert_eq!(
            result.expect_err("cannot start").kind(),
            std::io::ErrorKind::NotFound
        );
    }

    /// The retry matches the error the kernel really returns for a busy executable.
    ///
    /// Unix only, and deliberately so: `ETXTBSY` is errno 26 there, while 26 on Windows is
    /// an unrelated code — which is exactly why the helper matches on `kind()` and not on
    /// `raw_os_error()`.
    #[cfg(unix)]
    #[test]
    fn an_executable_busy_errno_is_the_kind_the_retry_matches() {
        assert_eq!(
            std::io::Error::from_raw_os_error(26).kind(),
            std::io::ErrorKind::ExecutableFileBusy
        );
    }

    /// An `exec` the kernel refuses because the file is being written is retried, and it
    /// succeeds once the writer lets go.
    ///
    /// The refusal is produced for real: this test holds a **write** handle on the script
    /// while the first attempt is made and releases it from another thread. The kernel's
    /// rule is about **any** process and this process is one; the window a test binary can
    /// also hit — a sibling thread that forked a child which has not exec'd yet — is the
    /// same rule with a shorter lever.
    #[cfg(unix)]
    #[test]
    fn an_exec_that_is_busy_is_retried_until_it_succeeds() {
        use std::os::unix::fs::PermissionsExt;

        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("riscdom-busy-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let script = dir.join("busy-probe.sh");
        std::fs::write(&script, "#!/bin/sh\nexit 0\n").unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();

        // Held open for writing, and let go after about two attempts' worth of time.
        let held = std::fs::OpenOptions::new()
            .write(true)
            .open(&script)
            .unwrap();
        let release = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(15));
            drop(held);
        });

        let mut attempts = 0;
        let result = exec_retrying(
            &mut std::process::Command::new(&script),
            &mut attempts,
            EXEC_MAX_ATTEMPTS,
            EXEC_RETRY_DELAY,
        );
        release.join().unwrap();

        assert!(
            result.is_ok(),
            "the budget must outlast the writer: {:?}",
            result.err()
        );
        assert!(attempts > 1, "the first attempt must have been refused");
    }
}
