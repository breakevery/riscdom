//! `riscdom-sdk` — a typed Rust client for the RiscDom control plane (v1.0 M7c).
//!
//! [docs/sdk.md](../../docs/sdk.md) freezes what this is: **a thin, typed layer over the surface the
//! other documents already define**, which adds no semantics of its own. The HTTP surface is
//! [control-plane-api.md](../../docs/control-plane-api.md) §5, the error model its §4, and
//! authentication its §3.
//!
//! **This batch (BA) covers the query half**: the 37 `GET` endpoints of §5.1, the bearer token, and
//! the `{code, message, retryable, cause}` error as a typed error. The control endpoints (`POST`, §5.2)
//! and the event stream are the next batch (BB); the TypeScript SDK is BC.
//!
//! **The crate links nothing of this workspace's runtime.** It talks HTTP over `reqwest`'s blocking
//! client and never depends on `host-core`, `server` or `net`, so a client on another machine — the
//! SDK's normal case — needs nothing else. No async runtime is imposed ([sdk.md](../../docs/sdk.md) §3).
//!
//! **The endpoint table cannot drift from the server.** [`QUERY_ENDPOINTS`] is a committed table, and a
//! test parses the marked blocks of
//! [`docs/tool-schema-control-plane.md`](../../docs/tool-schema-control-plane.md) and asserts the two are
//! equal; those marked blocks are already asserted to be exactly the server's own `ROUTES`, so the chain
//! is **SDK ⇄ tool schema ⇄ server**, with no dependency between them and no second list to maintain.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// One endpoint of the control plane, as the documents name it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Endpoint {
    /// The tool name the tool-schema document gives it (`audit_status`, `runs`, …).
    pub tool: &'static str,
    /// `GET` or `POST`.
    pub method: &'static str,
    /// The path, e.g. `/v0/audit/status`.
    pub path: &'static str,
    /// The capability a caller must hold, e.g. `audit.read`.
    pub capability: &'static str,
}

/// The query endpoints (`GET`), exactly as `docs/tool-schema-control-plane.md`'s `queries` marked block
/// lists them — which is exactly the server's own `ROUTES` filtered to `GET`
/// ([control-plane-api.md](../../docs/control-plane-api.md) §5.1).
pub const QUERY_ENDPOINTS: &[Endpoint] = &[
    endpoint("audit_status", "/v0/audit/status", "audit.read"),
    endpoint("audit_events", "/v0/audit/events", "audit.read"),
    endpoint("runs", "/v0/runs", "runs.read"),
    endpoint("runs_diff", "/v0/runs/diff", "runs.read"),
    endpoint(
        "llm_provider_presets",
        "/v0/llm/provider-presets",
        "llm.read",
    ),
    endpoint("llm_config", "/v0/llm/config", "llm.read"),
    endpoint("llm_readiness", "/v0/llm/readiness", "llm.read"),
    endpoint("llm_local_probe", "/v0/llm/local-probe", "llm.read"),
    endpoint("llm_stored_key", "/v0/llm/stored-key", "llm.read"),
    endpoint("sessions", "/v0/sessions", "session.read"),
    endpoint("sessions_current", "/v0/sessions/current", "session.read"),
    endpoint("snapshots", "/v0/snapshots", "snapshot.read"),
    endpoint("vm_running", "/v0/vm/running", "vm.read"),
    endpoint("vm_status", "/v0/vm/status", "vm.read"),
    endpoint("toolchain", "/v0/toolchain", "toolchain.read"),
    endpoint(
        "toolchain_download",
        "/v0/toolchain/download",
        "toolchain.read",
    ),
    endpoint("qemu", "/v0/qemu", "qemu.read"),
    endpoint("qemu_status", "/v0/qemu/status", "qemu.read"),
    endpoint("qemu_download", "/v0/qemu/download", "qemu.read"),
    endpoint("preflight", "/v0/preflight", "preflight.read"),
    endpoint("settings_theme", "/v0/settings/theme", "settings.read"),
    endpoint(
        "settings_language",
        "/v0/settings/language",
        "settings.read",
    ),
    endpoint("workspace_root", "/v0/workspace/root", "workspace.read"),
    endpoint("workspace_files", "/v0/workspace/files", "workspace.read"),
    endpoint("workspace_file", "/v0/workspace/file", "workspace.read"),
    endpoint("serial", "/v0/serial", "serial.read"),
    endpoint("sandboxes", "/v0/sandboxes", "sandbox.read"),
    endpoint("sandboxes_current", "/v0/sandboxes/current", "sandbox.read"),
    endpoint(
        "sandboxes_candidates",
        "/v0/sandboxes/candidates",
        "sandbox.read",
    ),
    endpoint(
        "sandboxes_requests",
        "/v0/sandboxes/requests",
        "sandbox.read",
    ),
    endpoint("resources", "/v0/resources", "vm.read"),
    endpoint("executors", "/v0/executors", "agent.run"),
    endpoint("capabilities", "/v0/capabilities", "status.read"),
    endpoint("identity", "/v0/identity", "status.read"),
    endpoint("peers", "/v0/peers", "status.read"),
    endpoint("rooms", "/v0/rooms", "status.read"),
    endpoint("connection", "/v0/connection", "status.read"),
];

/// A `GET` row, so the table above reads as a table.
const fn endpoint(tool: &'static str, path: &'static str, capability: &'static str) -> Endpoint {
    Endpoint {
        tool,
        method: "GET",
        path,
        capability,
    }
}

/// The query endpoints, as a function, for a caller that would rather not name the constant.
pub fn query_endpoints() -> &'static [Endpoint] {
    QUERY_ENDPOINTS
}

/// The **control** endpoints (`POST`), exactly as `docs/tool-schema-control-plane.md`'s `controls`
/// marked block lists them — which is exactly the server's own `ROUTES` filtered to `POST`
/// ([control-plane-api.md](../../docs/control-plane-api.md) §5.2).
pub const CONTROL_ENDPOINTS: &[Endpoint] = &[
    control("agent_run", "/v0/agent/run", "agent.run"),
    control("tasks", "/v0/tasks", "agent.run"),
    control("runs_export", "/v0/runs/export", "audit.export"),
    control(
        "runs_abandon_stale",
        "/v0/runs/abandon-stale",
        "runs.control",
    ),
    control("vm_start", "/v0/vm/start", "vm.control"),
    control("vm_stop", "/v0/vm/stop", "vm.control"),
    control("snapshots_save", "/v0/snapshots/save", "snapshot.write"),
    control("snapshots_resume", "/v0/snapshots/resume", "snapshot.write"),
    control("snapshots_delete", "/v0/snapshots/delete", "snapshot.write"),
    control("sessions_create", "/v0/sessions/create", "session.write"),
    control("sessions_open", "/v0/sessions/open", "session.write"),
    control("sessions_rename", "/v0/sessions/rename", "session.write"),
    control("sessions_delete", "/v0/sessions/delete", "session.write"),
    control("sessions_clear", "/v0/sessions/clear", "session.write"),
    control(
        "toolchain_download_post",
        "/v0/toolchain/download",
        "toolchain.install",
    ),
    control(
        "toolchain_download_cancel",
        "/v0/toolchain/download/cancel",
        "toolchain.install",
    ),
    control("qemu_download_post", "/v0/qemu/download", "qemu.configure"),
    control(
        "qemu_download_cancel",
        "/v0/qemu/download/cancel",
        "qemu.configure",
    ),
    control(
        "toolchain_path",
        "/v0/toolchain/path",
        "toolchain.configure",
    ),
    control(
        "toolchain_path_clear",
        "/v0/toolchain/path/clear",
        "toolchain.configure",
    ),
    control("qemu_path", "/v0/qemu/path", "qemu.configure"),
    control("qemu_path_clear", "/v0/qemu/path/clear", "qemu.configure"),
    control("preflight_run", "/v0/preflight/run", "preflight.run"),
    control("preflight_ack", "/v0/preflight/ack", "preflight.run"),
    control("audit_alert", "/v0/audit/alert", "settings.write"),
    control("audit_export", "/v0/audit/export", "audit.export"),
    control(
        "settings_theme_post",
        "/v0/settings/theme",
        "settings.write",
    ),
    control(
        "settings_language_post",
        "/v0/settings/language",
        "settings.write",
    ),
    control("llm_config_post", "/v0/llm/config", "llm.configure"),
    control(
        "llm_stored_key_load",
        "/v0/llm/stored-key/load",
        "llm.configure",
    ),
    control("llm_config_clear", "/v0/llm/config/clear", "llm.configure"),
    control("serial_export", "/v0/serial/export", "serial.export"),
    control("sandboxes_switch", "/v0/sandboxes/switch", "sandbox.switch"),
    control(
        "sandboxes_requests_post",
        "/v0/sandboxes/requests",
        "agent.run",
    ),
    control(
        "workspace_import",
        "/v0/workspace/import",
        "workspace.write",
    ),
    control("workspace_export", "/v0/workspace/export", "workspace.read"),
];

