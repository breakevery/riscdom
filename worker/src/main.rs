//! The executor process (v0.8 main deliverable 1/2).
//!
//! One task in, one outcome out. It reads a single `Task` as one JSON line on
//! **stdin**, runs it through the host's `run_agent` path, and writes one
//! `TaskOutcome` as one JSON line on **stdout**. Its events (JSON lines) go to
//! **stderr**, so stdout stays a pure protocol channel a supervisor can parse
//! without filtering.
//!
//! ```text
//! worker --workspace <dir> --data-dir <dir> [--sleep-ms <n>]
//! ```
//!
//! - `--workspace` / `--data-dir` are required and have no environment fallback:
//!   an executor's identity is its command line, so two executors sharing a
//!   workspace still get their own `settings.json`, `sessions.db` and toolchain
//!   directory (`AppState::with_data_dir`). Environment variables would be
//!   inherited silently and could make two workers collide without saying so.
//! - `--sleep-ms` is a diagnostic hook (tests and manual timing): wait this long
//!   after reading the task, before answering. It changes nothing else.
//!
//! Exit code: **0 whenever a `TaskOutcome` was written**, including a failed run —
//! the outcome is the answer, not the exit status. A usage error exits 2 and
//! writes no stdout line; the supervisor reports that as a protocol failure.

use agent::{AgentId, AgentOutcome, Task, TaskOutcome};
use host::state::AppState;
use host::EventSink;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

/// Identities a `TaskOutcome` carries when the task itself could not be read.
///
/// A malformed request still gets an answer: the supervisor learns "your task was
/// not readable" instead of waiting for a line that will never come. The
/// placeholder names are stable and documented (`task-unparsed` / `unparsed`) so
/// the failure is recognisable on the wire.
const UNPARSED_TASK_ID: &str = "task-unparsed";
const UNPARSED_AGENT_ID: &str = "unparsed";

const USAGE: &str = "usage: worker --workspace <dir> --data-dir <dir> [--sleep-ms <n>]";

/// The worker's command line.
#[derive(Debug, PartialEq, Eq)]
struct Args {
    workspace: PathBuf,
    data_dir: PathBuf,
    sleep: Option<Duration>,
}

impl Args {
    fn parse(argv: impl IntoIterator<Item = String>) -> Result<Self, String> {
        let mut workspace: Option<PathBuf> = None;
        let mut data_dir: Option<PathBuf> = None;
        let mut sleep: Option<Duration> = None;

        let mut args = argv.into_iter();
        while let Some(flag) = args.next() {
            let mut value = || {
                args.next()
                    .ok_or_else(|| format!("{flag} needs a value\n{USAGE}"))
            };
            match flag.as_str() {
                "--workspace" => workspace = Some(PathBuf::from(value()?)),
                "--data-dir" => data_dir = Some(PathBuf::from(value()?)),
                "--sleep-ms" => {
                    let raw = value()?;
                    let ms: u64 = raw
                        .parse()
                        .map_err(|e| format!("--sleep-ms {raw:?} is not a number: {e}\n{USAGE}"))?;
                    sleep = Some(Duration::from_millis(ms));
                }
                other => return Err(format!("unknown argument {other:?}\n{USAGE}")),
            }
        }

        Ok(Self {
            workspace: workspace.ok_or_else(|| format!("--workspace is required\n{USAGE}"))?,
            data_dir: data_dir.ok_or_else(|| format!("--data-dir is required\n{USAGE}"))?,
            sleep,
        })
    }
}

/// Events as JSON lines on **stderr**.
///
/// stdout belongs to the single `TaskOutcome` line. Every event line names the
/// agent that caused it (v0.8 batch B's identity), which is how a supervisor can
/// tell which executor produced what after the fact.
struct LineEventSink {
    agent_id: String,
}

impl EventSink for LineEventSink {
    fn emit(&self, event: &str, payload: serde_json::Value) {
        // The one envelope every transport uses (v0.9). `worker:ready` and
        // `worker:done` are this process's own protocol events and travel in it
        // too — as `kind: "event"`, so a supervisor parses all lines one way.
        let line = host::events::event_envelope(event, &self.agent_id, payload);
        eprintln!("{}", line.to_json());
    }
}

