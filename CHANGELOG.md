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

**The host's tests moved with the split, and two guards are whole again.** The 39
integration test files now live in `host-core/tests`, so they test the portable half where
it lives; and the mirror-constant guard and the clippy step cover both crates again.

**`worker` and `server` no longer link Tauri.** Both depend on the kernel facade's
portable half, so neither binary pulls a GUI toolkit into a headless process.

**The host split is complete: `host-core` + `host-tauri`.** The crate that carries the Tauri
commands is now named for what it is, the desktop shell is its only consumer, and nothing below
it links Tauri.

**The documents and comments catch up with the rename.** Every live reference to the pre-split
crate now points at `host-core` (kernel capability, tests) or `host-tauri` (the Tauri layer);
history is untouched.

**There is a CLI, and it is a control-plane client.** `riscdom` drives the control plane from a
shell — the same HTTP interface the desktop app uses — with the local mode starting the control
plane inside its own process.

**Zig is the second language the sandbox can build.** A `.zig` source is compiled through
`zig build-exe -target riscv64-freestanding` into the same bare-metal ELF at the same load
address; Zig brings its own cross linker, so the freestanding target needs no external
toolchain and no sysroot. The language follows the source extension, so
`compile_freestanding` keeps its signature and the C path is untouched, and the generated
linker script is shared verbatim. Downloading Zig's own archive is **not** part of this:
its macOS/Linux builds are `.tar.xz`, which the existing downloader cannot unpack — that
is a separate batch, and it serves Rust as well.

### Added

- **Zig compiles (v0.9 F3a)**: `compile` dispatches on the source extension — `.c` /
  `.h` / `.S` / `.s` through GCC, `.zig` through `zig build-exe -target
  riscv64-freestanding`. Nothing is injected for Zig: the source writes its own `_start`
  (the `-bios none` guest jumps to the load address, so the startup code must be first),
  and the generated `link.ld` is reused as it stands. `ZigConfig` discovers the compiler
  (`RISCDOM_ZIG` → well-known locations → `PATH`) and `settings.json` gained `zig_path`,
  pinned by `AppState::set_zig_path` / `clear_zig_path`. The Zig archive is **not**
  downloaded here: its macOS/Linux builds are `.tar.xz`, which the current downloader
  cannot unpack (a separate batch, shared with Rust).
- **`cli`, a new workspace crate with the `riscdom` binary**: eight read-only commands
  (`health`, `status`, `agents`, `runs list` / `runs get <id>`, `audit status`,
  `audit events`, `snapshots list`), each one HTTP against the control plane.
- **Two modes, one code path**: `--remote host:port` talks to a running `riscdom-server`;
  without it the CLI starts the control plane on `127.0.0.1:0` **inside its own process** and
  speaks HTTP to that. The CLI never calls `AppState` directly.
- **`--json`** passes the control plane's answer through unchanged (the error object on stderr
  when it fails); human mode prints tables and `key value` lines.
- **Exit codes**: `0` success, `1` local failure, `2` usage or `400`, `3` refused or
  `5xx`, `4` `401`/`403`. Documented in `cli/README.md`.
- **Token handling**: local mode reads (and, on first use, generates) `<data-dir>/token` through
  the same code `riscdom-server` runs; remote mode prefers `--token-file`, then
  `RISCDOM_TOKEN`, and only then `--token` — which warns, because it lands in the shell history.
  The token is never printed or logged.

**`server` is clippy-clean, and the gate lints it.** The crate had never been linted — adding
`-p cli` to the clippy step is what surfaced it.

### Fixed

- **Six `clippy::result_large_err` sites in `server`**: `http.rs:379` and
  `routes.rs:489/495/503/513/522` returned `Result<_, Response<RespBody>>`, and hyper's
  `Response` is 128+ bytes. The error is now `Box<Response<RespBody>>` and every caller returns
  `*response` — the same response value on the same path. Three `bool_assert_comparison`
  assertions in `routes.rs`'s tests and one `filter_next` in `tests/smoke.rs` are fixed with
  them. No behaviour changed.
- **`scripts/gate.sh`** selects `-p cli -p server -p host-core -p host-tauri` (still
  `--no-deps`), so the control plane is linted like everything else we own.

### Changed

- **`scripts/gate.sh`** lints `-p cli -p host-core -p host-tauri` with `--no-deps`, so the new
  crate is covered without dragging the (never-linted) `server` crate's own findings into the
  gate.

### Changed

- **Stale `host` references repointed** across the root `README`, `CONTRIBUTING`,
  `SECURITY`, `PROJECT_CONSTITUTION`, `THIRD_PARTY_NOTICES`, eleven `docs/` pairs,
  `ui/README`, `ci.yml`'s comment and the doc comments of the four source files that named the
  old crate: `-p host` → `-p host-core`, `host/tests` → `host-core/tests`, `host/src/…` →
  `host-core/src/…` (or `host-tauri/src/commands.rs` where the file is the Tauri layer),
  `host/README.md` → `host-tauri/README.md`, `host::` → `host_core::` / `host_tauri::`.
  The CHANGELOG, RELEASE_NOTES, the decisions ledger, handoff §1 and the architecture-evolution
  snapshot keep their historical mentions.
- **`ui/scripts/probe-ui-*.mjs`** and the gate scripts had already been repointed in wave 4; the
  workspace is now free of live stale references (`host/src`, `host/tests`, `-p host`, `host::`).

### Changed

- **`host` is renamed `host-tauri`** (directory, `[package] name`, workspace member), and the
  desktop shell depends on it: all 57 `host::` paths in `ui/src-tauri/src/lib.rs` became
  `host_tauri::`, resolving through the facade, so `ui/src-tauri` needs no direct `host-core`
  edge. `cargo tree`: `-p host-core` 0 Tauri lines, `-p host-tauri` 15, `-p worker` /
  `-p server` 0.
- **The dead `tokio` dependency is gone** from `host-tauri/Cargo.toml`: nothing in the crate or
  its tests ever named it (`cargo tree` still shows tokio through Tauri).
- **`host-tauri/README.md`** now documents the two-crate boundary and points at `host-core` for
  the kernel capability; **`worker/README.md`** is new (it had none), covering the CLI, the stdio
  protocol, the supervisor half and the demo.

### Changed

