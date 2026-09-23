//! The human mode: tables and short key/value lines.
//!
//! `--json` passes the control plane's JSON through untouched; this module is what
//! the same answer looks like to a person. Everything is read out of the parsed
//! JSON by field name, and a missing field prints `-` rather than disappearing:
//! the shape belongs to the API document, not to this file.

use crate::args::Command;
use crate::client::Reply;
use crate::sse::Frame;
use serde_json::Value;

/// Render one answer for a human.
pub fn human(command: &Command, reply: &Reply) -> String {
    if reply.is_empty() {
        // `204 No Content`: the control plane said "done" and nothing else.
        return "ok".to_string();
    }
    let Some(value) = &reply.json else {
        // Not JSON: hand back exactly what the control plane said.
        return reply.body.trim().to_string();
    };
    match command {
        Command::Health => health(value),
        Command::Status => status(value),
        Command::Agents => agents(value),
        Command::RunsList { .. } => runs(value),
        Command::RunsGet { .. } => one_run(value),
        Command::AuditStatus => audit_status(value),
        Command::AuditEvents { .. } => events(value),
        Command::SnapshotsList => snapshots(value),
        Command::SandboxesList => sandboxes(value),
        Command::SandboxesCurrent => sandbox_current(value),
        Command::SandboxesCandidates => sandbox_candidates(value),
        Command::SandboxesShow { .. } => sandbox_detail(value),
        Command::SandboxesSwitch { .. } => sandbox_switched(value),
        // The queue (v0.9 sandbox F2c): a list, and the record a decision left.
        Command::SandboxesRequests { .. } => sandbox_requests(value),
        Command::SandboxesRequestsApprove { .. } | Command::SandboxesRequestsReject { .. } => {
            sandbox_request_decided(value)
        }
        Command::Run { .. } => outcome(value),
        Command::VmStop | Command::VmStart => "ok".to_string(),
        Command::SnapshotsSave { .. } => written(value),
        Command::SnapshotsResume { .. } => "ok".to_string(),
        Command::SnapshotsDelete { .. } => deleted(value),
        Command::SessionsCreate { .. } => session_created(value),
        Command::SessionsOpen { .. } => session_detail(value),
        Command::SessionsDelete { .. }
        | Command::SessionsRename { .. }
        | Command::SessionsClearAll => "ok".to_string(),
        Command::RunsAbandonStale => abandoned(value),
        Command::ExportAuditJsonl { .. }
        | Command::ExportRunAudit { .. }
        | Command::ExportSerialLog { .. } => export_written(command, value),
        Command::ToolchainDownload | Command::ToolchainCancel | Command::PreflightRun => {
            acknowledged(command, value)
        }
        Command::QemuDownload | Command::QemuCancel => acknowledged(command, value),
        Command::QemuStatus => download_status(value),
        Command::PreflightAck => preflight_view(value),
        // Every remaining configuration endpoint answers `204`: nothing to say,
        // which the empty-body branch above already turned into `ok`.
        Command::LlmSet { .. }
        | Command::LlmClear
        | Command::LlmLoadKey { .. }
        | Command::QemuPath { .. }
        | Command::QemuClear
        | Command::ToolchainPath { .. }
        | Command::ToolchainClear
        | Command::AuditAlertSet { .. }
        | Command::ThemeSet { .. }
        | Command::LanguageSet { .. } => "ok".to_string(),
    }
}

/// What an export wrote, and where.
///
/// The two audit exports answer with the number of **events** they wrote — the
/// field says so, `events_exported` — while the serial export answers with a
/// `bytes_written` byte count. The CLI prints what the number is.
fn export_written(command: &Command, value: &Value) -> String {
    let Some(path) = command.output_path() else {
        return format!("wrote {}", number(value, "bytes_written"));
    };
    match command {
        Command::ExportSerialLog { .. } => {
            format!("wrote {} bytes to {path}", number(value, "bytes_written"))
        }
        _ => {
            let count = number(value, "events_exported");
            let unit = if count == "1" { "event" } else { "events" };
            format!("exported {count} {unit} to {path}")
        }
    }
}

