[中文](README.zh-CN.md) | English

# examples/python — a reference supervisor, and a dispatcher

`dispatch.py` is the smallest complete **supervisor** RiscDom ships: a process that is not
inside the kernel, holds no model of its own, and drives a node through the control plane
over HTTP. It is the skeleton an AI supervisor wraps around a model — the tool calls are the
control plane's endpoints ([`docs/tool-schema-control-plane.md`](../../docs/tool-schema-control-plane.md)),
and the requests below are what a tool call turns into.

`supervisor.py` is the other shape: the same client with a **loop** around it, which is what
an AI dispatcher (the roadmap's **M**) looks like as a program. There is a section for it
below.

**Standard library only.** No `requests`, no `httpx`, no SSE library: `urllib.request`,
`json`, `argparse`. A reference implementation should not teach a dependency it does not
need.

## What it does

Three endpoints, in the order a supervisor meets them:

| Step | Endpoint | Why |
|---|---|---|
| 1 | `GET /v0/executors` | Who can be asked. An empty list is the node saying it has no fleet. |
| 2 | `POST /v0/tasks` | One task, one executor, one `TaskOutcome` — **synchronously**, exactly like `POST /v0/agent/run`. There is no task table to poll. |
| 3 | `GET /v0/events` (`--follow`) | The event stream, while the work happens. |

A task is routed by its `target`: that is what makes `/v0/tasks` a different thing from
`/v0/agent/run` (which runs on the node itself). A target nobody owns is refused — `404`,
`cause: "target"` — and the script reports that as **refused**, not as a failed run.

## Before you start

1. **A node, with a fleet.** Add `executors` to its `settings.json` (see
   [`server/README.md`](../../server/README.md) and the E0 decision §39 in
   [`docs/decisions.md`](../../docs/decisions.md)) — each entry is a label, a program and
   its arguments, and the program is normally the `worker` binary:
   ```json
   { "version": 1,
     "executors": [ { "label": "executor-0", "program": "../target/debug/worker",
                      "args": ["--workspace", "./ws", "--data-dir", "./data-0"] } ] }
   ```
2. **A control plane.** Either `riscdom-server` or the CLI's local mode (`riscdom health`
   starts one, then exits — use `riscdom-server` for a supervisor).
3. **A token.** `--token-file <path>` or `$RISCDOM_TOKEN`. The default is the server's
   `<data-dir>/token` file. **The token is never a command-line argument**: an argument lands
   in the shell history and in the process list, and the CLI's own `--token` warns for that
   reason.

## Running it

```bash
# One task, one executor, whichever fleet member comes first.
export RISCDOM_TOKEN="$(cat ./data/token)"        # or pass --token-file ./data/token
python dispatch.py --target executor-0 "build the blink example"

# A task list, routed round-robin over the fleet, with the event stream on stderr.
python dispatch.py --tasks tasks.jsonl --follow

# The same, against a server somewhere else, declaring a sandbox per task.
python dispatch.py --server 10.0.0.7:7821 --tasks tasks.jsonl --sandbox blink
```

