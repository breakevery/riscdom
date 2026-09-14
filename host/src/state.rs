//! Application state shared by all Tauri commands.

use crate::error::HostError;
use crate::events::{EventSink, EV_AGENT_FINAL, EV_AGENT_ITERATION, EV_AGENT_TOOL_CALL,
                    EV_AGENT_TOOL_RESULT, EV_SERIAL_CHUNK, EV_VM_STATE};
use crate::keyring::{user_for_provider, InMemoryKeyring, KeyringBackend, OsKeyring, SERVICE};
use agent::llm::{DeepSeekClient, LlmClient};
use agent::message::{ChatRequest, ChatResponse};
use agent::policy::WorkspacePolicy;
use agent::presets::{builtin_presets, find_preset, ProviderPreset, DEFAULT_PRESET_ID};
use agent::{AgentConfig, AgentLoop, AgentOutcome};
use audit::{AuditSink, AuditStore, ChainStatus, SqliteAuditSink, StoredEvent};
use sandbox::vm::RiscVVirtualMachine;
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// The constitution is embedded so the desktop app works without the repo.
const CONSTITUTION: &str = include_str!("../../AGENTS.md");

/// Poll interval for the audit/serial bridge.
pub const POLL_INTERVAL: Duration = Duration::from_millis(200);

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
    ("ollama", "Ollama（本地）", "http://localhost:11434/v1/models"),
    ("ollama", "Ollama（本地）", "http://127.0.0.1:11434/v1/models"),
    ("lmstudio", "LM Studio（本地）", "http://localhost:1234/v1/models"),
    ("lmstudio", "LM Studio（本地）", "http://127.0.0.1:1234/v1/models"),
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
    /// Reserved: the VM slot. In the current MVP the agent loop owns its own VM,
    /// so this stays `None`; it exists for the v0.2 host-owned-VM mode.
    pub vm: Mutex<Option<RiscVVirtualMachine>>,
    pub llm_config: Mutex<Option<LlmConfigInput>>,
    pub workspace_root: PathBuf,
    pub compiler: agent::CompilerConfig,
    /// Test seam: an injected LLM client (bypasses HTTP).
    pub llm_override: Mutex<Option<Arc<dyn LlmClient>>>,
    /// OS keyring backend (or an in-memory one in tests).
    pub keyring: Arc<dyn KeyringBackend>,
    /// Whether the current key has been persisted to the keyring.
    persisted: Mutex<bool>,
}

impl AppState {
    /// Build state backed by an on-disk audit DB under `<workspace>/.riscdom`.
    pub fn new(workspace_root: impl Into<PathBuf>) -> Result<Self, HostError> {
        let root = workspace_root.into();
        std::fs::create_dir_all(&root)?;
        let db_dir = root.join(".riscdom");
        std::fs::create_dir_all(&db_dir)?;
        let store = AuditStore::open(&db_dir.join("audit.db"))?;
        let state = Self::from_store(root, store, Arc::new(OsKeyring::new()));
        state.init_from_env();
        Ok(state)
    }

