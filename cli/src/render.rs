//! The human mode: tables and short key/value lines.
//!
//! `--json` passes the control plane's JSON through untouched; this module is what
//! the same answer looks like to a person. Everything is read out of the parsed
//! JSON by field name, and a missing field prints `-` rather than disappearing:
//! the shape belongs to the API document, not to this file.

use crate::args::Command;
use crate::client::Reply;
use serde_json::Value;

/// Render one answer for a human.
pub fn human(command: &Command, reply: &Reply) -> String {
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
    }
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
    fn a_body_that_is_not_json_is_passed_through() {
        let mut not_json = reply("plain text");
        not_json.json = None;
        assert_eq!(human(&Command::Health, &not_json), "plain text");
    }
}
