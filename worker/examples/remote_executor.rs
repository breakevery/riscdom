//! A remote executor: an [`AgentHandle`] whose executor is **another node**, over HTTP.
//!
//! `agent/src/dispatch.rs` says of the trait: *"A remote implementation (another process,
//! another machine) implements exactly this trait. **None is written yet** — that is the
//! seam (v0.8 batch C)."* This example is that implementation, written against the seam and
//! nothing else:
//!
//! ```text
//! cargo run -p worker --example remote_executor                 # a stand-in node, no setup
//! cargo run -p worker --example remote_executor -- --self-test   # prove the handle offline
//! cargo run -p worker --example remote_executor -- --server 127.0.0.1:7821 --target executor-0 "say hi"
//! ```
//!
//! It changes **no production code**: `agent`'s trait, `host-core`'s `StdioExecutorHandle`
//! and the node's registry are all left alone, and the handle drops into the same
//! `LocalDispatcher` the stdio one does:
//!
//! ```text
//! LocalDispatcher::new(vec![Arc::new(handle) as Arc<dyn AgentHandle>])
//! ```
//!
//! ## Why `POST /v0/tasks`
//!
//! The endpoint to reach is `POST /v0/tasks`: it routes one task to an executor **the remote
//! node owns** and answers the `TaskOutcome` that executor produced. That is the same
//! contract [`StdioExecutorHandle`](host_core::StdioExecutorHandle) gets from a child
//! process, one transport over — so the handle is a real executor, not a shape demo.
//!
//! The one thing it must not do is send its own local label as the target: the remote node
//! routes by **its own** labels. So a handle carries two names, exactly like the stdio one
//! carries a supervisor label and a child-minted identity:
//!
//! - [`HttpExecutorHandle::agent_id`] — the label the **local** dispatcher routes on;
//! - the remote target — what the task must say to be routable **there** (the same string
//!   by default, because a fleet is usually named the same on both sides).
//!
//! ## No HTTP client crate
//!
//! There is no `reqwest` here: `worker` depends on `host-core`, `agent` and `serde_json`,
//! and a reference implementation that adds a dependency to demonstrate a wire format has
//! hidden the wire format. The request is written by hand — the technique this repository
//! already uses in `server/tests/smoke.rs` and `host-core/tests/common/mod.rs` — so the
//! reader can see exactly what crosses.

use agent::{AgentHandle, AgentId, DispatchError, Dispatcher, Task, TaskOutcome};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// How long one dispatch may take before the handle gives up.
///
/// Generous, like the stdio handle's: a run compiles and boots a guest, so this is a
/// deadlock guard rather than a performance budget.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(300);

/// An executor reached over a node's control plane.
pub struct HttpExecutorHandle {
    /// The identity a task must target to reach this handle **locally**.
    agent_id: AgentId,
    /// The node, as `host:port` or `http://host:port`.
    base: String,
    /// The bearer token, when the node requires one.
    token: Option<String>,
    /// The label the **remote** node knows this executor by.
    remote_target: String,
    timeout: Duration,
}

impl HttpExecutorHandle {
    /// A handle for `agent_id`, which asks `base` for the executor `remote_target`.
    pub fn new(
        agent_id: impl Into<String>,
        base: impl Into<String>,
        remote_target: impl Into<String>,
    ) -> Self {
        Self {
            agent_id: AgentId::new(agent_id),
            base: base.into(),
            token: None,
            remote_target: remote_target.into(),
            timeout: DEFAULT_TIMEOUT,
        }
    }

    /// Send the bearer token the node asks for.
    pub fn with_token(mut self, token: impl Into<String>) -> Self {
        self.token = Some(token.into());
        self
    }

    /// Override the answer deadline.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// The request body: a `Task` whose target is the **remote** label.
    ///
    /// The id travels with it, so the node echoes back the task that was asked for and the
    /// handle can refuse an answer to a different one.
    fn body(&self, task: &Task) -> serde_json::Value {
        serde_json::json!({
            "id": task.id.as_str(),
            "target": self.remote_target,
            "input": task.input,
            "sandbox": task.sandbox,
        })
    }

