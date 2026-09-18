//! Application state shared by all Tauri commands.

use crate::error::HostError;
use crate::events::{
    EventSink, EV_AGENT_FINAL, EV_AGENT_ITERATION, EV_AGENT_STREAM_DELTA, EV_AGENT_STREAM_DONE,
    EV_AGENT_TOOL_CALL, EV_AGENT_TOOL_RESULT, EV_SERIAL_CHUNK, EV_VM_STATE,
};
use crate::keyring::{user_for_provider, InMemoryKeyring, KeyringBackend, OsKeyring, SERVICE};
use crate::session::{SessionMessage, SessionMeta, SessionStore};
use crate::settings::LocalSettings;
use crate::toolchain_download::DownloadEvent;
use agent::llm::{DeepSeekClient, LlmClient};
use agent::message::{ChatMessage, ChatRequest, ChatResponse, StreamEvent};
use agent::policy::WorkspacePolicy;
use agent::presets::{builtin_presets, find_preset, ProviderPreset, DEFAULT_PRESET_ID};
use agent::{AgentConfig, AgentLoop, AgentOutcome};
use audit::{AuditSink, AuditStore, ChainStatus, SqliteAuditSink, StoredEvent};
use sandbox::platform::{QmpEndpoint, SerialEndpoint};
use sandbox::vm::{RiscVVirtualMachine, VMConfig};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// The constitution is embedded so the desktop app works without the repo.
const CONSTITUTION: &str = include_str!("../../AGENTS.md");

/// Poll interval for the audit/serial bridge.
pub const POLL_INTERVAL: Duration = Duration::from_millis(200);

/// The snapshot mechanism the host uses, as recorded in a run's fingerprint and
/// in the snapshot listing.
const SNAPSHOT_MODE: &str = "tcp-relay";

/// The older reboot-fallback snapshots the host still lists and deletes.
const SNAPSHOT_FALLBACK_MODE: &str = "reboot-fallback";

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
}

/// Download status for the UI / polling clients.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ToolchainDownloadStatus {
    pub in_progress: bool,
    pub last_event: Option<DownloadEvent>,
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

