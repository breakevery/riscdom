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
    AgentHandle, AgentId, AgentOutcome, DispatchError, LocalDispatcher, Task, TaskOutcome,
};
use std::sync::Arc;

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
        let view = self
            .state
            .run_agent_for(
                Arc::clone(&self.emitter),
                &task.input,
                // A task that declares a sandbox runs under it (v0.9 sandbox F2d);
                // one that does not gets the node's own, exactly as before.
                task.sandbox.as_deref(),
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