/// A `POST` row, so the table above reads as a table.
const fn control(tool: &'static str, path: &'static str, capability: &'static str) -> Endpoint {
    Endpoint {
        tool,
        method: "POST",
        path,
        capability,
    }
}

/// The control endpoints, as a function.
pub fn control_endpoints() -> &'static [Endpoint] {
    CONTROL_ENDPOINTS
}

/// The query string of a request: the parameter names the documents fix, with their values.
pub type Query = Vec<(&'static str, String)>;

/// Build a [`Query`] from optional and required parameters, skipping the ones that are absent.
fn query(pairs: &[(&'static str, Option<String>)]) -> Query {
    pairs
        .iter()
        .filter_map(|(name, value)| value.as_ref().map(|value| (*name, value.clone())))
        .collect()
}

/// `/v0/audit/events` — `limit` is required; the rest narrow the window.
#[derive(Debug, Clone, Default, Serialize)]
pub struct AuditEvents {
    /// How many rows at most. Required by the endpoint.
    pub limit: u64,
    /// Only rows whose actor is this `agent_id`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actor: Option<String>,
    /// Only rows whose action starts with this.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action_prefix: Option<String>,
    /// Window start, epoch ms.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_ms: Option<i64>,
    /// Window end, epoch ms.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to_ms: Option<i64>,
    /// Rows after this id.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_id: Option<i64>,
    /// Rows up to this id.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to_id: Option<i64>,
    /// Rows before this id (the paging cursor).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub before_id: Option<i64>,
}

impl AuditEvents {
    /// The query pairs for this request.
    pub fn to_query(&self) -> Query {
        query(&[
            ("limit", Some(self.limit.to_string())),
            ("actor", self.actor.clone()),
            ("action_prefix", self.action_prefix.clone()),
            ("from_ms", self.from_ms.map(|v| v.to_string())),
            ("to_ms", self.to_ms.map(|v| v.to_string())),
            ("from_id", self.from_id.map(|v| v.to_string())),
            ("to_id", self.to_id.map(|v| v.to_string())),
            ("before_id", self.before_id.map(|v| v.to_string())),
        ])
    }
}

/// `/v0/runs` — `limit` defaults to 20 on the server.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Runs {
    /// How many runs at most; the server's default is 20.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u64>,
}

impl Runs {
    /// The query pairs for this request.
    pub fn to_query(&self) -> Query {
        query(&[("limit", self.limit.map(|v| v.to_string()))])
    }
}

/// `/v0/runs/diff` — two runs' configuration fingerprints, field by field.
#[derive(Debug, Clone, Serialize)]
pub struct RunsDiff {
    /// The first run.
    pub run_a: String,
    /// The second run.
    pub run_b: String,
}

impl RunsDiff {
    /// The query pairs for this request.
    pub fn to_query(&self) -> Query {
        query(&[
            ("run_a", Some(self.run_a.clone())),
            ("run_b", Some(self.run_b.clone())),
        ])
    }
}

/// `/v0/llm/config`.
#[derive(Debug, Clone, Default, Serialize)]
pub struct LlmConfig {
    /// Which executor; the node itself when absent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub executor: Option<String>,
}

impl LlmConfig {
    /// The query pairs for this request.
    pub fn to_query(&self) -> Query {
        query(&[("executor", self.executor.clone())])
    }
}

/// `/v0/llm/readiness`.
#[derive(Debug, Clone, Default, Serialize)]
pub struct LlmReadiness {
    /// Which executor; the node itself when absent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub executor: Option<String>,
}

impl LlmReadiness {
    /// The query pairs for this request.
    pub fn to_query(&self) -> Query {
        query(&[("executor", self.executor.clone())])
    }
}

/// `/v0/llm/stored-key`.
#[derive(Debug, Clone, Serialize)]
pub struct LlmStoredKey {
    /// The provider whose key is asked about.
    pub provider_id: String,
    /// Which executor; the node itself when absent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub executor: Option<String>,
}

impl LlmStoredKey {
    /// The query pairs for this request.
    pub fn to_query(&self) -> Query {
        query(&[
            ("provider_id", Some(self.provider_id.clone())),
            ("executor", self.executor.clone()),
        ])
    }
}

/// `/v0/sessions`.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Sessions {
    /// How many sessions at most.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u64>,
    /// Which executor's sessions.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub executor: Option<String>,
}

impl Sessions {
    /// The query pairs for this request.
    pub fn to_query(&self) -> Query {
        query(&[
            ("limit", self.limit.map(|v| v.to_string())),
            ("executor", self.executor.clone()),
        ])
    }
}

/// `/v0/sessions/current`.
#[derive(Debug, Clone, Default, Serialize)]
pub struct SessionsCurrent {
    /// Which executor; the node itself when absent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub executor: Option<String>,
}

impl SessionsCurrent {
    /// The query pairs for this request.
    pub fn to_query(&self) -> Query {
        query(&[("executor", self.executor.clone())])
    }
}

/// `/v0/workspace/file` — `path` is required and must stay inside the workspace.
#[derive(Debug, Clone, Serialize)]
pub struct WorkspaceFile {
    /// The path, relative to the workspace root.
    pub path: String,
}

impl WorkspaceFile {
    /// The query pairs for this request.
    pub fn to_query(&self) -> Query {
        query(&[("path", Some(self.path.clone()))])
    }
}

/// `/v0/sandboxes/requests`.
#[derive(Debug, Clone, Default, Serialize)]
pub struct SandboxesRequests {
    /// Only requests in this state.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}

impl SandboxesRequests {
    /// The query pairs for this request.
    pub fn to_query(&self) -> Query {
        query(&[("status", self.status.clone())])
    }
}

/// `/v0/agent/run` — run one agent turn on this node.
#[derive(Debug, Clone, Serialize)]
pub struct AgentRun {
    /// What the agent is asked to do.
    pub user_input: String,
    /// The sandbox definition this run wants (a declaration, not a switch).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sandbox: Option<String>,
    /// Which of this node's instances the run wants.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instance: Option<String>,
}

/// `/v0/tasks` — dispatch a task to a configured executor.
#[derive(Debug, Clone, Serialize)]
pub struct Tasks {
    /// The executor to route to.
    pub target: String,
    /// What the task is.
    pub input: String,
    /// The sandbox the task wants.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sandbox: Option<String>,
    /// Which of the executor's instances the task wants.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instance: Option<String>,
    /// A caller-chosen task id; the server mints one when absent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
}

/// `/v0/runs/export`.
#[derive(Debug, Clone, Serialize)]
pub struct RunsExport {
    /// The run to export.
    pub run_id: String,
    /// Where the server writes it (resolved against the workspace root).
    pub path: String,
}

/// A snapshot's name — `save`, `resume` and `delete` all take exactly this.
#[derive(Debug, Clone, Serialize)]
pub struct SnapshotName {
    /// The snapshot's name.
    pub name: String,
}

/// `/v0/sessions/create`.
#[derive(Debug, Clone, Default, Serialize)]
pub struct SessionsCreate {
    /// The new session's title.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Which executor's table the session belongs to.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub executor: Option<String>,
}

/// `/v0/sessions/open`.
#[derive(Debug, Clone, Serialize)]
pub struct SessionsOpen {
    /// The session to make current.
    pub session_id: String,
    /// Which executor's table.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub executor: Option<String>,
}

/// `/v0/sessions/rename`.
#[derive(Debug, Clone, Serialize)]
pub struct SessionsRename {
    /// The session to rename.
    pub session_id: String,
    /// The new title.
    pub title: String,
    /// Which executor's table.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub executor: Option<String>,
}

/// `/v0/sessions/delete`.
#[derive(Debug, Clone, Serialize)]
pub struct SessionsDelete {
    /// The session to drop.
    pub session_id: String,
    /// Which executor's table.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub executor: Option<String>,
}

/// `/v0/sessions/clear` — drop every session in an executor's table.
#[derive(Debug, Clone, Default, Serialize)]
pub struct SessionsClear {
    /// Which executor's table.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub executor: Option<String>,
}

/// `POST /v0/toolchain/download` — which toolchain to fetch; the server defaults to `c`.
#[derive(Debug, Clone, Default, Serialize)]
pub struct ToolchainDownload {
    /// `c`, `zig` or `rust`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub toolchain: Option<String>,
}

/// A single `path` field — `toolchain/path`, `qemu/path`, `audit/export` and `serial/export` all
/// take exactly this shape, so they share the type.
#[derive(Debug, Clone, Serialize)]
pub struct PathArgument {
    /// The path the endpoint wants (resolved against the workspace root, or an output file).
    pub path: String,
}

/// `/v0/audit/alert`.
#[derive(Debug, Clone, Serialize)]
pub struct AuditAlert {
    /// Whether the interface shouts when an audit write fails.
    pub enabled: bool,
}

/// `POST /v0/settings/theme`.
#[derive(Debug, Clone, Serialize)]
pub struct SettingsTheme {
    /// `light`, `dark` or `system`.
    pub theme: String,
}

/// `POST /v0/settings/language`.
#[derive(Debug, Clone, Serialize)]
pub struct SettingsLanguage {
    /// `system`, `en` or `zh`.
    pub language: String,
}

/// `POST /v0/llm/config` — set one executor's model configuration. The key is a credential, so the
/// caller supplies it here and the server decides where it lives (the OS keyring).
#[derive(Debug, Clone, Serialize)]
pub struct LlmConfigPost {
    /// The provider's API key.
    pub api_key: String,
    /// The endpoint to talk to.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    /// The model name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// The provider id.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<String>,
    /// Whether the server should remember the key in the keyring.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remember: Option<bool>,
    /// Which executor's configuration.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub executor: Option<String>,
}

/// `/v0/llm/stored-key/load`.
#[derive(Debug, Clone, Serialize)]
pub struct LlmStoredKeyLoad {
    /// The provider whose stored key to load.
    pub provider_id: String,
    /// Which executor's configuration.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub executor: Option<String>,
}

/// `/v0/llm/config/clear`.
#[derive(Debug, Clone, Default, Serialize)]
pub struct LlmConfigClear {
    /// Which executor's configuration.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub executor: Option<String>,
}

/// `/v0/sandboxes/switch`.
#[derive(Debug, Clone, Serialize)]
pub struct SandboxesSwitch {
    /// The definition to switch to.
    pub name: String,
}

/// `POST /v0/sandboxes/requests` — leave an ask on the queue.
#[derive(Debug, Clone, Serialize)]
pub struct SandboxesRequestsPost {
    /// `switch` or `assemble`.
    pub action: String,
    /// The definition the ask is about.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sandbox: Option<String>,
    /// Why the ask is being made.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// The control plane's error object ([control-plane-api.md](../../docs/control-plane-api.md) §4): the
/// four fields every non-2xx answer carries, as a type rather than a string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiError {
    /// The HTTP status the server answered with.
    pub status: u16,
    /// Stable, machine-readable: `bad_request`, `unauthorized`, `forbidden`, `not_found`,
    /// `method_not_allowed`, `conflict`, `not_implemented`, `unavailable`, `internal`.
    pub code: String,
    /// Human-readable, never a secret.
    pub message: String,
    /// Whether an identical retry can plausibly succeed.
    pub retryable: bool,
    /// The offending input field or subsystem, when there is one.
    pub cause: Option<String>,
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "{} (HTTP {}): {}",
            self.code, self.status, self.message
        )?;
        match &self.cause {
            Some(cause) => write!(formatter, " [cause: {cause}]"),
            None => Ok(()),
        }
    }
}