- **`worker` + `server` depend on `host-core` instead of `host`** (A1 wave 3): 30 `host::`
  paths rewritten to `host_core::` (7 in 4 worker files, 23 in 7 server files) and each
  dependency line moved to the portable half. `cargo tree -p worker` and `-p server` now
  name no Tauri crate; each listed 15 `tauri` lines before. No logic changed.
- **`server/README.md`** states the Layer 3 boundary without the old caveat that `tauri` is
  still linked; `worker`'s and `server`'s `Cargo.toml` comments say the same.

### Changed

- **`host/tests` → `host-core/tests`** (39 files, `git mv`, history kept), with all 133
  `host::` paths rewritten to `host_core::`. `host` now has no tests, and its
  `[dev-dependencies]` are gone: the tests use `host-core`'s own dependencies.
- **`scripts/check-mirrored-constants.mjs` scans `host-core/src` and `host/src`** — 17 files,
  up from the 3 that wave 1's move had left inside the guard. `--dir` may be repeated.
- **`scripts/gate.sh` lints `host-core` too**:
  `cargo clippy -p host-core -p host --all-targets -- -D warnings`. Wave 1's split had put
  the bulk of the host outside every clippy step.

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

**The CLI drives the control plane, and it asks before it destroys.** `riscdom` now covers
the control half of the API as well — `run`, `vm stop`, `vm start` (still the reserved
`501`), the snapshot and session writes, `runs abandon-stale` — and two things came with
them: a confirmation the five destructive commands require (a prompt on a terminal, `--yes`
in a script, a refusal when there is nobody to ask), and `--follow`, which prints the
`/v0/events` stream while a run is going.

### Added

- **Twelve control subcommands in `riscdom`**: `run <task>`, `vm stop`, `vm start`,
  `snapshots save|resume|delete <name>`, `sessions create|open|rename|delete|clear-all` and
  `runs abandon-stale`. Each one is an HTTP `POST` through the same client the read-only
  commands use; human mode prints `kind`/`iterations` and the answer for a run, the byte
  count for a save, and whether anything was deleted for a snapshot delete.
- **The confirmation, and `--yes` (alias `-y`)**: the five commands that destroy state
  (`vm stop`, `snapshots resume`, `snapshots delete`, `sessions delete`,
  `sessions clear-all`) ask first. `--yes` answers up front; a terminal is prompted and
  anything but `y`/`yes` declines; stdin that is not a terminal is refused with exit `2`,
  so a script cannot destroy state by omission.
- **`--follow` (alias `-f`), `run` only**: the CLI subscribes to `GET /v0/events`
  *before* it starts the run, so no event is missed, prints one line per frame (the event
  name and a short payload; the envelope verbatim under `--json`), and then the outcome.
  On any other command it is a usage error.
- **`cli/src/sse.rs`**: the frame reader — `id:` / `data:` lines, a blank line terminating
  each frame, comment heartbeats skipped, several `data:` lines joined into one payload.

### Changed

- **`cli/src/lib.rs` owns the dispatch and `cli/src/client.rs` the session**: one `Session`
  per invocation (remote, or the control plane embedded in this process, with the host's own
  credential presented back to it), and three timeouts, because a read answers now, a control
  may take half an hour, and an event stream must never time out.
- **`cli/tests/control.rs`** drives the real binary against a real control plane with the
  model environment cleared, so the suite stays off the network and off QEMU.

**The CLI is complete: exports, configuration, and a `--wait` that watches the work.**
`riscdom` gained the last seventeen endpoints — the three exports and the fourteen
configuration commands — so every control in the API document is a shell command now.

### Added

- **The export commands**: `export audit-jsonl [--out <path>]`,
  `export run-audit <run_id> [--out <path>]`, `export serial-log [--out <path>]`.
  `--out` is the *server's* path — resolved against the workspace root, refused with `403`
  when it escapes — and the CLI never receives the file. The defaults are `audit.jsonl`,
  `run-<run_id>.jsonl` and `serial.log`. Human mode names the count:
  `exported 3 events to audit.jsonl` / `wrote 4096 bytes to serial.log` (the two audit
  exports answer with a count of **events**, the serial export with **bytes**; the control
  plane calls the field `bytes_written` either way).
- **Fourteen configuration commands**, grouped: `llm set|clear|load-key <provider_id>`,
  `qemu path <file>|clear`, `toolchain download|cancel|path <file>|clear`,
  `preflight run|ack`, `audit alert set <on|off>`, `theme set <light|dark|system>`,
  `language set <system|en|zh>`. `llm set` takes the endpoint's five fields: `--api-key`
  (or `--api-key-file`), `--base-url`, `--model`, `--provider-id`, `--remember`.
- **`--api-key-file`** reads the key from a file — the shape that keeps it out of the shell
  history and `ps` — and **`--remember`** is what persists it to the OS credential store.
- **`--wait`** for the two asynchronous controls. It subscribes to `/v0/events` *before*
  posting, prints that work's frames (`toolchain:download` / `preflight:progress`) and closes
  with `download ok` / `preflight failed`; the exit code is the **work's** verdict, so a
  failed download or a failed preflight is `3`.
- **The confirmation reaches the three clears**: `llm clear`, `qemu clear` and
  `toolchain clear` ask before they act, because what they remove cannot be read back out of
  the host.

### Changed

- **`cli/src/lib.rs` owns a second streaming path** (`wait`) beside `--follow`, both built on
  one `subscribe` helper that opens the stream and eats the `hello` frame; the preflight's end
  is the first `failed` (fail-fast) or the last step's `ok`, read from
  `host_core::preflight::STEPS` rather than a literal.
