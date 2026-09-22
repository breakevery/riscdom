//! The supervisor side of the two-process prototype (v0.8 main deliverable 1/2).
//!
//! [`StdioExecutorHandle`] is an [`AgentHandle`] whose executor is **another
//! process**: it spawns the worker binary, writes one `Task` JSON line into its
//! stdin, and reads one `TaskOutcome` JSON line from its stdout. Nothing above it
//! changes — the dispatcher does not know, and must not care, that this handle
//! crosses a process boundary. That indifference is exactly the seam
//! `agent::dispatch` left open.
//!
//! Why stdio + JSON lines: it needs **no new dependency** (std's `Command` plus
//! the `serde_json` already in the tree), one code path covers Windows, macOS and
//! Linux, and the channel's lifetime is the child's — a worker that dies is an
//! EOF, never a hung read. A long-lived TCP or Unix-domain-socket transport is
//! deliberately not implemented (v0.9).
//!
//! The child's **stderr** is not part of the protocol: it carries the worker's
//! event lines, and this handle drains it into memory so a supervisor can read
//! them ([`StdioExecutorHandle::events`]). stdout stays reserved for the answer.

use agent::{AgentHandle, AgentId, DispatchError, Task, TaskOutcome};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// How long a worker gets to answer before it is killed.
///
/// Generous on purpose: a real run compiles a guest and boots it, so this is a
/// deadlock guard, not a performance budget.
pub const DEFAULT_EXECUTOR_TIMEOUT: Duration = Duration::from_secs(3600);

/// An executor reached over the child process's stdio.
pub struct StdioExecutorHandle {
    /// The identity the **supervisor** addresses this executor by. The child
    /// mints its own (v0.8 batch B) and reports it in the outcome — the two are
    /// deliberately different values, and the one that matters for the audit
    /// trail is the child's.
    agent_id: AgentId,
    program: PathBuf,
    args: Vec<String>,
    /// Environment entries applied to the child, in order: `Some(value)` sets,
    /// `None` removes. A supervisor decides what an executor inherits — in
    /// particular whether it gets the API key at all.
    env: Vec<(String, Option<String>)>,
    timeout: Duration,
    events: Arc<Mutex<Vec<String>>>,
}

impl StdioExecutorHandle {
    /// A handle for `agent_id`, reached by running `program` with `args`.
    pub fn new(agent_id: AgentId, program: impl Into<PathBuf>, args: Vec<String>) -> Self {
        Self {
            agent_id,
            program: program.into(),
            args,
            env: Vec::new(),
            timeout: DEFAULT_EXECUTOR_TIMEOUT,
            events: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Override the answer deadline (tests, or a tight supervisor policy).
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Set one environment variable on the child.
    pub fn with_env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.push((key.into(), Some(value.into())));
        self
    }

    /// Remove one environment variable from the child's environment.
    ///
    /// The supervisor owns the executor's environment: an inherited API key is a
    /// decision, not an accident.
    pub fn with_env_removed(mut self, key: impl Into<String>) -> Self {
        self.env.push((key.into(), None));
        self
    }

    /// The child's stderr lines so far: its events, one JSON object per line.
    pub fn events(&self) -> Vec<String> {
        self.events
            .lock()
            .map(|held| held.clone())
            .unwrap_or_default()
    }

    /// The identity the child announced in its `worker:ready` event.
    ///
    /// Read from the event lines this run produced (`since` is the buffer length
    /// before the spawn). By the time this is called the child has exited and its
    /// stderr has been drained, so the announcement is already in hand — no extra
    /// wait, and nothing to read asynchronously afterwards.
    fn announced_identity(&self, since: usize) -> Option<AgentId> {
        let events = self.events.lock().ok()?;
        events.iter().skip(since).find_map(|line| {
            let parsed: serde_json::Value = serde_json::from_str(line).ok()?;
            if parsed["event"] != "worker:ready" {
                return None;
            }
            parsed["agent_id"].as_str().map(AgentId::new)
        })
    }

    /// Drain the child's stderr into `events` on its own thread.
    ///
    /// A thread rather than a plain read: a worker that writes more than the pipe
    /// buffer holds would block forever once the buffer fills, and the outcome
    /// would never arrive.
    fn drain_stderr(&self, child: &mut Child) -> Option<std::thread::JoinHandle<()>> {
        let stderr = child.stderr.take()?;
        let events = Arc::clone(&self.events);
        Some(std::thread::spawn(move || {
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                if line.trim().is_empty() {
                    continue;
                }
                if let Ok(mut held) = events.lock() {
                    held.push(line);
                }
            }
        }))
    }
}

