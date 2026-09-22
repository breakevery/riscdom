[中文](CHANGELOG.zh-CN.md) | English

# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

**The control plane can be driven, and it is guarded.** The 27 control endpoints of
`docs/control-plane-api.md` §5.2 are live — run an agent, manage sessions, save and
resume snapshots, stop the VM, set the toolchain and QEMU paths, run the preflight,
configure the LLM, export the audit — and the server now requires a bearer token for
every request unless it is started with `--no-auth`. The event stream gained its
`Last-Event-ID` replay and `gap` frames.

**Every endpoint now checks a capability, and the two transports agree on identity.** The
control plane does not just name what each route needs: the server checks it against the
actor the `Authn` hook returned and refuses with `403` when the actor does not hold it.
The identity a sink stamps is now taken from its source, so one event cannot look like two
agents.

**The host is split into a portable half and a Tauri half.** `host-core` now holds
everything the host does without a webview, and its dependency tree contains no Tauri
crate; `host` keeps the commands, the Tauri transport and the Tauri dependency, and
re-exports the portable surface, so nothing else changed in this wave.

### Added

- **`host-core`, a new workspace crate**: the audit wiring, `AppState`, snapshots,
  sessions, the toolchain and QEMU download paths, the preflight, `run_diff`, `paths`,
  `settings`, `keyring`, `error`, `dispatch`, `executor` and the event envelope with the
  `EventSink` trait. `agent` / `sandbox` / `audit` are its only workspace dependencies,
  and `cargo tree -p host-core` names no Tauri crate.

### Changed

- **`host` is a facade over `host-core`** (A1 wave 1; three waves still to come). It still
  provides `host::commands`, `host::events::TauriEventSink` and the Tauri dependency, and
  it re-exports the portable surface (`pub use host_core::*`) — so `worker`, `server`,
  `ui/src-tauri` and the 39 `host/tests` files compile unchanged, which is what made this
  wave green with no temporarily-red state. `host-core/README.md` documents the split and
  the no-Tauri constraint.

### Added

- **Capability enforcement.** Every served route declares exactly one capability, as a typed
  column of the route table (`server/src/routes.rs`) — a route cannot be written without
  naming one, so no path skips the check. Before the handler runs, the request path asks the
  `Actor` whether it `allows` that capability and answers `403 forbidden` with
  `cause: "capability"` if it does not (`server/src/http.rs`). Default deny; the vocabulary
  is the 28 names of the API document's §5 tables.
- **`Actor::capabilities`**: the actor a hook returns now carries the set it may use, so a
  hook can narrow a caller without new plumbing. v0.9 has two shapes — the token holder
  (`operator`) and the `--no-auth` hook — and both hold all 28, so a `403` only comes from a
  hook that returns a narrower actor. Per-capability tokens are v1.0 work.

### Changed

