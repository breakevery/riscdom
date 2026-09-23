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
