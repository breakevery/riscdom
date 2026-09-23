//! Asks for a sandbox change, and the queue they wait in (v0.9 sandbox F2c).
//!
//! A **request** is not a command. `POST /v0/sandboxes/switch` changes the node
//! and needs `sandbox.switch`; an actor that may not switch — an agent, most of
//! all — can only leave one of these behind. Deciding it (`approve` / `reject`)
//! is another actor's, needs the capability the request's `action` implies, and
//! **does not perform the change**: the switch is a second, authorised call
//! (F2c decision 4). The queue is therefore a ledger of intent, not a to-do list
//! anybody executes automatically.
//!
//! The queue is a queue, not one slot: the switch slot's "one at a time" is about
//! *doing* the work, and several asks may wait at once. `#[derive(Clone)]` is for
//! the agent loop's tool gateway — the loop must not hold an `Arc<AppState>` (it
//! lives inside one), so the gateway holds these handles instead.

use crate::events::{EventSink, EV_SANDBOX_REQUEST};
use crate::sandbox_def::SandboxDef;
use crate::HostError;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// A request's own counter: `req-<pid>-<seq>`.
///
/// Its own namespace on purpose (F2c decision 3): a request is not a task, and
/// `task-` in front of it would make the two indistinguishable in a log.
static REQ_SEQ: AtomicU64 = AtomicU64::new(0);

/// What a request asks for. The capability a decision needs follows from it
/// (`switch` → `sandbox.switch`, `define` / `assemble` → `sandbox.assemble`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SandboxAction {
    Switch,
    Define,
    Assemble,
}

impl SandboxAction {
    pub fn as_str(self) -> &'static str {
        match self {
            SandboxAction::Switch => "switch",
            SandboxAction::Define => "define",
            SandboxAction::Assemble => "assemble",
        }
    }

    /// Parse the wire name. `None` for anything else, so a typo is a refusal
    /// rather than a silently different action.
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "switch" => Some(SandboxAction::Switch),
            "define" => Some(SandboxAction::Define),
            "assemble" => Some(SandboxAction::Assemble),
            _ => None,
        }
    }
}

impl std::fmt::Display for SandboxAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Where a request stands. `pending → approved | rejected`, and no way back:
/// a decision that could be taken twice would make `decided_by` meaningless
/// (F2c decision 5).
///
/// `Expired` is **reserved and never set** in v0.9 — the queue has no TTL and no
/// sweeper (F2c decision 2). It is here so the day one is added the wire already
/// has the state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SandboxRequestStatus {
    Pending,
    Approved,
    Rejected,
    Expired,
}

impl SandboxRequestStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            SandboxRequestStatus::Pending => "pending",
            SandboxRequestStatus::Approved => "approved",
            SandboxRequestStatus::Rejected => "rejected",
            SandboxRequestStatus::Expired => "expired",
        }
    }

    /// Parse the wire name (the `?status=` filter and the request body).
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "pending" => Some(SandboxRequestStatus::Pending),
            "approved" => Some(SandboxRequestStatus::Approved),
            "rejected" => Some(SandboxRequestStatus::Rejected),
            "expired" => Some(SandboxRequestStatus::Expired),
            _ => None,
        }
    }
}

impl std::fmt::Display for SandboxRequestStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One ask, as it waits in the queue.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SandboxRequest {
    /// `req-<pid>-<seq>`.
    pub id: String,
    /// Who asked: an agent id, or the interface's actor.
    pub requester_agent_id: String,
    pub action: SandboxAction,
    /// The sandbox to switch to (`switch`), or the name to give a definition.
    #[serde(default)]
    pub sandbox: Option<String>,
    /// A definition to register, when the ask carries one. The AI's
    /// `request_sandbox` tool cannot: a model does not get to write a path
    /// (`SandboxDef` carries `qemu_exe` / `toolchain_path`), so it only ever
    /// asks (F2c decision 4).
    #[serde(default)]
    pub definition: Option<SandboxDef>,
    #[serde(default)]
    pub reason: Option<String>,
    pub requested_at_ms: u64,
    pub status: SandboxRequestStatus,
    #[serde(default)]
    pub decided_by: Option<String>,
    #[serde(default)]
    pub decided_at_ms: Option<u64>,
}

/// What the wire shows: the same record with the two enums as their names, so a
/// client compares strings instead of guessing a casing.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SandboxRequestView {
    pub id: String,
    pub requester_agent_id: String,
    pub action: &'static str,
    pub sandbox: Option<String>,
    pub definition: Option<SandboxDef>,
    pub reason: Option<String>,
    pub requested_at_ms: u64,
    pub status: &'static str,
    pub decided_by: Option<String>,
    pub decided_at_ms: Option<u64>,
}

