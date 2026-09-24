[中文](handoff.zh-CN.md) | English

# Handoff — carrying RiscDom into the next conversation

**This file is a cross-conversation handoff.** Section 1 is a volatile snapshot and is updated when
a release ships. Sections 2–12 are the stable constraints: they have not changed as the batches went
by, and they are the ones a new conversation must not break.

Repository `D:\codeagent\breakevery\riscdom`, remote `https://github.com/breakevery/riscdom.git`,
branch `main`. The close-out of every batch is the same: gate green → `scripts\commit.ps1 "<msg>"`
(which runs the gate itself) → push — and none of those remote-facing steps happens without the
current request authorising it (§2).

## 1. Snapshot — `v0.8.0` is the newest release (update this section when the next release ships)

- **The gate lints and checks every workspace crate on every platform now** (v0.9
  gate-consistency batch B-2). The last two skips are gone — `host-tauri` and `ui/src-tauri` are
  linted on Linux as well — and `worker`, which no clippy list on either platform mentioned,
  joined them. The OS branch in `scripts/gate.sh` went with the skips, and the `gate` job in
  `ci.yml` now installs the Tauri system libraries (webkit2gtk / gtk / librsvg / libsoup) beside
  the `libdbus-1-dev` it already had. `ui/dist` turned out **not** to be a prerequisite for
  `cargo check` / `clippy`: `tauri::generate_context!()` takes the dev branch unless the
  `custom-protocol` feature is on (`tauri-macros/src/context.rs`), which is true for a plain
  `cargo check`; verified by moving `ui/dist` aside and checking the crate. Still open: a
  non-Windows gate runs `cargo test -p audit -p sandbox --lib` — 10 tests of 638 (B-3).
- **B-1's follow-up: the lint it surfaced is fixed** (v0.9 gate-consistency batch B-1-fix).
  Un-skipping `cli` on Linux failed there immediately: `cli/tests/control.rs`'s
  `write_executor_settings` was called only from a `#[cfg(windows)]` dispatch test but carried
  no gate of its own, so on Linux it was dead code and `-D warnings` made that a build failure
  — and the `std::path::Path` import, which only it and the fake-executor helper use, was
  ungated for the same reason. Both are `#[cfg(windows)]` now. No production code changed. CI
  is green on `main` again.
- **The gate lints `cli` / `server` / `host-core` on Linux now** (v0.9 gate-consistency batch
  B-1). The non-Windows branch of `scripts/gate.sh` used to skip all four crates together; it now
  runs `cargo clippy -p cli -p server -p host-core --all-targets --no-deps -- -D warnings` on
  every platform and skips only the two Tauri crates (`host-tauri`, `ui/src-tauri`). No new
  system package was needed: `libdbus-1-dev` + `pkg-config`, which CI already installs, are what
  `host-core`'s `keyring` backend needs on Linux. The gap this closes was found the hard way —
  E4's `worker` example step was the first Linux gate step to compile `host-core`, and it went
  red for four commits. `ci.yml` is unchanged. Still open: the two Tauri crates (B-2) and the
  fact that a non-Windows gate runs `cargo test -p audit -p sandbox --lib` — 10 tests of 638
  (B-3).
- **The crate examples are documented, and the contributor templates exist** (v0.9
  small-changes batch). `agent/README` and `audit/README`, both languages, gained an `## Example`
  section for `examples/audit_demo.rs` and `examples/chain_demo.rs` — the two examples that were
  the only ones not named in their crate's README (`sandbox`'s `run_hello`, and `worker`'s
  `dispatch` and `remote_executor`, already were). `.github/` gained two issue forms
  (`ISSUE_TEMPLATE/bug_report.yml`, `ISSUE_TEMPLATE/feature_request.yml`) and a bilingual
  pull-request template (`PULL_REQUEST_TEMPLATE.md` + `.zh-CN.md`). The forms are `.yml` on
  purpose: `scripts/check-bilingual.sh` scans every `*.md`, so a Markdown template would need a
  `.zh-CN.md` sibling — and GitHub would then offer that sibling as a second template. **Decision
  §43** withdraws §20's DCO clause: the two instruments do not do the same job, the CLA (a grant
  of rights) is the stronger and is what open-core needs, and a DCO is only a statement of origin.
- **The Linux CI red is fixed in configuration** (v0.9 CI fix). The `gate` job went red on Linux
  from `00fca17` on — four consecutive commits — because its `remote executor example` step is the
  first Linux gate step that compiles `host-core`, and `host-core`'s `keyring` backend on Linux
  builds `libdbus-sys`, which needs the system `dbus-1` library via pkg-config. The `gate` job now
  installs `libdbus-1-dev`, and the Linux `bundle` job's dependency list gained it too. The local
  (Windows) gate uses `keyring`'s `windows-native` backend and never needs it, which is how four
  pushes went out under a red CI. **Process lesson**: a green local gate is not a green CI; run
  `gh run list --limit 3` after every push.
