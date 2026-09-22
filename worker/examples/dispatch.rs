//! The runnable supervisor: dispatch a task list to a fleet of executor processes.
//!
//! ```text
//! cargo build -p worker                       # the executor binary must exist first
//! cargo run  -p worker --example dispatch     # two executors, one demo task each
//! cargo run  -p worker --example dispatch -- --tasks tasks.jsonl --executors 3
//! ```
//!
//! Options:
//!
//! - `--executors <n>` — how many executor processes to start (default 2). They
//!   **share one workspace** (that is the interesting case: one audit chain,
//!   per-agent snapshots) and each gets its **own data dir**.
//! - `--base <dir>` — where the shared workspace and the per-executor data dirs
//!   live (default: a fresh `riscdom-dispatch-<pid>` directory under the system
//!   temp dir).
//! - `--worker <path>` — the executor binary (default: `worker` next to this
//!   example, i.e. `target/<profile>/worker`).
//! - `--tasks <file>` — a JSON-lines task list; `-` reads stdin. Without it the
//!   example sends one demo task to each executor, so it runs with no input.
//!
//! Nothing here is an AI: the supervisor reads tasks, routes each to the executor
//! named in `Task.target`, and prints what came back. An executor with no LLM
//! configured will answer `Failed` — the plumbing is what this demo shows.

use agent::{AgentId, Task};
use std::io::Read;
use std::path::PathBuf;
use worker::supervisor::{dispatch_all, dispatcher, parse_tasks, report, ExecutorSpec};

fn main() {
    let options = match Options::parse(std::env::args().skip(1)) {
        Ok(options) => options,
        Err(message) => {
            eprintln!("dispatch: {message}");
            std::process::exit(2);
        }
    };

    let base = options.base.clone();
    if let Err(e) = std::fs::create_dir_all(&base) {
        eprintln!("dispatch: cannot create {}: {e}", base.display());
        std::process::exit(1);
    }
    let workspace = base.join("workspace");

    // One fleet: every executor shares the workspace, none shares a data dir.
    let specs: Vec<ExecutorSpec> = (0..options.executors)
        .map(|index| {
            let data_dir = base.join(format!("data-{index}"));
            ExecutorSpec::new(
                format!("executor-{index}"),
                options.worker.clone(),
                vec![
                    "--workspace".into(),
                    workspace.display().to_string(),
                    "--data-dir".into(),
                    data_dir.display().to_string(),
                ],
            )
        })
        .collect();

    println!("supervisor: a dispatcher, not an agent (no model is called)");
    println!("workspace : {}", workspace.display());
    for spec in &specs {
        println!(
            "executor  : {} (own data dir, shared workspace)",
            spec.label
        );
    }

    let tasks = match options.tasks() {
        Ok(text) => match parse_tasks(&text) {
            Ok(tasks) if !tasks.is_empty() => tasks,
            Ok(_) => demo_tasks(&specs),
            Err(e) => {
                eprintln!("dispatch: cannot read the task list — {e}");
                std::process::exit(2);
            }
        },
        Err(e) => {
            eprintln!("dispatch: cannot read the task list — {e}");
            std::process::exit(2);
        }
    };
    println!("tasks     : {}", tasks.len());

    let fleet = dispatcher(&specs);
    let outcomes = dispatch_all(&fleet, tasks);
    println!("\n{}", report(&outcomes));
}

/// One task per executor, so the demo runs with no input at all.
fn demo_tasks(specs: &[ExecutorSpec]) -> Vec<Task> {
    specs
        .iter()
        .map(|spec| Task::new(AgentId::new(spec.label.clone()), "say hi"))
        .collect()
}

struct Options {
    executors: usize,
    base: PathBuf,
    worker: PathBuf,
    tasks: Option<String>,
}

impl Options {
    fn parse(argv: impl IntoIterator<Item = String>) -> Result<Self, String> {
        let mut executors = 2usize;
        let mut base: Option<PathBuf> = None;
        let mut worker: Option<PathBuf> = None;
        let mut tasks: Option<String> = None;

        let mut args = argv.into_iter();
        while let Some(flag) = args.next() {
            let mut value = || args.next().ok_or_else(|| format!("{flag} needs a value"));
            match flag.as_str() {
                "--executors" => {
                    let raw = value()?;
                    executors = raw
                        .parse()
                        .map_err(|e| format!("--executors {raw:?} is not a number: {e}"))?;
                    if executors == 0 {
                        return Err("--executors must be at least 1".into());
                    }
                }
                "--base" => base = Some(PathBuf::from(value()?)),
                "--worker" => worker = Some(PathBuf::from(value()?)),
                "--tasks" => tasks = Some(value()?),
                other => return Err(format!("unknown argument {other:?}")),
            }
        }

        let base = base.unwrap_or_else(|| {
            std::env::temp_dir().join(format!("riscdom-dispatch-{}", std::process::id()))
        });
        Ok(Self {
            executors,
            base,
            worker: worker.unwrap_or_else(default_worker),
            tasks,
        })
    }

    /// The task list as text: a file, stdin for `-`, or the demo tasks when the
    /// flag was not given at all.
    fn tasks(&self) -> Result<String, String> {
        match self.tasks.as_deref() {
            None => Ok(String::new()),
            Some("-") => {
                let mut text = String::new();
                std::io::stdin()
                    .read_to_string(&mut text)
                    .map_err(|e| format!("stdin: {e}"))?;
                Ok(text)
            }
            Some(path) => std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}")),
        }
    }
}

/// The executor binary cargo built next to this example: `target/<profile>/worker`.
///
/// Examples do not get `CARGO_BIN_EXE_*` (only integration tests do), so the path
/// is derived from this example's own location — `.../examples/dispatch` sits one
/// directory below the binary. `--worker` overrides it.
fn default_worker() -> PathBuf {
    let name = format!("worker{}", std::env::consts::EXE_SUFFIX);
    std::env::current_exe()
        .ok()
        .and_then(|exe| {
            exe.parent()
                .and_then(|dir| dir.parent())
                .map(|dir| dir.join(&name))
        })
        .unwrap_or_else(|| {
            eprintln!("dispatch: cannot locate the worker binary; pass --worker <path>");
            PathBuf::from(name)
        })
}