- **Identity comes from the source, and the API says so.** `Server::sink()` no longer takes
  an `agent_id`: it uses `AppState::agent_id()`, so two transports in one process ("two
  sinks, one event") cannot stamp the same event with different identities. A new end-to-end
  test publishes one event through two sinks and asserts the `agent_id` matches on the wire.
  The API document's §3 is now "Authentication and capabilities" and states plainly that the
  hook authenticates while the server authorises; the client guide gained a capability
  section and a secure-deployment example (bind narrowly, keep the token file owner-only,
  terminate TLS at a proxy, and never pair `--no-auth` with a public bind).

### Added

- **The 27 controls** (`POST`), plus `POST /v0/runs/abandon-stale` (the reserved gap G4)
  and the still-reserved `POST /v0/vm/start` (501). Bodies are JSON objects, validated
  against the documented vocabularies, with `400`, `409` and `404` answers where the
  request is the problem and the host's own errors mapped into the same model.
- **`TokenAuth` as the default**: 32 random bytes (`getrandom`) written to
  `<data-dir>/token` on first start, mode `600` on Unix and an owner-only ACL on Windows
  (verified with `icacls`; the server refuses to start if it cannot be restricted),
  compared in constant time (`subtle`). The token is never printed or logged. `--no-auth`
  turns the requirement off and prints a warning; `--auth` is the default.
- **`Last-Event-ID` replay and `gap` frames**: frames carry a server-wide ordinal, the hub
  keeps the last 1024 in memory, and a cursor older than that gets a `gap` frame naming the
  oldest id still held.

### Changed

- **A frame's `id` is minted per server, not per connection** (`<ts>-<seq>`): a cursor has
  to mean the same thing after a reconnect. The document's description of `seq` was
  updated to match.

**The audit store's open path is no longer a concurrency hazard.** Opening one fresh
`audit.db` from two processes at the same moment used to fail one of them: `PRAGMA
journal_mode = WAL` needs exclusive access and answers `SQLITE_BUSY` *without* consulting
the busy timeout (SQLite skips the handler when waiting could deadlock), so the 5 s timeout
that covers writes never covered the switch. The open sequence — connect, configure, create
the schema, migrate the columns — is now retried on a locked database, with the same budget
and backoff shape the append path has used since v0.8. Nothing else in the sequence changed,
and nothing about the chain changed.

### Fixed

- **`AuditStore::open` retries a locked database** (`OPEN_MAX_ATTEMPTS` /
  `OPEN_BACKOFF_BASE`, defined from the append path's constants so the two cannot drift
  apart): a fresh connection per attempt, retrying only `SQLITE_BUSY` / `SQLITE_LOCKED`, and
  the failure is still reported — never a silent fallback — once the budget is spent. Pinned
  by a test that races 8 threads on one new file for 25 rounds, one that does the same against
  a file already in WAL, and one that shows a lock outlasting the budget is an error.

**The control plane answers queries and carries one event envelope.** The 26 query
endpoints of `docs/control-plane-api.md` §5.1 are live, with a new
`method_not_allowed` (405) code and a reserved `/v0/resources` (501); and every event,
whatever the transport, now travels in the envelope settled in
`docs/control-plane-events.md` — the SSE stream, the Tauri webview (which unwraps at its
one boundary) and the worker's line protocol.

### Added

- **The 26 query endpoints** (`GET`): audit status and events, runs (list, one, diff),
  the LLM views, sessions, snapshots, VM state, toolchain, QEMU, preflight, settings,
  workspace and serial — plus the host-local `/v0/health`, `/v0/status` and
  `/v0/events`, and the reserved `/v0/resources` answering `501`. Required parameters
  are validated (`400` with the offending name in `cause`), paths take part in a
  `405 method_not_allowed`, and every endpoint's capability is declared in the route
  table and handed to the `Authn` hook.
- **The envelope** in `host/src/events.rs`: `version` / `kind` / `event` /
  `agent_id` / `task_id` / `ts` / `payload`, built by each transport. The
  `EventSink` trait keeps its signature, so no emit site changed shape.
- **[docs/control-plane-client-guide.md](docs/control-plane-client-guide.md)**: the
  client-side walkthrough — curl per endpoint, response examples, the error table, and an
  SSE subscription example.

### Changed

- **Three event payloads** follow the document now: `vm:state` always carries `name`
  (`null` unless it is a snapshot), `audit:failed` carries `message` (it was `error`),
  and `toolchain:download` is tagged `state` (it was `kind`). The eight other payloads
  are unchanged. The webview unwraps the envelope in `ui/src/api/tauri.ts`, so panel
  callbacks read the payload they always read.
- **`TauriEventSink::new` takes the agent identity**, which every envelope it sends
  carries.

**The control plane has a process now.** v0.9's main line is a control plane: a human
supervising AIs and an AI supervising AIs go through the same HTTP + SSE interface. This
batch lands the skeleton the rest of that API is built on — a new `server` crate, two
smoke endpoints, the event stream and the authentication hook — and nothing else: no
kernel source changed and no event emit site changed.

### Added

- **`server`, a new Layer 3 workspace crate** (binary `riscdom-server`): it depends only
  on `host` (Layer 2) and names no Tauri type. It binds `GET /v0/health`,
  `GET /v0/status`, and `GET /v0/events` (SSE), answers everything else with the error
  model of `docs/control-plane-api.md` §4, and installs the `Authn` hook with `NoAuth`
  as the v0.9 default. The host's events reach the stream through `HttpEventSink`, handed
  to `AppState::run_agent` as its `emitter` argument — the host needs no change to be
  driven from a second process. `httpdate` enters the lock file as hyper's `server`
  feature's only dependency that was not already there.
- **`gap` frames and `Last-Event-ID` replay are not implemented yet**: the stream ships
  the `hello` and `event` kinds, and the design's place for the third is marked in
  `docs/control-plane-events.md`.

**The environment preflight is per agent too.** `<workspace>/.riscdom/preflight` was the last write path a
workspace still shared: two processes sharing one workspace compiled the preflight guest into the same
`guest.c` / `guest.elf`, and booted their preflight VM into the same directory at the same time. New
artifacts go to `<workspace>/.riscdom/preflight/<agent_id>/`, and the guest to boot is resolved from this
agent's own directory first, then the shared root — so a guest an older version left behind stays usable
instead of being orphaned. The cached result in `settings.json` was already per instance and is untouched.

### Changed

- **The preflight guest, and the preflight VM's snapshot directory, are per agent** (follow-up A2):
  `AppState::preflight_dir` / `preflight_root` / `write_preflight_guest` / `find_preflight_guest` are
  public, and the layout is pinned by a filesystem test that compiles nothing and boots nothing. Same
  pattern as v0.8 batch B's snapshots; the audit chain is not involved.

**A dispatched outcome now names the executor that actually ran the task.** `TaskOutcome.agent_id` used to be
stamped by the dispatcher with the `Task.target` it routed to — which for a child process is a label the
supervisor invented, not the identity that did the work. `AgentHandle::run` returns a `TaskOutcome` now (it
returned a bare `AgentOutcome`), so each handle fills in the identity it alone knows: a local loop's own id,
the host instance's id, or — for `StdioExecutorHandle` — the identity the child announces in its
`worker:ready` event, with a missing announcement reported as `DispatchError::Failed` rather than guessed at.
`LocalDispatcher` passes the handle's outcome through; its only judgement left is the route. Nothing about the
audit chain, the hash formula or the append-only triggers changes.

### Changed

- **`TaskOutcome.agent_id` means "who ran this", not "who it was sent to"** (main deliverable 3/3):
  `AgentHandle::run` returns `Result<TaskOutcome, DispatchError>` and builds the record itself; `LocalAgent`
  and `HostAgentHandle` fill their own identity, `StdioExecutorHandle` fills the child's announced one, and
  `LocalDispatcher` no longer assembles anything. The two differ whenever the executor is a child process,
  which is exactly what a supervisor needs to tell "executor-0" apart from the process that answered.

## [0.8.0] - 2026-09-22

**v0.8 batch 1 — technical-debt cleanup ahead of the multi-agent runtime.** Three dead-ends the
architecture re-assessment named are cleared. Nothing on the golden path changes.

**v0.8 batch 2 — several processes can write one `audit.db`, and a failed write is loud.**
`audit.db` is shared on purpose (one chain per workspace), which is exactly what the multi-agent
runtime needs: several processes appending to the same file. The connection now opens in WAL mode
with a five-second busy timeout and `synchronous=NORMAL`; an append takes the write lock **before**
it reads the head (`BEGIN IMMEDIATE`); and a locked database is retried with backoff before it is
reported. A write that still fails is **never dropped silently**: `AuditSink::record` returns the
error, the sink tells the host, and the host logs it, sends an `audit:failed` event and — by
default — shows a banner and a popup in *Settings → Audit*. Only the alert can be switched off.

**v0.8 batch 3 — every audit event names its agent, and snapshots stop colliding.**
`agent_id` was a field with no producers; it now reaches every writer. An identity is
`<device>-<pid>-<seq>` (`agent::next_agent_id`; `local-<pid>-<seq>` on one machine) — one per process,
and one per agent inside it, from a process-wide counter. The host mints one at construction and hands
it to the loop it builds, so the sandbox's events and the agent's events carry the same identity as the
host's. Snapshots move to `<workspace>/.riscdom/snapshots/<agent_id>/`, so two agents sharing a
workspace can both save `snap1` without overwriting each other; reads fall back to the shared root, so a
snapshot taken before this change still lists, restores and deletes.

**v0.8 batch 4 — a task can be handed to an executor, and the caller does not care where it runs.**
The multi-agent runtime needs something the host has never had: a way to *dispatch* work instead of
calling an agent inline. `agent::dispatch` adds the vocabulary — `Task`, `TaskId`, `AgentId`,
`TaskOutcome`, `DispatchError` — and the two traits that make up the seam: `AgentHandle` (an executor:
"run this task, give me the outcome") and `Dispatcher` ("turn a task into an outcome"). The **local**
half is implemented: `LocalDispatcher` routes a task to the handle that owns its target, and the host
wraps its existing `run_agent` path in `HostAgentHandle`. The **remote** half is deliberately absent —
that absence *is* the seam: a handle reaching another process or machine implements `AgentHandle` and
drops into the same dispatcher with nothing above it changing. The Tauri commands keep calling
`run_agent` exactly as before; the dispatch path is an added internal route, not a replacement.

**v0.8 main deliverable 1/2 — the same dispatch interface, with the executor in another process.**
A new `worker` crate is the executor binary: it reads **one** `Task` JSON line on stdin, runs the
host's own `run_agent` path against a data directory it was given, and writes **one** `TaskOutcome`
JSON line on stdout. Its events go to stderr as JSON lines, so stdout stays a channel a supervisor
can parse without filtering. On the supervisor side, `host::StdioExecutorHandle` is an `AgentHandle`
whose executor is that child process: it spawns it, writes the task, reads the outcome, drains the
events, and kills it if it does not answer in time. Nothing above the handle changed — the dispatcher
cannot tell a local executor from a process boundary, which is exactly the seam v0.8 batch 4 left
open. `AgentOutcome`, `TaskOutcome` and `DispatchError` gained `Serialize`/`Deserialize` for the wire
(pure addition: plain `String`/`u32` fields, nowhere near the chain). Transport is stdio + JSON lines,
which needs **no new dependency**. Two known edges: the worker links Tauri (because `host` depends on
it unconditionally — v0.9 makes it optional), and the child's own agent identity comes back through
its **events**, not through the dispatcher's `TaskOutcome` (whose `agent_id` follows batch 4's meaning:
the executor the task was addressed to).

