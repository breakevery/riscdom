//! The query surface: the 26 `GET` endpoints of `docs/control-plane-api.md` §5.1,
//! the reserved `/v0/resources`, the route table that names them, and the
//! dispatcher that calls `AppState` and serialises the result.
//!
//! Control endpoints (`POST`, §5.2) are a later batch, as are `gap` frames and
//! `Last-Event-ID` replay.

use crate::http::{error_response, json_response, RespBody};
use host::{AppState, HostError};
use hyper::{Response, StatusCode};
use std::collections::HashMap;

/// One query endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Action {
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
    /// The reserved aggregate (`docs/control-plane-api.md` §6, G3): answers 501.
    Resources,
}

/// What a request resolves to.
#[derive(Debug)]
pub(crate) enum Resolution {
    Query {
        action: Action,
        capability: &'static str,
        params: Params,
    },
    /// An endpoint the server answers itself (it needs more than `AppState`).
    Local {
        kind: Local,
        capability: &'static str,
    },
    /// The path is served, the method is not.
    MethodNotAllowed { allowed: &'static str },
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
/// intermediary's job, which is a later batch, so `NoAuth` ignores it and every
/// query is allowed in v0.9.
const ROUTES: &[(&str, &str, &str, Action)] = &[
    ("GET", "/v0/audit/status", "audit.read", Action::AuditStatus),
    ("GET", "/v0/audit/events", "audit.read", Action::AuditEvents),
    ("GET", "/v0/runs", "runs.read", Action::Runs),
    ("GET", "/v0/runs/diff", "runs.read", Action::RunDiff),
    (
        "GET",
        "/v0/llm/provider-presets",
        "llm.read",
        Action::LlmProviderPresets,
    ),
    ("GET", "/v0/llm/config", "llm.read", Action::LlmConfig),
    ("GET", "/v0/llm/readiness", "llm.read", Action::LlmReadiness),
    (
        "GET",
        "/v0/llm/local-probe",
        "llm.read",
        Action::LlmLocalProbe,
    ),
    (
        "GET",
        "/v0/llm/stored-key",
        "llm.read",
        Action::LlmStoredKey,
    ),
    ("GET", "/v0/sessions", "session.read", Action::Sessions),
    (
        "GET",
        "/v0/sessions/current",
        "session.read",
        Action::SessionCurrent,
    ),
    ("GET", "/v0/snapshots", "snapshot.read", Action::Snapshots),
    ("GET", "/v0/vm/running", "vm.read", Action::VmRunning),
    ("GET", "/v0/vm/status", "vm.read", Action::VmStatus),
    ("GET", "/v0/toolchain", "toolchain.read", Action::Toolchain),
    (
        "GET",
        "/v0/toolchain/download",
        "toolchain.read",
        Action::ToolchainDownload,
    ),
    ("GET", "/v0/qemu", "qemu.read", Action::Qemu),
    ("GET", "/v0/qemu/status", "qemu.read", Action::QemuStatus),
    ("GET", "/v0/preflight", "preflight.read", Action::Preflight),
    (
        "GET",
        "/v0/settings/theme",
        "settings.read",
        Action::SettingsTheme,
    ),
    (
        "GET",
        "/v0/settings/language",
        "settings.read",
        Action::SettingsLanguage,
    ),
    (
        "GET",
        "/v0/workspace/root",
        "workspace.read",
        Action::WorkspaceRoot,
    ),
    (
        "GET",
        "/v0/workspace/files",
        "workspace.read",
        Action::WorkspaceFiles,
    ),
    (
        "GET",
        "/v0/workspace/file",
        "workspace.read",
        Action::WorkspaceFile,
    ),
    ("GET", "/v0/serial", "serial.read", Action::Serial),
    // Reserved (§6, G3): served, and answers 501 until the aggregate lands.
    ("GET", "/v0/resources", "vm.read", Action::Resources),
];

/// `/v0/runs/{run_id}` is the one path with a parameter.
const RUN_PREFIX: &str = "/v0/runs/";

/// The endpoints this crate added to the settled surface (see the API document's
/// "host-local endpoints"): a liveness check, a summary, and the event stream.
const LOCAL_ROUTES: &[(&str, Local, &str)] = &[
    ("/v0/health", Local::Health, "health.read"),
    ("/v0/status", Local::Status, "status.read"),
    ("/v0/events", Local::Events, "events.subscribe"),
];

/// Query-string parameters, percent-decoded.
#[derive(Debug)]
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

    /// Read a required parameter, or the `400` to answer with.
    pub fn required(&self, name: &str) -> Result<&str, Response<RespBody>> {
        self.get(name).ok_or_else(|| {
            error_response(
                400,
                "bad_request",
                &format!("missing required parameter {name:?}"),
                Some(name),
            )
        })
    }

    /// Read a required non-negative integer, or the `400` to answer with.
    pub fn usize_required(&self, name: &str) -> Result<usize, Response<RespBody>> {
        let raw = self.required(name)?;
        raw.parse().map_err(|e| {
            error_response(
                400,
                "bad_request",
                &format!("parameter {name:?} is not a number: {e}"),
                Some(name),
            )
        })
    }

    /// Read an optional non-negative integer, falling back to `default`.
    pub fn usize_or(&self, name: &str, default: usize) -> Result<usize, Response<RespBody>> {
        match self.get(name) {
            Some(raw) => raw.parse().map_err(|e| {
                error_response(
                    400,
                    "bad_request",
                    &format!("parameter {name:?} is not a number: {e}"),
                    Some(name),
                )
            }),
            None => Ok(default),
        }
    }
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
    Some(rest)
}

/// Resolve a request to its route.
pub(crate) fn resolve(method: &str, path: &str, query: Option<&str>) -> Resolution {
    for (local_path, kind, capability) in LOCAL_ROUTES {
        if *local_path == path {
            return if method == "GET" {
                Resolution::Local {
                    kind: *kind,
                    capability,
                }
            } else {
                Resolution::MethodNotAllowed { allowed: "GET" }
            };
        }
    }
    for (route_method, route_path, capability, action) in ROUTES {
        if *route_path == path {
            return if *route_method == method {
                Resolution::Query {
                    action: *action,
                    capability,
                    params: parse_query(query),
                }
            } else {
                Resolution::MethodNotAllowed {
                    allowed: route_method,
                }
            };
        }
    }
    if let Some(run_id) = run_id_from(path) {
        if method != "GET" {
            return Resolution::MethodNotAllowed { allowed: "GET" };
        }
        let mut params = parse_query(query);
        // The path segment is a parameter like any other, so handlers read it the
        // same way they read the query string.
        params
            .values
            .insert("run_id".to_string(), run_id.to_string());
        return Resolution::Query {
            action: Action::Run,
            capability: "runs.read",
            params,
        };
    }
    Resolution::NotFound
}

/// Run one query against the host and shape the answer.
pub(crate) fn dispatch(action: Action, params: &Params, app: &AppState) -> Response<RespBody> {
    match action {
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
                Err(response) => return response,
            };
            let actor = params.get("actor").map(str::to_string);
            let action_prefix = params.get("action_prefix").map(str::to_string);
            result_json(app.list_events(limit, actor, action_prefix))
        }
        Action::Runs => match params.usize_or("limit", 20) {
            Ok(limit) => result_json(app.list_runs(limit)),
            Err(response) => response,
        },
        Action::Run => match params.required("run_id") {
            Ok(run_id) => result_json(app.get_run(run_id)),
            Err(response) => response,
        },
        Action::RunDiff => {
            let run_a = match params.required("run_a") {
                Ok(v) => v,
                Err(response) => return response,
            };
            let run_b = match params.required("run_b") {
                Ok(v) => v,
                Err(response) => return response,
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
            Err(response) => response,
        },
        Action::Sessions => match params.usize_required("limit") {
            Ok(limit) => result_json(app.list_sessions(limit)),
            Err(response) => response,
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
            Err(response) => response,
        },
        Action::Serial => ok_json(&serde_json::json!({ "buffer": app.serial_buffer() })),
        // §6 G3: the shape is settled, the aggregate is not built.
        Action::Resources => error_response(
            501,
            "not_implemented",
            "resource accounting is reserved and not implemented yet (v0.9 batch 4)",
            Some("resources"),
        ),
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

/// A result from the host, or the error model's mapping of it.
fn result_json<T: serde::Serialize>(result: Result<T, HostError>) -> Response<RespBody> {
    match result {
        Ok(value) => ok_json(&value),
        Err(e) => host_error(e),
    }
}

/// Map a host error into the documented error model (§4).
fn host_error(error: HostError) -> Response<RespBody> {
    let (status, code) = match &error {
        // "no LLM / no QEMU / no toolchain configured" is the documented 503.
        HostError::NotConfigured(_) => (503, "unavailable"),
        // A workspace read outside the policy is a refusal, not a failure.
        HostError::Policy(_) => (403, "forbidden"),
        _ => (500, "internal"),
    };
    error_response(status, code, &error.user_message(), None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_has_the_twenty_six_queries_plus_the_reserved_one() {
        let queries = ROUTES
            .iter()
            .filter(|(_, _, _, action)| *action != Action::Resources)
            .count();
        // 26 queries, one of which (`/v0/runs/{run_id}`) is a pattern, so 25 rows.
        assert_eq!(queries, 25, "table rows for the 26 queries");
        assert_eq!(ROUTES.len(), 26, "plus the reserved /v0/resources");
    }

    #[test]
    fn resolve_matches_every_documented_path() {
        for (method, path, capability, _) in ROUTES {
            match resolve(method, path, None) {
                Resolution::Query {
                    capability: found, ..
                } => assert_eq!(found, *capability, "{path}"),
                other => panic!("{path} did not resolve to a query: {other:?}"),
            }
        }
    }

    #[test]
    fn resolve_runs_by_id_and_rejects_a_nested_path() {
        match resolve("GET", "/v0/runs/run-1-7", None) {
            Resolution::Query { action, params, .. } => {
                assert_eq!(action, Action::Run);
                assert_eq!(params.get("run_id"), Some("run-1-7"));
            }
            other => panic!("got {other:?}"),
        }
        // `/v0/runs/diff` is its own endpoint, not a run called "diff".
        match resolve("GET", "/v0/runs/diff", None) {
            Resolution::Query { action, .. } => assert_eq!(action, Action::RunDiff),
            other => panic!("got {other:?}"),
        }
        assert!(matches!(
            resolve("GET", "/v0/runs/a/b", None),
            Resolution::NotFound
        ));
    }

    #[test]
    fn a_known_path_under_the_wrong_method_is_405() {
        match resolve("POST", "/v0/health", None) {
            Resolution::MethodNotAllowed { allowed } => assert_eq!(allowed, "GET"),
            other => panic!("got {other:?}"),
        }
        match resolve("POST", "/v0/snapshots", None) {
            Resolution::MethodNotAllowed { allowed } => assert_eq!(allowed, "GET"),
            other => panic!("got {other:?}"),
        }
        match resolve("DELETE", "/v0/events", None) {
            Resolution::MethodNotAllowed { allowed } => assert_eq!(allowed, "GET"),
            other => panic!("got {other:?}"),
        }
        match resolve("POST", "/v0/runs/run-1-7", None) {
            Resolution::MethodNotAllowed { allowed } => assert_eq!(allowed, "GET"),
            other => panic!("got {other:?}"),
        }
    }

    #[test]
    fn the_host_local_endpoints_resolve() {
        for (path, kind) in [
            ("/v0/health", Local::Health),
            ("/v0/status", Local::Status),
            ("/v0/events", Local::Events),
        ] {
            match resolve("GET", path, None) {
                Resolution::Local { kind: found, .. } => assert_eq!(found, kind, "{path}"),
                other => panic!("{path} did not resolve locally: {other:?}"),
            }
        }
    }

    #[test]
    fn an_unknown_path_is_not_found() {
        assert!(matches!(
            resolve("GET", "/v0/nope", None),
            Resolution::NotFound
        ));
        assert!(matches!(
            resolve("GET", "/other/health", None),
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
    fn a_missing_or_unparsable_number_is_a_400() {
        let params = parse_query(Some("limit=nine"));
        let response = params.usize_required("limit").expect_err("not a number");
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let params = parse_query(None);
        assert!(params.usize_required("limit").is_err());
        assert_eq!(params.usize_or("limit", 20).expect("default"), 20);
    }
}
