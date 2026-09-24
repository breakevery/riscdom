[中文](multi-agent-foundation.zh-CN.md) | English

# RiscDom Multi-Agent Foundation — the v0.8 Settled Shapes

Snapshot: the state of the four decisions v0.8 settled, written down **from the code**, after the
v0.8 main deliverable (batches 1/2 and 2/2). It records what exists and what does not; a shape that
is missing is named as missing, not implied.

Repository `D:\codeagent\breakevery\riscdom`, branch `main`. Its sibling is the
[architecture-evolution note](architecture-evolution.md), which holds the plan; this file holds the
landed result.

## 1. What v0.8 delivered

| Batch | Shape landed |
| --- | --- |
| 1 | app-data directory **injected** (`AppState::with_data_dir`); `audit_events.agent_id` column added beside the chain; one VM slot per `AppState` pinned by a test |
| 2 | several processes may write one `audit.db` (WAL, `busy_timeout`, `BEGIN IMMEDIATE`, retry) and a failed write is **loud** (`audit:failed`, banner, popup) |
| 3 | `agent_id` has producers everywhere; snapshots move to a per-agent subdirectory with a read fallback |
| 4 | the minimal dispatch abstraction: `Task`, `AgentHandle`, `Dispatcher`, local implementation, the remote half deliberately absent |
| 5 (1/2) | two processes: the `worker` executor binary + `host_core::StdioExecutorHandle`; stdio + JSON lines; per-executor data dir |
| 6 (2/2) | the supervisor: a **non-AI** dispatcher driving several executors concurrently, with a report and a tally |

Not in v0.8, and not pretended: an AI supervisor, a remote (other-machine) executor, preflight
directory isolation, and a Tauri-free executor binary.

## 2. Decision 1 — B2: one machine, several processes

1. **One workspace, one chain.** `audit.db` lives at `<workspace>/.riscdom/audit.db` and is shared on
   purpose: one chain is what lets the finished system answer "who made whom do what".
2. **Several writers are safe.** The connection opens in WAL with a five-second busy timeout and
   `synchronous=NORMAL`; an append takes the write lock *before* reading the head
   (`BEGIN IMMEDIATE` — without it two writers chained onto one row and forked the chain, which the
   v0.8 batch-2 concurrency test caught); a locked append is retried five times with
   20/40/80/160 ms backoff. `AuditSink::record` returns `Result`; the host turns a failure into an
   `audit:failed` event, a log line, and (by default) a banner plus a popup.
3. **Each process owns its private state.** `AppState::with_data_dir(workspace, data_dir)` resolves
   `settings.json`, `sessions.db` and the toolchain directory inside `data_dir`, so two processes in
   one workspace never share them. The audit DB stays under the workspace by design.
4. **Supervisor and executor are separate processes.** The executor is the `worker` binary; the
   supervisor is a dispatcher (see §5). Transport is **stdio + JSON lines**, which needs no new
   dependency and makes a dead child an EOF rather than a hung read.
5. **The known cost was paid.** `worker` used to depend on `host`, and so the executor binary
   linked Tauri — which it needs neither (`AppHandle` nor a window). Since v0.9's A1 wave 3 it
   depends on `host-core`, so the executor binary links no Tauri crate at all (§7).

## 3. Decision 2 — agent identity: `<device>-<pid>-<seq>`

1. **Minting.** `agent::next_agent_id()` (`agent/src/identity.rs`) builds `DEVICE` (the machine;
   `local` for now), `std::process::id()` and a process-wide counter: `local-12345-1`. Two processes
   cannot share a pid, and two agents inside one process cannot share a counter value.
2. **Producers.** The `AgentLoop` takes its identity at construction and stamps every event it writes;
   the five `audit_hook` helpers and the tool layer take it too; the host stamps its own events and
   its `run.start` / `run.end` / `run.abandoned` markers with the identity it minted for its
   `AppState`.
3. **Beside the chain, not in it.** The hash formula (`prev|ts|actor|action|detail`), the `prev_hash`
   linkage, every historical row's `hash` and the append-only triggers are untouched. Old rows carry
   `NULL`. The JSONL export and `list_audit_events` include the field.
4. **What the supervisor sees — and what it does not.** `TaskOutcome` carries the `agent_id` the task
   was **addressed to** (the executor label), following the dispatch interface's meaning. The child's
   own identity arrives on its **event stream** (and in its own audit DB), not in the outcome. See §7
   item 1.

## 4. Decision 3 — snapshots are isolated per agent

1. New snapshots are written to `<workspace>/.riscdom/snapshots/<agent_id>/`, so two agents sharing a
   workspace can both save `snap1` without overwriting each other.
2. Reads fall back to the shared root (`<workspace>/.riscdom/snapshots`), so a snapshot taken before
   v0.8 still lists, restores and deletes; the per-agent entry wins on a name collision.
3. This is the same trick the fleet uses for its data directories: shared where sharing is the point
   (the chain, the workspace), private where it is not (snapshots, settings, sessions, toolchains).