**v0.8 main deliverable 2/2 — a supervisor drives several executors at once.** The prototype is now
demonstrable end to end: `worker` gained a library half (`worker::supervisor`) and a runnable demo
(`cargo run -p worker --example dispatch`), which starts N executor processes that **share one
workspace** and each own a **data dir**, routes tasks to the executor named in `Task.target`, and
prints one line per task plus a tally. Tasks are dispatched **concurrently** (one `std::thread::scope`
thread per task — the handles are `Send + Sync`, so no thread-pool dependency is needed), and a task
naming an executor that is not in the fleet is **refused** rather than handed to a best guess. There is
no model anywhere in the supervisor: for this stage the supervisor is a dispatcher, not an agent.
Housekeeping in the same batch: `worker`'s `audit` dependency was declared but never used (the executor
reaches the chain through `host::AppState`) and is gone; the supervisor logic lives in `worker`'s
library so the demo and the tests share one implementation instead of two copies of the loop; and the
four settled decisions are written down in [docs/multi-agent-foundation.md](docs/multi-agent-foundation.md).

### Changed

- **`worker` gained a library target, and lost an unused dependency** (v0.8 main deliverable 2/2):
  the crate now has a library (`worker::supervisor`) next to the executor binary, so the demo and the
  tests share one supervisor implementation; and its `audit` dependency was declared but never used
  (the executor reaches the chain through `host::AppState`), so it is removed.
- **`host::dispatch::outcome_from_view` is public** (v0.8 main deliverable 1/2): the one
`AgentOutcomeView` → `AgentOutcome` mapping used to be private; the out-of-process worker reuses it
instead of keeping a second copy. No behaviour change.
- **`agent_id` has producers** (v0.8 batch 3): `AgentLoop` takes an identity at construction (`new` /
`with_vm` gained the parameter) and stamps it onto every event it writes — including the ones written
through `audit_hook` and the tool layer, which now take the id as well. The host mints one per
`AppState` (`local-<pid>-<seq>`) and stamps its own events and its run markers with it. The field stays
**beside** the chain: no hash formula, `prev_hash` link or historical row changes.
- **Snapshots are per agent** (v0.8 batch 3): new snapshots go to
`<workspace>/.riscdom/snapshots/<agent_id>/`; `list_snapshots` reads that directory and the shared root
(the per-agent entry wins on a name collision), `resume_from_snapshot_real` and `save_snapshot_real`
resolve through the same fallback, and `delete_snapshot` removes from both. A pre-v0.8 snapshot keeps
working.
- **Audit writes survive several processes** (v0.8 batch 2): `journal_mode=WAL`, `busy_timeout=5s`
and `synchronous=NORMAL` are set in one place when the connection opens (`audit::store`); a locked
append is retried up to five times with 20/40/80/160 ms backoff; and the read-head-then-insert pair
runs inside `BEGIN IMMEDIATE` — without that last part WAL alone still let two writers chain onto
the same row and fork the chain (the new concurrency test caught it). `AuditSink::record` now
returns `Result<(), AuditError>` instead of dropping a failed event on the floor; the sandbox and
agent-loop call sites report through `audit::report_failure`. The chain structure, the hash formula,
the historical rows and the append-only triggers are untouched.
- **The app-data directory is injected, not global** (v0.8 batch 1): `host::paths` kept its default
data directory in a `OnceLock`, so the first caller won and every later caller was silently ignored —
a second `AppState` in one process could not have its own data directory. The default is now a
re-settable `RwLock`, and `AppState::with_data_dir(workspace, data_dir)` resolves `settings.json`,
the sessions DB and the toolchain download directory inside a directory the instance owns. The Tauri
shell uses it, and a host test pins two instances writing to two different files.
- **One VM slot per `AppState`** (v0.8 batch 1): the host-owned slot was already a per-instance field
(`AppState::vm_slot`), not a process-wide singleton. The batch records that with a test — two
instances keep separate chains and separate slots — so nothing re-shares them by accident.

### Added

- **The supervisor** (v0.8 main deliverable 2/2): `worker::supervisor` (a library half on the `worker`
  crate) plus the runnable demo `worker/examples/dispatch.rs`. `ExecutorSpec` describes one executor
  (label, program, args, optional deadline, environment); `dispatcher(&specs)` builds a
  `LocalDispatcher` over stdio handles; `dispatch_all` runs the tasks concurrently with
  `std::thread::scope` and returns the results **in input order**; `tally` / `report` count and print
  answered / refused / broken; `parse_tasks` reads the JSON-lines task list and names the line number
  of a bad line. A panicking dispatch thread becomes a failed outcome instead of taking the plan down.
- **`docs/multi-agent-foundation.md`** (+ [中文](docs/multi-agent-foundation.zh-CN.md)) (v0.8 main
  deliverable 2/2): the four settled decisions — the B2 process model, the `<device>-<pid>-<seq>`
  identity, per-agent snapshots, the dispatch abstraction — written from the code, with the open items
  handed to v0.9 (including the child identity not reaching `TaskOutcome`).