impl std::error::Error for ApiError {}

impl ApiError {
    /// Read the error object out of a body. A body that is not the documented shape still becomes an
    /// `ApiError` — one with `internal` — because a client that cannot say what went wrong is worse
    /// than one that says "the server said something unreadable".
    pub fn from_body(status: u16, body: &[u8]) -> Self {
        let parsed: Option<Value> = serde_json::from_slice(body).ok();
        let object = parsed.as_ref().and_then(Value::as_object);
        let text = |key: &str| {
            object
                .and_then(|map| map.get(key))
                .and_then(Value::as_str)
                .map(str::to_string)
        };
        ApiError {
            status,
            code: text("code").unwrap_or_else(|| "internal".to_string()),
            message: text("message").unwrap_or_else(|| {
                "the server answered with a body this client cannot read".to_string()
            }),
            retryable: object
                .and_then(|map| map.get("retryable"))
                .and_then(Value::as_bool)
                .unwrap_or(false),
            cause: text("cause"),
        }
    }
}

/// What a call can fail with: the transport, or the server's own answer.
#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    /// The request could not be sent or its answer could not be read.
    #[error("the request could not be completed: {0}")]
    Transport(String),
    /// The server answered with the error object of §4.
    #[error("{0}")]
    Api(#[from] ApiError),
}

impl ClientError {
    /// The error object, when the failure was the server's answer rather than the transport.
    pub fn api(&self) -> Option<&ApiError> {
        match self {
            ClientError::Api(error) => Some(error),
            ClientError::Transport(_) => None,
        }
    }
}

/// A client for one node's control plane.
///
/// The bearer token is the node's own (`<data-dir>/token`, [control-plane-api.md](../../docs/control-plane-api.md) §3),
/// and it lives here rather than in the caller's code so a request cannot forget to carry it.
pub struct Client {
    base_url: String,
    auth: String,
    agent_name: Option<String>,
    http: reqwest::blocking::Client,
}

