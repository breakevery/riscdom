//! The endpoint surface: the queries and the controls of `docs/control-plane-api.md`
//! §5.1 and §5.2, the two path-parameter routes (`/v0/runs/{run_id}` and
//! `/v0/sandboxes/{name}`), the reserved `/v0/resources` and `/v0/vm/start`, the
//! host-local endpoints, the route table that names them, and the dispatcher that
//! calls `AppState`.
//!
//! The tables are `docs/control-plane-api.md` §5.1 and §5.2.

use crate::auth::{Actor, Capability};
use crate::http::{binary_response, error_response, json_response, no_content, RespBody};
use crate::log::{self, LogLevel};
use crate::sse::{HttpEventSink, SseHub};
use host_core::{
    AppState, ArchiveFormat, EventSink, HostError, SandboxAction, SandboxRequestStatus,
};
use hyper::body::Bytes;
use hyper::header::{HeaderValue, CONTENT_DISPOSITION};
use hyper::{Response, StatusCode};
use std::collections::HashMap;
use std::sync::Arc;

/// One endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Action {
    // ---- queries (§5.1) ----
    AuditStatus,
    AuditEvents,
    Runs,
    Run,
    RunDiff,
    LlmProviderPresets,
    LlmConfig,
    LlmReadiness,
    LlmLocalProbe,
    LlmStoredKey,
    Sessions,
    SessionCurrent,
    Snapshots,
    VmRunning,
    VmStatus,
    Toolchain,
    ToolchainDownload,
    Qemu,
    QemuStatus,
    Preflight,
    SettingsTheme,
    SettingsLanguage,
    WorkspaceRoot,
    WorkspaceFiles,
    WorkspaceFile,
    Serial,
    /// The sandbox registry (v0.9 sandbox F2a-2). Read-only: switching is F2b.
    Sandboxes,
    SandboxCurrent,
    SandboxCandidates,
    /// `/v0/sandboxes/{name}`, the second path-parameter route.
    Sandbox,
    /// The reserved aggregate (§6, G3): answers 501.
    Resources,
    // ---- controls (§5.2) ----
    AgentRun,
    RunExport,
    RunsAbandonStale,
    VmStart,
    VmStop,
    SnapshotSave,
    SnapshotResume,
    SnapshotDelete,
    SessionCreate,
    SessionOpen,
    SessionRename,
    SessionDelete,
    SessionClear,
    ToolchainDownloadStart,
    ToolchainDownloadCancel,
    /// QEMU download (v0.9 sandbox F1): the same three shapes as the toolchain's.
    QemuDownload,
    QemuDownloadStart,
    QemuDownloadCancel,
    ToolchainPath,
    ToolchainPathClear,
    QemuPath,
    QemuPathClear,
    PreflightRun,
    PreflightAck,
    AuditAlert,
    AuditExport,
    SettingsThemeSet,
    SettingsLanguageSet,
    LlmConfigSet,
    LlmStoredKeyLoad,
    LlmConfigClear,
    SerialExport,
    /// The sandbox switch (v0.9 sandbox F2b-2): the one write on that surface.
    SandboxSwitch,
    /// The request queue, read (v0.9 sandbox F2c).
    SandboxRequests,
    /// Leave a request (v0.9 sandbox F2c).
    SandboxRequestCreate,
    /// Decide a request: the two halves of the one write the queue allows.
    SandboxRequestApprove,
    SandboxRequestReject,
    /// Bring a project in, and take one out (v0.9 project in/out).
    WorkspaceImport,
    WorkspaceExport,
}

/// What a request resolves to.
#[derive(Debug)]
pub(crate) enum Resolution {
    Query {
        action: Action,
        capability: Capability,
        /// `/v0/runs/{run_id}` and `/v0/sandboxes/{name}` are the paths with one.
        path_param: Option<(&'static str, String)>,
    },
    /// An endpoint the server answers itself (it needs more than `AppState`).
    Local { kind: Local, capability: Capability },
    /// The path is served, the method is not. `allowed` lists the methods it is
    /// served under (one path can have two: `/v0/toolchain/download` is a `GET`
    /// status query and a `POST` start).
    MethodNotAllowed { allowed: String },
    /// Nothing serves this path.
    NotFound,
}

/// The endpoints served by the server rather than by a query on `AppState`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Local {
    Health,
    Status,
    Events,
}