- **`cli/src/client.rs`** reads `--api-key-file` and warns about `--api-key` the way `--token`
  warns; **`cli/src/render.rs`** adds three renderers (an export's count, the `202`
  acknowledgement, the preflight's step table), while the fourteen `204` answers keep
  printing `ok` through the empty-body path that was already there.

**The interface tells the truth, and the library's log is opt-in.** Four small things the
CLI batches left behind, fixed together: two export fields renamed to say what they count,
a workspace-path refusal moved from `403` to `400`, and the server's runtime logging put
behind a switch that defaults to off — which is what keeps an embedded server out of the
CLI's stderr.

### Changed

- **`POST /v0/audit/export` and `POST /v0/runs/export` answer `events_exported`**, not
  `bytes_written`: the host's `write_events_jsonl` returns `events.len()`, so the old field
  name described something the endpoint never did. `POST /v0/serial/export` really does write
  bytes and keeps `bytes_written`. The CLI renders `exported N events to <path>` and
  `wrote N bytes to <path>` accordingly.
- **A path the workspace policy refuses is a `400 bad_request` with `cause: "path"`**, not a
  `403 forbidden`: it is the caller's parameter being unusable, and `403` is reserved for
  authentication and authorisation (the capability check and the `Authn` hook, whose tests
  pin `403` unchanged). The rule covers the two audit exports, the serial export and
  `/v0/workspace/file`.
- **The library's runtime logging is off by default and opt-in**: `--log-level <off|error|info>`
  on `riscdom-server`, `ServerConfig::with_log_level` for a library caller. `error` writes
  failures (a failed accept, a download or preflight that ended badly), `info` adds the
  per-connection `connection … ended` line. The four sites that used to write unconditionally
  are the switch's only callers; the binary's own start-up banner, usage text and fatal
  errors stay as they were, because an embedded server never runs that `main`.
- **`docs/handoff.md` §1** lost its two stale `../host/src/run_diff.rs` links (the file has
  lived in `host-core` since the A1 split).

**The two assemblies are one shape: QEMU is wired like the toolchain, and its refusal is the
recorded decision.** The reconnaissance found `qemu_download` was code without a caller; F1
adds the caller — slot, status, cancel, adoption, audit, event family, Tauri commands,
endpoints, CLI — in the toolchain's exact shape, while leaving the download itself unwired by
decision (no QEMU release is pinned; the project guides instead of fetching).

### Added

- **The QEMU download wiring** (`host-core`): `AppState::begin_qemu_download` /
  `qemu_download_status` / `cancel_qemu_download` / `download_qemu_now`, plus
  `record_qemu_download_event`, `finish_qemu_download` and `qemu_dir()`. The slot is its own
  (`qemu_download`), so a running toolchain download never blocks a QEMU one.
- **Audit events** `host.qemu.download.start|done|failed|cancelled`, mirroring the
  toolchain's four.
- **`qemu:download`**, the twelfth SSE event, with the toolchain's payload shape (the
  internally tagged enum under the tag `state`): `started` / `progress` / `verifying` /
  `extracting` / `done` / `failed` / `cancelled`.
- **Three Tauri commands** (`start_qemu_download`, `cancel_qemu_download`,
  `qemu_download_status`), registered in the desktop shell. The UI is not wired to them in
  this batch.
- **Three endpoints**: `GET /v0/qemu/download` (`qemu.read`), `POST /v0/qemu/download` and
  `POST /v0/qemu/download/cancel` (`qemu.configure`) — the API tables are 27 queries and 29
  controls now.
- **Three CLI commands**: `qemu download [--wait]`, `qemu cancel`, `qemu status`.
- **`QemuDownloadStatus`** and **`paths::qemu_dir_in`** (`<data-dir>/qemu`, versions side by
  side, nothing pruned).

### Changed

- **`QemuDownloadEvent`'s payload tag is `state`**, not `kind`: the two downloads report
  themselves with one vocabulary, which is what lets a client read one shape for both.
- **The CLI's `--wait` has one terminal predicate** (`download_terminal`) for both download
  families instead of one per resource.
- **`spec_for_current_platform` is the platform branch, and it refuses everywhere**: no QEMU
  release is pinned (`docs/qemu-distribution.md` §5), so `POST /v0/qemu/download` answers
  `503 unavailable` with `cause: "qemu"` and the install guidance, and claims no slot. Every
  layer behind the refusal is tested against a loopback fixture, so pinning a release later
  is a data change.

**Sandboxes are names before they are runtimes, and the scan never writes back.** F2a-1
lands the definition layer: a definition a person writes in `settings.json`, the shapes the
control plane will serve, the scan of what is actually installed on this machine, and the
merge of the three. Switching, approval and `Task.sandbox` are later batches (F2b / F2c /
F2d), and so are the endpoints, the Tauri commands and the CLI (F2a-2) — this batch stops at
"a sandbox can be named and read".

### Added

- **`host-core/src/sandbox_def.rs`**: `SandboxDef` — `name`, plus the optional
  `display_name` / `memory_mb` / `qemu_exe` / `toolchain_path` / `kernel` / `notes` — and the
  shapes the API serves, `SandboxView` (the definition plus `source`, `runnable` and
  `shadowed`) and `CandidateView` / `CandidatesView` (one installed resource; the two
  independent lists, never a cartesian product). `DEFAULT_SANDBOX_NAME` is `"default"`.
- **`sandbox_def::discover_in(data_dir)`**, the read-only scan: every version directory under
  `<data-dir>/toolchain` and `<data-dir>/qemu`, found through the downloaders' own
  `find_compiler` / `find_qemu`, plus the QEMU this machine already has. The installers'
  `.download-tmp` / `.extract-tmp` by-products are skipped, a missing directory is an empty
  list, and nothing found is ever written back.
- **`LocalSettings::sandboxes` and `LocalSettings::default_sandbox`**, both `#[serde(default)]`:
  additive, so `SETTINGS_VERSION` stays at 1 and a `settings.json` written before the fields
  existed loads unchanged.
- **Five `AppState` methods**: `sandboxes()` (the merged registry: hand-written, then scanned,
  then the built-in `default`), `sandbox(name)`, `current_sandbox()`,
  `sandbox_candidates()` and `sandbox_default_name()`.
- **`Capability::SandboxRead`**, the 29th name (`sandbox.read`): additive, so every actor that
  held the other 28 holds it too and no request that used to succeed is refused.

### Changed

- **`host-core::find_compiler` and `host-core::find_qemu` are `pub(crate)`**, so the sandbox
  scan reuses the downloaders' notion of "installed" instead of keeping a second copy of it.
- **A hand-written definition wins a name collision, and the scanned entry stays visible**,
  marked `shadowed` — the merge reports itself instead of hiding behind the winner.
- **`runnable` is computed per read, never stored**: a QEMU that exists and answers
  `--version`, a toolchain that exists, and a kernel that exists or can be compiled. A
  definition whose resource was uninstalled is still a definition; it simply cannot run.
- **The capability count in the documents is 29** (`docs/control-plane-api.md` and its
  Chinese pair, `server/README.md` and its pair, `docs/control-plane-client-guide.md` and its
  pair). The §5 tables themselves still name the 28 routes that exist: `sandbox.read` gets
  its endpoints in F2a-2, so the count moves one ahead of the tables on purpose.