/// The `202` acknowledgement: work has started, and this is not its result.
fn acknowledged(command: &Command, value: &Value) -> String {
    let what = match command {
        Command::ToolchainDownload | Command::ToolchainCancel => "download",
        Command::QemuDownload | Command::QemuCancel => "qemu download",
        Command::PreflightRun => "preflight",
        _ => "request",
    };
    match value.get("state").and_then(Value::as_str) {
        Some(state) => format!("{what} {state}"),
        None => "ok".to_string(),
    }
}

/// `{ "in_progress": bool, "last_event": … }` — a download's status query.
fn download_status(value: &Value) -> String {
    let last = value
        .get("last_event")
        .filter(|event| !event.is_null())
        .and_then(|event| event.get("state"))
        .and_then(Value::as_str)
        .unwrap_or("-")
        .to_string();
    format!(
        "in_progress {}\nlast_event  {last}",
        text(value, "in_progress")
    )
}

/// `PreflightView`: the steps the host checked, and what it found.
fn preflight_view(value: &Value) -> String {
    let mut lines = vec![
        format!("checked  {}", text(value, "checked")),
        format!("ok       {}", text(value, "ok")),
    ];
    if let Some(rows) = value.get("rows").and_then(Value::as_array) {
        for row in rows {
            lines.push(format!("{:<14} {}", text(row, "step"), text(row, "state")));
        }
    }
    for key in ["failed_step", "detail", "suggestion"] {
        if let Some(line) = value.get(key).and_then(Value::as_str) {
            lines.push(format!("{key:<11} {line}"));
        }
    }
    lines.join("\n")
}

/// One line for a stream frame (`--follow`).
///
/// The event name, then the payload as compact JSON — long enough to read, short
/// enough to keep a run's output scannable.
pub fn frame_line(frame: &Frame) -> String {
    let Some(value) = frame.json() else {
        return frame.data.clone();
    };
    let kind = value.get("kind").and_then(Value::as_str).unwrap_or("frame");
    match value.get("event").and_then(Value::as_str) {
        Some(event) => {
            let payload = value
                .get("payload")
                .map(|payload| payload.to_string())
                .unwrap_or_default();
            format!("{event} {}", truncate(&payload, 160))
        }
        // `hello` and `gap` carry no event name; name the kind instead.
        None => match serde_json::to_string(&value).ok() {
            Some(text) => format!("{kind} {}", truncate(&text, 160)),
            None => kind.to_string(),
        },
    }
}

fn truncate(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_string();
    }
    let kept: String = text.chars().take(limit).collect();
    format!("{kept}…")
}

/// `AgentOutcomeView`: `{ kind, content, reason, iterations }`.
fn outcome(value: &Value) -> String {
    let mut lines = vec![
        format!("kind       {}", text(value, "kind")),
        format!("iterations {}", number(value, "iterations")),
    ];
    if value.get("reason").map(|r| !r.is_null()).unwrap_or(false) {
        lines.push(format!("reason     {}", text(value, "reason")));
    }
    if let Some(content) = value.get("content").and_then(Value::as_str) {
        lines.push(String::new());
        lines.push(content.to_string());
    }
    lines.join("\n")
}

/// `{ "bytes_written": n }` — the two write endpoints answer this.
fn written(value: &Value) -> String {
    format!("wrote {} bytes", number(value, "bytes_written"))
}

/// `{ "deleted": bool }`.
fn deleted(value: &Value) -> String {
    match value.get("deleted").and_then(Value::as_bool) {
        Some(true) => "deleted".to_string(),
        Some(false) => "nothing to delete".to_string(),
        None => "ok".to_string(),
    }
}

/// `{ "session_id": … }`.
fn session_created(value: &Value) -> String {
    format!("session_id {}", text(value, "session_id"))
}