impl Client {
    /// Point a client at a node. `base_url` is like `http://127.0.0.1:7821`; `auth` is the bearer
    /// value the node's token file holds.
    pub fn new(base_url: impl Into<String>, auth: impl Into<String>) -> Result<Self, ClientError> {
        let http = reqwest::blocking::Client::builder()
            .build()
            .map_err(|error| ClientError::Transport(error.to_string()))?;
        Ok(Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            auth: auth.into(),
            agent_name: None,
            http,
        })
    }

    /// Name the client as an agent ([control-plane-api.md](../../docs/control-plane-api.md) §3's
    /// `X-RiscDom-Agent`): every audit row the request writes then says which agent acted.
    pub fn with_agent_name(mut self, name: impl Into<String>) -> Self {
        self.agent_name = Some(name.into());
        self
    }

    /// The base URL this client talks to.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Build a request: the base URL, the path, the bearer, the optional agent name.
    fn request(
        &self,
        method: reqwest::Method,
        path: &str,
        query: &[(&str, String)],
    ) -> reqwest::blocking::RequestBuilder {
        let mut request = self
            .http
            .request(method, format!("{}{path}", self.base_url))
            .bearer_auth(&self.auth);
        if !query.is_empty() {
            request = request.query(query);
        }
        if let Some(name) = &self.agent_name {
            request = request.header("X-RiscDom-Agent", name);
        }
        request
    }

    /// Send a request and read the answer's body. A non-2xx answer becomes the typed error of §4;
    /// anything else comes back as bytes, because two endpoints answer with bytes rather than JSON
    /// (`/v0/workspace/export`, §5.2).
    fn send(
        &self,
        method: reqwest::Method,
        path: &str,
        query: &[(&str, String)],
        body: Option<(&[u8], &str)>,
    ) -> Result<Vec<u8>, ClientError> {
        let mut request = self.request(method, path, query);
        if let Some((bytes, content_type)) = body {
            request = request
                .header("Content-Type", content_type)
                .body(bytes.to_vec());
        }
        let response = request
            .send()
            .map_err(|error| ClientError::Transport(error.to_string()))?;
        let status = response.status().as_u16();
        let body = response
            .bytes()
            .map_err(|error| ClientError::Transport(error.to_string()))?;
        if !(200..300).contains(&status) {
            return Err(ClientError::Api(ApiError::from_body(status, &body)));
        }
        Ok(body.to_vec())
    }

    /// Read a body as JSON; an empty body is `null`.
    fn json(body: &[u8]) -> Result<Value, ClientError> {
        if body.is_empty() {
            return Ok(Value::Null);
        }
        serde_json::from_slice(body).map_err(|error| ClientError::Transport(error.to_string()))
    }

    /// `GET` a path with a query, and read the answer as JSON.
    ///
    /// Every typed method below is this call with the endpoint's own path and parameters, so a caller
    /// that needs an endpoint the SDK does not wrap can still make the call — the surface is the API's,
    /// not the SDK's.
    pub fn get(&self, path: &str, query: &[(&str, String)]) -> Result<Value, ClientError> {
        Self::json(&self.send(reqwest::Method::GET, path, query, None)?)
    }

    /// `GET` a path and read the answer as raw bytes.
    pub fn get_raw(&self, path: &str, query: &[(&str, String)]) -> Result<Vec<u8>, ClientError> {
        self.send(reqwest::Method::GET, path, query, None)
    }

    /// `POST` a JSON body and read a JSON answer.
    pub fn post(&self, path: &str, body: &Value) -> Result<Value, ClientError> {
        let bytes =
            serde_json::to_vec(body).map_err(|error| ClientError::Transport(error.to_string()))?;
        Self::json(&self.send(
            reqwest::Method::POST,
            path,
            &[],
            Some((&bytes, "application/json")),
        )?)
    }

    /// `POST` with no body and read a JSON answer — most control endpoints.
    pub fn post_empty(&self, path: &str) -> Result<Value, ClientError> {
        Self::json(&self.send(reqwest::Method::POST, path, &[], None)?)
    }

    /// `POST` a raw body (a workspace archive is not JSON, §5.2) and read the answer as bytes.
    pub fn post_raw(
        &self,
        path: &str,
        query: &[(&str, String)],
        body: &[u8],
        content_type: &str,
    ) -> Result<Vec<u8>, ClientError> {
        self.send(
            reqwest::Method::POST,
            path,
            query,
            Some((body, content_type)),
        )
    }

    /// Subscribe to the event stream (`GET /v0/events`,
    /// [control-plane-events.md](../../docs/control-plane-events.md) §1).
    ///
    /// The answer is a [`Subscription`], read frame by frame on **this thread** — the stream is read
    /// with `std::io::Read` over `reqwest`'s blocking response, so **no async runtime is imposed**
    /// (§3 of [sdk.md](../../docs/sdk.md)) and no extra feature or package is needed.
    ///
    /// `last_event_id` is the replay cursor: pass what [`Subscription::last_id`] gave you to resume a
    /// dropped stream (`Last-Event-ID`). If the server cannot replay that far back, the first frame is
    /// a **`gap`** — an instruction to re-sync from a query, never an error
    /// ([control-plane-events.md](../../docs/control-plane-events.md) §2).
    pub fn subscribe(
        &self,
        filters: &Filters,
        last_event_id: Option<&str>,
    ) -> Result<Subscription, ClientError> {
        let mut request = self
            .request(reqwest::Method::GET, "/v0/events", &filters.to_query())
            .header("Accept", "text/event-stream");
        if let Some(last) = last_event_id {
            request = request.header("Last-Event-ID", last);
        }
        let response = request
            .send()
            .map_err(|error| ClientError::Transport(error.to_string()))?;
        let status = response.status().as_u16();
        if !(200..300).contains(&status) {
            let body = response
                .bytes()
                .map_err(|error| ClientError::Transport(error.to_string()))?;
            return Err(ClientError::Api(ApiError::from_body(status, &body)));
        }
        Ok(Subscription {
            reader: std::io::BufReader::new(response),
            last_id: last_event_id.map(str::to_string),
        })
    }

    /// `GET /v0/audit/status` — capability `audit.read`.
    pub fn audit_status(&self) -> Result<Value, ClientError> {
        self.get("/v0/audit/status", &[])
    }

    /// `GET /v0/audit/events` — capability `audit.read`.
    pub fn audit_events(&self, params: &AuditEvents) -> Result<Value, ClientError> {
        self.get("/v0/audit/events", &params.to_query())
    }

    /// `GET /v0/runs` — capability `runs.read`.
    pub fn runs(&self, params: &Runs) -> Result<Value, ClientError> {
        self.get("/v0/runs", &params.to_query())
    }

    /// `GET /v0/runs/diff` — capability `runs.read`.
    pub fn runs_diff(&self, params: &RunsDiff) -> Result<Value, ClientError> {
        self.get("/v0/runs/diff", &params.to_query())
    }

    /// `GET /v0/llm/provider-presets` — capability `llm.read`.
    pub fn llm_provider_presets(&self) -> Result<Value, ClientError> {
        self.get("/v0/llm/provider-presets", &[])
    }

    /// `GET /v0/llm/config` — capability `llm.read`.
    pub fn llm_config(&self, params: &LlmConfig) -> Result<Value, ClientError> {
        self.get("/v0/llm/config", &params.to_query())
    }

    /// `GET /v0/llm/readiness` — capability `llm.read`.
    pub fn llm_readiness(&self, params: &LlmReadiness) -> Result<Value, ClientError> {
        self.get("/v0/llm/readiness", &params.to_query())
    }

    /// `GET /v0/llm/local-probe` — capability `llm.read`.
    pub fn llm_local_probe(&self) -> Result<Value, ClientError> {
        self.get("/v0/llm/local-probe", &[])
    }

    /// `GET /v0/llm/stored-key` — capability `llm.read`.
    pub fn llm_stored_key(&self, params: &LlmStoredKey) -> Result<Value, ClientError> {
        self.get("/v0/llm/stored-key", &params.to_query())
    }

    /// `GET /v0/sessions` — capability `session.read`.
    pub fn sessions(&self, params: &Sessions) -> Result<Value, ClientError> {
        self.get("/v0/sessions", &params.to_query())
    }

    /// `GET /v0/sessions/current` — capability `session.read`.
    pub fn sessions_current(&self, params: &SessionsCurrent) -> Result<Value, ClientError> {
        self.get("/v0/sessions/current", &params.to_query())
    }

    /// `GET /v0/snapshots` — capability `snapshot.read`.
    pub fn snapshots(&self) -> Result<Value, ClientError> {
        self.get("/v0/snapshots", &[])
    }

    /// `GET /v0/vm/running` — capability `vm.read`.
    pub fn vm_running(&self) -> Result<Value, ClientError> {
        self.get("/v0/vm/running", &[])
    }

    /// `GET /v0/vm/status` — capability `vm.read`.
    pub fn vm_status(&self) -> Result<Value, ClientError> {
        self.get("/v0/vm/status", &[])
    }

    /// `GET /v0/toolchain` — capability `toolchain.read`.
    pub fn toolchain(&self) -> Result<Value, ClientError> {
        self.get("/v0/toolchain", &[])
    }

    /// `GET /v0/toolchain/download` — capability `toolchain.read`.
    pub fn toolchain_download(&self) -> Result<Value, ClientError> {
        self.get("/v0/toolchain/download", &[])
    }

    /// `GET /v0/qemu` — capability `qemu.read`.
    pub fn qemu(&self) -> Result<Value, ClientError> {
        self.get("/v0/qemu", &[])
    }

    /// `GET /v0/qemu/status` — capability `qemu.read`.
    pub fn qemu_status(&self) -> Result<Value, ClientError> {
        self.get("/v0/qemu/status", &[])
    }

    /// `GET /v0/qemu/download` — capability `qemu.read`.
    pub fn qemu_download(&self) -> Result<Value, ClientError> {
        self.get("/v0/qemu/download", &[])
    }

    /// `GET /v0/preflight` — capability `preflight.read`.
    pub fn preflight(&self) -> Result<Value, ClientError> {
        self.get("/v0/preflight", &[])
    }

    /// `GET /v0/settings/theme` — capability `settings.read`.
    pub fn settings_theme(&self) -> Result<Value, ClientError> {
        self.get("/v0/settings/theme", &[])
    }

    /// `GET /v0/settings/language` — capability `settings.read`.
    pub fn settings_language(&self) -> Result<Value, ClientError> {
        self.get("/v0/settings/language", &[])
    }

    /// `GET /v0/workspace/root` — capability `workspace.read`.
    pub fn workspace_root(&self) -> Result<Value, ClientError> {
        self.get("/v0/workspace/root", &[])
    }

    /// `GET /v0/workspace/files` — capability `workspace.read`.
    pub fn workspace_files(&self) -> Result<Value, ClientError> {
        self.get("/v0/workspace/files", &[])
    }

    /// `GET /v0/workspace/file` — capability `workspace.read`.
    pub fn workspace_file(&self, params: &WorkspaceFile) -> Result<Value, ClientError> {
        self.get("/v0/workspace/file", &params.to_query())
    }

    /// `GET /v0/serial` — capability `serial.read`.
    pub fn serial(&self) -> Result<Value, ClientError> {
        self.get("/v0/serial", &[])
    }

    /// `GET /v0/sandboxes` — capability `sandbox.read`.
    pub fn sandboxes(&self) -> Result<Value, ClientError> {
        self.get("/v0/sandboxes", &[])
    }

    /// `GET /v0/sandboxes/current` — capability `sandbox.read`.
    pub fn sandboxes_current(&self) -> Result<Value, ClientError> {
        self.get("/v0/sandboxes/current", &[])
    }

    /// `GET /v0/sandboxes/candidates` — capability `sandbox.read`.
    pub fn sandboxes_candidates(&self) -> Result<Value, ClientError> {
        self.get("/v0/sandboxes/candidates", &[])
    }

    /// `GET /v0/sandboxes/requests` — capability `sandbox.read`.
    pub fn sandboxes_requests(&self, params: &SandboxesRequests) -> Result<Value, ClientError> {
        self.get("/v0/sandboxes/requests", &params.to_query())
    }

    /// `GET /v0/resources` — reserved: answers `501` ([control-plane-api.md](../../docs/control-plane-api.md) §6).
    pub fn resources(&self) -> Result<Value, ClientError> {
        self.get("/v0/resources", &[])
    }

    /// `GET /v0/executors` — capability `agent.run`.
    pub fn executors(&self) -> Result<Value, ClientError> {
        self.get("/v0/executors", &[])
    }

    /// `GET /v0/capabilities` — capability `status.read`.
    pub fn capabilities(&self) -> Result<Value, ClientError> {
        self.get("/v0/capabilities", &[])
    }

    /// `GET /v0/identity` — capability `status.read`.
    pub fn identity(&self) -> Result<Value, ClientError> {
        self.get("/v0/identity", &[])
    }

    /// `GET /v0/peers` — capability `status.read`.
    pub fn peers(&self) -> Result<Value, ClientError> {
        self.get("/v0/peers", &[])
    }

    /// `GET /v0/rooms` — capability `status.read`.
    pub fn rooms(&self) -> Result<Value, ClientError> {
        self.get("/v0/rooms", &[])
    }

    /// `GET /v0/connection` — capability `status.read`.
    pub fn connection(&self) -> Result<Value, ClientError> {
        self.get("/v0/connection", &[])
    }

    /// `POST` a typed body and read a JSON answer — what the control methods below use.
    fn post_body(&self, path: &str, body: &impl Serialize) -> Result<Value, ClientError> {
        let value = serde_json::to_value(body)
            .map_err(|error| ClientError::Transport(error.to_string()))?;
        self.post(path, &value)
    }

    /// `POST /v0/agent/run` — capability `agent.run`.
    pub fn agent_run(&self, params: &AgentRun) -> Result<Value, ClientError> {
        self.post_body("/v0/agent/run", params)
    }

    /// `POST /v0/tasks` — capability `agent.run`.
    pub fn tasks(&self, params: &Tasks) -> Result<Value, ClientError> {
        self.post_body("/v0/tasks", params)
    }

    /// `POST /v0/runs/export` — capability `audit.export`.
    pub fn runs_export(&self, params: &RunsExport) -> Result<Value, ClientError> {
        self.post_body("/v0/runs/export", params)
    }

    /// `POST /v0/runs/abandon-stale` — capability `runs.control`.
    pub fn runs_abandon_stale(&self) -> Result<Value, ClientError> {
        self.post_empty("/v0/runs/abandon-stale")
    }

    /// `POST /v0/vm/start` — capability `vm.control`; reserved, answers `501`.
    pub fn vm_start(&self) -> Result<Value, ClientError> {
        self.post_empty("/v0/vm/start")
    }

    /// `POST /v0/vm/stop` — capability `vm.control`.
    pub fn vm_stop(&self) -> Result<Value, ClientError> {
        self.post_empty("/v0/vm/stop")
    }

    /// `POST /v0/snapshots/save` — capability `snapshot.write`.
    pub fn snapshots_save(&self, params: &SnapshotName) -> Result<Value, ClientError> {
        self.post_body("/v0/snapshots/save", params)
    }

    /// `POST /v0/snapshots/resume` — capability `snapshot.write`.
    pub fn snapshots_resume(&self, params: &SnapshotName) -> Result<Value, ClientError> {
        self.post_body("/v0/snapshots/resume", params)
    }

    /// `POST /v0/snapshots/delete` — capability `snapshot.write`.
    pub fn snapshots_delete(&self, params: &SnapshotName) -> Result<Value, ClientError> {
        self.post_body("/v0/snapshots/delete", params)
    }

    /// `POST /v0/sessions/create` — capability `session.write`.
    pub fn sessions_create(&self, params: &SessionsCreate) -> Result<Value, ClientError> {
        self.post_body("/v0/sessions/create", params)
    }

    /// `POST /v0/sessions/open` — capability `session.write`.
    pub fn sessions_open(&self, params: &SessionsOpen) -> Result<Value, ClientError> {
        self.post_body("/v0/sessions/open", params)
    }

    /// `POST /v0/sessions/rename` — capability `session.write`.
    pub fn sessions_rename(&self, params: &SessionsRename) -> Result<Value, ClientError> {
        self.post_body("/v0/sessions/rename", params)
    }

    /// `POST /v0/sessions/delete` — capability `session.write`.
    pub fn sessions_delete(&self, params: &SessionsDelete) -> Result<Value, ClientError> {
        self.post_body("/v0/sessions/delete", params)
    }

    /// `POST /v0/sessions/clear` — capability `session.write`.
    pub fn sessions_clear(&self, params: &SessionsClear) -> Result<Value, ClientError> {
        self.post_body("/v0/sessions/clear", params)
    }

    /// `POST /v0/toolchain/download` — capability `toolchain.install`.
    pub fn toolchain_download_post(
        &self,
        params: &ToolchainDownload,
    ) -> Result<Value, ClientError> {
        self.post_body("/v0/toolchain/download", params)
    }

    /// `POST /v0/toolchain/download/cancel` — capability `toolchain.install`.
    pub fn toolchain_download_cancel(&self) -> Result<Value, ClientError> {
        self.post_empty("/v0/toolchain/download/cancel")
    }

    /// `POST /v0/qemu/download` — capability `qemu.configure`.
    pub fn qemu_download_post(&self) -> Result<Value, ClientError> {
        self.post_empty("/v0/qemu/download")
    }

    /// `POST /v0/qemu/download/cancel` — capability `qemu.configure`.
    pub fn qemu_download_cancel(&self) -> Result<Value, ClientError> {
        self.post_empty("/v0/qemu/download/cancel")
    }

    /// `POST /v0/toolchain/path` — capability `toolchain.configure`.
    pub fn toolchain_path(&self, params: &PathArgument) -> Result<Value, ClientError> {
        self.post_body("/v0/toolchain/path", params)
    }

    /// `POST /v0/toolchain/path/clear` — capability `toolchain.configure`.
    pub fn toolchain_path_clear(&self) -> Result<Value, ClientError> {
        self.post_empty("/v0/toolchain/path/clear")
    }

    /// `POST /v0/qemu/path` — capability `qemu.configure`.
    pub fn qemu_path(&self, params: &PathArgument) -> Result<Value, ClientError> {
        self.post_body("/v0/qemu/path", params)
    }

    /// `POST /v0/qemu/path/clear` — capability `qemu.configure`.
    pub fn qemu_path_clear(&self) -> Result<Value, ClientError> {
        self.post_empty("/v0/qemu/path/clear")
    }

    /// `POST /v0/preflight/run` — capability `preflight.run`.
    pub fn preflight_run(&self) -> Result<Value, ClientError> {
        self.post_empty("/v0/preflight/run")
    }

    /// `POST /v0/preflight/ack` — capability `preflight.run`.
    pub fn preflight_ack(&self) -> Result<Value, ClientError> {
        self.post_empty("/v0/preflight/ack")
    }

    /// `POST /v0/audit/alert` — capability `settings.write`.
    pub fn audit_alert(&self, params: &AuditAlert) -> Result<Value, ClientError> {
        self.post_body("/v0/audit/alert", params)
    }

    /// `POST /v0/audit/export` — capability `audit.export`.
    pub fn audit_export(&self, params: &PathArgument) -> Result<Value, ClientError> {
        self.post_body("/v0/audit/export", params)
    }

    /// `POST /v0/settings/theme` — capability `settings.write`.
    pub fn settings_theme_post(&self, params: &SettingsTheme) -> Result<Value, ClientError> {
        self.post_body("/v0/settings/theme", params)
    }

    /// `POST /v0/settings/language` — capability `settings.write`.
    pub fn settings_language_post(&self, params: &SettingsLanguage) -> Result<Value, ClientError> {
        self.post_body("/v0/settings/language", params)
    }

    /// `POST /v0/llm/config` — capability `llm.configure`.
    pub fn llm_config_post(&self, params: &LlmConfigPost) -> Result<Value, ClientError> {
        self.post_body("/v0/llm/config", params)
    }

    /// `POST /v0/llm/stored-key/load` — capability `llm.configure`.
    pub fn llm_stored_key_load(&self, params: &LlmStoredKeyLoad) -> Result<Value, ClientError> {
        self.post_body("/v0/llm/stored-key/load", params)
    }

    /// `POST /v0/llm/config/clear` — capability `llm.configure`.
    pub fn llm_config_clear(&self, params: &LlmConfigClear) -> Result<Value, ClientError> {
        self.post_body("/v0/llm/config/clear", params)
    }

    /// `POST /v0/serial/export` — capability `serial.export`.
    pub fn serial_export(&self, params: &PathArgument) -> Result<Value, ClientError> {
        self.post_body("/v0/serial/export", params)
    }

    /// `POST /v0/sandboxes/switch` — capability `sandbox.switch`.
    pub fn sandboxes_switch(&self, params: &SandboxesSwitch) -> Result<Value, ClientError> {
        self.post_body("/v0/sandboxes/switch", params)
    }

    /// `POST /v0/sandboxes/requests` — capability `agent.run`.
    pub fn sandboxes_requests_post(
        &self,
        params: &SandboxesRequestsPost,
    ) -> Result<Value, ClientError> {
        self.post_body("/v0/sandboxes/requests", params)
    }

    /// `POST /v0/workspace/import` — capability `workspace.write`. The archive **is** the body (§5.2),
    /// so this one takes bytes rather than a struct; `force` replaces files that are already there.
    pub fn workspace_import(&self, archive: &[u8], force: bool) -> Result<Value, ClientError> {
        let query: Vec<(&str, String)> = if force {
            vec![("force", "true".to_string())]
        } else {
            Vec::new()
        };
        let bytes = self.post_raw(
            "/v0/workspace/import",
            &query,
            archive,
            "application/octet-stream",
        )?;
        Self::json(&bytes)
    }

    /// `POST /v0/workspace/export` — capability `workspace.read`. **Bytes out, not JSON** (§5.2), so
    /// this is the one control method that does not answer with a `Value`.
    pub fn workspace_export(&self) -> Result<Vec<u8>, ClientError> {
        self.send(reqwest::Method::POST, "/v0/workspace/export", &[], None)
    }
}