**The sandbox registry is reachable — read-only — from both surfaces.** F2a-2 adds the four
queries the definition layer needed (the merged registry, the current/default pair, the raw
scan and one definition by name), the four Tauri commands behind them and the four CLI
subcommands. Switching is still F2b: nothing here can change a sandbox.

### Added

- **Four queries** (`docs/control-plane-api.md` §5.1): `GET /v0/sandboxes` (`sandbox.read`),
  `/v0/sandboxes/current`, `/v0/sandboxes/candidates` and `/v0/sandboxes/{name}` — the second
  path-parameter route after `/v0/runs/{run_id}`, and the four routes that serve the 29th
  capability.
- **Four Tauri commands** (`host-tauri`): `list_sandboxes`, `current_sandbox`,
  `sandbox_candidates` and `get_sandbox`, registered in the desktop shell. The interface is
  not wired to them in this batch.
- **Four CLI subcommands**: `sandboxes list` / `current` / `candidates` / `show <name>`,
  each one an HTTP `GET`, with tables in human mode and `--json` passthrough.

### Changed

- **§5.1 is 31 queries, and the vocabulary's 29 names all have a route now.** F2a-1 shipped
  `sandbox.read` a batch before the routes that use it; the count and the tables are
  self-consistent again.
- **`/v0/sandboxes/{name}` never reads a literal sub-path as a name.** `current` and
  `candidates` are their own routes, and `requests` / `switch` / `assemble` — the three the
  rest of the F2 line reserves — answer `404` rather than resolving to a definition that
  happens to be called that.
- **`render` gained the four human-mode shapes** (`sandboxes`, `sandbox_current`,
  `sandbox_candidates`, `sandbox_detail`), and `args.rs` the four commands' paths, methods
  and encodings: a name is percent-encoded like a run id, so a slash cannot escape the one
  segment the route matches on.

**A version-less resource gets a name, not a concatenation.** F2a-1 named every scanned
definition `format!("{kind}-{version}")`, and the QEMU a machine already has carries no
version — so the merged registry listed a definition called `qemu--`, which F2a-2 then
served over HTTP and the CLI. It is `qemu-system-riscv64` now. Resources the scan *does*
know a version for are unchanged.

### Fixed

- **`qemu--` is gone.** A scanned resource the scan has no version for is named for the
  resource itself (a new `resource_name` behind `SandboxDef::for_resource`), not for a
  missing version: the machine's QEMU is `qemu-system-riscv64` on every platform. The name
  comes from `sandbox::qemu_discover::exe_name()` with the platform's `.exe` suffix trimmed,
  so no second copy of the stem is kept and a Windows file name does not become a definition
  name.

### Changed

- **The `"-"` sentinel has one definition**: `host-core::sandbox_def::NO_VERSION`, used by
  the scan's `CandidateView::system_qemu` and by the naming rule, instead of two literals
  that had to agree. It is exported with the rest of the definition layer's types.
- **An installed resource keeps `<kind>-<version>`** (`toolchain-15.2.0-1`, `qemu-11.1.0`).
  A version directory literally named `-` would read as the sentinel and take the
  version-less name; the installers never write one, and the edge is recorded here rather
  than defended against. Nothing else about the scan or the merge changed.

**A node can be switched to another sandbox, and the switch is validate-before-stop.**
F2b-1 lands the core: the sandbox a node is *running* became runtime state (the stored
`default_sandbox` is what a restart starts from), a definition is checked **before** the
running VM is touched, one switch runs at a time, and a switch is refused while a run is in
flight. The endpoints, the Tauri commands, the CLI and the `sandbox:switch` event are F2b-2.

### Added

- **`AppState::switch_sandbox(name)`**: resolve the definition, validate it (QEMU, toolchain,
  kernel), resolve the kernel, stop the current VM, start a fresh one from the definition
  (three attempts on fresh ports, the shape `tool_start_vm` uses), adopt it, and only then
  record the name as current.
- **`AppState::sandbox_check(def)`**: the reason a definition cannot run, as four new
  `HostError` variants — `sandbox_not_found`, `sandbox_qemu_missing`,
  `sandbox_toolchain_missing`, `sandbox_kernel_missing`. `sandbox_runnable` is now its
  boolean face: one rule, two shapes, same behaviour.
- **`AppState::run_in_flight()`**: whether a call is inside `run_agent` right now, read from
  the run bookkeeping `begin_run` sets and `finish_run` clears.
- **The switch slot**: `begin_sandbox_switch` / `cancel_sandbox_switch` /
  `finish_sandbox_switch`, the same shape as the download slots' four — one switch at a time,
  a second one refused with `already in progress`.

### Changed

- **`AppState::current_sandbox()` is runtime state now** (v0.9 F2b decision 1, ledger §34):
  `None` until a switch succeeds and **never** written to `settings.json`, while
  `sandbox_default_name()` keeps answering the stored `default_sandbox`. A switch changes
  what is running, not what is configured.
- **A failed switch leaves the node stopped, not half-switched**: the new handle is dropped,
  `Drop` kills whatever it spawned, and what is current does not change. A definition that
  fails validation leaves the running VM alone — that order is the point.
- **The registry's merge has one implementation** (`merged_sandbox_defs`), read by
  `sandboxes()` and by the switch, so the definition a switch uses is the winner the list
  shows.

**The switch has a surface: an event, an endpoint, a capability and a CLI command.**
F2b-2 exposes what F2b-1 built. `sandbox:switch` is the thirteenth event (`{from, to, ok,
reason}`, one per attempt, success or failure); `POST /v0/sandboxes/switch` is the one write
on the sandbox surface and answers a status per reason rather than a sentence; `sandbox.switch`
is the 30th capability; and `riscdom sandboxes switch <name>` is its CLI, in the destructive
family. The `events.rs` guard is whole again — it lists all thirteen names, so F1's
`qemu:download` is no longer missing from it.

### Added

- **`sandbox:switch`**, the thirteenth event, with `{from, to, ok, reason}` on every exit —
  `from` is the definition that was current (`null` when none was), `reason` carries the code
  when `ok` is `false`.
- **`POST /v0/sandboxes/switch`** (`capability sandbox.switch`): `200` with `{from, to}`;
  `404` `cause: "name"`; `409` `cause: "run"` or `cause: "sandbox"`; `503` with the reason
  code as `cause`; `500` `cause: "sandbox_start_failed"`.