/// `{ "abandoned": [run_id, …] }`.
fn abandoned(value: &Value) -> String {
    match value.get("abandoned").and_then(Value::as_array) {
        Some(ids) if ids.is_empty() => "no stale runs".to_string(),
        Some(ids) => format!("abandoned {}", ids.len()),
        None => "ok".to_string(),
    }
}

/// `SessionDetailView`: `{ meta, messages }`.
fn session_detail(value: &Value) -> String {
    let meta = value.get("meta").cloned().unwrap_or(Value::Null);
    let messages = value
        .get("messages")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    [
        format!("session_id {}", text(&meta, "session_id")),
        format!("title      {}", text(&meta, "title")),
        format!("messages   {messages}"),
    ]
    .join("\n")
}

fn health(value: &Value) -> String {
    format!(
        "{}  version {}  uptime {} ms",
        text(value, "status"),
        text(value, "version"),
        number(value, "uptime_ms"),
    )
}

fn status(value: &Value) -> String {
    let mut lines = vec![
        format!("status          {}", text(value, "status")),
        format!("version         {}", text(value, "version")),
        format!("uptime_ms       {}", number(value, "uptime_ms")),
        format!("connections     {}", number(value, "connections")),
        format!("sse_subscribers {}", number(value, "sse_subscribers")),
        format!("agents          {}", number(value, "agents")),
        format!("agent_id        {}", text(value, "agent_id")),
    ];
    lines.dedup();
    lines.join("\n")
}

/// `/v0/status` is the only endpoint that knows these two; the view is derived.
fn agents(value: &Value) -> String {
    format!(
        "agents {}  agent_id {}",
        number(value, "agents"),
        text(value, "agent_id")
    )
}

fn runs(value: &Value) -> String {
    let Some(rows) = value.as_array() else {
        return value.to_string();
    };
    if rows.is_empty() {
        return "no runs".to_string();
    }
    let mut lines = vec![format!(
        "{:<28} {:<10} {:>13} {:>13}",
        "RUN_ID", "STATUS", "STARTED_MS", "ENDED_MS"
    )];
    for row in rows {
        lines.push(format!(
            "{:<28} {:<10} {:>13} {:>13}",
            text(row, "run_id"),
            text(row, "status"),
            number(row, "started_at_ms"),
            optional_number(row, "ended_at_ms"),
        ));
    }
    lines.join("\n")
}

fn one_run(value: &Value) -> String {
    [
        ("run_id", text(value, "run_id")),
        ("status", text(value, "status")),
        ("started_at_ms", number(value, "started_at_ms")),
        ("ended_at_ms", optional_number(value, "ended_at_ms")),
        ("fingerprint", text(value, "fingerprint")),
        ("fingerprint_short", text(value, "fingerprint_short")),
        ("parent_run_id", optional_text(value, "parent_run_id")),
        ("session_id", optional_text(value, "session_id")),
        (
            "resumed_from_snapshot",
            optional_text(value, "resumed_from_snapshot"),
        ),
    ]
    .iter()
    .map(|(key, value)| format!("{key:<20} {value}"))
    .collect::<Vec<_>>()
    .join("\n")
}

fn audit_status(value: &Value) -> String {
    let chain = value.get("chain").cloned().unwrap_or(Value::Null);
    let chain_verdict = match chain.get("status").and_then(Value::as_str) {
        Some("Broken") => format!(
            "Broken at {} ({})",
            number(&chain, "at_id"),
            text(&chain, "reason")
        ),
        _ => format!("Intact length {}", number(&chain, "length")),
    };
    let failures = value
        .get("failures")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    [
        format!("count            {}", number(value, "count")),
        format!("chain            {chain_verdict}"),
        format!("alert_on_failure {}", text(value, "alert_on_failure")),
        format!("failures         {failures}"),
    ]
    .join("\n")
}

