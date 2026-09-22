//! A non-AI supervisor: dispatch a task list to a fleet of executor processes.
//!
//! There is **no model anywhere in this module**. For this stage the architecture's
//! "supervisor" is a *dispatcher*: it reads tasks, hands each to the dispatcher that
//! routes by `Task.target`, and reports what came back. An LLM supervisor is a
//! separate design (its own loop, prompts and budget policy) and is not attempted
//! here.
//!
//! Routing is explicit and never guessed: a task names the executor it wants
//! (`Task.target` == that executor's label), and a task naming something not in the
//! fleet is refused with `DispatchError::NoSuchAgent` rather than handed to
//! whichever executor happens to be free. Sending a task to the wrong executor is
//! worse than not sending it.

use agent::{
    AgentHandle, AgentId, AgentOutcome, DispatchError, Dispatcher, LocalDispatcher, Task,
    TaskOutcome,
};
use host::StdioExecutorHandle;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

/// One executor in the fleet: what to run, and the label tasks address it by.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutorSpec {
    /// The identity tasks use in `Task.target` (and the label the supervisor
    /// addresses the handle by). The child mints its **own** identity; see
    /// `docs/multi-agent-foundation.md`.
    pub label: String,
    pub program: PathBuf,
    pub args: Vec<String>,
    /// Per-executor answer deadline; `None` means the handle's default.
    pub timeout: Option<Duration>,
    /// Environment entries applied to the child: `Some(value)` sets, `None`
    /// removes. A supervisor decides what its executors inherit.
    pub env: Vec<(String, Option<String>)>,
}

impl ExecutorSpec {
    pub fn new(label: impl Into<String>, program: impl Into<PathBuf>, args: Vec<String>) -> Self {
        Self {
            label: label.into(),
            program: program.into(),
            args,
            timeout: None,
            env: Vec::new(),
        }
    }

    /// Override this executor's answer deadline.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// Set one environment variable on this executor's child process.
    pub fn with_env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.push((key.into(), Some(value.into())));
        self
    }

    /// Remove one environment variable from this executor's child process.
    pub fn with_env_removed(mut self, key: impl Into<String>) -> Self {
        self.env.push((key.into(), None));
        self
    }

    /// The handle this spec describes.
    pub fn handle(&self) -> Arc<StdioExecutorHandle> {
        let mut handle = StdioExecutorHandle::new(
            AgentId::new(self.label.clone()),
            self.program.clone(),
            self.args.clone(),
        );
        if let Some(timeout) = self.timeout {
            handle = handle.with_timeout(timeout);
        }
        for (key, value) in &self.env {
            handle = match value {
                Some(value) => handle.with_env(key.clone(), value.clone()),
                None => handle.with_env_removed(key.clone()),
            };
        }
        Arc::new(handle)
    }
}

/// A dispatcher over `specs`, one stdio handle per executor.
pub fn dispatcher(specs: &[ExecutorSpec]) -> LocalDispatcher {
    LocalDispatcher::new(
        specs
            .iter()
            .map(|spec| spec.handle() as Arc<dyn AgentHandle>)
            .collect(),
    )
}

/// One task's fate.
#[derive(Debug)]
pub struct PlanOutcome {
    pub task: Task,
    pub result: Result<TaskOutcome, DispatchError>,
}

impl PlanOutcome {
    /// Whether an executor answered (a refused or broken dispatch did not).
    pub fn answered(&self) -> bool {
        self.result.is_ok()
    }
}

/// Dispatch every task **concurrently** — one thread per task — and return the
/// results in input order.
///
/// Concurrency is the point of a fleet: several executors work at once, and one
/// slow executor must not hold up the rest. `std::thread::scope` keeps it
/// dependency-free (the handles are `Send + Sync`, so one dispatcher is shared),
/// and a thread that panics becomes a failed outcome instead of taking the whole
/// plan down.
pub fn dispatch_all(dispatcher: &LocalDispatcher, tasks: Vec<Task>) -> Vec<PlanOutcome> {
    std::thread::scope(|scope| {
        let mut pending = Vec::with_capacity(tasks.len());
        for task in tasks {
            let sent = task.clone();
            pending.push((task, scope.spawn(move || dispatcher.dispatch(sent))));
        }
        pending
            .into_iter()
            .map(|(task, handle)| PlanOutcome {
                result: handle.join().unwrap_or_else(|_| {
                    Err(DispatchError::Failed(
                        "the dispatch thread panicked before answering".into(),
                    ))
                }),
                task,
            })
            .collect()
    })
}