`tasks.jsonl` is one task per line (the same JSON-lines shape the worker's supervisor reads):

```jsonl
{"target": "executor-0", "input": "compile the blink example"}
{"target": "executor-1", "input": "read the serial buffer and summarize it", "sandbox": "blink"}
```

Arguments: `--server` (default `127.0.0.1:7821`), `--token-file`, `--tasks <file|->`,
`--target`, `--sandbox`, `--follow`, `--timeout`, `--json`, `--self-test`. Bare arguments
after the options are task texts, and only then does `--target` have to name the executor.

**Exit codes** (the CLI's convention): `0` every task answered a success, `1` at least one
did not (refused, broken, or a run that failed), `2` a usage error, `3` the control plane
could not be reached or refused the credential.

## Proving it offline

```bash
python dispatch.py --self-test      # the batch client
python supervisor.py --self-test    # the dispatcher
```

Neither needs a server, a worker, or any network beyond loopback: each starts its own
**fake** node on `127.0.0.1:0` (stdlib `http.server`) and runs its real code path against it.

`dispatch.py`'s asserts the fleet list, a successful outcome, a failed one, a refused target
(`404` with `cause: "target"`), the `--follow` subscription, a wrong token raising a
credential error, and a malformed task line being a usage error. `supervisor.py`'s asserts
that the state snapshot reads every source it claims, that an idle turn sends **no** control
request, that a read which fails ends the turn before any control request and the next turn
recovers, that every tool in the action table hits its endpoint, that the event reader keeps
the last `id` and resumes with `Last-Event-ID`, and that nothing but the standard library is
imported. `scripts/gate.sh` runs both steps when a Python interpreter is on `PATH` (and
prints a skip when one is not).

## The dispatcher: `supervisor.py`

`dispatch.py` is handed a task list and sends it. A dispatcher **stays up**, reads the
node's state, and decides whether to act:

    read the state (one snapshot)  →  decide  →  act  →  report

**The decision layer is a stub.** `decide()` returns `None`, which means "do nothing", and
that is the point rather than a placeholder: a dispatcher that cannot see the whole picture
dispatches nothing, so the conservative answer is also the default one, and turning this on
cannot by itself change a node. The model call that fills `decide()` in is a later batch;
everything around it is here — the snapshot, the action plumbing, the event reader, and the
credential and transport (imported from `dispatch.py`, not copied).

```bash
python supervisor.py --once           # one conservative turn, then exit
python supervisor.py --interval 30    # stay up, a turn every 30 seconds
python supervisor.py --events         # just the event stream (read-only)
python supervisor.py --self-test      # offline, fake node, no server
```

`--server` and `--token-file` mean what they mean above, and the exit codes are the same
table: `0` the turn completed (acted, or had nothing to do), `1` the turn did not complete —
the state could not be read, so it acted on nothing — `2` a usage error, `3` the control
plane could not be reached or refused the credential.

`import dispatch` is deliberate: the transport, the token rule and the error taxonomy are
already written once, and a second copy of the wire format would be a second thing to keep
true. Python does not always put a script's own directory on `sys.path` (`-P`,
`PYTHONSAFEPATH=1`), so the file adds it explicitly — two lines, with the reason.

### Known boundaries

Stated rather than hidden. The M2c reconnaissance found each one; changing any of them is a
kernel batch, not this file:

- **M has no identity of its own in the chain.** A token client acts as `operator`, so M's
  rows and a person's look alike, and the node's own `m.sandbox.*` rows carry
  `actor: "host"`.
- **A decision is not written to the chain.** `approve` / `reject` announce a
  `sandbox:request` event and record nothing durable.
- **The instance table and the pending-approval slot are in memory.** A node restart loses
  both; the audit chain is the only durable source.
- **The audit read has no window and no pagination.** `GET /v0/audit/events` takes `limit`
  (required), `actor` and `action_prefix`, so "everything since X" is an export, not a
  query.
- **Five capability names are vocabulary only** (`task.dispatch`, `task.dispatch.remote`,
  `sandbox.instantiate.remote`, `audit.read.remote`, `request.approve`): no route requires
  them. A dispatch needs `agent.run`; deriving an instance needs `sandbox.instantiate`;
  deciding a request needs `sandbox.read` **and** whatever the request's own action implies.
- **The tool list is not a new document.** M's tools are
  [`docs/tool-schema-control-plane.md`](../../docs/tool-schema-control-plane.md) — the same
  "every endpoint as a function" list the client guide's §8 hands a supervisor.

| | `dispatch.py` | `supervisor.py` |
|---|---|---|
| Shape | a batch: a task list in, a report out | a loop: state in, one decision out |
| Decides | nothing — `--target` or round-robin | `decide()`, a stub here |
| Writes | `POST /v0/tasks` | nothing yet (the plumbing is there) |
| Reads | the fleet | status / capabilities / sandboxes / instances / pending requests |
| Events | `--follow`, no resume | `--events`, resuming with `Last-Event-ID` |
| Reuses | — | `dispatch.py`'s transport, token rule and errors |

## Compared with `worker/examples/dispatch.rs`

Both are supervisors; they are the two halves of the picture, and neither replaces the other.

| | `worker/examples/dispatch.rs` | `examples/python/dispatch.py` |
|---|---|---|
| Language | Rust, a cargo example | Python 3, stdlib only |
| Reaches executors | directly, as child processes (stdin/stdout) | through the node's control plane (HTTP) |
| Needs a running node | no — it starts the executors itself | **yes** — the node owns its fleet |
| Needs a token | no | yes (the control plane is authenticated) |
| Concurrency | `dispatch_all` runs the fleet in parallel | one task at a time, in order |
| Shows | the task protocol and the process boundary | the HTTP surface and the event stream |
| Task source | `--tasks <file>` / stdin (JSON lines) | `--tasks <file>` / stdin (JSON lines) |

## What this is not

- **Not an AI.** There is no model here; it shows the plumbing a supervisor's model would sit
  on top of.
- **Not a client of `agent`'s tools.** The eight tools an executor's model may call
  ([`docs/tool-schema-executor.md`](../../docs/tool-schema-executor.md)) are inside the
  executor, not reachable from here.
- **Not a scheduler.** Tasks are sent one at a time, in order, and the script waits for each
  answer. A real supervisor would overlap them; the interesting part here is the contract,
  and a queue would hide it.

One honest caveat about `--follow`: the envelope's `task_id` is `null` for host events, so a
frame cannot be attributed to the task that caused it — the `agent_id` on the frame is the
executor that ran. A supervisor that needs attribution should read the audit chain
(`GET /v0/audit/events`, and the `agent.file.write` rows), where the actor is recorded.