/// The route table: `(method, path, capability, action)`.
///
/// The capability is declared here and handed to the [`Authn`](crate::Authn) hook
/// through [`ReqMeta::capability`](crate::ReqMeta); enforcing it is the permission
/// intermediary's job, which is a later batch, so the check is *recorded* and the
/// endpoint's authorisation today comes from the token alone.
const ROUTES: &[(&str, &str, Capability, Action)] = &[
    // ---- queries ----
    (
        "GET",
        "/v0/audit/status",
        Capability::AuditRead,
        Action::AuditStatus,
    ),
    (
        "GET",
        "/v0/audit/events",
        Capability::AuditRead,
        Action::AuditEvents,
    ),
    ("GET", "/v0/runs", Capability::RunsRead, Action::Runs),
    (
        "GET",
        "/v0/runs/diff",
        Capability::RunsRead,
        Action::RunDiff,
    ),
    (
        "GET",
        "/v0/llm/provider-presets",
        Capability::LlmRead,
        Action::LlmProviderPresets,
    ),
    (
        "GET",
        "/v0/llm/config",
        Capability::LlmRead,
        Action::LlmConfig,
    ),
    (
        "GET",
        "/v0/llm/readiness",
        Capability::LlmRead,
        Action::LlmReadiness,
    ),
    (
        "GET",
        "/v0/llm/local-probe",
        Capability::LlmRead,
        Action::LlmLocalProbe,
    ),
    (
        "GET",
        "/v0/llm/stored-key",
        Capability::LlmRead,
        Action::LlmStoredKey,
    ),
    (
        "GET",
        "/v0/sessions",
        Capability::SessionRead,
        Action::Sessions,
    ),
    (
        "GET",
        "/v0/sessions/current",
        Capability::SessionRead,
        Action::SessionCurrent,
    ),
    (
        "GET",
        "/v0/snapshots",
        Capability::SnapshotRead,
        Action::Snapshots,
    ),
    (
        "GET",
        "/v0/vm/running",
        Capability::VmRead,
        Action::VmRunning,
    ),
    ("GET", "/v0/vm/status", Capability::VmRead, Action::VmStatus),
    (
        "GET",
        "/v0/toolchain",
        Capability::ToolchainRead,
        Action::Toolchain,
    ),
    (
        "GET",
        "/v0/toolchain/download",
        Capability::ToolchainRead,
        Action::ToolchainDownload,
    ),
    (
        "GET",
        "/v0/qemu/download",
        Capability::QemuRead,
        Action::QemuDownload,
    ),
    ("GET", "/v0/qemu", Capability::QemuRead, Action::Qemu),
    (
        "GET",
        "/v0/qemu/status",
        Capability::QemuRead,
        Action::QemuStatus,
    ),
    (
        "GET",
        "/v0/preflight",
        Capability::PreflightRead,
        Action::Preflight,
    ),
    (
        "GET",
        "/v0/settings/theme",
        Capability::SettingsRead,
        Action::SettingsTheme,
    ),
    (
        "GET",
        "/v0/settings/language",
        Capability::SettingsRead,
        Action::SettingsLanguage,
    ),
    (
        "GET",
        "/v0/workspace/root",
        Capability::WorkspaceRead,
        Action::WorkspaceRoot,
    ),
    (
        "GET",
        "/v0/workspace/files",
        Capability::WorkspaceRead,
        Action::WorkspaceFiles,
    ),
    (
        "GET",
        "/v0/workspace/file",
        Capability::WorkspaceRead,
        Action::WorkspaceFile,
    ),
    ("GET", "/v0/serial", Capability::SerialRead, Action::Serial),
    (
        "GET",
        "/v0/sandboxes",
        Capability::SandboxRead,
        Action::Sandboxes,
    ),
    (
        "GET",
        "/v0/sandboxes/current",
        Capability::SandboxRead,
        Action::SandboxCurrent,
    ),
    (
        "GET",
        "/v0/sandboxes/candidates",
        Capability::SandboxRead,
        Action::SandboxCandidates,
    ),
    // The request queue (v0.9 sandbox F2c). Its two decisions (`approve` /
    // `reject`) carry an id, so they are pattern routes like `/v0/runs/{run_id}`
    // — resolved below, and deliberately not rows here.
    (
        "GET",
        "/v0/sandboxes/requests",
        Capability::SandboxRead,
        Action::SandboxRequests,
    ),
    // Reserved (§6, G3): served, and answers 501 until the aggregate lands.
    (
        "GET",
        "/v0/resources",
        Capability::VmRead,
        Action::Resources,
    ),
    // ---- controls ----
    (
        "POST",
        "/v0/agent/run",
        Capability::AgentRun,
        Action::AgentRun,
    ),
    (
        "POST",
        "/v0/runs/export",
        Capability::AuditExport,
        Action::RunExport,
    ),
    (
        "POST",
        "/v0/runs/abandon-stale",
        Capability::RunsControl,
        Action::RunsAbandonStale,
    ),
    // Reserved (§6, G1): served, and answers 501 until the kernel grows a
    // standalone "start a VM" method.
    (
        "POST",
        "/v0/vm/start",
        Capability::VmControl,
        Action::VmStart,
    ),
    ("POST", "/v0/vm/stop", Capability::VmControl, Action::VmStop),
    (
        "POST",
        "/v0/snapshots/save",
        Capability::SnapshotWrite,
        Action::SnapshotSave,
    ),
    (
        "POST",
        "/v0/snapshots/resume",
        Capability::SnapshotWrite,
        Action::SnapshotResume,
    ),
    (
        "POST",
        "/v0/snapshots/delete",
        Capability::SnapshotWrite,
        Action::SnapshotDelete,
    ),
    (
        "POST",
        "/v0/sessions/create",
        Capability::SessionWrite,
        Action::SessionCreate,
    ),
    (
        "POST",
        "/v0/sessions/open",
        Capability::SessionWrite,
        Action::SessionOpen,
    ),
    (
        "POST",
        "/v0/sessions/rename",
        Capability::SessionWrite,
        Action::SessionRename,
    ),
    (
        "POST",
        "/v0/sessions/delete",
        Capability::SessionWrite,
        Action::SessionDelete,
    ),
    (
        "POST",
        "/v0/sessions/clear",
        Capability::SessionWrite,
        Action::SessionClear,
    ),
    (
        "POST",
        "/v0/toolchain/download",
        Capability::ToolchainInstall,
        Action::ToolchainDownloadStart,
    ),
    (
        "POST",
        "/v0/toolchain/download/cancel",
        Capability::ToolchainInstall,
        Action::ToolchainDownloadCancel,
    ),
    (
        "POST",
        "/v0/qemu/download",
        Capability::QemuConfigure,
        Action::QemuDownloadStart,
    ),
    (
        "POST",
        "/v0/qemu/download/cancel",
        Capability::QemuConfigure,
        Action::QemuDownloadCancel,
    ),
    (
        "POST",
        "/v0/toolchain/path",
        Capability::ToolchainConfigure,
        Action::ToolchainPath,
    ),
    (
        "POST",
        "/v0/toolchain/path/clear",
        Capability::ToolchainConfigure,
        Action::ToolchainPathClear,
    ),
    (
        "POST",
        "/v0/qemu/path",
        Capability::QemuConfigure,
        Action::QemuPath,
    ),
    (
        "POST",
        "/v0/qemu/path/clear",
        Capability::QemuConfigure,
        Action::QemuPathClear,
    ),
    (
        "POST",
        "/v0/preflight/run",
        Capability::PreflightRun,
        Action::PreflightRun,
    ),
    (
        "POST",
        "/v0/preflight/ack",
        Capability::PreflightRun,
        Action::PreflightAck,
    ),
    (
        "POST",
        "/v0/audit/alert",
        Capability::SettingsWrite,
        Action::AuditAlert,
    ),
    (
        "POST",
        "/v0/audit/export",
        Capability::AuditExport,
        Action::AuditExport,
    ),
    (
        "POST",
        "/v0/settings/theme",
        Capability::SettingsWrite,
        Action::SettingsThemeSet,
    ),
    (
        "POST",
        "/v0/settings/language",
        Capability::SettingsWrite,
        Action::SettingsLanguageSet,
    ),
    (
        "POST",
        "/v0/llm/config",
        Capability::LlmConfigure,
        Action::LlmConfigSet,
    ),
    (
        "POST",
        "/v0/llm/stored-key/load",
        Capability::LlmConfigure,
        Action::LlmStoredKeyLoad,
    ),
    (
        "POST",
        "/v0/llm/config/clear",
        Capability::LlmConfigure,
        Action::LlmConfigClear,
    ),
    (
        "POST",
        "/v0/serial/export",
        Capability::SerialExport,
        Action::SerialExport,
    ),
    (
        "POST",
        "/v0/sandboxes/switch",
        Capability::SandboxSwitch,
        Action::SandboxSwitch,
    ),
    (
        "POST",
        "/v0/sandboxes/requests",
        Capability::AgentRun,
        Action::SandboxRequestCreate,
    ),
    // Project in/out (v0.9). Import is the only route whose body is not JSON, and
    // it is the only one that needs `workspace.write`: everything else on the
    // workspace surface reads.
    (
        "POST",
        "/v0/workspace/import",
        Capability::WorkspaceWrite,
        Action::WorkspaceImport,
    ),
    (
        "POST",
        "/v0/workspace/export",
        Capability::WorkspaceRead,
        Action::WorkspaceExport,
    ),
];

/// `/v0/runs/{run_id}` is the one path with a parameter.
const RUN_PREFIX: &str = "/v0/runs/";

/// `/v0/sandboxes/{name}`, the sandbox registry's path-parameter route.
const SANDBOX_PREFIX: &str = "/v0/sandboxes/";

/// The endpoints this crate added to the settled surface (see the API document's
/// "host-local endpoints"): a liveness check, a summary, and the event stream.
const LOCAL_ROUTES: &[(&str, Local, Capability)] = &[
    ("/v0/health", Local::Health, Capability::HealthRead),
    ("/v0/status", Local::Status, Capability::StatusRead),
    ("/v0/events", Local::Events, Capability::EventsSubscribe),
];

/// Query-string and body parameters, flattened to strings.
#[derive(Debug, Default)]
pub(crate) struct Params {
    values: HashMap<String, String>,
}

impl Params {
    /// Read a parameter, `None` when absent or empty.
    pub fn get(&self, name: &str) -> Option<&str> {
        self.values
            .get(name)
            .map(String::as_str)
            .filter(|v| !v.is_empty())
    }

    /// Set one (the path parameter from `/v0/runs/{run_id}`).
    pub fn insert(&mut self, name: impl Into<String>, value: impl Into<String>) {
        self.values.insert(name.into(), value.into());
    }

    /// Fold the body's scalar fields in; an explicitly sent body wins over the
    /// query string.
    pub fn merge(&mut self, other: Params) {
        for (key, value) in other.values {
            self.values.insert(key, value);
        }
    }

    /// Flatten a JSON body's scalar fields. Nested values are not addressable and
    /// are dropped: a required lookup then answers `400`, which is the honest
    /// answer for "you sent the wrong shape".
    pub fn from_json(value: &serde_json::Value) -> Params {
        let mut values = HashMap::new();
        if let Some(object) = value.as_object() {
            for (key, value) in object {
                match value {
                    serde_json::Value::String(text) => {
                        values.insert(key.clone(), text.clone());
                    }
                    serde_json::Value::Bool(flag) => {
                        values.insert(key.clone(), flag.to_string());
                    }
                    serde_json::Value::Number(number) => {
                        values.insert(key.clone(), number.to_string());
                    }
                    _ => {}
                }
            }
        }
        Params { values }
    }

    /// Read a required parameter, or the `400` to answer with.
    pub fn required(&self, name: &str) -> Result<&str, Box<Response<RespBody>>> {
        self.get(name)
            .ok_or_else(|| Box::new(bad_request(name, "is required")))
    }

    /// Read a required non-negative integer, or the `400` to answer with.
    pub fn usize_required(&self, name: &str) -> Result<usize, Box<Response<RespBody>>> {
        match self.required(name)?.parse() {
            Ok(value) => Ok(value),
            Err(_) => Err(Box::new(bad_request(name, "must be a non-negative number"))),
        }
    }

    /// Read an optional non-negative integer, falling back to `default`.
    pub fn usize_or(&self, name: &str, default: usize) -> Result<usize, Box<Response<RespBody>>> {
        match self.get(name) {
            Some(raw) => raw
                .parse()
                .map_err(|_| Box::new(bad_request(name, "must be a non-negative number"))),
            None => Ok(default),
        }
    }

    /// Read a required boolean (`true` / `false`), or the `400` to answer with.
    pub fn bool_required(&self, name: &str) -> Result<bool, Box<Response<RespBody>>> {
        match self.required(name)? {
            "true" => Ok(true),
            "false" => Ok(false),
            _ => Err(Box::new(bad_request(name, "must be true or false"))),
        }
    }