/// The stream's filters ([control-plane-events.md](../../docs/control-plane-events.md) §4).
///
/// Filtering is an **optimisation, never a correctness guarantee**: the document requires a client to
/// tolerate an event it did not ask for, and `hello` and `gap` are never filtered.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Filters {
    /// The event names to receive; empty means all.
    pub events: Vec<String>,
    /// One agent's events.
    pub agent_id: Option<String>,
    /// One task's events.
    pub task_id: Option<String>,
}

impl Filters {
    fn to_query(&self) -> Vec<(&'static str, String)> {
        let mut query: Vec<(&'static str, String)> = self
            .events
            .iter()
            .map(|event| ("event", event.clone()))
            .collect();
        if let Some(agent_id) = &self.agent_id {
            query.push(("agent_id", agent_id.clone()));
        }
        if let Some(task_id) = &self.task_id {
            query.push(("task_id", task_id.clone()));
        }
        query
    }
}

/// One frame as it arrived: the `id:` line (the replay cursor) and the `data:` payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    /// The `id:` line. Opaque — store it and send it back, do not parse it (§1).
    pub id: Option<String>,
    /// The `data:` payload, exactly as it arrived.
    pub data: String,
}

impl Frame {
    /// The frame's payload as an envelope, when it parses as one.
    pub fn envelope(&self) -> Option<Envelope> {
        serde_json::from_str(&self.data).ok()
    }

