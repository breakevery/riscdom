//! Local task dispatch (v0.8 batch C).
//!
//! The types, the [`AgentHandle`] / [`Dispatcher`] traits and the local routing
//! live in the `agent` crate (`agent::dispatch`) — deliberately, so the
//! abstraction is not owned by the Tauri-facing host and survives the eventual
//! host-core / host-tauri split instead of forcing it. What the host adds here is
//! its **own** executor: a handle whose "run" is the existing `run_agent` path.
//!
//! This is an added internal route, not a replacement: the Tauri commands still
//! call [`AppState::run_agent`](crate::state::AppState::run_agent) exactly as
//! before, with the same behaviour and the same events. Nothing above it changes.
//!
//! The remote half of the seam is intentionally missing: a handle that reaches
//! another process or another machine would implement [`AgentHandle`] and drop
//! into the same [`LocalDispatcher`], with no change in this file's callers.

use crate::events::EventSink;
use crate::state::{AgentOutcomeView, AppState};
use agent::{
    AgentHandle, AgentId, AgentOutcome, DispatchError, LocalDispatcher, Task, TaskId, TaskOutcome,
};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// This host instance as an executor.
///
/// It holds the state and the emitter the host already uses for a run, so the
/// dispatch path emits the same events (`agent:stream:*`, `agent:final`,
/// `audit:failed`, serial chunks, the audit bridge) as the direct call does.
pub struct HostAgentHandle {
    state: Arc<AppState>,
    emitter: Arc<dyn EventSink>,
    agent_id: AgentId,
}

impl HostAgentHandle {
    /// Wrap a host instance as an executor; the handle takes the instance's
    /// identity, the same one every event it writes carries.
    pub fn new(state: Arc<AppState>, emitter: Arc<dyn EventSink>) -> Self {
        let agent_id = AgentId::new(state.agent_id());
        Self {
            state,
            emitter,
            agent_id,
        }
    }
}

impl AgentHandle for HostAgentHandle {
    fn agent_id(&self) -> &AgentId {
        &self.agent_id
    }

    fn run(&self, task: &Task) -> Result<TaskOutcome, DispatchError> {
        // A handle answers only for its own identity. The dispatcher already
        // routes by target; this keeps the handle correct standing alone (and
        // catches a future remote handle reached with the wrong task).
        if task.target != self.agent_id {
            return Err(DispatchError::NoSuchAgent(task.target.clone()));
        }
        // The one place the run knows which task it is running, so the one place a sink for it can
        // be asked for (v1.0 M6-3a). A sink that cannot rebind is used as it stands, which is the
        // behaviour every event had before this batch.
        let emitter = self
            .emitter
            .with_task(Some(task.id.as_str()))
            .unwrap_or_else(|| Arc::clone(&self.emitter));
        let view = self
            .state
            .run_agent_for(
                emitter,
                &task.input,
                // A task that declares a sandbox runs under it (v0.9 sandbox F2d);
                // one that does not gets the node's own, exactly as before.
                task.sandbox.as_deref(),
                // And a task that names an **instance** runs on it (v1.0 M2a-3),
                // which is checked strictly inside `run_agent_for`.
                task.instance.as_ref(),
            )
            .map_err(|error| DispatchError::Failed(error.to_string()))?;
        // The host instance is the executor here; the identity is the one every
        // event it writes already carries.
        Ok(TaskOutcome {
            task_id: task.id.clone(),
            agent_id: self.agent_id.clone(),
            outcome: outcome_from_view(view),
        })
    }
}

/// A local dispatcher holding this host instance as its only executor.
///
/// One handle today; the vector is where a second local agent — or a remote one —
/// goes without touching the dispatch interface.
pub fn local_dispatcher(state: Arc<AppState>, emitter: Arc<dyn EventSink>) -> LocalDispatcher {
    LocalDispatcher::new(vec![Arc::new(HostAgentHandle::new(state, emitter))])
}

/// How long a remote handle waits between looks at its reply slot (v1.0 M6-1a).
///
/// Short enough that a reply that has landed is picked up at once, long enough that a node waiting for a
/// peer is not a busy loop. The same shape §33's ticker uses for its own slot.
const REPLY_POLL: Duration = Duration::from_millis(100);

/// How long a remote handle waits for a peer to answer (v1.0 M6-1a).
///
/// A default, in the same sense §6.6's 15 s and §7's 30 s are: a peer that has not answered inside it is
/// reported as not having answered, which is a fact and not a guess. Thirty seconds is three orders of
/// magnitude above a local dispatch and one order below a migration.
pub const REMOTE_DISPATCH_TIMEOUT: Duration = Duration::from_secs(30);