## 5. Decision 4 — the dispatch abstraction

1. **Where.** `agent::dispatch` (the `agent` crate): Tauri-free, so the abstraction does not force the
   host-core / host-tauri split. The host and the worker both build on it.
2. **Types.** `Task { id, target, input }`, `TaskId` (`task-<pid>-<seq>`), `AgentId`, `TaskOutcome`,
   `DispatchError::{NoSuchAgent, Failed}`. All serialisable (v0.8 main deliverable 1/2 added the
   derives the wire needs).
3. **Traits.** `AgentHandle` ("run this task, return the outcome") and `Dispatcher` ("turn a task into
   an outcome"). **The remote half is deliberately absent** — an executor in another process or on
   another machine implements `AgentHandle` and plugs into the same dispatcher.
4. **Implementations that exist.** Local: `agent::LocalAgent` (wraps a loop), `agent::LocalDispatcher`
   (routes by `Task.target`), `host_core::HostAgentHandle` (the host's own `run_agent` path), and
   `host_core::StdioExecutorHandle` (a child process over stdio).
5. **Routing rule.** A task names the executor it wants; a task naming an executor that is not in the
   fleet is refused with `NoSuchAgent`. Nothing is sent to a best-guess executor — a wrong executor is
   worse than no executor.
6. **Concurrency.** `AgentHandle: Send + Sync` and the dispatcher holds `Arc<dyn AgentHandle>`, so one
   dispatcher is shared across threads. The supervisor dispatches with `std::thread::scope`, one
   thread per task: several executors work at once, one slow executor does not hold up the rest, and
   no thread-pool dependency is needed.

## 6. The prototype, end to end

1. **The executor** (`worker`, `worker/src/main.rs`): reads **one** `Task` JSON line on stdin, builds
   `AppState::with_data_dir`, runs the host's own `run_agent` path, writes **one** `TaskOutcome` JSON
   line on stdout, and sends its events to **stderr** as JSON lines (so stdout stays parseable without
   filtering). Usage: `worker --workspace <dir> --data-dir <dir> [--sleep-ms <n>]`.
2. **Exit codes.** `0` whenever an outcome was written — a failed run is an answer, not an error; `2`
   for a usage error, with no stdout line, which the supervisor reports as a protocol failure. A task
   that cannot be read is answered with the documented placeholder ids `task-unparsed` / `unparsed`.
3. **The supervisor** (`worker::supervisor`, driven by `worker/examples/dispatch.rs`):

   ```text
   cargo build -p worker
   cargo run  -p worker --example dispatch                     # two executors, one demo task each
   cargo run  -p worker --example dispatch -- --executors 3 --tasks tasks.jsonl
   ```

   An executor with no LLM configured answers `Failed` (the readiness check refuses first), which is
   what the offline demo shows: routing, concurrency, aggregation — not a model call.
4. **The task list** is JSON lines, one `Task` per line, the same framing the executor protocol uses.
   A bad line fails the whole list and names its line number.

## 7. Open items handed to v0.9

1. **The child's identity does not reach `TaskOutcome`.** `AgentHandle::run` returns `AgentOutcome`, so
   `LocalDispatcher` stamps the addressed target. Two ways to close it: add an `executor` field to
   `TaskOutcome`, or let `AgentHandle::run` return the child's `TaskOutcome` (the latter changes a
   released trait signature, which is why v0.8 only reported it).
2. ~~**`tauri` is not optional**, so the executor binary links it.~~ **Closed** in v0.9's A1 wave
   3: the executor depends on `host-core`, so the crate split — not a feature flag — is what
   removed the link. Kept as the record of what the cost was.
3. **The preflight directory** (`<workspace>/.riscdom/preflight`) is still shared and unlocked between
   processes in one workspace; it can be isolated the way snapshots were, or left with
   last-writer-wins.
4. **No AI supervisor.** The supervisor for this stage is a dispatcher, not an agent: no LLM loop, no
   prompts, no budget policy.
5. **No remote executor.** The trait seam exists; nothing implements it for another process or machine
   beyond the local stdio handle.
6. **The sessions DB waits for a lock, and still has no WAL** (v0.9). SQLite's default
   `busy_timeout` of zero made a second process's write fail on the spot — and a failed open took
   the whole instance down with it. Two processes can meet on one file (two default-path CLI or
   server processes, or a shared `--data-dir`), so the connection now waits five seconds, set
   before its first write, the shape the audit store uses. WAL stays off **on purpose**, unlike the
   audit store: that one is shared across processes by design, this one is per instance, so the two
   concurrency models differ (decisions §54).
7. **Temporary directories are never removed by the code.** Tests and the demo leave
   `riscdom-*` entries in the system temp directory; `scripts/clean-temp.ps1` / `clean-temp.sh` clear
   them (dry run by default, `-Force` / `--force` to delete). One-off artefacts from manual debugging
   (a `wdbg-*` prefix, say) are outside that filter and are the operator's to remove.
