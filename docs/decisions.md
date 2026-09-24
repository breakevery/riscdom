[中文](decisions.zh-CN.md) | English

# Technical decision ledger

> **Applies to the v0.x line; every entry carries its own date and status.** These decisions
> were taken in conversation and are written down here so that a new collaborator — human or
> AI — does not have to rediscover them, and so that v1.0 has something to re-open instead of
> something to reconstruct.

**Append-only.** A new decision goes at the end. The body of an existing entry is never edited;
only its status may change (a decided mechanism gaining an implementation version, or a
decision being overturned). An overturned decision is recorded by appending a new entry that
names the old one — history is not rewritten.

**Status vocabulary.** *Decided* / *Open* / *Overturned*. Dates are local (Asia/Shanghai).

**Audience.** Kernel developers and distribution integrators: each entry states the constraint
it creates and the seam it leaves for later work.

## 1. Kernel positioning

**Date**: 2026-09-20 ｜ **Status**: Decided

**Decision**: The kernel provides mechanism only, never policy.

**Why**: A generic sandbox, a built-in AI supervisor and an officially operated service are all
policy. Baking any of the three into the kernel would fix choices that belong to the caller.

**Impact**: Mechanism lives in the kernel, policy lives in the caller. No work on becoming a
general-purpose sandbox, no built-in supervisor, no officially operated service.

## 2. RISC-V positioning

**Date**: 2026-09-22 ｜ **Status**: Decided

**Decision**: RiscDom is "a pluggable secure-sandbox runtime with RISC-V at its core". RISC-V
is the primary tone, the substrate and the default implementation — not the only one.

**Why**: RISC-V's central place comes from being the project's root, not from exclusivity.

**Impact**: A sandbox can be extended by plugin to other architectures, and sandbox content is
not constrained by architecture. Kernel code may not assume the guest is RISC-V.

## 3. The sandbox is a plugin

**Date**: 2026-09-22 ｜ **Status**: Decided (mechanism); implemented in v1.x

**Decision**: The sandbox is a plugin: pluggable, and several may run in parallel.

**Why**: The sandbox is where architectures and execution models differ most; a single built-in
implementation would put every such difference inside the kernel.

**Impact**: Out of process (stdio JSON lines, isomorphic with `worker`). The mechanism layer
(start / stop / execute / output) is mandatory; the semantics layer (snapshot / fingerprint) is
optional, and the plugin declares which capabilities it has. A developer may write a plugin
within limits — the plugin's capabilities are constrained and reuse the capability model. The
plugin interface is frozen before the kernel API. v0.9 leaves the seam; v1.x implements.

## 4. Sandbox preset environments

**Date**: 2026-09-22 ｜ **Status**: Decided (mechanism); implemented in v1.x

**Decision**: A sandbox may carry preset environments, which answers "the AI rebuilds the wheel
every single time". All four mechanisms are provided; which one is used is policy.

**Why**: Presets are the difference between an AI that spends its first minutes reinstalling a
compiler and one that starts work.

**Impact**: The four mechanisms are (i) persistent directories — an instance directory plus a
shared directory, with the plugin deciding what is mounted; (ii) manifest sources — plugin
declaration, kernel scan and developer-written files, merged; (iii) installation path — a manual
shell plus a setup script; (iv) AI awareness — system-prompt injection plus a tool query. Preset
content carries a hash and is verified at start-up. v0.9 leaves the seam.

## 5. Plugin repository

**Date**: 2026-09-22 ｜ **Status**: Decided; the repositories are built in v1.x

**Decision**: An official repository and community repositories coexist, and the tool is
configured with several sources. The main repository keeps the full official index, pointing
outward.

**Why**: A single centrally-curated repository would make the project the gatekeeper of every
plugin; several sources keep the official index authoritative without making publication
depend on one server.

**Impact**: A plugin package reuses the credential spec — Ed25519 signature, metadata, content.
v0.9 only leaves the seam.

## 6. Control plane

**Date**: 2026-09-22 ｜ **Status**: Decided; landed in v0.9

**Decision**: A human supervising AIs and an AI supervising AIs go through the same interface.

**Why**: Building two control channels instead of one is the mistake this design exists to
avoid; to the kernel, an instruction from a supervisor AI and one from a human are both
authorised instructions from the control plane.

**Impact**: HTTP for commands and queries, SSE for event push (zero new dependencies,
one-directional, no WebSocket). Device-independent semantics: the same protocol carries local
IPC and a networked connection. Authentication is `Authorization: Bearer` plus the `Authn`
hook; the open-source build offers plaintext HTTP and the hook only, and TLS is the
deployment's responsibility — the boundary is stated in the documents rather than assumed.
Capability decisions are made in the request path against the route table, whose third column
is the `Capability` type itself, so a route that skips the check cannot be written.

## 7. Cross-device

**Date**: 2026-09-22 ｜ **Status**: Decided; implemented in v1.0

**Decision**: The public internet is treated as one large LAN (an overlay), with a three-layer
topology: LAN nodes > core bridge machine < WAN nodes.

**Why**: NAT and dynamic addressing are the obstacles; a stateless bridge that both sides dial
out to removes them without giving the bridge authority over anyone's data.

**Impact**: The core bridge is stateless and replaceable. Logs are fully replicated — the
group-chat model, where every device holds the whole log and cross-validates it. Discovery uses
UDP broadcast with room isolation; task dispatch uses point-to-point TCP with Ed25519
signatures. `@` means address plus signature. Rate, rooms, cost metering and `@` permissions
are mechanism in the kernel and policy in the caller.

## 8. CLI

**Date**: 2026-09-22 ｜ **Status**: Decided; early v0.9

**Decision**: Ship a CLI: one `riscdom` command with subcommands, in a hybrid architecture —
embedded locally, or `--remote host:port` to connect to a daemon.

**Why**: An AI driving the tool from a shell is a first-class user, and a shell is the one
interface every environment has.

**Impact**: AI-friendliness is a hard requirement: `--json` output, exit codes with defined
meanings, idempotence, streaming stdin/stdout. The CLI is a control-plane client: it never
touches kernel state directly.

## 9. Management program

**Date**: 2026-09-22 ｜ **Status**: Decided; implemented in v0.9

**Decision**: Mixed form — a web front end (mobile browser first) plus the desktop keeping
Tauri.

**Why**: The human who supervises AIs is often away from the machine running them, and a phone
browser reaches a Tauri-only desktop build nowhere.

**Impact**: v0.9 does not split the repository (it stays in the main one); v1.0 splits it out.
The management program is a control-plane client. It targets fixed addresses (a LAN, or a fixed
public IP); the dynamic-IP case is outside this project's scope.

## 10. API stability policy

**Date**: 2026-09-22 ｜ **Status**: Decided; frozen at v1.0

**Decision**: Freeze two layers — the `host` public API and the control-plane HTTP protocol.

**Why**: These are the two surfaces an integrator builds against; freezing anything else would
either be too narrow to matter or too wide to keep.

**Impact**: Allowed: adding optional fields, adding endpoints. Not allowed: changing an
existing field's meaning, deleting a field, changing a URL's semantics. Deprecation: an old
endpoint stays for at least one major version. semver: 0.x promises nothing stable; from 1.0
the promise is strict.

## 11. Data migration and schema evolution

**Date**: 2026-09-22 ｜ **Status**: Decided

**Decision**: Every persisted format carries `schema_version` as its first field.

**Why**: Without a version in the data itself, the only way to know how to read a file is to
know which build wrote it.

**Impact**: Opening migrates; the user is never asked to run a tool. A new version must read
old data. An old version reading new data refuses and reports — never silently degrades. A
`.bak` backup is taken before migration. Migration is not reversible.

## 12. Error model

**Date**: 2026-09-22 ｜ **Status**: Decided; extended before v1.0

**Decision**: One classification enum: `Network { timeout | unreachable }`,
`Refused { reason }`, `Crashed { exit_status }`, `Partial { completed, failed }`,
`Invalid { reason }`.