impl From<&SandboxRequest> for SandboxRequestView {
    fn from(request: &SandboxRequest) -> Self {
        Self {
            id: request.id.clone(),
            requester_agent_id: request.requester_agent_id.clone(),
            action: request.action.as_str(),
            sandbox: request.sandbox.clone(),
            definition: request.definition.clone(),
            reason: request.reason.clone(),
            requested_at_ms: request.requested_at_ms,
            status: request.status.as_str(),
            decided_by: request.decided_by.clone(),
            decided_at_ms: request.decided_at_ms,
        }
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// The queue itself, shared by `AppState` and the loop's tool gateway.
///
/// `current` is the node's running sandbox (`AppState::current_sandbox`'s own
/// `Arc`): the gateway answers `sandbox_status` from here, and reading it through
/// the same handle is what keeps the two views from drifting.
#[derive(Clone)]
pub struct SandboxRequests {
    requests: Arc<Mutex<Vec<SandboxRequest>>>,
    current: Arc<Mutex<Option<String>>>,
}

impl SandboxRequests {
    pub fn new(current: Arc<Mutex<Option<String>>>) -> Self {
        Self {
            requests: Arc::new(Mutex::new(Vec::new())),
            current,
        }
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, Vec<SandboxRequest>>, HostError> {
        self.requests
            .lock()
            .map_err(|_| HostError::Other("sandbox request queue lock poisoned".into()))
    }

    /// Append a request and answer the record (the caller announces it).
    pub fn enqueue(
        &self,
        requester: &str,
        action: SandboxAction,
        sandbox: Option<String>,
        definition: Option<SandboxDef>,
        reason: Option<String>,
    ) -> Result<SandboxRequest, HostError> {
        let seq = REQ_SEQ.fetch_add(1, Ordering::Relaxed) + 1;
        let request = SandboxRequest {
            id: format!("req-{}-{seq}", std::process::id()),
            requester_agent_id: requester.to_string(),
            action,
            sandbox,
            definition,
            reason,
            requested_at_ms: now_ms(),
            status: SandboxRequestStatus::Pending,
            decided_by: None,
            decided_at_ms: None,
        };
        self.lock()?.push(request.clone());
        Ok(request)
    }

    /// The queue, newest first, optionally filtered by status.
    pub fn list(&self, status: Option<SandboxRequestStatus>) -> Vec<SandboxRequestView> {
        let Ok(requests) = self.lock() else {
            return Vec::new();
        };
        requests
            .iter()
            .rev()
            .filter(|r| status.is_none_or(|wanted| r.status == wanted))
            .map(SandboxRequestView::from)
            .collect()
    }

    /// `pending → approved | rejected`, once.
    ///
    /// An unknown id is a `404` and a second decision is a `409`: both are
    /// refusals a caller can branch on, not silent no-ops.
    pub fn decide(
        &self,
        id: &str,
        decision: SandboxRequestStatus,
        by: &str,
    ) -> Result<SandboxRequest, HostError> {
        let mut requests = self.lock()?;
        let Some(request) = requests.iter_mut().find(|r| r.id == id) else {
            return Err(HostError::SandboxRequestNotFound(id.to_string()));
        };
        if request.status != SandboxRequestStatus::Pending {
            return Err(HostError::SandboxRequestDecided(format!(
                "{} was already {}",
                request.id,
                request.status.as_str()
            )));
        }
        request.status = decision;
        request.decided_by = Some(by.to_string());
        request.decided_at_ms = Some(now_ms());
        Ok(request.clone())
    }

    /// What a request asks for — the route needs it before deciding, because the
    /// capability a decision requires follows from the action (F2c decision 1).
    pub fn action_of(&self, id: &str) -> Result<SandboxAction, HostError> {
        let requests = self.lock()?;
        requests
            .iter()
            .find(|r| r.id == id)
            .map(|r| r.action)
            .ok_or_else(|| HostError::SandboxRequestNotFound(id.to_string()))
    }

    /// The queued requests still waiting, oldest first.
    pub fn pending(&self) -> Vec<SandboxRequestView> {
        let Ok(requests) = self.lock() else {
            return Vec::new();
        };
        requests
            .iter()
            .filter(|r| r.status == SandboxRequestStatus::Pending)
            .map(SandboxRequestView::from)
            .collect()
    }

    /// A one-screen summary for the AI's `sandbox_status` tool: what runs now,
    /// and how much is waiting. Deliberately prose — it is a tool result, and the
    /// model does not need the ids it cannot act on.
    pub fn summary(&self) -> String {
        let running = self
            .current
            .lock()
            .ok()
            .and_then(|current| current.clone())
            .unwrap_or_else(|| "(default)".to_string());
        let waiting = self.pending();
        if waiting.is_empty() {
            return format!("running: {running}\nno sandbox requests are waiting");
        }
        let mut out = format!(
            "running: {running}\n{} request(s) waiting:\n",
            waiting.len()
        );
        for view in waiting {
            out.push_str(&format!(
                "- {} {}{}\n",
                view.id,
                view.action,
                view.sandbox
                    .map(|name| format!(" → {name}"))
                    .unwrap_or_default()
            ));
        }
        out
    }
}

/// The queue plus the sink every change is announced on.
///
/// `AppState` builds one per call from its own queue and the caller's sink (HTTP,
/// Tauri, or a run's emitter); the agent loop's tool gateway holds one built at
/// loop-construction time. One place emits, so the three ways in cannot drift.
pub struct SandboxRequestService {
    requests: SandboxRequests,
    sink: Arc<dyn EventSink>,
}

impl SandboxRequestService {
    pub fn new(requests: SandboxRequests, sink: Arc<dyn EventSink>) -> Self {
        Self { requests, sink }
    }

    fn announce(&self, request: &SandboxRequest, status: SandboxRequestStatus) {
        self.sink.emit(
            EV_SANDBOX_REQUEST,
            crate::events::sandbox_request_payload(
                &request.id,
                status.as_str(),
                &request.requester_agent_id,
                request.action.as_str(),
            ),
        );
    }

    /// Enqueue, announce `pending`, and answer the new id.
    #[allow(clippy::too_many_arguments)]
    pub fn request(
        &self,
        requester: &str,
        action: SandboxAction,
        sandbox: Option<String>,
        definition: Option<SandboxDef>,
        reason: Option<String>,
    ) -> Result<SandboxRequestView, HostError> {
        let request = self
            .requests
            .enqueue(requester, action, sandbox, definition, reason)?;
        self.announce(&request, SandboxRequestStatus::Pending);
        Ok(SandboxRequestView::from(&request))
    }

    /// Decide, announce, and answer the decided record.
    pub fn decide(
        &self,
        id: &str,
        decision: SandboxRequestStatus,
        by: &str,
    ) -> Result<SandboxRequestView, HostError> {
        let request = self.requests.decide(id, decision, by)?;
        self.announce(&request, decision);
        Ok(SandboxRequestView::from(&request))
    }

    pub fn action_of(&self, id: &str) -> Result<SandboxAction, HostError> {
        self.requests.action_of(id)
    }

    pub fn list(&self, status: Option<SandboxRequestStatus>) -> Vec<SandboxRequestView> {
        self.requests.list(status)
    }

    pub fn summary(&self) -> String {
        self.requests.summary()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::RecordingEventSink;

    fn queue() -> SandboxRequests {
        SandboxRequests::new(Arc::new(Mutex::new(None)))
    }

    fn ask(queue: &SandboxRequests, action: SandboxAction, by: &str) -> SandboxRequest {
        queue
            .enqueue(
                by,
                action,
                Some("blink".into()),
                None,
                Some("why not".into()),
            )
            .expect("enqueue")
    }

    #[test]
    fn a_request_is_pending_and_carries_its_own_id_namespace() {
        let queue = queue();
        let first = ask(&queue, SandboxAction::Switch, "agent-1");
        let second = ask(&queue, SandboxAction::Switch, "agent-1");
        assert!(first.id.starts_with("req-"), "{}", first.id);
        assert!(!first.id.starts_with("task-"), "{}", first.id);
        assert_ne!(first.id, second.id);
        assert_eq!(first.status, SandboxRequestStatus::Pending);
        assert!(first.decided_by.is_none());
        assert!(first.decided_at_ms.is_none());
        assert!(first.requested_at_ms > 0);
        assert_eq!(first.sandbox.as_deref(), Some("blink"));
    }

    #[test]
    fn a_decision_is_recorded_once_and_is_not_reversible() {
        let queue = queue();
        let request = ask(&queue, SandboxAction::Switch, "agent-1");
        let decided = queue
            .decide(&request.id, SandboxRequestStatus::Approved, "human")
            .expect("approve");
        assert_eq!(decided.status, SandboxRequestStatus::Approved);
        assert_eq!(decided.decided_by.as_deref(), Some("human"));
        assert!(decided.decided_at_ms.is_some());

        let again = queue.decide(&request.id, SandboxRequestStatus::Rejected, "human");
        assert!(matches!(again, Err(HostError::SandboxRequestDecided(_))));
        // The first decision stands.
        let listed = queue.list(None);
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].status, "approved");
    }

    #[test]
    fn an_unknown_id_is_a_not_found_not_a_panic() {
        let queue = queue();
        // `action_of` is what the route calls first, so it refuses the same way.
        assert!(matches!(
            queue.action_of("req-0-404"),
            Err(HostError::SandboxRequestNotFound(_))
        ));
        assert!(matches!(
            queue.decide("req-0-404", SandboxRequestStatus::Approved, "human"),
            Err(HostError::SandboxRequestNotFound(_))
        ));
    }

    #[test]
    fn the_list_filters_by_status_and_shows_the_newest_first() {
        let queue = queue();
        let first = ask(&queue, SandboxAction::Switch, "agent-1");
        let second = ask(&queue, SandboxAction::Define, "agent-2");
        queue
            .decide(&first.id, SandboxRequestStatus::Rejected, "human")
            .expect("reject");

        let all: Vec<String> = queue.list(None).into_iter().map(|v| v.id).collect();
        assert_eq!(all, vec![second.id.clone(), first.id.clone()]);
        let pending: Vec<String> = queue
            .list(Some(SandboxRequestStatus::Pending))
            .into_iter()
            .map(|v| v.id)
            .collect();
        assert_eq!(pending, vec![second.id.clone()]);
        assert_eq!(queue.pending().len(), 1);
        let rejected: Vec<String> = queue
            .list(Some(SandboxRequestStatus::Rejected))
            .into_iter()
            .map(|v| v.id)
            .collect();
        assert_eq!(rejected, vec![first.id.clone()]);
    }

    #[test]
    fn expired_is_a_state_nothing_sets() {
        // Reserved for a TTL that v0.9 does not implement (F2c decision 2): the
        // wire has the name, no path produces it.
        let queue = queue();
        ask(&queue, SandboxAction::Switch, "agent-1");
        assert!(queue.list(Some(SandboxRequestStatus::Expired)).is_empty());
        assert_eq!(
            SandboxRequestStatus::parse("expired"),
            Some(SandboxRequestStatus::Expired)
        );
    }

    #[test]
    fn every_change_lands_on_the_event_stream_once() {
        let sink = Arc::new(RecordingEventSink::new());
        let service = SandboxRequestService::new(
            queue(),
            Arc::clone(&sink) as Arc<dyn crate::events::EventSink>,
        );
        let first = service
            .request(
                "local-1-1",
                SandboxAction::Switch,
                Some("blink".into()),
                None,
                None,
            )
            .expect("request");
        service
            .decide(&first.id, SandboxRequestStatus::Approved, "operator")
            .expect("approve");
        let second = service
            .request("local-1-1", SandboxAction::Assemble, None, None, None)
            .expect("request");
        service
            .decide(&second.id, SandboxRequestStatus::Rejected, "operator")
            .expect("reject");

        let events = sink.events();
        assert_eq!(events.len(), 4, "one frame per change, and no more");
        assert!(events
            .iter()
            .all(|(name, _)| name == crate::events::EV_SANDBOX_REQUEST));
        let statuses: Vec<String> = events
            .iter()
            .map(|(_, payload)| payload["status"].as_str().unwrap_or_default().to_string())
            .collect();
        assert_eq!(statuses, vec!["pending", "approved", "pending", "rejected"]);
        // The frames name the ask, and the requester is the host's agent id.
        assert_eq!(events[1].1["id"], serde_json::json!(first.id));
        assert_eq!(events[1].1["requester"], "local-1-1");
        assert_eq!(events[1].1["action"], "switch");
        assert_eq!(events[3].1["action"], "assemble");
    }

    #[test]
    fn a_refused_decision_announces_nothing() {
        // A `404` / `409` changes no record, so nothing is published either: a
        // stream that showed a decision that did not happen would be worse than
        // silence.
        let sink = Arc::new(RecordingEventSink::new());
        let service = SandboxRequestService::new(
            queue(),
            Arc::clone(&sink) as Arc<dyn crate::events::EventSink>,
        );
        assert!(service
            .decide("req-0-404", SandboxRequestStatus::Approved, "operator")
            .is_err());
        assert!(sink.events().is_empty());
    }

    #[test]
    fn the_summary_names_what_runs_and_what_waits() {
        let queue = SandboxRequests::new(Arc::new(Mutex::new(Some("blink".to_string()))));
        assert!(queue.summary().contains("running: blink"));
        assert!(queue.summary().contains("no sandbox requests are waiting"));
        ask(&queue, SandboxAction::Assemble, "agent-1");
        let summary = queue.summary();
        assert!(summary.contains("1 request(s) waiting"), "{summary}");
        assert!(summary.contains("assemble"), "{summary}");
    }

    #[test]
    fn the_action_names_round_trip_and_a_typo_is_refused() {
        for action in [
            SandboxAction::Switch,
            SandboxAction::Define,
            SandboxAction::Assemble,
        ] {
            assert_eq!(SandboxAction::parse(action.as_str()), Some(action));
        }
        assert_eq!(SandboxAction::parse("reboot"), None);
        assert_eq!(SandboxRequestStatus::parse("maybe"), None);
    }
}