    /// One HTTP/1.1 request over a fresh connection, answered with `(status, body)`.
    ///
    /// `Connection: close` is deliberate: the answer is read to the end of the body, and
    /// with a `Content-Length` it stops there — a chunked answer is read to EOF instead.
    fn post(&self, path: &str, body: &str) -> Result<(u16, String), DispatchError> {
        let base = self.base.trim_end_matches('/');
        let authority = base
            .strip_prefix("http://")
            .ok_or_else(|| DispatchError::Failed(format!("{base} is not an http:// URL")))?
            .to_string();

        let stream = TcpStream::connect(&authority)
            .map_err(|e| DispatchError::Failed(format!("cannot reach {authority}: {e}")))?;
        stream
            .set_read_timeout(Some(self.timeout))
            .and_then(|()| stream.set_write_timeout(Some(self.timeout)))
            .map_err(|e| DispatchError::Failed(format!("cannot set a deadline: {e}")))?;
        let mut stream = stream;

        let mut request = format!(
            "POST {path} HTTP/1.1\r\nHost: {authority}\r\nContent-Type: application/json\r\n\
             Content-Length: {}\r\nConnection: close\r\n",
            body.len()
        );
        if let Some(token) = &self.token {
            request.push_str(&format!("Authorization: Bearer {token}\r\n"));
        }
        request.push_str("\r\n");
        request.push_str(body);

        stream
            .write_all(request.as_bytes())
            .and_then(|()| stream.flush())
            .map_err(|e| DispatchError::Failed(format!("cannot send the task: {e}")))?;

        let mut reader = BufReader::new(stream);
        let mut status_line = String::new();
        reader
            .read_line(&mut status_line)
            .map_err(|e| DispatchError::Failed(format!("the node did not answer: {e}")))?;
        let status = status_line
            .split_whitespace()
            .nth(1)
            .and_then(|code| code.parse::<u16>().ok())
            .ok_or_else(|| {
                DispatchError::Failed(format!("the node answered something else: {status_line:?}"))
            })?;

        let mut length = None;
        loop {
            let mut header = String::new();
            let read = reader
                .read_line(&mut header)
                .map_err(|e| DispatchError::Failed(format!("cannot read the answer: {e}")))?;
            if read == 0 || header.trim().is_empty() {
                break;
            }
            let lowered = header.to_ascii_lowercase();
            if let Some(rest) = lowered.strip_prefix("content-length:") {
                length = rest.trim().parse::<usize>().ok();
            }
        }

        let mut text = String::new();
        match length {
            Some(length) => {
                let mut bytes = vec![0u8; length];
                reader
                    .read_exact(&mut bytes)
                    .map_err(|e| DispatchError::Failed(format!("the answer is short: {e}")))?;
                text.push_str(&String::from_utf8_lossy(&bytes));
            }
            None => {
                // No length: `Connection: close` means EOF is the end.
                reader
                    .read_to_string(&mut text)
                    .map_err(|e| DispatchError::Failed(format!("cannot read the answer: {e}")))?;
            }
        }
        Ok((status, text))
    }
}

impl AgentHandle for HttpExecutorHandle {
    fn agent_id(&self) -> &AgentId {
        &self.agent_id
    }

    fn run(&self, task: &Task) -> Result<TaskOutcome, DispatchError> {
        // A handle answers only for its own identity, standing alone: the dispatcher
        // already routes by target, and a remote handle reached with the wrong task should
        // say so rather than ask a node for it.
        if task.target != self.agent_id {
            return Err(DispatchError::NoSuchAgent(task.target.clone()));
        }

        let body = self.body(task).to_string();
        let (status, text) = self.post("/v0/tasks", &body)?;

        if status == 404 {
            // The remote node owns nobody by that name: the *remote* fleet is what is
            // missing, which is a routing fact, not a broken dispatch.
            return Err(DispatchError::NoSuchAgent(AgentId::new(
                self.remote_target.clone(),
            )));
        }
        if !(200..300).contains(&status) {
            return Err(DispatchError::Failed(format!(
                "the node answered {status}: {}",
                text.trim()
            )));
        }

        let parsed: TaskOutcome = serde_json::from_str(&text)
            .map_err(|e| DispatchError::Failed(format!("the node answered something else: {e}")))?;
        if parsed.task_id != task.id {
            // Same discipline as the stdio handle: an answer to a different task is a
            // protocol break, never a result.
            return Err(DispatchError::Failed(format!(
                "the node answered task {} but {} was sent",
                parsed.task_id, task.id
            )));
        }
        // The identity in the outcome is the node's own — the executor that really ran
        // it — and is deliberately *not* overwritten with this handle's local label.
        Ok(parsed)
    }
}

// ---------------------------------------------------------------------------------------
// A stand-in node, so the example and its self-test need nothing running.
// ---------------------------------------------------------------------------------------