impl AgentHandle for StdioExecutorHandle {
    fn agent_id(&self) -> &AgentId {
        &self.agent_id
    }

    fn run(&self, task: &Task) -> Result<TaskOutcome, DispatchError> {
        // Where the child's events start for *this* run: the handle's event buffer
        // is shared across runs, so the identity is looked up only among the lines
        // this run produced.
        let seen = self.events.lock().map(|held| held.len()).unwrap_or(0);

        let line = serde_json::to_string(task)
            .map_err(|e| DispatchError::Failed(format!("the task is not serialisable: {e}")))?;

        let mut command = Command::new(&self.program);
        command.args(&self.args);
        for (key, value) in &self.env {
            match value {
                Some(value) => command.env(key, value),
                None => command.env_remove(key),
            };
        }
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| {
                DispatchError::Failed(format!("cannot start {}: {e}", self.program.display()))
            })?;

        // One task line in, then close stdin: the worker reads exactly one line,
        // and a closed pipe is how it knows the supervisor has finished talking.
        {
            let mut stdin = child
                .stdin
                .take()
                .ok_or_else(|| DispatchError::Failed("the child has no stdin pipe".into()))?;
            stdin
                .write_all(line.as_bytes())
                .and_then(|()| stdin.write_all(b"\n"))
                .and_then(|()| stdin.flush())
                .map_err(|e| DispatchError::Failed(format!("cannot send the task: {e}")))?;
        }

        let stderr_reader = self.drain_stderr(&mut child);

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| DispatchError::Failed("the child has no stdout pipe".into()))?;
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let mut buf = String::new();
            let read = BufReader::new(stdout)
                .read_line(&mut buf)
                .map(|read| (read, buf));
            let _ = tx.send(read);
        });

        let answered = rx.recv_timeout(self.timeout);
        match answered {
            Ok(Ok((0, _))) => {
                // EOF before a line: the worker died without answering.
                let status = wait_for(&mut child);
                join(stderr_reader);
                Err(DispatchError::Failed(format!(
                    "the executor closed stdout without an outcome ({status})"
                )))
            }
            Ok(Ok((_, line))) => {
                let parsed: TaskOutcome = serde_json::from_str(line.trim()).map_err(|e| {
                    DispatchError::Failed(format!("the executor answered with something else: {e}"))
                })?;
                let status = wait_for(&mut child);
                join(stderr_reader);
                if parsed.task_id != task.id {
                    // A worker answering a different task is a protocol break, not
                    // a result: never hand it up as if it were this task's.
                    return Err(DispatchError::Failed(format!(
                        "the executor answered task {} but {} was sent ({status})",
                        parsed.task_id, task.id
                    )));
                }
                // The child's answer names the identity **it** minted; ask the
                // child, through the announcement it makes before it runs, rather
                // than believing the label the supervisor addressed it by.
                let executor = self.announced_identity(seen).ok_or_else(|| {
                    DispatchError::Failed(format!(
                        "the executor never announced its identity (no worker:ready event; {status})"
                    ))
                })?;
                Ok(TaskOutcome {
                    agent_id: executor,
                    ..parsed
                })
            }
            Ok(Err(e)) => {
                let _ = child.kill();
                let status = wait_for(&mut child);
                join(stderr_reader);
                Err(DispatchError::Failed(format!(
                    "cannot read the executor's answer ({status}): {e}"
                )))
            }
            Err(_) => {
                // No answer in time: kill it, so a wedged worker cannot outlive
                // the dispatch that spawned it.
                let _ = child.kill();
                let status = wait_for(&mut child);
                join(stderr_reader);
                Err(DispatchError::Failed(format!(
                    "the executor did not answer within {:?} ({status})",
                    self.timeout
                )))
            }
        }
    }
}

/// Reap the child and describe how it ended.
fn wait_for(child: &mut Child) -> String {
    match child.wait() {
        Ok(status) => format!("status {status}"),
        Err(e) => format!("cannot wait for the child: {e}"),
    }
}

fn join(handle: Option<std::thread::JoinHandle<()>>) {
    if let Some(handle) = handle {
        let _ = handle.join();
    }
}