    /// Read an optional boolean, falling back to `default`.
    pub fn bool_or(&self, name: &str, default: bool) -> Result<bool, Box<Response<RespBody>>> {
        match self.get(name) {
            Some("true") => Ok(true),
            Some("false") => Ok(false),
            Some(_) => Err(Box::new(bad_request(name, "must be true or false"))),
            None => Ok(default),
        }
    }
}

/// The `400` for a parameter that is missing or unusable.
fn bad_request(name: &str, why: &str) -> Response<RespBody> {
    error_response(
        400,
        "bad_request",
        &format!("parameter {name:?} {why}"),
        Some(name),
    )
}

/// Parse `a=1&b=two` into a parameter set. Both sides are percent-decoded, so a
/// path parameter may travel as `?path=src%2Fmain.c`.
pub(crate) fn parse_query(query: Option<&str>) -> Params {
    let mut values = HashMap::new();
    for pair in query.unwrap_or("").split('&').filter(|p| !p.is_empty()) {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        values.insert(decode(key), decode(value));
    }
    Params { values }
}

/// Percent-decode, treating `+` as a space. Byte-wise, so a multi-byte character
/// encoded as `%XX` sequences cannot split mid-character.
fn decode(raw: &str) -> String {
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hi = (bytes[i + 1] as char).to_digit(16);
            let lo = (bytes[i + 2] as char).to_digit(16);
            if let (Some(hi), Some(lo)) = (hi, lo) {
                out.push((hi * 16 + lo) as u8);
                i += 3;
                continue;
            }
        }
        out.push(if bytes[i] == b'+' { b' ' } else { bytes[i] });
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The `run_id` of `/v0/runs/<id>`, when the path is exactly that shape.
fn run_id_from(path: &str) -> Option<&str> {
    let rest = path.strip_prefix(RUN_PREFIX)?;
    if rest.is_empty() || rest.contains('/') {
        return None;
    }
    // The literal sub-paths of `/v0/runs/…` are their own routes.
    if matches!(rest, "diff" | "export" | "abandon-stale") {
        return None;
    }
    Some(rest)
}

/// `/v0/sandboxes/requests/<id>`, the request decisions' path.
const SANDBOX_REQUESTS_PREFIX: &str = "/v0/sandboxes/requests/";

/// The `<id>` of `/v0/sandboxes/requests/<id>/(approve|reject)`, when the path is
/// exactly that shape.
///
/// A third path with a parameter, and the first with **two** segments after it.
/// The tail names the decision, so one extractor answers for both routes and an
/// unknown tail is simply not a route. `sandbox_name_from` never sees these paths:
/// it refuses anything whose rest contains a `/`, and this one has two.
fn sandbox_request_decision_from(path: &str) -> Option<(&str, Action)> {
    let rest = path.strip_prefix(SANDBOX_REQUESTS_PREFIX)?;
    let (id, decision) = rest.rsplit_once('/')?;
    if id.is_empty() || id.contains('/') {
        return None;
    }
    let action = match decision {
        "approve" => Action::SandboxRequestApprove,
        "reject" => Action::SandboxRequestReject,
        _ => return None,
    };
    Some((id, action))
}

/// The `<name>` of `/v0/sandboxes/<name>`, when the path is exactly that shape.
///
/// The literal sub-paths of `/v0/sandboxes/…` are their own routes. The two that
/// exist today are matched by the table before this is reached; the three that the
/// rest of the F2 line adds (`requests` opens a switch, `switch` performs one,
/// `assemble` builds a definition) are named here as well, so a definition called
/// `switch` can never be mistaken for a command — it answers `404` until F2b/F2c
/// give those paths a handler.
fn sandbox_name_from(path: &str) -> Option<&str> {
    let rest = path.strip_prefix(SANDBOX_PREFIX)?;
    if rest.is_empty() || rest.contains('/') {
        return None;
    }
    if matches!(
        rest,
        "current" | "candidates" | "requests" | "switch" | "assemble"
    ) {
        return None;
    }
    Some(rest)
}

/// Resolve a request to its route. The query string is the caller's business.
pub(crate) fn resolve(method: &str, path: &str) -> Resolution {
    for (local_path, kind, capability) in LOCAL_ROUTES {
        if *local_path == path {
            return if method == "GET" {
                Resolution::Local {
                    kind: *kind,
                    capability: *capability,
                }
            } else {
                Resolution::MethodNotAllowed {
                    allowed: "GET".to_string(),
                }
            };
        }
    }
    // Two passes over the table on purpose: a path may be served under more than
    // one method, so the method has to be matched before the path is declared
    // unserved. Everything the path does serve is collected for the 405 answer.
    let mut allowed: Vec<&str> = Vec::new();
    for (route_method, route_path, capability, action) in ROUTES {
        if *route_path != path {
            continue;
        }
        if *route_method == method {
            return Resolution::Query {
                action: *action,
                capability: *capability,
                path_param: None,
            };
        }
        allowed.push(route_method);
    }
    if !allowed.is_empty() {
        return Resolution::MethodNotAllowed {
            allowed: allowed.join(", "),
        };
    }
    if let Some(run_id) = run_id_from(path) {
        if method != "GET" {
            return Resolution::MethodNotAllowed {
                allowed: "GET".to_string(),
            };
        }
        return Resolution::Query {
            action: Action::Run,
            capability: Capability::RunsRead,
            path_param: Some(("run_id", run_id.to_string())),
        };
    }
    if let Some((id, action)) = sandbox_request_decision_from(path) {
        if method != "POST" {
            return Resolution::MethodNotAllowed {
                allowed: "POST".to_string(),
            };
        }
        // The gate is `sandbox.read` — a decider has to be able to see the queue
        // it decides on. Which capability the *decision* needs follows from the
        // request's own `action`, so it is checked in the handler, where the id is
        // resolved and the actor is in hand (F2c decision 1).
        return Resolution::Query {
            action,
            capability: Capability::SandboxRead,
            path_param: Some(("request_id", id.to_string())),
        };
    }
    if let Some(name) = sandbox_name_from(path) {
        if method != "GET" {
            return Resolution::MethodNotAllowed {
                allowed: "GET".to_string(),
            };
        }
        return Resolution::Query {
            action: Action::Sandbox,
            capability: Capability::SandboxRead,
            path_param: Some(("name", name.to_string())),
        };
    }
    Resolution::NotFound
}

/// The settings vocabularies the API document fixes. Validating them here means a
/// typo is a `400`, not a host error dressed up as a server fault.
const THEMES: &[&str] = &["light", "dark", "system"];
const LANGUAGES: &[&str] = &["system", "en", "zh"];

