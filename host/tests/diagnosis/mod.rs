//! Failure-path diagnostics for the end-to-end runs (stage 5c-3).
//!
//! When a run fails, the useful question is "which step broke, and what did it
//! say" — not which assertion noticed first. This module collects that answer from
//! the audit trail and the host's own events and renders one report:
//!
//! - the outcome (kind, reason, iterations);
//! - **the first thing that went wrong**, named;
//! - every tool execution in order, with the failing ones marked;
//! - the serial console (byte count and tail), including the explicit "the guest
//!   never printed anything" case;
//! - the VM state, the audit-chain verdict and the host event counts.
//!
//! `e2e_ui.rs` prints it on every run (it is a manual `--ignored` test), and
//! `run_diagnosis.rs` pins the wording. Nothing here changes an outcome: the
//! assertions still decide pass/fail.
#![allow(dead_code)]

use host::events::RecordingEventSink;
use host::state::{AgentOutcomeView, AppState};
use std::collections::HashMap;

/// One tool execution, as it appears in the audit trail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolStep {
    pub name: String,
    pub ok: bool,
    pub detail: String,
}

/// Everything the report needs.
#[derive(Debug, Clone)]
pub struct RunDiagnosis<'a> {
    pub outcome_kind: &'a str,
    pub outcome_reason: Option<&'a str>,
    pub iterations: u32,
    pub steps: &'a [ToolStep],
    pub serial_bytes: usize,
    pub serial_tail: &'a str,
    pub vm_running: bool,
    pub chain: &'a str,
    pub events: &'a [(String, usize)],
    /// Set when the host returned an error before an outcome existed.
    pub host_error: Option<&'a str>,
}

/// How much of the serial tail a report keeps.
const TAIL_CHARS: usize = 200;

/// Pair every `agent.tool.result` with its `agent.tool.call`.
///
/// The pairing is by `call_id`, not by position, and the steps come back in chain
/// order whatever order the caller handed in (the host's `list_events` is newest
/// first; the audit ids are the chronology).
pub fn tool_steps(events: &[host::StoredEventView]) -> Vec<ToolStep> {
    let mut ordered: Vec<&host::StoredEventView> = events.iter().collect();
    ordered.sort_by_key(|event| event.id);

    let mut names: HashMap<&str, &str> = HashMap::new();
    for event in &ordered {
        if event.action != "agent.tool.call" {
            continue;
        }
        let id = event.detail.get("id").and_then(|v| v.as_str());
        let name = event.detail.get("name").and_then(|v| v.as_str());
        if let (Some(id), Some(name)) = (id, name) {
            names.insert(id, name);
        }
    }

    ordered
        .iter()
        .filter(|event| event.action == "agent.tool.result")
        .map(|event| {
            let call_id = event
                .detail
                .get("call_id")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            ToolStep {
                name: names
                    .get(call_id)
                    .map(|name| (*name).to_string())
                    .unwrap_or_else(|| "(unknown)".to_string()),
                ok: event
                    .detail
                    .get("ok")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false),
                detail: event
                    .detail
                    .get("result")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string(),
            }
        })
        .collect()
}

/// The first thing that went wrong: a host error, the run's own failure reason, or
/// the first failed tool step. `None` means the report found nothing wrong.
pub fn first_failure(d: &RunDiagnosis<'_>) -> Option<String> {
    if let Some(err) = d.host_error {
        return Some(format!("the host refused the run: {err}"));
    }
    if d.outcome_kind != "final" {
        return Some(match d.outcome_reason {
            Some(reason) => format!("the run ended as `{}`: {reason}", d.outcome_kind),
            None => format!("the run ended as `{}`", d.outcome_kind),
        });
    }
    d.steps
        .iter()
        .find(|step| !step.ok)
        .map(|step| format!("tool `{}` failed: {}", step.name, first_line(&step.detail)))
}