- **`HostError::SandboxStart`**, so the "every check passed and the VM still would not
  start" case answers with a code rather than prose (the node is stopped, not half-switched).
- **`AppState::sandbox_switch_in_progress()`**, the probe the `409 cause: "sandbox"` is
  decided from — the shape `toolchain_download_status().in_progress` already had.
- **`riscdom sandboxes switch <name>`**, which asks first (it stops the running VM and
  refuses while a run is in flight); `--yes` answers up front and a non-terminal stdin is
  refused with exit `2`. The answer prints `switched from <old> to <new>`.
- **A Tauri command** `switch_sandbox`, registered in the desktop shell. The interface is
  not wired to it (that is the D line).

### Changed

- **`switch_sandbox` takes the `EventSink` its caller owns**, so the event travels the same
  way every other host event does: the route injects an `HttpEventSink`, the Tauri command a
  `TauriEventSink`. The switching itself is unchanged.
- **`events.rs`'s guard names all thirteen events** (`all_events_are_named`, was
  `all_eleven_events_are_named`), and asserts the names are unique — it had been missing
  `qemu:download` since F1.
- **The capability count in the documents is 30, and §5.2 has the switch row**: the
  API document (both languages), `server/README` (both) and the client guide (both).

**The port lease says what it means, and the test asserts that.** The relay's concurrency
test had been failing rarely — twice, both with the `sandbox` crate untouched: it recorded
every port it had *ever* leased and asserted no port appeared twice, but a lease only
promises that **live** leases differ. A number a finished holder released is free to come
back, and the OS does hand it out again. The test now parks every thread's leases until all
of them have leased, so it asserts the invariant the code keeps; nothing in the allocator
changed, because nothing in it was wrong (the check and the record are one lock scope, and
`PortLease` has exactly one construction site).

### Fixed

- **`concurrent_leases_never_repeat_a_port` asserts the right thing**: eight threads lease
  four ports each, every lease stays alive until the last thread has leased, and the numbers
  are compared then — so a port a finished thread released (and the OS handed out again) can
  no longer be mistaken for two holders at once. The message is unchanged.

### Added

- **`a_released_port_is_free_to_come_back`** pins the other half of the contract: a dropped
  lease unregisters its number and leaves the port bindable for anyone.
- **`sandbox/README.md`** (both languages) gained "the port-lease contract": what the lease
  promises, what it deliberately does not, and why.

### Changed

- **`PortLease`'s release removes its own number once** (`HashSet::remove`, was
  `Vec::retain`, which would have taken every equal entry with it — latent, and now
  impossible to write by accident).
- **`HELD_PORTS` is a set**: `LazyLock<Mutex<HashSet<u16>>>`, because `HashSet::new` cannot
  initialise a `static` (its hasher wants a runtime seed). `relay::reserve` is one `insert`;
  `leased_ports()` keeps its signature, and its order was never a contract (no caller reads
  one).

**An actor may ask for a sandbox change, and another actor decides it.** The sandbox surface
had one write (the switch, which needs `sandbox.switch`). It now has a **request queue**: the
agent — or any actor that may run one — can ask, and an actor that holds the capability the
request's action implies decides. Nothing is switched by a decision: the switch stays a
second, authorised call, so a request is a ledger of intent rather than a queued command.

### Added