/// Run one endpoint against the host and shape the answer.
///
/// `actor` is the authenticated caller. Most routes never look at it — the hook
/// already checked the capability the route declares — but a decision on a
/// sandbox request does: which capability it needs follows from the request's own
/// `action`, and only the handler knows the request (F2c decision 1).
///
/// `archive` is the raw body of the one endpoint that takes bytes rather than a
/// JSON object (`POST /v0/workspace/import`, v0.9 project in/out); it is `None`
/// everywhere else.
#[allow(clippy::too_many_arguments)]
pub(crate) fn dispatch(
    action: Action,
    params: &Params,
    app: &Arc<AppState>,
    hub: &Arc<SseHub>,
    log_level: LogLevel,
    actor: &Actor,
    archive: Option<&[u8]>,
) -> Response<RespBody> {
    match action {
        // ---- queries ----
        Action::AuditStatus => match app.audit_status() {
            Ok(mut view) => {
                // A `GET` must not consume the queue another client is waiting for:
                // peek, where the Tauri command takes.
                view.failures = app.audit_failures();
                ok_json(&view)
            }
            Err(e) => host_error(e),
        },
        Action::AuditEvents => {
            let limit = match params.usize_required("limit") {
                Ok(limit) => limit,
                Err(response) => return *response,
            };
            let actor = params.get("actor").map(str::to_string);
            let action_prefix = params.get("action_prefix").map(str::to_string);
            result_json(app.list_events(limit, actor, action_prefix))
        }
        Action::Runs => match params.usize_or("limit", 20) {
            Ok(limit) => result_json(app.list_runs(limit)),
            Err(response) => *response,
        },
        Action::Run => match params.required("run_id") {
            Ok(run_id) => result_json(app.get_run(run_id)),
            Err(response) => *response,
        },
        Action::RunDiff => {
            let run_a = match params.required("run_a") {
                Ok(v) => v,
                Err(response) => return *response,
            };
            let run_b = match params.required("run_b") {
                Ok(v) => v,
                Err(response) => return *response,
            };
            result_json(app.compare_run_fingerprints(run_a, run_b))
        }
        Action::LlmProviderPresets => ok_json(&app.provider_presets()),
        Action::LlmConfig => ok_json(&app.llm_config_status()),
        Action::LlmReadiness => ok_json(&app.llm_readiness()),
        Action::LlmLocalProbe => ok_json(&app.probe_local_llm()),
        Action::LlmStoredKey => match params.required("provider_id") {
            Ok(provider_id) => {
                ok_json(&serde_json::json!({ "present": app.has_stored_key(provider_id) }))
            }
            Err(response) => *response,
        },
        Action::Sessions => match params.usize_required("limit") {
            Ok(limit) => result_json(app.list_sessions(limit)),
            Err(response) => *response,
        },
        Action::SessionCurrent => {
            ok_json(&serde_json::json!({ "session_id": app.current_session_id() }))
        }
        Action::Snapshots => result_json(app.list_snapshots()),
        Action::VmRunning => ok_json(&serde_json::json!({ "running": app.vm_is_running() })),
        Action::VmStatus => ok_json(&app.vm_status()),
        Action::Toolchain => ok_json(&app.probe_toolchain()),
        Action::ToolchainDownload => ok_json(&app.toolchain_download_status()),
        Action::Qemu | Action::QemuStatus => ok_json(&app.probe_qemu()),
        Action::Preflight => ok_json(&app.preflight_status()),
        Action::SettingsTheme => ok_json(&serde_json::json!({ "theme": app.theme() })),
        Action::SettingsLanguage => ok_json(&serde_json::json!({ "language": app.language() })),
        Action::WorkspaceRoot => {
            ok_json(&serde_json::json!({ "root": app.workspace_root_display() }))
        }
        Action::WorkspaceFiles => result_json(app.workspace_files()),
        Action::WorkspaceFile => match params.required("path") {
            Ok(path) => match app.read_workspace_file(path.to_string()) {
                Ok(content) => ok_json(&serde_json::json!({ "content": content })),
                Err(e) => host_error(e),
            },
            Err(response) => *response,
        },
        Action::Serial => ok_json(&serde_json::json!({ "buffer": app.serial_buffer() })),
        // ---- sandboxes (v0.9 sandbox F2a-2) ----
        Action::Sandboxes => ok_json(&serde_json::json!({
            "sandboxes": app.sandboxes(),
            "current": app.current_sandbox(),
            "default": app.sandbox_default_name(),
        })),
        Action::SandboxCurrent => ok_json(&serde_json::json!({
            "current": app.current_sandbox(),
            "default": app.sandbox_default_name(),
        })),
        // The scan's raw answer, not the registry: nothing here is a definition
        // yet, and nothing here has been written to `settings.json`.
        Action::SandboxCandidates => ok_json(&app.sandbox_candidates()),
        Action::Sandbox => match params.required("name") {
            Ok(name) => match app.sandbox(name) {
                Some(view) => ok_json(&view),
                None => error_response(
                    404,
                    "not_found",
                    &format!("no sandbox named {name:?}"),
                    Some("name"),
                ),
            },
            Err(response) => *response,
        },
        // The one write on the sandbox surface (v0.9 sandbox F2b-2). The switch is
        // synchronous — validation, a stop, a start — so it runs inline here (and
        // inline in a Tauri command), and the two pre-checks exist so a refusal
        // carries the status and `cause` a client can branch on (§4) instead of a
        // message nobody can match on.
        Action::SandboxSwitch => {
            let name = match params.required("name") {
                Ok(name) => name,
                Err(response) => return *response,
            };
            if app.run_in_flight() {
                return error_response(
                    409,
                    "conflict",
                    "a run is in flight; a sandbox switch would take its VM away",
                    Some("run"),
                );
            }
            if app.sandbox_switch_in_progress() {
                return error_response(
                    409,
                    "conflict",
                    "a sandbox switch is already in progress",
                    Some("sandbox"),
                );
            }
            let from = app.current_sandbox();
            let emitter: Arc<dyn host_core::EventSink> =
                Arc::new(HttpEventSink::new(Arc::clone(hub), app.agent_id()));
            match app.switch_sandbox(name, emitter) {
                // 200 with the two ends, not an empty `204`: a switch has something
                // to say, and a client that only reads the answer should not have
                // to subscribe to the stream to learn what changed.
                Ok(()) => ok_json(&serde_json::json!({ "from": from, "to": name })),
                Err(e) => sandbox_switch_error(e),
            }
        }
        // §6 G3: the shape is settled, the aggregate is not built.
        Action::Resources => error_response(
            501,
            "not_implemented",
            "resource accounting is reserved and not implemented yet",
            Some("resources"),
        ),
        // The request queue (v0.9 sandbox F2c). A request is a ledger entry, not a
        // command: it says what an actor wants, and someone who holds the
        // capability decides. Neither read nor write here switches anything.
        Action::SandboxRequests => {
            let status = match params.get("status") {
                None => None,
                Some(raw) => match SandboxRequestStatus::parse(raw) {
                    Some(status) => Some(status),
                    None => {
                        return error_response(
                            400,
                            "bad_request",
                            &format!("unknown status {raw:?}"),
                            Some("status"),
                        )
                    }
                },
            };
            ok_json(&serde_json::json!({ "requests": app.list_sandbox_requests(status) }))
        }
        Action::SandboxRequestCreate => {
            let raw = match params.required("action") {
                Ok(raw) => raw,
                Err(response) => return *response,
            };
            let Some(action) = SandboxAction::parse(raw) else {
                return error_response(
                    400,
                    "bad_request",
                    &format!("unknown action {raw:?}: expected switch, define or assemble"),
                    Some("action"),
                );
            };
            let sandbox = params.get("sandbox").map(str::to_string);
            let reason = params.get("reason").map(str::to_string);
            let emitter: Arc<dyn host_core::EventSink> =
                Arc::new(HttpEventSink::new(Arc::clone(hub), app.agent_id()));
            // Who asked: the caller's own identity. An agent with `agent.run` and
            // no `sandbox.switch` lands here and nowhere else (F2c decision 1).
            match app.request_sandbox(
                &actor.agent_id,
                action,
                sandbox,
                // The body carries no definition: registering one is the assemble
                // endpoint's job, and a path is not something a model names.
                None,
                reason,
                emitter,
            ) {
                // 201: something was created, and its id is the whole answer.
                Ok(view) => created_json(&serde_json::json!({ "id": view.id })),
                Err(e) => host_error(e),
            }
        }
        Action::SandboxRequestApprove | Action::SandboxRequestReject => {
            let id = match params.required("request_id") {
                Ok(id) => id,
                Err(response) => return *response,
            };
            // Two checks, in this order on purpose: resolving the request is what
            // says which capability the decision needs, so `404` comes first — and
            // then the capability the request's action implies is checked against
            // the actor the hook handed us (F2c decision 1).
            let wanted = match app.sandbox_request_action(id) {
                Ok(action) => capability_for_action(action),
                Err(e) => return sandbox_request_error(e),
            };
            if !actor.allows(wanted) {
                return error_response(
                    403,
                    "forbidden",
                    &format!("the actor may not {}", wanted.as_str()),
                    Some("capability"),
                );
            }
            let approve = matches!(action, Action::SandboxRequestApprove);
            let emitter: Arc<dyn host_core::EventSink> =
                Arc::new(HttpEventSink::new(Arc::clone(hub), app.agent_id()));
            let decided = if approve {
                app.approve_sandbox_request(id, &actor.agent_id, emitter)
            } else {
                app.reject_sandbox_request(id, &actor.agent_id, emitter)
            };
            match decided {
                // 200 with the record: what the decision landed on, not just "ok".
                Ok(view) => ok_json(&view),
                Err(e) => sandbox_request_error(e),
            }
        }
        // Project in/out (v0.9). Import is the one endpoint whose body is bytes
        // rather than a JSON object, so the archive arrives as an argument rather
        // than through `params`; everything it is checked for (traversal, links,
        // the host's own state, overwrite) lives in `host-core`'s archive reader,
        // so the HTTP surface cannot forget one of them.
        Action::WorkspaceImport => {
            let Some(archive) = archive else {
                return error_response(
                    400,
                    "bad_request",
                    "the request body must be a zip or a tar.gz archive",
                    Some("body"),
                );
            };
            // What the caller says it sent, or the bytes themselves when it says
            // nothing we recognise: a `.tar.gz` uploaded as octet-stream is still a
            // tar.gz, and refusing it would be pedantry.
            let format = params
                .get("content_type")
                .and_then(ArchiveFormat::from_content_type)
                .or_else(|| ArchiveFormat::from_magic(archive));
            let Some(format) = format else {
                return error_response(
                    400,
                    "bad_request",
                    "the body is neither a zip nor a gzip stream (send application/zip or application/gzip)",
                    Some("archive"),
                );
            };
            let force = match params.bool_or("force", false) {
                Ok(force) => force,
                Err(response) => return *response,
            };
            match app.import_workspace(archive, format, force) {
                Ok(report) => ok_json(&report),
                Err(e) => workspace_io_error(e),
            }
        }
        Action::WorkspaceExport => match app.export_workspace() {
            // Bytes, not JSON: the project itself is the answer. The name is the
            // caller's to choose; this is the one a browser saves it under.
            Ok(bytes) => {
                let mut response = binary_response(
                    StatusCode::OK,
                    ArchiveFormat::TarGz.content_type(),
                    Bytes::from(bytes),
                );
                response.headers_mut().insert(
                    CONTENT_DISPOSITION,
                    HeaderValue::from_static("attachment; filename=\"workspace.tar.gz\""),
                );
                response
            }
            Err(e) => host_error(e),
        },

        // ---- controls ----
        Action::AgentRun => {
            // The one precondition worth answering precisely: with no usable model
            // there is nothing to run, and that is `unavailable`, not a fault.
            let readiness = app.llm_readiness();
            if !readiness.ready {
                return error_response(
                    503,
                    "unavailable",
                    &host_core::state::readiness_error(&readiness),
                    Some("llm"),
                );
            }
            let user_input = match params.required("user_input") {
                Ok(value) => value,
                Err(response) => return *response,
            };
            let sink: Arc<dyn host_core::EventSink> =
                Arc::new(HttpEventSink::new(Arc::clone(hub), app.agent_id()));
            match app.run_agent(sink, user_input) {
                Ok(view) => ok_json(&view),
                Err(e) => host_error(e),
            }
        }
        Action::RunExport => {
            let run_id = match params.required("run_id") {
                Ok(value) => value,
                Err(response) => return *response,
            };
            let path = match params.required("path") {
                Ok(value) => value,
                Err(response) => return *response,
            };
            match app.get_run(run_id) {
                Ok(Some(_)) => {}
                Ok(None) => {
                    return error_response(
                        404,
                        "not_found",
                        &format!("no run with id {run_id:?}"),
                        Some("run_id"),
                    )
                }
                Err(e) => return host_error(e),
            }
            match app.export_run_audit(run_id, path.to_string()) {
                Ok(events_exported) => {
                    ok_json(&serde_json::json!({ "events_exported": events_exported }))
                }
                Err(e) => host_error(e),
            }
        }
        Action::RunsAbandonStale => match app.abandon_stale_runs() {
            Ok(abandoned) => ok_json(&serde_json::json!({ "abandoned": abandoned })),
            Err(e) => host_error(e),
        },
        // §6 G1: reserved. An explicit start has no lifetime semantics yet.
        Action::VmStart => error_response(
            501,
            "not_implemented",
            "starting a VM on its own is reserved: today the VM starts inside a run",
            Some("vm"),
        ),
        Action::VmStop => match app.stop_current_vm() {
            Ok(()) => no_content(),
            Err(e) => host_error(e),
        },
        Action::SnapshotSave => {
            let name = match params.required("name") {
                Ok(value) => value,
                Err(response) => return *response,
            };
            if !app.vm_is_running() {
                return error_response(
                    409,
                    "conflict",
                    "there is no VM to snapshot; run something first",
                    Some("vm"),
                );
            }
            match app.save_snapshot_real(name) {
                Ok(bytes_written) => {
                    ok_json(&serde_json::json!({ "bytes_written": bytes_written }))
                }
                Err(e) => state_clash(e),
            }
        }
        Action::SnapshotResume => {
            let name = match params.required("name") {
                Ok(value) => value,
                Err(response) => return *response,
            };
            let known = match app.list_snapshots() {
                Ok(snapshots) => snapshots.iter().any(|s| s.name == name),
                Err(e) => return host_error(e),
            };
            if !known {
                return error_response(
                    404,
                    "not_found",
                    &format!("no snapshot named {name:?}"),
                    Some("name"),
                );
            }
            match app.resume_from_snapshot_real(name) {
                Ok(()) => no_content(),
                Err(e) => state_clash(e),
            }
        }
        Action::SnapshotDelete => match params.required("name") {
            Ok(name) => match app.delete_snapshot(name) {
                Ok(deleted) => ok_json(&serde_json::json!({ "deleted": deleted })),
                Err(e) => host_error(e),
            },
            Err(response) => *response,
        },
        Action::SessionCreate => match params.required("title") {
            Ok(title) => match app.create_session(title) {
                Ok(session_id) => ok_json(&serde_json::json!({ "session_id": session_id })),
                Err(e) => host_error(e),
            },
            Err(response) => *response,
        },
        Action::SessionOpen => match params.required("session_id") {
            Ok(session_id) => match app.open_session(session_id) {
                Ok(view) => ok_json(&view),
                Err(e) => not_found_or_internal(e, "session_id", session_id),
            },
            Err(response) => *response,
        },
        Action::SessionRename => {
            let session_id = match params.required("session_id") {
                Ok(value) => value,
                Err(response) => return *response,
            };
            let title = match params.required("title") {
                Ok(value) => value,
                Err(response) => return *response,
            };
            // The host's rename is idempotent: an unknown id is a no-op, not an
            // error. The endpoint mirrors that rather than inventing a 404.
            match app.rename_session(session_id, title) {
                Ok(()) => no_content(),
                Err(e) => host_error(e),
            }
        }
        Action::SessionDelete => match params.required("session_id") {
            Ok(session_id) => match app.delete_session(session_id) {
                Ok(()) => no_content(),
                Err(e) => host_error(e),
            },
            Err(response) => *response,
        },
        Action::SessionClear => match app.clear_all_sessions() {
            Ok(()) => no_content(),
            Err(e) => host_error(e),
        },
        Action::ToolchainDownloadStart => {
            if app.toolchain_download_status().in_progress {
                return error_response(
                    409,
                    "conflict",
                    "a toolchain download is already running",
                    Some("download"),
                );
            }
            let spec = match host_core::toolchain_download::spec_for_current_platform() {
                Ok(spec) => spec,
                Err(e) => return host_error(HostError::Other(e.to_string())),
            };
            let cancel = match app.begin_toolchain_download(&spec) {
                Ok(cancel) => cancel,
                Err(e) => return host_error(e),
            };
            // The download is blocking, so it runs on its own thread and reports
            // progress through the stream, exactly as the Tauri command does.
            let state = hub.clone();
            let app = Arc::clone(app);
            std::thread::spawn(move || {
                let sink = HttpEventSink::new(Arc::clone(&state), app.agent_id());
                let dest_root = app.toolchain_dir();
                let mut on_event = |event: host_core::toolchain_download::DownloadEvent| {
                    app.record_download_event(event.clone());
                    sink.emit(
                        host_core::events::TOOLCHAIN_DOWNLOAD,
                        serde_json::to_value(&event).unwrap_or(serde_json::Value::Null),
                    );
                };
                if let Err(e) = app.download_toolchain_now(&spec, &dest_root, cancel, &mut on_event)
                {
                    log::line(
                        log_level,
                        LogLevel::Error,
                        format!("toolchain download failed: {e}"),
                    );
                }
            });
            accepted(&serde_json::json!({ "state": "started" }))
        }
        Action::ToolchainDownloadCancel => {
            if !app.toolchain_download_status().in_progress {
                return error_response(
                    409,
                    "conflict",
                    "no toolchain download is running",
                    Some("download"),
                );
            }
            match app.cancel_toolchain_download() {
                Ok(()) => accepted(&serde_json::json!({ "state": "cancelling" })),
                Err(e) => host_error(e),
            }
        }
        Action::QemuDownload => ok_json(&app.qemu_download_status()),
        Action::QemuDownloadStart => {
            // Today this refuses on every platform, and that is the recorded
            // decision (`docs/qemu-distribution.md` §5): RiscDom guides the user to
            // a QEMU they install themselves instead of fetching one, so no release
            // is pinned and `spec_for_current_platform` hands back the guidance. The
            // platform branch lives **here**, so pinning a release later is a data
            // change and the rest of this arm already works.
            let spec = match host_core::qemu_download::spec_for_current_platform() {
                Ok(spec) => spec,
                Err(e) => {
                    return error_response(503, "unavailable", &e.to_string(), Some("qemu"));
                }
            };
            if app.qemu_download_status().in_progress {
                return error_response(
                    409,
                    "conflict",
                    "a QEMU download is already running",
                    Some("download"),
                );
            }
            let cancel = match app.begin_qemu_download(&spec) {
                Ok(cancel) => cancel,
                Err(e) => return host_error(e),
            };
            // The download is blocking, so it runs on its own thread and reports
            // progress through the stream, exactly as the toolchain's does.
            let state = hub.clone();
            let app = Arc::clone(app);
            std::thread::spawn(move || {
                let sink = HttpEventSink::new(Arc::clone(&state), app.agent_id());
                let dest_root = app.qemu_dir();
                let mut on_event = |event: host_core::qemu_download::QemuDownloadEvent| {
                    app.record_qemu_download_event(event.clone());
                    sink.emit(
                        host_core::events::EV_QEMU_DOWNLOAD,
                        serde_json::to_value(&event).unwrap_or(serde_json::Value::Null),
                    );
                };
                // A failure reaches the caller through the audit event and the
                // status query; `log_level` covers the operator who asked for it.
                if let Err(e) = app.download_qemu_now(&spec, &dest_root, cancel, &mut on_event) {
                    log::line(
                        log_level,
                        LogLevel::Error,
                        format!("qemu download failed: {e}"),
                    );
                }
            });
            accepted(&serde_json::json!({ "state": "started" }))
        }
        Action::QemuDownloadCancel => {
            if !app.qemu_download_status().in_progress {
                return error_response(
                    409,
                    "conflict",
                    "no QEMU download is running",
                    Some("download"),
                );
            }
            match app.cancel_qemu_download() {
                Ok(()) => accepted(&serde_json::json!({ "state": "cancelling" })),
                Err(e) => host_error(e),
            }
        }
        // The two path setters: their only `Other` failure is "the path you gave
        // is not usable", which the API document calls `bad_request`.
        Action::ToolchainPath => match params.required("path") {
            Ok(path) => match app.set_toolchain_path(path) {
                Ok(()) => no_content(),
                Err(e) => unusable_input(e, "path"),
            },
            Err(response) => *response,
        },
        Action::ToolchainPathClear => match app.clear_toolchain_path() {
            Ok(()) => no_content(),
            Err(e) => host_error(e),
        },
        Action::QemuPath => match params.required("path") {
            Ok(path) => match app.set_qemu_path(path) {
                Ok(()) => no_content(),
                Err(e) => unusable_input(e, "path"),
            },
            Err(response) => *response,
        },
        Action::QemuPathClear => match app.clear_qemu_path() {
            Ok(()) => no_content(),
            Err(e) => host_error(e),
        },
        Action::PreflightRun => {
            // Compiles and boots a guest: never on the request thread.
            let emitter: Arc<dyn host_core::EventSink> =
                Arc::new(HttpEventSink::new(Arc::clone(hub), app.agent_id()));
            let app = Arc::clone(app);
            std::thread::spawn(move || {
                if let Err(e) = app.ensure_preflight(true, Some(emitter)) {
                    log::line(log_level, LogLevel::Error, format!("preflight failed: {e}"));
                }
            });
            accepted(&serde_json::json!({ "state": "running" }))
        }
        Action::PreflightAck => match app.acknowledge_preflight() {
            Ok(view) => ok_json(&view),
            Err(e) => host_error(e),
        },
        Action::AuditAlert => match params.bool_required("enabled") {
            Ok(enabled) => match app.set_alert_on_audit_failure(enabled) {
                Ok(()) => no_content(),
                Err(e) => host_error(e),
            },
            Err(response) => *response,
        },
        Action::AuditExport => match params.required("path") {
            Ok(path) => match app.export_audit_jsonl(path.to_string()) {
                Ok(events_exported) => {
                    ok_json(&serde_json::json!({ "events_exported": events_exported }))
                }
                Err(e) => host_error(e),
            },
            Err(response) => *response,
        },
        Action::SettingsThemeSet => match params.required("theme") {
            Ok(theme) if THEMES.contains(&theme.trim().to_lowercase().as_str()) => {
                match app.set_theme(theme) {
                    Ok(()) => no_content(),
                    Err(e) => host_error(e),
                }
            }
            Ok(_) => error_response(
                400,
                "bad_request",
                "theme must be one of \"light\", \"dark\", \"system\"",
                Some("theme"),
            ),
            Err(response) => *response,
        },
        Action::SettingsLanguageSet => match params.required("language") {
            Ok(language) if LANGUAGES.contains(&language.trim().to_lowercase().as_str()) => {
                match app.set_language(language) {
                    Ok(()) => no_content(),
                    Err(e) => host_error(e),
                }
            }
            Ok(_) => error_response(
                400,
                "bad_request",
                "language must be one of \"system\", \"en\", \"zh\"",
                Some("language"),
            ),
            Err(response) => *response,
        },
        Action::LlmConfigSet => {
            let api_key = match params.required("api_key") {
                Ok(value) => value,
                Err(response) => return *response,
            };
            let base_url = match params.required("base_url") {
                Ok(value) => value,
                Err(response) => return *response,
            };
            let model = match params.required("model") {
                Ok(value) => value,
                Err(response) => return *response,
            };
            let provider_id = params.get("provider_id").map(str::to_string);
            let remember = match params.bool_or("remember", false) {
                Ok(value) => Some(value),
                Err(response) => return *response,
            };
            match app.set_llm_config_with(
                provider_id,
                api_key.to_string(),
                base_url.to_string(),
                model.to_string(),
                remember,
            ) {
                Ok(()) => no_content(),
                Err(e) => unusable_input(e, "llm config"),
            }
        }
        Action::LlmStoredKeyLoad => match params.required("provider_id") {
            Ok(provider_id) => match app.load_stored_key(provider_id) {
                Ok(()) => no_content(),
                // The host answers with the reason as text; a key that is not
                // stored is a not-found, whatever the wording.
                Err(message) => error_response(
                    404,
                    "not_found",
                    &format!("no stored key for {provider_id:?}: {message}"),
                    Some("provider_id"),
                ),
            },
            Err(response) => *response,
        },
        Action::LlmConfigClear => {
            app.clear_llm_config();
            no_content()
        }
        Action::SerialExport => match params.required("path") {
            Ok(path) => match app.export_serial_log(path.to_string()) {
                Ok(bytes_written) => {
                    ok_json(&serde_json::json!({ "bytes_written": bytes_written }))
                }
                Err(e) => host_error(e),
            },
            Err(response) => *response,
        },
    }
}