- **`worker`** (v0.8 main deliverable 1/2): the executor process. Usage:
`worker --workspace <dir> --data-dir <dir> [--sleep-ms <n>]` — the paths are required and have no
environment fallback, because an executor's identity is its command line and an inherited variable
that silently redirects it is worse than a missing argument. It builds `AppState::with_data_dir`, so
each executor owns its `settings.json`, `sessions.db` and toolchain directory. Exit code is **0
whenever an outcome was written** — a failed run is still an answer, written as
`TaskOutcome { outcome: Failed { .. } }` — and 2 for a usage error (no stdout line; the supervisor
reports a protocol failure). A malformed task is answered, not crashed: the outcome carries the
documented placeholder ids `task-unparsed` / `unparsed`.
- **`host::StdioExecutorHandle`** (v0.8 main deliverable 1/2): the supervisor-side `AgentHandle` for a
child process — one task line in on its stdin, one outcome line out on its stdout, a deadline that
kills a worker that never answers, the child's stderr drained into `events()`, the child's exit status
in every failure message, and an answer that names a different task rejected rather than passed up.
`with_env` / `with_env_removed` let the supervisor decide what an executor inherits.
- **Serde on the dispatch wire types** (v0.8 main deliverable 1/2): `AgentOutcome`,
`TaskOutcome` and `DispatchError` now derive `Serialize` + `Deserialize` (the first two were already
serialisable in shape; `thiserror`'s `#[error]` and serde coexist on the third). Additive only.
- **`agent::dispatch`** (v0.8 batch 4): the dispatch vocabulary and seam. `Task { id, target, input }`;
`TaskId` (`task-<pid>-<seq>`, from a process-wide counter, so ids are unique inside a process and
across processes); `AgentId` (the batch-3 `<device>-<pid>-<seq>` shape, as a newtype);
`TaskOutcome { task_id, agent_id, outcome }`, whose `outcome` is the executor's own `AgentOutcome`
unchanged; and `DispatchError::NoSuchAgent` / `::Failed`. The traits are `AgentHandle` (`agent_id`,
`run(&self, &Task)`) and `Dispatcher` (`dispatch(Task) -> Result<TaskOutcome, DispatchError>`).
`agent::LocalAgent` wraps one loop as an executor and `agent::LocalDispatcher` routes by target;
the host adds `HostAgentHandle` (its `run_agent` path) and `host::local_dispatcher`.
**No remote implementation is written** — that is the seam, left open on purpose.
- **`agent::next_agent_id`** (v0.8 batch 3): the identity helper — `DEVICE` (the machine; `local` for
now) plus a process-wide sequence, so `local-<pid>-1`, `local-<pid>-2`, … are unique inside a process
and across processes.
- **The audit-failure alert** (v0.8 batch 2): `settings.json` gains `alert_on_audit_failure`
(default `true`, so an upgrade cannot switch it off), *Settings → Audit* shows the toggle and a
banner, and a new failure also raises a dialog — the `dialog:allow-message` permission is granted
for it, and the dialog probe pins the permission set. The `audit:failed` event and the log line are
sent whatever the setting says. New command: `set_audit_alert`.
- **`agent_id` on every audit event** (v0.8 batch 1): `AuditEvent` gains an optional `agent_id` (set
with the `with_agent` builder), stored in a new `audit_events.agent_id` column that an older database
picks up on the next open. It sits **beside** the chain: the hash formula, the `prev_hash` linkage
and every existing row's `hash` are untouched, so a pre-v0.8 chain verifies exactly as it did before.
Producers leave it `None` until the multi-agent runtime gives them an identity. The JSONL export and
`list_audit_events` carry it.

## [0.7.0] - 2026-09-21

**v0.7 is on `main` and unreleased: a self-built i18n facility, a language switch, and macOS/Linux
builds.** `v0.6.0-preview.1` is still the Latest release.

### Added

- **A self-built i18n facility** (v0.7 batches 1–2): `ui/src/i18n/` holds a two-language string
  registry with no third-party library; `scripts/check-ui-strings.mjs` (in the gate, with its own
  self-test) requires every key in both languages; and *Settings → Appearance* has a language chooser
  (follow the system / 中文 / English) that persists to `settings.json`, moves `lang` on `<html>` and
  re-renders live. The four v0.6 diff strings are the pilot. **Rolling the registry out over the
  remaining ~190 interface strings is deliberately not done** — the facility is the value, and a
  kernel-shaped tool does not need a fully bilingual surface.
- **macOS and Linux builds** (v0.7 batch B): a `bundle` job in `ci.yml` (dispatch or a `v*` tag;
  macOS and Linux runners) runs `npm run tauri build` and uploads the results as artifacts — macOS
  `.app` + `.dmg`, Linux `.deb` + `.rpm` + `.AppImage`. Verified end to end in run `35572294916`.
  The packages are **unsigned**: Developer ID signing and notarization belong to the commercialisation
  layer, so macOS Gatekeeper blocks a first run.
- **Platform-aware QEMU guidance** (v0.7 batch A): the "QEMU not found" guidance follows the platform
  (`winget` / Homebrew / the distribution's package, via `sandbox::qemu_discover::install_hint_for`),
  `icons/icon.icns` exists for the macOS bundle, and the Unix `-qmp unix:` argument is pinned by a
  cross-platform unit test.

### Fixed

- **`host` did not compile off Windows** (v0.7 batch 8): `fn extract_zip` is Windows-only, but the
  `ArchiveKind::Zip` arm calling it was not, so every macOS/Linux build failed with `error[E0425]:
  cannot find function 'extract_zip' in this scope`. Non-Windows platforms now get a same-named stub
  that reports "zip archives are not supported on this platform", and `zip` stays a Windows-only
  dependency. The new `bundle` job found this: it was the first time `host` was ever compiled off
  Windows, because the Linux gate skips `host` entirely.

## [0.6.0-preview.1] - 2026-09-19

**The golden path's eighth step ships as a preview: two runs compared field by field.** It has not
been walked by a human — not on a clean machine, and not on this one — so it is a preview, and
`v0.5.0` remains the Latest release: a pre-release takes no Latest marker. What this preview is and
what it does not prove is in [RELEASE_NOTES.md](RELEASE_NOTES.md).

### Added

- **Two runs' fingerprints, field by field** (v0.6 batch 1, data layer + API): `host/src/run_diff.rs`
  turns the two fingerprint documents into an ordered list of their top-level fields — the field name,
  both values and whether they differ — in the order `AppState::run_fingerprint` declares them (never
  alphabetical). Nested values are compared as a whole, the list covers every field the two documents
  carry even when nothing differs, and only fields they actually carry appear in it.
  `AppState::compare_run_fingerprints(run_a, run_b)` reads both documents off the chain's `run.start`
  events, and the `compare_run_fingerprints` command exposes it to the UI.
- **The field-level diff, in the audit tab** (v0.6 batch 2): under the two-run side-by-side panel, a
  collapsed block whose header counts the fields and the differences (`字段级差异 · 7 个字段 · 3 处不同`,
  and `· 0 处不同` for two runs configured identically). Expanded, each row is the field name, the
  first run's value and the second run's value; a row whose values differ is highlighted, an equal one
  is dimmed, and values are shown **whole** — monospace and wrapped, never truncated. The panel renders
  the host's rows in the order they arrive and re-sorts nothing.

### Notes

- **This preview's MSI carries a separately pinned installer version.** `0.6.0-preview.1` is a valid
  semantic version but not a valid MSI `ProductVersion` (WiX takes `major.minor.patch.build`, numeric
  only), so `bundle.windows.wix.version = "0.6.0"` in `tauri.conf.json` supplies the numeric form
  while the package version — and therefore the artifact names — stays `0.6.0-preview.1`. Remove or
  update that field once the package version is numeric again.

## [0.5.0] - 2026-09-19

**The golden path is complete, and this release is the preview's work with its walkthrough findings
fixed.** One walk of the whole path has been done — on the developer's machine, not a clean one — and
its result is [walkthroughs/2026-09-19-preview1-local.md](walkthroughs/2026-09-19-preview1-local.md);
what that does and does not prove is in [RELEASE_NOTES.md](RELEASE_NOTES.md).

### Added

- **A recorded walkthrough of the seven steps** (v0.5 batches 10–11): install → create an environment
  → run a task → save a snapshot → export the audit record → roll back → change one configuration
  field and run again, with a real model, a real API key and the **installed MSI**. Every step
  passed; the record lists the five findings the walk produced and what happened to each.
- **`scripts/scan-encoding.py`** (v0.5 batch 13): a hand-run diagnostic for the 0x3F family of
  encoding accidents (U+FFFD, runs of `?`, `?`-only literals, `?` beside CJK, UTF-8-read-as-GBK).
  Deliberately **not** in the gate — its `?`-literal rule cannot tell damage from legitimate code.

### Fixed

- **The audit tab's run rows were invisible apart from their checkbox** (walkthrough S-1): the global
  `input { width: 100% }` rule applied to the checkbox too, so it filled the row and pushed the
  status, fingerprint, time and 导出 button out of view. The checkbox now has its own size, the row's
  text shrinks with an ellipsis instead, the controls never shrink, and the settings pane uses the
  window's width rather than its own content's.
- **The snapshot prompt opened with a hard-coded `snap1`** (G-1): the default now comes from the
  clock (`snap-YYYYMMDD-HHMM`), so pressing Enter no longer reuses one name for every snapshot.
- **Tool-call rows read `?? write_source ?`** (E-1): those JSX lines had been written with literal
  `?` characters where their markers used to be; the row now shows a CSS status dot and a word.
- **The preflight sentence looked clickable where it was not** (E-2) and **the audit actor filter
  only applied on blur** (E-3): the sentence points at the button beside it, and the filter follows
  what you type.
- **QEMU opened a console window over the app** (G-4): the sandbox starts QEMU with
  `CREATE_NO_WINDOW` on Windows. `-display none` hides the guest's display, not that console.

### Changed

- **The preview's MSI version override is gone.** `bundle.windows.wix.version` existed only because
  `preview.1` is not a valid MSI `ProductVersion`; `0.5.0` is numeric, so the MSI now carries the
  package version and *Apps & features* shows `0.5.0`. `scripts/check-wix-version.mjs` (in the gate)
  fails if that override ever comes back alongside a numeric version.

## [0.5.0-preview.1] - 2026-09-19

**A preview, for people who will walk the golden path on a clean machine.** A preview has not been
verified on a clean environment yet: what a tester needs, and how to report back, is in
[RELEASE_NOTES.md](RELEASE_NOTES.md).

### Added

- **The golden path, all seven steps** (v0.5): install → create an environment → run a task → save a
  snapshot → get the audit record → roll back → change one configuration field and run again. The
  design, the settled decisions and the smallest honest scope are in
  [docs/golden-path.md](docs/golden-path.md).
- **A run's record exports self-contained** (v0.5 batches 1–4): *Settings → Audit* exports one run as
  JSONL, written from the chain's **first event** to the event that **closes the run**, so the file's
  first line is anchored at genesis and an empty database plus `audit-verify` judges it with nothing
  carried over from the machine that produced it. An abandoned run's file ends on its
  `host.run.abandoned` marker; an open run is refused rather than exported to wherever the chain
  happens to stop. The mid-chain slice form was removed rather than kept beside it (batch 4): two
  meanings of "export" is one meaning too many.
- **A run names the snapshot it came from** (v0.5 batch 3): `resumed_from_snapshot` joined the derived
  `runs` index — rebuilt from the chain like every other column — travels through `RunView` to the run
  list, and is shown in the audit tab and in the two-run comparison. A database written earlier picks
  the column up through an `ALTER TABLE runs ADD COLUMN` migration.
- **Two runs side by side** (v0.5 batch 2): the audit tab's run list takes two selections and shows
  their short and full fingerprints, start time, status and source snapshot. A field-by-field
  fingerprint diff stays v0.6 work.
- **A contributor licence agreement** (v0.5 batch 5): [CLA.md](CLA.md) with a Chinese reference
  translation, a CLA section in [CONTRIBUTING.md](CONTRIBUTING.md), a
  [`.github/workflows/cla.yml`](.github/workflows/cla.yml) that runs the CLA Assistant on
  `pull_request_target` **without checking out the pull request's code**, and a pre-created
  `signatures/version1/cla.json`.
- **A manual checklist for the golden path** (v0.5 batch 3):
  [docs/golden-path-checklist.md](docs/golden-path-checklist.md) — the fields a person fills in while
  walking steps 1–2 on a clean machine, with a worked example.
- **An `--ignored` walk of steps 3–7** (`host/tests/golden_path.rs`): a real QEMU guest driven by a
  mock LLM through run → snapshot → export → restore → change one field → run again, ending in
  `audit-verify` over the exported file.

### Changed

- **The README's licence note is short again** (v0.5 batch 6): the code is Apache-2.0, and a
  contribution needs the [CLA](CLA.md). The open-core wording left the public README; the licence
  grants themselves stay in CLA.md, which is what a contributor actually signs.
- **The CLA section of CONTRIBUTING is conditional** (v0.5 batch 6): "if you contribute to this
  repository", because the contribution flow may move elsewhere later.

### Notes

- **This preview's MSI carries a separately pinned installer version.** `0.5.0-preview.1` is a valid
  semantic version but not a valid MSI `ProductVersion` (WiX takes `major.minor.patch.build`, numeric
  only), so `bundle.windows.wix.version = "0.5.0.1"` in `tauri.conf.json` supplies the numeric form
  while the package version — and therefore the artifact names — stays `0.5.0-preview.1`. Remove or
  update that field once the package version is numeric again.

## [0.4.0] - 2026-09-19

### Added

- **QEMU setup is guided, not downloaded, and the third-party notices exist** (v0.4 #4): the app says
  what to run — `winget install SoftwareFreedomConservancy.QEMU` where `winget` exists, the official
  download page otherwise — instead of fetching QEMU itself. Upstream publishes no Windows binary to
  pin, a third-party packager would be an unnamed supply-chain link, and building QEMU ourselves would
  make us the distributor of a GPL-2.0 binary ([docs/qemu-distribution.md](docs/qemu-distribution.md)
  §5). The downloader written along the way (`host/src/qemu_download.rs`) stays in the tree unwired,
  with an empty spec table, because a digest nobody can reproduce is worse than no download. What we
  rely on is written down in [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
- **Theme switching**: light / dark / follow the system, chosen in *Settings → 外观* and stored in
  `settings.json`. Every colour in the stylesheet is now a token, so a theme is one token block;
  the serial terminal follows the same tokens.
- **`scripts/clean-temp.ps1` / `scripts/clean-temp.sh`** remove RiscDom's directories from the
  system temp directory. Dry run by default; `-Force` / `--force` deletes. Only entries whose
  name starts with `riscdom-` are matched, and `<temp>/riscdom` (the fallback data directory)
  is excluded explicitly.
- **End-to-end failure-path diagnostics** (stage 5c-3): an end-to-end run now prints a report
  naming the first failing step, that step's own output, the serial state (including “the
  guest never printed anything”) and the audit-chain verdict. How to read it:
  [docs/e2e-debugging.md](docs/e2e-debugging.md).
- **Environment capability preflight**: after the toolchain or QEMU path changes (and
  on the first run with a stale cache) the host compiles a minimal guest and boots
  it on the real paths, reporting which of the four steps failed. It is warn-only,
  cached in `settings.json` against the configuration fingerprint, never writes to
  the audit chain, and offers a recorded "continue anyway". Version-number rules
  were deliberately not implemented: no QEMU × GCC compatibility matrix exists in
  this repository (see `PROJECT_CONSTITUTION.md` §10).

### Changed

- **Relay ports come from a process-wide lease** (v0.4 #1): `sandbox::relay::lease_local_port` /
  `lease_local_ports` replace `free_local_port`, returning a `PortLease` that keeps the port — and a
  bound listener — reserved until it is handed off and dropped. `start_vm`, the snapshot restore and
  the preflight take their ports from it and release the OS-level hold only just before QEMU starts,
  so no two parts of this program can be handed the same port and the port cannot be stolen by
  anyone else until the last moment. The three-attempt retries stay: the hand-off itself cannot be
  made atomic while QEMU binds the port itself. See [docs/qemu-stdio.md](docs/qemu-stdio.md).

### Fixed

- **CI runs the gate instead of keeping its own command list**: `.github/workflows/ci.yml` now
  installs Rust + Node and calls `sh scripts/gate.sh`, so a check can no longer drift between CI and
  a developer machine (the separate frontend job folded into it). The first run of that arrangement
  found a real Linux-only defect: two `use` statements in a test were needed only on Windows and
  failed `-D warnings` on Linux. `scripts/gate.sh` also detects its platform and prints what it
  skips there (Tauri lint/check without the system libraries; the guest-booting tests without QEMU +
  a RISC-V GCC) instead of failing or skipping silently.
- **Stale temp directories are swept at startup**: every `riscdom-*` directory older than 24 h is
  removed when the host starts (directories only). `<temp>/riscdom` — the fallback data directory —
  is whitelisted and never touched, and the examples now use pid-unique names so a cleanup cannot
  delete a running one.
- **Stale build scratch directories are swept at startup**: a build cleans up after itself, and
  the ones a killed process could not clean (older than 24 h) are removed when the host starts.
  Only the `riscdom-build-*` prefix is touched.
- **Build scratch directories are removed when the build finishes**: the per-build path fixed
  the concurrency race but leaked one small directory per compile; success and failure both
  clean up now.
- **Concurrent builds no longer share files**: the injected `crt0.S` / linker script
  used to live at one fixed temporary path, so two builds running at once (a run and
  the preflight, or parallel tests) could compile against a half-written file. Every
  build now gets its own directory.
- **The preflight's compile step has a guard**: a compiler that has not answered within
  30 s is stopped and reported as a timeout, instead of hanging the panel.

## [0.3.1] - 2026-09-17

### Fixed

- **A snapshot restore now honours a manually configured QEMU path**: the restore built its own
  `VMConfig` with `qemu_exe: None`, so it silently fell back to auto-discovery and could boot with a
  different binary than the one configured in *Settings → Toolchain*. The agent loop and the restore
  now inject the configured path through one helper. (Reproduction: the old function did not fail —
  it returned `Ok(())` while ignoring the path. The regression test asserts that the restore must go
  through the configured binary, so that assertion panics on the `Ok`.)
- **A QEMU process that has exited is no longer reported as running**: `vm_is_running` looked at the
  slot only, so a handle left behind by a guest shutdown (or a killed or crashed QEMU) kept the
  top-bar badge on "VM running" forever. The child process is checked as well and a dead handle is
  dropped.
- **`read_serial` returns the captured output when the guest never goes quiet**: the overall wait
  used to answer "No serial output yet" even though the buffer was full (a guest printing in a loop
  never reaches the 150 ms quiet window). Only a genuinely empty buffer reports silence now.
- **A finished run no longer pulls the chat back to the bottom**: the completion handler forced
  scroll-to-bottom even when the reader had scrolled up. It now respects the scroll state and shows
  the "jump to latest" button, matching the serial panel.
- **The two-pane layout stays inside the window**: the chat column was clamped only against a fixed
  240–900 px range, so a wide chat column pushed the serial column off-screen in a narrow window. The
  drag bound is derived from the measured container and the serial column has an adaptive minimum.

> Note: the v0.3.1 entry in RELEASE_NOTES.md / RELEASE_NOTES.zh-CN.md was added in the commit right
> after the tag. The tag itself (`b9be911`) already carries this changelog entry and the complete
> code.

## [0.3.0] - 2026-09-16

### Added

- **One-click download of RISC-V GCC (xPack)**: SHA-256 verified, Zip-Slip protected,
  cancellable.
- **QEMU discovery and a manual path**: `RISCDOM_QEMU` → known paths → `PATH`, plus a manual
  path in *Settings*, persisted to `settings.json` and injected into the agent loop.
- **VM status badge** in the top bar (visible across runs).
- **Settings page with tabs**: Model / Toolchain / Snapshot / Audit / Plugins.

### Changed

- **Two-pane main view** (chat + serial); settings moved to a separate page (Esc returns).
- **The system prompt is now entirely in English.**
- **The AI no longer stops the VM automatically after a task**: the prompt, the `stop_vm`
  tool description and the VM badge guarantee it three times over.

### Fixed

- **Chat and serial auto-scroll** to the latest output; a user scrolling up is not
  interrupted and a "jump to latest" button appears.
- **`read_serial` waits for ~150 ms of silence** before returning, so the first byte is no
  longer truncated.
- **Snapshot resume retries the relay port** on QMP 10054 / bind failure (up to 3 attempts).
- **Test gate stability**: port TOCTOU retry in `start_vm`.

### Notes

- Residual items are tracked in `PROJECT_CONSTITUTION.md` §10 (v0.4).

## [0.2.2] - 2026-09-15

### Fixed

- **The Windows keyring was a silent no-op**: the `keyring` crate ships **no** default backend,
  so a bare `keyring = "3"` compiled to an empty implementation — `set` reported success while
  nothing reached Credential Manager, and every restart lost the key. `host` now opts into
  `windows-native` (and `apple-native` / `linux-native-sync-persistent` on the other platforms),
  so API keys really persist and are read back at startup.

## [0.2.1] - 2026-09-15

### Added

- **Manual toolchain path is persisted**: *Settings → Toolchain* writes the chosen compiler to
  `settings.json` in the app data directory (never into the repo, never a key), so it survives a
  restart. A failed write is audited as `host.settings.save_failed` and never blocks the run.

### Fixed

- **The RISC-V toolchain is discovered, explained and configurable** (stages 24a–24c):
  resolution order is `RISCDOM_RISCV_GCC` → `RISCV_GCC` → well-known install locations → `PATH`,
  accepting both `riscv64-unknown-elf-gcc` and the xPack name `riscv-none-elf-gcc`. When nothing
  is found, the error lists every path that was searched, links the installer and explains how
  to point the app at a compiler; `run_agent` refuses early with a structured `toolchain_missing`
  error and the UI shows a red banner with “probe again” / “set path manually”
  (see `docs/toolchain-setup.md`).
- **No more duplicated error prefix**: a manual toolchain that cannot run is reported once
  (`not runnable: …`) instead of twice.

## [0.2.0] - 2026-09-14

### Changed

- **VM lifecycle moved to host**: the VM is decoupled from `AgentLoop` into
  `AppState::vm_slot`, so it survives the run and the next run reuses the same guest
  (`AgentLoop::with_vm` injection; behaviour is unchanged when nothing is injected). The
  serial forwarder is now **long-lived** (created at app startup) and subscriptions continue
  **across runs**.
- **Serial source is now a sandbox push**: the sandbox serial reader thread fans out frames
  through `VMConfig.serial_observer` in real time → `agent::AgentLoop::subscribe_serial()`
  (`std::sync::mpsc`) → host forwards them as `serial:chunk` and accumulates them for
  `get_serial_buffer()`. No longer derived from the audit log's `read_serial` tool results
  (the old `serial_full_text` / `SerialDiff` are gone). The `read_serial` tool semantics are
  unchanged; observer panics are caught with `catch_unwind` and audited as
  `sandbox.serial.observer_panic`.

### Added

- **Real snapshot save / restore**: host `save_snapshot_real` / `resume_from_snapshot_real`
  (audit `host.snapshot.save` / `host.snapshot.resume`), plus a "save current state" button
  and a per-entry "restore" button in the UI (with confirmation).
- **`real_api` asserts audit-chain integrity** (stage 21): the real-API test's audit backend
  moved from in-memory to **file SQLite**, and after the run an independent handle uses
  `audit::verify_chain` to require `ChainStatus::Intact { length > 0 }` plus at least one
  `agent.llm.request` / `agent.tool.call` / `agent.tool.result` event; the temp DB is cleaned
  up by a `Drop` guard (including on failure).
- **Snapshot panel** (list / delete; real snapshots labelled "real", reboot fallbacks
  labelled "reboot"), host commands `list_snapshots` / `delete_snapshot`
  (audit `host.snapshot.delete`).
- **Session persistence** (list / open / rename / delete / clear): host `SessionStore`
  (SQLite, reusing `rusqlite`) + 7 Tauri commands; sessions are saved automatically under the
  app data directory and survive restarts; restoring injects history messages only (tool
  calls are not replayed) and **never persists** API keys / the system prompt / audit events.
- **Streaming LLM responses (agent + host + UI)**: `LlmClient::chat_stream` (degrades to
  `chat` by default) + the SSE implementation in `OpenAiCompatClient` + the `sse` parser;
  `AgentLoop::subscribe_stream`; host `agent:stream:delta` / `agent:stream:done`; the UI
  appends token by token (the final content supersedes it). Audit records only
  `agent.llm.stream.start` / `.end`, not every chunk.
- CI workflows (`.github/workflows/ci.yml`): secret scanning (gitleaks, full history), Rust
  checks (`fmt --check` / `clippy -D warnings` / `check` / `audit` unit tests, portable
  crates only) and the frontend build (`npm ci` + `npm run build`).
- Local preflight scripts: `scripts/preflight.ps1` (Windows) and `scripts/preflight.sh` (Unix).
- `SECURITY.md`, `.env.example`, and a fuller `.gitignore` (`.env*` / `*.db` / `*.jsonl`, …).

### Security

- Dependency audit (2026-09-14): `cargo audit` scanned 470 crates — **0 vulnerabilities**;
  7 informational warnings (6 unmaintained: `proc-macro-error`, `unic-char-property` /
  `unic-char-range` / `unic-common` / `unic-ucd-ident` / `unic-ucd-version`; 1 unsound:
  `glib 0.18.5`, still a Linux/GTK transitive dependency, not built on Windows).
  `npm audit --omit=dev`: **0 vulnerabilities**.
- The README gained a "security statement"; the v0.2 roadmap gained item **f**
  (pre-launch security checklist).
- No dependency was upgraded by us (warnings left untouched pending a human decision).

### Notes

- **Real snapshots are implemented with plan A′ (TCP migration + a local file relay).**
  Stage 18a showed `migrate` → `file:` is unusable on Windows + QEMU 11.1.0, while
  `migrate` → `tcp:` works; 19b uses a local TCP relay to persist the migration stream as
  `<name>.mig` and, on restore, feeds it back to a QEMU started with `-incoming tcp:`.
  See `sandbox/docs/snapshot-experiment.md`.
  Residual limits: the old reboot fallback (`.json`) is still supported; restore takes
  `-kernel` from the newest `*.elf` in the workspace (the migration stream overwrites memory;
  the kernel only lets QEMU boot).

### Planned (v0.2) — multi-model access and key security

- **LLM client refactor**: `DeepSeekClient` → `OpenAiCompatClient` (`base_url` / `api_key` /
  `model` fully user-configurable; keep the OpenAI-compatible protocol and demote DeepSeek to
  one default preset)
- **Built-in provider presets**: DeepSeek (default) / OpenAI / Ollama (local, no key) /
  LM Studio (local) / custom; the UI provider dropdown fills `base_url` / `model`
- **Local offline model support**: Ollama / LM Studio reuse the same client; offline mode =
  QEMU + RISC-V GCC + audit + sandbox + local LLM, with no network at all
- **Key-less degradation**: no key never crashes; the UI guides configuration; probe
  `localhost:11434` and offer local Ollama; a new user's first launch must not just error
- **API key persistence: OS keyring** (Windows Credential Manager / macOS Keychain /
  Linux Secret Service, via the Rust `keyring` crate); never `localStorage` / plain files /
  `.env`; "memory only" becomes the fallback
- **Pre-launch security checklist**: `.env.example` holds placeholders only; `.gitignore`
  covers `.env` / `*.db` / `*.jsonl`; CI runs secret scanning (gitleaks or GitHub native);
  the README states that no API key is provided

### Planned (v0.2) — other

- Expose a minimal serial access interface on `AgentLoop` (the host currently derives it from
  the audit log, which is brittle)
- Real QEMU snapshots with `savevm` / `loadvm` (already satisfied by plan A′; remaining work:
  the `AppState.vm` slot so the UI can save/restore)
- Host serial polling moved to sandbox push callbacks
- gdbstub integration (debugging)
- Unix sockets (macOS / Linux) and virtio devices
- Audit log sharding and remote backup
- **Bilingual (English/Chinese) docs before going public**: README / CHANGELOG /
  PROJECT_CONSTITUTION / AGENTS / release notes in both languages; English is the main
  document (GitHub default), Chinese lives in `*.zh-CN.md`; language switcher at the top;
  LICENSE is not translated

## [0.1.0] - 2026-09-14

> RiscDom v0.1.0 — AI-native RISC-V sandbox MVP

### Added

- **sandbox**: a QEMU RISC-V `virt` bare-metal sandbox. Process lifecycle, platform endpoint
  abstraction (QMP / serial → QEMU arguments), a minimal QMP client (greeting /
  `qmp_capabilities` / `stop` / `cont` / `quit`), serial capture with incremental buffering,
  snapshot/rollback (MVP fallback), and auditing of every outbound operation.
- **audit**: append-only SQLite + SHA-256 hash chain. `BEFORE UPDATE` / `BEFORE DELETE`
  triggers make rewrites impossible; no UPDATE/DELETE API and no off switch; querying /
  filtering / JSONL export; an `audit-verify` CLI (exit 0/1/2, locating the first broken
  event).
- **agent**: the agent loop and tools. DeepSeek client + `MockLlm`; the capability policy
  `WorkspacePolicy` (deny by default, traversal guard, extension allowlist); the tool set
  `write_source` / `compile` / `start_vm` / `read_serial` / `stop_vm` / `list_workspace`;
  a freestanding RISC-V compiler wrapper (injects crt0 + linker script); the system prompt;
  context trimming and an iteration cap; audit events across the whole chain.
- **host**: the Tauri backend. 10 commands (audit status/list, LLM config, run agent,
  workspace, serial, export); events `agent:iteration` / `agent:tool_call` /
  `agent:tool_result` / `agent:final` / `serial:chunk` / `vm:state`.
- **ui**: a React + TypeScript + Vite three-pane desktop UI (chat / settings / serial
  canvas), an xterm.js serial canvas and draggable splitters with no third-party splitter
  library.
- Project docs: `AGENTS.md` (the constitution), `PROJECT_CONSTITUTION.md` (full constitution
  + architecture + audit event types), `ENVIRONMENT.md` (toolchain and platform limits),
  per-crate READMEs and the root README.

### Known limitations (MVP fallbacks)

- **Snapshots are a fallback**: `save_snapshot` / `load_snapshot` store and reload launch
  parameters and reboot — they are **not** real VM memory/device state (v0.2 moves to
  `savevm`/`loadvm`).
- **Windows + TCP only**: QMP/serial go over TCP; Unix sockets and macOS/Linux are not
  implemented.
- **No streaming**: LLM responses arrive as one block.
- **No session persistence**: every `run_agent` is an isolated context.
- **The compiler injects crt0**: the AI only writes `int main(void)`; the `_start` entry and
  the stack are injected by the compiler (rationale in `agent/README.md` and
  `ENVIRONMENT.md`).
- **API key in memory only**: never on disk, never in the audit log; gone when the app closes.

### Build artifacts (Windows x64)

Produced by `npm run tauri build` (build output under `target/`, not committed):

- `ui/src-tauri/target/release/bundle/msi/RiscDom_0.1.0_x64_en-US.msi` (about 5.16 MB)
- `ui/src-tauri/target/release/bundle/nsis/RiscDom_0.1.0_x64-setup.exe` (about 3.65 MB)

### GitHub Release

The repository stays **private**. No GitHub Release has been published; installers are kept
locally only. (An earlier draft was deleted; the `v0.1.0` tag remains.)

### Verification

- The whole workspace passes `cargo test` (sandbox/audit/agent/host + doc tests).
- `npm run build` (tsc + vite build) passes.
- `cargo check --manifest-path ui/src-tauri/Cargo.toml` passes.
- Mock-LLM end-to-end: `cargo test -p host -- --ignored --nocapture` → `agent:final` arrives,
  `serial:chunk` contains `HELLO RISCV`, `verify_chain` is Intact.
- Real DeepSeek API end-to-end: **executed and passing** (2026-09-14, `iterations = 6`,
  serial captured `HELLO RISCV`; see `host/README.md`).

[Unreleased]: https://github.com/breakevery/riscdom/compare/v0.8.0...HEAD
[0.8.0]: https://github.com/breakevery/riscdom/compare/v0.7.0...v0.8.0
[0.7.0]: https://github.com/breakevery/riscdom/compare/v0.6.0-preview.1...v0.7.0
[0.5.0]: https://github.com/breakevery/riscdom/compare/v0.5.0-preview.1...v0.5.0
[0.4.0]: https://github.com/breakevery/riscdom/compare/v0.3.1...v0.4.0
[0.3.1]: https://github.com/breakevery/riscdom/compare/v0.3.0...v0.3.1
[0.3.0]: https://github.com/breakevery/riscdom/compare/v0.2.2...v0.3.0
[0.2.2]: https://github.com/breakevery/riscdom/compare/v0.2.1...v0.2.2
[0.2.1]: https://github.com/breakevery/riscdom/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/breakevery/riscdom/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/breakevery/riscdom/releases/tag/v0.1.0