**Why**: The categories are what a caller can actually act on differently — retry, refuse,
report a crash, resume a partial, reject an invalid input.

**Impact**: Every variant carries a `Retryable` flag and a `Cause` chain, and serialises
cleanly across processes and devices.

## 13. Credentials and key management

**Date**: 2026-09-22 ｜ **Status**: Decided

**Decision**: Ed25519, in a standard format (PEM or JWK) — nothing home-grown.

**Why**: A standard primitive plus a standard encoding is the only combination that lets other
implementations verify what this one signs.

**Impact**: Private keys default to a file with mode 600, with the system keyring optional.
Rotation runs several keys in parallel — old and new are both valid during the grey period.
Revocation broadcasts a revocation list to the room and ejects a compromised node network-wide.
Bulk import is `{node_id, addresses[], public_key, capabilities, rooms[]}` as JSON.

## 14. Upgrade and migration path

**Date**: 2026-09-22 ｜ **Status**: Decided

**Decision**: In-place upgrade — the installer overwrites the installation, nothing is
reinstalled. Data migration is automatic.

**Why**: An upgrade that asks the user to reinstall or to run a migration by hand is an upgrade
that will be skipped.

**Impact**: Crossing a major version must be stepwise; a version may not be skipped, and a
migration tool is provided for the step. Documented in `docs/upgrade.md`.

## 15. Security disclosure policy

**Date**: 2026-09-22 ｜ **Status**: Decided

**Decision**: `SECURITY.md` plus GitHub's private vulnerability reporting.

**Why**: A single stated channel is what makes a report arrive privately instead of as a public
issue.

**Impact**: Timelines: 48 hours to acknowledge, 7 days to assess, 90 days to disclose
(coordinated with the reporter). The maintainer applies for a CVE. There is no bug bounty — the
project is non-profit, and the document says so plainly.

## 16. Configuration schema

**Date**: 2026-09-22 ｜ **Status**: Decided; during v0.9

**Decision**: Define the configuration as JSON Schema, in `docs/config-schema.md`.

**Why**: A machine-readable schema is what lets a validator, an editor and a generated
reference all come from one source.

**Impact**: It covers node configuration, rooms, rate limits and permission rules.

## 17. Observability contract

**Date**: 2026-09-22 ｜ **Status**: Decided; during v0.9

**Decision**: Structured logs (JSON lines), a metrics endpoint (`/metrics`, Prometheus format)
and a tracing id — the `agent_id` + `task_id` pair joined.

**Why**: The audit chain already carries an identity per event; reusing it as the tracing id
means one identifier per unit of work rather than a second namespace to correlate.

**Impact**: Logs and metrics are machine-readable by contract, not by convention.

## 18. Performance budget

**Date**: 2026-09-22 ｜ **Status**: Decided

**Decision**: VM start ≤ 2 s; dispatch round trip ≤ 100 ms on one machine and ≤ 500 ms across
the network; ten agents on one node fit in ≤ 2 GB; log growth is predictable.

**Why**: These are the numbers at which the product stops feeling like a local tool.

**Impact**: They are budgets, not measurements: a change that breaks one is a regression to be
answered for.

## 19. Backup and portability

**Date**: 2026-09-22 ｜ **Status**: Decided

**Decision**: A `riscdom-backup` CLI exports the audit store, snapshots and credentials as one
movable package.

**Why**: Being able to leave is what makes it safe to stay.

**Impact**: The package is the unit of portability; nothing outside it is required to restore a
node's history and identity.

## 20. Contributor workflow

**Date**: 2026-09-22 ｜ **Status**: Decided; the DCO clause is withdrawn (see §43)

**Decision**: CONTRIBUTING, issue and PR templates, a DCO (the CLA already exists) and review
rules.

**Why**: A contribution process that exists in writing is the difference between a patch that
can be merged and one that has to be negotiated.

**Impact**: Every contribution carries a sign-off; review rules are stated once rather than per
pull request.

## 21. Documentation layering

**Date**: 2026-09-22 ｜ **Status**: Decided

**Decision**: Five audiences: kernel developers, distribution integrators, administrators, end
users, contributors.

**Why**: A document written for everyone is read by no one; naming the audience is what makes
the level of detail a decision instead of an accident.

**Impact**: Every document is bilingual and paired with its counterpart, aimed at one audience,
with runnable examples and its applicable version marked. Each batch's deliverables include the
documentation for its audience — documentation is not written afterwards.

## 22. Open-source governance

**Date**: 2026-09-22 ｜ **Status**: Decided

**Decision**: There is no foundation today: the project is run by an individual through the
`breakevery` organisation.

**Why**: Stating the current situation plainly is what keeps a future change of form a
decision rather than a drift.

**Impact**: Neutralisation is a stated direction, not an assigned form. The licence is
Apache 2.0, in force permanently, and does not change.

## 23. Road-map constraint

**Date**: 2026-09-22 ｜ **Status**: Decided

**Decision**: The v1.0 road map contains no feature whose purpose is revenue; work on that line
starts only after v1.0 is complete.

**Why**: A pre-1.0 project that splits its attention between an unstable kernel and a product
finishes neither.

**Impact**: The v0.x line is technical work only, and no road-map item may assume a commercial
party. Where that line eventually leads is not recorded in this document.

## 24. Telemetry and legal stance

**Date**: 2026-09-22 ｜ **Status**: Decided

**Decision**: Telemetry: none is collected.

**Why**: A sandbox runtime sees source code and command lines; a telemetry channel would be the
one place where the project's own promise is easiest to break by accident.

**Impact**: Legally: Apache 2.0, plus the disclaimer and the bounds of the terms of use.
Release cadence: a minor every 3–6 months, patches as needed.

## 25. SSE or WebSocket

**Date**: 2026-09-22 ｜ **Status**: Decided

**Decision**: The control plane pushes events over SSE.

**Why**: SSE needs zero new dependencies, is one-directional (which is what event push is) and
has a native client in every browser (`EventSource`).

**Impact**: WebSocket is deferred to v1.0, and only if cross-device work turns out to need a
bidirectional stream.

## 26. Event envelope

**Date**: 2026-09-22 ｜ **Status**: Decided; landed in v0.9

**Decision**: Every event, on every transport, carries one envelope:
`{version, kind, event, agent_id, task_id, ts, payload}`, with `kind` one of `event`, `hello`
or `gap`.

**Why**: Several transports carry the same events; one envelope is what makes a client written
against one of them work against the others.

**Impact**: Version rules — adding a payload field, adding an event name or adding a `kind`
value all leave `version` alone; changing a field's meaning or type, or removing one, bumps it.
The top-level fields are frozen. An event's identity comes from the **event source**, not from
the sink that carries it.

## 27. The host split: host-core + host-tauri

**Date**: 2026-09-23 ｜ **Status**: Decided; landed in v0.9 (A1, four waves)

**Decision**: The host is two crates — `host-core` (the kernel facade: audit wiring, `AppState`,
snapshots, sessions, the download paths, the preflight, the event envelope) and `host-tauri` (the
53 Tauri commands and the `TauriEventSink` transport), with `host-tauri` re-exporting
`host-core`.

**Why**: `tauri` was linked unconditionally, so `worker` and `server` — headless processes that
never touch a webview — dragged a GUI toolkit into every build. Splitting the crates is what turns
"links Tauri" from a property of the host into a choice of the process.

**Impact**: The boundary is the question "does it need a webview", and the dependency runs one way
only: `host-tauri → host-core → {agent, sandbox, audit}`. `worker` and `server` depend on
`host-core` and name no Tauri crate (`cargo tree -p worker` / `-p server` confirm); the desktop
shell depends on `host-tauri` alone, because `host-tauri` re-exports the portable surface
(`pub use host_core::*`). The 39 integration tests live in `host-core/tests`; the mirror-constant
guard and the clippy step cover both crates. The work was four waves — core + facade, tests +
guards, worker/server, rename + shell — each green on its own, with no temporarily-red intermediate
state. The crate names are load-bearing: `host-core` says "no webview", `host-tauri` says
"webview only".