/// Where a peer's reply lands (v1.0 M6-1a).
///
/// One shared map, two threads: the node's reader writes a reply when a `task_reply` frame arrives, and a
/// [`RemoteAgentHandle`] takes its own out. The same shape as §33's `takeover_heard` slot — a fact shared
/// between threads without a shared lock order. The map is the caller's to create, because the reader is
/// (v1.0 M6-1b).
pub type TaskReplies = Arc<Mutex<HashMap<TaskId, Result<TaskOutcome, String>>>>;

/// How a remote handle hands a frame to its peer (v1.0 M6-1a).
///
/// A closure rather than the concrete [`net::RelayClient`], so this handle's own logic — the wait, the
/// timeout and the four outcomes — is testable without a socket. The production caller hands it the
/// node's own client, which is what makes the frame an ordinary §3 message (v1.0 M6-1b).
pub type TaskSender = Arc<dyn Fn(&str, &net::TaskFrame) -> Result<(), String> + Send + Sync>;

/// An executor that lives on **another node** (v1.0 M6-1a).
///
/// It is the "remote one" the seam was left for: [`AgentHandle`] is exactly this trait, so this handle
/// drops into a [`LocalDispatcher`] with no change above it (roadmap §5's "hand a task to a node by
/// name"). `run` is **synchronous**, like every other handle and like `/v0/tasks` itself (v0.9 E0: the
/// answer is the outcome, and there is no task table to poll): the handle sends one frame, waits for the
/// peer's reply, and reports it — or reports that no reply came.
///
/// **What it does not do.** It does not retry, queue, reorder or schedule: §5's red line is that those are
/// policy. One frame out, one answer or none, and the caller judges.
pub struct RemoteAgentHandle {
    /// The identity a task must target to reach this handle.
    agent_id: AgentId,
    /// The node the task is handed to.
    peer: String,
    /// How the frame leaves this node.
    send: TaskSender,
    /// Where the peer's reply lands.
    replies: TaskReplies,
    /// How long to wait for it.
    timeout: Duration,
}

impl RemoteAgentHandle {
    /// A handle for `peer`, known as `agent_id`, sending through `send` and reading `replies`.
    pub fn new(
        agent_id: AgentId,
        peer: impl Into<String>,
        send: TaskSender,
        replies: TaskReplies,
        timeout: Duration,
    ) -> Self {
        Self {
            agent_id,
            peer: peer.into(),
            send,
            replies,
            timeout,
        }
    }

    /// Take this task's reply, when one has landed.
    fn take_reply(&self, task_id: &TaskId) -> Option<Result<TaskOutcome, String>> {
        self.replies
            .lock()
            .ok()
            .and_then(|mut held| held.remove(task_id))
    }
}

impl AgentHandle for RemoteAgentHandle {
    fn agent_id(&self) -> &AgentId {
        &self.agent_id
    }

    /// Hand the task to the peer, and wait for its answer.
    ///
    /// Four refusals are distinguishable by their message, and none of them needs its own error variant:
    /// **unreachable** (the frame could not leave), **unauthorised** and **failed** (both composed by the
    /// peer, which is the only side that knows which it was), and **timed out** (nothing came back).
    fn run(&self, task: &Task) -> Result<TaskOutcome, DispatchError> {
        // Standing alone, this handle answers only for its own identity — the same guard every handle has.
        if task.target != self.agent_id {
            return Err(DispatchError::NoSuchAgent(task.target.clone()));
        }
        let frame = net::TaskFrame {
            task_id: task.id.as_str().to_string(),
            target: task.target.as_str().to_string(),
            input: task.input.clone(),
            sandbox: task.sandbox.clone(),
            instance: task.instance.as_ref().map(|id| id.as_str().to_string()),
        };
        (self.send)(&self.peer, &frame)
            .map_err(|error| DispatchError::Failed(format!("the peer is unreachable: {error}")))?;

        let deadline = Instant::now() + self.timeout;
        loop {
            if let Some(reply) = self.take_reply(&task.id) {
                return match reply {
                    Ok(outcome) => Ok(outcome),
                    // The peer's own words: it knows whether it refused or failed.
                    Err(text) => Err(DispatchError::Failed(text)),
                };
            }
            if Instant::now() >= deadline {
                return Err(DispatchError::Failed(format!(
                    "the peer did not answer in {} s",
                    self.timeout.as_secs()
                )));
            }
            std::thread::sleep(REPLY_POLL);
        }
    }
}