/// A `201` with the created resource's body.
fn created_json(value: &serde_json::Value) -> Response<RespBody> {
    json_response(StatusCode::CREATED, value)
}

/// The capability a decision on a request needs, read from what it asks for
/// (v0.9 sandbox F2c, decision 1).
///
/// Switching is one power, assembling a definition another: an actor trusted to
/// move the node is not automatically trusted to give it a new definition to run.
fn capability_for_action(action: SandboxAction) -> Capability {
    match action {
        SandboxAction::Switch => Capability::SandboxSwitch,
        SandboxAction::Define | SandboxAction::Assemble => Capability::SandboxAssemble,
    }
}

/// A request's own refusals: an id nobody knows is a `404`; a second decision on
/// the same request is a `409`, because the first one stands.
fn sandbox_request_error(error: HostError) -> Response<RespBody> {
    match &error {
        HostError::SandboxRequestNotFound(id) => error_response(
            404,
            "not_found",
            &format!("no sandbox request {id:?}"),
            Some("id"),
        ),
        HostError::SandboxRequestDecided(_) => state_clash(error),
        _ => host_error(error),
    }
}

/// An import's own refusals (v0.9 project in/out).
///
/// An archive the host cannot accept is the **caller's input** being unusable, so
/// it is a `400` with `cause: "archive"` — the message names which entry and why.
/// A file that is already in the workspace is the other answer: `409`, with
/// `cause: "exists"`, because nothing is wrong with the archive and the caller can
/// decide (send `?force=true`). Anything else is the host's own failure.
fn workspace_io_error(error: HostError) -> Response<RespBody> {
    match &error {
        HostError::Archive(_) => {
            error_response(400, "bad_request", &error.user_message(), Some("archive"))
        }
        HostError::WorkspaceEntryExists(name) => error_response(
            409,
            "conflict",
            &format!("{name:?} is already in the workspace; send ?force=true to replace it"),
            Some("exists"),
        ),
        _ => host_error(error),
    }
}