/// A loopback node that answers one `POST /v0/tasks` per connection.
///
/// The same technique `host-core/tests/common/mod.rs` uses for its download fixture, and
/// the answer is whatever `answer` returns for the task it received.
fn start_fake_node(
    answer: fn(&serde_json::Value) -> (u16, serde_json::Value),
) -> (String, Arc<Mutex<Vec<serde_json::Value>>>, SocketAddr) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind a loopback port");
    let addr = listener.local_addr().expect("the bound address");
    let seen: Arc<Mutex<Vec<serde_json::Value>>> = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&seen);

    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            let Ok(clone) = stream.try_clone() else {
                continue;
            };
            let mut reader = BufReader::new(clone);

            let mut request_line = String::new();
            if reader.read_line(&mut request_line).unwrap_or(0) == 0 {
                continue;
            }
            let mut length = 0usize;
            loop {
                let mut header = String::new();
                if reader.read_line(&mut header).unwrap_or(0) == 0 || header.trim().is_empty() {
                    break;
                }
                if let Some(rest) = header.to_ascii_lowercase().strip_prefix("content-length:") {
                    length = rest.trim().parse().unwrap_or(0);
                }
            }
            let mut body = vec![0u8; length];
            let _ = reader.read_exact(&mut body);
            let received: serde_json::Value =
                serde_json::from_slice(&body).unwrap_or(serde_json::Value::Null);
            sink.lock().expect("lock").push(received.clone());

            let (status, payload) = answer(&received);
            let text = payload.to_string();
            let response = format!(
                "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\n\
                 Content-Length: {}\r\nConnection: close\r\n\r\n{text}",
                text.len()
            );
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.flush();
        }
    });

    (format!("http://{addr}"), seen, addr)
}

/// A node that owns `remote-executor` and answers with a `Final` outcome, announced under
/// its own identity — never the label the caller used.
fn a_working_node(task: &serde_json::Value) -> (u16, serde_json::Value) {
    if task.get("target").and_then(|value| value.as_str()) != Some("remote-executor") {
        return (
            404,
            serde_json::json!({
                "code": "not_found",
                "message": format!("no executor for agent {}", task.get("target").and_then(|v| v.as_str()).unwrap_or("?")),
                "cause": "target",
            }),
        );
    }
    (
        200,
        serde_json::json!({
            "task_id": task.get("id").and_then(|value| value.as_str()).unwrap_or(""),
            "agent_id": "remote-node-child-1",
            "outcome": { "Final": { "content": "done on the other node", "iterations": 3 } },
        }),
    )
}

// ---------------------------------------------------------------------------------------

