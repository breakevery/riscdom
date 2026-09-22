//! Minimal task dispatch (v0.8 batch C).
//!
//! The shape the architecture note asks for in §7 seam 2: a supervisor hands a
//! [`Task`] to an **executor handle**, and whether that handle is in this process
//! or on another machine is none of the supervisor's business.
//!
//! What is here: the types, the two traits ([`AgentHandle`], [`Dispatcher`]) and a
//! **local** dispatcher. What is deliberately absent: any remote implementation.
//! That absence is the seam — a `RemoteAgentHandle` would implement
//! [`AgentHandle`] over IPC or a socket and plug into the very same
//! [`LocalDispatcher`], with no change above it. Leaving the trait without that
//! implementation is the point, not an oversight.
//!
//! Nothing here depends on Tauri or the host: this module lives in `agent` so the
//! abstraction survives the host-core / host-tauri split instead of forcing it.

use crate::agent::{AgentLoop, AgentOutcome};
use crate::error::AgentError;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// Who a task is for. The same shape the audit chain records
/// (`crate::identity::next_agent_id`): `<device>-<pid>-<seq>`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct AgentId(String);

impl AgentId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for AgentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// A task's identity: `task-<pid>-<seq>`.
///
/// A process-wide counter with the pid in it, like [`crate::identity`]: unique
/// inside a process and across processes, monotonic, and cheap. The type is a
/// newtype so the day a task crosses a machine the wire format is one decision in
/// one place (a `Uuid` would do as well; nothing else would change).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct TaskId(String);

static TASK_SEQ: AtomicU64 = AtomicU64::new(0);