## 28. The CLI: placement, modes and exit codes

**Date**: 2026-09-23 ｜ **Status**: Decided; the skeleton and the read-only commands landed in v0.9

**Decision**: The CLI is a new `cli` crate with the `riscdom` binary. It is a **client of the
control plane**: every command goes through HTTP, and the local mode starts the control plane
inside the CLI process on `127.0.0.1:0`. Exit codes: `0` success, `1` local failure, `2` usage
or `400`, `3` refused or `5xx`, `4` `401`/`403`.

**Why**: Putting it in `server` would have made one crate both the served process and its client;
putting it in `host-tauri` would have linked Tauri into a headless tool. Since a local call and a
remote call must behave identically, the local mode has nothing to gain from reaching past HTTP
into `AppState` — and a great deal to lose, because then only the remote path would be exercised.

**Impact**: The CLI depends on `server` (for the embedded mode), `host-core`, `reqwest` and
`serde_json` — all already in the lock file; no argument-parsing crate was added, the CLI parses by
hand like `riscdom-server`, `worker` and the two `audit` binaries. `--json` passes the control
plane's JSON through unchanged, and the token is never printed or logged. The gate's clippy step
covers `-p cli` with `--no-deps`, so the crate is linted without pulling `server`'s own
(pre-existing) findings into the gate. The control commands and `--follow` are the next batch.

## 29. The control plane boxes its parameter errors

**Date**: 2026-09-23 ｜ **Status**: Decided; landed with the v0.9 CLI batch 3/N

**Decision**: `Params`'s five readers (`required`, `usize_required`, `usize_or`,
`bool_required`, `bool_or`) and `read_json_body` return `Result<_, Box<Response<RespBody>>>`
rather than `Result<_, Response<RespBody>>`.

**Why**: A `400` is answered by handing the caller a ready-made response, which is the shape that
keeps the handlers readable; but hyper's `Response` is 128+ bytes, so every `Result` carrying one
was mostly error by size (`clippy::result_large_err`). Boxing puts the large value behind a
pointer on the path that actually produces one, and costs a single allocation when an error is
built — never on the success path.

**Impact**: Callers write `return *response`, so the response that travels is the same value on
the same path: no behaviour change, only its address. The helpers are `pub` on a
`pub(crate)` type, so nothing outside the crate sees the signature. `server` is now inside the
gate's clippy step, which is what surfaced the six sites in the first place.

## 30. The two assemblies share one shape

**Date**: 2026-09-23 ｜ **Status**: Decided; landed with the v0.9 sandbox batch F1

**Decision**: "Assembling a resource" has one shape, and the toolchain and QEMU both use it:
a *spec* (`version`, `url`, `sha256`, `archive_kind`, `install_subdir`) → a *download* in
chunks with a cancel flag → a *sha256 check* → a *Zip-Slip-guarded extraction* → an
*adoption* step that validates the result and writes it into `settings.json` → *audit events*
(`host.<resource>.download.start|done|failed|cancelled`) → an *SSE family*
(`<resource>:download`, the internally tagged enum under the tag `state`) → one *slot* per
resource in `AppState` (`begin` / `status` / `cancel` / `finish`).

**Why**: The two resources are two instances of one problem — a pinned, integrity-checked
binary that has to be fetched, verified, installed and made current — and a client should read
one vocabulary for both. Sharing the shape is also what makes "sandbox = kernel + toolchain +
QEMU" (F2) a matter of naming resources rather than of inventing a mechanism per resource.

**Impact**: The QEMU download's payload tag moved from `kind` to `state` and the CLI's `--wait`
terminal test became one function for both families, so an existing client of one family
already speaks the other. **The shape is shared, not abstracted**: there is no generic
`Resource` trait or spec type, and F1 deliberately did not add one — the two modules stay
separate files with separate types, and the duplication is the documented cost of not
guessing at the abstraction before F2 names what it is. Nothing about the toolchain's
semantics changed. `spec_for_current_platform` remains the platform branch for QEMU, and it
refuses on every platform today (`docs/qemu-distribution.md` §5): the shared shape means
pinning a release later is a data change in one table.

## 31. The sandbox registry: stored definitions, a scanned list, one merged view

**Date**: 2026-09-23 ｜ **Status**: Decided; landed with the v0.9 sandbox batch F2a-1

**Decision**: A sandbox has one *definition*, and the definition is **stored data, not a
runtime**: `{ name, display_name?, memory_mb?, qemu_exe?, toolchain_path?, kernel?, notes? }`,
written by hand in `settings.json`. What the host *serves* is that plus the three things only
it can answer at the moment of the question — `source` (`manual` / `discovered`), `runnable`
and `shadowed` — and **neither `runnable` nor `shadowed` is ever stored**. The registry is
assembled on every read from three sources in a fixed order — hand-written definitions, what
the scan found, one built-in fallback named `default` — and a name collision is resolved in
favour of the hand-written entry **while the shadowed entry stays in the list, marked**.

**Why**: The three questions a client asks about a sandbox — what is installed here, what is
written down, and what can run now — have three different lifetimes. Installed resources
change without anyone editing a file; `runnable` changes when a QEMU is uninstalled, and
storing it would make a definition a lie the moment the machine changes; a merge that hid the
losing entry would make a shadowed scan invisible, which is exactly the case a person needs to
see when their hand-written definition is not the one being used. Writing the scan back was
rejected for the same reason: it would turn a cache of the machine into settings a person is
supposed to own, and make two hosts in one process overwrite each other's view.

**Impact**: `LocalSettings` gains `sandboxes` and `default_sandbox`, both additive
(`#[serde(default)]`), so no migration runs and `SETTINGS_VERSION` stays at 1. The scan is
bounded to the host's own data directory (`<data-dir>/toolchain/*`, `<data-dir>/qemu/*`) plus
the machine's QEMU, reusing the downloaders' `find_compiler` / `find_qemu` (now `pub(crate)`)
rather than keeping a second notion of "installed". `sandbox.read` is the 29th capability; the
endpoints, Tauri commands and CLI are F2a-2, switching is F2b, approval is F2c and
`Task.sandbox` is F2d. **The definition is deliberately not the runtime configuration**: it
names resources, and what a run does with them remains F2b's decision.

## 32. The QEMU assembly spec is empty on purpose (supply-chain safety)

**Date**: 2026-09-23 ｜ **Status**: Decided; recorded while landing the v0.9 sandbox batch F1

**Decision**: `spec_for_current_platform()` refuses on **every** platform: RiscDom pins no
QEMU release, ships no URL and no digest, and never fetches an emulator. `POST
/v0/qemu/download` answers `503 unavailable` with `cause: "qemu"` and the install guidance,
and claims no download slot. The toolchain's assembly is unaffected: its release is pinned,
hashed and fetched.

**Why**: Upstream QEMU publishes no Windows binary, so any URL RiscDom chose would be either
a third party's repackaging or a release we cannot verify. A digest invented for a binary we
did not fetch is not verification, it is an integrity hole with a hash in front of it — the
exact failure mode the pinned toolchain exists to avoid. Guiding the user to an emulator they
install themselves keeps the trust boundary where it can actually be checked.

**Impact**: The QEMU download path is complete behind the refusal (slot, status, cancel,
adoption, audit events, the `qemu:download` event family, the Tauri commands, the endpoints
and the CLI), so **pinning a release later is a data change**: the answer becomes `202` and
nothing else moves. One asymmetry is left standing and reported rather than fixed: the spec
being empty is the reason `qemu.read` / `qemu.configure` endpoints can report "not available"
without a network call, which is what makes them testable against a loopback fixture offline.

## 33. Centralised connection and the temporary centre

**Date**: 2026-09-23 ｜ **Status**: Decided; implemented in v1.0 (cross-device)