fn self_test() -> i32 {
    let mut failures: Vec<String> = Vec::new();

    // 1 + 2 + 4: the request is a task-shaped body, the answer is parsed, and the identity
    // that comes back is the node's, not this handle's label.
    let (base, seen, _addr) = start_fake_node(a_working_node);
    let handle = HttpExecutorHandle::new("local-label", base.clone(), "remote-executor")
        .with_timeout(Duration::from_secs(10));
    let task = Task::new(AgentId::new("local-label"), "say hi");
    match handle.run(&task) {
        Ok(outcome) => {
            if outcome.agent_id.as_str() != "remote-node-child-1" {
                failures.push(format!(
                    "the node's identity must win: {}",
                    outcome.agent_id.as_str()
                ));
            }
            if outcome.task_id != task.id {
                failures.push("the outcome must answer the task that was asked".into());
            }
            if outcome.outcome
                != (agent::AgentOutcome::Final {
                    content: "done on the other node".into(),
                    iterations: 3,
                })
            {
                failures.push(format!("the outcome is wrong: {:?}", outcome.outcome));
            }
        }
        Err(error) => failures.push(format!("a working node must answer: {error}")),
    }
    let sent = seen.lock().expect("lock").clone();
    if sent.len() != 1 {
        failures.push(format!("the node saw {} requests, expected 1", sent.len()));
    } else {
        let body = &sent[0];
        if body.get("target").and_then(|v| v.as_str()) != Some("remote-executor") {
            failures.push(format!("the body must carry the remote target: {body}"));
        }
        if body.get("input").and_then(|v| v.as_str()) != Some("say hi") {
            failures.push(format!("the body must carry the input: {body}"));
        }
        if body.get("id").and_then(|v| v.as_str()) != Some(task.id.as_str()) {
            failures.push(format!("the body must carry the task id: {body}"));
        }
    }

    // 5: a target the remote node does not own is `NoSuchAgent`, not a broken dispatch.
    let (base, _seen, _addr) = start_fake_node(a_working_node);
    let stranger = HttpExecutorHandle::new("local-label", base, "somebody-else");
    let task = Task::new(AgentId::new("local-label"), "say hi");
    match stranger.run(&task) {
        Err(DispatchError::NoSuchAgent(agent)) => {
            if agent.as_str() != "somebody-else" {
                failures.push(format!("the refusal must name the remote target: {agent}"));
            }
        }
        other => failures.push(format!("expected NoSuchAgent, got {other:?}")),
    }

    // 6: an answer to a different task is a protocol break.
    let (base, _seen, _addr) = start_fake_node(|_| {
        (
            200,
            serde_json::json!({
                "task_id": "task-somebody-else-1",
                "agent_id": "remote-node-child-1",
                "outcome": { "Final": { "content": "wrong task", "iterations": 1 } },
            }),
        )
    });
    let handle = HttpExecutorHandle::new("local-label", base, "remote-executor");
    match handle.run(&Task::new(AgentId::new("local-label"), "say hi")) {
        Err(DispatchError::Failed(message)) => {
            if !message.contains("answered task") {
                failures.push(format!("the mismatch message is wrong: {message}"));
            }
        }
        other => failures.push(format!("a mismatched outcome must fail: {other:?}")),
    }

    // 3: nothing listening is a dispatch failure, and the message says where.
    let handle = HttpExecutorHandle::new("local-label", "http://127.0.0.1:1", "remote-executor")
        .with_timeout(Duration::from_secs(2));
    match handle.run(&Task::new(AgentId::new("local-label"), "say hi")) {
        Err(DispatchError::Failed(message)) => {
            if !message.contains("cannot reach") {
                failures.push(format!("the transport message is wrong: {message}"));
            }
        }
        other => failures.push(format!("an unreachable node must fail: {other:?}")),
    }

    // A handle standing alone answers only for its own identity.
    let handle = HttpExecutorHandle::new("local-label", "http://127.0.0.1:1", "remote-executor");
    match handle.run(&Task::new(AgentId::new("someone-else"), "say hi")) {
        Err(DispatchError::NoSuchAgent(agent)) => {
            if agent.as_str() != "someone-else" {
                failures.push(format!("the wrong task must be named: {agent}"));
            }
        }
        other => failures.push(format!("a foreign task must be refused: {other:?}")),
    }

    if failures.is_empty() {
        println!("remote executor self-test: OK (loopback node, no network beyond loopback)");
        0
    } else {
        eprintln!("remote executor self-test: FAILED");
        for failure in &failures {
            eprintln!("  {failure}");
        }
        1
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|arg| arg == "--self-test") {
        std::process::exit(self_test());
    }

    let value = |flag: &str| -> Option<String> {
        args.iter()
            .position(|arg| arg == flag)
            .and_then(|at| args.get(at + 1))
            .cloned()
    };
    let token = value("--token-file")
        .and_then(|path| std::fs::read_to_string(path).ok())
        .map(|text| text.trim().to_string())
        .or_else(|| {
            std::env::var("RISCDOM_TOKEN")
                .ok()
                .filter(|v| !v.is_empty())
        });

    // With no `--server`, the example runs against a stand-in node on loopback: the point
    // is the handle, and a reader should not have to configure a fleet to see it work.
    let (base, seen, remote) = match value("--server") {
        Some(server) => (
            server,
            None,
            value("--remote-target").unwrap_or_else(|| "executor-0".into()),
        ),
        None => {
            let (base, seen, addr) = start_fake_node(a_working_node);
            println!("node      : a stand-in on {addr} (pass --server for a real one)");
            (base, Some(seen), "remote-executor".to_string())
        }
    };
    let target = value("--target").unwrap_or_else(|| "executor-0".to_string());
    let input = value("--input").unwrap_or_else(|| "say hi".to_string());

    let mut handle = HttpExecutorHandle::new(target.clone(), base.clone(), remote.clone());
    if let Some(token) = token {
        handle = handle.with_token(token);
    }

    // The registration the seam promised: one line, and the stdio handle's neighbours do
    // not change.
    let dispatcher = agent::LocalDispatcher::new(vec![Arc::new(handle) as Arc<dyn AgentHandle>]);
    println!("handle    : {target} (remote target {remote})");
    println!(
        "fleet     : {:?}",
        dispatcher
            .agent_ids()
            .iter()
            .map(|id| id.as_str())
            .collect::<Vec<_>>()
    );

    match dispatcher.dispatch(Task::new(AgentId::new(target), input)) {
        Ok(outcome) => {
            println!("answered  : {}", outcome.agent_id.as_str());
            println!("outcome   : {:?}", outcome.outcome);
            if let Some(seen) = seen {
                let seen = seen.lock().expect("lock").clone();
                println!(
                    "the stand-in saw {} request(s): {}",
                    seen.len(),
                    seen.first().cloned().unwrap_or_default()
                );
            }
        }
        Err(error) => {
            eprintln!("dispatch: {error}");
            std::process::exit(1);
        }
    }
}