impl TaskId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// The next id in this process.
    pub fn next() -> Self {
        let seq = TASK_SEQ.fetch_add(1, Ordering::Relaxed) + 1;
        Self(format!("task-{}-{seq}", std::process::id()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for TaskId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// One unit of work: what to do, and who should do it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Task {
    pub id: TaskId,
    pub target: AgentId,
    pub input: String,
}

impl Task {
    /// A task for `target`, with a fresh [`TaskId`].
    pub fn new(target: AgentId, input: impl Into<String>) -> Self {
        Self {
            id: TaskId::next(),
            target,
            input: input.into(),
        }
    }
}

/// What a dispatched task produced.
///
/// Serialisable since v0.8 (main deliverable 1/2): this is the record a worker
/// process writes back as one JSON line.
///
/// `agent_id` is the **executor that actually ran the task** — its own identity,
/// which the handle fills in. It need not equal the `Task.target` the supervisor
/// routed by: a task addressed to a label lands on a child process whose identity
/// (`local-<pid>-<seq>`) the supervisor could not have known in advance. Before
/// v0.8 main deliverable 3/3 the dispatcher stamped the *target* here, so a
/// supervisor could not tell who had done the work.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaskOutcome {
    pub task_id: TaskId,
    /// The identity of the executor that ran it (not the target it was sent to).
    pub agent_id: AgentId,
    /// The executor's own outcome, unchanged: the supervisor reads runs, not a
    /// second, dispatch-shaped vocabulary for the same thing.
    pub outcome: AgentOutcome,
}

/// Why a dispatch produced no outcome.
///
/// Serialisable since v0.8 (main deliverable 1/2). `#[error(...)]` is
/// `thiserror`'s; serde ignores it and tags the variants by name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
pub enum DispatchError {
    /// No handle in this dispatcher owns `task.target`.
    #[error("no executor for agent {0}")]
    NoSuchAgent(AgentId),
    /// The executor was reached but the run failed.
    #[error("the executor failed: {0}")]
    Failed(String),
}

/// An executor: "run this task, give me the outcome".
///
/// A remote implementation (another process, another machine) implements exactly
/// this trait. **None is written yet** — that is the seam (v0.8 batch C).
pub trait AgentHandle: Send + Sync {
    /// The identity a task must target to reach this handle.
    fn agent_id(&self) -> &AgentId;

    /// Run one task and report it, **filling in the identity of the executor that
    /// ran it**. `&self`: a handle is shared, so any mutable state it needs lives
    /// inside it.
    ///
    /// The handle owns the `TaskOutcome` assembly because only the handle knows
    /// who the executor really is: for an in-process loop that is its own id, for
    /// a child process it is the id the child announces.
    fn run(&self, task: &Task) -> Result<TaskOutcome, DispatchError>;
}

/// Anything that can turn a [`Task`] into a [`TaskOutcome`].
pub trait Dispatcher: Send + Sync {
    fn dispatch(&self, task: Task) -> Result<TaskOutcome, DispatchError>;
}

/// The local dispatcher: the handles in **this** process, found by identity.
///
/// Lookup is by `task.target`, so a task for an agent this process does not hold
/// is a [`DispatchError::NoSuchAgent`] rather than a panic or a wrong run.
pub struct LocalDispatcher {
    handles: Vec<Arc<dyn AgentHandle>>,
}

impl LocalDispatcher {
    pub fn new(handles: Vec<Arc<dyn AgentHandle>>) -> Self {
        Self { handles }
    }

    /// The identities this process can dispatch to.
    pub fn agent_ids(&self) -> Vec<&AgentId> {
        self.handles
            .iter()
            .map(|handle| handle.agent_id())
            .collect()
    }
}

impl Dispatcher for LocalDispatcher {
    /// Route by `Task.target`, then **pass the handle's own outcome through**.
    ///
    /// The dispatcher no longer assembles a [`TaskOutcome`]: it does not know the
    /// executor's identity (a child process has one the dispatcher never sees) and
    /// must not invent one. Its only judgement left is the route itself.
    fn dispatch(&self, task: Task) -> Result<TaskOutcome, DispatchError> {
        let handle = self
            .handles
            .iter()
            .find(|handle| *handle.agent_id() == task.target)
            .ok_or_else(|| DispatchError::NoSuchAgent(task.target.clone()))?;
        handle.run(&task)
    }
}

/// A local executor: one [`AgentLoop`] behind a mutex.
///
/// `AgentLoop::run` takes `&mut self` and a handle is shared behind `&self`, which
/// is the whole reason for the mutex — no other state is added.
pub struct LocalAgent {
    agent_id: AgentId,
    agent: Mutex<AgentLoop>,
}

impl LocalAgent {
    /// Wrap a loop; the handle takes the loop's own identity.
    pub fn new(agent: AgentLoop) -> Self {
        let agent_id = AgentId::new(agent.agent_id().to_string());
        Self {
            agent_id,
            agent: Mutex::new(agent),
        }
    }
}

impl AgentHandle for LocalAgent {
    fn agent_id(&self) -> &AgentId {
        &self.agent_id
    }

    fn run(&self, task: &Task) -> Result<TaskOutcome, DispatchError> {
        let mut agent = self
            .agent
            .lock()
            .map_err(|_| DispatchError::Failed("agent loop mutex poisoned".into()))?;
        let outcome = agent
            .run(&task.input)
            .map_err(|error: AgentError| DispatchError::Failed(error.to_string()))?;
        // An in-process executor is its own identity: there is nobody else it
        // could be.
        Ok(TaskOutcome {
            task_id: task.id.clone(),
            agent_id: self.agent_id.clone(),
            outcome,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_wire_types_survive_a_json_round_trip() {
        let task = Task::new(AgentId::new("local-1-1"), "say hi");
        let json = serde_json::to_string(&task).expect("task json");
        assert_eq!(serde_json::from_str::<Task>(&json).expect("task"), task);

        for outcome in [
            AgentOutcome::Final {
                content: "done".into(),
                iterations: 3,
            },
            AgentOutcome::MaxIterations {
                last_content: "partial".into(),
                iterations: 9,
            },
            AgentOutcome::Failed {
                reason: "boom".into(),
                iterations: 0,
            },
        ] {
            let wire = TaskOutcome {
                task_id: task.id.clone(),
                agent_id: task.target.clone(),
                outcome,
            };
            let json = serde_json::to_string(&wire).expect("outcome json");
            assert_eq!(
                serde_json::from_str::<TaskOutcome>(&json).expect("outcome"),
                wire,
                "every outcome variant survives the wire: {json}"
            );
        }

        for error in [
            DispatchError::NoSuchAgent(AgentId::new("elsewhere-1-1")),
            DispatchError::Failed("boom".into()),
        ] {
            let json = serde_json::to_string(&error).expect("error json");
            assert_eq!(
                serde_json::from_str::<DispatchError>(&json).expect("error"),
                error,
                "every dispatch error survives the wire: {json}"
            );
        }
    }

    #[test]
    fn task_ids_are_unique_and_ordered_by_creation() {
        let first = TaskId::next();
        let second = TaskId::next();
        assert_ne!(first, second);
        assert_ne!(first, TaskId::new("task-0-1"));
        assert!(first.as_str().starts_with("task-"));
        assert!(first < second, "the counter is monotonic");
    }

    #[test]
    fn a_task_carries_its_target_and_its_own_id() {
        let target = AgentId::new("local-1-1");
        let task = Task::new(target.clone(), "write hello world");
        assert_eq!(task.target, target);
        assert_eq!(task.input, "write hello world");
        assert_ne!(
            Task::new(target, "x").id,
            task.id,
            "every task mints its own id"
        );
    }
}