    /// The frame's kind, when the payload is an envelope.
    pub fn frame_kind(&self) -> Option<FrameKind> {
        self.envelope().map(|envelope| envelope.frame_kind())
    }
}

/// The envelope's `kind` ([control-plane-events.md](../../docs/control-plane-events.md) §2).
///
/// A new kind value does not bump the envelope's `version`, and clients must ignore frames they do not
/// recognise — so [`FrameKind::Unknown`] exists rather than a panic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrameKind {
    /// One of the twenty events; the envelope's `event` names it.
    Event,
    /// The stream opened; the payload describes its buffer and filters.
    Hello,
    /// The replay cursor was too old: **re-sync from a query**.
    Gap,
    /// A kind this build does not know.
    Unknown(String),
}

/// The unified envelope every frame carries ([control-plane-events.md](../../docs/control-plane-events.md) §2).
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Envelope {
    /// Envelope schema version: `1`.
    pub version: u32,
    /// `event`, `hello` or `gap`.
    pub kind: String,
    /// One of the twenty names, or absent for `hello` / `gap`.
    #[serde(default)]
    pub event: Option<String>,
    /// The agent that caused the event, `<device>-<pid>-<seq>`.
    pub agent_id: String,
    /// The dispatched task it belongs to; absent when it belongs to none.
    #[serde(default)]
    pub task_id: Option<String>,
    /// Epoch milliseconds.
    pub ts: i64,
    /// The event-specific body.
    #[serde(default)]
    pub payload: Value,
}

impl Envelope {
    /// The kind, as a value.
    pub fn frame_kind(&self) -> FrameKind {
        match self.kind.as_str() {
            "event" => FrameKind::Event,
            "hello" => FrameKind::Hello,
            "gap" => FrameKind::Gap,
            other => FrameKind::Unknown(other.to_string()),
        }
    }

    /// A `gap` frame's `payload.lost_after`: the oldest id the server still holds. **A client that
    /// sees a `gap` must re-sync from a query** — the stream cannot repair the hole, which is exactly
    /// why this is an instruction and not an error
    /// ([control-plane-events.md](../../docs/control-plane-events.md) §2).
    pub fn lost_after(&self) -> Option<&str> {
        self.payload.get("lost_after").and_then(Value::as_str)
    }
}

/// A live event stream, read frame by frame.
///
/// Blocking on purpose: the frames are read as the server writes them, with `std::io::Read` over
/// `reqwest`'s blocking response, so **no async runtime is imposed** ([sdk.md](../../docs/sdk.md) §3).
/// Comment lines (the 15-second `: keep-alive`, §1) are skipped, and several `data:` lines in one
/// frame are joined with newlines, which is what the frame format says a reader should do.
pub struct Subscription {
    reader: std::io::BufReader<reqwest::blocking::Response>,
    last_id: Option<String>,
}

impl Subscription {
    /// The next frame, or `None` at end of stream.
    pub fn next_frame(&mut self) -> Result<Option<Frame>, ClientError> {
        use std::io::BufRead;
        let mut id: Option<String> = None;
        let mut data: Vec<String> = Vec::new();
        let mut line = String::new();
        loop {
            line.clear();
            let read = self
                .reader
                .read_line(&mut line)
                .map_err(|error| ClientError::Transport(error.to_string()))?;
            if read == 0 {
                // The stream ended without the blank line that closes a frame.
                return Ok(None);
            }
            let trimmed = line.trim_end_matches(['\r', '\n']);
            if trimmed.is_empty() {
                if id.is_none() && data.is_empty() {
                    continue; // a blank line between frames, or after a keep-alive
                }
                let frame = Frame {
                    id: id.clone(),
                    data: data.join("\n"),
                };
                if let Some(value) = &frame.id {
                    self.last_id = Some(value.clone());
                }
                return Ok(Some(frame));
            }
            if trimmed.starts_with(':') {
                continue; // a comment (the heartbeat)
            }
            if let Some(rest) = trimmed.strip_prefix("id:") {
                id = Some(rest.trim_start().to_string());
                continue;
            }
            if let Some(rest) = trimmed.strip_prefix("data:") {
                data.push(rest.trim_start().to_string());
                continue;
            }
            // Any other field is ignored: the stream uses `id:` and `data:` only (§1).
        }
    }

