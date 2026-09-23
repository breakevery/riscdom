[中文](README.zh-CN.md) | English

# examples/python — a reference supervisor

`dispatch.py` is the smallest complete **supervisor** RiscDom ships: a process that is not
inside the kernel, holds no model of its own, and drives a node through the control plane
over HTTP. It is the skeleton an AI supervisor wraps around a model — the tool calls are the
control plane's endpoints ([`docs/tool-schema-control-plane.md`](../../docs/tool-schema-control-plane.md)),
and the requests below are what a tool call turns into.

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
python dispatch.py --self-test
```

No server, no worker, no network beyond loopback: the script starts a **fake control plane**
on `127.0.0.1:0` (stdlib `http.server`, three endpoints) and runs the real dispatch path
against it. It asserts the fleet list, a successful outcome, a failed one, a refused target
(`404` with `cause: "target"`), the `--follow` subscription, a wrong token raising a
credential error, and a malformed task line being a usage error. `scripts/gate.sh` runs this
step when a Python interpreter is on `PATH` (and prints a skip when one is not).

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