/// A `200` with the serialised value.
fn ok_json<T: serde::Serialize>(value: &T) -> Response<RespBody> {
    match serde_json::to_value(value) {
        Ok(value) => json_response(StatusCode::OK, &value),
        Err(e) => error_response(
            500,
            "internal",
            &format!("the response could not be serialised: {e}"),
            None,
        ),
    }
}

/// A `202` with the acknowledgement body.
fn accepted(value: &serde_json::Value) -> Response<RespBody> {
    json_response(StatusCode::ACCEPTED, value)
}

/// A result from the host, or the error model's mapping of it.
fn result_json<T: serde::Serialize>(result: Result<T, HostError>) -> Response<RespBody> {
    match result {
        Ok(value) => ok_json(&value),
        Err(e) => host_error(e),
    }
}

/// Map a host error into the documented error model (§4).
fn host_error(error: HostError) -> Response<RespBody> {
    // A path the workspace policy refuses is the *caller's parameter* being
    // unusable — the boundary is what they asked to cross — so it is a `400`
    // naming the parameter. `403` is reserved for authentication and
    // authorisation: the capability check in `http.rs` and `auth.rs`'s hook.
    if matches!(error, HostError::Policy(_)) {
        return error_response(400, "bad_request", &error.user_message(), Some("path"));
    }
    let (status, code) = match &error {
        // "no LLM / no QEMU / no toolchain configured" is the documented 503.
        HostError::NotConfigured(_) => (503, "unavailable"),
        _ => (500, "internal"),
    };
    error_response(status, code, &error.user_message(), None)
}