    /// The last `id:` seen — or the one the subscription resumed from — which is the `Last-Event-ID`
    /// to send when reconnecting (§1).
    pub fn last_id(&self) -> Option<&str> {
        self.last_id.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    /// The tool-schema document, parsed the way the server's own test parses it: the rows between
    /// `<!-- tool-routes:NAME:begin -->` and the matching end marker.
    const TOOL_SCHEMA: &str = include_str!("../../../docs/tool-schema-control-plane.md");

    /// The rows of one marked block, as `(tool, method, path, capability)`.
    fn marked_rows(block: &str) -> Vec<(String, String, String, String)> {
        let begin = format!("<!-- tool-routes:{block}:begin -->");
        let end = format!("<!-- tool-routes:{block}:end -->");
        let mut inside = false;
        let mut rows = Vec::new();
        for line in TOOL_SCHEMA.lines() {
            let trimmed = line.trim();
            if trimmed == begin {
                inside = true;
                continue;
            }
            if trimmed == end {
                break;
            }
            if !inside {
                continue;
            }
            let cells: Vec<&str> = trimmed
                .trim_matches('|')
                .split('|')
                .map(str::trim)
                .collect();
            // The header and the separator are not rows.
            if cells.len() < 5 || !cells[0].starts_with('`') {
                continue;
            }
            rows.push((
                cells[0].trim_matches('`').to_string(),
                cells[1].to_string(),
                cells[2].trim_matches('`').to_string(),
                cells[3].trim_matches('`').to_string(),
            ));
        }
        rows
    }

    /// The drift guard: the SDK's table is exactly the documented `queries` block — which is exactly
    /// the server's own `ROUTES` filtered to `GET`, asserted by the server's own test. One list, two
    /// checks, no dependency between them.
    #[test]
    fn the_endpoint_table_matches_the_documented_queries() {
        let mut documented = marked_rows("queries");
        documented.sort();
        let mut sdk: Vec<(String, String, String, String)> = QUERY_ENDPOINTS
            .iter()
            .map(|endpoint| {
                (
                    endpoint.tool.to_string(),
                    endpoint.method.to_string(),
                    endpoint.path.to_string(),
                    endpoint.capability.to_string(),
                )
            })
            .collect();
        sdk.sort();
        assert_eq!(
            sdk, documented,
            "the SDK's query table against the document"
        );
        assert_eq!(QUERY_ENDPOINTS.len(), 37, "§5.1 is 37 queries");
        assert!(
            QUERY_ENDPOINTS
                .iter()
                .all(|endpoint| endpoint.method == "GET"),
            "every query is a GET"
        );
    }

    #[test]
    fn no_two_endpoints_share_a_path_or_a_tool_name() {
        for (index, endpoint) in QUERY_ENDPOINTS.iter().enumerate() {
            for other in &QUERY_ENDPOINTS[index + 1..] {
                assert_ne!(endpoint.path, other.path, "two endpoints share a path");
                assert_ne!(endpoint.tool, other.tool, "two endpoints share a tool name");
            }
        }
    }

    #[test]
    fn the_error_object_becomes_a_typed_error() {
        let body = br#"{"code":"forbidden","message":"the actor may not session.write","retryable":false,"cause":"capability"}"#;
        let error = ApiError::from_body(403, body);
        assert_eq!(error.status, 403);
        assert_eq!(error.code, "forbidden");
        assert_eq!(error.message, "the actor may not session.write");
        assert!(!error.retryable);
        assert_eq!(error.cause.as_deref(), Some("capability"));
        assert_eq!(
            error.to_string(),
            "forbidden (HTTP 403): the actor may not session.write [cause: capability]"
        );
    }

    #[test]
    fn an_unreadable_error_body_is_still_an_error() {
        let error = ApiError::from_body(500, b"<html>not json</html>");
        assert_eq!(error.code, "internal");
        assert!(!error.retryable);
        assert_eq!(error.cause, None);
    }

    #[test]
    fn a_missing_cause_and_a_null_cause_are_both_none() {
        let body = br#"{"code":"not_found","message":"no run with id run-1","retryable":false,"cause":null}"#;
        assert_eq!(ApiError::from_body(404, body).cause, None);
        let without = br#"{"code":"internal","message":"x","retryable":true}"#;
        let error = ApiError::from_body(500, without);
        assert!(error.retryable);
        assert_eq!(error.cause, None);
    }

    #[test]
    fn the_request_parameters_serialize_and_skip_what_is_absent() {
        let events = AuditEvents {
            limit: 10,
            actor: Some("local-1-1".to_string()),
            ..Default::default()
        };
        let pairs = events.to_query();
        assert_eq!(
            pairs,
            vec![
                ("limit", "10".to_string()),
                ("actor", "local-1-1".to_string())
            ]
        );

        let runs = Runs { limit: None };
        assert!(runs.to_query().is_empty());
        let runs = Runs { limit: Some(5) };
        assert_eq!(runs.to_query(), vec![("limit", "5".to_string())]);

        let file = WorkspaceFile {
            path: "src/main.rs".to_string(),
        };
        assert_eq!(file.to_query(), vec![("path", "src/main.rs".to_string())]);
    }

    /// One HTTP/1.1 answer over a loopback socket, so the client is exercised without a network —
    /// and the request it sent comes back for inspection.
    fn serve_typed(
        status: u16,
        content_type: &'static str,
        body: &'static str,
    ) -> (String, std::thread::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buffer = vec![0u8; 16384];
            let read = stream.read(&mut buffer).unwrap();
            let request = String::from_utf8_lossy(&buffer[..read]).to_string();
            let response = format!(
                "HTTP/1.1 {status} X\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).unwrap();
            request
        });
        (format!("http://{address}"), handle)
    }

    fn serve_once(status: u16, body: &'static str) -> (String, std::thread::JoinHandle<String>) {
        serve_typed(status, "application/json", body)
    }