fn events(value: &Value) -> String {
    let Some(rows) = value.as_array() else {
        return value.to_string();
    };
    if rows.is_empty() {
        return "no events".to_string();
    }
    let mut lines = vec![format!(
        "{:>6} {:>13} {:<9} {}",
        "ID", "TIMESTAMP_MS", "ACTOR", "ACTION"
    )];
    for row in rows {
        lines.push(format!(
            "{:>6} {:>13} {:<9} {}",
            number(row, "id"),
            number(row, "timestamp_ms"),
            text(row, "actor"),
            text(row, "action"),
        ));
    }
    lines.join("\n")
}

fn snapshots(value: &Value) -> String {
    let Some(rows) = value.as_array() else {
        return value.to_string();
    };
    if rows.is_empty() {
        return "no snapshots".to_string();
    }
    let mut lines = vec![format!(
        "{:<28} {:>12} {:>13} {}",
        "NAME", "SIZE_BYTES", "CREATED_AT_MS", "MODE"
    )];
    for row in rows {
        lines.push(format!(
            "{:<28} {:>12} {:>13} {}",
            text(row, "name"),
            number(row, "size_bytes"),
            number(row, "created_at_ms"),
            text(row, "mode"),
        ));
    }
    lines.join("\n")
}

/// The merged registry: what a run would use, and every definition in it.
fn sandboxes(value: &Value) -> String {
    let Some(rows) = value.get("sandboxes").and_then(Value::as_array) else {
        return value.to_string();
    };
    let mut lines = vec![
        format!("{:<10} {}", "current", optional_text(value, "current")),
        format!("{:<10} {}", "default", text(value, "default")),
        String::new(),
        format!(
            "{:<28} {:<11} {:>8} {:>9} {:>9}",
            "NAME", "SOURCE", "RUNNABLE", "SHADOWED", "MEMORY_MB"
        ),
    ];
    for row in rows {
        lines.push(format!(
            "{:<28} {:<11} {:>8} {:>9} {:>9}",
            text(row, "name"),
            text(row, "source"),
            text(row, "runnable"),
            text(row, "shadowed"),
            optional_number(row, "memory_mb"),
        ));
    }
    lines.join("\n")
}

/// The stored choice and the fallback's name, two lines.
fn sandbox_current(value: &Value) -> String {
    [
        ("current", optional_text(value, "current")),
        ("default", text(value, "default")),
    ]
    .iter()
    .map(|(key, value)| format!("{key:<10} {value}"))
    .collect::<Vec<_>>()
    .join("\n")
}

/// The raw scan, the two lists kept apart (they are not combined).
fn sandbox_candidates(value: &Value) -> String {
    let mut lines = Vec::new();
    for (key, label) in [("toolchains", "TOOLCHAINS"), ("qemus", "QEMUS")] {
        let rows = value.get(key).and_then(Value::as_array);
        lines.push(format!("{label} ({})", rows.map(Vec::len).unwrap_or(0)));
        match rows {
            Some(rows) if !rows.is_empty() => {
                lines.push(format!(
                    "  {:<10} {:<12} {:<10} {:>8}  {}",
                    "KIND", "VERSION", "ORIGIN", "RUNNABLE", "PATH"
                ));
                for row in rows {
                    lines.push(format!(
                        "  {:<10} {:<12} {:<10} {:>8}  {}",
                        text(row, "kind"),
                        text(row, "version"),
                        text(row, "origin"),
                        text(row, "runnable"),
                        text(row, "path"),
                    ));
                }
            }
            _ => lines.push("  (none)".to_string()),
        }
    }
    lines.join("\n")
}

/// The switch's answer: where the node came from and where it went.
fn sandbox_switched(value: &Value) -> String {
    let to = text(value, "to");
    match value.get("from").and_then(Value::as_str) {
        Some(from) => format!("switched from {from} to {to}"),
        // Nothing was current before: say where it went, not "from null".
        None => format!("switched to {to}"),
    }
}