    /// Build state with a private in-memory audit DB and keyring (tests).
    pub fn in_memory(workspace_root: impl Into<PathBuf>) -> Result<Self, HostError> {
        let root = workspace_root.into();
        std::fs::create_dir_all(&root)?;
        let store = AuditStore::in_memory()?;
        Ok(Self::from_store(
            root,
            store,
            Arc::new(InMemoryKeyring::new()),
        ))
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
        let model =
            std::env::var("DEEPSEEK_MODEL").unwrap_or_else(|_| "deepseek-chat".to_string());

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

    fn from_store(root: PathBuf, store: AuditStore, keyring: Arc<dyn KeyringBackend>) -> Self {
        let shared = Arc::new(Mutex::new(store));
        let sink: Arc<Mutex<dyn AuditSink>> =
            Arc::new(Mutex::new(SqliteAuditSink::from_shared(Arc::clone(&shared))));
        Self {
            audit: shared,
            sink,
            vm: Mutex::new(None),
            llm_config: Mutex::new(None),
            workspace_root: root,
            compiler: agent::CompilerConfig::from_env(),
            llm_override: Mutex::new(None),
            keyring,
            persisted: Mutex::new(false),
        }
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

        let existing = self.llm_config.lock().ok().and_then(|g| g.as_ref().cloned());
        let input = match existing {
            Some(current) if current.provider_id == provider_id => LlmConfigInput {
                api_key: key,
                ..current
            },
            _ => {
                let preset = find_preset(provider_id)
                    .ok_or_else(|| "unknown_provider".to_string())?;
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
        if self.llm_override.lock().map(|g| g.is_some()).unwrap_or(false) {
            return LlmReadiness {
                ready: true,
                reason: None,
                suggestion: None,
            };
        }
        let no_config = || LlmReadiness {
            ready: false,
            reason: Some("no_config".into()),
            suggestion: Some(
                "请在设置中选择服务商并填写 API Key，或使用本地模型".to_string(),
            ),
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

    /// The accumulated serial text observed so far (MVP: derived from the
    /// agent's `read_serial` tool results in the audit log).
    pub fn serial_buffer(&self) -> String {
        match self.audit.lock() {
            Ok(store) => store.all().map(|e| serial_full_text(&e)).unwrap_or_default(),
            Err(_) => String::new(),
        }
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
        let llm = self.build_llm()?;
        let policy = WorkspacePolicy::new(self.workspace_root.clone());
        let system_prompt = format!("{CONSTITUTION}\n\n{}", agent::prompt::OPERATING_RULES);
        let mut agent = AgentLoop::new(
            llm,
            self.agent_config(),
            policy,
            Arc::clone(&self.sink),
            system_prompt,
        )?;

        // Bridge: poll the audit log, emit derived host events + serial chunks.
        let stop = Arc::new(AtomicBool::new(false));
        let stop_bridge = Arc::clone(&stop);
        let audit = Arc::clone(&self.audit);
        let bridge_emitter = Arc::clone(&emitter);
        let handle = std::thread::spawn(move || {
            let mut bridge = AuditBridge::new(audit, bridge_emitter);
            while !stop_bridge.load(Ordering::Relaxed) {
                bridge.tick();
                std::thread::sleep(POLL_INTERVAL);
            }
            bridge.tick(); // final flush
        });

        let outcome = agent.run(user_input);

        stop.store(true, Ordering::Relaxed);
        let _ = handle.join();

        let outcome = outcome?;
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
}

/// Turns new audit events into frontend events, and tracks serial increments.
struct AuditBridge {
    audit: Arc<Mutex<AuditStore>>,
    emitter: Arc<dyn EventSink>,
    last_id: i64,
    serial: SerialDiff,
}

impl AuditBridge {
    fn new(audit: Arc<Mutex<AuditStore>>, emitter: Arc<dyn EventSink>) -> Self {
        Self {
            audit,
            emitter,
            last_id: 0,
            serial: SerialDiff::new(),
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
                "vm.start" => self
                    .emitter
                    .emit(EV_VM_STATE, serde_json::json!({ "state": "running" })),
                "vm.stop" => self
                    .emitter
                    .emit(EV_VM_STATE, serde_json::json!({ "state": "stopped" })),
                "vm.snapshot.save" => self.emitter.emit(
                    EV_VM_STATE,
                    serde_json::json!({
                        "state": "snapshot",
                        "name": e.event.detail.get("name"),
                    }),
                ),
                _ => {}
            }
            new_max = new_max.max(e.id);
        }
        self.last_id = new_max;

        let full = serial_full_text(&events);
        if let Some(chunk) = self.serial.next_chunk(&full) {
            self.emitter
                .emit(EV_SERIAL_CHUNK, serde_json::json!({ "chunk": chunk }));
        }
    }
}

/// Emits only the part of `full` that has not been emitted before.
pub struct SerialDiff {
    seen: usize,
}

impl SerialDiff {
    pub fn new() -> Self {
        Self { seen: 0 }
    }

    /// Returns the new tail, or `None` when there is nothing new.
    pub fn next_chunk(&mut self, full: &str) -> Option<String> {
        if full.len() <= self.seen {
            return None;
        }
        // `full.len()` is always a char boundary, and `seen` was set to a
        // previous `len()`, so this slice is always valid UTF-8.
        let chunk = full[self.seen..].to_string();
        self.seen = full.len();
        Some(chunk)
    }
}

impl Default for SerialDiff {
    fn default() -> Self {
        Self::new()
    }
}

/// MVP serial source: concatenate the agent's `read_serial` tool results, in
/// order, from the audit log.
pub fn serial_full_text(events: &[StoredEvent]) -> String {
    let mut names: HashMap<String, String> = HashMap::new();
    for e in events {
        if e.event.action == "agent.tool.call" {
            if let (Some(id), Some(name)) = (
                e.event.detail.get("id").and_then(|v| v.as_str()),
                e.event.detail.get("name").and_then(|v| v.as_str()),
            ) {
                names.insert(id.to_string(), name.to_string());
            }
        }
    }

    let mut out = String::new();
    for e in events {
        if e.event.action != "agent.tool.result" {
            continue;
        }
        let call_id = e.event.detail.get("call_id").and_then(|v| v.as_str());
        let is_read = call_id
            .and_then(|id| names.get(id))
            .map(|n| n == "read_serial")
            .unwrap_or(false);
        if is_read {
            if let Some(text) = e.event.detail.get("result").and_then(|v| v.as_str()) {
                out.push_str(text);
            }
        }
    }
    out
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