**Decision**: A centre is a RiscDom node with a different **role** — the same kernel,
differentiated by deployment. The rules: legitimacy comes from a reserved priority order;
the priority is issued by the centre at registration and every node keeps a copy; a
disagreement about what a node knows is settled by majority; the trigger is **every** node
failing to reach the centre (a global confirmation, not one node's); when the events are
merged, the temporary centre's chain folds into the main one; the authority is AI-initiated
with an after-the-fact audit; ending the period means returning to the centre; a bridge
machine stays inside its own network and can stand in for the centre temporarily; the kernel
interface is "the node interface plus an optional centre capability", and the deployer
decides.

**Suppression (all three layers are required)**: first, a waiting period (30 s – 2 min of
silent retries); second, global confirmation (most nodes must report "cannot reach the
centre" before anything fires); third, backoff plus precedence (the first in line waits a
random backoff and stands down the moment it sees a takeover broadcast).

**Audit merge (option A)**: events written during a temporary centre carry `provisional:
true`; when the real centre returns, a conflict-free run is merged and the mark cleared,
while a conflicting one keeps **both** sides marked `fork` — never a silent merge. The
chain's semantics extend to "main chain + temporary segments", where the cross-segment
reference at a segment's head **adds metadata and does not change the hash formula**.

**Impact**: one code base, differentiated by deployment (the cell-differentiation model);
the commercial edition's multi-centre redundancy is not part of this model.

**Pending authorisation**: the cross-device design of v1.0 must be approved on its own —
extending the audit chain to "main chain + temporary segments" touches the boundary of red
line 5 (`When in doubt, ask first`, PROJECT_CONSTITUTION.md §8).

## 34. What a node runs is runtime state; what it starts from is configuration

**Date**: 2026-09-23 ｜ **Status**: Decided; landed with the v0.9 sandbox batch F2b-1

**Decision**: Two names, two lifetimes. `default_sandbox` lives in `settings.json` and is
what a node **starts from** after a restart; `current_sandbox` lives in memory only and is
what a node is **running now** — set by a successful switch, and `None` until one happens.
`AppState::sandbox_default_name()` answers the first and `AppState::current_sandbox()` the
second, and a switch writes only the second.

**Why**: They answer different questions at different times. A person who switches to a
scratch sandbox for one experiment has not asked for that to become the configuration of the
machine; and a configuration edited on disk is not the same claim as "the VM in the slot came
from this definition" — the slot can be empty, the VM can be stopped, and the machine can be
restarted. Collapsing the two would make every switch a settings write (and every settings
edit a claim about what is running), which is the confusion F2a's registry exists to avoid.

**Impact**: A switch changes the runtime field only; `settings.json` is left byte-identical
by it, which F2b-1's tests assert. The registry's `current` and `default` are therefore
genuinely different values, and a client asking "what would a run use" has to say which of
the two it means. The endpoints, the Tauri commands, the CLI and the `sandbox:switch` event
that expose all this are F2b-2; `Task.sandbox` (F2d) is the third question — what one *run*
asks for — and it sits on top of both.

## 35. A port lease promises distinct *live* leases, not never-reused numbers

**Date**: 2026-09-23 ｜ **Status**: Decided; landed with the v0.9 relay-fix batches 1/N–2/N

**Decision**: `relay::lease_local_ports` guarantees two things and no more. A number is
reserved for as long as its lease is alive (and a bound listener holds it at the OS level
until `hand_off`), so **two leases that exist at the same time never carry the same number**;
and a lease's release returns its number to the pool, from which the OS may hand it out
again — to this process or another. "A number is never handed out twice while the process
lives" was considered and **rejected**: it would need a quarantine of released numbers that
grows without bound, and it would change a contract no caller needs, because the callers
already retry around the window that cannot be closed (the peer binds only after we let go).

**Why**: The window that matters is the one between *our* release and *the peer's* bind, and
no in-process registry can close it — QEMU cannot be given a pre-bound socket with today's
flags. So the registry's job is narrower than it looks: keep two parts of this program from
being handed the same port at once, and hold the port at the OS level until the last possible
moment. Stating that exactly is what makes the test writable: the assertion that failed twice
(`concurrent_leases_never_repeat_a_port`) recorded every port ever leased in the run, so a
number a finished thread had released and the OS handed out again read as "two holders at
once" — a claim the library never made.

**Impact**: The test parks every thread's leases until all have leased and compares them then
(the invariant the code keeps), and a second test pins the other half (a dropped lease leaves
its port bindable). The release path removes **its own** number once (`HashSet::remove`) so a
release can never take another holder's reservation with it, and the registry is a
`LazyLock<Mutex<HashSet<u16>>>` because `HashSet::new` cannot initialise a `static`. No public
signature changed and no call site changed: `leased_ports()` keeps returning `Vec<u16>` (its
order was never a contract), and `agent` / `host-core` / `sandbox::vm` are untouched.
`sandbox/README.md` states the contract for callers, and `port_race.rs` (ignored) keeps
walking the inter-process window as a stress test.

## 36. A sandbox request is an ask; the decision is another actor's, and it executes nothing

**Date**: 2026-09-23 ｜ **Status**: Decided; landed with the v0.9 sandbox F2c batch

**Decision**: Asking for a sandbox change and making one are **two surfaces**. `POST
/v0/sandboxes/switch` still changes the node and still needs `sandbox.switch`;
`POST /v0/sandboxes/requests` leaves a request instead, and needs only `agent.run` — the
actor that may run an agent may say what it wants. Deciding a request (`approve` / `reject`)
first needs `sandbox.read`, because a decider has to be able to see the queue, and then the
capability the request's own `action` implies: `sandbox.switch` for a `switch`, and
**`sandbox.assemble`** (the 31st capability) for `define` / `assemble`. Moving a node and
giving it a new definition to run are different powers, and the vocabulary now says so.

**Why**: The alternative — a route table that names one capability per route — cannot
express "whichever this request asks for": the table is a static column checked before the
handler runs, and the action is only known once the request is resolved. So the two
decisions declare `sandbox.read` as their gate and the handler checks the precise capability
against the actor, which is why `dispatch` now receives the actor. The consequence is
recorded rather than hidden: the document's "every capability in the vocabulary has at least
one route" became "is enforced somewhere", because `sandbox.assemble` has no route of its
own until the assemble endpoint lands — it is enforced inside that handler, where a decision
knows what it is deciding.

**Impact**: `AppState` grew a request queue (`SandboxRequests`: a `Vec<SandboxRequest>`behind a `Mutex`, cloned into the loop's tool gateway, plus the `Arc` on `current_sandbox`
so the two views cannot drift), ids are `req-<pid>-<seq>` in their own namespace, and the
record is `{id, requester_agent_id, action, sandbox, definition, reason, requested_at_ms,
status, decided_by, decided_at_ms}`. **Approving executes nothing** (the switch remains a
second, authorised call), a decision is not reversible (`409`), an unknown id is `404`, and
there is **no TTL**: `expired` is reserved and nothing produces it — a queue of asks is not a
high-risk resource, and a sweeper would be a new mechanism for no caller's benefit. The
loop reaches the queue through `agent::SandboxRequester`, a trait the `agent` crate owns
because it cannot name `AppState`, implemented by the host over cloned sub-handles (the loop
lives inside `AppState`, so an `Arc<AppState>` would be a cycle). `sandbox:request` is the
14th event: one frame per change, `{id, status, requester, action}`.

## 37. A project leaves and enters as one file; the AI's writes are on the chain

**Date**: 2026-09-23 ｜ **Status**: Decided; the v0.9 endpoints landed, the git integration (B) is v1.0

**Decision**: "Take the project with you" is **two HTTP endpoints**, not a mount and not a
repository. `POST /v0/workspace/export` answers the workspace as a `tar.gz`; `POST
/v0/workspace/import` takes an archive as its body (zip, tar.gz or tar). A **git
integration is B, and B is v1.0**: a project that leaves as an archive is something every
host can already do, while a repository brings a second source of truth and a second
credential story with it. A **bind mount is C, and C is not done**: sharing a host
directory into the sandbox is exactly the boundary the sandbox exists to draw. One
workspace per node for v0.9 (multi-project is v1.0 with B).

**Why**: The archive is the format a shell already speaks (`tar czf`, `unzip`), so a
project can leave this tool and enter it again without this tool deciding anything about
how a project is stored. The alternative — writing the archive server-side and answering a
path, the way the audit exports do — would be the wrong shape here: those exports write
**into** the workspace because the file is a product of the run, while a project export is
the workspace being taken away, and a client on another machine cannot read a path.

**Impact**: `host-core/src/workspace_io.rs` owns both directions and every guard: no entry
may escape the destination, no symlink or hard link is followed, and `.riscdom/` — the
host's own state (the audit DB, snapshots, the preflight cache) — is neither packed nor
unpacked. **Containment only, no extension allow-list**, for the same reason the existing
exports use it: `WorkspacePolicy::check_write` allows four source extensions, and a
project is not made of source files alone. Import has its **own 64 MiB ceiling** (`413`)
rather than raising the shared 64 KiB `MAX_BODY_BYTES` every JSON body uses; the body is
read as bytes, the first non-JSON request on the surface; and an existing file is a `409`
unless `?force=true`, because replacing what is in a workspace is a decision, not a
default. The capabilities split by direction: **`workspace.write`** (the 32nd) to import,
`workspace.read` to export — reading a project and replacing it are not the same
permission. The packers (`zip`, `flate2` + `tar`) were already in `Cargo.lock` and are now
declared for **every** platform: a project archive is whatever the user's tooling
produced, so a Windows host must read a `.tar.gz` and a unix host a `.zip`. Finally, the
AI's own writes are visible: `write_source` records **`agent.file.write`** `{path,
bytes}` on the audit chain. That is an audit event, not an SSE one — the event count stays
14 — because it belongs where the provenance is provable, and it exists so that "which
files did the model write" is a row rather than a re-parse of `agent.tool.call`'s
arguments, which are truncated at 4 KiB.

## 38. A task declares its sandbox; only a switch changes the node

**Date**: 2026-09-23 ｜ **Status**: Decided; landed with the v0.9 sandbox F2d batch

**Decision**: A run may **declare** which sandbox it uses — `Task.sandbox`, the `sandbox`
field of `POST /v0/agent/run`, the Tauri command's new parameter — and the declaration
reaches the VM the run starts (toolchain, QEMU executable, guest memory). It **never moves
the node**: `current_sandbox` is unchanged, and moving it stays `POST /v0/sandboxes/switch`,
which needs `sandbox.switch`. Two refusals guard it: a name nobody has is a `404`
(`cause: "name"`), and a name that is not what the **running** VM came from is a `409`
(`cause: "sandbox"`). Resolution order for a run: the declaration, else `current_sandbox`,
else the configured default, else the built-in fallback (host discovery).

**Why**: A VM cannot be replaced from inside a run: one `vm_slot` holds it, its QMP and
serial ports are handed to it, and the agent's tools operate on it. So a task-level choice
is a choice of *starting parameters*, and the alternative readings are worse than a
refusal — silently running against the wrong guest, or turning "run a task" into a node
change that needs a capability the caller may not hold (which is exactly what F2b/F2c
separated). The same reasoning fixes the kernel question (F2d decision 2): `start_vm`'s
`elf_path` is what a **run** boots, `def.kernel` is what a **switch** boots; mixing them
would make a definition silently override the model's choice.

**Impact**: `Task` gained `sandbox: Option<String>` with `#[serde(default)]` (an older
supervisor's task line still parses) and `Task::with_sandbox`; the worker already read a
whole `Task`, so the cross-process protocol change is that one field. `run_agent` keeps its
signature and delegates to the new `run_agent_for(emitter, input, sandbox)`, so the direct
call sites did not move; the three surfaces that can declare (HTTP, Tauri, worker) pass it
through. The host needed a fact it did not have: `current_sandbox` is written only by a
successful switch, so a VM started by the `start_vm` *tool* had **no recorded provenance**
— which is what the conflict check reads. `AppState::active_sandbox` now records it: written
when a run's VM appears in the slot (the host's only view of a tool-started VM), by
`switch_sandbox`, and on a snapshot restore (as the node's own sandbox, the honest answer
for a guest restored from this node); cleared by `stop_current_vm`. The refusal ladder for a
run moved into one function in this order — declaration, conflict, readiness (a new typed
`HostError::NotConfigured`, so the route stops checking readiness twice) — which means a bad
*parameter* is answered before the environment: `POST /v0/agent/run` with an unknown sandbox
is the caller's `404` even with no model configured. A definition's `memory_mb` reaches the
VM through a new `agent.set_memory_mb` (`VM_MEMORY_MB` was a hard-coded 128; the `VMConfig`
shape is unchanged). The resolution reads the registry's merged view, so a task naming a
hand-written definition gets it exactly as a switch would.