/// The request queue, one line per request: what it is, what it wants, where it
/// stands, and who decided.
fn sandbox_requests(value: &Value) -> String {
    let rows = value
        .get("requests")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if rows.is_empty() {
        return "no sandbox requests".to_string();
    }
    rows.iter()
        .map(|row| {
            let target = match row.get("sandbox").and_then(Value::as_str) {
                Some(name) => format!(" -> {name}"),
                None => String::new(),
            };
            let decided = match row.get("decided_by").and_then(Value::as_str) {
                Some(by) => format!(" by {by}"),
                None => String::new(),
            };
            format!(
                "{} {}{} [{}] {}{}",
                text(row, "id"),
                text(row, "action"),
                target,
                text(row, "status"),
                text(row, "requester_agent_id"),
                decided,
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// What a decision landed on.
fn sandbox_request_decided(value: &Value) -> String {
    format!("{} is now {}", text(value, "id"), text(value, "status"))
}

/// One definition, one `key value` line per field.
fn sandbox_detail(value: &Value) -> String {
    [
        ("name", text(value, "name")),
        ("display_name", optional_text(value, "display_name")),
        ("source", text(value, "source")),
        ("runnable", text(value, "runnable")),
        ("shadowed", text(value, "shadowed")),
        ("memory_mb", optional_number(value, "memory_mb")),
        ("qemu_exe", optional_text(value, "qemu_exe")),
        ("toolchain_path", optional_text(value, "toolchain_path")),
        ("kernel", optional_text(value, "kernel")),
        ("notes", optional_text(value, "notes")),
    ]
    .iter()
    .map(|(key, value)| format!("{key:<20} {value}"))
    .collect::<Vec<_>>()
    .join("\n")
}

/// A string field, or `-` when it is absent or `null`.
fn text(value: &Value, key: &str) -> String {
    match value.get(key) {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Bool(flag)) => flag.to_string(),
        Some(Value::Number(number)) => number.to_string(),
        _ => "-".to_string(),
    }
}

fn number(value: &Value, key: &str) -> String {
    match value.get(key).and_then(Value::as_i64) {
        Some(number) => number.to_string(),
        None => "-".to_string(),
    }
}

/// A number that is legitimately absent (`ended_at_ms` on an open run).
fn optional_number(value: &Value, key: &str) -> String {
    match value.get(key) {
        Some(Value::Number(number)) => number.to_string(),
        _ => "-".to_string(),
    }
}

fn optional_text(value: &Value, key: &str) -> String {
    match value.get(key) {
        Some(Value::String(text)) => text.clone(),
        _ => "-".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reply(body: &str) -> Reply {
        Reply {
            status: 200,
            body: body.to_string(),
            json: serde_json::from_str(body).ok(),
        }
    }

    #[test]
    fn health_is_one_short_line() {
        let reply = reply(r#"{"status":"ok","version":"0.8.0","uptime_ms":1971}"#);
        assert_eq!(
            human(&Command::Health, &reply),
            "ok  version 0.8.0  uptime 1971 ms"
        );
    }

    #[test]
    fn status_lists_the_fields_the_endpoint_documents() {
        let reply = reply(
            r#"{"agents":1,"agent_id":"local-17480-1","connections":1,"sse_subscribers":0,
                "status":"ok","uptime_ms":1997,"version":"0.8.0"}"#,
        );
        let text = human(&Command::Status, &reply);
        assert!(text.contains("agent_id        local-17480-1"), "{text}");
        assert!(text.contains("sse_subscribers 0"), "{text}");
        assert!(text.contains("connections     1"), "{text}");
    }

    #[test]
    fn agents_is_a_derived_view_of_status() {
        let reply = reply(r#"{"agents":2,"agent_id":"local-1-1","status":"ok"}"#);
        assert_eq!(
            human(&Command::Agents, &reply),
            "agents 2  agent_id local-1-1"
        );
    }

    #[test]
    fn an_empty_list_says_so_rather_than_printing_a_header() {
        let reply = reply("[]");
        assert_eq!(human(&Command::RunsList { limit: None }, &reply), "no runs");
        assert_eq!(
            human(&Command::AuditEvents { limit: 20 }, &reply),
            "no events"
        );
        assert_eq!(human(&Command::SnapshotsList, &reply), "no snapshots");
    }

    #[test]
    fn runs_are_a_table_with_the_documented_columns() {
        let rows = reply(
            r#"[{"run_id":"local-1-1","status":"ok","started_at_ms":100,"ended_at_ms":200,
                 "fingerprint":"ab","fingerprint_short":"ab","parent_run_id":null,
                 "session_id":null,"resumed_from_snapshot":null}]"#,
        );
        let text = human(&Command::RunsList { limit: None }, &rows);
        let lines: Vec<&str> = text.lines().collect();
        assert!(lines[0].contains("RUN_ID"), "{text}");
        assert!(lines[0].contains("STARTED_MS"), "{text}");
        assert!(lines[1].contains("local-1-1"), "{text}");
        assert!(lines[1].contains("200"), "{text}");
        // An open run prints `-` for its end, not an empty column.
        let open = reply(r#"[{"run_id":"local-1-2","status":"open","started_at_ms":100}]"#);
        assert!(human(&Command::RunsList { limit: None }, &open).contains('-'));
    }

    #[test]
    fn audit_status_shows_the_chain_verdict() {
        let intact = reply(
            r#"{"count":42,"chain":{"status":"Intact","length":42},"alert_on_failure":true,"failures":[]}"#,
        );
        let text = human(&Command::AuditStatus, &intact);
        assert!(text.contains("chain            Intact length 42"), "{text}");
        assert!(text.contains("alert_on_failure true"), "{text}");
        assert!(text.contains("failures         0"), "{text}");

        let broken = reply(
            r#"{"count":3,"chain":{"status":"Broken","at_id":7,"reason":"hash mismatch"},"alert_on_failure":false,"failures":["disk"]}"#,
        );
        let text = human(&Command::AuditStatus, &broken);
        assert!(text.contains("Broken at 7 (hash mismatch)"), "{text}");
        assert!(text.contains("failures         1"), "{text}");
    }

    #[test]
    fn events_are_a_table_too() {
        let reply = reply(
            r#"[{"id":1,"timestamp_ms":100,"actor":"host","action":"host.start","detail":{},"prev_hash":"","hash":"","agent_id":null}]"#,
        );
        let text = human(&Command::AuditEvents { limit: 20 }, &reply);
        assert!(text.contains("ACTION"), "{text}");
        assert!(text.contains("host.start"), "{text}");
        assert!(text.contains("host"), "{text}");
    }

    #[test]
    fn a_missing_field_prints_a_dash_instead_of_vanishing() {
        let reply = reply(r#"{"status":"ok"}"#);
        let text = human(&Command::Health, &reply);
        assert_eq!(text, "ok  version -  uptime - ms");
    }

    #[test]
    fn a_run_outcome_leads_with_its_shape_and_then_the_answer() {
        let outcome =
            reply(r#"{"kind":"final","content":"all done","reason":null,"iterations":2}"#);
        let text = human(&Command::Run { task: "t".into() }, &outcome);
        assert!(text.starts_with("kind       final\niterations 2"), "{text}");
        assert!(text.ends_with("all done"), "{text}");
        // A turn that gave up carries its reason on its own line.
        let stopped =
            reply(r#"{"kind":"stopped","content":"","reason":"max iterations","iterations":9}"#);
        let text = human(&Command::Run { task: "t".into() }, &stopped);
        assert!(text.contains("reason     max iterations"), "{text}");
    }

    #[test]
    fn the_vm_and_session_writes_render_as_short_lines() {
        assert_eq!(human(&Command::VmStop, &reply("{}")), "ok");
        assert_eq!(human(&Command::VmStart, &reply("{}")), "ok");
        assert_eq!(
            human(
                &Command::SnapshotsSave { name: "a".into() },
                &reply(r#"{"bytes_written":4096}"#)
            ),
            "wrote 4096 bytes"
        );
        assert_eq!(
            human(
                &Command::SessionsCreate { title: "t".into() },
                &reply(r#"{"session_id":"s-1"}"#)
            ),
            "session_id s-1"
        );
        assert_eq!(human(&Command::SessionsClearAll, &reply("{}")), "ok");
        // The session deletes and the rename answer `204`: success, nothing to print.
        assert_eq!(
            human(
                &Command::SessionsDelete {
                    session_id: "s".into()
                },
                &reply("{}")
            ),
            "ok"
        );
        assert_eq!(
            human(
                &Command::SessionsRename {
                    session_id: "s".into(),
                    title: "t".into()
                },
                &reply("{}")
            ),
            "ok"
        );
    }

    #[test]
    fn a_snapshot_delete_says_whether_it_deleted_anything() {
        // The one delete endpoint that answers a body; the session deletes answer
        // `204` and render as `ok` (see the test above).
        let command = Command::SnapshotsDelete { name: "a".into() };
        assert_eq!(human(&command, &reply(r#"{"deleted":true}"#)), "deleted");
        assert_eq!(
            human(&command, &reply(r#"{"deleted":false}"#)),
            "nothing to delete"
        );
    }

    #[test]
    fn abandoning_stale_runs_names_the_count_or_says_there_were_none() {
        assert_eq!(
            human(&Command::RunsAbandonStale, &reply(r#"{"abandoned":[]}"#)),
            "no stale runs"
        );
        assert_eq!(
            human(
                &Command::RunsAbandonStale,
                &reply(r#"{"abandoned":["local-1-1","local-1-2"]}"#)
            ),
            "abandoned 2"
        );
    }

    #[test]
    fn opening_a_session_shows_its_meta_and_how_many_messages_it_holds() {
        let detail = reply(r#"{"meta":{"session_id":"s-1","title":"work"},"messages":[{},{}]}"#);
        let text = human(
            &Command::SessionsOpen {
                session_id: "s-1".into(),
            },
            &detail,
        );
        assert!(text.contains("session_id s-1"), "{text}");
        assert!(text.contains("title      work"), "{text}");
        assert!(text.contains("messages   2"), "{text}");
    }

    #[test]
    fn a_follow_frame_is_the_event_name_and_a_short_payload() {
        let frame = Frame::for_test(
            Some("17-3".into()),
            "{\"kind\":\"event\",\"event\":\"agent:tool_call\",\"payload\":{\"name\":\"compile\"}}",
        );
        assert_eq!(frame_line(&frame), "agent:tool_call {\"name\":\"compile\"}");

        // `hello` and `gap` carry no event name: the kind is the label.
        let hello = Frame::for_test(
            Some("17-0".into()),
            "{\"kind\":\"hello\",\"event\":null,\"payload\":{}}",
        );
        assert!(
            frame_line(&hello).starts_with("hello "),
            "{}",
            frame_line(&hello)
        );

        // A payload longer than the cut is truncated, not dropped.
        let long = format!(
            "{{\"kind\":\"event\",\"event\":\"x\",\"payload\":\"{}\"}}",
            "y".repeat(400)
        );
        let frame = Frame::for_test(Some("17-1".into()), &long);
        let line = frame_line(&frame);
        assert!(line.ends_with('…'), "{line}");
        assert!(line.chars().count() <= 200, "{line}");
    }

    #[test]
    fn a_frame_that_is_not_json_is_printed_as_it_arrived() {
        let frame = Frame::for_test(Some("17-1".into()), "not json at all");
        assert_eq!(frame_line(&frame), "not json at all");
    }

    #[test]
    fn a_body_that_is_not_json_is_passed_through() {
        let mut not_json = reply("plain text");
        not_json.json = None;
        assert_eq!(human(&Command::Health, &not_json), "plain text");
    }

    #[test]
    fn an_export_says_how_much_it_wrote_and_where() {
        // The audit exports answer `events_exported`; the serial export answers
        // `bytes_written`. The human line says which is which.
        let one = reply(r#"{"events_exported":1}"#);
        assert_eq!(
            human(
                &Command::ExportAuditJsonl {
                    out: "audit.jsonl".into()
                },
                &one
            ),
            "exported 1 event to audit.jsonl"
        );
        let many = reply(r#"{"events_exported":42}"#);
        assert_eq!(
            human(
                &Command::ExportRunAudit {
                    run_id: "r-1".into(),
                    out: "run-r-1.jsonl".into()
                },
                &many
            ),
            "exported 42 events to run-r-1.jsonl"
        );
        assert_eq!(
            human(
                &Command::ExportSerialLog {
                    out: "serial.log".into()
                },
                &reply(r#"{"bytes_written":4096}"#)
            ),
            "wrote 4096 bytes to serial.log"
        );
    }

    #[test]
    fn the_async_acknowledgements_name_what_started() {
        // The `202` bodies are acknowledgements, not results: the human line says
        // so rather than inventing an outcome.
        assert_eq!(
            human(
                &Command::ToolchainDownload,
                &reply(r#"{"state":"started"}"#)
            ),
            "download started"
        );
        assert_eq!(
            human(
                &Command::ToolchainCancel,
                &reply(r#"{"state":"cancelling"}"#)
            ),
            "download cancelling"
        );
        assert_eq!(
            human(&Command::PreflightRun, &reply(r#"{"state":"running"}"#)),
            "preflight running"
        );
        assert_eq!(
            human(&Command::QemuDownload, &reply(r#"{"state":"started"}"#)),
            "qemu download started"
        );
        assert_eq!(
            human(&Command::QemuCancel, &reply(r#"{"state":"cancelling"}"#)),
            "qemu download cancelling"
        );
    }

    #[test]
    fn a_download_status_names_the_last_state() {
        let idle = reply(r#"{"in_progress":false,"last_event":null}"#);
        assert_eq!(
            human(&Command::QemuStatus, &idle),
            "in_progress false\nlast_event  -"
        );
        let running =
            reply(r#"{"in_progress":true,"last_event":{"state":"progress","downloaded":1}}"#);
        assert_eq!(
            human(&Command::QemuStatus, &running),
            "in_progress true\nlast_event  progress"
        );
    }

    #[test]
    fn the_configuration_writes_that_answer_nothing_render_as_ok() {
        for command in [
            Command::LlmClear,
            Command::QemuClear,
            Command::ToolchainClear,
            Command::LlmLoadKey {
                provider_id: "p".into(),
            },
            Command::QemuPath { path: "p".into() },
            Command::ToolchainPath { path: "p".into() },
            Command::ThemeSet {
                theme: "dark".into(),
            },
            Command::LanguageSet {
                language: "zh".into(),
            },
            Command::AuditAlertSet { enabled: true },
        ] {
            assert_eq!(human(&command, &reply("{}")), "ok", "{command:?}");
        }
    }

    #[test]
    fn a_preflight_view_lists_the_steps_and_the_failure() {
        let view = reply(
            r#"{"ran":true,"checked":true,"ok":false,"rows":[
                 {"step":"gcc_runs","state":"ok","detail":"gcc 13.2.0"},
                 {"step":"gcc_compiles","state":"failed","detail":"not found"},
                 {"step":"qemu_runs","state":"not_run","detail":null},
                 {"step":"guest_boots","state":"not_run","detail":null}],
               "failed_step":"gcc_compiles","detail":"not found",
               "suggestion":"install the toolchain"}"#,
        );
        let text = human(&Command::PreflightAck, &view);
        assert!(text.contains("checked  true"), "{text}");
        assert!(text.contains("ok       false"), "{text}");
        assert!(text.contains("gcc_compiles   failed"), "{text}");
        assert!(text.contains("guest_boots    not_run"), "{text}");
        assert!(text.contains("failed_step gcc_compiles"), "{text}");
        assert!(text.contains("install the toolchain"), "{text}");
    }
}