    #[test]
    fn a_query_carries_the_bearer_and_reads_json() {
        let (base, server) = serve_once(200, r#"{"status":"ok","uptime_ms":5}"#);
        let client = Client::new(base, "the-node-token").unwrap();
        let answer = client.audit_status().unwrap();
        assert_eq!(answer["status"], "ok");
        let request = server.join().unwrap();
        assert!(
            request.starts_with("GET /v0/audit/status "),
            "the path was not the endpoint's: {request}"
        );
        assert!(
            request.contains("authorization: Bearer the-node-token"),
            "the bearer was not sent: {request}"
        );
    }

    #[test]
    fn an_error_answer_becomes_the_typed_error() {
        let body = r#"{"code":"unauthorized","message":"missing or invalid bearer token","retryable":false,"cause":null}"#;
        let (base, server) = serve_once(401, body);
        let client = Client::new(base, "wrong").unwrap();
        let error = client.audit_status().unwrap_err();
        let api = error.api().expect("an API error");
        assert_eq!(api.status, 401);
        assert_eq!(api.code, "unauthorized");
        assert!(!api.retryable);
        server.join().unwrap();
    }

    #[test]
    fn the_agent_name_is_sent_when_it_is_set() {
        let (base, server) = serve_once(200, "{}");
        let client = Client::new(base, "t")
            .unwrap()
            .with_agent_name("supervisor-1");
        client.capabilities().unwrap();
        let request = server.join().unwrap();
        assert!(
            request.contains("x-riscdom-agent: supervisor-1"),
            "the agent name was not sent: {request}"
        );
    }

    #[test]
    fn query_parameters_travel_in_the_url() {
        let (base, server) = serve_once(200, "[]");
        let client = Client::new(base, "t").unwrap();
        client
            .workspace_file(&WorkspaceFile {
                path: "src/lib.rs".to_string(),
            })
            .unwrap();
        let request = server.join().unwrap();
        assert!(
            request.starts_with("GET /v0/workspace/file?path=src%2Flib.rs "),
            "the query was not sent: {request}"
        );
    }

    #[test]
    fn the_control_table_matches_the_documented_controls() {
        let mut documented = marked_rows("controls");
        documented.sort();
        let mut sdk: Vec<(String, String, String, String)> = CONTROL_ENDPOINTS
            .iter()
            .map(|endpoint| {
                (
                    endpoint.tool.to_string(),
                    endpoint.method.to_string(),
                    endpoint.path.to_string(),
                    endpoint.capability.to_string(),
                )
            })
            .collect();
        sdk.sort();
        assert_eq!(
            sdk, documented,
            "the SDK's control table against the document"
        );
        assert_eq!(CONTROL_ENDPOINTS.len(), 36, "§5.2 is 36 controls");
        assert!(
            CONTROL_ENDPOINTS
                .iter()
                .all(|endpoint| endpoint.method == "POST"),
            "every control is a POST"
        );
    }

    /// Across **both** tables: a tool name is unique, and so is a `(method, path)` pair — the query and
    /// control tables share paths by design (`POST /v0/toolchain/download` beside its `GET`), and the
    /// tool names are what tell them apart.
    #[test]
    fn no_two_endpoints_share_a_tool_name_or_a_method_and_path() {
        let all: Vec<&Endpoint> = QUERY_ENDPOINTS
            .iter()
            .chain(CONTROL_ENDPOINTS.iter())
            .collect();
        for (index, endpoint) in all.iter().enumerate() {
            for other in &all[index + 1..] {
                assert_ne!(endpoint.tool, other.tool, "two endpoints share a tool name");
                assert!(
                    endpoint.method != other.method || endpoint.path != other.path,
                    "two endpoints share a method and a path: {} {}",
                    endpoint.method,
                    endpoint.path
                );
            }
        }
    }

    #[test]
    fn a_control_posts_its_json_body_with_the_bearer_and_the_agent() {
        let (base, server) = serve_once(200, r#"{"id":"req-1"}"#);
        let client = Client::new(base, "the-node-token")
            .unwrap()
            .with_agent_name("supervisor-1");
        let answer = client
            .sandboxes_switch(&SandboxesSwitch {
                name: "blink".to_string(),
            })
            .unwrap();
        assert_eq!(answer["id"], "req-1");
        let request = server.join().unwrap();
        let lower = request.to_lowercase();
        assert!(
            request.starts_with("POST /v0/sandboxes/switch "),
            "the path was not the endpoint's: {request}"
        );
        assert!(
            lower.contains("authorization: bearer the-node-token"),
            "{request}"
        );
        assert!(lower.contains("x-riscdom-agent: supervisor-1"), "{request}");
        assert!(
            lower.contains("content-type: application/json"),
            "{request}"
        );
        assert!(request.contains(r#"{"name":"blink"}"#), "{request}");
    }

    #[test]
    fn a_control_without_a_body_sends_none() {
        let (base, server) = serve_once(200, "null");
        let client = Client::new(base, "t").unwrap();
        client.vm_stop().unwrap();
        let request = server.join().unwrap();
        assert!(request.starts_with("POST /v0/vm/stop "), "{request}");
        assert!(
            !request.to_lowercase().contains("content-type"),
            "a bodyless control sent a content type: {request}"
        );
    }

    #[test]
    fn workspace_export_answers_with_bytes_not_json() {
        let (base, server) = serve_typed(200, "application/zip", "PK-an-archive");
        let client = Client::new(base, "t").unwrap();
        assert_eq!(client.workspace_export().unwrap(), b"PK-an-archive");
        let request = server.join().unwrap();
        assert!(
            request.starts_with("POST /v0/workspace/export "),
            "{request}"
        );
    }

    #[test]
    fn workspace_import_sends_the_archive_and_asks_for_force() {
        let (base, server) = serve_once(200, r#"{"files":1,"bytes":4}"#);
        let client = Client::new(base, "t").unwrap();
        let answer = client.workspace_import(b"PK\x03\x04", true).unwrap();
        assert_eq!(answer["files"], 1);
        let request = server.join().unwrap();
        assert!(
            request.starts_with("POST /v0/workspace/import?force=true "),
            "{request}"
        );
    }

    /// The stream, including the two things [control-plane-events.md](../../docs/control-plane-events.md)
    /// §1 and §2 insist on: the keep-alive comment is not a frame, and a `gap` is a **re-sync
    /// instruction**, surfaced as a kind — never an error, and never abstracted away.
    #[test]
    fn frames_arrive_typed_and_a_gap_is_an_instruction() {
        let body = concat!(
            "id: 0-0\n",
            "data: {\"version\":1,\"kind\":\"hello\",\"event\":null,\"agent_id\":\"server\",\"task_id\":null,\"ts\":1,\"payload\":{\"buffer\":{\"from\":0,\"to\":42}}}\n",
            "\n",
            ": keep-alive\n",
            "\n",
            "id: 1758533001207-1\n",
            "data: {\"version\":1,\"kind\":\"event\",\"event\":\"agent:tool_call\",\"agent_id\":\"dev-1-1\",\"task_id\":\"task-1-1\",\"ts\":1758533001207,\"payload\":{\"name\":\"write_source\"}}\n",
            "\n",
            "id: 1758533001880-2\n",
            "data: {\"version\":1,\"kind\":\"gap\",\"event\":null,\"agent_id\":\"server\",\"task_id\":null,\"ts\":1758533001880,\"payload\":{\"lost_after\":\"1758533001777-9\"}}\n",
            "\n",
        );
        let (base, server) = serve_typed(200, "text/event-stream", body);
        let client = Client::new(base, "t").unwrap();
        let mut subscription = client.subscribe(&Filters::default(), None).unwrap();

        let hello = subscription.next_frame().unwrap().expect("a hello frame");
        assert_eq!(hello.id.as_deref(), Some("0-0"));
        assert_eq!(hello.frame_kind(), Some(FrameKind::Hello));

        let event = subscription.next_frame().unwrap().expect("an event frame");
        assert_eq!(event.frame_kind(), Some(FrameKind::Event));
        let envelope = event.envelope().unwrap();
        assert_eq!(envelope.event.as_deref(), Some("agent:tool_call"));
        assert_eq!(envelope.task_id.as_deref(), Some("task-1-1"));
        assert_eq!(envelope.payload["name"], "write_source");
        assert_eq!(subscription.last_id(), Some("1758533001207-1"));

        let gap = subscription.next_frame().unwrap().expect("a gap frame");
        assert_eq!(gap.frame_kind(), Some(FrameKind::Gap));
        assert_eq!(
            gap.envelope().unwrap().lost_after(),
            Some("1758533001777-9"),
            "a gap names the oldest id the server still holds"
        );

        assert!(
            subscription.next_frame().unwrap().is_none(),
            "end of stream"
        );
        server.join().unwrap();
    }

    #[test]
    fn an_unknown_frame_kind_is_not_an_error() {
        let envelope = Frame {
            id: None,
            data: r#"{"version":1,"kind":"something-new","event":null,"agent_id":"server","task_id":null,"ts":1,"payload":{}}"#.to_string(),
        }
        .envelope()
        .unwrap();
        assert_eq!(
            envelope.frame_kind(),
            FrameKind::Unknown("something-new".to_string())
        );
    }

    #[test]
    fn filters_travel_as_repeated_query_parameters() {
        let filters = Filters {
            events: vec!["agent:tool_call".to_string(), "vm:state".to_string()],
            agent_id: Some("dev-12345-1".to_string()),
            task_id: None,
        };
        assert_eq!(
            filters.to_query(),
            vec![
                ("event", "agent:tool_call".to_string()),
                ("event", "vm:state".to_string()),
                ("agent_id", "dev-12345-1".to_string()),
            ]
        );
        assert!(Filters::default().to_query().is_empty());
    }

    #[test]
    fn a_subscription_reports_the_cursor_it_resumed_from() {
        let (base, server) = serve_typed(200, "text/event-stream", "");
        let client = Client::new(base, "t").unwrap();
        let mut subscription = client
            .subscribe(&Filters::default(), Some("1758533001207-1"))
            .unwrap();
        assert_eq!(subscription.last_id(), Some("1758533001207-1"));
        assert!(subscription.next_frame().unwrap().is_none());
        let request = server.join().unwrap();
        assert!(
            request
                .to_lowercase()
                .contains("last-event-id: 1758533001207-1"),
            "the resume cursor was not sent: {request}"
        );
        assert!(request.starts_with("GET /v0/events "), "{request}");
    }
}