## 39. The task endpoint: synchronous, configuration-driven, and declared `agent.run`

**Date**: 2026-09-23 ｜ **Status**: Decided; landed with the v0.9 interface E0 batch

**Decision**: `Dispatcher` had been reachable in-process since v0.8 and reachable from
**nowhere else**. This batch makes it reachable over HTTP and settles three things. **(1) The
executor registry ships with the endpoint**: a `/v0/tasks` that could only reach this node
would be an alias of `/v0/agent/run`, so the fleet comes from `executors` in `settings.json`
(label + program + args), with **no `env`** and **no runtime registration endpoint**. **(2)
Synchronous**: `POST /v0/tasks` answers with the executor's `TaskOutcome`; there is no task
table and no `GET /v0/tasks/{id}`. **(3) No capability was added**: both routes declare
`agent.run`.

**Why**: (1) The endpoint's entire value is routing by target to **another** executor; with
only this node in the vector, the difference from "run here" is too small to explain to
anyone (routing by target, a `TaskOutcome` instead of an `AgentOutcomeView`, one extra
`NoSuchAgent`). The fleet has to come from somewhere, and **configuration** is where "what is
this node when you are handed it" already lives: not a runtime write path that changes the
node's behaviour (a privilege question for another batch), but a file a human can open, read
and edit. Not exposing `env` is this repository's red line: a settings file is where a
secret would end up — and the handle's `env` builders remain for code that is entitled to
decide an executor's environment. (2) `/v0/agent/run` is already synchronous, and work that
compiles and boots a guest across processes is not short by nature: forcing asynchrony in
would need a task table the repository does not have anywhere (the reconnaissance's safety
valve 3), in exchange for a polling shape nobody asked for. A task whose run *failed* is
still an `Ok(TaskOutcome)` — its `outcome` says `failed` — so failure needs no `500`. (3) The
`agent` capability vocabulary is 32 entries and `server/src/auth.rs` **hard-asserts**
`Capability::ALL.len() == 32`; more fundamentally, dispatching a task *is* causing an agent
to run, which is what `agent.run` has always meant (F2c used the same reasoning: the actor
who may run an agent is the actor who may say what it wants). Telling a target nobody owns
apart is a **parameter** question, not a permission one.