/// An error whose only sensible reading is "the caller's input was unusable".
fn unusable_input(error: HostError, cause: &str) -> Response<RespBody> {
    error_response(400, "bad_request", &error.user_message(), Some(cause))
}

/// Something that is not there: the host has no typed not-found, so the command
/// whose argument is the identifier maps to `404` and everything else stays 500.
fn not_found_or_internal(error: HostError, cause: &str, value: &str) -> Response<RespBody> {
    match &error {
        HostError::Io(_) | HostError::Policy(_) => host_error(error),
        _ => error_response(
            404,
            "not_found",
            &format!("no {cause} {value:?}: {}", error.user_message()),
            Some(cause),
        ),
    }
}

/// A state clash: the VM or the snapshot is not in the state the action needs.
fn state_clash(error: HostError) -> Response<RespBody> {
    error_response(409, "conflict", &error.user_message(), None)
}

/// The status a failed switch answers with, read from the reason it carries.
///
/// A definition that is not there is the caller's parameter (`404`, the same answer
/// `/v0/sandboxes/{name}` gives), a definition that cannot run is the environment
/// (`503`, the same answer the unpinned QEMU download gives), and a VM that would
/// not start is the host failing (`500`). `cause` is the reason code, so a client
/// branches on a name rather than on a sentence.
fn sandbox_switch_error(error: HostError) -> Response<RespBody> {
    let message = error.user_message();
    let (status, code, cause) = match &error {
        HostError::SandboxNotFound(_) => (404, "not_found", "name"),
        HostError::SandboxQemuMissing(_) => (503, "unavailable", "sandbox_qemu_missing"),
        HostError::SandboxToolchainMissing(_) => (503, "unavailable", "sandbox_toolchain_missing"),
        HostError::SandboxKernelMissing(_) => (503, "unavailable", "sandbox_kernel_missing"),
        HostError::SandboxStart(_) => (500, "internal", "sandbox_start_failed"),
        // Anything else is the host's own shape (an IO error, a poisoned lock).
        _ => return host_error(error),
    };
    error_response(status, code, &message, Some(cause))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_route_declares_a_capability_the_owner_holds() {
        // A capability the enum has but `ALL` forgot would be a route the owner
        // could not reach: the check is what keeps the two in step.
        for (method, path, capability, _) in ROUTES {
            assert!(
                Capability::ALL.contains(capability),
                "{method} {path} declares {capability}"
            );
        }
        for (path, _, capability) in LOCAL_ROUTES {
            assert!(
                Capability::ALL.contains(capability),
                "{path} declares {capability}"
            );
        }
    }

    #[test]
    fn the_project_endpoints_declare_their_own_capability() {
        // Import writes into the workspace and is the only route needing
        // `workspace.write`; export reads it, like every other workspace route.
        for (path, capability, action) in [
            (
                "/v0/workspace/import",
                Capability::WorkspaceWrite,
                Action::WorkspaceImport,
            ),
            (
                "/v0/workspace/export",
                Capability::WorkspaceRead,
                Action::WorkspaceExport,
            ),
        ] {
            match resolve("POST", path) {
                Resolution::Query {
                    action: found,
                    capability: found_capability,
                    path_param,
                } => {
                    assert_eq!(found, action, "{path}");
                    assert_eq!(found_capability, capability, "{path}");
                    assert!(path_param.is_none(), "{path}");
                }
                other => panic!("POST {path}: {other:?}"),
            }
            // Both are writes, so a `GET` is a `405`.
            assert!(
                matches!(resolve("GET", path), Resolution::MethodNotAllowed { .. }),
                "GET {path}"
            );
        }
        // A workspace route that is not one of the three queries or these two.
        assert!(matches!(
            resolve("GET", "/v0/workspace/nope"),
            Resolution::NotFound
        ));
    }

    #[test]
    fn the_table_has_the_documented_endpoints() {
        let queries = ROUTES
            .iter()
            .filter(|(method, ..)| *method == "GET")
            .count();
        let controls = ROUTES
            .iter()
            .filter(|(method, ..)| *method == "POST")
            .count();
        // 31 queries (two of them patterns — `/v0/runs/{run_id}` and
        // `/v0/sandboxes/{name}` — so 29 rows), plus the reserved aggregate and
        // the QEMU download status (v0.9 F1) and the three sandbox queries
        // (v0.9 sandbox F2a-2) and the request queue (v0.9 sandbox F2c).
        //
        // The two request decisions (`approve` / `reject`) carry an id, so they
        // are pattern routes like `/v0/runs/{run_id}` and are **not** rows here —
        // the count is rows, and a pattern is not one. Project in/out (v0.9) added
        // no query: both of its endpoints write a body.
        assert_eq!(queries, 31, "query rows");
        // The 27 controls of §5.2, plus the reserved `POST /v0/vm/start` and
        // `POST /v0/runs/abandon-stale` (§6, G1 and G4), plus the QEMU download
        // start and cancel (v0.9 F1), the sandbox switch (v0.9 sandbox F2b-2), the
        // request queue's create (v0.9 sandbox F2c) and project in/out's two
        // (v0.9) — its two request decisions are pattern routes, as above.
        assert_eq!(controls, 35, "control rows");
    }

    #[test]
    fn resolve_matches_every_documented_path() {
        for (method, path, capability, _) in ROUTES {
            match resolve(method, path) {
                Resolution::Query {
                    capability: found,
                    path_param,
                    ..
                } => {
                    assert_eq!(found, *capability, "{path}");
                    assert!(path_param.is_none(), "{path}");
                }
                other => panic!("{path} did not resolve to a query: {other:?}"),
            }
        }
    }

    #[test]
    fn resolve_runs_by_id_and_rejects_a_nested_path() {
        match resolve("GET", "/v0/runs/run-1-7") {
            Resolution::Query {
                action, path_param, ..
            } => {
                assert_eq!(action, Action::Run);
                assert_eq!(path_param, Some(("run_id", "run-1-7".to_string())));
            }
            other => panic!("got {other:?}"),
        }
        // The literal sub-paths are their own endpoints, not runs named that.
        // The literal sub-paths are their own endpoints, not runs named that.
        match resolve("GET", "/v0/runs/diff") {
            Resolution::Query {
                action: Action::RunDiff,
                ..
            } => {}
            other => panic!("GET /v0/runs/diff: {other:?}"),
        }
        for path in ["/v0/runs/export", "/v0/runs/abandon-stale"] {
            match resolve("POST", path) {
                Resolution::Query { .. } => {}
                other => panic!("POST {path}: {other:?}"),
            }
            match resolve("GET", path) {
                Resolution::MethodNotAllowed { allowed } => assert_eq!(allowed, "POST", "{path}"),
                other => panic!("GET {path}: {other:?}"),
            }
        }
        assert!(matches!(
            resolve("GET", "/v0/runs/a/b"),
            Resolution::NotFound
        ));
    }

    #[test]
    fn resolve_a_sandbox_by_name_and_reject_a_literal_sub_path() {
        match resolve("GET", "/v0/sandboxes/blink") {
            Resolution::Query {
                action, path_param, ..
            } => {
                assert_eq!(action, Action::Sandbox);
                assert_eq!(path_param, Some(("name", "blink".to_string())));
            }
            other => panic!("got {other:?}"),
        }
        // The literal sub-paths are their own endpoints, not sandboxes named that.
        for (path, action) in [
            ("/v0/sandboxes", Action::Sandboxes),
            ("/v0/sandboxes/current", Action::SandboxCurrent),
            ("/v0/sandboxes/candidates", Action::SandboxCandidates),
        ] {
            match resolve("GET", path) {
                Resolution::Query {
                    action: found,
                    path_param,
                    ..
                } => {
                    assert_eq!(found, action, "{path}");
                    assert!(path_param.is_none(), "{path}");
                }
                other => panic!("GET {path}: {other:?}"),
            }
        }
        // `switch` is a route of its own since F2b-2 (`POST`-only, so a `GET` is a
        // `405`), `requests` since F2c (served under both methods), while
        // `assemble` is still nothing — and none of them is a definition.
        match resolve("GET", "/v0/sandboxes/switch") {
            Resolution::MethodNotAllowed { allowed } => assert_eq!(allowed, "POST"),
            other => panic!("GET /v0/sandboxes/switch: {other:?}"),
        }
        match resolve("POST", "/v0/sandboxes/switch") {
            Resolution::Query {
                action: Action::SandboxSwitch,
                path_param,
                capability,
            } => {
                assert_eq!(capability, Capability::SandboxSwitch);
                assert!(path_param.is_none());
            }
            other => panic!("POST /v0/sandboxes/switch: {other:?}"),
        }
        assert!(
            matches!(
                resolve("GET", "/v0/sandboxes/assemble"),
                Resolution::NotFound
            ),
            "the assemble endpoint is not built yet (F2c decision 4 keeps it out)"
        );
        match resolve("GET", "/v0/sandboxes/requests") {
            Resolution::Query {
                action: Action::SandboxRequests,
                capability,
                path_param,
            } => {
                assert_eq!(capability, Capability::SandboxRead);
                assert!(path_param.is_none());
            }
            other => panic!("GET /v0/sandboxes/requests: {other:?}"),
        }
        match resolve("POST", "/v0/sandboxes/requests") {
            Resolution::Query {
                action: Action::SandboxRequestCreate,
                capability,
                ..
            } => {
                // An ask is what `agent.run` may leave behind: the capability the
                // *decision* needs is the handler's business.
                assert_eq!(capability, Capability::AgentRun);
            }
            other => panic!("POST /v0/sandboxes/requests: {other:?}"),
        }
        // The two decisions carry the id, and answer with the gate's capability:
        // which one the decision *needs* follows from the request's action, so it
        // is checked in the handler (F2c decision 1).
        for (path, wanted) in [
            (
                "/v0/sandboxes/requests/req-1-1/approve",
                Action::SandboxRequestApprove,
            ),
            (
                "/v0/sandboxes/requests/req-1-1/reject",
                Action::SandboxRequestReject,
            ),
        ] {
            match resolve("POST", path) {
                Resolution::Query {
                    action,
                    capability,
                    path_param,
                } => {
                    assert_eq!(action, wanted, "{path}");
                    assert_eq!(capability, Capability::SandboxRead, "{path}");
                    assert_eq!(
                        path_param,
                        Some(("request_id", "req-1-1".to_string())),
                        "{path}"
                    );
                }
                other => panic!("POST {path}: {other:?}"),
            }
            assert!(
                matches!(resolve("GET", path), Resolution::MethodNotAllowed { .. }),
                "GET {path}"
            );
        }
        // Anything else under that path is not a route at all.
        for path in [
            "/v0/sandboxes/requests/req-1-1",
            "/v0/sandboxes/requests/req-1-1/delete",
            "/v0/sandboxes/requests/a/b/approve",
        ] {
            assert!(
                matches!(resolve("POST", path), Resolution::NotFound),
                "{path}"
            );
        }
        // A name never spans a slash, and the route serves `GET` only.
        assert!(matches!(
            resolve("GET", "/v0/sandboxes/a/b"),
            Resolution::NotFound
        ));
        match resolve("POST", "/v0/sandboxes/blink") {
            Resolution::MethodNotAllowed { allowed } => assert_eq!(allowed, "GET"),
            other => panic!("POST: {other:?}"),
        }
    }

    #[test]
    fn a_path_served_under_two_methods_resolves_to_each_of_them() {
        // `/v0/toolchain/download` is a `GET` status query and a `POST` start.
        match resolve("GET", "/v0/toolchain/download") {
            Resolution::Query {
                action: Action::ToolchainDownload,
                ..
            } => {}
            other => panic!("GET: {other:?}"),
        }
        match resolve("POST", "/v0/toolchain/download") {
            Resolution::Query {
                action: Action::ToolchainDownloadStart,
                ..
            } => {}
            other => panic!("POST: {other:?}"),
        }
        // And anything else names both, rather than pretending there is one.
        match resolve("DELETE", "/v0/toolchain/download") {
            Resolution::MethodNotAllowed { allowed } => assert_eq!(allowed, "GET, POST"),
            other => panic!("DELETE: {other:?}"),
        }

        // The QEMU download is the same shape for the other resource.
        match resolve("GET", "/v0/qemu/download") {
            Resolution::Query {
                action: Action::QemuDownload,
                ..
            } => {}
            other => panic!("GET: {other:?}"),
        }
        match resolve("POST", "/v0/qemu/download") {
            Resolution::Query {
                action: Action::QemuDownloadStart,
                ..
            } => {}
            other => panic!("POST: {other:?}"),
        }
        match resolve("POST", "/v0/qemu/download/cancel") {
            Resolution::Query {
                action: Action::QemuDownloadCancel,
                ..
            } => {}
            other => panic!("POST cancel: {other:?}"),
        }
        match resolve("DELETE", "/v0/qemu/download") {
            Resolution::MethodNotAllowed { allowed } => assert_eq!(allowed, "GET, POST"),
            other => panic!("DELETE: {other:?}"),
        }
    }

    #[test]
    fn a_known_path_under_the_wrong_method_is_405() {
        for (method, path, allowed) in [
            ("POST", "/v0/health", "GET"),
            ("POST", "/v0/snapshots", "GET"),
            ("DELETE", "/v0/events", "GET"),
            ("POST", "/v0/runs/run-1-7", "GET"),
            ("GET", "/v0/snapshots/save", "POST"),
            ("GET", "/v0/sessions/create", "POST"),
            ("DELETE", "/v0/llm/config", "GET, POST"),
        ] {
            match resolve(method, path) {
                Resolution::MethodNotAllowed { allowed: found } => {
                    assert_eq!(found, allowed, "{method} {path}")
                }
                other => panic!("{method} {path}: {other:?}"),
            }
        }
    }

    #[test]
    fn an_unknown_path_is_not_found() {
        assert!(matches!(resolve("GET", "/v0/nope"), Resolution::NotFound));
        assert!(matches!(
            resolve("GET", "/other/health"),
            Resolution::NotFound
        ));
        assert!(matches!(
            resolve("POST", "/v0/snapshots/save/x"),
            Resolution::NotFound
        ));
    }

    #[test]
    fn the_query_string_is_percent_decoded() {
        let params = parse_query(Some("path=src%2Fmain.c&limit=10&actor=a+b"));
        assert_eq!(params.get("path"), Some("src/main.c"));
        assert_eq!(params.get("limit"), Some("10"));
        assert_eq!(params.get("actor"), Some("a b"));
    }

    #[test]
    fn a_json_body_becomes_scalar_parameters() {
        let params = Params::from_json(&serde_json::json!({
            "name": "after-blink",
            "enabled": true,
            "remember": false,
            "limit": 7,
            "nested": { "no": "not addressable" },
        }));
        assert_eq!(params.get("name"), Some("after-blink"));
        assert!(params.bool_required("enabled").expect("enabled"));
        assert!(!params.bool_or("remember", true).expect("remember"));
        assert_eq!(params.usize_required("limit").expect("limit"), 7);
        assert_eq!(params.get("nested"), None);
    }

    #[test]
    fn a_missing_or_unparsable_parameter_is_a_400() {
        let params = parse_query(Some("limit=nine"));
        let response = params.usize_required("limit").expect_err("not a number");
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let params = parse_query(None);
        assert!(params.usize_required("limit").is_err());
        assert_eq!(params.usize_or("limit", 20).expect("default"), 20);
        assert!(params.bool_required("enabled").is_err());
        assert!(params.bool_or("enabled", true).expect("default"));
    }
}