fn main() {
    let args = match Args::parse(std::env::args().skip(1)) {
        Ok(args) => args,
        Err(message) => {
            // No task was read, so there is no outcome to answer with: stderr and
            // a non-zero exit is the honest answer (see the module docs).
            eprintln!("worker: {message}");
            std::process::exit(2);
        }
    };

    let task = read_task();
    if let Some(sleep) = args.sleep {
        std::thread::sleep(sleep);
    }
    let outcome = run(&args, task.as_ref());
    write_outcome(&outcome);
}

/// Read exactly one JSON line from stdin. `None` when it cannot be read as a
/// `Task` — the caller answers with a `Failed` outcome.
fn read_task() -> Option<Task> {
    let mut line = String::new();
    match std::io::stdin().read_to_string(&mut line) {
        Ok(_) => {}
        Err(e) => {
            eprintln!("worker: cannot read the task from stdin: {e}");
            return None;
        }
    }
    let raw = line.trim();
    if raw.is_empty() {
        eprintln!("worker: no task on stdin");
        return None;
    }
    match serde_json::from_str::<Task>(raw) {
        Ok(task) => Some(task),
        Err(e) => {
            eprintln!("worker: the task is not readable as JSON: {e}");
            None
        }
    }
}

/// Run one task through the host's own path, mapping every failure to an outcome.
fn run(args: &Args, task: Option<&Task>) -> TaskOutcome {
    let (task_id, agent_id) = match task {
        // A malformed request still gets an answer (see the constants above).
        None => (
            agent::TaskId::new(UNPARSED_TASK_ID),
            AgentId::new(UNPARSED_AGENT_ID),
        ),
        Some(task) => (task.id.clone(), task.target.clone()),
    };

    let Some(task) = task else {
        return task_outcome(
            task_id,
            agent_id,
            AgentOutcome::Failed {
                reason: "the task could not be read: not a JSON Task".into(),
                iterations: 0,
            },
        );
    };

    let state = match AppState::with_data_dir(&args.workspace, &args.data_dir) {
        Ok(state) => state,
        Err(e) => {
            return task_outcome(
                task_id,
                agent_id,
                AgentOutcome::Failed {
                    reason: format!("cannot start the executor state: {e}"),
                    iterations: 0,
                },
            );
        }
    };

    // The identity the events carry is this process's own (minted here by v0.8
    // batch B), and it is the one reported back in the outcome.
    let agent_id = AgentId::new(state.agent_id());
    let sink = Arc::new(LineEventSink {
        agent_id: agent_id.to_string(),
    });
    sink.emit(
        "worker:ready",
        serde_json::json!({ "task_id": task.id.to_string() }),
    );

    let outcome = match state.run_agent(Arc::clone(&sink) as Arc<dyn EventSink>, &task.input) {
        Ok(view) => host::dispatch::outcome_from_view(view),
        // A host-level refusal (not ready, no toolchain, no QEMU, …) is an
        // outcome, not a crash: the supervisor sees it as data.
        Err(e) => AgentOutcome::Failed {
            reason: e.user_message(),
            iterations: 0,
        },
    };
    sink.emit(
        "worker:done",
        serde_json::json!({ "task_id": task.id.to_string() }),
    );
    task_outcome(task_id, agent_id, outcome)
}

fn task_outcome(task_id: agent::TaskId, agent_id: AgentId, outcome: AgentOutcome) -> TaskOutcome {
    TaskOutcome {
        task_id,
        agent_id,
        outcome,
    }
}

/// Write the outcome as the single stdout line.
fn write_outcome(outcome: &TaskOutcome) {
    match serde_json::to_string(outcome) {
        Ok(line) => {
            let stdout = std::io::stdout();
            let mut stdout = stdout.lock();
            if let Err(e) = stdout
                .write_all(line.as_bytes())
                .and_then(|()| stdout.write_all(b"\n"))
            {
                eprintln!("worker: cannot write the outcome: {e}");
                std::process::exit(1);
            }
            let _ = stdout.flush();
        }
        Err(e) => {
            // Cannot happen for these plain fields, but never answer with silence.
            eprintln!("worker: cannot serialise the outcome: {e}");
            std::process::exit(1);
        }
    }
}