**Impact**: `LocalSettings` gained `executors: Vec<ExecutorSpecSettings>` (`#[serde(default)]`,
`SETTINGS_VERSION` unmoved — the same additive shape as `sandboxes`), and `load` drops an
entry whose label or program is blank (listing a fleet with an unreachable target would
promise something that cannot answer). `ExecutorSpecSettings` is a **host-core-local** type:
`worker::ExecutorSpec` cannot be reused, because the dependency runs `worker → host-core` and
the other direction is a cycle. `AppState` gained
`executors: Mutex<Vec<Arc<dyn AgentHandle>>>`, filled by `register_executors` after
`load_settings` in all three constructors — **pure data, no process spawned**
(`StdioExecutorHandle::new` only records what to run; the child appears in `run`), so the
safety valve "registration spawns at construction" did not trigger and whether registration is
eager or lazy makes no difference. The node is **not** registered: its own handle is still
built by `local_dispatcher`, the seam left for "this node as an executor" — and while the
(unused) `HostAgentHandle` holds an `Arc<AppState>`, putting it into `AppState` would close a
self-referential cycle, so leaving the node out is both the right answer and the one that
avoids it. `AppState::dispatch_task` mints a `TaskId` when the caller sent none and goes
through the same `LocalDispatcher` via `dispatch_task_value`; two new `HostError` variants
(`NoSuchExecutor`, `TaskFailed`) let the route tell the caller's parameter apart from a broken
host, and `task_error` maps them to `404 cause "target"` / `500 cause "task"`. Each surface
gained a Tauri command (registered, not wired) and a CLI subcommand.

## 40. Tool schemas are documents with one owner each

**Date**: 2026-09-23 ｜ **Status**: Decided; landed with the v0.9 interface E2 batch

**Decision**: The interface's vocabulary is published as two documents, and each half of
the truth has exactly one checker. `docs/tool-schema-executor.md` carries the eight tools
`tools_json()` builds, as the array an executor's model is offered;
`docs/tool-schema-control-plane.md` writes every endpoint as an OpenAI-style function
definition (75 tools: 32 queries, 36 controls, 3 host-local, 4 path-parameter routes), named
by a documented derivation from the path. The checks, in order of what each can see:
`agent/tests/tool_schema_doc.rs` compares the executor document with `tools_json()`;
`server/src/routes.rs`'s tests compare the control plane's marked route tables with `ROUTES`
+ `LOCAL_ROUTES` + `resolve`; `scripts/check-tool-schema.mjs` (one gate step) owns what
neither can see — the translation carries byte-identical marked blocks, every table name is
the document's own derivation, and every name has both a row and a definition.
`agent/README.md`'s table becomes an index pointing at the schema document.

**Why**: A tool schema is a contract with a model, not prose about one: an executor's model
decides what to do from those descriptions, and a supervisor's model decides what to ask for
from those definitions. Both are hand-written — the executor's because `tool_specs()` builds
its `parameters` from `serde_json::json!` literals at call time, the control plane's because
there is no machine-readable parameter source at all — so both can drift. Splitting the guard
by **visibility** rather than by document is what makes it honest: Rust is the only thing that
can read `tools_json()` and `ROUTES`, and a Node script is the only thing that can compare a
document with its translation without compiling. Neither half restates the other's job, so a
failure names one cause. The alternative — one generated file — was rejected because the
control plane's descriptions and arguments are *authored*, and generating the surrounding text
would have meant inventing a parameter source the server does not have.

**Impact**: Four new documents (two pairs), two new checks, one gate step, and a smaller
`agent/README.md`. What is deliberately **not** machine-checked, and is said so in both
documents: the `description` and `arguments` columns and the `parameters` objects of the
control-plane document. The normative tables remain the API document's §5. The naming
derivation is enforced by the script (a table row whose name is not the derivation fails),
and the six `_post` suffixes plus the four verb-named routes are the only exceptions — they
are listed in the document and hard-coded in the script, so a fifth exception has to be
decided rather than discovered.

## 41. The reference supervisor is Python, stdlib-only, over HTTP

**Date**: 2026-09-23 ｜ **Status**: Decided; landed with the v0.9 interface E3 batch

**Decision**: The external half of the interface gets a runnable reference implementation at
`examples/python/dispatch.py`: a process outside the kernel, with no model of its own, that
drives a node through the control plane over HTTP — `GET /v0/executors`, `POST /v0/tasks`
(one task at a time, synchronously) and `GET /v0/events` under `--follow`. It is **standard
library only** (`urllib.request`, `json`, `argparse`, a hand-rolled SSE reader); the token
comes from `--token-file` or `$RISCDOM_TOKEN` and never from an argument; exit codes follow
the CLI's convention (`0` / `1` / `2` / `3`); and a `--self-test` runs the real dispatch path
against a stdlib `http.server` fake control plane, so it proves itself offline. The gate runs
that self-test when a Python interpreter is on `PATH`, and prints a skip when one is not.