- **The documentation has an entrance** (v0.9 documentation batch). [`docs/README.md`](README.md)
  is the map: every Markdown file in the repository, grouped by the five audiences
  [decisions](decisions.md) §21 names (start here · kernel developers · distribution
  integrators · administrators · end users · contributors), each row saying what the
  document is for, its **state** (*living* / *snapshot* / *history*) and its version — with
  the history labelled as history (`architecture-evolution.md` is the v0.7 snapshot; older
  `CHANGELOG` sections and released `RELEASE_NOTES` are not rewritten). The root `README`'s
  layout tree was three years of batches out of date (it knew `host/` and four crates; the
  workspace has eight) and its test list called `host-core` "Tauri backend commands", which
  is `host-tauri`'s job since the A1 split; both languages are corrected, and the "More"
  section now opens with the map and lists all nine crate READMEs. Two stale counts in the
  normative documents (`control-plane-api` said 35 controls, the client guide said 31
  queries) are fixed. **Reported, not fixed**: four crate READMEs (`audit`, `cli`,
  `host-core`, `ui`) link to no neighbour at all — their only link is the language switcher.
  (The other two items that batch reported — the crate examples missing from their crates'
  READMEs, and §20's templates and DCO — were closed in the small-changes batch of
  2026-09-24; §20's DCO clause turned out to be the wrong call, and §43 withdraws it.)
- **The repository owns its line endings now** (v0.9 line-ending batch, mechanical). A
  `.gitattributes` (`* text=auto eol=lf`; `*.sh` named; the six tracked binaries marked
  `binary`; **no `*.ps1` exception**, because all five PowerShell scripts are LF today and
  are what every batch's gate and commit run through). The working tree held 38 CRLF or
  mixed files (`sandbox/src/relay.rs` worst, 401 of 413 lines) that git could not see: the
  **index** had been LF all along, so `git add --renormalize .` staged nothing and **the
  commit carries no line-ending change** — it is a working-tree repair, proven content-free
  by comparing all 356 tracked files with their committed blobs (byte-identical). Seven
  relative links were wrong (a root-level file referenced from `docs/` or the reverse:
  the CHANGELOG's `multi-agent-foundation`, `handoff(.zh-CN).md` → `RELEASE_NOTES`,
  `qemu-distribution(.zh-CN).md` → `THIRD_PARTY_NOTICES`, `toolchain-setup(.zh-CN).md` →
  `ENVIRONMENT`); all 365 relative links in the repository now resolve.
- **The small debts are paid** (v0.9 clean-up). Four things, no new surface. (1) **No
  hand-bumped counts in the tests**: `every_control_endpoint_answers` derives its expectation
  from the route table through a new read-only accessor (`server::routes::control_paths()`),
  so a control without a case fails naming the missing path, and the seven the test leaves
  alone are named rather than counted; `the_table_has_the_documented_endpoints` reads the
  counts off the API document's §5 headings, both languages. (2) **`__pycache__/` and `*.pyc`
  are ignored**. (3) Comments that counted endpoints ("26 query / 27 control", "the two
  path-parameter routes") are corrected or de-numbered. (4) `agent/README.zh-CN.md`'s tool
  table became the index its English sibling is. Two findings came out of the scan and are
  **reported, not fixed** (the batch's rule): **seven broken relative links** (a root file
  referenced from `docs/` or the reverse — `CHANGELOG.zh-CN.md` → `docs/multi-agent-foundation.zh-CN.md`,
  `handoff(.zh-CN).md` → `../RELEASE_NOTES(.zh-CN).md`, `qemu-distribution(.zh-CN).md` →
  `../THIRD_PARTY_NOTICES.md`, `toolchain-setup(.zh-CN).md` → `../ENVIRONMENT.md`), and
  **eleven files with mixed CRLF/LF endings** in the working tree (`sandbox/src/relay.rs` and
  neighbours) — an artefact of the editing tools versus `core.autocrlf=true`, invisible to
  git. The two defects the batch went looking for do **not** exist: no lone `\r` and no
  unpaired fence in any of the 85 Markdown files.
- **The seam has its other half: a remote executor** (v0.9 interface E4).
  `agent/src/dispatch.rs` has said since v0.8 that "a remote implementation … implements
  exactly this trait. **None is written yet**"; `worker/examples/remote_executor.rs` is it.
  `HttpExecutorHandle` carries a local label, the node's base URL, an optional bearer token
  and the **remote target**, and its `run` POSTs a task-shaped body to `POST /v0/tasks` — the
  endpoint that routes to an executor the *remote* node owns and answers the `TaskOutcome`,
  which is what makes this a real executor and not a shape demo. The identity in the answer
  is the node's, never the handle's label; an answer naming another task is a protocol break;
  `404` is `NoSuchAgent` (the far fleet is what is missing) and every other failure is
  `Failed`. **No transport crate**: `worker` depends on `host-core`, `agent` and `serde_json`
  only, and the request is written by hand over `std::net::TcpStream` — the technique
  `server/tests/smoke.rs` and `host-core/tests/common/mod.rs` already use, so the reader sees
  what crosses. **No production crate changed**: `LocalDispatcher::new(vec![Arc::new(handle)
  as Arc<dyn AgentHandle>])` is the whole integration, which is what the seam promised. The
  example runs with no setup (an in-process stand-in node on `127.0.0.1:0`) and proves itself
  with `--self-test` (seven assertions); `scripts/gate.sh` runs that step. The client guide
  gained §9 on writing a handle, and the README states what it is not: loopback HTTP, not a
  cross-device story (decision §42).
- **A supervisor you can run** (v0.9 interface E3). `examples/python/dispatch.py` is the
  smallest complete external supervisor: a process outside the kernel, with no model of its
  own, that drives a node through the control plane — `GET /v0/executors` for the fleet,
  `POST /v0/tasks` per task (synchronously, one at a time), and `GET /v0/events` under
  `--follow` for the stream while the work happens. **Standard library only** (`urllib.request`,
  `json`, `argparse`, and a hand-rolled SSE reader), because a reference implementation
  should not teach a dependency it does not need; the token comes from `--token-file` or
  `$RISCDOM_TOKEN` and never from an argument. Its exit codes follow the CLI's (`0` all
  succeeded, `1` something did not, `2` usage, `3` unreachable or refused), and its
  `--self-test` proves the whole path offline against a stdlib `http.server` fake control
  plane — which `scripts/gate.sh` now runs when a Python interpreter is on `PATH` (printed
  skip when it is not; the gate's first optional-toolchain step). Two documents came with it:
  `examples/python/README.md` and the client guide's §8, which now points at it. The Rust
  sibling stays what it was (`worker/examples/dispatch.rs`, executors as child processes,
  no HTTP, parallel), and the README tabulates the difference on purpose: they are the two
  halves of the picture (decision §41).
- **Both tool schemas are documents, and both are checked** (v0.9 interface E2). The
  interface's other half was vocabulary: what an executor's model may call, and what an AI
  supervisor may call. `docs/tool-schema-executor.md` is the eight tools `tools_json()`
  builds — verbatim, the same array `AgentLoop` puts in the request's `tools` field — and
  `docs/tool-schema-control-plane.md` writes every endpoint as an OpenAI-style function
  definition (32 queries + 36 controls + 3 host-local + 4 path-parameter routes = 75 tools),
  named by a derivation from the path (drop `/v0/`, fold separators; a path served by both
  methods gives its `POST` a `_post` suffix; the four id-taking routes get a verb). The
  distinction is the point, and both documents say so: the executor's tools run *inside* an
  executor, the supervisor's are how one *drives* a node. Hand-written schemas drift, so
  each half has one owner: `agent/tests/tool_schema_doc.rs` compares the executor document
  with `tools_json()`, `server/src/routes.rs`'s own tests compare the control plane's three
  marked route tables with `ROUTES` + `LOCAL_ROUTES` + `resolve` (so an endpoint cannot
  land without a tool), and a new `scripts/check-tool-schema.mjs` — one step in
  `scripts/gate.sh` — owns what neither can see: the translation must carry the same marked
  blocks, every table name must be the derivation of the document's §2, and every name must
  have a definition and a row. `agent/README.md`'s tool table is now an index pointing at
  the schema document instead of a second copy of the catalogue (decision §40).
- **The node can be dispatched to, and its fleet is configuration** (v0.9 interface E0). The
  batch that made the `Dispatcher` reachable over HTTP. `executors` in `settings.json` — a
  label, a program and its arguments, and **no `env`**, because a settings file is not a
  secret store — are turned into `StdioExecutorHandle`s once, after `load_settings`, and
  `AppState::dispatch_task(target, input, sandbox, id)` routes one task through the same
  `LocalDispatcher` the worker's supervisor builds. Registration **spawns nothing**: the
  handle is data until a task arrives, which is why it needs no lazy init (and why a program
  that does not exist is the first task's failure, not a startup error). `POST /v0/tasks`
  takes a `Task`'s four scalar fields, mints a `TaskId` when the caller sent none, and
  answers with the executor's `TaskOutcome` — **synchronously**, like `/v0/agent/run`: there
  is no task table and no `GET /v0/tasks/{id}`, so nothing to poll. The ladder is two things
  kept apart: a target nobody owns is the caller's `404 cause "target"`, a dispatch that
  broke is the host's `500 cause "task"`, and a run that merely *failed* is still a `200`
  whose `outcome` says `failed`. `GET /v0/executors` lists who is reachable, in configuration
  order, empty list and all. **The node is deliberately not one of its own executors** — a
  target naming it is a `404`, because running here is `POST /v0/agent/run` — so the two
  endpoints are siblings, not synonyms, which is exactly the distinction that made E0 worth
  a batch. Both routes declare `agent.run` (E0 decision 3: no capability was added — who may
  cause an agent to run may ask who can be asked), both have a Tauri command (registered, not
  wired to the interface: the D line) and two CLI subcommands (`executors list`,
  `tasks dispatch --target <agent_id> --input <text> [--sandbox <name>]`). Eight documents
  went with it, including decision §39.
- **A task declares which sandbox it runs under, and nothing about the node moves** (v0.9
  sandbox F2d, the sandbox line's last piece). `Task` gained `sandbox: Option<String>`
  (`#[serde(default)]`, so an older supervisor's line is still readable) with a
  `Task::with_sandbox` builder, and the three surfaces that can carry a declaration do:
  `POST /v0/agent/run` (body field `sandbox`), the Tauri `run_agent` command (new
  optional parameter) and the worker (`Task.sandbox`, already on the line it reads — the
  protocol change was exactly one optional field, because `Task` has been serialisable
  since v0.8). Where it lands: `AppState::run_agent_for(emitter, input, sandbox)` —
  `run_agent` keeps its old signature and now delegates with `None`, so the direct call
  sites did not move. The order is **task > `current_sandbox` > configured default >
  built-in fallback**, an unknown name is a `404` (`cause: "name"`) rather than a silent
  fallback, and the resolved definition is injected as the compiler, the QEMU path and
  the guest's memory (a new `agent.set_memory_mb`, since `VM_MEMORY_MB` was a hard-coded
  128). **It does not reach the kernel**: `start_vm`'s `elf_path` still chooses the ELF,
  and `def.kernel` stays what a *switch* boots (F2d decision 2). Two refusals guard the
  declaration: the unknown name, and a name that is not what the **running** VM came
  from — `409` `cause: "sandbox"`, because a task declares and only a switch changes,
  and the message names both ways out (stop it, or `POST /v0/sandboxes/switch`). That
  check needed a fact the host did not have: `current_sandbox` is written only by a
  successful switch, so a VM started by the `start_vm` *tool* had no recorded
  provenance. `active_sandbox` now records it — written when a run's VM appears, by
  `switch_sandbox`, and (as the node's own sandbox) on a snapshot restore, cleared by
  `stop_current_vm`. As a consequence the run endpoint's whole refusal ladder (the
  declaration, then readiness) moved into `run_agent_for`: a bad *parameter* is answered
  before the environment, so `POST /v0/agent/run` with an unknown sandbox is the caller's
  `404` even with no model configured, and the readiness refusal became a typed
  `HostError::NotConfigured` so the route could stop checking readiness twice. Seven
  documents went with it, including decision §38.
- **A project travels as one file** (v0.9 project in/out). `POST /v0/workspace/export`
  answers the workspace as a `tar.gz` — **bytes, not JSON**, the first such body on that
  surface apart from the event stream — and `POST /v0/workspace/import` takes an archive
  as its body (zip, tar.gz or tar, chosen by `Content-Type` and falling back to the
  bytes). The host's own `.riscdom/` state is never packed and never unpacked; entries
  that escape the workspace, arrive as links, or are unreadable are `400`
  `cause: "archive"`; an existing file is `409` `cause: "exists"` unless `?force=true`;
  and the import has its own **64 MiB** ceiling (`413`) instead of raising the shared
  64 KiB one every JSON body uses. Capability-wise the pair splits: import needs the new
  **`workspace.write`** (32nd), export `workspace.read`. The AI's writes became visible
  too: `write_source` now records **`agent.file.write`** `{path, bytes}` on the audit
  chain (not an SSE event — the event count stays 14), so "which files did the model
  write" is a row rather than a re-parse of a truncated tool argument. Both packers
  (`zip`, `flate2`+`tar`) were already in the lock file and are now declared for every
  platform: a Windows host reads a `.tar.gz`, a unix host reads a `.zip`. Two Tauri
  commands (registered, not wired — the D line) and two CLI subcommands
  (`workspace import <archive> [--force]`, `workspace export [--out <file>]`; the export
  writes the archive to `--out` or stdout, and the count to stderr). Eight documents
  went with it, including decision §37.
- **An agent may ask for a sandbox change, and somebody else decides it** (v0.9 sandbox
  F2c). The control plane grew a request queue: `POST /v0/sandboxes/requests` (declared
  `agent.run` — the actor that may run an agent is the actor that may say what it wants)
  leaves an ask and answers `201` with its id; `GET /v0/sandboxes/requests?status=`
  (`sandbox.read`) reads it back, newest first. `approve` / `reject` change the record and
  **nothing else**: the switch stays a second, authorised call to `POST /v0/sandboxes/switch`
  (F2c decision 4). A decision needs the capability the request's own `action` implies —
  `sandbox.switch` for `switch`, the new **`sandbox.assemble`** (31st) for `define` /
  `assemble` — which the static route table cannot express, so those two routes declare
  `sandbox.read` as their gate and the handler checks the precise one against the actor
  `dispatch` now receives (F2c decision 1); an unknown id is `404`, a second decision `409`.
  There is **no TTL** (F2c decision 2): `expired` is reserved and nothing produces it. The
  queue is a queue, not a slot, and its ids are `req-<pid>-<seq>` — their own namespace, not
  the task one. The AI got the two tools that make the surface reachable:
  `request_sandbox` and `sandbox_status`, wired through a new `agent::SandboxRequester`
  trait the host implements over cloned sub-handles (`agent` cannot name `AppState`, and the
  loop lives inside it — F2c decision 4), plus four Tauri commands (registered, not wired
  to the interface: that is the D line) and three CLI subcommands (`sandboxes requests`,
  `requests approve|reject`; the two decisions ask first, like `sandboxes switch`).
  `sandbox:request` is the 14th SSE event — `{id, status, requester, action}`, one frame per
  change — and the `all_events_are_named` guard lists all fourteen again. Eight documents
  went with it: the API tables (`§5.1` 32, `§5.2` 33, and the vocabulary's "every capability
  has a route" became "is enforced somewhere", because `sandbox.assemble` is enforced in
  that handler until its own endpoint lands), the events table, the client guide, both
  READMEs, this section, the changelog, and decision §36.
- **The relay's port-lease contract is written down, and its flaky test now asserts it**
  (v0.9 relay-fix 2/N). `concurrent_leases_never_repeat_a_port` had failed twice, both times
  with the `sandbox` crate untouched. The reconnaissance found the allocator right — the
  check and the record are one lock scope (`relay::reserve`), and `PortLease` has exactly one
  construction site — and the assertion too strong: it recorded every port *ever* leased in
  the run, so a number a finished thread had released and the OS had handed out again looked
  like two holders at once. The test now parks every thread's leases until all eight have
  leased and compares then, which is the invariant the code keeps; a new
  `a_released_port_is_free_to_come_back` pins the other half (a dropped lease leaves its port
  bindable for anyone). Library hygiene, no behaviour change: `HELD_PORTS` is a `HashSet`
  behind a `LazyLock` (`HashSet::new` cannot initialise a `static`; `relay::reserve` is now
  one `insert`), and `Drop` removes **its own** number instead of every equal entry. No API
  and no call site changed (`agent/src/tools.rs`, `host-core/src/state.rs`,
  `sandbox/src/vm.rs` are untouched), `sandbox/README.md` states the contract in both
  languages, and `port_race.rs` (ignored, a real-QEMU stress test) still walks the
  inter-process window the callers retry around. The stronger "never reused while the process
  lives" invariant was considered and rejected (ledger §35).
- **The switch has a surface: an event, an endpoint, a capability and a CLI command**
  (v0.9 sandbox F2b-2). `sandbox:switch` is the thirteenth SSE event — `{from, to, ok,
  reason}`, emitted once on every exit, success or failure — and `POST /v0/sandboxes/switch`
  (`capability sandbox.switch`, the 30th) answers a status per reason: `200` with
  `{from, to}`; `404` with `cause: "name"` (no such definition); `409` with `cause: "run"`
  (a run is in flight) or `cause: "sandbox"` (a switch is in progress); `503` with the
  reason code as `cause` (`sandbox_qemu_missing` / `sandbox_toolchain_missing` /
  `sandbox_kernel_missing`); and `500` with `cause: "sandbox_start_failed"` — the "stopped,
  not half-switched" case, which became its own `HostError` (`SandboxStart`) so the answer
  could carry a code. The route answers with the two ends rather than an empty `204`, so a
  client that only reads the answer still learns what changed. `switch_sandbox` now takes
  the `EventSink` its caller owns (the route injects an `HttpEventSink`, the Tauri command a
  `TauriEventSink`; the interface is still not wired — that is the D line), and
  `sandbox_switch_in_progress()` is the probe the `409` is decided from. The CLI has
  `sandbox switch <name>` — `sandboxes switch <name>`, in the destructive family: it asks,
  `--yes` answers up front, a non-terminal stdin is refused (`2`), and the answer prints
  `switched from <old> to <new>`. **The `events.rs` guard is whole again**:
  `all_events_are_named` lists all thirteen names and asserts they are unique, so F1's
  `qemu:download` is no longer missing from it, and `docs/control-plane-events.md` §3 carries
  thirteen rows. Still open, reported since F2b-1: the *success* path has no hermetic test
  (a real QEMU and a kernel ELF — the golden path's `--ignored` ticket), and the endpoint's
  `409 cause: "run"` branch cannot be walked in a test either (a run in flight needs an LLM
  and a toolchain), so it is guarded at the state level instead.
- **A node can be switched to another sandbox, validate-before-stop** (v0.9 sandbox
  F2b-1). `AppState::current_sandbox()` became **runtime state** (F2b decision 1, ledger
  §34): `None` until a switch succeeds, never written to `settings.json`, while
  `sandbox_default_name()` keeps answering the stored `default_sandbox` — a switch changes
  what is *running*, not what is *configured*. `AppState::switch_sandbox(name)` resolves the
  definition (the same merge the list serves, now one implementation,
  `merged_sandbox_defs`), validates it — `sandbox_check` answers four new `HostError`s,
  `SandboxNotFound` / `SandboxQemuMissing` / `SandboxToolchainMissing` /
  `SandboxKernelMissing`, and `sandbox_runnable` is its boolean face, behaviour unchanged —
  resolves the kernel (`def.kernel`, else the workspace's newest ELF), and **only then** stops
  the current VM, starts a fresh one from the definition (three attempts on fresh ports, the
  shape `tool_start_vm` uses) and records the name as current. A switch is refused while
  another is in progress (`begin_sandbox_switch` / `cancel_sandbox_switch` /
  `finish_sandbox_switch`, the download slots' shape, `already in progress`) and while a run
  is in flight (`run_in_flight()`, read from the run bookkeeping `begin_run` sets and
  `finish_run` clears — the loop shares this VM slot and takes it per tool call, so a switch
  under a running agent would hand it a different guest). A failed switch leaves the node
  **stopped**, not half-switched (`Drop` kills whatever the failed handle spawned); a
  definition that fails validation leaves the running VM alone. No event, no endpoint, no
  Tauri command, no CLI and no `sandbox:switch` audit row yet — that is F2b-2, together with
  the capability (29 → 30). Reported and now closed in F2b-2: the `events.rs` guard listed
  eleven names while the document counted twelve (`qemu:download` was missing from it).
  Still open: the success path has no hermetic test (it needs a real QEMU and a kernel ELF —
  the golden path's `--ignored` ticket).
- **A version-less scanned resource is named for the resource, not for a missing version**
  (v0.9 sandbox F2a-3). F2a-1's `format!("{kind}-{version}")` turned the machine's own QEMU
  — which the scan records no version for — into a definition called `qemu--`, and F2a-2
  served that name over the four endpoints and the CLI. It is `qemu-system-riscv64` now, on
  every platform: the stem `sandbox::qemu_discover` searches for, with a Windows build's
  `.exe` trimmed, so the file keeps no second copy of the name. A resource the scan **does**
  know a version for keeps `<kind>-<version>` (`toolchain-15.2.0-1`, `qemu-11.1.0`), and the
  `"-"` sentinel became one exported constant, `NO_VERSION`, instead of two literals that
  had to agree. Nothing else about the scan or the merge changed — this is a naming fix.
- **The sandbox registry has a control-plane surface: four read-only routes, four Tauri
  commands, four CLI subcommands** (v0.9 sandbox F2a-2). `GET /v0/sandboxes` answers the
  merged registry plus `current` and `default`; `/v0/sandboxes/current` the two names;
  `/v0/sandboxes/candidates` the **raw scan** (not the registry, and nothing written back);
  `/v0/sandboxes/{name}` one definition, or `404` naming the parameter when no definition
  carries that name. All four declare the 29th capability, `sandbox.read`. The name route is
  the second path-parameter route after `/v0/runs/{run_id}`, and the literal sub-paths
  (`current`, `candidates`, and the three the rest of the F2 line reserves — `requests`,
  `switch`, `assemble`) are never read as a name: they answer `404` until F2b/F2c serve
  them. Four Tauri commands (`list_sandboxes`, `current_sandbox`, `sandbox_candidates`,
  `get_sandbox`) are registered in the desktop shell and **not wired to the interface** —
  that is the D line. Four CLI subcommands (`sandboxes list` / `current` / `candidates` /
  `show <name>`) print a table in human mode and pass the JSON through unchanged with
  `--json`. **The gap F2a-1 reported is closed**: §5.1 is 31 queries, the vocabulary is 29
  names, and every name now has at least one route. (Switching landed above, in F2b; approval is
  F2c, `Task.sandbox` is F2d.)
- **The sandbox registry exists: definitions, the scan, and the merge** (v0.9 sandbox
  F2a-1). A *sandbox* is now a nameable thing. `host-core/src/sandbox_def.rs` holds
  `SandboxDef` — the **stored** fields `name` / `display_name` / `memory_mb` / `qemu_exe` /
  `toolchain_path` / `kernel` / `notes`, every one but the name optional — and the two shapes
  the API **serves**: `SandboxView` (the definition plus the three things only the host can
  answer at the moment of the question, `source`, `runnable` and `shadowed`) and
  `CandidateView` / `CandidatesView` (one installed resource; the two independent lists).
  `LocalSettings` gained `sandboxes` and `default_sandbox`, both `#[serde(default)]`, so no
  migration runs and a file written before the fields existed still loads. `AppState` answers
  `sandboxes()` (the merged registry), `sandbox(name)`, `current_sandbox()`,
  `sandbox_candidates()` and `sandbox_default_name()`. The merge is hand-written first, then
  the scan, then the built-in `default`; a hand-written definition wins a name collision and
  the scanned entry **stays in the list, marked `shadowed`**, so the merge is visible instead
  of silent. The scan is bounded to `<data-dir>/toolchain/*` and `<data-dir>/qemu/*` —
  through the downloaders' own `find_compiler` / `find_qemu`, now `pub(crate)`, rather than a
  second copy of the scan — plus the machine's own QEMU, and it is **never written back to
  `settings.json`**. `runnable` is computed, never stored: a QEMU that exists and answers
  `--version`, a toolchain that exists, and a kernel that exists or can be compiled, so
  uninstalling a resource leaves a valid definition that simply cannot run. `Capability`
  gains its 29th name, `sandbox.read`; the four endpoints, the Tauri commands and the CLI
  are F2a-2. Reported, not fixed: the API document's §5 tables still name the capabilities
  of the 28 routes that exist, because `sandbox.read` gets its endpoints in F2a-2 — the
  count moved one ahead of the table on purpose.
- **QEMU is wired like the toolchain, and the refusal is the recorded decision** (v0.9
  sandbox F1). The F reconnaissance found one asymmetry: `toolchain_download` ran end to end
  (host methods, endpoints, events, CLI) while `qemu_download` was **code without a caller**.
  The caller exists now, in the toolchain's exact shape: `AppState::begin_qemu_download` /
  `qemu_download_status` / `cancel_qemu_download` / `download_qemu_now` (plus
  `record_qemu_download_event` / `finish_qemu_download` and `qemu_dir()`), the audit events
  `host.qemu.download.start|done|failed|cancelled`, the SSE family `qemu:download`, the three
  Tauri commands, the three endpoints (`GET`/`POST /v0/qemu/download`, `POST
  /v0/qemu/download/cancel`, capability `qemu.read` / `qemu.configure`) and the three CLI
  commands (`qemu download [--wait]` / `cancel` / `status`). **What is not wired is a
  download**, and that is by decision: `spec_for_current_platform()` refuses on every
  platform (`docs/qemu-distribution.md` §5 — RiscDom guides the user to a QEMU they install
  themselves, and pins no release, because upstream publishes no Windows binary and a guessed
  digest is a silent integrity hole), so `POST /v0/qemu/download` answers `503 unavailable`
  with `cause: "qemu"` and the guidance, and claims no slot. Everything behind that refusal
  is exercised by tests against a loopback fixture, so **pinning a release later is a data
  change**: the answer becomes `202` and the rest already works. Two shapes became one:
  `QemuDownloadEvent`'s payload tag is now `state` (it was `kind`), the same vocabulary
  `toolchain:download` uses, and the CLI's `--wait` terminal predicate is now one function
  (`download_terminal`) shared by both families. `docs/control-plane-api.md` §5.1/§5.2 are
  27 queries and 29 controls (was 26/27), `docs/control-plane-events.md` has twelve events
  (was eleven, §3 table row 12), and `docs/decisions.md` §29 records the shared assembly
  shape. Two asymmetries left standing, reported not fixed: `host-tauri` still writes an
  unconditional `eprintln!` on a toolchain-download failure (F1's QEMU command deliberately
  does not — C6's log switch covers `server` only), and `paths.rs` gained `qemu_dir_in` but no
  process-wide `qemu_dir()`, because the toolchain's process-wide counterpart has no callers
  either.
- **The interface tells the truth, and the library's log is opt-in** (v0.9 CLI batch 6/N).
  Four small things C3/C4 left behind, fixed together. The two audit exports answered a field
  named `bytes_written` while returning the number of **events** (`write_events_jsonl`
  returns `events.len()`): they answer `events_exported` now, and `/v0/serial/export`, which
  really does write bytes, keeps `bytes_written` — so the batch 5/N note above reads as
  history. A path the workspace policy refuses was a `403 forbidden`, which reads as an
  authorisation decision when it is really the caller's parameter: it is
  `400 bad_request` with `cause: "path"` now, and `403` is left to authentication and
  authorisation (the capability check in `http.rs` and the `Authn` hook — the tests that pin
  those are unchanged). The library's runtime lines became optional:
  `http.rs`'s `connection … ended` and `accept failed` and `routes.rs`'s
  `toolchain download failed` / `preflight failed` go through
  `ServerConfig::with_log_level` — `--log-level <off|error|info>`, **off by default** — which
  is what keeps an embedded server out of the CLI's stderr: the rough edge batch 4/N
  reported, now closed. `main.rs`'s start-up banner, usage text and fatal errors stay
  unconditional: they are the binary's own console output, and an embedded server never runs
  that `main`. The two stale `../host/src/run_diff.rs` links in this section are fixed too.
- **The CLI line is done: five batches, and every control endpoint is a command**
  (v0.9 CLI batch 5/N). The last seventeen endpoints of `docs/control-plane-api.md` §5.2
  landed as commands — three exports (`export audit-jsonl`, `export run-audit <run_id>`,
  `export serial-log`) and fourteen configuration commands grouped the way batch 4 settled
  (`llm set|clear|load-key`, `qemu path|clear`, `toolchain download|cancel|path|clear`,
  `preflight run|ack`, `audit alert set <on|off>`, `theme set`, `language set`). Together
  with batch 4's twelve, all 27 controls are now reachable from the shell, plus the two
  beyond the table (`vm start` → the reserved `501`, `runs abandon-stale`). The query half
  is unchanged: the six read-only commands of batch 2/N.
  Two things came with them. **`--api-key` / `--api-key-file` / `--remember`**: the model
  key can be given inline (warned, exactly like `--token`) or read from a file, and only
  `--remember` puts it in the OS credential store. **`--wait`**: `toolchain download --wait`
  and `preflight run --wait` subscribe to `/v0/events` *before* they post, print the frames
  of that work's family (`toolchain:download` / `preflight:progress`) and exit with the
  **work's** verdict — `3` when the download failed or a preflight step did.
  Three facts recorded on the way, each of which shaped the code: **`--out` is the server's
  path**, resolved against the workspace root (a path that escapes it is the policy's `403`),
  and the CLI never receives the file; **the audit exports' `bytes_written` counts events,
  not bytes** (the serial export's is a byte count), so the human line says which; and
  **`preflight:progress` has no end-of-run event**, so `--wait` ends at fail-fast's first
  `failed`, or at the last step's `ok` read from the host's own `preflight::STEPS`. Known
  rough edge, reported and not fixed: an embedded `--follow`/`--wait` that exits with the
  stream still open can leave the server's `connection … ended` line on the CLI's stderr.
- **The CLI can drive the control plane now** (v0.9 CLI batch 4/N). Twelve control
  subcommands joined the eight read-only ones: `run <task>` (the outcome of one agent turn),
  `vm stop`, `vm start` (the reserved `501`), `snapshots save|resume|delete`,
  `sessions create|open|rename|delete|clear-all` and `runs abandon-stale` — all HTTP `POST`
  against the control plane, none of them reaching into `AppState`. Two things came with them.
  **Confirmation**: the five commands that destroy state (`vm stop`, `snapshots resume`,
  `snapshots delete`, `sessions delete`, `sessions clear-all`) ask before they act — `--yes`
  answers up front, a terminal is prompted, and stdin that is not a terminal is a refusal
  (exit `2`), because silence is not consent. **`--follow`**: `run --follow` subscribes to
  `/v0/events` *before* starting the run, prints each frame as it arrives (event name plus a
  short payload; the envelope verbatim under `--json`) and then the outcome, so a run's stream
  is never read after the fact. `cli/src/sse.rs` is new and reads the frames;
  `client::confirm` owns the prompt; `cli/tests/control.rs` drives the real binary against a
  control plane with no model configured, so it stays off the network and off QEMU. Known
  rough edge, reported and not fixed: an embedded `--follow` that exits with the stream still
  open can leave the server's `connection … ended` line on the CLI's stderr, ahead of the CLI's
  own error object.
- **`server` is inside the gate's clippy step, and clippy-clean** (v0.9 CLI batch 3/N).
  The crate had never been linted — the gate selected the portable crates plus
  `host-core`/`host-tauri` only — and adding `-p cli` to that step is what exposed it. Six
  `clippy::result_large_err` sites (`http.rs:379`, `routes.rs:489/495/503/513/522`) are fixed the
  way the lint asks: the error type is `Box<Response<RespBody>>`, and every caller returns
  `*response` — the same response value, only boxed. Three `bool_assert_comparison` assertions in
  `routes.rs`'s tests and one `filter_next` in `tests/smoke.rs` came with it.
  `cargo clippy -p server --all-targets --no-deps -- -D warnings` is now silent, and the step lints
  `-p cli -p server -p host-core -p host-tauri` with `--no-deps`. No behaviour changed: the same
  `400` objects, built the same way, travel the same path.
- **The CLI is a control-plane client now** (v0.9 CLI batch 2/N). A new `cli` crate (bin
  `riscdom`) speaks HTTP to the control plane and nothing else: with `--remote host:port` it talks
  to a running `riscdom-server`, and without it it starts the control plane **inside its own
  process** on `127.0.0.1:0` (a port the OS picks) — one code path for both, so `riscdom` never
  calls `AppState` directly. Eight read-only commands (`health`, `status`, `agents`,
  `runs list|get`, `audit status|events`, `snapshots list`), `--json` passing the control
  plane's JSON through unchanged, tables in human mode, and exit codes `0`/1/2/3/4 (success /
  local / usage-or-400 / refused-or-5xx / 401-403). The token comes from `<data-dir>/token`
  locally (generated on first use by the same code `riscdom-server` runs) or from
  `--token-file` > `RISCDOM_TOKEN` > `--token` remotely; it is never printed. No new crate
  entered the lock file, and the gate's clippy step now covers `-p cli` too.
- **The stale references the rename left behind are gone** (v0.9 A1 wave 5). Every *live*
  mention of the pre-split crate now points at the crate that owns the thing: `cargo test -p host`
  → `-p host-core` (the tests live there), `host/tests/…` → `host-core/tests/…`,
  `host/src/state.rs` and its neighbours → `host-core/src/…`, `host/src/commands.rs` →
  `host-tauri/src/commands.rs`, `host/README.md` → `host-tauri/README.md`, `host::` prefixes →
  `host_core::` or `host_tauri::` as the item demanded. The crate lists in CONTRIBUTING, SECURITY,
  PROJECT_CONSTITUTION and ci.yml's comment name both crates now, and `README.md`'s crate index
  lists `host-core` and `host-tauri`. History was left alone: the CHANGELOG, RELEASE_NOTES, the
  decisions ledger, §1 of this file and the architecture-evolution snapshot still say `host` for
  the waves that happened. The non-Windows clippy gap is deliberately **not** closed in this wave
  (it was **partly** closed later, in the v0.9 gate-consistency batch B-1: `cli`, `server` and
  `host-core` lint on Linux too; the two Tauri crates wait for B-2).
- **The host split is complete: `host-core` + `host-tauri`** (v0.9 A1, wave 4 of 4).
  `host` is renamed `host-tauri` (directory, `[package] name`, workspace member) and the
  desktop shell depends on it: all 57 `host::` paths in `ui/src-tauri/src/lib.rs` became
  `host_tauri::`, every one of them resolving through the facade, so the shell needs one crate
  and no direct `host-core` edge. The dead `tokio` dependency is gone from the manifest
  (nothing in the crate or its tests ever used it). `cargo tree`: `-p host-core` 0 Tauri lines,
  `-p host-tauri` 15, `-p worker` and `-p server` still 0. `worker` gained the README pair it
  never had; `host-tauri/README.md` describes the two-crate boundary; the ledger has the entry
  (`docs/decisions.md` §27).
- **`worker` and `server` no longer link Tauri** (v0.9 A1 wave 3 of 4). Both moved off
  `host` to `host-core`: 30 `host::` paths became `host_core::` (worker 7 in 4 files,
  server 23 in 7 files — every occurrence, not only the `use` lines) and each `Cargo.toml`
  names the portable half. `cargo tree -p worker` and `cargo tree -p server` now name **no**
  Tauri crate; each listed 15 `tauri` lines before. No logic changed — import paths and one
  dependency line. Both crates' end-to-end tests pass unchanged (the worker protocol and the
  HTTP + SSE control plane), and the worktree suite is still 418 tests. One consumer is left:
  wave 4 renames `host` to `host-tauri` and moves the desktop shell.
- **The host's tests moved with it, and the two guards the split had weakened are whole
  again** (v0.9 A1 wave 2 of 4). All 39 integration test files moved from `host/tests` to
  `host-core/tests` (`git mv`, history kept) and their 133 `host::` paths became
  `host_core::` — 67 `use host::` lines, 65 inline paths and one doc link. `host` now has
  no tests and no `[dev-dependencies]`; the tests use `host-core`'s own dependencies
  (`agent`, `audit`, `sandbox`, `serde_json`, `sha2`, `rusqlite`, `zip` / `flate2` +
  `tar`), so nothing is declared twice. Two guards had gone quiet in wave 1 and are fixed
  here: `scripts/check-mirrored-constants.mjs` now scans `host-core/src` **and** `host/src`
  (17 files, up from 3) and `scripts/gate.sh` lints `-p host-core -p host` in one clippy
  step (host-core had been escaping clippy entirely). The consumers are still untouched —
  that is waves 3 and 4.
- **The host is split: `host-core` + a facade** (v0.9 A1 wave 1 of 4). The portable half
  of the kernel facade — the audit wiring, the VM slot, snapshots, sessions, the download
  paths, the preflight, the event envelope and the `EventSink` trait — is now the
  `host-core` crate, and **no Tauri crate appears in its dependency tree**
  (`cargo tree -p host-core` names none; `-p host` still names 15). `host` keeps the 53
  Tauri commands, the `TauriEventSink` transport, and the Tauri dependency, and re-exports
  the portable surface (`pub use host_core::*;` plus `host::events::*`), so **no consumer
  and no test changed in this wave**: `worker` / `server` / `ui/src-tauri` still compile
  against `host` and still link Tauri. The three later waves move the tests to
  `host-core/tests`, then `worker` + `server` to `host-core` (which drops Tauri from
  both), then rename `host` to `host-tauri` and move the desktop shell. See
  [host-core/README.md](../host-core/README.md).
- **Capabilities are enforced, and the transports agree on identity** (v0.9 batch 5/N).
  `Capability` is now a typed column of the route table (`server/src/routes.rs`): a route
  cannot be written without naming one, so no path skips the check. After the `Authn` hook
  returns its `Actor`, the request path asks whether it `allows` that capability and
  answers `403 forbidden` with `cause: "capability"` when it does not
  (`server/src/http.rs`) — default deny, 28 names, and `Actor` carries the set. v0.9 has
  two actor shapes and both hold everything (the token holder as `operator`, and the
  `--no-auth` hook), so a `403` only comes from a hook that returns a narrower actor.
  Identity review: every sink stamps the identity of its **source** — `AppState::agent_id()`
  — and `Server::sink()` no longer accepts one, so two transports in one process cannot
  disagree; a new test publishes one event through two sinks and asserts the `agent_id`
  matches over the wire. Docs: the API document's §3 is now "Authentication and
  capabilities", the client guide gained a capability section and a secure-deployment
  example (an nginx front end, `proxy_buffering off` for the stream), and
  [server/README.md](../server/README.md) says what the token does and does not cover.
- **The controls, the token and the event replay are in** (v0.9 batch 4). All 27
  `POST` endpoints of `docs/control-plane-api.md` §5.2 answer, plus the reserved
  `POST /v0/vm/start` (501) and `POST /v0/runs/abandon-stale` (G4, implemented); the server
  now installs `TokenAuth` by default — a generated 32-byte token in `<data-dir>/token`,
  owner-readable only, compared in constant time, `--no-auth` to opt out — and the event
  stream carries a server-wide frame ordinal, replays from a bounded 1024-frame buffer on
  `Last-Event-ID`, and sends a `gap` frame when the cursor is older than the buffer. Every
  route declares its capability and hands it to the hook (enforcement landed in batch 5/N,
  above).
- **The audit store's open path is concurrency-safe now** (fix after v0.9 batch 3).
  Opening one fresh `audit.db` from two processes at once used to fail one of them:
  `PRAGMA journal_mode = WAL` needs exclusive access and answers `SQLITE_BUSY` without
  consulting the busy timeout, so the 5 s timeout that covers writes never covered the switch
  — which is why the gate flaked once on `audit::concurrency`. `AuditStore::open` now retries
  the whole sequence (a fresh connection per attempt, `OPEN_MAX_ATTEMPTS` /
  `OPEN_BACKOFF_BASE`, defined from the append path's constants) on lock errors only. The
  write path's `BEGIN IMMEDIATE` + retry, the hash formula, the chain rows and the triggers
  are untouched. Pinned by two new race tests (8 threads × 25 rounds, on a new file and on one
  already in WAL).
- **The query API and the one event envelope are in** (v0.9 batch 3/N). All 26 query
  endpoints of `docs/control-plane-api.md` §5.1 answer over HTTP — plus the host-local
  `/v0/health`, `/v0/status`, `/v0/events`, the reserved `/v0/resources` (501), and a
  new `405 method_not_allowed` — and every transport now wraps what it sends in the one
  envelope, built in `host/src/events.rs`. The three payload shapes the events document
  calls changed have landed (`vm:state` always carries `name`, `audit:failed` uses
  `message`, `toolchain:download` is tagged `state`); the other eight are untouched.
  The webview unwraps them at its single boundary (`ui/src/api/tauri.ts`); the worker's
  line protocol and the SSE stream carry the envelope as-is. A client walkthrough is in
  [docs/control-plane-client-guide.md](control-plane-client-guide.md). All three gaps this
  bullet lists — the controls, `gap`/`Last-Event-ID` and capability enforcement — were
  closed by batches 4 and 5/N.
- **The control plane has a skeleton** (v0.9 batch 2/N). A new `server` crate —
  binary `riscdom-server` — binds the documented surface: `GET /v0/health`,
  `GET /v0/status`, `GET /v0/events` (SSE, the `hello` and `event` frames), the B1
  error model, and the `Authn` hook (`NoAuth` then; `TokenAuth` became the default in
  batch 4). It is Layer 3
  over `host` and names no Tauri type (`tauri` is still linked — the known cost). It
  changes no `host` source: `HttpEventSink` goes in as `run_agent`'s `emitter`
  argument. `gap` frames and `Last-Event-ID` replay are **not implemented yet** (next
  batch); the design's place for them is marked in `docs/control-plane-events.md`. See
  [server/README.md](../server/README.md).
- **The control-plane protocol is designed** (v0.9 batch 1/N — design only, no code). Two
  bilingual pairs fix the interface the management side codes against: the HTTP command/query
  surface in [control-plane-api.md](control-plane-api.md) — 53 commands become 53 endpoints
  (26 `GET` / 27 `POST`), and the four kernel-capability gaps are resolved explicitly — and the
  push side in [control-plane-events.md](control-plane-events.md): SSE framing plus one envelope
  all eleven events share (`version` / `kind` / `event` / `agent_id` / `task_id` / `ts` /
  `payload`). Three of the eleven payloads change shape (`vm:state`, `audit:failed`,
  `toolchain:download`); the other eight are the identity mapping. **The transport is HTTP + SSE,
  not WebSocket**: the push is one-way (server to client), SSE is plain HTTP (no upgrade
  handshake, no extra crate), and it reconnects with `Last-Event-ID` on its own. The design is
  settled; no implementation is written yet.
- **After the v0.8.0 release: the preflight directory is per agent as well** (follow-up A2 — the last
  write path a shared workspace still had). New artifacts go to
  `<workspace>/.riscdom/preflight/<agent_id>/`; the guest to boot is resolved from this agent's own
  directory first, then the shared root, so a pre-A2 guest stays usable instead of being orphaned. The
  cached result in `settings.json` was already per instance. The v0.8.0 notes' "the preflight directory
  is still shared" edge is closed by this; the only edge left there is `tauri` not being optional.
- **After the v0.8.0 release: a dispatched outcome names the executor that actually ran the task** (main
  deliverable 3/3 — the first of the three "known limitations" the v0.8.0 release notes named).
  `AgentHandle::run` returns a `TaskOutcome` and fills in the identity it alone knows: a local loop's own id,
  the host instance's id, or the child process's announced id (read from its `worker:ready` event, with a
  missing announcement reported rather than guessed). `LocalDispatcher` passes it through instead of stamping
  `Task.target`, so `TaskOutcome.agent_id` now answers "who ran this" rather than "who it was sent to".
  One v0.8.0 edge remains open: `tauri` is still not optional (the shared-preflight one was closed by
  follow-up A2). `RELEASE_NOTES.md` is the released text and is **not** edited: its "Known limitations"
  list still carries the edges this line and the one above supersede.
- **`v0.8.0` is released** (2026-09-22): the version is bumped to `0.8.0` (7 files: `Cargo.toml`, the two
  `Cargo.lock`s, `ui/package.json`, `ui/package-lock.json`, `ui/src-tauri/Cargo.toml`,
  `ui/src-tauri/tauri.conf.json` — the wix guard requires no `bundle.windows.wix.version` on a numeric
  release, and there is none), `CHANGELOG`'s `[Unreleased]` is folded into `[0.8.0] - 2026-09-22`, and
  [RELEASE_NOTES.md](../RELEASE_NOTES.md) is rewritten as the release text — **the GitHub release body is
  that file verbatim** (the v0.7.0 release worked that way). Assets: `RiscDom_0.8.0_x64_en-US.msi` and
  `RiscDom_0.8.0_x64-setup.exe` built on this machine, plus the macOS/Linux bundles downloaded from the
  CI `bundle` job.
  What v0.8 is: a fully bilingual interface, and the multi-agent foundation (multi-process audit writes,
  an identity on every event, per-agent snapshots, the dispatch abstraction, the two-process prototype).
- **`v0.7.0` is released** (2026-09-21, the release before this one). The version is bumped to `0.7.0` (7 files / 15
  places — the same sites as v0.6.0-preview.1), the preview-only `bundle.windows.wix.version` override
  is **deleted** again (the package version is numeric, which the wix guard requires), `CHANGELOG` and
  `RELEASE_NOTES` are rewritten for the release, and the Windows installers are built:
  `RiscDom_0.7.0_x64_en-US.msi` and `RiscDom_0.7.0_x64-setup.exe`. The release was cut the next day: an
  annotated tag `v0.7.0` (→ `2bddae6b0897bb5fe262af2b7e4bf4b3733ec7eb`), a GitHub release with the
  verbatim `RELEASE_NOTES.md` body, and six assets (the two Windows installers plus the macOS/Linux
  bundles from a fresh `bundle` dispatch, which is what produced the `0.7.0`-named packages). Three pieces landed in v0.7: the **self-built i18n facility** (batches 1–2 —
  `ui/src/i18n/`, `scripts/check-ui-strings.mjs` in the gate, and a language switch in *Settings →
  Appearance* that persists to `settings.json` and moves `lang` on `<html>`), **macOS/Linux builds**
  (batch A's platform work plus batch B's `bundle` CI job — the next bullet), and the fix that made
  `host` compile off Windows (batch 8). **The i18n rollout over the remaining ~190 interface strings
  is deliberately not done**: the facility and the four diff strings stay, the translation does not.
- **macOS and Linux: built by CI, and not yet walked.** `ci.yml` has a `bundle` job (dispatch or a
  `v*` tag; macOS and Linux runners) that runs `npm run tauri build` and uploads the packages as
  artifacts — macOS aarch64 `.app` + `.dmg`, Linux amd64 `.deb` + `.rpm` + `.AppImage` — green in run
  `35572294916`. Batch A also added `icons/icon.icns`, made the QEMU install guidance follow the
  platform (`winget` / Homebrew / the distribution's package, via
  `sandbox::qemu_discover::install_hint_for`) and pinned the Unix `-qmp unix:` argument with a
  cross-platform unit test. What is **not** done: nobody has launched those packages, they are
  **unsigned** (macOS Gatekeeper blocks a first run; Developer ID signing and notarization belong to
  the commercialisation layer), and a real Unix-socket QEMU run still needs a Mac or a Linux machine.
  Windows remains the platform the golden path is verified on. The CI packages on hand at the time
  were built under the `0.6.0-preview.1` name, so the v0.7.0 release dispatched `bundle` again to get
  `0.7.0`-named ones — which is what it shipped.
- **`v0.6.0-preview.1` is released as a pre-release** (v0.6 batches 1–2, released in batch 4): two
  runs are compared field by field — the data layer and the API ([../host-core/src/run_diff.rs](../host-core/src/run_diff.rs),
  `AppState::compare_run_fingerprints`, the `compare_run_fingerprints` command) and the collapsed
  block under the audit tab's two-run panel. A pre-release takes **no Latest marker**, so the marker
  stayed on `v0.5.0` at the time (it has since moved to `v0.7.0`, and now to `v0.8.0`).
  Assets: `RiscDom_0.6.0-preview.1_x64_en-US.msi` and
  `RiscDom_0.6.0-preview.1_x64-setup.exe`, built with `bundle.windows.wix.version = "0.6.0"` (WiX
  cannot take a pre-release `ProductVersion`), so *Apps & features* shows `0.6.0` while the artifact
  names keep the package version. What it proves and what it does not is in
  [RELEASE_NOTES.md](../RELEASE_NOTES.md) — and the gap it named first has since closed: **the
  step-8 interface has been walked by eye and passed** (the operator's walk; no separate record file
  was archived, so `walkthroughs/` still holds only the v0.5 local walk).
- **`v0.5.0` was released, and it held the Latest marker then.** (Latest has since moved on, to
  `v0.7.0` and then `v0.8.0`.)
  <https://github.com/breakevery/riscdom/releases/tag/v0.5.0> — assets `RiscDom_0.5.0_x64_en-US.msi`
  and `RiscDom_0.5.0_x64-setup.exe`, built without the preview's MSI version override (so *Apps &
  features* shows `0.5.0`). What it proves and what it does not is in [RELEASE_NOTES.md](../RELEASE_NOTES.md).
- **`v0.5.0-preview.1` is kept as history** (a pre-release, which is why `v0.4.0` held the Latest
  marker until this release). Its assets stay where they were.
- **One walk has been recorded, and it was local**: [../walkthroughs/2026-09-19-preview1-local.md](../walkthroughs/2026-09-19-preview1-local.md)
  — seven steps, a real model and a real key, the **installed MSI**; but **not** a clean machine
  (QEMU and a RISC-V GCC were already installed there). Its five findings were fixed (S-1, G-1, E-1,
  E-2, E-3 in v0.5 batch 11; G-4 in batch 12); G-2 (a model change that did not take effect) and
  "typing Chinese into the chat box" still need a human with a keyboard, and G-3 (`0.5.0.1` in
  *Apps & features*) disappeared with the override itself.
- **An external walk is still outstanding.** It was this release's plan and it did not happen, so
  the clean-machine walk is now a **v0.5.x strengthening item** (§8), not a blocker: [golden-path-checklist.md](golden-path-checklist.md)
  is the form a tester fills in, and `walkthroughs/` is where it goes.
- Recent commits (newest first): `20f2052` (the v0.7.0 release preparation: version bump, changelog,
  release notes) ← `57dd25e` (the v0.7 documentation snapshot) ← `202dd75` (the non-Windows
  `extract_zip` stub) ← `344fd2b` (rpm for the Linux bundle) ← `0633bdc` (the macOS/Linux bundle CI
  job) ← `833f9c3` (platform-aware QEMU guidance, icon.icns, Unix QMP arg test) ← `06fef0a` (the
  language switch) ← `6abcb44` (the i18n pilot) ← `b0efeb8` (the v0.6.0-preview.1 release).
- Tags: `v0.8.0` is the newest tag and the release **holding the Latest marker** (confirmed with
  `gh release list`: 2026-09-22T07:37:50Z, annotated tag object
  `0b018081de1a4e89e04e7bc1570d595d38ab4b4b` → `8a5381436b62fa84b4f4a972a630061ce2203373`);
  `v0.7.0` = annotated tag object `f267f13dc6f8df9a3ff196b05d3bb9b7724f2d60` →
  `2bddae6b0897bb5fe262af2b7e4bf4b3733ec7eb` (it held the Latest marker until this release, which moved
  it); `v0.6.0-preview.1` and `v0.5.0-preview.1` are pre-releases; `v0.5.0` =
  `cea44f7b9920a079422217f811afb49350e08477` → `287ffdb095e1659b89a8cafe040647ada64d0026`;
  `v0.4.0` = `25bd3da3c31c1d1ec7e163f3835b0c2bbb74546d` → `15fda1f6d76d53a4ff1b621c2d3d91f0b4b87311`;
  `v0.3.1` = `d8fdba66a366632ca569d8db2657ab5a566b991c` → `b9be9111c620faad686c7a9d095e0ebc04b31225`.
- Test totals at `main` (this release, `602f402`): **330 passed / 0 failed / 8 ignored / 90 suites** —
  324 / 0 / 8 / 87 before the supervisor batch, 295 / 0 / 8 / 80 at the `v0.7.0` release commit,
  291 / 0 / 8 / 80 at `v0.6.0-preview.1`, and 281 / 0 / 8 / 79 at `v0.5.0`. The gate is 13 steps (v0.7
  batch 1 added the UI string registry), green locally and in CI (`scripts/gate.sh` on `ubuntu-latest`
  plus gitleaks).
- **The architecture-evolution note is finalized and on disk**: [architecture-evolution.md](architecture-evolution.md)
  (bilingual, paired with [architecture-evolution.zh-CN.md](architecture-evolution.zh-CN.md)) records the
  architecture re-assessment done after v0.7.0 — the four layers and the syscall-layer mechanism/policy
  split, the settled decisions (Tauri decoupling A3 → A1, the B2 multi-process model, the audit chain as
  a single chain + agent_id), the seams left open for multi-device, and the milestone path to the v1.0
  kernel-API freeze. Documentation only: no code changed.
- **v0.8 main deliverable is complete — a supervisor dispatcher plus several executors, demonstrable**
  (v0.8 main deliverable 2/2): the prototype runs end to end. `worker` gained a library half
  (`worker::supervisor`) and a runnable demo (`cargo run -p worker --example dispatch`): it starts
  several executor processes that **share one workspace** and each own a **data dir**, routes each task
  to the executor named in `Task.target`, and prints one line per task plus a tally. Tasks are
  dispatched **concurrently** (`std::thread::scope`, one thread per task — the handles are `Send +
  Sync`, so no thread-pool dependency), and a task naming an executor that is not in the fleet is
  **refused** rather than sent to a best guess. No model sits anywhere in the supervisor: for this
  stage it is a dispatcher, not an agent. Same batch: `worker`'s `audit` dependency (declared, never
  used) is gone, the supervisor logic lives in `worker`'s library so the demo and the tests share one
  implementation, and the four settled decisions are written down in
  [the multi-agent foundation note](multi-agent-foundation.md).
- **The two-process prototype has landed** (v0.8 main deliverable 1/2): a supervisor and an executor
  as separate processes, over **stdio + JSON lines**. `worker` (new crate; the executor binary) reads
  one `Task` JSON line on stdin, runs the host's own `run_agent` path with an injected data dir, and
  writes one `TaskOutcome` line on stdout; its events go to stderr as JSON lines. On the supervisor
  side, `host::StdioExecutorHandle` (an `AgentHandle`) spawns that binary, sends the task, reads the
  outcome, drains the events, and kills it if it does not answer in time — nothing above the handle
  changed, which is the seam v0.8 batch 4 left open. Transport needs no new dependency; the worker
  **does link Tauri** (host depends on it unconditionally — making it optional is a v0.9 cleanup).
  Two edges worth knowing: the child's own `agent_id` comes back through its **events**, while the
  dispatcher's `TaskOutcome.agent_id` keeps batch 4's meaning (the executor the task was addressed
  to); and an executor with an API key in its environment **will really run** (the probe test removes
  the key, so nothing here calls a model).
- **A minimal dispatch abstraction has landed** (v0.8 batch 4): work can now be *dispatched*
  rather than only called inline. `agent::dispatch` holds the vocabulary — `Task`, `TaskId`
  (`task-<pid>-<seq>`), `AgentId` (the batch-3 identity shape), `TaskOutcome`, `DispatchError` — and
  the two traits that make the seam: `AgentHandle` (an executor: run this task, return the outcome)
  and `Dispatcher`. The **local** half is implemented: `agent::LocalAgent` wraps a loop,
  `agent::LocalDispatcher` routes by target, and the host adds `HostAgentHandle` +
  `host::local_dispatcher` over its existing `run_agent` path. The **remote** half is deliberately
  absent — that absence *is* the seam. It lives in `agent`, not `host`, precisely so the abstraction
  is not owned by the Tauri-facing crate and does **not** force the host-core / host-tauri split
  ([architecture-evolution](architecture-evolution.md) §7 seam 2). The Tauri commands still call
  `run_agent` unchanged: the dispatch path is an added internal route.
- **The v0.8 technical-debt batch has landed** (v0.8 batch 1): the three dead-ends the
  [architecture-evolution note](architecture-evolution.md) §8 listed as debt are cleared. The app-data
  directory is **injected** — `AppState::with_data_dir(workspace, data_dir)` replaces the process-wide
  `OnceLock` in `host::paths`, so two instances in one process keep their own `settings.json`, sessions
  DB and toolchain directory. The audit chain gains an **`agent_id`** column, sitting *beside* the
  chain: the hash formula, the `prev_hash` linkage and every existing row's `hash` are untouched, so a
  pre-v0.8 chain still verifies. And **one VM slot per `AppState`** is pinned by a test rather than
  assumed. Code, not documentation: two new audit tests and one new host test (`multi_instance.rs`),
  and no change on the golden path.
- **Every audit event now names its agent, and snapshots are per agent** (v0.8 batch 3): the identity
  is `local-<pid>-<seq>` (`agent::next_agent_id`), minted once per `AppState` and passed to the
  `AgentLoop` it builds, so the host's, the sandbox's and the agent's events all carry it; `AgentLoop`
  and the `audit_hook` helpers take it as a parameter. `agent_id` stays a **beside-the-chain** field —
  no hash formula, `prev_hash` link or historical row changes. Snapshots move to
  `<workspace>/.riscdom/snapshots/<agent_id>/` with a read fallback to the shared root, so two agents
  sharing a workspace cannot overwrite each other's names and pre-v0.8 snapshots still list, restore and
  delete.
- **Audit writes now survive several processes, and a failure is loud** (v0.8 batch 2): `audit.db` is
  shared on purpose (one chain per workspace), so the connection opens in **WAL** with a 5 s
  `busy_timeout` and `synchronous=NORMAL`; an append takes the write lock **before** reading the head
  (`BEGIN IMMEDIATE` — without it two writers chained onto the same row and forked the chain, which the
  new concurrency test caught); and a locked append is retried 5× with 20/40/80/160 ms backoff before it
  reports. `AuditSink::record` now returns `Result<(), AuditError>` instead of dropping the event; the
  sink tells the host, which logs it, emits `audit:failed` and (by default) shows a banner + dialog in
  *Settings → Audit*. **Product decision: the alert is on by default and the user may switch it off;
  the event and the log line cannot be switched off.** The chain structure, the hash formula, the
  historical rows and the triggers are untouched.
- Open items: the temp directories under `%TEMP%` have not been cleaned (the deletion confirmation
  was never granted; 144 `riscdom-*` entries were counted on 2026-09-21); CLA.md awaits a lawyer's
  eye; **the clean-machine walk by someone else has not happened** — still a v0.5.x strengthening item
  rather than a blocker; **the macOS/Linux packages have never been launched** and are unsigned
  (signing is a commercialisation-layer item); the two items the v0.5 walk left for a human (G-2, and
  typing Chinese with a real keyboard) are still open. **The v0.7.0 release itself is the next
  batch**: push, tag, release, and a fresh `bundle` dispatch for the `0.7.0`-named macOS/Linux
  packages.

## 2. Remote operations are per-turn and authorised in words

Pushing, tagging, creating or deleting a release, moving or deleting a remote tag, and any other
remote write happen **only** when the current request says so in plain words. A menu choice, an
inferred intent, a previous batch's authorisation or "it is obviously the next step" is not
authorisation. The standing close-out (gate → commit → push) is part of a batch that asks for it,
not a default.

## 3. Commit messages: ASCII, or `git commit -F`

`git commit -m "…"` on Windows passes the message through the console's ANSI code page, so
non-ASCII text is replaced by `?` (`0x3F`) **before git sees it** — measured here on 2026-09-18: a
probe subject `test: 中文正文测试` was stored as `test: ?????????`. Therefore:

- write `-m` messages in ASCII (English);
- when a message must contain non-ASCII, write it to a file as **UTF-8 without BOM** and use
  `git commit -F <file>`;
- `scripts/commit.ps1 "<msg>"` takes the message as an argument, so the same rule applies to it.

The same accident has a second door: **PowerShell redirection**. `>` and `Out-File` write
**UTF-16LE** by default, so `git show … > file` (or any command that redirects text) leaves a file
whose first bytes are `FF FE` — anything that reads it as UTF-8 sees a broken first character
(v0.5 batch 12 hit this while diffing an old revision). Two habits cover both doors: write text
through a file with an explicit encoding, and pass it as a **file** rather than as a command-line
argument, because the argv path is the one that eats characters. To sweep a tree for the whole
family, run `python3 scripts/scan-encoding.py` by hand — a diagnostic, **not** a gate check
(§4 says why it stays out: its `?`-literal rule cannot tell damage from legitimate code).

## 4. The gate is the only list of what "green" means

`scripts/gate.sh` holds every check, in order. `scripts/gate.ps1` and `scripts/commit.ps1` are thin
wrappers: they locate Git's `sh.exe` and run that file; they never keep a second copy of the
command list. CI calls the same `sh scripts/gate.sh`. **A new check goes into `gate.sh`** (and into
the short list in `CONTRIBUTING.md`), never into a workflow, a README, or someone's memory.

## 5. Audit invariants

Frozen and never to be changed: the chain structure (`audit_events` and its append-only triggers),
the hash formula, historical rows, and the semantics of `audit-verify` and `audit-rebuild`. The
read-only surface may grow — an export, filters, a derived-index column with a migration — as long
as the chain is untouched and a checker keeps no write path.

## 6. The CLA is on standby, not an active flow

- `CLA.md` (English authoritative) and `CLA.zh-CN.md`; a conditional CLA section in
  `CONTRIBUTING.md`; `.github/workflows/cla.yml` running the **self-hosted** CLA Assistant action
  (no GitHub App to install) on `pull_request_target` **without checking out the PR's code**;
  `signatures/version1/cla.json` pre-created.
- Clause 3 (dual licensing and relicensing) is the commercialisation-critical grant and **needs a
  lawyer's review** before anyone relies on it. The document states that it takes effect only once
  the project owner confirms it.
- The signing sentence is **not translated** — the bot matches
  `I have read the CLA Document and I hereby sign the CLA` exactly.
- Contributions may be taken in somewhere other than this repository later, which is why the
  CONTRIBUTING section is phrased conditionally.

## 7. QEMU: guided install, never downloaded or bundled

Settled in v0.4: the app tells the user what to install (`winget install
SoftwareFreedomConservancy.QEMU`, or the official page) and then finds, remembers and verifies it.
The reasons — no upstream Windows binary to pin, an unnamed third-party packager as a supply-chain
link, and not becoming the distributor of a GPL-2.0 binary — are in
[qemu-distribution.md](qemu-distribution.md) §5.

## 8. The release gate is the walk, not the code

v0.5 shipped when a person walked steps 1–2 on a clean machine with a real API key and recorded it
against [golden-path-checklist.md](golden-path-checklist.md) — machine, OS build, QEMU/GCC versions
**and their sources**, provider and model, the preflight's four steps, the two runs' short
fingerprints, the exported file and its `audit-verify` verdict, and any failure verbatim. That walk
did **not** happen before `v0.5.0` shipped, so it is a **v0.5.x strengthening item** rather than a
blocker (§1), with [../walkthroughs/2026-09-19-preview1-local.md](../walkthroughs/2026-09-19-preview1-local.md)
as the local walk standing in for it. Steps 3–7 are covered by `cargo test -p host-core --test golden_path
-- --ignored`; step 8's comparison has **no** such automated walk yet (§9). "The implementation is
complete" is not the gate.

## 9. v0.6 starts at golden-path step 8 — and the step is delivered

v0.6 is the automatic comparison of two runs — which fields differ between their fingerprints —
which v0.5 deliberately stops short of. **Delivered** in batches 1–2 (`host-core/src/run_diff.rs`,
`AppState::compare_run_fingerprints`, the `compare_run_fingerprints` command, and the collapsed
field-level block under the audit tab's two-run panel); it awaits a walk and a release. The parallel
items in `PROJECT_CONSTITUTION.md` §10's v0.5 roadmap (QEMU stdio, macOS/Linux, several VMs,
incremental snapshots, session encryption, several AIs, a bilingual interface) are **not** v0.6
content: they are candidates to be chosen or dropped.

## 10. Documentation is bilingual, enforced

Every `*.md` in the repository needs a counterpart and an exact language switcher on line 1
(`X.md` ↔ `X.zh-CN.md`); `scripts/check-bilingual.sh` reports and never rewrites. A new document
means a new pair, in the same commit.

## 11. Building installers on this machine needs a real Node

The `node` on `PATH` in this workspace is LobsterAI's Electron-as-node shim
(`…\LobsterAI\cowork\bin\node.cmd` → `ELECTRON_RUN_AS_NODE=1 "<electron>" %*`). Under it the Tauri
CLI's native addon mis-reads `argv` and every bundling command dies with
`error: unrecognized subcommand '<…>\LobsterAI.exe'`. Workaround: run the CLI with a real Node —
e.g. `C:\Users\cloud_user\AppData\Local\Programs\Tuanjie Cowork\cli\bin\win32-x64\node.exe`
(v24.16.0) — as `node node_modules\@tauri-apps\cli\tauri.js build` from `ui/`. WiX 3.14 and NSIS are
cached under `%LOCALAPPDATA%\tauri`, so bundling needs no network.

## 12. `wix.version` is a preview-only override, and it is guarded

A package version with a pre-release suffix is **not** a valid MSI `ProductVersion` (WiX takes
`major.minor.patch.build`, numeric only), so `ui/src-tauri/tauri.conf.json` carries
`bundle.windows.wix.version = "0.5.0.1"` while the package version — and therefore the artifact
names — stays `0.5.0-preview.1`. **Delete that field as soon as the package version is numeric
again**; a leftover value makes the MSI's ProductVersion disagree with every other artifact, tag and
document, with no build error to notice it by. `scripts/check-wix-version.mjs` (in the gate, with
its own self-test) fails the build in exactly that case.