/// `AgentOutcomeView` back into `AgentOutcome`.
///
/// Lossless in both directions: the view keeps every field the outcome has
/// (`kind`, `content`, `reason`, `iterations`), so a dispatched task reports the
/// same outcome a direct `run_agent` call does. Public since v0.8 (main
/// deliverable 1/2): the out-of-process worker reuses this one mapping instead of
/// keeping a second copy of it.
pub fn outcome_from_view(view: AgentOutcomeView) -> AgentOutcome {
    match view.kind.as_str() {
        "final" => AgentOutcome::Final {
            content: view.content.unwrap_or_default(),
            iterations: view.iterations,
        },
        "max_iterations" => AgentOutcome::MaxIterations {
            last_content: view.content.unwrap_or_default(),
            iterations: view.iterations,
        },
        // `failed`, and anything a future host adds: an unknown kind is a failure
        // with a reason that names it, never a silent success.
        kind => AgentOutcome::Failed {
            reason: view
                .reason
                .unwrap_or_else(|| format!("unknown outcome kind: {kind}")),
            iterations: view.iterations,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a_task() -> Task {
        Task {
            id: TaskId::new("task-dev-a-1-1"),
            target: AgentId::new("dev-b"),
            input: "hello".to_string(),
            sandbox: None,
            instance: None,
        }
    }

    fn an_outcome(task_id: &TaskId) -> TaskOutcome {
        TaskOutcome {
            task_id: task_id.clone(),
            agent_id: AgentId::new("dev-b"),
            outcome: AgentOutcome::Final {
                content: "done".to_string(),
                iterations: 1,
            },
        }
    }

    /// A handle with a slot and a sender the test owns; the sender records what it carried.
    fn a_handle(send: TaskSender, replies: TaskReplies, timeout: Duration) -> RemoteAgentHandle {
        RemoteAgentHandle::new(AgentId::new("dev-b"), "dev-b", send, replies, timeout)
    }

    #[test]
    fn a_reply_is_the_peers_outcome() {
        let replies: TaskReplies = Arc::new(Mutex::new(HashMap::new()));
        let task = a_task();
        replies
            .lock()
            .expect("the slot")
            .insert(task.id.clone(), Ok(an_outcome(&task.id)));
        let sent = Arc::new(Mutex::new(Vec::new()));
        let recorded = Arc::clone(&sent);
        let handle = a_handle(
            Arc::new(move |peer, frame| {
                recorded
                    .lock()
                    .expect("the log")
                    .push((peer.to_string(), frame.clone()));
                Ok(())
            }),
            Arc::clone(&replies),
            Duration::from_secs(1),
        );

        let outcome = handle.run(&task).expect("the peer's answer");
        assert_eq!(outcome.task_id, task.id);
        assert_eq!(outcome.agent_id.as_str(), "dev-b");
        // It went out as one frame, to the peer, carrying the task's own fields.
        let log = sent.lock().expect("the log");
        assert_eq!(log.len(), 1);
        assert_eq!(log[0].0, "dev-b");
        assert_eq!(log[0].1.task_id, "task-dev-a-1-1");
        assert_eq!(log[0].1.input, "hello");
        // And the reply was taken, not left for the next task to find.
        assert!(replies.lock().expect("the slot").is_empty());
    }

    #[test]
    fn a_refusal_from_the_peer_is_its_own_message() {
        let replies: TaskReplies = Arc::new(Mutex::new(HashMap::new()));
        let task = a_task();
        replies.lock().expect("the slot").insert(
            task.id.clone(),
            Err("the peer did not authorise dispatch".to_string()),
        );
        let handle = a_handle(
            Arc::new(|_, _| Ok(())),
            Arc::clone(&replies),
            Duration::from_secs(1),
        );
        let error = handle.run(&task).expect_err("refused");
        assert!(error.to_string().contains("did not authorise"), "{error}");
    }

    #[test]
    fn a_frame_that_cannot_leave_is_unreachable() {
        let replies: TaskReplies = Arc::new(Mutex::new(HashMap::new()));
        let handle = a_handle(
            Arc::new(|_, _| Err("connect refused".to_string())),
            replies,
            Duration::from_secs(1),
        );
        let error = handle.run(&a_task()).expect_err("unreachable");
        assert!(
            error.to_string().contains("the peer is unreachable"),
            "{error}"
        );
    }

    #[test]
    fn no_answer_is_a_timeout_and_not_a_message() {
        let replies: TaskReplies = Arc::new(Mutex::new(HashMap::new()));
        let handle = a_handle(Arc::new(|_, _| Ok(())), replies, Duration::from_secs(1));
        let error = handle.run(&a_task()).expect_err("no answer");
        assert!(
            error.to_string().contains("did not answer in 1 s"),
            "{error}"
        );
    }

    #[test]
    fn a_task_for_somebody_else_is_a_lookup_miss() {
        let replies: TaskReplies = Arc::new(Mutex::new(HashMap::new()));
        let handle = a_handle(Arc::new(|_, _| Ok(())), replies, Duration::from_secs(1));
        let mut task = a_task();
        task.target = AgentId::new("somebody-else");
        assert!(matches!(
            handle.run(&task),
            Err(DispatchError::NoSuchAgent(_))
        ));
    }
}