**Why**: The reconnaissance found four Rust examples and nothing reusable in Python, so
"write a supervisor" started from a blank page. A reference implementation is read, not just
run: it has to show the *shape* — three endpoints, one task in and one outcome out, the four
fates of a dispatch — without burying it under a client library. `requests` and `httpx` were
rejected for that reason (they would teach a dependency the supervisor does not need, and
would hide the fact that the wire format is the interface), and so was talking to a `worker`
over stdio (that is `worker/examples/dispatch.rs`'s job, and it needs no node at all). The
self-test is the part that makes the example honest: an unverifiable example rots, and this
one cannot rot quietly while it is a gate step. Python rather than a fifth Rust example
because the audience is whoever writes the supervisor — likely not a Rust developer — and
because stdlib Python is the shortest true illustration.

**Impact**: `examples/python/` (script + a bilingual README, so the documentation gate
applies to it like everything else), one new `scripts/gate.sh` step with a printed skip, one
section in the client guide (§8) and this entry. Known limits, stated in the README rather
than hidden: tasks are sent one at a time (a real supervisor would overlap them, and a queue
would hide the contract); `--follow` cannot attribute a frame to a task, because the
envelope's `task_id` is `null` for host events — attribution lives in the audit chain; and
the node must already have `executors` configured, because the fleet is configuration (E0).
`worker/examples/dispatch.rs` is untouched: it is the other half of the picture, and the
README tabulates the difference.

## 42. The remote executor is an example, and its transport is hand-written

**Date**: 2026-09-23 ｜ **Status**: Decided; landed with the v0.9 interface E4 batch

**Decision**: The seam `agent::AgentHandle` left open since v0.8 (*"a remote implementation …
implements exactly this trait. **None is written yet**"*) is filled by an **example**,
`worker/examples/remote_executor.rs`, not by production code. `HttpExecutorHandle` holds a
local label, the node's base URL, an optional bearer token and the remote target; its `run`
POSTs a task-shaped body to `POST /v0/tasks` and returns the `TaskOutcome` the remote
executor produced, with the **node's** identity and never its own label. Registration is the
single line the seam promised — `LocalDispatcher::new(vec![Arc::new(handle) as Arc<dyn
AgentHandle>])` — and **no crate in the workspace changed**. The transport is HTTP written by
hand over `std::net::TcpStream`; the self-test runs against a stand-in node on loopback.

**Why**: (1) **The seam is the deliverable.** The trait was designed to be implementable from
outside, so the proof is an implementation that touches nothing — if filling it had needed a
change to `agent` or `host-core`, the seam would have been wrong and that would have been the
finding. (2) **An example, not a shipped handle.** A production `HttpExecutorHandle` would
need a policy for tokens, TLS, retries and identity that v0.9 has not decided; an example can
demonstrate the shape and say so in its README. (3) **No new dependency.** `worker` depends on
`host-core`, `agent` and `serde_json` only; `reqwest` 0.12 is in the lock because `cli` uses
it, but adding it here would make a reference implementation hide the very wire it is meant to
show. The request is written by hand with the technique already in the repository
(`server/tests/smoke.rs`, `host-core/tests/common/mod.rs`). (4) **The endpoint is
`POST /v0/tasks`, not `/v0/agent/run`.** The latter runs on the node itself and answers an
`AgentOutcomeView`; only the former routes to an executor the far node *owns* and answers a
`TaskOutcome` — the same contract `StdioExecutorHandle` gets from a child process, which is
what makes this a real executor rather than a shape demo. (5) **Two names, like the stdio
handle.** The local dispatcher routes on the handle's `agent_id`; the far node routes on a
label it knows, so the body carries that as `target`. Sending the local label would ask for an
executor the far node may not own.

**Impact**: One new example, one new gate step (`cargo run -q -p worker --example
remote_executor -- --self-test`, alongside the Python one), a section in `worker/README.md`,
§9 of the client guide (which renumbers "what is not there yet" to §10) and this entry. The
mapping is deliberate and documented in the code: a `404` from the far node is
`DispatchError::NoSuchAgent` (the *far* fleet is what is missing — a routing fact), everything
else is `Failed`, and an answer naming a different task is a protocol break rather than a
result, exactly as the stdio handle treats it. Known gaps, stated in the README rather than
hidden: it is loopback HTTP, not a cross-device story — a real two-machine handle needs
mutual authentication and a story for what a token authorises on the far side, which is v1.0
work; and the example is proven against a stand-in node, because the real one lives in the
`server` crate, which `worker` deliberately does not depend on (the server's own tests own
`POST /v0/tasks`).

## 43. The DCO clause of §20 is withdrawn; the CLA covers its purpose

**Date**: 2026-09-24 ｜ **Status**: Decided

**Decision**: §20's "a DCO" is withdrawn. Contributions are covered by the CLA alone — the
signature `.github/workflows/cla.yml` records, exactly as `CONTRIBUTING.md` describes it. No
`Signed-off-by` trailer is required, and no DCO check is wired into CI.

**Why**: The two instruments do not do the same job, and the stronger one is already in place. A
CLA is a *grant of rights* (relicensing, patents) — what lets an open-core project distribute
derived work under a commercial proprietary licence, so it is mandatory here. A DCO is a
*statement of origin* (`Signed-off-by`) and is the weaker of the two. §20's own wording carried
the contradiction — "a DCO (the CLA already exists)". Enforcing a DCO is not free either: it is
another rule on every pull request, and it would put a `Signed-off-by` check into CI.

**Impact**: `.github/` carries no DCO check. The issue forms and the pull-request template added
in the same batch ask for the CLA and for the gate, not for a sign-off. If a DCO is ever wanted
on top of the CLA, it is a new entry here and a new CI step — not a rewrite of §20.

## 44. A test that needs a guest or a toolchain says so in its `#[ignore]` marker

**Date**: 2026-09-24 ｜ **Status**: Decided

**Decision**: A test whose environment prerequisite the CI runner does not have carries
`#[ignore = "<what it needs>; run with --include-ignored"]`, one of three markers: `requires a QEMU
guest and a RISC-V GCC` (it compiles a guest and boots it), `requires a discoverable QEMU` (it
builds a VM handle or probes discovery without booting) or `requires a discoverable RISC-V GCC`.
The gate then splits by **capability**, not by platform: without the tools it runs
`cargo test --workspace --no-fail-fast` (the markers keep the dependent tests out), with them
`cargo test --no-fail-fast -- --include-ignored` plus a `--skip` flag for each test that needs a
`DEEPSEEK_API_KEY` or writes a real OS-keyring entry. A test that needs none of that stays
unmarked and runs everywhere.

**Why**: The split used to be by platform — non-Windows ran
`cargo test -p audit -p sandbox --lib`, 10 tests of 638 — so everything below it silently lost its
coverage off Windows, and the loss was only noticed when a Linux step finally compiled `host-core`
and went red (E4). The prerequisite is a capability, not an OS. The marker's reason field is the
only place a requirement can live, because Rust has no test tags — and `--skip` filters on the
**test name**, which for an integration test is the function name: a file name never matches, and
a short substring can take portable tests with it (`--skip keyring` would have taken three, two of
them portable).

**Impact**: 50 tests carry a marker (37 + 8 + 5), the `tests/` ignores went from 8 to 58, and
`scripts/gate.sh` has no per-platform test branch left. Without the tools `cargo test --workspace`
runs 588 tests where it ran 10; with them `--include-ignored` runs 643. A test that quietly needs a
tool now fails on the runner instead of being skipped — the three markers exist so the
prerequisite is **named** rather than inferred.

## 45. Simulating a missing prerequisite must cover every discovery path

**Date**: 2026-09-24 ｜ **Status**: Decided

**Decision**: Deciding whether a test needs an environment prerequisite (a QEMU guest, a RISC-V
GCC, the network, the OS keyring) by *simulating its absence* has to cover **every** discovery
path the code under test can take, not only the explicit one. For the toolchain that means four:
`CompilerConfig::discover()` (an explicit probe that fails with `Err`), `CompilerConfig::from_env()`
(which **falls back to a bare executable name**, and a bare name is resolved against `PATH`), the
environment variables (`RISCDOM_*` / `QEMU_SYSTEM_*`) and `PATH` itself. The simulation therefore
points the variables at a path that does not exist **and** removes the known toolchain / QEMU
directories from `PATH` (`.cowork-temp/run-noguest.ps1`).

**Why**: The first simulation for §44 only pointed the `RISCDOM_*` variables at non-existent files,
which stops `discover()` — but not `from_env()`, whose fallback is a bare name and whose `PATH` on
a developer machine really holds the toolchain. Four tests were therefore never marked, and the
batch that added the markers went red in CI on targets that had looked green locally. The hole is
systematic rather than accidental: any discovery path added later reopens it.

**Impact**: A future toolchain, sandbox or network probe has to be covered path by path, and
`.cowork-temp/run-noguest.ps1` grows with each one. It is also the second root cause this series
has recorded for "the local gate is green and CI is red": the first was a missing system package
(E4's `libdbus-1-dev`), this one is an environment **capability**.

## 46. The second language is chosen by the source extension, not by a toolchain map

**Date**: 2026-09-24 ｜ **Status**: Decided

**Decision**: v0.9 F3a gives the sandbox a second language, Zig, and the language is picked by
the **source file extension** — `.c` / `.h` / `.S` / `.s` go to GCC, `.zig` goes to
`zig build-exe -target riscv64-freestanding`. `compile_freestanding` keeps its signature, and
the Zig compiler rides inside `CompilerConfig` as a second value (`CompilerConfig.zig:
ZigConfig`), so `toolchain_path` and `zig_path` are **two independent single values**, not a
map. The generated `link.ld` is shared verbatim (it names no compiler), and nothing is injected
for Zig: the source writes its own `_start`, because the `-bios none` guest jumps to the load
address rather than to the ELF entry point, so the startup code has to be first — which is what
the `.text.start` section is for.

**Why**: §9 of the architecture already reserved `toolchain` becoming a map for the day several
languages exist, and this batch deliberately did **not** take that step: a map moves the
fingerprint schema v1 → v2, which is a separate decision about history and diffs. Two sibling
single values cost nothing today and stay additive later — the map's `{c: …}` entry is exactly
`toolchain_path` under another name.

**Why the extension rather than a parameter**: the model writes a file and then names the file,
so the extension is already in the request; a separate "language" argument would be a second
thing to keep in sync with the source. It also leaves the C path untouched — no branch of the
old code moved.

**Separated out (F3a-download)**: downloading a Zig archive is **not** in this batch. Zig's
macOS/Linux builds are `.tar.xz`, and `toolchain_download::ArchiveKind` knows only `Zip` and
`TarGz`, so unpacking one needs a new archive kind plus an xz decoder; the product locator is
GCC-shaped as well (`find_compiler` matches `agent::GCC_NAMES`, while Zig ships `zig` /
`zig.exe`). Either is more than the "fill in a spec" the downloader is built for, so it is its
own batch — and it is the same batch Rust needs, which is why it comes before F3b.

**Impact**: `write_source` admits `.zig` (`Policy.allowed_extensions`), `settings.json` gained
`zig_path`, and both the tool-schema document and `agent/README.md` name the two languages.
Rust (F3b) reuses the same dispatch point; Python stays out of v0.9 (it needs a Linux sandbox,
which is v1.x).

## 47. The MVP-era Zig ban is lifted

**Date**: 2026-09-24 ｜ **Status**: Decided

**Decision**: `PROJECT_CONSTITUTION.md` forbade Zig in **three** places — §3.6 (inside the
"non-negotiable" list), §4.6 (the agent layer) and §5 (Language limits) — and recorded it once
more in §9's v0.1 checklist. From v0.9 F3a **Zig is allowed**; C++, Rust and Python stay
forbidden (Rust waits for F3b, Python for a Linux sandbox, v1.x). The original sentences are
**kept**, each of the three carrying an annotation that the time condition it states has passed.
§9 is not touched: it is a historical checklist, and v0.1 really did support only C.

**Why**: Zig brings its own cross compiler (no sysroot, no external linker) and emits a
bare-metal ELF for `riscv64-freestanding` at load address `0x80000000` — the same shape as the C
path, so the sandbox itself did not have to change (§46). The MVP-era ban existed to keep the MVP
small, not as a long-term language policy. Decisively, the clauses forbid Zig **"During MVP"**:
MVP ended at v0.8.0, so the qualifier they carry no longer holds. Lifting Zig is therefore **not
a negotiation of a non-negotiable principle** — it is reading the clause's own time condition
honestly. Nothing in the "non-negotiable" list is weakened, and its heading is untouched.

**Impact**: §3.6, §4.6 and §5 carry a time-condition annotation (original sentences intact,
`non-negotiable` intact); §9 is unchanged. Supported languages are now C / Zig. The constitution
itself is a live document that stopped being maintained after its v0.5 roadmap section — a
separate issue, recorded here and deliberately not fixed by this batch.

## 48. The xz decoder is `xz2`, and the `.tar.xz` arm carries no platform gate

**Date**: 2026-09-24 ｜ **Status**: Decided

**Decision**: `ArchiveKind` gained `TarXz`, unpacked by `extract_tar_xz` — `extract_tar_gz`
line for line with `xz2::read::XzDecoder` in place of `flate2::read::GzDecoder`, keeping the
same Zip-Slip guard (`safe_relative`), the same `set_overwrite(true)` and the same per-entry
cancellation. The decoder is **`xz2`**, not `lzma-rs`, and the dispatch arm has **no `cfg`
gate**.

**Why `xz2`**: it is `flate2`'s sibling by the same author and its API is the same shape
(`read::XzDecoder` beside `read::GzDecoder`), so the new function is a copy rather than a new
design; and it was already in `Cargo.lock` — `zip` pulls `xz2` → `lzma-sys` — so the direct
edge moved the lock by **one line** and no version. `lzma-sys` compiles its vendored liblzma C
on MSVC (where it disables `pkg-config` on purpose) and falls back to that same vendored build
when a unix host has no `liblzma`, so no platform gains a system-library prerequisite.
`lzma-rs` (pure Rust, also in the lock through `zip`) was the alternative; it would have meant
writing the "decompress the stream, then hand it to `tar`" bridge ourselves, for no gain —
`zip` already compiles the C path on every platform we ship.

**Why no `cfg` gate**: `.tar.gz` is gated to non-Windows here because it is only ever a **unix
asset** of our own downloads. A `.tar.xz` is different: it is the shape of the **host's own**
Zig release (Windows is the `.zip` one, macOS/Linux are `.tar.xz`) and of Rust's
`rust-std-*.tar.xz` on every platform. Gating it to non-Windows would only have to be undone by
the next batch, and a Windows host reading a `.tar.xz` is exactly what those downloads need.

**Impact**: `extract_tar_xz` is available on every platform; `spec_for_current_platform()` is
**unchanged** (still the single xPack spec), so nothing downloads an xz archive yet — the Zig
locator and the Zig/Rust specs belong to the apply batch. `ArchiveKind` carries no serde, so no
wire type moved.

## 49. A download names its toolchain; the language is a label, and the spec stays two functions

**Date**: 2026-09-24 ｜ **Status**: Decided

**Decision**: `DownloadSpec` gained `toolchain: Toolchain` (`C` / `Zig`, serde-able, absent means
C). It decides the two things the module cannot infer: which locator finds the product inside the
extracted archive (`product_locator`: `find_compiler` for C, the new `find_zig` for Zig) and which
adopt call the host makes after the install (`set_toolchain_path` for C, `set_zig_path` for Zig).
The language reaches the download as a **label** — `--toolchain zig` on the CLI, a
`{"toolchain":"zig"}` body on `POST /v0/toolchain/download`, an optional argument on the Tauri
command — and one function, `Toolchain::parse`, turns a label into the enum, so no edge can
disagree with another. Zig gets its own spec function (`zig_spec_for_current_platform`), and
`spec_for_current_platform()` is untouched.

**Why a field rather than a second spec family**: two spec functions, one enum. The spec is what
carries the *product* (version, URL, checksum, archive kind) and the two releases share nothing of
that; but everything after the download — the staging directory, the rename, the idempotent
"already installed" check, the adoption — is identical, and it needs to know which product it is
looking at. A `toolchain` field is that knowledge in the one place both halves already read.

**Why a label and not a richer type on the wire**: the HTTP body's parameters are flat strings by
design (`Params::from_json` keeps scalars and drops nested values), and the Tauri command takes
`Option<String>` so a frontend cannot send a shape the other edges would refuse. `None` and the
empty string both mean C, which is exactly what every caller sent before this batch — so the
change is additive at every edge, including the endpoint whose body did not exist.

**Why `install_subdir` is still dead**: the batch set out to "wire it up or delete it" and found
the wiring does not work. `download_and_install` finishes with `fs::rename(staging, install_dir)`,
where `install_dir = dest_root/<version>`; an "extract to `dest_root/<install_subdir>`" reading
with Zig's empty string would rename onto the existing, non-empty `dest_root` and fail. What the
module actually needed was the **locator**, which is per-language, not a directory hint. The field
stays as it is and remains dead: it is its own (small) decision, recorded here so the next batch
does not rediscover it.

**Impact**: `find_compiler` and `is_compiler_name` are untouched (the sandbox registry scans
through them); the C arm of `product_locator` names the same function the code called before, so
the C path does not move. `ToolchainDownloadStatus` carries the running toolchain. Zig's five
assets cover the same `(os, arch)` set xPack does; Zig also publishes `aarch64-windows`, which is
**not** in this batch's five — one arm and one checksum away when someone wants it.

## 50. A reader of another process's output must not stop at the first line it cannot read

**Date**: 2026-09-24 ｜ **Status**: Decided

**Decision**: the loop that drains a child process's stderr **counts** a line it cannot read and
**keeps reading**. `InvalidData` (the bytes are not UTF-8) and any other I/O error increment a
`bad_lines` counter; `Interrupted` is retried rather than counted; `Ok(0)` (EOF, the child
exited) still ends the thread. The thread's liveness is published next to the buffer, and a test
that times out prints all three: lines captured, unreadable lines, reader alive or stopped.

**Why**: the old loop — `for line in reader.lines() { let Ok(line) = line else { break }; … }` —
turned **one** unreadable line into the permanent, silent loss of every line after it. That is
the worst failure shape available: the reader looks healthy, the buffer simply stops growing, and
the test that times out cannot tell "the line never came" from "the reader had already stopped".
`server/tests/logging.rs`'s `the_connection_line_appears_at_info` failed on Linux CI in exactly
that shape (twice, 5.09 s each, with only the startup warning in the captured stderr).

**What this decision does not claim**: that it was the CI root cause. The evidence says the
failure cannot be ours (the only `server` edit in the commit before it was the download arm's
parameter read) and that the same test passed on the two Linux runs before it; whether the reader
had stopped there is what the new diagnosis is for. The 5 s timeout and the 25 ms poll are
deliberately unchanged: lengthening a timeout hides this class of bug rather than finding it, and
both previous flakes in this repository (the relay's port lease, `audit::concurrency`) were fixed
by removing a race.

**Impact**: the rule generalises to every helper that reads another process's output — a reader
that dies on bad input is a reader whose silence means nothing. No production code changed; the
three existing tests keep their timeouts, and two new unit tests read a `Cursor` (no child
process) to pin both halves: every line of a normal stream, and everything after a non-UTF-8 line.