/// Run `<path> --version`; returns its first output line, or the raw error text
/// (callers add their own, single, `not runnable:` prefix).
fn toolchain_runs(path: &Path) -> Result<String, String> {
    match std::process::Command::new(path).arg("--version").output() {
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
    /// Host-owned VM slot: a VM started during a run stays here after the run
    /// ends, so later runs reuse the same guest (v0.2 host-owned lifecycle).
    pub vm_slot: Arc<Mutex<Option<RiscVVirtualMachine>>>,
    /// User-chosen RISC-V GCC (`set_toolchain_path`); `None` means auto-discovery.
    pub toolchain_path: Mutex<Option<PathBuf>>,
    /// User-chosen QEMU (`set_qemu_path`); `None` means auto-discovery.
    pub qemu_path: Mutex<Option<PathBuf>>,
    /// In-flight toolchain download (v0.3 #3b). `Arc` so the worker thread can
    /// be handed the cancel flag and clear the slot when it finishes.
    pub toolchain_download: Arc<Mutex<Option<ToolchainDownloadState>>>,
    /// When the host-owned VM started (epoch ms); shared with the audit bridge,
    /// which learns about VM starts/stops from the sandbox's audit events.
    vm_started_at_ms: Arc<Mutex<Option<i64>>>,
    /// Last download event seen, kept after the download ends (for polling).
    toolchain_download_last: Mutex<Option<DownloadEvent>>,
    /// Non-secret local settings mirrored to `settings.json`.
    settings: Mutex<LocalSettings>,
    /// Where `settings.json` lives.
    settings_path: PathBuf,
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
        Ok(state)
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
        let sink: Arc<Mutex<dyn AuditSink>> = Arc::new(Mutex::new(SqliteAuditSink::from_shared(
            Arc::clone(&shared),
        )));
        let state = Self {
            audit: shared,
            sink,
            vm_slot: Arc::new(Mutex::new(None)),
            toolchain_path: Mutex::new(None),
            qemu_path: Mutex::new(None),
            toolchain_download: Arc::new(Mutex::new(None)),
            vm_started_at_ms: Arc::new(Mutex::new(None)),
            toolchain_download_last: Mutex::new(None),
            settings: Mutex::new(LocalSettings::default()),
            settings_path: crate::paths::settings_path(),
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
        state
    }

    // ----- Snapshots --------------------------------------------------------

    /// Snapshot directory used by the sandbox (`<workspace>/.riscdom/snapshots`).
    fn snapshot_dir(&self) -> PathBuf {
        self.workspace_root.join(".riscdom").join("snapshots")
    }

    /// Snapshots present on disk: real (`.mig`) and reboot-fallback (`.json`).
    pub fn list_snapshots(&self) -> Result<Vec<SnapshotMetaView>, HostError> {
        let dir = self.snapshot_dir();
        if !dir.exists() {
            return Ok(Vec::new());
        }
        let mut out = Vec::new();
        for entry in std::fs::read_dir(&dir)? {
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
        out.sort_by_key(|s| std::cmp::Reverse(s.created_at_ms));
        Ok(out)
    }

    /// Delete a snapshot (either mode). Returns whether a file was removed.
    pub fn delete_snapshot(&self, name: &str) -> Result<bool, HostError> {
        let dir = self.snapshot_dir();
        let mut removed = false;
        for ext in [sandbox::SNAPSHOT_MIG_EXT, sandbox::SNAPSHOT_JSON_EXT] {
            let path = dir.join(format!("{name}.{ext}"));
            if path.is_file() {
                std::fs::remove_file(&path)?;
                removed = true;
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
        let bytes = std::fs::metadata(
            self.snapshot_dir()
                .join(format!("{name}.{}", sandbox::SNAPSHOT_MIG_EXT)),
        )
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
        let path = self
            .snapshot_dir()
            .join(format!("{name}.{}", sandbox::SNAPSHOT_MIG_EXT));
        if !path.is_file() {
            return Err(HostError::Other(format!("snapshot not found: {name}")));
        }

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
            let config = VMConfig {
                kernel: self.resume_kernel()?,
                memory_mb: agent::VM_MEMORY_MB,
                qmp: QmpEndpoint::tcp(
                    "127.0.0.1",
                    sandbox::relay::free_local_port()
                        .map_err(|e| HostError::Other(e.to_string()))?,
                ),
                serial: SerialEndpoint::tcp(
                    "127.0.0.1",
                    sandbox::relay::free_local_port()
                        .map_err(|e| HostError::Other(e.to_string()))?,
                ),
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
    pub fn begin_toolchain_download(
        &self,
        spec: &crate::toolchain_download::DownloadSpec,
    ) -> Result<Arc<AtomicBool>, HostError> {
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
        });
        drop(slot);
        self.emit_host(
            "host.toolchain.download.start",
            serde_json::json!({ "version": spec.version }),
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
        let in_progress = self
            .toolchain_download
            .lock()
            .map(|slot| slot.is_some())
            .unwrap_or(false);
        let last_event = self
            .toolchain_download_last
            .lock()
            .ok()
            .and_then(|event| event.clone());
        ToolchainDownloadStatus {
            in_progress,
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
                let adopted = self.set_toolchain_path(&path);
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

    /// Effective compiler config: an explicit user path wins over discovery.
    fn toolchain_config(&self) -> agent::CompilerConfig {
        match self.toolchain_path.lock().ok().and_then(|g| g.clone()) {
            Some(path) => agent::CompilerConfig::manual(path),
            None => agent::CompilerConfig::from_env(),
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

    /// Read settings from disk and apply them (missing/corrupt → defaults).
    fn load_settings(&self) {
        let loaded = LocalSettings::load(&self.settings_path);
        if let Ok(mut g) = self.settings.lock() {
            *g = loaded.clone();
        }
        if let Ok(mut g) = self.toolchain_path.lock() {
            *g = loaded.toolchain_path.map(PathBuf::from);
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

    /// Record a host-originated audit event (best effort).
    fn emit_host(&self, action: &str, detail: serde_json::Value) {
        if let Ok(mut sink) = self.sink.lock() {
            sink.record(audit::AuditEvent::new("host", action, detail));
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

    // ----- Audit ------------------------------------------------------------

    /// Event count + chain status.
    pub fn audit_status(&self) -> Result<AuditStatusView, HostError> {
        let store = self
            .audit
            .lock()
            .map_err(|_| HostError::Other("audit store lock poisoned".into()))?;
        Ok(AuditStatusView {
            count: store.count()?,
            chain: ChainStatusView::from(audit::verify_chain(&store)?),
        })
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
            .append(audit::AuditEvent::new("host", action, detail))
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
        Ok(())
    }

    /// Run one agent turn, emitting host events through `emitter`.
    pub fn run_agent(
        &self,
        emitter: Arc<dyn EventSink>,
        user_input: &str,
    ) -> Result<AgentOutcomeView, HostError> {
        // Readiness gate: never enter the loop when the LLM is not usable.
        let readiness = self.llm_readiness();
        if !readiness.ready {
            return Err(HostError::Other(readiness_error(&readiness)));
        }
        // Toolchain pre-check: never enter the loop without a working compiler.
        let toolchain = self.probe_toolchain();
        if !toolchain.found {
            return Err(HostError::Other(format!(
                "toolchain_missing\n{}",
                toolchain.diagnostics
            )));
        }
        // QEMU pre-check: the sandbox cannot boot a guest without it.
        let qemu = self.probe_qemu();
        if !qemu.found {
            return Err(HostError::Other(format!(
                "qemu_missing\n{}",
                qemu.diagnostics
            )));
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
        )?;
        // Host-owned VM: the loop works on `vm_slot`; serial bytes go to the
        // same broadcast list that the long-lived forwarder reads from.
        agent.attach_serial(Arc::clone(&self.serial_senders));
        // Host-configured toolchain (falls back to auto-discovery).
        agent.set_compiler(self.toolchain_config());
        // Host-configured QEMU (falls back to the sandbox's discovery).
        if let Some(path) = self.manual_qemu_path() {
            agent.set_qemu_path(path);
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
    fn vm_payload(&self, state: &str) -> serde_json::Value {
        let since = self.vm_started_at.lock().ok().and_then(|g| *g);
        serde_json::json!({
            "state": state,
            "running": since.is_some(),
            "since_ms": since,
        })
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
                    let payload = self.vm_payload("running");
                    self.emitter.emit(EV_VM_STATE, payload);
                }
                "vm.stop" => {
                    self.set_vm_started(false);
                    let payload = self.vm_payload("stopped");
                    self.emitter.emit(EV_VM_STATE, payload);
                }
                "vm.snapshot.save" => {
                    let mut payload = self.vm_payload("snapshot");
                    if let Some(object) = payload.as_object_mut() {
                        object.insert(
                            "name".to_string(),
                            e.event
                                .detail
                                .get("name")
                                .cloned()
                                .unwrap_or(serde_json::Value::Null),
                        );
                    }
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