/// How a plan ended, by kind of outcome.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Tally {
    /// Executors answered (whatever the outcome inside says).
    pub answered: usize,
    /// No executor held the task's target.
    pub refused: usize,
    /// The executor was reached but the exchange failed.
    pub broken: usize,
}

pub fn tally(outcomes: &[PlanOutcome]) -> Tally {
    let mut tally = Tally::default();
    for outcome in outcomes {
        match &outcome.result {
            Ok(_) => tally.answered += 1,
            Err(DispatchError::NoSuchAgent(_)) => tally.refused += 1,
            Err(DispatchError::Failed(_)) => tally.broken += 1,
        }
    }
    tally
}

/// The supervisor's report: one line per task, then the tally.
pub fn report(outcomes: &[PlanOutcome]) -> String {
    let mut lines: Vec<String> = outcomes
        .iter()
        .map(|outcome| {
            let detail = match &outcome.result {
                Ok(wire) => format!("answered [{}]", describe(&wire.outcome)),
                Err(DispatchError::NoSuchAgent(agent)) => {
                    format!("refused: no executor for {agent}")
                }
                Err(DispatchError::Failed(message)) => format!("broken: {message}"),
            };
            format!("{} -> {}: {detail}", outcome.task.target, outcome.task.id)
        })
        .collect();
    let tally = tally(outcomes);
    lines.push(format!(
        "tally: {} answered, {} refused, {} broken (of {})",
        tally.answered,
        tally.refused,
        tally.broken,
        outcomes.len()
    ));
    lines.join("\n")
}

/// One outcome, in words.
fn describe(outcome: &AgentOutcome) -> String {
    match outcome {
        AgentOutcome::Final { iterations, .. } => {
            format!("final answer after {iterations} iterations")
        }
        AgentOutcome::MaxIterations { iterations, .. } => {
            format!("iteration cap reached ({iterations})")
        }
        AgentOutcome::Failed { reason, .. } => format!("failed: {reason}"),
    }
}

/// Parse a task list: one `Task` JSON object per line — JSON lines, the same
/// framing the executor protocol uses.
///
/// Blank lines are skipped so a hand-written file can breathe. A line that does
/// not parse fails the whole list, naming the line: dispatching the readable half
/// of a mistyped list would hide the typo.
pub fn parse_tasks(text: &str) -> Result<Vec<Task>, String> {
    let mut tasks = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        match serde_json::from_str::<Task>(line) {
            Ok(task) => tasks.push(task),
            Err(e) => return Err(format!("line {}: {e}", index + 1)),
        }
    }
    Ok(tasks)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_task_list_is_one_json_object_per_line() {
        let first = Task::new(AgentId::new("a"), "one");
        let second = Task::new(AgentId::new("b"), "two");
        let text = format!(
            "\n{}\n\n{}\n",
            serde_json::to_string(&first).unwrap(),
            serde_json::to_string(&second).unwrap()
        );
        let parsed = parse_tasks(&text).expect("parse");
        assert_eq!(
            parsed,
            vec![first.clone(), second],
            "blank lines are skipped"
        );

        let empty = parse_tasks("\n  \n").expect("parse");
        assert!(empty.is_empty(), "an empty list is an empty list");

        let good = serde_json::to_string(&first).unwrap();
        let broken = parse_tasks(&format!("{good}\n not json\n")).expect_err("parse");
        assert!(broken.starts_with("line 2:"), "{broken}");

        // A first line that is not a task at all is reported as line 1.
        let broken = parse_tasks("{\"id\":\"task-1-1\"}\n").expect_err("parse");
        assert!(broken.starts_with("line 1:"), "{broken}");
    }

    #[test]
    fn a_refusal_and_a_break_are_counted_apart() {
        let task = Task::new(AgentId::new("anywhere"), "x");
        let outcomes = vec![
            PlanOutcome {
                task: task.clone(),
                result: Err(DispatchError::NoSuchAgent(AgentId::new("nobody"))),
            },
            PlanOutcome {
                task: task.clone(),
                result: Err(DispatchError::Failed("the executor died".into())),
            },
        ];
        let tally = tally(&outcomes);
        assert_eq!(
            tally,
            Tally {
                answered: 0,
                refused: 1,
                broken: 1
            }
        );
        assert!(!outcomes[0].answered() && !outcomes[1].answered());

        let report = report(&outcomes);
        assert!(
            report.contains("refused: no executor for nobody"),
            "{report}"
        );
        assert!(report.contains("broken: the executor died"), "{report}");
        assert!(
            report.contains("0 answered, 1 refused, 1 broken (of 2)"),
            "{report}"
        );
    }
}
