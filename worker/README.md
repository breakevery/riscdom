[中文](README.zh-CN.md) | English

# worker

The **executor process** of the two-process prototype, plus the supervisor half that drives a
fleet of them (v0.8 main deliverables 1/2 and 2/2).

One task in, one outcome out. The binary reads a single `Task` as **one JSON line on stdin**,
runs it through the host's own `AppState::run_agent` path — the very same path the desktop app
runs, not a second agent-driving implementation — and writes one `TaskOutcome` as **one JSON line
on stdout**. Its events (JSON lines) go to **stderr**, so stdout stays a pure protocol channel a
supervisor can parse without filtering.

It depends on [`host-core`](../host-core/README.md), the kernel facade's portable half, so this
binary **links no Tauri crate** (`cargo tree -p worker` names none). Before v0.9's A1 wave 3 it
depended on `host` and inherited Tauri as the known cost.

## Running it

```text
worker --workspace <dir> --data-dir <dir> [--sleep-ms <n>]
```

| Argument | Meaning |
|---|---|
| `--workspace <dir>` | Required. The workspace this executor works in; its audit chain lives at `<dir>/.riscdom/audit.db`. |
| `--data-dir <dir>` | Required. Where this executor's `settings.json`, `sessions.db` and toolchain directory live. |
| `--sleep-ms <n>` | Diagnostic hook (tests, manual timing): wait this long after reading the task, before answering. Changes nothing else. |

`--workspace` and `--data-dir` have **no environment fallback on purpose**: an executor's identity
is its command line, so two executors sharing a workspace still get their own state, and no
inherited variable can make two workers collide without saying so.

**Exit codes.** `0` whenever a `TaskOutcome` was written — including a failed run, because the
outcome is the answer and the exit status is not. A usage error exits `2` and writes no stdout
line; the supervisor reports that as a protocol failure. A malformed request still gets an answer:
the outcome carries the stable placeholder identities `task-unparsed` / `unparsed`, so the
supervisor learns "your task was not readable" instead of waiting for a line that never comes.

## The supervisor half

The library target (`src/lib.rs`, `src/supervisor.rs`) is a **non-AI dispatcher**: it reads a task
list, routes each task to the executor named in `Task.target`, and reports what came back. There is
no model anywhere in it. Routing is explicit and never guessed — a task naming an executor that is
not in the fleet is refused (`DispatchError::NoSuchAgent`) rather than handed to whichever
executor happens to be free.

The runnable demo:

```text
cargo build -p worker                       # the executor binary must exist first
cargo run  -p worker --example dispatch     # two executors, one demo task each
cargo run  -p worker --example dispatch -- --tasks tasks.jsonl --executors 3
```

`--executors <n>` (default 2) executors **share one workspace** — one audit chain, per-agent
snapshots — and each gets its **own data dir**. `--base <dir>` sets where those live, `--worker
<path>` overrides the executor binary, and `--tasks <file>` (or `-` for stdin) supplies a
JSON-lines task list; without it the demo invents one task per executor. An executor with no LLM
configured answers `Failed`: the plumbing is what the demo shows, not a model.

## The host's own dispatch (v0.9 interface E0)

The node can hold a fleet the same way a supervisor does: `executors` in
`settings.json` (a label, a program and its arguments — **no `env`**, because a
settings file is not a secret store) are registered at startup, and
`POST /v0/tasks` routes one task to the executor its `target` names, answering with
the `TaskOutcome`. `GET /v0/executors` lists who is reachable. Nothing is spawned
at registration: `StdioExecutorHandle::new` only records what to run.

The node itself is deliberately **not** one of its own executors — a target naming
it is a `404` — because running *here* is `POST /v0/agent/run`. The two endpoints
are siblings, not synonyms. Registration is configuration, not an API: there is no
runtime endpoint to add an executor, and the `worker` binary is the natural
`program` to point at (one task line in on stdin, one outcome line out on stdout,
events on stderr — exactly the protocol `StdioExecutorHandle` speaks, since that is
what this crate's tests already drive).

## A remote executor (v0.9 interface E4)

`examples/remote_executor.rs` is the other end of the same seam `examples/dispatch.rs` and
`host-core`'s `StdioExecutorHandle` sit on: an [`AgentHandle`](../agent/src/dispatch.rs) whose
executor is **another node**, reached over HTTP. `agent`'s trait says it has been waiting for
one since v0.8 (*"A remote implementation … implements exactly this trait. None is written
yet"*); this is it, written against the seam and nothing else — no crate in this workspace
changed.

```text
cargo run -p worker --example remote_executor                  # a stand-in node, no setup
cargo run -p worker --example remote_executor -- --self-test    # prove the handle offline
cargo run -p worker --example remote_executor -- --server 127.0.0.1:7821 --target executor-0 "say hi"
```

What it demonstrates:

- **The endpoint is `POST /v0/tasks`.** That is the one that routes a task to an executor
  the remote node owns and answers the `TaskOutcome` — the same contract the stdio handle
  gets from a child process, one transport over. It is therefore a real executor, not a
  shape demo.
- **Two names, one handle.** The local dispatcher routes on the handle's `agent_id`; the
  remote node routes on a label **it** knows, so the task body carries that as `target`
  (`--remote-target`, the same string by default). The stdio handle has the same split — a
  supervisor's label versus the identity the child announces.
- **The identity in the answer wins.** `TaskOutcome.agent_id` comes from the response, never
  from the handle's own label, and an answer naming a different task is a protocol break —
  both exactly as `StdioExecutorHandle` does it.
- **The registration is one line**: `LocalDispatcher::new(vec![Arc::new(handle) as Arc<dyn
  AgentHandle>])`, beside a stdio handle or instead of it.

`--self-test` binds a stand-in node on `127.0.0.1:0` (the technique
`host-core/tests/common/mod.rs` uses for its download fixture) and asserts seven things:
the request body is task-shaped (id, target, input), the answer parses, the node's identity
wins over the label, a target the node does not own is `NoSuchAgent`, an answer to another
task fails, an unreachable node fails with the address named, and a task addressed to
somebody else is refused locally. `scripts/gate.sh` runs it (`cargo run -q -p worker
--example remote_executor -- --self-test`).

It is **not** production code and **not** a cross-device story: the transport is HTTP on
loopback, and a handle between two machines would be the same code with a different `base`
(the part that is *not* the same — mutual authentication, what a token authorises on the
other side — is v1.0 work, and the client guide says so).

## Tests

```text
cargo test -p worker
```

- `tests/stdio.rs` — the process boundary, against the real `worker` binary
  (`CARGO_BIN_EXE_worker`): a task crosses it and comes back as an outcome; a malformed task is
  answered with a failure and no crash; a worker that cannot start is reported, not hung; a worker
  that never answers is killed and reported; two workers keep their own data dirs.
- `tests/supervisor.rs` — routing and the tally: every executor label is registered, an executor
  outside the fleet is refused with `DispatchError::NoSuchAgent`, an empty plan reports an empty
  tally, a refusal and a break are counted apart, and a task list is one JSON object per line.

A worker cannot *complete* a run in a test — it has no LLM configured, and QEMU is never called
from here — so the child answers with the failure the host reports, and that is the assertion: a
refusal arrives as a well-formed outcome. Full-run behaviour stays covered in-process by
[`host-core`](../host-core/tests)'s own tests.
