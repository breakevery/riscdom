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

use serde::Serialize;
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

    /// `GET` a path with a query, and read the answer as JSON.
    ///
    /// Every typed method below is this call with the endpoint's own path and parameters, so a caller
    /// that needs an endpoint the SDK does not wrap can still make the call — the surface is the API's,
    /// not the SDK's.
    pub fn get(&self, path: &str, query: &[(&str, String)]) -> Result<Value, ClientError> {
        let url = format!("{}{path}", self.base_url);
        let mut request = self.http.get(url).bearer_auth(&self.auth);
        if !query.is_empty() {
            request = request.query(query);
        }
        if let Some(name) = &self.agent_name {
            request = request.header("X-RiscDom-Agent", name);
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
        if body.is_empty() {
            return Ok(Value::Null);
        }
        serde_json::from_slice(&body).map_err(|error| ClientError::Transport(error.to_string()))
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
    fn serve_once(status: u16, body: &'static str) -> (String, std::thread::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buffer = vec![0u8; 8192];
            let read = stream.read(&mut buffer).unwrap();
            let request = String::from_utf8_lossy(&buffer[..read]).to_string();
            let response = format!(
                "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).unwrap();
            request
        });
        (format!("http://{address}"), handle)
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
}