/// One line, trimmed: tool output is multi-line and the report stays scannable.
fn first_line(text: &str) -> String {
    let line = text.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
    let trimmed = line.trim();
    if trimmed.chars().count() > 160 {
        let cut: String = trimmed.chars().take(160).collect();
        format!("{cut}…")
    } else {
        trimmed.to_string()
    }
}

fn tail(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= TAIL_CHARS {
        return text.to_string();
    }
    chars[chars.len() - TAIL_CHARS..].iter().collect()
}

/// The whole report, as the e2e test prints it.
pub fn render(d: &RunDiagnosis<'_>) -> String {
    let mut out = String::from("--- run diagnosis (stage 5c-3) ---\n");
    out.push_str(&format!(
        "outcome    : {} after {} iteration(s){}\n",
        d.outcome_kind,
        d.iterations,
        match d.outcome_reason {
            Some(reason) => format!(" (reason: {reason})"),
            None => String::new(),
        }
    ));
    out.push_str(&format!(
        "first failure: {}\n",
        first_failure(d).unwrap_or_else(|| "(none)".to_string())
    ));

    if d.steps.is_empty() {
        out.push_str("steps      : none — the model never called a tool\n");
    } else {
        out.push_str(&format!("steps      : {}\n", d.steps.len()));
        for step in d.steps {
            out.push_str(&format!(
                "  [{}] {}: {}\n",
                if step.ok { "ok " } else { "ERR" },
                step.name,
                first_line(&step.detail)
            ));
        }
    }

    if d.serial_bytes == 0 {
        out.push_str("serial     : 0 byte(s) — the guest never printed anything\n");
    } else {
        out.push_str(&format!(
            "serial     : {} byte(s), tail {:?}\n",
            d.serial_bytes,
            tail(d.serial_tail)
        ));
    }

    out.push_str(&format!(
        "vm         : {}\n",
        if d.vm_running {
            "running"
        } else {
            "not running"
        }
    ));
    out.push_str(&format!("audit chain: {}\n", d.chain));
    if !d.events.is_empty() {
        let counts: Vec<String> = d
            .events
            .iter()
            .map(|(name, count)| format!("{name} x{count}"))
            .collect();
        out.push_str(&format!("events     : {}\n", counts.join(", ")));
    }
    out
}

/// Build the report from a live state: the audit trail, the host's own events and
/// the VM/chain status. `outcome` is `None` when the host refused the run (the
/// error text is then reported instead).
pub fn report(
    state: &AppState,
    sink: &RecordingEventSink,
    outcome: Option<&AgentOutcomeView>,
    host_error: Option<&str>,
) -> String {
    let events = state.list_events(500, None, None).unwrap_or_default();
    let steps = tool_steps(&events);
    let serial = sink.serial_text();
    let chain = match state.audit_status() {
        Ok(status) => format!("{:?}", status.chain),
        Err(e) => format!("unavailable: {e}"),
    };
    let counts = [
        host::events::EV_AGENT_ITERATION,
        host::events::EV_AGENT_TOOL_CALL,
        host::events::EV_AGENT_TOOL_RESULT,
        host::events::EV_AGENT_FINAL,
        host::events::EV_SERIAL_CHUNK,
        host::events::EV_VM_STATE,
        host::events::EV_PREFLIGHT,
    ]
    .iter()
    .map(|name| ((*name).to_string(), sink.count(name)))
    .collect::<Vec<_>>();

    let diagnosis = RunDiagnosis {
        outcome_kind: outcome.map(|o| o.kind.as_str()).unwrap_or("(no outcome)"),
        outcome_reason: outcome.and_then(|o| o.reason.as_deref()),
        iterations: outcome.map(|o| o.iterations).unwrap_or(0),
        steps: &steps,
        serial_bytes: serial.len(),
        serial_tail: &serial,
        vm_running: state.vm_is_running(),
        chain: &chain,
        events: &counts,
        host_error,
    };
    render(&diagnosis)
}
