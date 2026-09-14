//! Application state shared by all Tauri commands.

use crate::error::HostError;
use crate::events::{EventSink, EV_AGENT_FINAL, EV_AGENT_ITERATION, EV_AGENT_TOOL_CALL,
                    EV_AGENT_TOOL_RESULT, EV_SERIAL_CHUNK, EV_VM_STATE};
use agent::llm::{DeepSeekClient, LlmClient};
use agent::message::{ChatRequest, ChatResponse};
use agent::policy::WorkspacePolicy;
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
    pub api_key: String,
    pub base_url: String,
    pub model: String,
}

impl std::fmt::Debug for LlmConfigInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LlmConfigInput")
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
    pub base_url: String,
    pub model: String,
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
}

impl AppState {
    /// Build state backed by an on-disk audit DB under `<workspace>/.riscdom`.
    pub fn new(workspace_root: impl Into<PathBuf>) -> Result<Self, HostError> {
        let root = workspace_root.into();
        std::fs::create_dir_all(&root)?;
        let db_dir = root.join(".riscdom");
        std::fs::create_dir_all(&db_dir)?;
        let store = AuditStore::open(&db_dir.join("audit.db"))?;
        Ok(Self::from_store(root, store))
    }

    /// Build state with a private in-memory audit DB (tests).
    pub fn in_memory(workspace_root: impl Into<PathBuf>) -> Result<Self, HostError> {
        let root = workspace_root.into();
        std::fs::create_dir_all(&root)?;
        let store = AuditStore::in_memory()?;
        Ok(Self::from_store(root, store))
    }

    fn from_store(root: PathBuf, store: AuditStore) -> Self {
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
        }
    }

    // ----- LLM config -------------------------------------------------------

    /// Store LLM config in memory (replaces any previous value).
    pub fn set_llm_config(&self, input: LlmConfigInput) {
        if let Ok(mut g) = self.llm_config.lock() {
            *g = Some(input);
        }
    }

    /// Forget LLM config.
    pub fn clear_llm_config(&self) {
        if let Ok(mut g) = self.llm_config.lock() {
            *g = None;
        }
    }

    /// Status for the UI (never returns the key).
    pub fn llm_config_status(&self) -> LlmConfigStatus {
        match self.llm_config.lock() {
            Ok(g) => match g.as_ref() {
                Some(c) => LlmConfigStatus {
                    configured: true,
                    base_url: c.base_url.clone(),
                    model: c.model.clone(),
                },
                None => LlmConfigStatus {
                    configured: false,
                    base_url: String::new(),
                    model: String::new(),
                },
            },
            Err(_) => LlmConfigStatus {
                configured: false,
                base_url: String::new(),
                model: String::new(),
            },
        }
    }

    fn agent_config(&self) -> AgentConfig {
        let (api_key, base_url, model) = match self.llm_config.lock() {
            Ok(g) => match g.as_ref() {
                Some(c) => (c.api_key.clone(), c.base_url.clone(), c.model.clone()),
                None => (
                    String::new(),
                    "https://api.deepseek.com".to_string(),
                    "deepseek-chat".to_string(),
                ),
            },
            Err(_) => (
                String::new(),
                "https://api.deepseek.com".to_string(),
                "deepseek-chat".to_string(),
            ),
        };
        AgentConfig {
            api_key,
            base_url,
            model,
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