- **The request queue** (v0.9 sandbox F2c). `POST /v0/sandboxes/requests` (`agent.run`)
  leaves an ask — `{action: switch|define|assemble, sandbox?, reason?}` — and answers `201`
  with `req-<pid>-<seq>` (its own namespace, not the tasks'). `GET
  /v0/sandboxes/requests?status=` (`sandbox.read`) lists it, newest first. The two decisions
  (`…/{id}/approve`, `…/{id}/reject`) take no body, answer `200` with the record, and are
  final: deciding twice is a `409`, an unknown id a `404`.
- **`sandbox.assemble`, the 31st capability**: moving a node and giving it a new definition
  to run are different powers, so a decision needs the one its request asks for —
  `sandbox.switch` for a `switch`, `sandbox.assemble` for `define` / `assemble`.
- **`SandboxRequester`, the agent crate's two sandbox tools**: `request_sandbox` (leaves an
  ask, answers the id) and `sandbox_status` (what runs now, what waits). The host implements
  the trait over cloned sub-handles and injects it into the loop — the loop cannot hold an
  `Arc<AppState>`, because it lives inside one.
- **Four Tauri commands** (`request_sandbox`, `list_sandbox_requests`,
  `approve_sandbox_request`, `reject_sandbox_request`), registered but not wired to the
  interface (the D line).
- **Three CLI subcommands**: `sandboxes requests [--status <s>]`,
  `sandboxes requests approve <id>`, `sandboxes requests reject <id>`. The two decisions
  confirm like `sandboxes switch` does, and stdin that is not a terminal is refused.
- **`sandbox:request`, the 14th SSE event**: `{id, status, requester, action}`, one frame per
  change (`pending`, then `approved` / `rejected`).

### Changed

- **Route-level capability is not always the whole check.** The two decisions declare
  `sandbox.read` — a decider has to be able to see the queue — and the handler checks the
  capability the request's own `action` implies against the actor, which is why `dispatch`
  now receives it. The API document says "every capability is enforced somewhere" rather
  than "every capability has a route": `sandbox.assemble` is enforced in that handler until
  the assemble endpoint lands.
- **The capability count in the documents is 31**, and `§5.1` / `§5.2` carry 32 / 33
  endpoints: the API document (both languages), `server/README` (both) and the client guide
  (both).
- **`all_events_are_named` lists fourteen events again**, and the events document's §3 table
  has fourteen rows.

**A project travels as one file, and the AI's writes are on the chain.** The workspace can
now leave as a `tar.gz` and come back as an archive — the two endpoints, the packers, the
guards and the capability split — and `write_source` records what it wrote.

### Added

- **`POST /v0/workspace/export`**: the workspace as a `tar.gz`, answered as **bytes**
  (the first non-JSON body on that surface apart from the event stream) with
  `Content-Disposition`. An empty workspace exports a valid empty archive. The host's own
  `.riscdom/` state is not packed.
- **`POST /v0/workspace/import`**: an archive as the request body — zip, tar.gz or tar,
  chosen by `Content-Type` and falling back to the bytes — answered with
  `{files, bytes}`. Its own **64 MiB** ceiling (`413`), so the shared 64 KiB JSON limit
  stays where it is. A file already in the workspace is `409` `cause: "exists"` unless
  `?force=true`; an entry that escapes the workspace, arrives as a symlink or hard link,
  names `.riscdom/`, or is not a readable archive is `400` `cause: "archive"`.
- **`workspace.write`, the 32nd capability**: importing replaces the project, exporting
  reads it, and the two are not the same permission.
- **`agent.file.write` `{path, bytes}`**: one audit row per `write_source` write, so a
  project's provenance is a row rather than a re-parse of a truncated tool argument. It is
  an audit event, not an SSE one — the event count stays 14.
- **Two Tauri commands** (`import_workspace`, `export_workspace`) and **two CLI
  subcommands** (`workspace import <archive> [--force]`, `workspace export [--out <file>]`;
  the archive goes to `--out` or stdout, the count to stderr).

### Changed

- **`zip` and `flate2` + `tar` are declared for every platform.** They were already in
  `Cargo.lock` — a Windows host read only zips, a unix host only tar.gz — and a project
  archive is whatever the user's tooling produced.
- **The capability count in the documents is 32**, and §5.2 carries 35 endpoints: the API
  document (both languages), `server/README` (both) and the client guide (both).

**A task declares its sandbox, and the node is not switched.** The sandbox line's last
piece: a run may say which definition it uses, the node's own sandbox is only what answers
when nothing is declared, and a declaration never moves the node.

### Added

- **`Task.sandbox`** (v0.9 sandbox F2d): `Option<String>` with `#[serde(default)]` — an
  older supervisor's task line still reads — plus `Task::with_sandbox`. The worker already
  deserialises a whole `Task`, so the cross-process protocol change is that one field.
- **`sandbox` on a run**: `POST /v0/agent/run` takes an optional `sandbox`; the Tauri
  `run_agent` command takes it as a parameter; the CLI grew `run <task> --sandbox <name>`.
- **`AppState::run_agent_for(emitter, input, sandbox)`**: the one place a run's sandbox is
  resolved and refused. `run_agent` keeps its signature and delegates.
- **`AppState::active_sandbox()`**: the definition the **running** VM came from — written
  when a run's VM appears, by `switch_sandbox`, and on a snapshot restore; cleared by
  `stop_current_vm`. `current_sandbox` alone could not answer it: only a switch wrote it,
  so a `start_vm`-started VM had no recorded provenance.
- **`agent.set_memory_mb`**, so a definition's `memory_mb` reaches the VM the tool starts.

### Changed

- **A run's refusal ladder moved into `run_agent_for`, in this order**: an unknown sandbox
  name is `404` `cause: "name"`; a name that is not what the running VM came from is `409`
  `cause: "sandbox"`; readiness is `503` `cause: "llm"`, now from a typed
  `HostError::NotConfigured` rather than the route checking twice. A bad parameter is
  therefore answered before the environment, and `POST /v0/agent/run` with an unknown
  sandbox is a `404` even with no model configured.
- **Resolution order for a run**: the declared name, else what the node runs, else its
  configured default, else the built-in fallback (which means “discover this host's own
  QEMU and toolchain” — the behaviour before this batch).

**The node can be dispatched to, and its fleet is configuration.** `Dispatcher` has been
reachable from Rust since v0.8 and from nowhere else; the task endpoint is what makes it an
interface. The fleet comes from `executors` in `settings.json`, and from nothing else.

### Added

- **`executors` in `settings.json`** (`ExecutorSpecSettings`: a label, a program and its
  arguments): registered into `StdioExecutorHandle`s once, after the settings file is read.
  **No `env`** — a settings file is not a secret store. Registration spawns nothing, so a
  program that is not there is the first task's failure, not a startup error.
- **`POST /v0/tasks`** (`agent.run`): a `Task`'s four scalar fields in, the executor's
  `TaskOutcome` out. A missing `id` is minted by the server. Synchronous, like
  `/v0/agent/run`: there is no task table and no `GET /v0/tasks/{id}`.
- **`GET /v0/executors`** (`agent.run`): the identities a task can reach, in configuration
  order — an empty list is a fact, not an error.
- **`AppState::dispatch_task`** (and `dispatch_task_value` / `executors`), plus two Tauri
  commands (`dispatch_task`, `list_executors`) registered but **not** wired to the interface
  — the D line.
- **Two CLI subcommands**: `executors list` and `tasks dispatch --target <agent_id> --input
  <text> [--sandbox <name>]`, with `--target` / `--input` as new flags.

### Changed

- **Two refusals, kept apart**: a target nobody owns is `404` `cause: "target"` (the caller's
  parameter — a node with no fleet refuses every target that way), while a dispatch that broke
  is `500` `cause: "task"`. A run that merely *failed* is still a `200`; its `outcome` says
  `failed`.
- **The node is not one of its own executors**: a target naming it is a `404`, because running
  here is `POST /v0/agent/run`. The two endpoints are siblings, not synonyms.
- **No capability was added**: both routes declare `agent.run` (E0 decision 3).

**Both tool schemas are documents, and both are checked.** The interface's other half was
vocabulary: what an executor's model may call, and what an AI supervisor may call.

### Added

- **`docs/tool-schema-executor.md`**: the eight tools `tool_specs()` declares, as the exact
  JSON array `tools_json()` puts in a request's `tools` field, plus a human table.
- **`docs/tool-schema-control-plane.md`**: every endpoint as an OpenAI-style function
  definition — 32 queries + 36 controls + 3 host-local + 4 path-parameter routes = 75 tools
  — grouped, with each row's capability and arguments, and a `tools[]`-ready array per
  group. Names are derived from the path (drop `/v0/`, fold separators; the `POST` side of a
  two-method path takes `_post`; the four id-taking routes get a verb).
- **`scripts/check-tool-schema.mjs`**, wired into `scripts/gate.sh`: the translation must
  carry byte-identical marked blocks; every table name must be the document's own
  derivation; every name needs both a row and a definition. It has a self-test that plants a
  drift and requires a rejection.

### Changed

- **`agent/README.md`'s tool table is an index, not a second catalogue**: it points at
  `docs/tool-schema-executor.md`, which a test compares with `tools_json()`
  (`agent/tests/tool_schema_doc.rs`). The control plane's tables are compared with the
  server's route table from `server/src/routes.rs`'s own tests.

**A supervisor you can run.** The reference implementation of the external half: a process
outside the kernel that drives a node over HTTP, with no model of its own.

### Added

- **`examples/python/dispatch.py`**: `GET /v0/executors` → `POST /v0/tasks` per task →
  `GET /v0/events` under `--follow`. Standard library only (`urllib.request`, `json`,
  `argparse`, and a hand-rolled SSE reader). Token from `--token-file` or `$RISCDOM_TOKEN`,
  never from an argument. Exit codes follow the CLI: `0` all succeeded, `1` something did
  not, `2` usage, `3` unreachable or refused.
- **`--self-test`**: a stdlib `http.server` fake control plane on `127.0.0.1:0`, so the
  script proves its own path offline (fleet list, a success, a failure, a refused target,
  the `--follow` subscription, a wrong token, a malformed task line).
- **`examples/python/README.md`**: what it demonstrates, the two documents' split of the
  picture, and a table comparing it with `worker/examples/dispatch.rs`.
- **A gate step**: `scripts/gate.sh` runs the self-test when `python3`/`python` is on `PATH`
  and prints a skip when it is not — the gate's first optional toolchain.

### Changed

- **`docs/control-plane-client-guide.md` §8** points at the example as the worked shape of
  an AI supervisor.

**The seam has its other half.** `AgentHandle` said since v0.8 that a remote implementation
implements exactly it and that none was written. One is now.

### Added

- **`worker/examples/remote_executor.rs`**: `HttpExecutorHandle`, an `AgentHandle` whose
  executor is another node over HTTP. Its `run` POSTs a task-shaped body to
  `POST /v0/tasks` and returns the `TaskOutcome` the remote executor produced — with the
  **node's** identity, never the handle's label. `404` maps to `NoSuchAgent`, every other
  failure to `Failed`, and an answer naming a different task is a protocol break.
- **`--self-test`**: a stand-in node on `127.0.0.1:0`, and seven assertions over the real
  handle path (body shape, parse, the node's identity, an unknown target, a mismatched
  answer, an unreachable node, a task addressed to someone else). `scripts/gate.sh` runs it.
- **A section in `worker/README.md`** and **§9 of the client guide**: what a remote handle is,
  the two names it carries, and why `POST /v0/tasks` (and not `/v0/agent/run`) is the
  dispatch.

### Changed

- **Nothing in production**: no crate in the workspace changed. The integration is the line
  the seam promised — `LocalDispatcher::new(vec![Arc::new(handle) as Arc<dyn AgentHandle>])`.

**Small debts paid, so the ground is clear.** Nothing new: four things tidied.

### Changed

- **No hand-bumped counts in the tests.** `server/tests/smoke.rs`'s
  `every_control_endpoint_answers` derives its expectation from the route table
  (`server::routes::control_paths()`, a new read-only accessor over the paths) instead of
  asserting a literal, so a control that lands without a case fails with the **missing path**
  in the message — and the seven controls the test deliberately leaves alone are *named*, not
  counted. `routes.rs`'s `the_table_has_the_documented_endpoints` now reads the counts off
  the API document's own §5 headings, in both languages, rather than carrying them as
  literals.
- **`__pycache__/` and `*.pyc` are ignored**: the Python reference supervisor's byte cache
  dirtied the working tree.
- **Stale counts in comments** — `server/src/lib.rs`'s "26 query / 27 control endpoints",
  `routes.rs`'s "the two path-parameter routes" — are either correct now or gone: a comment
  that counts endpoints is a comment that rots.
- **`agent/README.zh-CN.md`'s tool table** is the index its English sibling already was: the
  eight names and a pointer at the schema document, not a second copy of the descriptions.

**Mechanical: the repository owns its line endings now.** No behaviour, no prose — a
`.gitattributes`, a working-tree normalisation, and seven link paths.

### Added

- **`.gitattributes`**: `* text=auto eol=lf`, with `*.sh` named out loud and the six tracked
  binaries (`*.png`, `*.ico`, `*.icns`) marked `binary`. **No `*.ps1` exception**: all five
  PowerShell scripts in `scripts/` are LF today *and* are what every batch's gate and commit
  run through, so declaring CRLF would have meant writing it into five files rather than
  preserving it.

### Fixed

- **Seven relative links** that pointed at a root-level file from `docs/` (or the reverse)
  without the prefix: the CHANGELOG's `multi-agent-foundation` link,
  `handoff(.zh-CN).md` → `RELEASE_NOTES`, `qemu-distribution(.zh-CN).md` →
  `THIRD_PARTY_NOTICES`, `toolchain-setup(.zh-CN).md` → `ENVIRONMENT`. A scan of all 365
  relative links in the repository now reports **zero broken**.

### Changed

- **The working tree is LF everywhere.** 38 files held CRLF or mixed endings (27 pure CRLF,
  11 mixed — `sandbox/src/relay.rs` worst at 401 of its 413 lines), because
  `core.autocrlf=true` checked files out one way while the editing tools wrote another, and
  git could not see the difference. The **index** had been LF all along, so
  `git add --renormalize .` staged nothing and **this commit contains no line-ending
  change**: it is a working-tree repair, proven content-free by comparing every one of the
  356 tracked files with its committed blob (byte-identical, 0 differences).

**The documentation has an entrance.** One page that names every document in the
repository, who it is for, and whether it is living, a snapshot or history.

### Added

- **`docs/README.md`** (with its translation): the documentation map. Five audience
  sections — start here · kernel developers · distribution integrators · administrators ·
  end users · contributors — plus a section for what is deliberately outside it
  (`IDENTITY.md` / `SOUL.md` / `USER.md`, `LICENSE`, the CLA signature store). Every
  Markdown file in the repository appears with what it is for, its **state** (*living* /
  *snapshot* / *history*) and the version it applies to. History is labelled as history:
  `architecture-evolution.md` is the v0.7 snapshot, and the older `CHANGELOG` sections and
  the released `RELEASE_NOTES` are marked as not rewritten.

- **The two crate examples that were not named in their crate's README are now named.**
  `agent/README` and `audit/README`, both languages, gained an `## Example` section for
  `examples/audit_demo.rs` and `examples/chain_demo.rs`, in the shape `sandbox/README` already
  used (`sandbox`'s `run_hello`, and `worker`'s `dispatch` and `remote_executor`, were already
  documented — those two were the last).
- **Contributor templates.** `.github/ISSUE_TEMPLATE/bug_report.yml` and
  `feature_request.yml` (GitHub issue forms) and `.github/PULL_REQUEST_TEMPLATE.md` with its
  `.zh-CN.md` translation. The forms are `.yml` deliberately: `scripts/check-bilingual.sh`
  scans every `*.md`, so a Markdown template would need a `.zh-CN.md` sibling — and GitHub
  would then offer that sibling as a second template in the chooser. The pull-request template
  is Markdown (GitHub reads one, and a translation is the repository's rule), so its first line
  is the language switcher, which means that line also appears in a new pull request's body.

### Fixed

- **The root `README`'s layout tree** described `host/` and four crates; the workspace has
  eight (`cli`, `host-core`, `host-tauri`, `sandbox`, `audit`, `agent`, `worker`, `server`)
  plus `docs/`, `examples/`, `scripts/` and `walkthroughs/`. Its test list also called
  `host-core` "Tauri backend commands + serial deltas" — the Tauri half is `host-tauri`
  since v0.9's A1 split — and named four of the eight crates.
- **Two stale counts in the normative documents**: `control-plane-api.md` said §5.2 holds
  35 controls (it holds 36) and the client guide said the query surface is 31 endpoints (it
  is 32) — both in both languages.
- **CI was red on Linux from `00fca17` on, for four commits.** The `gate` job's
  `remote executor example` step is the first Linux step that compiles `host-core`, and
  `host-core`'s `keyring` backend on Linux builds `libdbus-sys`, which needs the system
  `dbus-1` library — which the runner does not ship. The `gate` job now installs
  `libdbus-1-dev` (and the Linux `bundle` job's dependency list gained it as well). A
  Windows developer machine uses `keyring`'s `windows-native` backend, so the local gate
  could never have caught this.
- **`cli`'s test binary compiled on Linux again.** Un-skipping `cli` there (the change above)
  failed at once: `cli/tests/control.rs`'s `write_executor_settings` was called only from a
  `#[cfg(windows)]` dispatch test but carried no gate of its own, so on Linux it was dead code
  — and `-D warnings` turns `dead_code` into a build failure. It is `#[cfg(windows)]` now, as
  is the `std::path::Path` import only it and the fake-executor helper use. No production code
  changed.

### Changed

- **The root `README`'s "More" section** opens with the map and lists all nine crate
  READMEs rather than six.
- **[Decision §43](docs/decisions.md) withdraws §20's DCO clause.** The ledger's own rule is
  that an overturned decision is recorded by appending a new entry that names the old one, so
  §43 records it and §20's status line points at §43. A CLA is a grant of rights (relicensing,
  patents) and is what lets an open-core project ship derived work under a commercial
  proprietary licence; a DCO is only a statement of origin, and §20's wording — "a DCO (the
  CLA already exists)" — carried the contradiction. No `Signed-off-by` check goes into CI.
- **The gate lints `cli`, `server` and `host-core` on non-Windows platforms now.** That branch
  of `scripts/gate.sh` skipped all four together — `cli`, `server`, `host-core`, `host-tauri` —
  because `host-tauri` needs webkit2gtk / gtk / librsvg there. The three crates that carry no
  Tauri are linted on every platform now, and only the two Tauri crates (`host-tauri`,
  `ui/src-tauri`) stay Windows-only. `ci.yml` did not change: `host-core`'s Linux system
  dependency is `libdbus-1-dev` (reached through `keyring`), which the gate job already
  installs. E4 is why this was overdue — its `worker` example step was the first Linux gate
  step to compile `host-core`, and it was red for four commits before anyone looked.
- **Every workspace crate is linted and checked on every platform.** The non-Windows branch is
  gone from `scripts/gate.sh` (the OS detection went with it): `host-tauri` and `ui/src-tauri`
  are linted and checked on Linux too, and `worker` — absent from every clippy list until now,
  on both platforms — joined the same command. The Linux `gate` job installs the Tauri system
  libraries (webkit2gtk / gtk / librsvg / libsoup) next to the `libdbus-1-dev` it already
  installed. No frontend-ordering change was needed: `tauri::generate_context!()` takes its dev
  branch while `custom-protocol` is off, which is the case for a plain `cargo check` /
  `cargo clippy`.
- **A gate without a guest runs every crate's unit tests.** Where the non-Windows branch ran
  `cargo test -p audit -p sandbox --lib` — 10 tests of 638 — it runs
  `cargo test --workspace --lib` now: **204** unit tests across all eight crates. They are pure
  logic (nothing boots a guest), so this needs no QEMU, and it is the first coverage `agent`,
  `host-core`, `cli`, `server` and `worker` have had off Windows. The portable **integration**
  tests still do not run there; separating them from the guest-booting ones is B-3b.
- **`agent`'s two C-compiling unit tests print a skip when no toolchain is present.** They call
  `compile_freestanding(...).expect("run gcc")`, so on the Linux job — which has no RISC-V GCC —
  turning on `cargo test --workspace --lib` failed at once. They probe
  `CompilerConfig::discover()` first and print `skip: <test> -- no RISC-V GCC found` instead;
  a machine with the toolchain still compiles for real. Both `cargo test` invocations also run
  with `--no-fail-fast`, so one failing test binary no longer hides the rest of the workspace.
- **The gate's test split is by capability now, not by platform.** A test that needs a QEMU guest
  or a RISC-V GCC carries `#[ignore = "<what it needs>; run with --include-ignored"]`, and 54 do
  (37 need a guest and a compiler, 8 a discoverable QEMU, 9 a discoverable compiler). A machine
  without them runs `cargo test --workspace --no-fail-fast` — **584** tests, where the non-Windows
  branch ran 10 before this series — and one with them runs `--include-ignored` (**643**) with
  three `--skip` flags for the tests that need an API key or write a real OS-keyring entry. Worth
  knowing before reusing those flags: `--skip` matches the **test function name**, so a file name
  never matches and a short substring can take portable tests with it.
- **Two test fixtures stopped being Windows-only.** `host-core/tests/common/mod.rs`'s tar builder
  called `tar::append_data` with a `../` entry, which that crate refuses at *write* time — so on
  Linux the fixture panicked before the test reached the installer it is about; the name is
  written into the header by hand now, and the escaping entry reaches the code under test. And
  `qemu_archive()`'s emulator body was a `#!` script: with mode 0755 a Unix host *runs* it, so the
  "emulator that cannot run" was adopted and the test failed there (on Windows a text `.exe` never
  runs, which is why it passed). It is plain non-program bytes now.

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
