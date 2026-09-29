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

## 51. Every pipe a child is given is read to EOF, or the child can be killed by writing

**Date**: 2026-09-24 ｜ **Status**: Decided

**Decision**: a pipe this repository hands a child (`Stdio::piped()`) is read **to EOF**, not just
until the reader has what it came for. `server/tests/logging.rs`'s `read_banner` returns its
reader so the caller can drain the rest, and the child's stdout and stderr go through the same
`drain_reader`.

**Why**: the test used to read the banner line and drop the child's stdout. The server writes
three more lines (`server/src/main.rs:81-83`) immediately afterwards, and a write to a pipe whose
read end is gone is EPIPE — which **on Unix raises SIGPIPE, whose default action terminates the
process**, silently: no panic message, no chance to log anything, stderr simply at EOF. Two CI
runs showed exactly that shape (a healthy stderr reader reporting `0 line(s) unreadable`, the
reader at EOF, and a server that had vanished before writing its `connection from … ended` line).
Windows has no SIGPIPE — a closed pipe is only an error there — which is why the same commit was
green locally and red on Linux. This is the third "local green, CI red" root cause this series
has recorded, after a missing system package (E4) and a missing environment capability (B-3b).

**Impact**: `Stdio::piped()` implies "drain it", here and anywhere else this repository drives a
child through pipes; the failure message reports the child's exit status and the stdout line
count, so a recurrence is diagnosed from one line of CI output instead of three batches. No
production code changed — the server's own `println!`s are correct; the reader was not.

## 52. A version-coupled download is refused before it starts, not after

**Date**: 2026-09-24 ｜ **Status**: Decided

**Decision**: the Rust sysroot (`rust-std`) is downloaded only when the machine's `rustc -vV`
reports the **same release** as the pinned asset, and the refusal happens in
`AppState::begin_toolchain_download` — the one method the Tauri command, the HTTP route and the CLI
all go through — so no edge can bypass it and no bytes move first. The decision itself is a pure
function (`rust_release_matches`), so the rule is testable on a machine that has no `rustc` at all.
A sysroot offered for another release is refused with a message naming both.

**Why**: a `rust-std` carries metadata that `rustc` compares against its own, so a mismatched
sysroot is not "slightly wrong", it cannot be used. The alternative — fetching the asset's sibling
`.sha256` at download time — was considered and rejected when the version was pinned (§F3b
reconnaissance, ruling 3): Cargo's lock-style promise here is "the checksum in the source is the one
this build verifies", and a checksum fetched at runtime verifies transport, not intent. Refusing
early is also what keeps the failure honest: the user learns their `rustc` is a different release
instead of discovering it at the first `.rs` compile, with 12 MB already on disk.

**Impact**: `RUST_VERSION` (1.98.1) is the pin, and a user whose toolchain is another release
cannot download a sysroot until the pin moves — deliberately, since that download could not work.
The rule generalises: any future component bound to a compiler release is gated the same way, in
`begin_toolchain_download`, not at adoption.

## 53. The MVP-era Rust ban is lifted

**Date**: 2026-09-24 ｜ **Status**: Decided

**Decision**: the constitution's §3.6, §4.6 and §5 forbid Rust during MVP. From v0.9 F3b the Rust
ban is lifted; the C++ and Python bans are unchanged (Python waits for a Linux sandbox, v1.x; C++
is still out of scope). The original sentences are all kept — each of the three now carries an
annotation saying the time condition it states has expired.

**Why**: Rust's `riscv64gc-unknown-none-elf` target with `no_std` builds a bare-metal ELF that
loads at `0x80000000` — the same shape as the C and Zig paths — so the sandbox itself needs no
change. `rustc` comes from the system (following the "the user installs it" precedent set by
QEMU), while `rust-std` is self-downloaded and version-coupled to `rustc`, pinned at 1.98.1 (§52).
The ban is written with the qualifier "During MVP", and MVP ended at v0.8.0, so lifting it is not
renegotiating a non-negotiable principle; it is reading the clause's own time condition honestly.

**Impact**: §3.6, §4.6 and §5 each carry an annotation (originals intact, the `non-negotiable`
heading untouched); §9 — the v0.1 status list — is not touched, because it is history: in v0.1
only C was supported. The supported languages are now C / Zig / Rust; Python waits for v1.x.

## 54. The sessions store waits for a lock, but is deliberately not the audit store

**Date**: 2026-09-24 ｜ **Status**: Decided

**Decision**: `SessionStore` sets a `busy_timeout` (5 s, `session::BUSY_TIMEOUT`) **before** its first
write, and `append_message` runs the message insert and the session's `updated_at_ms` bump as **one
transaction**. It gets **no WAL, no `synchronous = NORMAL` and no open retry** — the three things the
audit store has carried since v0.8/v0.9.

**Why**: the two stores' concurrency models differ on purpose. `audit.db` is **shared across
processes by design** (one chain per workspace, several writers appending), which is what makes WAL,
the busy timeout and the open retry load-bearing there — `PRAGMA journal_mode = WAL` bypasses the busy
handler, so the switch itself needs the retry. The sessions DB is **per instance**: `with_data_dir`
gives every instance its own `<data-dir>/sessions.db`. But "per instance" is a default, not a
guarantee — two default-path CLI or server processes resolve to the same `<temp>/riscdom/sessions.db`,
and an explicitly shared `--data-dir` collides on purpose — and in those cases SQLite's default
`busy_timeout` of zero turned the second writer into an immediate `SQLITE_BUSY`. That failure lands
in `SessionStore::open`, so it did not fail one command; it failed the whole instance at startup. The
busy timeout is what removes it; WAL would only add its two side files (`-wal`, `-shm`) for a sharing
model this store does not have. If sessions ever become shared, **this entry is the one to reopen** —
not to quietly copy the audit store's WAL in.

**Impact**: `session.rs` sets one pragma and wraps one method in a transaction; nothing else changed,
and the audit store is untouched. The leftover `docs/multi-agent-foundation.md` recorded ("the
sessions DB has no WAL or busy timeout") is superseded. The asymmetry between the two stores is now a
decision rather than an omission.

## 55. The Web UI is served outside the route table, and without a capability

**Date**: 2026-09-24 ｜ **Status**: Decided; landed with the v0.9 D2a batch

**Decision**: when `riscdom-server` is given `--web-root <dir>` it serves that directory's
`index.html` at `/` and its files under `/assets/*`, **before** the route table is consulted and
**without** an `Authn` check. Only `GET` is served this way, only those two shapes are in the
namespace, there is **no SPA fallback**, and **no route is added to `ROUTES`** for any of it. The
directory is read at request time; nothing is embedded in the binary. Everything behind `/v0/*` is
unchanged and still capability-checked.

**Why**: three reasons, each of which would have been a defect in a different design. (a) The route
table **is** the API: `Capability::ALL` is a vocabulary, every route declares one, and two tests hold
the table against the documents in both languages. A static file has no capability to declare, so
putting it there would have forced either a fake capability or a second notion of "route". (b) The
assets carry no secret — a document, a stylesheet and a script that only call the API. Requiring a
token for them buys nothing (an unauthenticated request still learns nothing about the host) and
costs the one thing that matters: the page that asks for the token. (c) The alternative — a second
static server, or CORS — puts the UI on a different origin than the API, which turns a same-origin
`fetch` into a preflighted cross-origin one and makes the token a cross-site credential.

**Impact**: `.js` / `.css` / image types are mapped explicitly (`content_type_for`), and hashed
assets are `immutable` while `index.html` is `no-cache`, because an `index.html` naming assets a
later build deleted is the one stale file that breaks the app. Traversal is refused twice: the name
may contain no `..`, root or prefix component (the `workspace_io::safe_relative` rule), **and** the
file that is found must canonicalize inside the resolved root, because a symlink inside the root can
point out of it; a client-supplied name is never percent-decoded, so an encoded `..` is a file that
does not exist rather than a traversal. Without `--web-root`, `/` answers a 404 whose message names
the flag. D2b builds the page this serves.

## 56. One built front end, two transports, chosen at runtime

**Date**: 2026-09-24 ｜ **Status**: Decided; landed with the v0.9 D2b-1 batch

**Decision**: the UI's API surface has **two implementations of one interface** —
`ui/src/api/tauri.ts` (the desktop shell's `invoke` / `listen`) and `ui/src/api/http.ts` (the
control plane's endpoints) — and `ui/src/api/index.ts` chooses between them **once, at runtime**,
from the presence of Tauri 2's own global (`window.__TAURI_INTERNALS__`, the object
`@tauri-apps/api/core.js` itself calls `invoke` through). The build is unchanged: one `npm run build`,
one `ui/dist`, which the desktop shell loads as `frontendDist` and the server serves with
`--web-root`. The shared shapes live in `api/types.ts` and the one rule both transports need in
`api/envelope.ts`, so neither transport owns either.

**Why**: the alternative — resolving the implementation at **build time** (a Vite alias, or a second
entry with its own `outDir`) — produces two artifacts and immediately raises the question this
project does not want to answer: which one does `--web-root` point at, and which one did `tauri build`
just bundle? A stale or crossed artifact is a class of bug with no visible symptom until a user
reports it. A runtime check costs one `if` and makes "the same UI" literal. The global is chosen
deliberately over a build flag or an env var: it is Tauri's own marker, so it cannot be forgotten in
a config file, and a browser (including Node in a probe) simply lacks it.

**Impact**: the browser bundle carries the Tauri branch and vice versa — **+7,041 bytes (+1.1%)** in the
built `dist/` today, which is the price of one artifact and accepted. `api/index.ts` spells the
surface out one name at a time instead of `export *`, and annotates the chosen implementation as the
other's shape minus the Web-only helpers, so a name or a signature that exists in only one
implementation is a **compile error**; `ui/scripts/probe-ui-api.mjs` asserts the same names on both
source texts and exercises the HTTP implementation against a stand-in `fetch`. A refusal is a
**string** on both paths, because the desktop's commands are `Result<_, String>` and the store renders
`String(e)`. Anything that later needs a genuinely different bundle (a mobile shell, a different
protocol) reopens this entry rather than forking the build.

## 57. The Web client's own names are declared, and its gate is the application's front door

**Date**: 2026-09-24 ｜ **Status**: Decided; landed with the v0.9 D2b-2 batch

**Decision**: two related rules from the management program's second step. (a) The API surface has a
**shared** part — the names both transports carry — and a **declared Web-only** part
(`setApiBase` / `setToken` / `currentToken` / `clearToken` / `verifyToken` / `getHealth` /
`getStatus`). The shared part is held by `api/index.ts`'s `SharedApi` (one implementation's shape,
which the other must satisfy to compile) and by `ui/scripts/probe-ui-api.mjs` (the two source texts,
name by name); the Web-only part is an explicit list in **both** places, and the probe asserts the
extra names are **exactly** that list. (b) The login screen is a **gate in `App.tsx`**, not a route and
not a branch inside `AppShell`: no token means `<Login>` and nothing else.

**Why**: (a) The names are not symmetric and should not pretend to be. The desktop **is** its host —
same process, authenticated by being so — so "is this token accepted", "where is it kept" and "what
does `/v0/status` say" are not questions it has. Making them shared would mean inventing desktop
commands that return something plausible, or an interface that lies. Declaring them Web-only keeps
the shared list honest, and putting the list in two places (the type and the probe) means a name added
to one implementation without the other is caught by the compiler *and* by the gate. (b) `AppShell` is
where `useAppStore()` is called, and the store reads a dozen things on mount. A gate **inside** the
shell would have to mount the store first, which means every one of those reads runs unauthenticated
and fills the error state before the user has typed anything — a shell that looks broken at the exact
moment it is being introduced. Gating outside also keeps `AppShell` unchanged in the one respect that
matters (it still takes no props and still owns the store), so the desktop path is untouched: on the
desktop the gate is satisfied by the absence of the question.

**Impact**: the token lives in `sessionStorage`, or in `localStorage` when "remember this device" is
ticked, and never in a URL; a rejected candidate is never installed, because the check runs before
the install. `verifyToken` reports four outcomes rather than one, so "wrong token" and "server not
answering" stay distinguishable to the person who has to fix one of them. The status page is the
shell's third view and is offered only where it can work (`isTauriRuntime()`), since `/v0/status` is
a control-plane endpoint the desktop never calls. A future page that the desktop cannot serve belongs
in the same declared list, with the same probe assertion — not in the shared one.

## 58. A busy executable is a moment, not a defect — and only that error is retried

**Date**: 2026-09-24 ｜ **Status**: Decided; landed with the v0.9 ETXTBSY fix

**Decision**: the four "is this product runnable" probes in `state.rs` — `zig_runs`,
`rustc_release`, `rust_runs`, `toolchain_runs` — run their command through one helper,
`exec_with_busy_retry`, which retries **only** `std::io::ErrorKind::ExecutableFileBusy`, at most
`EXEC_MAX_ATTEMPTS` (5) times, `EXEC_RETRY_DELAY` (10 ms) apart. Every other error is returned on
the first attempt, and a busy refusal that outlives the budget is returned too. The match is on
`kind()`, **never** on `raw_os_error() == 26`.

**Why**: the fourth "local green, CI red" mechanism this ledger records (after a missing system
package, a missing environment capability, and a child killed by `SIGPIPE`), and the first one whose
cause is **not in our own handle lifetimes**. `ETXTBSY` is the kernel refusing `execve` because
**some** process has the file open for writing — and on Unix that includes a process that has
**forked but not exec'd yet**: `CLOEXEC` closes an inherited descriptor only at `exec`, so in a
multi-threaded process a sibling thread's `spawn` can hold the write reference for microseconds
after this process has closed its own. A test binary is exactly such a process, which is why
`a_mock_zig_download_installs_and_adopts_the_compiler` failed once and passed the run before on
identical code: the extraction writes the fixture binary, adopts it, and execs it — and if a sibling
test happened to fork inside that window, the exec was refused. `kind()` rather than the raw errno
because 26 is `ETXTBSY` on Unix and an unrelated Windows code (measured: `from_raw_os_error(26)` is
`Uncategorized` on Windows), and a raw-number match would be a second, wrong source of truth. The
retry is bounded at fifty milliseconds because the window it waits out is microseconds wide; the
budget exists to make a *transient* failure survive, not to make a *permanent* one look healthy —
which is also why nothing else is retried.

**Impact**: the fix is in **production**, not in the test: it is the same robustness a user needs
when an indexer or an antivirus holds a freshly downloaded binary open for a moment, and it keeps
the probes' promise (a failure still names the tool and the error). Two things were considered and
rejected: an explicit `drop`/`sync_all` in the extractor (there is no handle of ours left to close
at that point — the write reference belongs to *another* process, so it cannot help; and a `sync_all`
would address durability, which is a different question), and a blanket retry (a wrong architecture
or a missing library would be retried five times for nothing). `#[cfg(unix)]` tests pin the
error-kind mapping and reproduce the refusal in the process itself; on Windows there is no such
error to reproduce.

## 59. PowerShell never reads or writes source files

**Date**: 2026-09-25 ｜ **Status**: Decided

**Decision**: no source file (`.rs` / `.ts` / `.tsx` / `.mjs` / `.js` / `.py` / `.sh` / `.ps1` /
`.md` / `.css` / `.json`) is read or written **with PowerShell**. Files go through the `edit` and
`write` tools, as UTF-8 without a BOM. PowerShell is for *commands* (`git`, `gh`, `cargo`, `npm`,
`node`, `python`).

**One exception**: a **byte-level** replacement of an *invisible* character (a BOM, a private-use
codepoint) may use Python's explicit byte mode (`open(rb)` / `open(wb)`) — with a hex dump before,
a hex dump after, and a `git diff` review to prove nothing else moved. `edit` cannot express such a
character at all: a BOM at the start of an `oldText` is normalised away before the match is even
attempted, which is how this exception came to exist.

**Why**: Windows PowerShell 5.1 reads a BOM-less UTF-8 file as the ANSI code page (GBK here) and
writes the text back as UTF-8, which is lossy in three ways at once: `E2 80 xx` (an em dash, an
ellipsis) becomes `U+9225` plus a lost byte, `C2 A7` (`§`) becomes `U+6402`, and
`Set-Content -Encoding utf8` *adds* a BOM that was never there. It happened twice in this
repository — v0.7 (`sandbox/`, 4 spots) and v0.9's D2b-1 batch (`ui/src/api/`, 18 spots plus 3 BOMs)
— and **no check saw either**, because every damaged character sat inside a comment: the compiler,
the bundler, the probes, the string registry and the bilingual scan all passed. The rule that
already existed for Rust sources existed for exactly this reason; it was simply written one language
too narrowly.

**Impact**: this entry **extends** the earlier "do not read or write Rust sources with PowerShell"
beyond Rust, so the wording in the batch prompts is now the general one.
`scripts/scan-encoding.py --check` runs in `scripts/gate.sh`, so a third occurrence cannot land
(and the two that did are cleaned up: 24 spots across five files, no behaviour changed). The
invisible-character exception is recorded here rather than improvised next time, and it is bounded:
bytes in, bytes out, hex dumps on both sides, `git diff` as the proof.

## 60. The gate only fails on classes that are certain

**Date**: 2026-09-25 ｜ **Status**: Decided

**Decision**: a static check in the gate fails on the classes of finding that **cannot be a false
positive** — for the encoding scan, mojibake (class E) and a BOM (class F). The ambiguous classes
keep reporting and never block: three or more `?` in a row, a string literal that is nothing but
`?`, an ASCII `?` beside CJK text.

**Why**: `scripts/scan-encoding.py`'s class C cannot be decided — "a literal that is only `?`" is
indistinguishable from a ternary's tokens or a legitimate `"?"` — and it measured 13 hits on this
repository, every one a false positive (v0.5 batch 12). The script was therefore *deliberately*
kept out of the gate, with that reason written in its own docstring. That reasoning holds; what
changed is that the script can now be asked for the certain classes only. A guard that cries wolf
erodes the gate — the gate is the one list of what "green" means (handoff §4) — so `--check` is
narrow on purpose, and a finding it does raise is worth acting on immediately.

**Impact**: `--check` exits `1` on E or F and `0` otherwise; every other mode still exits `0` and
still prints everything. On the day it was added it reported `A 0, B 4, C 16, D 0, E 0, F 0` — the
four and the sixteen are the ambiguous classes, kept visible on purpose and harmless by design.

## 61. The Web client reads the event stream by hand, over `fetch`

**Date**: 2026-09-25 ｜ **Status**: Decided; landed with the v0.9 D2b-3 batch

**Decision**: the browser reads `GET /v0/events` with `fetch` and a `ReadableStream` reader, not with
`EventSource`. Frames are decoded by `ui/src/lib/sse.ts` — a pure module (no `fetch`, no DOM) that the
probe imports directly — and the envelope inside each frame decides where it goes: `kind: "event"` to
the subscribers of that `event` name, `kind: "gap"` to a separate **`onGap`** callback, `kind:
"hello"` nowhere. The transport keeps the last `id:` it saw and sends it as `Last-Event-ID` on the
next dial, and re-dials with a doubling delay (1 s → 15 s) while anyone is subscribed.

**Why**: `EventSource` cannot set request headers, and this stream is token-authenticated with
`Authorization: Bearer` — the alternatives (a cookie session, a token in the query string) are the two
options `docs/control-plane-events.md` already records as undesirable, and this server implements
neither. `hello` is not dispatched because it describes the *stream* (its buffer and filters) rather
than the host, and what a client actually needs from it — how far the buffer reaches — arrives on
every frame as its `id`. `gap` gets its own callback rather than being dressed up as a host event,
because it is not one: it says the replay buffer could not cover the hole, and the only honest answer
is to re-read what can be re-read. The parser lives in `lib/` for the reason the other rules there do:
it is the part worth testing, and a pure module can be probed in Node without a server or a browser.

**Impact**: one stream serves every subscriber — opened by the first subscription, closed by the last —
so a page that stops caring does not leave a request open. A dropped connection is not surfaced as an
error (pages keep showing what they last read) and the re-dial backs off instead of spinning. What a
gap *cannot* restore is written down where it is handled: streamed chat text and serial bytes only ever
arrived as events, and no endpoint replays them. `api/index.ts`'s `Omit` list and `probe-ui-api.mjs`
both name the Web client's own exports (`onGap` joins them here), so the two lists cannot drift.

## 62. A read-only board may still write display preferences

**Date**: 2026-09-25 ｜ **Status**: Decided; landed with the v0.9 D2b-4a batch

**Decision**: the Web client's "look, do not touch" rule has exactly **two** exceptions — theme
and language. They are implemented over HTTP like everything else the browser uses
(`POST /v0/settings/theme` and `/v0/settings/language` in `api/http.ts`), and the appearance
screen is deliberately **not** wrapped in `DesktopOnly` while every other control in the client is.

**Why**: those two are **display preferences**, not node configuration: they change how the person
looking at the board sees it, and they change nothing about the machine, the sandbox or the audit
chain. The alternative was not "a nicer read-only" — it was the behaviour that existed: the choice
applied locally and the host call failed, so the user saw an error message about a switch that had,
in fact, already worked. That is the only failure worse than a missing feature: a working control
that reports it is broken. Of the 26 control endpoints, these two are the only ones whose subject is
the client itself.

**Impact**: `AppearanceTab` stays unwrapped (the probe asserts it has no `DesktopOnly`), and the two
functions in `api/http.ts` are real calls rather than `desktopOnly` refusals — which also means
`probe-ui-api.mjs` now expects them to resolve while the other twenty-four refuse. If a genuinely
read-only token ever exists (per-capability tokens are v1.0 work), it must carry `settings.write`
for the board to keep working, and this entry is where that trade-off gets revisited.

## 63. The Web board reuses the node as tabs, not as more views

**Date**: 2026-09-25 ｜ **Status**: Decided; landed with the v0.9 D2b-4b batch

**Decision**: the browser does not grow a view per subject. Its node page becomes one page with
three tabs — **Status / Executors / Sandboxes** — held as local `useState` inside `StatusPanel`,
which is now a container over `panels/node/`. `AppShell`'s view union stays three (`main` /
`settings` / `status`).

**Why**: three reasons, and each is a different kind of cost avoided. (a) On a phone a tab row is
one gesture and a second layer of navigation is two; the node's status, its fleet and its sandboxes
are three faces of *one* subject, and asking someone to leave the page to see the second face is
navigation for the sake of a file layout. (b) A view is **global** state, held by the shell on
behalf of every panel; a tab is **local** state, invisible to everything else. Putting these three
behind tabs keeps the shell's state from growing every time a new kind of node information appears.
(c) The row itself is the settings page's own `.settings-tabs` / `.tab-btn` pair, so "add a tab"
costs no new styling — whereas "add a view" means a header button, a branch in the shell's render,
and a decision about what the Escape key should do.

**Impact**: future node-dimension information (another resource kind, a queue) extends the tab row
rather than the view union, and `probe-ui-node-panel.mjs` asserts both halves of that promise: three
tabs in the page, and the shell's union unchanged. The four reads the new tabs need are **shared**
names in the adapter — the desktop had the commands all along — so no name is declared Web-only for
this, and the probe's list checks did not move.

## 64. A field no one reads is deleted, not kept as a note

**Date**: 2026-09-25 ｜ **Status**: Decided; updates §49

**Decision**: `DownloadSpec` and `QemuDownloadSpec` no longer carry `install_subdir`. §49 recorded that the field "stays as it is and remains dead" and left the removal to a later batch; that later batch is this one, and the answer is deletion.

**Why**: a dead field is not a note — it is a promise someone will misread. §49's own reason for keeping it was that the next batch should not rediscover why it was dead, and this entry *is* that record, which removes the reason to keep the field. Nothing in production read it (what the module actually needed was the locator, `product_locator`), one test asserted its value, and so the field made ten construction sites pass an argument for nothing. That test now asserts what the spec is for — the pinned URL, the checksum's shape, the platform-following archive kind.

**Impact**: both `spec_for_current_platform` functions and `begin_toolchain_download`'s callers behave exactly as before; the two spec structs are one field smaller and their `Debug` output one line shorter. §49 stays as it is, as history.

## 65. The migration relay holds its port as a lease, like every other bind

**Date**: 2026-09-25 ｜ **Status**: Decided; extends §35

**Decision**: `MigrationRelay::bind_local_with_timeout` reserves the loopback port it binds in the process-wide registry (`reserve`) and keeps the resulting `PortLease` as its listener, so the number stays held for as long as the relay lives. Until now the registry was fed only by `lease_local_ports` / `lease_local_port`.

**Why**: §35's contract is about *live* leases — two leases alive at once never carry the same number — and the relay was the one bind that did not take part. The OS already refuses to hand out a port that is bound right now, so the practical gap was small: what was missing is the weaker half of the rule, that this process will not hand the number to a second holder the moment it lets go. The fix stays internal — the lease owns the listener, `addr()` is unchanged, the constructors keep their signatures — so the public API does not move.

**Impact**: a relay's port appears in `leased_ports()` while it is alive and leaves it when it is dropped (pinned by a new unit test); the exhaustion path reports `SandboxError::PortLease`, the same shape `lease_local_ports` uses. `send_file_to` is unaffected: it connects to a peer that is already listening and never binds.

## 66. The desktop is answered before any token is read

**Date**: 2026-09-25 ｜ **Status**: Decided; landed with the v0.9.1 fix

**Decision**: the front door asks **which runtime it is** *before* it asks for a token. The desktop short-circuits straight to the shell; the token gate is a separate component that the desktop never reaches.

**Why**: the two runtimes answer "is there a token?" differently — the Web client has one to demand, while the desktop has none to hold and no `/v0/health` to check one against, because its host lives in the same process. v0.9.0 shipped a gate that never asked, and every desktop user was stopped at a login screen no input could pass: the desktop was unusable. The ordering **is** the fix — a shortcut placed after the token read would still have read the token, and a single component holding both would still have drawn the login screen.

**Impact**: `SharedApi` and its `Omit` list are untouched (the fix lives inside `App`), so no name becomes Web-only and the adapter's shape does not move. `probe-ui-login.mjs` pins the ordering — the desktop is let in before any token is read, and the login screen is only on the Web side — which is the half a Node probe can see at all: probes run without a `window`, so every probe exercises the Web branch and the desktop branch is verified by hand.

## 67. A settings page may show the token, but never mint one

**Date**: 2026-09-25 ｜ **Status**: Decided; landed with the v0.9.9 network batch 2

**Decision**: the desktop's network tab may show the LAN token, through a shell-local command (`read_lan_token`) that **reads `<data-dir>/token` and never creates it**. Opening a settings page must not bring a credential into existence; minting a token stays the job of the server that actually starts.

**Why**: the token has to reach a person — a phone on the same network needs to be given it — and until this batch the only reader was `riscdom-server`, which *generates* the file when it is missing. A command that reused `load_or_create` would have written a token merely because somebody looked at a settings screen: a credential appearing as a side effect of navigation. Reading the file and refusing cleanly (`no token yet: …`) leaves the act of creation where it belongs.

**Impact**: the value goes to the caller and nowhere else — not to the log, not to disk, not over the wire — and the page shows it only when its button is pressed. The file name is `server`'s own constant (`TOKEN_FILE`); `probe-ui-network-tab.mjs` asserts the two agree, so a rename there cannot make this read the wrong file in silence. `settings.json`'s own rule stands: a **token** that belongs to somebody else's node is not stored by this batch (the field exists and stays `None`).

## 68. The board the desktop serves is the node it runs, and the bundle carries the front end

**Date**: 2026-09-25 ｜ **Status**: Decided; landed with the v0.9.9 network batch 3

**Decision**: the desktop's LAN board is the embedded control plane started over the **app's own `Arc<AppState>`**. The shell manages the state as an `Arc` and `host-tauri`'s 68 commands take that same handle. The built front end ships as a Tauri **resource** (`{"../dist": "dist"}`), resolved by one helper that tries the resource directory first and the source tree second. Any change to a network setting rebinds — stop, then start — and `RunEvent::ExitRequested` aborts the server.

**Why**: the alternative — handing the server a *clone* — was rejected for a reason worth writing down: `AppState` keeps a VM slot, a settings cache and the run bookkeeping **in memory**, so a clone is not the same node. A board that can start a second QEMU on one machine is worse than no board at all, and no test would have caught it. Sixty-eight signatures is the price of making that mistake impossible; they are mechanical (the bodies are untouched, because `Arc<AppState>` derefs to `AppState`), and `probe-ui-lan.mjs` counts them. The resource exists because `frontendDist` embeds the front end *in the binary* and leaves no directory on disk to serve; the fallback exists because `tauri dev` has no resource bundle at all.

**Impact**: one `Arc`, one node, from the window and from a phone. Loopback unless `lan_allow_lan` is on, which is why that switch carries the warning. The token a phone must type is the one the server minted in `<data-dir>/token` — the same file the read-only command of the batch before reads — so nothing about credentials moved into `settings.json`.

## 69. Build output lives under a stable parent directory

**Date**: 2026-09-25 ｜ **Status**: Decided; landed with the v0.9.9 3/N-fix2 batch

**Decision**: the front end builds into `ui/dist/app/`, and the parent `ui/dist/` holds one tracked file (`.gitkeep`) whose only job is to keep that directory present in a fresh checkout — because `tauri-build` checks `bundle.resources` paths with `Path::exists()`. `emptyOutDir` keeps its default: it empties `app/`, never the parent.

**Why**: batch 3/N declared `"resources": {"../dist": "dist"}` and thereby turned a **build artifact into a compile-time prerequisite**: on a fresh checkout (CI) `ui/dist` does not exist, so `cargo clippy ui/src-tauri` failed with `resource path '../dist' doesn't exist`. That is the **fifth** “local green, CI red” mechanism in this repository's ledger — the first four were missing system packages, a missing environment capability, SIGPIPE, and `ETXTBSY` — and it differs from all of them in one way worth recording: it was introduced by our own change rather than by the environment. The obvious repair, a tracked `.gitkeep` *inside* `ui/dist`, was tried and disproved: Vite's `emptyOutDir` deletes it on every build, which would leave the working tree permanently dirty (batch 3/N-fix, stopped at that valve). Turning `emptyOutDir` off was rejected as well — a local `tauri build` would then pack every stale hashed asset into the installer. A stable parent with an emptied child satisfies all three requirements at once.

**Impact**: `--web-root`, `frontendDist` and both branches of `resolve_web_root` name `ui/dist/app`, and `docs/manual-acceptance.md`'s LAN recipes follow. The property the B-2 batch recorded — `ui/dist` is **not** a prerequisite for `cargo check` / `clippy` — holds again, verified with and without `dist/app/index.html`. The rule for the future: a path a build tool requires to exist at compile time belongs at a **stable parent**, with generated content in a subdirectory that tool may empty.

## 70. An in-network node's credential lives in the OS keyring, not in a settings file

**Date**: 2026-09-25 ｜ **Status**: Decided; landed with the v0.9.9 `"out"` batch

**Decision**: the desktop's connection to an in-network server stores its **address** in `settings.json` (`network.remote_url`, a preference) and its **token** in the OS keyring under `remote-token:<host>` (`host_core::keyring::user_for_remote`). The `NetworkSettings.remote_token` field that batch 2 left as a placeholder is **deleted**, not populated.

**Why**: `settings.json`'s own first line is that only non-secret preferences live there — a rule the file has kept since v0.4, when the API key was sent to the keyring instead. A remote server's token is the strongest credential this application handles: it is another machine's owner token, the file is plain JSON, and the file's directory is exactly the sort of thing a backup, a support bundle or a screenshot collects. The keyring is already the home of the provider key, so the shape existed; naming the account **per host** means connecting to a second node cannot silently overwrite the first one's token.

**Impact**: JS cannot read a keyring, so the three shell commands (`save_remote_token`, `read_remote_token`, `clear_remote_token`) are the bridge, and they are **local in every mode** — the machine that has to act when a remote window wants out is this one. `read_remote_token` is read-only, like `read_lan_token`: nothing here brings a credential into existence. **Caveat, inherited from `keyring.rs` and worth knowing**: a keyring that refuses degrades to in-memory storage, so a "remembered" token can be gone after a restart; the caller is told what the keyring said.

## 71. The host mode is a value, and the eight commands that wire a node ignore it

**Date**: 2026-09-25 ｜ **Status**: Decided; landed with the v0.9.9 `"out"` batch

**Decision**: `api/index.ts` holds the implementation in a **variable** (`let current: SharedApi`), settled once at startup by a new `setImpl(mode, url)`; every data-plane export is a one-line forwarder through it. **Eight names are exempt and always address this machine**: `getNetwork`, `setNetwork`, `readLanToken`, `lanStatus`, `saveRemoteToken`, `readRemoteToken`, `clearRemoteToken`, `restartApp`. A second predicate joins the first: `isTauriRuntime()` (what am I running inside) and `isLocalHost()` (is the node on screen this process's own).

**Why**: batch 4/N knew the load-time `const impl` could not answer "which host" — that is a *configuration* fact, not an environment one. The forwarders keep the property the `const` bought: one `SharedApi` type holds both implementations to each other at compile time, and a forwarder for a name one of them lacks does not compile (`probe-ui-remote.mjs` counts 68 forwarders, 60 of them through the mode). The exemption is the load-bearing part: in remote mode the forwarded `getNetwork` would ask *the remote server* to rewire itself — the one thing that has no endpoint and never will — so a window whose server is unreachable could never come back. Wiring a node is a local act; that is why these eight, and only these eight, ignore the mode.

**Impact**: the mode is settled **before the shell mounts** (`App.tsx`), so the store's existing mount-time pulls are the re-read and nothing has to be re-fetched mid-session; changing the mode is a **restart**, which is why `restart_app` exists. The screens had to stop asking `isTauriRuntime()` where they meant `isLocalHost()`: the gate, the settings page's tab table and the top bar's badge; `DesktopOnly` / `WebOnly` keep their old meaning ("does this build have a Tauri runtime") on purpose. A remote window is read-only over HTTP for the same reason the browser is — the remote server's own copies of the controls are refusals.

## 72. A node owns sandbox instances, and its own is what a switch acts on

**Date**: 2026-09-27 ｜ **Status**: Decided; landed with the v1.0 M2a-1 batch (the instance table)

**Decision**: `AppState`'s single VM slot becomes an **instance table**
(`instances: Mutex<HashMap<InstanceId, SandboxInstance>>`) with a `current_instance` pointer, and the
state that used to be per node — the VM slot, the serial broadcast list, the accumulated serial text
and the VM start time — is **per instance**. A node creates exactly one instance at construction:
**its own**, which is never removed and is what a switch and a plain run act on. Deriving an instance
(`spawn_instance`) starts a second VM in a slot of its own and does **not** change what the node is
running. `InstanceId` is `<device>-<pid>-<seq>` (`agent::identity`), the device is settable (default
`local`; naming a node is the connection layer's, roadmap §4), and **one counter serves agents and
instances** so the two spaces cannot mint the same string. The parts are not a path syntax: they are
read from the right (`rsplitn(3, '-')`), because a device name may contain a `-`.

**Why**: v0.9.9's model was one node, one VM — a slot, a serial buffer and a start time as singletons,
which make a second guest *impossible* rather than unlikely (decision §68 kept the node's own
`Arc<AppState>` for exactly this reason). The cheapest correct change was already half built:
`AgentLoop::with_vm` takes an `Arc<Mutex<Option<VM>>>`, so an instance that owns its slot is handed to
the loop **unchanged**. The node's own instance exists so that no pre-M2a path had to learn a new
meaning — a switch replaces the VM inside it, exactly as it replaced the slot's VM — and so that
"which instance is current" always has an answer (`current_instance` falls back to it).

**Impact**: `vm_slot()` is a **method** now (the current instance's slot), because a field cannot
alias a per-instance `Arc`; `spawn_instance` is the derive path and `stop_instance` the reap path (the
node's own instance is emptied, never removed). Snapshots move to
`snapshots/<device>/<instance_id>` — one directory per instance — and the two older layouts (per
agent, and the shared root) stay **readable**, which is what a node upgrading from v0.9.9 needs.
Ports were never the obstacle: `HELD_PORTS` leases a pair per VM, so two guests in one process cannot
take each other's (the shape of decision §65 holds). The audit chain is untouched: an instance's
identity rides in `detail`, the same way `agent_id` was added as a field that is deliberately not in
the hash formula. The endpoint that reaches all of this is M2a-2 and `Task.instance` is M2a-3; until
then the table is reachable only from inside the host.

## 73. An instance route is a pattern, and a member path carries both halves

**Date**: 2026-09-27 ｜ **Status**: Decided; landed with the v1.0 M2a-2 batch

**Decision**: the instance model's four sandbox routes are **pattern routes**, not rows in
`ROUTES` — the table compares literal paths and `{name}` is not one — so they resolve through
extractors (`sandbox_instance_path_from` for the collection and the member,
`sandbox_capabilities_from` for a definition's capabilities). `Resolution::Query`'s `path_param`
becomes a **`Vec<(&'static str, String)>`** (it was an `Option`): a member route carries **both**
`name` and `instance_id`, because a handler that reaps an instance has to check that the id
belongs to the definition the path names. `GET /v0/capabilities` is the one **row** the batch
adds — a literal path — and it answers what the *caller* may do (`status.read`), a deliberately
different question from `/v0/sandboxes/{name}/capabilities`'s "what can this definition do"
(`sandbox.read`). The two are not to be merged.

**Why**: deriving an instance is not adopting one (decision §72), so the create endpoint takes
neither the switch's one-at-a-time slot nor the in-flight-run check — a second guest is not a
takeover — and the answer says so by being a `201` carrying the new id while the node's
`current_sandbox` stays where it was. The reserved-name list does **not** grow: `instances` and
`capabilities` are second segments, so a definition may be called either of them without
shadowing anything, while `requests` (a real first-segment route) stays reserved. One `Option`
parameter could not carry both halves of a member path, and packing them into one string would
have made the handler re-parse what the router already knew.

**Impact**: `docs/control-plane-api.md` §5.1 is **33**; the tool-schema tables grew five rows and
the definitions five entries, which moved `patterns.len()`'s hard assertion from 4 to **8** and
`scripts/check-tool-schema.mjs`'s `NAMED_PATTERN_ROUTES` map (that checker derives a name
mechanically, so a verb-named pattern is an entry there or it is a failure). The five tool names
are `instance_list`, `instance_create`, `instance_delete`, `sandbox_capabilities` and
`capabilities`. Failures map as: an unknown definition `404 cause "name"`, a definition that
cannot run `503` (the answer the switch gives for the same condition), a start that failed `500
cause "sandbox_start_failed"`, and an id that is not that definition's `404 cause "instance"`.
The browser's `SandboxInstanceView` is checked field by field against the host's `InstanceView`
by the node-panel probe, and `Task.instance` — the field that routes a run to one of these
instances — arrives with M2a-3.

## 74. An instance outranks a sandbox declaration, and the pair is checked

**Date**: 2026-09-27 ｜ **Status**: Decided; landed with the v1.0 M2a-3 batch

**Decision**: `Task` gains `instance: Option<InstanceId>` beside `sandbox: Option<String>` (both
`#[serde(default)]`), and a run that names an instance runs on **that** instance's VM slot — the
finer declaration decides. The pair is validated **strictly, before anything moves**: an id this
node does not own is `InstanceNotFound` → `404 cause "instance"`, and an instance whose definition
is not the sandbox the task also named is `InstanceConflict` → `409 cause "instance"`. The rule
lives in `task_instance_conflict`, a pure function beside `task_sandbox_conflict` (F2d's), so it can
be pinned without a live instance. `run_agent_for` takes the id as a fourth parameter and resolves it
through `task_instance`; **`AgentLoop::with_vm` is unchanged** — it already took a slot.

**Why**: the two declarations are not alternatives but two levels of one statement (*which
definition*, and *which running thing made from it*), and a run that honoured one while ignoring the
other would silently contradict what the caller asked for. A silent fallback is exactly what F2d
refused for the sandbox name, so the instance gets the same treatment. The routing shape follows from
the existing code instead of a new abstraction: `AgentHandle::run` already receives the whole `Task`,
`HostAgentHandle` already holds the `Arc<AppState>` whose table can answer it, and the loop already
accepts an `Arc<Mutex<Option<VM>>>` — so "which instance" is decided inside `run_agent_for` and
nothing above it changes shape. The id is validated where the executor runs, never by the
dispatcher: a stdio executor's instances belong to *its* table.

**Impact**: `/v0/agent/run` and `/v0/tasks` accept `instance` (the Tauri `run_agent` command grew an
optional argument; the front-end wrapper takes it and no call site changed); the id travels the
stdio protocol unchanged, and a worker validates it against its own node. The agent's `start_vm` tool
writes its `.mig` into the **instance's** directory (`ToolContext.snapshot_dir`, set by the host
through `AgentLoop::set_snapshot_dir`), which closes the inconsistency M2a-1 recorded. `run.start`'s
detail now carries `sandbox` and `instance` (`null` when a run had neither), written by a new
`run_start_detail_with`; an older detail still parses, because `parse_run_start` reads each key with
`get` — and **`detail` is part of each event's own hash**, so the two keys move *new* events' hashes
and no old row's; the hash *formula* is what does not change.

## 75. The LLM configuration is persisted per executor, and the settings file carries a version of its own

**Date**: 2026-09-27 ｜ **Status**: Decided; landed with the v1.0 M2b-1 batch

**Decision**: the non-secret half of a model configuration (provider, endpoint, model) is persisted in
`settings.json` under `llm_configs`, keyed by **executor id** — the node's own **device name** for the
machine itself, and `settings.executors[].label` for a worker — while the key stays in the OS keyring
under `llm-api-key:<executor>:<provider>`. `SETTINGS_VERSION` becomes **2**: `settings.json` is the
first format to use the migration rules of `docs/api-compatibility.md` §6 — read the declared version
(absent = the oldest format, not a number = corrupt), refuse a newer file with `data_too_new` (nothing
applied, nothing written), migrate an older one into the current format, keep the pre-migration bytes
as `settings.json.bak` **only when migrating**, write the migrated document back, and make a refusal
visible (an audit event plus `AppState::settings_problem`).

**Why**: a node that forgets which model it is configured for on every restart keeps its
configuration in an environment variable in practice — and with several executors on one machine,
"which model" is a per-executor fact, not a per-node one. The key stays in the keyring because that
rule has held since v0.4 and this is the file people copy, back up and paste into issues. The
**device name** keys the local executor rather than its `AgentId`, because the AgentId contains the
pid: an entry keyed by it would be unreadable after the very restart it exists for. The version bump
is forward-looking — v0.9.9 has no version check and cannot refuse anything — so its value is that
every reader from here on has a format to reason about, and that this build refuses a format it does
not know instead of reading the half it recognises.

**Impact**: `llm_configs` is additive in shape but the format is v2; a v1 file migrates to an empty
map, because that file carried no LLM configuration at all (nothing to guess). `LlmConfigStatus`
gains `config_persisted` beside `persisted`, whose meaning is unchanged — the interface reads it as
"your key is remembered". The keyring read migrates a v0.9.9 entry forward **and leaves the old entry
in place**, so no key has to be typed again and a build that goes back still finds it;
`clear_llm_config` deletes only the new name. The endpoints' executor dimension, the per-executor
session split and the interface are M2b-2/M2b-3: no endpoint, session or UI change ships here.

## 76. Session rows name their executor, and an old row stays unnamed

**Date**: 2026-09-27 ｜ **Status**: Decided; landed with the v1.0 M2b-2 batch

**Decision**: `sessions` gains an **`executor_id`** column (added idempotently when a migration runs),
and the session database's schema version lives in SQLite's own **`PRAGMA user_version`** — the
file-borne analogue of a JSON format's first-field marker. A file from before the column migrates **on
open** (its pre-migration bytes are kept as `sessions.db.bak`, and only a migration writes that), and a
file from a **newer** build is refused with `data_too_new` rather than half-read. Every session call
names an executor: the seven endpoints take an optional `executor` defaulting to this node's own, and
`current_session_id` is a per-executor map. A row that predates the column keeps **`NULL`**, and the
queries read `NULL` as **the node's own**; new rows always write their executor, and `rename` never
adopts an old row.

**Why**: writing `local` into the old rows would make them depend on a name the node is free to change
— the day it renames itself, its own history would vanish from its own list, which is silent data
loss. `NULL` ("no executor is named") and "the node itself" are two different facts that happen to
have the same consequence today; keeping them apart means a node that has no device name yet still
reads its history. SQLite's `user_version` is the version marker that needs no table of its own and is
read before anything else, so `docs/api-compatibility.md`'s rule now names what SQLite actually
provides instead of describing a metadata table nobody built.

**Impact**: `/v0/sessions`, `/v0/sessions/current` and the five session controls take `executor?`; the
Tauri commands pass this node's own and their signatures do not change, so no UI change ships here (the
selector is M2b-3). `SessionMeta` carries `executor_id` — `null` for an unnamed row, which the
browser's type marks as `string | null`. `open_session` looks a session up **by id** instead of scanning
every session, `clear_all` is scoped to one executor (the node's own also clears the unnamed rows), and
`ensure_session` repairs a **stale** pointer by starting a fresh session. `session_messages` is
untouched: a message belongs to exactly one session, and the join says so.

## 77. The executor is an optional parameter, and `*` is its only wildcard

**Date**: 2026-09-27 ｜ **Status**: Decided; landed with the v1.0 M2b-3a batch

**Decision**: The executor an LLM or session endpoint acts as is **one optional parameter**
(`executor`), absent meaning **this node's own** — never a second endpoint and never a boolean
flag. The one wildcard is spelled as its **value**: `/v0/sessions?executor=*` answers every
executor's sessions in one list. Endpoints whose answer is about a **single** executor refuse
`*` with `400 bad_request`, `cause: "executor"`: a model configuration belongs to one executor,
and so does "the current session".

**Why**: There is one axis here — *which executor am I talking about* — and a second spelling
(`?all=true`) or a second route (`/v0/sessions/all`) would be a second name for the same axis,
with its own row in the endpoint table and its own count to keep in step. Spelling the wildcard
as a **value** keeps the endpoint table unchanged: it is one token in the whole surface, so it is
also one sentence in the documentation. The refusal is the same rule read the other way: a
question with no plural answer does not get a wildcard answer, it gets the caller's parameter
named back. The merged list's `limit` is a **row count**, not a per-executor count, because one
statement and one `LIMIT` is what the store does — "N per executor" would need N queries and
would make the number mean a different thing per executor present.

**Impact**: `/v0/llm/config`, `/v0/llm/readiness`, `/v0/llm/stored-key`, the three LLM controls
and the five session controls take `executor?`; `/v0/sessions` additionally understands `*`.
The endpoint table's counts (§5.1 33 / §5.2 36) do not move, and neither does
`every_query_endpoint_answers`'s `cases.len() == 33`. The Tauri commands carry `executor` as an
optional argument, so a client that passes nothing keeps its old meaning; the UI selector that
uses it is M2b-3b.

## 78. `local` and `*` are reserved executor ids

**Date**: 2026-09-27 ｜ **Status**: Decided; landed with the v1.0 M2b-3a batch

**Decision**: An executor label may not be `"local"` (this node's own id until a node renames
itself) or `"*"` (the sessions wildcard). A settings file carrying one still **loads**, and the
offending entry is **skipped** with a `host.executor.reserved` audit event.

**Why**: The executor id is one key space. It names the entry in `settings.json`'s
`llm_configs`, the middle word of the keyring account, and the value of the `executor`
parameter — so a worker labelled `local` would share the node's own model configuration, and a
worker labelled `*` would collide with the wildcard itself. Skipping rather than refusing is the
same reading v1.0 M2b-1 chose for a newer settings file: the failure is made **visible** (an audit
event here, `settings_problem` there) instead of being swallowed or being allowed to brick a
hand-edited file. The file is left byte-for-byte as the person wrote it.

## 79. The reference dispatcher ships as a skeleton, and its boundaries are printed rather than fixed

**Date**: 2026-09-27 ｜ **Status**: Decided; landed with the v1.0 M2c-1 batch

**Decision**: M's reference implementation lands at `examples/python/supervisor.py` as a
**skeleton**: the loop (one state snapshot → decide → act → report), the action table, an event
reader that resumes with `Last-Event-ID`, and a `decide()` that returns `None`. It **imports**
`dispatch.py` — the transport, `read_token`, the error taxonomy — instead of carrying a second
copy of the wire format. The **five boundaries** the reconnaissance found are **stated** (in the
file's docstring, `examples/python/README.md` and the handoff), not fixed: M has no identity of
its own in the chain, a sandbox-request decision is not recorded, the instance table and the
approval slot live in memory, the audit read has no window or pagination, and five capability
names are vocabulary only. Conservative behaviour is the loop's **starting point**: a read that
fails ends the turn before any write, so "the safe decision" and "the default decision" are the
same one.

**Why**: The decision layer is the half that needs a model, a prompt and a user's policy;
shipping it would make the example into a product and its behaviour into somebody else's
opinion. What can be *proved offline* is everything around it, so that is what this batch
proves — with the fake node the E3 batch's technique already established. Importing rather than
copying is the same reasoning one level down (one description of the wire format), and it cost
exactly one discovery worth recording: this project's interpreter runs with
`sys.flags.safe_path` on, so a script's own directory is **not** on `sys.path` and the sibling
import needs two lines to say so. Printing the boundaries rather than fixing them keeps the
review honest: each one is a property of today's surface (who a token client is, what is
durable, what the audit read can express), and each fix has a home in a later batch — naming
them here is what stops them being rediscovered as surprises.

**Impact**: `examples/python/` gains a file and a bilingual README section; the client guide's
§8 points at it beside `dispatch.py`; the gate gains a second Python step (15 → 16), which the
`have_python` guard turns into a printed skip where there is no interpreter. `dispatch.py` is
**untouched** — imported, never edited — and `docs/tool-schema-control-plane.md` stays M's tool
list, so no third schema document (and no change to `check-tool-schema.mjs`) was needed.

## 80. M's decision layer is a bounded tool loop, and its model is not the node's

**Date**: 2026-09-27 ｜ **Status**: Decided; landed with the v1.0 M2c-2 batch

**Decision**: The reference dispatcher decides through a **bounded tool-calling loop**
(`LLMDecider`): the node's state — one snapshot — as the first user message, the control
plane's own tool schema **read** at startup from `docs/tool-schema-control-plane.md` and
filtered to the **eighteen** tools a dispatcher should have, at most `--max-rounds` model
calls per turn, and every tool call the model makes **performed inside the loop** and fed back.
The model is M's own — `--llm-base-url`, `--llm-model`, `--llm-api-key-file` or
`$RISCDOM_LLM_API_KEY` — and never the node's `llm_configs`. With no model configured the
layer is `OfflineDecider`: nothing is decided, the default rather than an error path.
`agent_run` and `events` are excluded **by name**; a tool that was never offered is not
performed; a failing or unreachable model ends the turn with no further control request.

**Why**: The tool schema already exists and the gate already keeps it in step with the
server's route table, so *reading* it is one less copy of the truth — and a whitelist the
document cannot satisfy is a startup error rather than a quietly weaker dispatcher. The two
named exclusions are the client guide's own rules, written down where the code is:
`agent_run` is the executor's loop (a dispatcher that ran turns would be a second executor),
and the stream is context, not a request that never returns. M's model and the node's model
are different jobs — deciding *which* task versus running *one* — so M carries its own
address, key and error types (`LLMTransportError` / `LLMApiError`, apart from the control
plane's): folding them together would make a dispatcher's outage look like a node's. The loop
needs a ceiling because the alternative is a process a model can talk into running forever.
And the **policy stays out on purpose**: the prompt is a skeleton — role, state, boundaries,
constraints — and the rules are the user's, which is what makes this a reference
implementation instead of an opinion.

**Impact**: `supervisor.py` grows the client, the loop, and a scripted **fake model** in its
self-test (45 assertion sites now), so the decision layer is provable offline; the README
gains the tool list and what is deliberately not offered; the client guide's §8 says what M
decides *with* instead of calling it a stub; and the gate's step count does not move (16 — the
same two Python steps). The real-machine half: against a live `riscdom-server`, M's dispatch
reached a real worker (which wrote a bare-metal guest and booted it in QEMU), `instance_create`
and `instance_delete` derived and reaped a real instance, a dead model left the chain
byte-unchanged (53 rows before and after), and a restarted M rebuilt its context from the node
— with the audit chain as the evidence for each.

## 81. Every act an AI supervisor takes leaves a row that names it

**Date**: 2026-09-27 ｜ **Status**: Decided; landed with the v1.0 gap 2/N batch

**Decision**: A client may name itself with the optional **`X-RiscDom-Agent`** header. When it
does, the `Actor` the request is served as carries that name as its `agent_id` and
[`ActorKind::Supervisor`] as its kind — and the **seven acts a dispatcher can take** write
chain rows that name it: `m.sandbox.spawn`, `m.sandbox.reap`, `m.sandbox.switch`,
`m.request.ask`, `m.request.approve`, `m.request.reject`, `m.task.dispatch`. The three acts that
had no frame of their own also gain one, so a watcher on `/v0/events` sees them; a **switch**
keeps its single `sandbox:switch` frame (F2b-2's "one frame per attempt, either way" — a
second frame for one attempt would be a second thing to count) and a decision keeps
`sandbox:request`, and what a request adds for both is the row. The identity travels in
`AuditEvent.agent_id` — the field that is deliberately **outside the hash
formula** — so attribution arrives without moving a single historical row. No header means the
credential's own identity (`operator`), which is what every release before this one assumed;
and the forty-odd **node** events (`host.theme.set`, …) keep using `emit_host` and keep naming
the node, because a caller header has nothing to say about a theme change.

**Why**: The M2c real-machine run showed the chain was **blind to AI decisions**: of the seven
acts M can take, `spawn`/`reap` had a row (attributed to `host`), and switch, ask, approve and
reject had **none** — a switch announced a frame and recorded nothing, and a decision was
durable only in a queue that lives in memory. `docs/roadmap-v1.0.md` §9 assumes the opposite
("M's state lives outside M"), and §36's own text says a decision "changes the record and
nothing else" — true, and exactly why the record has to live somewhere that survives a
restart. Two smaller decisions fall out of it: the name is **bounded and validated** (128
characters, no control characters) and a bad value is *ignored rather than refused*, because a
bad name is not a bad request; and the `.remote` names keep their local halves (the invariant
in `auth.rs`), so the cross-node contract of roadmap §5 has somewhere to land.

**Impact**: `ReqMeta` gains `caller`; `Actor::named_caller` is the one place that turns it into
an identity; `AppState::emit_m_action` is the one place the seven rows are written;
`host-core/src/events.rs`'s vocabulary goes **17 → 20** (`m:request:ask`, `m:request:reject` and
`m:task:dispatch` are new, and the three M2a-1 names that were declared and never emitted now
are). `docs/control-plane-events.md` §3's table is brought back in step with
the code — it had listed fourteen while the code defined seventeen, a drift this batch closes.
The API document's §3 documents the header. Nothing else on the surface moves: **no new route,
no new capability, no change to the hash formula, the triggers or a historical row**, and a
chain containing the new rows verifies intact.

## 82. An instance's past is derived from the chain, not kept in a file

**Date**: 2026-09-27 ｜ **Status**: Decided; landed with the v1.0 gap 3/N batch

**Decision**: The instance table is **not** persisted. `SandboxInstance.vm_slot` is a live
process handle — a VM, not data — and after a restart the guests are gone anyway, so "persist
the table" would be a file claiming VMs nobody has. What a definition **had** is derived
instead: `GET /v0/sandboxes/{name}/instances/history` folds the chain's `m.sandbox.spawn` and
`m.sandbox.reap` rows into a list (`instance_id`, `definition`, `spawned_at_ms`,
`reaped_at_ms`, `running`) — exactly the way `audit::derive_runs_from` rebuilds the run index.
`running` is read from the live table ("is it running **now**"), so after a restart it is
`false` for every record; the node's own instance is included with a `null` start, because
nothing derived it.

**Why**: §34 already separates what a node *runs* (runtime state) from what it *starts from*
(configuration), and the instance model inherited that: an instance outlives its VM, and a
table of live handles dies with the process. The chain, by contrast, is append-only and
durable — and since the gap 2/N batch it is also **attributable** (each row names its caller),
which is what makes it usable as the reconciliation source rather than a second bookkeeping
file that can disagree with it. Deriving answers the honest question too: "what was derived and
never reaped?" is a query over rows, not a field somebody has to remember to update.

**Impact**: `AppState::instance_history` and a `ReconciledInstance` view; one new query route
— a **pattern**, so §5.1's heading (which counts the table's static rows, checked against
`server/src/routes.rs`) does not move while the document's table and the tool-schema document
gain a row — whose `history` segment is a **reserved literal**: a member act on it is a `405`,
never a reap. `docs/control-plane-api.md` §5.4, `docs/tool-schema-control-plane.md` §2 and its
checker's naming table all follow. The audit window is this batch's other half: the
four fields `EventFilter` has always applied in SQL are now reachable over HTTP, with a reversed
pair refused as `400` and the `cause` naming the lower bound. **No table, no schema and no chain
structure changed**, and the reference dispatcher now declares its name — `--agent-id` is
required, because an unnamed dispatcher is what §81 went and fixed.

## 83. A capability name no route requires is not a capability

**Date**: 2026-09-27 ｜ **Status**: Decided; landed with the v1.0 gap 3/N clean-up (batch C)

**Decision**: The vocabulary loses the five names no route required: `task.dispatch`,
`task.dispatch.remote`, `sandbox.instantiate.remote`, `audit.read.remote` and
`request.approve`. `Capability` goes from 38 variants to 33, and the guard in `auth.rs` and the
endpoint's own answer move with it. The one M2a-1 name a route still asks for,
`sandbox.instantiate`, stays: the instance endpoints derive under it.

**Why**: A capability is **a typed column of the route table** — the thing that makes "an
undeclared route cannot be written down" true (the API document's §3). A name no route declares
has no such column, so it grants nothing and forbids nothing: it is vocabulary, and vocabulary
that says a power exists when no endpoint can exercise it is exactly the kind of drift the
document exists to prevent. The five were added in M2a-1 ahead of their endpoints; when the
endpoints arrived (`agent.run` for a dispatch, `sandbox.instantiate` for a derive, `sandbox.read`
plus the action's own implication for a decision) the five were left behind. Removing them now,
before M3 freezes the plugin interface, is the point: **the freeze should freeze 33 real powers,
not 38 with five placeholders**. The `.remote` invariant (a `.remote` name must have its local
half) makes "keep the three `.remote` halves, drop only `task.dispatch`" incoherent — a remote
half without a local one is refused by the test — so the clean-up removes all five and the
invariant holds vacuously.

**Impact**: `server/src/auth.rs` (five enum variants, five `ALL` entries, five `as_str` arms, the
`MUST_HAVE` list 14 → 9, the guard `>= 38` → `>= 33`); `server/tests/smoke.rs` (`33`, and the one
name a route still asks for); the count in both languages of `docs/control-plane-api.md`,
`docs/handoff.md` and `server/README.md`, plus the boundary note in `examples/python` (README ×2
and the dispatcher's docstring). The handoff's §1 keeps its M2a-1 record and gains an appended
correction — the same way that section already appends "gap 2/N took that list to 21". **No route
definition, no hash formula, no audit event constant and no `m.*` action name changes** — the
five were vocabulary, and the actions they were once confused with (`m.task.dispatch`,
`m.request.approve`) keep their rows. The historical CHANGELOG entries that recorded "grew to
38" stay as written: a changelog records what happened.

## 84. The queue is runtime state; the asks in it are derived from the chain, and a caller cleans up

**Date**: 2026-09-27 ｜ **Status**: Decided; landed with the v1.0 gap 3/N batch D clean-up

**Decision**: The pending-approval slot is **not** persisted, and it does not move into a file.
`SandboxRequests` stays what it is — a `Vec` behind a `Mutex` — and what survives a restart is
**derived from the chain**, the same way the instance table's past is (§82):
`derive_requests_from` folds the `m.request.ask` / `m.request.approve` / `m.request.reject`
rows into one record per request, and the process seeds the live queue with the rows still
**pending** at startup. A decided request does not come back live: it is history, and history
is the chain's. The chain also gains the queue's one cleanup — `DELETE
/v0/sandboxes/requests/{id}`, gated on `sandbox.read`, answering `200` with the record it
removed. It writes nothing: the queue loses the ask, the chain keeps it.

**Why**: §9 says M's state lives outside M — the chain, the instance table and the approval
slot are where the truth is, and M's memory is a cache. §82 answered the instance table with a
derivation, because a live handle cannot be persisted; the queue is the same shape of problem
with a different answer available, and picking the *same* answer keeps one rule: **why does
this survive a restart? because the chain says so**. A second file would put a second source
of truth beside the chain, which is the drift §82 was written to avoid, and it would need its
own version constant, its own migration and its own "newer file" story for a `Vec` the chain
already describes. The one thing the chain does not carry is `reason` (the ask row's detail
was always `{id, action, sandbox}`), and that is accepted rather than patched: a reason is a
human's note, not the state a decision needs, and putting it into `m.request.ask`'s detail
would add a field to a row the hash formula covers — a risk not worth a note. On TTL: §36 says
there is none, and this batch keeps that. It adds the **other** thing §36 left out — an
explicit cleanup — and the distinction is the mechanism: nothing expires on its own (no
sweeper, no clock), while a caller who is done with an ask takes it away. That is compatible
with "no TTL", not a repeal of it.

**Impact**: `host-core` gains `ReconciledRequest` and the pure `derive_requests_from`,
`SandboxRequests::restore` (which reports collisions rather than resolving them) and
`SandboxRequests::remove`; `AppState` seeds the queue in its constructor, best effort, and says
so with a `host.sandbox_request.restore` row **only when it restored something or an id
collided**. `server` gains one route — the third path under `/v0/sandboxes/requests/`, a
pattern matched before a definition name can see it, and the first `DELETE` there — plus
`Action::SandboxRequestDelete` and one dispatch arm. `docs/control-plane-api.md` §5.2 and
§5.4, `docs/tool-schema-control-plane.md` §2 and §3.3 and its checker's naming table
(`sandbox_request_delete`) all follow. **No hash formula, no audit event constant, no `m.*`
action name and no existing route changed**; the queue's stream has no "removed" frame,
because inventing one would be a new event for a caller that can simply re-read the queue.

## 85. The audit read pages with a cursor, and the order is the filter's

**Date**: 2026-09-28 ｜ **Status**: Decided; landed with the v1.0 gap 3/N batch E

**Decision**: `GET /v0/audit/events` gains one optional parameter, **`before_id`** — the newest
`limit` rows **strictly** older than the id it names — and `EventFilter` gains one **additive**
field, `descending: bool` (default `false`), which flips the store's `ORDER BY id ASC` to
`DESC`. `limit` stays required and unchanged, and `before_id` together with `to_id` is a `400`
naming `before_id` rather than a silent preference for one of them.

**Why**: The endpoint answers **newest first** (`AppState::list_events` reverses the store's
chain order), so "the next page" is *older* rows — but `LIMIT` applies to the scan, and an
ascending scan keeps the **oldest** rows of the window. That is the whole gap: `to_id` +
`limit` cannot express "the rows just before X", whatever the caller does with the answer, and
raising `limit` only widens the oldest end. So the order had to become something the caller
chooses. Two shapes were possible: `after_id` (which the ascending order already serves) or
`before_id` (which needs the other end). `after_id` was rejected as `from_id` under another
name — and `from_id` is already there, inclusive, for a reader that must not miss anything.
The cursor direction follows the surface's own order, not the store's. On the mechanism: the
flip went into `EventFilter` as an additive field rather than a second method, because one
field carries the whole meaning ("which end does `LIMIT` keep") and the derived `Default`
leaves every existing caller byte-identical; a `list_desc` would be a second name for one
query. The `before_id` + `to_id` refusal is deliberate: they are the two ends of the same
question, so a request that sends both has contradicted itself, and quietly picking one would
make an ambiguous read look like a successful one.

**Impact**: `audit/src/store.rs` (`EventFilter.descending`, `list`'s `ORDER BY`);
`host-core/src/state.rs` (`list_events` reverses only when the caller did not ask for the other
end); `server/src/routes.rs` (`audit_window` reads `before_id`, maps it to
`to_id = before_id - 1` with `descending`, and refuses the conflict). `docs/control-plane-api.md`
§5.1 and §5.4, `docs/control-plane-client-guide.md` (its "there is no offset or cursor anywhere
in v0.9" sentence was true when written and is not any more), `docs/tool-schema-control-plane.md`
and its definitions block all follow. **No hash formula, no audit event constant, no `m.*`
action name, no other route and no `limit` semantics changed** — `limit=0` still answers an
empty array, and `limit` is still required.

## 86. The freeze level is on disk, and the stability policy has passed the red lines

**Date**: 2026-09-28 ｜ **Status**: Decided; landed with the v1.0 M1 batch

**Decision**: The six things [roadmap §6](roadmap-v1.0.md) requires before the API may be called
frozen are on disk, and the first of them has been tested the way that section demands. Five were
already written (`docs/api-compatibility.md`, `docs/error-model.md`, `docs/security-model.md` and
`SECURITY.md`); the sixth — the **written upgrade procedure** — is `docs/upgrade.md`, the file
§14 has been naming as the place since 2026-09-22. And the stability policy now carries its
**red-line test** as `docs/api-compatibility.md` §9: each of [roadmap §1](roadmap-v1.0.md)'s four
constraints is checked against the policy's own clauses.

**Why**: Roadmap §6 is a gate, not a reading list: "the API may not be declared frozen before
these six are written down." Five of six is not a gate that has been passed — and the sixth had
been *referenced* for weeks, which is worse than missing: two documents pointed at a file that did
not exist, so a reader following the trail found nothing. Writing it in the form the decision
already implied — a procedure an integrator follows, citing the rules rather than restating them —
closes that. The red-line test is the other half, and the half that matters: the roadmap makes the
four constraints **the test the stability policy has to pass**, and a test that was never run is a
claim nobody checked. Running it also produced a useful result — the policy passes because of its
own shape (additive-only in a minor release, a major version for anything that changes a meaning),
so the test is a property of the document rather than a promise about future batches.

**Impact**: `docs/upgrade.md` + its translation (new; both listed in `docs/README.md`'s map);
`docs/api-compatibility.md` §9 (new) plus two corrections there — the capability count in §2
(**32** → **33**, after the v1.0 gap 3/N clean-up) and §7's "it does not exist yet", which now
points at a file that is there; `docs/roadmap-v1.0.md` §6's row 2, whose decision column said §14
while the decision describing data migration is **§11** (§14 is the upgrade path, and row 5's
citation of it was already right). **No source file, no gate and no check changed** — M1 is a
milestone about writing down what is already true. The dead-link gap this exposed (no checker
resolves a relative link, so the dangling `upgrade.md` reference was invisible) is recorded as
technical debt rather than fixed here.

## 87. The plugin interface is frozen

**Date**: 2026-09-28 ｜ **Status**: Decided; landed with the v1.0 M3 batch

**Decision**: The sandbox plugin interface is frozen as [`docs/plugin-interface.md`](plugin-interface.md).
The shape §3 settled and the bullets [roadmap §8](roadmap-v1.0.md) lists become a specification a plugin
author implements against: the transport, the four mandatory mechanism operations and their frame
grammar, the two optional semantics operations, the capability declaration's framework, the manifest's
required keys, the error and version rules, the architecture-independence constraint and the trust
model. Two things stay **open on purpose**: the capability declaration **format** — a draft, because it
is what a plugin author implements first and the part most likely to need one revision once a real
plugin exists ([roadmap §8](roadmap-v1.0.md) says it should be frozen last) — and the **architecture
abstraction**, where the *requirement* is frozen and the trait is v1.x work.

**Why**: §3 requires the plugin interface to freeze before the kernel API, and gives the reason: an
interface reshaped after the cross-device work is work done twice. Two things made the freeze doable
now rather than later, and both are about honesty rather than completeness. First, the shape was
already settled (§3, §4, §5) — what was missing was a document a plugin author could implement
against, and the frame grammar was the only genuinely new design work. Second, the two open items were
**named as open** rather than guessed at: the capability declaration format had been a **circular
reference** — roadmap §8 said the draft was in §14.11, and §14.11 said the draft was in §8, so nothing
was written anywhere — which is the one failure mode a freeze must not have; and the architecture trait
does not exist, so freezing one before a plugin does would invent an implementation contract out of
nothing. The freeze also inherits a rule from the rest of the project: "frozen" means what
[api-compatibility.md](api-compatibility.md) §1–§3 says it means, so this interface grows additively
and a meaning-changing move is a major version.

**Impact**: `docs/plugin-interface.md` + its translation (new; both listed in `docs/README.md`'s map);
`docs/roadmap-v1.0.md` §6's row 1 now cites §86 for the API stability policy (it carried a `—` while
§86 already existed); `docs/README.md`, `CHANGELOG.md` and `handoff.md` §1 follow. **No source file, no
capability name, no audit event constant, no route and no hash formula changed** — M3 is a design, and
§3 already says the implementation is v1.x. The two open items are tracked here rather than in a TODO:
the declaration format is settled before v1.x implements anything, and the abstraction's trait is
designed once a non-RISC-V plugin exists to design it against.

## 88. M4 is five pieces, and its first — identity and signing — is frozen

**Date**: 2026-09-28 ｜ **Status**: Decided (the split, and M4a's protocol); M4a–M4e unimplemented

**Decision**: Layer two ([roadmap §4](roadmap-v1.0.md)) is built in **five pieces**, and the first is
frozen now, as a document: [`docs/connection.md`](connection.md). **M4a** is node **identity and
signing** — an Ed25519 key pair in `<data-dir>/node.key` (or the keyring), one JWK whose **first
member** is `schema_version`, minted on the first start that has networking configured, and `@` as
address **plus** a signature over the canonical JSON of `{v, from, to, ts, body}`. **M4b** discovery,
**M4c** rooms, **M4d** the cross-region server's four roles and **M4e** audit digests follow as their
own batches, each written before it is built. The **temporary centre**, `provisional` and `fork` are
**not M4's** — §33 puts them behind audit v2, so they belong to M5/M6. The code lands in a **new
crate** (`net/` or `connection/`), never inside `server`: the control plane's HTTP face and the
node-to-node protocol are two protocols, and the workspace's dependency direction (`agent → sandbox →
audit`) must not be reversed to fit one into the other. `node.key` and `peers.json` are registered as
**new persisted formats** in [api-compatibility.md](api-compatibility.md) §6 and
[upgrade.md](upgrade.md) §2 (`rooms.json` is M4c's to register).

**Why**: Layer two is the first thing this project builds from nothing — the reconnaissance found no
network code, no key material and no parsing for any of the three files, only the decisions and the
seams. A milestone that size cannot be one batch, and the split follows the **dependencies** rather
than the file list: nothing can be signed before there is a key (§2), nothing can be discovered
before a node has an identity to be discovered by (M4b), rooms are rules *about* signatures (M4c), and
the cross-region server is where a signed message's transport is decided (M4d). Freezing **M4a alone**
follows §3's own logic one level down: the kernel API cannot freeze before the plugin interface, and
the connection protocol cannot freeze before the identity everything else signs with. Two choices
inside M4a are worth recording because both were live alternatives. **JWK rather than PEM**: §13
allows either, but §11 requires `schema_version` **first**, and a PEM's first field is its `BEGIN`
line — so a versioned PEM would need a container, which is a second format. **A key pair *beside* the
`AgentId`, not instead of it**: the process identity carries a pid and dies with it, while a node's
key must outlive every restart — so `node_id` stays the **device name** the `AgentId` already begins
with, and the key is the thing that proves it.

**Impact**: `docs/connection.md` + its translation (new; the specification, with §1–§3 written and
§4–§7 named as deferred); `docs/api-compatibility.md` §6 and `docs/upgrade.md` §2 gain two rows
(`node.key`, `peers.json`), and their credential row drops the `node.key` mention it had carried since
M1; `docs/README.md`, `CHANGELOG.md` and `handoff.md` §1 follow. **No source file, no dependency, no
capability name, no audit event constant and no hash formula changed** — M4a's freeze is a document,
and the Ed25519 dependency belongs to a later batch. §8's two constraints and §9's trust model restate
what the rest of the project already holds: nothing here names an architecture, and a signature
authenticates where a capability authorises.

## 89. A signed message travels as a line over TCP, and replay is bounded by a per-peer window

**Date**: 2026-09-28 ｜ **Status**: Decided; M4a's §3 open items closed (unimplemented)

**Decision**: §3 of [`docs/connection.md`](connection.md) left two things open, and both are now
frozen as that file's **§3.1** and **§3.2**. **Transport**: a signed message is **one JSON line over a
TCP connection** — the discipline `worker` and the plugin interface already use — sent **direct first**
and, when that fails, handed to the **relay**, which is the main path; the frame is **byte-identical on
both paths**, which is what keeps the relay stateless; the connection is plaintext and its integrity
comes from the signature, with no bearer token between peers; and there is **no separate handshake**,
because `v` is checked per message. **Replay protection**: a window of **five minutes behind and one
minute ahead**, enforced with a **per-peer, in-memory high-water mark plus the set of body hashes
accepted at that mark**, where advancing the mark discards the set — no timer, no sweeper. The record
is **not persisted**, and the exposure that leaves — a message still inside the window can be replayed
once across a restart — is written down rather than hidden.

**Why**: Both items had to close here rather than in M4d, because M4b, M4c and M4d all exchange signed
messages, and a signature protocol without a transport is not a protocol. The choices follow seams that
already exist. **TCP and JSON lines**: [decisions §7](decisions.md) already names point-to-point TCP
for a dispatch, and one-object-per-line is what `worker` and [plugin-interface.md](plugin-interface.md)
parse — reusing it gives this project one wire grammar instead of two. **The frame identical on both
paths**: [roadmap §4](roadmap-v1.0.md) calls the relay stateless, and the signature is what makes that
true rather than aspirational — a relay that alters a frame breaks the signature, so the "stateless
bridge" is enforced by the cryptography instead of promised. **−5 / +1 minutes**: [decisions §33](decisions.md)
allows 30 s – 2 min of silent retries before anything escalates, so a shorter window would refuse the
very retries the design depends on; the forward minute is clock skew and no more, because a longer
future is somewhere to hide a forgery. **A high-water mark rather than a bag of hashes**: it bounds the
record by "one timestamp's worth from one peer", turns cleanup into a discard instead of a sweep, and
survives a key rotation by construction — the record belongs to the peer, and [decisions §13](decisions.md)
has several keys valid at once. **Not persisting it** is the honest choice for a transport defence:
persisting it would be a new on-disk format with a version marker and a migration, for a bound that is
already five minutes wide, so it stays open with the cost written beside it.

**Impact**: `docs/connection.md` gains §3.1 and §3.2; its §1 "not frozen" list shrinks to the port
numbers, the timeouts, the record's data structure and whether a later batch persists it; and its §6
(M4d) now says the relay's **routing** is what M4d decides, not the transport, which is fixed here.
`CHANGELOG.md` and `handoff.md` §1 follow. **No source file, no dependency, no new persisted format,
no capability name, no audit event constant and no hash formula changed** — and `partial`, one of the
error model's five categories, is explicitly **not** used at this layer, because one frame is one
message rather than a batch.

## 90. Discovery hands out addresses; it does not hand out trust

**Date**: 2026-09-28 ｜ **Status**: Decided; M4b's protocol frozen (unimplemented)

**Decision**: §4 of [`docs/connection.md`](connection.md) is written, so discovery is frozen the way
§3.1 was: **two sources**. The **in-network server** — a node with a role, not a new kind of process —
hands down a table whose **entries are exactly `peers.json` entries**, at startup, on reconnect and on a
change, stamped with a **generation**; the hand-down is an ordinary signed frame over §3.1's transport,
and the table is a **source, not an authority** (a node's own `peers.json` still wins, and a conflict is
**reported**). A **UDP broadcast** is the supplement: **one datagram carries one signed frame** whose
only permitted effect is to offer an address, and it is the only thing in this protocol that travels
over UDP. Room isolation is a **filter with a safe default** — adopt only when the announced rooms
intersect the receiver's configured rooms, and a node with none configured adopts nothing. And the rule
that keeps §9 intact: **an announcement refreshes an address; it cannot introduce a key** — a node the
receiver does not already know is reported for the deployer, and only the in-network server, whose role
is to hold the table, may adopt one.

**Why**: §3.1 froze how a frame moves and left the address question open, and every later piece (M4c,
M4d, M4e) needs it answered. Three choices are worth recording because each closes a hole somebody could
otherwise walk through. **"Refresh an address, never introduce a key"**: §9 already forbids
trust-on-first-use and reputation by address, and a broadcast that let an unknown node become a trusted
peer on its own say-so would have reintroduced both through a side door marked "discovery". **The table
as a source, not an authority**: a handed-down table is another machine's opinion, and a node whose
connectivity depends on somebody else's file being right fails when that file is not; keeping the node's
own `peers.json` authoritative and *reporting* conflicts is the rule
[plugin-interface.md](plugin-interface.md) §6 already applies to manifest sources. **A fixed broadcast
port**: it must reach a node that knows nothing yet, so it cannot itself be discovered, and a
configurable one would let two nodes on one link fail to see each other in silence. The room filter's
**default-deny** follows the project's own habit ([security-model.md](security-model.md) §4), and
letting the filter read a membership list whose **shape is M4c's** keeps this section from swallowing the
next one: M4b fixes the filter, M4c fixes the file.

**Impact**: `docs/connection.md` §4 gains §4.1–§4.3; its §1 list moves discovery from *deferred* to
*frozen*; and §5's deferred note now says M4b fixed the filter while M4c fixes the file. `CHANGELOG.md`
and `handoff.md` §1 follow. **No source file, no dependency, no capability name, no audit event
constant, no hash formula, and — because the peer port is a field of the existing settings file — no new
persisted format changed.** The transport's own open items (port numbers, timeouts) stay where §3.1 left
them; what M4b fixes is the **rule** that a node's peer port is configuration and the broadcast port is a
constant.

## 91. A room is membership plus rules, and membership never introduces a key

**Date**: 2026-09-28 ｜ **Status**: Decided; M4c's protocol frozen (unimplemented)

**Decision**: §5 of [`docs/connection.md`](connection.md) is written. `rooms.json` is **one JSON file**
with `schema_version` first — the shape [`node.key`](connection.md) and `peers.json` already use — holding
rooms, each with a `name`, a `members[]` of **`node_id`s** and a `rules` object. The rules are
[roadmap §4](roadmap-v1.0.md)'s three: **`rate`** = `{messages, window_seconds}`, **per member**;
**`mention`** = `"members"` or `"nobody"`, defaulting to `"nobody"`; and **`require_signature`**, whose
only legal value in v1.0 is `true`. `rooms.json` and `peers.json` are **two files with two authors** — a
room names *who*, `peers.json` says what a node *is*, and neither overwrites the other. **Membership is
configuration**: v1.0 has no join protocol. And the discovery filter's phrase "configured for a room" now
has a definition: `rooms.json` names the room **and** its `members[]` lists this node's own `node_id`.

**Why**: §4.2 froze a filter that read a room set and left the file to this batch, so closing it means
saying what a room **is**. Four choices carry the weight. **A member is a `node_id`, and membership never
introduces a key**: [decisions §13](decisions.md) already puts a node's key in `peers.json`, and a second
place a key could come from would be a second place trust could be granted — so membership names, and
`peers.json` proves. **`require_signature` cannot lower §3's floor**: §3 made a signature universal on
the peer path, so a room flag that could admit unsigned traffic would contradict a frozen section; it is
recorded as `true` and a `false` is **refused at load**, which keeps [roadmap §4](roadmap-v1.0.md)'s third
rule visible without letting a config file undo the signature. **Membership is configuration, not a
protocol**: a dynamic join would be a mechanism with its own authority question — [decisions §33](decisions.md)'s
territory — and inventing one here would settle by accident what belongs to the centre. **The `rate`
refusal maps onto `refused`**: [error-model.md](error-model.md) §4 already gives that category to a
deployer's policy and to a full queue, so no sixth category appears and its neighbours stay what they
were.

**Impact**: `docs/connection.md` §5 gains §5.1–§5.3; its §1 list moves rooms from *deferred* to *frozen*,
leaving §6–§7 as the only deferred sections; `docs/api-compatibility.md` §6 and `docs/upgrade.md` §2 gain
a **`rooms.json`** row; `CHANGELOG.md` and `handoff.md` §1 follow. **No source file, no dependency, no
capability name, no audit event constant and no hash formula changed** — M4c is a file shape and three
rules, and a room's members are read by the same discovery filter §4.2 already froze.

## 92. The cross-region server is a deployer's deployment, and it authorises by signature

**Date**: 2026-09-28 ｜ **Status**: Decided; M4d's protocol frozen (unimplemented)

**Decision**: §6 of [`docs/connection.md`](connection.md) is written. The cross-region server is a
**dedicated deployment of the same software**, **run by a deployer** — never by the project — and its
four roles are frozen together with what each may know: **signalling** (addresses, never payloads),
**relay** (carrying a frame it cannot alter), **management** (publishing a registry and room definitions
as a **source, not an authority** — §4.1's rule one level out) and **audit aggregation** (the role's
shape and place only; digests are §7's and wait on M5). **Routing** is on the signed `to` field and
nothing else; **authorisation is the §3 model** — a `node_id` the server knows, with a signature that
verifies — with **no new credential and no new capability**; a frame is forwarded **only** to a
destination the server also knows; "stateless" means **about the content** (who-is-where and the §3.2
replay record are kept, and both are transport facts); a direct connection takes the data path off the
relay; and the server **never dials a node**, which is why no hole punching is needed. A node reaches one
only through **its own `peers.json` entry plus a field in its network settings** — no new persisted
format.

**Why**: Two obligations shaped this. The first is [roadmap §1](roadmap-v1.0.md)'s red line against an
officially operated service: the four roles, and signalling and management most of all, could be read as
"the project runs a service", so §6.5 answers that question **in the document** instead of leaving it to
a reader's charity — the runner is stated, there is no project endpoint, every role is mechanism with the
choices left to the deployer, the registry is held to *source* rather than *authority*, and the server
issues no credential that would have to be taken away. The second is consistency with what the earlier
batches froze. **Authorising by signature rather than by a credential**: §3 established that peers share
no secret, so a relay wanting a token would need a second, parallel trust system — and every capability
question already has one home ([security-model.md](security-model.md) §4). **Routing only on `to`, and
only to a known destination**: `to` is inside the signature, so a relay cannot be deceived about where a
frame goes, and refusing an unknown destination is what keeps one sender's mistake from becoming
everybody's traffic. **The registry as a source**: §4.1 already refused to let a handed-down table
overrule a node's own `peers.json`, and §5 already kept membership local; a management role with authority
would contradict both. **No new persisted format**: the server is a peer, so its key lives in `peers.json`
where every other key lives, and only the *policy* choice — "this peer is my cross-region server" — needs
a settings field.

**Impact**: `docs/connection.md` §6 gains §6.1–§6.5; its §1 list moves the cross-region server from
*deferred* to *frozen*, leaving §7 as the only deferred section; `CHANGELOG.md` and `handoff.md` §1
follow. **No source file, no dependency, no capability name, no audit event constant, no hash formula and
no new persisted format changed** — M4d is a deployment shape, four roles and one routing rule, and the
digests it will collect are still M4e's to write.

## 93. The connection layer's code starts as a crate that depends on the chain, not the other way round

**Date**: 2026-09-28 ｜ **Status**: Decided; the first piece is implemented (identity on disk)

**Decision**: The connection layer is implemented in a **new crate, `net/`**, and its first piece —
[connection.md](connection.md) §2, the node's identity — is on disk. `net` depends on **`audit`** and
on nothing else in this workspace; `host-core` is what will depend on `net`. What landed: a
versioned-JSON loader (a `Versioned` trait, `VersionedLoad`'s four outcomes, and `TooNew` refused
rather than half-read) and `NodeKey` — the JWK file (`schema_version` first, `kty`/`crv`, 32-byte
`x`/`d` as base64url), minted from `getrandom` on the first start with networking configured, written
owner-only with `create_new`, and **never minted by a read**. New dependencies: **`ed25519-dalek` 2**,
**`base64` 0.22**, **`getrandom` 0.4** (already in the workspace, used by the bearer token).
[`audit::canonical_json`](https://github.com/breakevery/riscdom/blob/main/audit/src/run.rs) and
`audit::fingerprint` are **borrowed**, not re-implemented, for the public-key fingerprint.

**Why**: Three choices were live ones. **The crate depends on `audit`, not on `host-core`**: the
canonical bytes a signature covers are the chain's own definition, so `net` must sit *below* the host
— and a dependency on `host-core` would close a cycle the moment the host wants to load a `node.key`
at startup. **The version rule is implemented once, generically**: §2 requires `schema_version` first
for `node.key`, and §5.1 requires the same for `peers.json` and `rooms.json`; one loader means one
reading of *those* rules, and its `Missing`/`Current`/`Migrated`/`TooNew` shape is `LocalSettings`'s,
which the project has already exercised with a real migration. **A read never mints, and a mint is
owner-only and `create_new`**: the second half stops two processes racing to mint a key and one of
them winning silently, and the first is the rule §2 states — a node with no networking configured
gets no key at all, so nothing is created on a machine that never joins a network. Two smaller things
are worth recording because they are readings rather than restatements. **The file has no `node_id`**:
§2 separates the key pair from the *device name*, and the file holds key material — a `node_id` member
would have been a sixth JWK member §2 does not freeze. **The halves are checked against each other at
load**: `x` is re-derived from `d` and compared, so a spliced or hand-edited key file is refused when
it is **read**, rather than the first time something signs with it.

**Impact**: `net/` is a new crate (`Cargo.toml`; `src/{lib,versioned,identity}.rs`; an `identity`
example with `--self-test`; integration tests; a bilingual README); the workspace's `members` list
gains it; and the gate gains one step (`cargo run -p net --example identity -- --self-test`) beside
the existing example proof, with `net` also joining the `cargo clippy` list it would otherwise have
silently escaped. `docs/README.md`'s crate table gains a row. **No existing source file's logic
changed, and no hash formula, route, capability name or audit event constant was touched**: `net`
reads the chain's canonical JSON and hashes it, and writes its own file. Signing, the transport,
discovery, rooms and the cross-region server are the next pieces, each after the section it
implements is frozen.

## 94. A signed message is verified in the frozen order, and its refusals carry the error model's categories

**Date**: 2026-09-28 ｜ **Status**: Decided; M4a's signing and replay protection implemented

**Decision**: [connection.md §3](connection.md) and its §3.2 are implemented in `net/`.
`SignedMessage` is `{v, from, to, ts, body}` plus `sig`, and what is signed is the **canonical
JSON of those five members** — `audit::canonical_json`'s bytes, not a second serialisation.
`verify` runs the six steps in the frozen order — knows the sender, signature verifies, `v` is
spoken, `to` is this node, `ts` is inside the window, not a replay — and every failure answers
with a category from the error model: **`refused`** for an unknown sender or a replay,
**`invalid`** for a broken signature, an unsupported version or a message addressed elsewhere,
**`network`** for a stale or future timestamp. The replay record is `net`'s `ReplayGuard`: per
peer, in memory, a high-water mark plus the payload hashes seen at it, over **−5 min / +1 min**,
with the set discarded when the mark advances.

**Why**: Three points were live. **The version is inside the signed bytes.** §3 checks `v` at
step 3, *after* the signature — and that is the only order that tells "wrong version" apart from
"forged": a version checked first would answer the same way to a lie and to a real message from
a newer build. The test says so explicitly: a message **signed at** v2 is refused as a version,
while a v2 **edited into** a v1 message is refused as a signature. **`to` is `invalid`, and §3
did not say so.** The frozen sentence names three mappings and leaves step 4 out; it is filled as
`invalid` because a message addressed to another node is a wrong input rather than a policy
refusal — and the gap is recorded here rather than chosen in silence. **The record is keyed by
the peer, never by the key.** §3.2 says a rotation must not reset it, and the implementation
cannot get that wrong by accident: the map is `from → record`, and the keys live in a different
structure (`PeerKeys`, a **set** per peer, so the grey period's several keys all verify). The
test proves it the only way that matters: a message signed by a **new** key, at an old timestamp,
is refused as a **replay** — not as an unknown sender and not as a bad signature.

**Impact**: `net/` gains `src/message.rs` (the frame and the canonical bytes), `src/sign.rs` (the
six steps, `PeerKeys`, and `VerifyError`'s mapping to `Category`) and `src/replay.rs`
(`ReplayGuard`), a `sign` example with `--self-test`, and thirteen integration tests; the gate
gains its second `net` step. `net/README.md` moves §3 from "not here yet" to "here". **No
existing source file changed, and no hash formula, route, capability name or audit event constant
was touched** — and authorisation is deliberately absent: this batch answers *who sent this*, and
whether that node may do the thing stays the capability model's question
([security-model.md](security-model.md) §4), so nothing here is a permission check.

## 95. The transport is std TCP with one JSON line per message, and the relay is a seam

**Date**: 2026-09-28 ｜ **Status**: Decided; the direct path implemented

**Decision**: [connection.md §3.1](connection.md) is implemented in `net` with **`std::net`** — no
async runtime. `Connection` and `Listener` send and receive **one JSON line per message**; the
frame is serialised **once** (`SignedMessage::to_line`) so the direct and relayed paths carry
identical bytes; and `deliver` tries the peer's address **first** and hands the frame to the
`Relay` seam when that fails. The relay's *routing* is M4d's: this batch fixes the trait
(`fn forward(&self, frame: &str)`), one honest implementation of "nothing wired" (`NoRelay`), and
the direct path. Ports and timeouts are not frozen by §3.1 and live in `TransportConfig`.
Failures map onto the error model exactly as §3.1's table says: a refused or timed-out connect, a
cut-short frame, an over-long line and a missing relay are **`network`**; a complete frame that
does not parse is **`invalid`**; and **`partial` is not used**, because one frame is one message
rather than a batch.

**Why**: Three choices. **`std` rather than a runtime**: `sandbox/relay.rs` already runs a framed
protocol on std sockets, so a runtime would be a scheduler every caller pays for and few use — and
the plugin interface's transport is stdio, not TCP, so there is nothing to share with an async
stack anyway. **One encoder**: the byte-identity §3.1 requires is not a convention here but a fact
of the code — there is exactly one function that turns a message into a frame, and the relay
receives its output verbatim; the test asserts that the bytes the direct path wrote equal the bytes
the relay was handed. **A trait, not a stub that compiles and lies**: M4d owns routing, so `Relay`
is one method wide and `NoRelay` answers `RelayUnavailable` (`network`) rather than pretending a
frame went somewhere. Two smaller notes: `receive` refuses a line longer than a configurable cap
instead of allocating without bound, and that refusal is `network` (a transport that did not
deliver a frame) rather than `invalid`, because the cap is this build's limit and not a protocol
rule; and a read timeout arrives as `WouldBlock` on Unix and `TimedOut` on Windows, so both kinds
are matched — which a test on one platform only half-proves.

**Impact**: `net/` gains `src/transport.rs`, a `transport` example with `--self-test`, ten
integration tests, and `src/error.rs` (the error model's five categories, moved out of `sign.rs` so
the verifier and the transport map onto one definition); the gate gains its third `net` step.
`net/README.md` moves §3.1 from "not here yet" to "here", and lists M4d as what is still missing.
**No existing source file changed, no dependency was added, and no hash formula, route, capability
name or audit event constant was touched** — and the relay is deliberately inert: nothing in this
batch forwards a frame anywhere.

## 96. Discovery hands out addresses: a peer file that wins, a table that is a source, and a beacon that introduces no key

**Date**: 2026-09-28 ｜ **Status**: Decided; M4b implemented

**Decision**: [connection.md §4](connection.md) is implemented in `net`. `peers.json` is a
versioned JSON file (version 1) whose entries are exactly the `{node_id, addresses[], public_key,
capabilities, rooms[]}` shape a handed-down table carries, and it is **authoritative for the node
that owns it**; a `PeerEntry` carrying a `d` — a private key — is refused by name, because a peer
table is a file that gets copied between machines. A handed-down table (`NodeTable`) travels as an
ordinary signed frame, carries a **generation**, and is merged as a **source**: `merge_table`
answers with the merged view plus a `MergeReport` whose `conflicts` name both sides — local wins,
and nothing is resolved in silence. The beacon is one datagram carrying one signed frame whose
effect is to offer an address: `consider_announcement` answers `RefreshedAddresses` for a node
already known, `ReportedUnknown` for one that is not, and `IgnoredOutOfRoom` when the announced
rooms miss the receiver's. The room filter is **`RoomFilter`** — an intersection test with
**default deny**, reading a set of room names (the `rooms.json` *file* is M4c's). The broadcast
port is a **protocol constant** (`BROADCAST_PORT = 47821`), not a setting.

**Why**: The three rules §4.2 states are the interesting part, and all three are about not growing
a second trust path. **The file wins, the table is a source**: a handed-down table is another
machine's opinion, so merging it must not be able to change what a node believes about a peer it
was configured with — hence a merge that *reports* rather than resolves, the rule
[plugin-interface.md](plugin-interface.md) §6 already applies to manifest sources. **An
announcement refreshes an address, it cannot introduce a key**: §9 forbids trust on first use, and
anyone on the link can send a datagram — the signature proves who wrote it, not that the receiver
should care — so an unknown node's beacon is reported to the deployer and adopted by nobody.
`Adoption::introduced_a_key()` is a method that can only return `false`, and that is the point: the
type makes the invariant visible at the place a future change would have to argue with it. **A
constant port**: a broadcast must reach a node that knows nothing, so it cannot be discovered, and
a configurable port would let two nodes on one link fail to see each other in silence. The beacon
itself is "the only thing in this protocol that travels over UDP", kept honest by having exactly
one function that sends one and one that receives one, both reusing §3's frame.

**Impact**: `net/` gains `src/peers.rs` (the file, the entry, and the public-JWK rule that refuses
`d`), `src/discovery.rs` (the table, the merge and its conflict report, the beacon and
`RoomFilter`), a `discovery` example with `--self-test`, and seventeen tests; the gate gains its
fourth `net` step. `net/README.md` moves §4 from "not here yet" to "here" and leaves §5 and §6.
**No existing source file changed, no dependency was added, and nothing in the persisted-format
table moved** — `peers.json`'s row was added when the format landed (M4a-impl-1), and this batch
only reads and writes what that row describes. `rooms.json`'s file, the relay's routing and the
cross-region server are M4c's and M4d's.

## 97. A room names who; `peers.json` says what a node is — and the filter reads membership

**Date**: 2026-09-28 ｜ **Status**: Decided; M4c implemented

**Decision**: [connection.md §5](connection.md) is implemented in `net`. `rooms.json` is a
versioned JSON file (version 1) whose rooms are `{name, members[], rules}`, with members being
**`node_id`s** and rules being §5.2's three: `rate` = `{messages, window_seconds}` **per member**,
`mention` = `"members"` | `"nobody"` (**default `"nobody"`**), and `require_signature`, whose only
legal value is `true`. Loading refuses a `false` **by name**, refuses a rate of zero messages or a
zero-second window, refuses two rooms with one name, and refuses an empty member. `RateCounters`
keeps the per-member, per-room budget **in memory** and answers "over budget" with `Refused`.
`Room::allows_mention_from` is membership first and the setting second. And
`RoomFilter::from_rooms(&RoomsFile, this_node_id)` is where M4b's filter meets this file: the rooms
the file **names** and whose `members[]` **lists this node**.

**Why**: Two rules carry the weight, and both are about not letting a second source of truth appear.
**A member is a `node_id`, and `rooms.json` and `peers.json` are two files with two authors.** §5.1
says a room's `members[]` is what the deployer says, while a `peers.json` entry's `rooms[]` is what
that node *claims about itself* — so a room holds membership and never identity, and neither file can
overwrite the other. That is also why membership **never introduces a key**: a name here is a name,
and the key is wherever `peers.json` says it is. **The signature flag cannot lower §3's floor.** §3
makes a signature universal on the peer path, so a loadable `require_signature: false` would be a
config file quietly undoing a frozen section; refusing it at load — rather than ignoring it — is what
keeps the field honest, and the refusal names the field so an operator sees why. Two smaller choices:
the **rate budget is per member**, because a room-wide budget would let one member starve the others,
and over budget is `Refused` because [error-model.md](error-model.md) §4 already gives that word to a
deployer's policy, so no sixth category appears; and **membership is configuration**, with no join
protocol, because a dynamic one would be a mechanism with its own authority question
([decisions §33](decisions.md)) that this batch must not settle by accident.

**Impact**: `net/` gains `src/rooms.rs`, a `rooms` example with `--self-test`, and seven integration
tests; `RoomFilter::from_rooms` closes the loop M4b left open, so the beacon filter now reads the
deployer's file instead of a set handed to it by a test; the gate gains its fifth `net` step; and
`docs/api-compatibility.md` §6 and `docs/upgrade.md` §2 gain a **`rooms.json`** row. **No existing
source file changed, no dependency was added, and no hash formula, route, capability name or audit
event constant was touched** — and authorisation is still absent, as in every `net` batch: this crate
answers *who sent this* and *what the room permits*, and whether a node may do a thing stays the
capability model's question.

## 98. The relay carries a frame down the destination's session, because the server never dials

**Date**: 2026-09-28 ｜ **Status**: Decided; M4d's relay implemented

**Decision**: [connection.md §6](connection.md)'s **relay** role is implemented in `net`, together with the
session it needs. `RelayServer` parses a frame, authenticates its sender against its own `peers.json` —
§3's steps 1–3 and §3.2's record, and **not** step 4 — routes on the signed `to` and nothing else, and hands
the frame down the destination's session **in the bytes that arrived**. A frame addressed to the server
itself is `Routed::Local` (the signalling and management roles' business, next batch); a destination the
server cannot place is refused rather than broadcast; and a destination that is known but **not dialled
in** is refused too, because §6.3 has the server wait to be dialled and **never dial a node**.
`SessionTable` is that knowledge — `node_id → the socket it dialled in on` — which is §6.3's "who is
where" read as a transport fact. `RelaySession` / `RelayClient` are the node's half, and `hello_body()` is
the one wire shape this batch invents: an ordinary signed frame addressed to the server, carrying the
protocol version, which is how a session says who it is and how a node with nothing to send yet can still
be found. `src/bin/riscdom-relay.rs` is the deployer's program — `--data-dir`, `--bind` and `--node-id`, no
default address, and a banner that states the runner, the storage rule and the no-dial rule. **A hello does
not consume the sender's §3.2 record**, and that is a rule rather than an oversight.

**Why**: Four choices carry this. **A session, not a dial-back**: §6.3 says the server never dials a node
and that "both sides dial out", so a relay that reached a destination at its `addresses[]` would contradict
the sentence and bring back the inbound-path problem the no-hole-punching claim is made of; the destination
holds the connection open and the frame goes down it. **Step 4 is dropped, and only step 4**: §6.3 names
steps 1 and 2, and a relayed frame is addressed to somebody else by design — so the relay and the receiver
now share `check_identity` (steps 1–3) rather than one calling the other, and the difference between them is
exactly one question, written where it is answered. **A hello the record cannot see**: a session is opened
*after* the frame it is about to carry was signed — the failed direct attempt is what comes just before it
— so a record the hello advanced would make the relay refuse the very frame it was handed, for being older
than the hello that carried it there. It was found by a failing test rather than by review, which is the
argument for the record being a rule with a test rather than a line of code. And **§6.5's answer is made
checkable in the program**: the banner states that a deployer runs it, that `--bind` is required so the
software names no address, and that no message is stored; two tests hold the rest — an unknown sender gets
no session, and a destination with a live, reachable address hears nothing until it dials in.

**Impact**: `net/` gains `src/relay.rs` (`RelayServer`, `SessionTable`, `Forwarder`, `RelayError`,
`route`, `RelaySession`, `RelayClient`), `src/bin/riscdom-relay.rs`, a `relay` example with `--self-test`,
and seventeen tests; `sign.rs` grows `check_identity` (steps 1–3, shared) and `authenticate_forwarded`
(steps 1–3 plus §3.2, without step 4), and `transport.rs` grows `frame_bytes` and
`Connection::writer_clone` / `set_read_timeout` — refactors with no behaviour change to §3 or §3.1; the
gate gains its sixth `net` step. `net/README.md` moves §6's relay from "not here yet" to "here". **No new
dependency, no new persisted format, no new capability name, no audit event constant, no hash formula and
no route was touched** — the server keeps no message store, draws its authority from §3 and issues no
credential, and signalling, management and §7's digests are still unwritten.

## 99. The server answers where a node is and what it publishes, and the answer is a source

**Date**: 2026-09-28 ｜ **Status**: Decided; M4d's signalling and management implemented

**Decision**: [connection.md §6.2](connection.md)'s other two roles are implemented in `net`, on the
same server and the same session the relay uses. A frame addressed to the server itself is now read by
`Local::of` into one of three questions: a **hello** (bind the socket, answer nothing), an **address
query** (`{"query": "<node_id>"}`), or a **registry request** (`{"registry": 1}`); anything else is
`Unrecognised`, and nothing at all is answered for it. **Signalling** answers `{"addresses": [...]}` —
the address the node dialled in from (its live session) first, then its `peers.json` entry's addresses,
de-duplicated — and a node the server cannot place is answered with an **empty list** rather than
refused. **Management** answers with a `Registry`: §4.1's `{generation, peers}` hand-down with the room
definitions beside it, signed by the server's own key (§6.4 makes it a peer). A node reads them back as
`Answer` and applies `Registry::merge` — `merge_table`'s rule from M4b, for the peers, and
`merge_rooms`, the same rule for room names — so the node's own `peers.json` and `rooms.json` win and
every disagreement is **reported**. The generation advances whenever the registry the server publishes
changes (`set_peers`, `set_rooms`). Both answers are §3 frames, so a node verifies them with a key it
already holds, and the authorisation is §3's model in both directions: no new credential, no new
capability.

**Why**: Four choices carry it. **The answers' shapes are this batch's, and §6 left them so**: §6 freezes
the roles, the routing rule and what each role may know — not the bytes — so `{"query"}`,
`{"registry"}` and `{"addresses"}` are recorded here rather than assumed, exactly as §98 recorded the
hello. **Signalling reports two addresses, and both are transport facts**: §6.2 has signalling answer
where a `node_id` "can be reached ... from what nodes have told it", and the two things the server
knows are the entry its deployer wrote and the socket the node just dialled in on. The live address
comes first because it is the more current, the list is de-duplicated, and nothing but addresses
crosses — "never payloads" is held by the answer body having exactly one member. **An unknown node is
answered, not refused**: the question was well-formed and authenticated, so "nowhere I know" is the
honest answer, and §6.3's `refused` is for *you may not ask* rather than *I do not know*. **A published
registry is a source**: §6.2 says so, and the implementation makes it structural — `merge` never
replaces a local entry, a room that differs keeps the local definition, and a published room set is
held to `rooms.json`'s own checks, one implementation with two reporters (a file read says `Shape`, a
published set says `RoomsError::Room`), so a source cannot carry a room a file would refuse. The table
is assembled per request rather than stored, so what is published cannot drift from the table the
server routes against.

**Impact**: `net/` gains `src/registry.rs` (`Registry`, `Merged`, `registry_request_body`,
`registry_category`) and `src/relay.rs` gains signalling (`address_query_body`, `address_answer_body`,
`Local`, `LocalReply`, `Answer`), the local dispatch in `serve_connection`, and — because answering
requires signing — a **key** and a **room set** on `RelayServer` (`new` takes both, `set_rooms`
publishes, the generation advances); `src/rooms.rs` gains `merge_rooms`, `RoomConflict`,
`RoomMergeReport`, `Room::summary` and `RoomsFile::check` (the load rules, callable on their own);
`src/bin/riscdom-relay.rs` mints or reads `<data-dir>/node.key` and publishes `rooms.json`; the
`relay` example grows four checks (thirteen in all) and `tests/server.rs` adds eight integration tests.
**No new dependency, no new persisted format, no new capability name, no audit event constant, no hash
formula and no route was touched** — the server still stores no message, draws its authority from §3
and issues no credential, and §7's digests are unwritten.

## 100. The host loads the connection layer's files — when the network settings say so

**Date**: 2026-09-28 ｜ **Status**: Decided; the wiring shipped (V-1)

**Decision**: `host-core` depends on `net` and loads the connection layer's three files at start-up:
`<data-dir>/node.key` ([connection.md §2](connection.md)), `<data-dir>/peers.json` (§4) and
`<data-dir>/rooms.json` (§5). One condition decides whether any of them is touched —
**`settings.network.is_some()`** — and an unconfigured node reads nothing and **grows no key**, which
is what §2 asks for and what every version before this one did. The host keeps what it loaded on
`AppState` (`node_key()`, `peers()`, `rooms()`, each answering a clone) plus `connection_problem()
for the first refusal, in the shape `settings_problem()` already has. A `peers.json` or `rooms.json`
that is **absent** is normal (a node may know nobody and be in no room); a file from a **newer** build
is refused and **nothing is written over it**; anything else is reported. Two names join the audit
vocabulary: **`host.connection.key_minted`** (`{node_id, fingerprint}`) and
**`host.connection.data_too_new`** (`{file, found, supported}`). `net` itself is untouched.

**Why**: Three choices carry it. **The condition is the settings, and it is one line.** §2 ties a key
to "the first start that has networking configured", and the settings are where that is already
written down: `NetworkSettings` is an `Option`, so `is_some()` *is* "this node is on a network".
Nothing new is persisted, `SETTINGS_VERSION` does not move, and a node that never joins a network has
no key file to leak. **The format knowledge stays in `net`, the policy stays here.** Every read goes
through `net`'s own loaders — the version rules, the four outcomes, the refusal of a newer file —
and this batch only decides *when* to read and *what to say*; that is [§93](decisions.md)'s direction
applied one layer up, and it is why the host grows no parser. The one thing worth naming is that
`net`'s `load_or_create_in` answers the **key**, not which of "read" and "minted" happened — so
whether the file existed first is what tells the host a mint happened, and a too-new key reaches the
same error channel as a broken one, so the refusal is read once more to name the case. **A refusal is
visible, and the two refusals are named apart.** A mint writes a row — a silently created key is a key
no deployer can put in the other nodes' `peers.json` — and a too-new file writes one naming the file
and both versions, the shape `host.settings.data_too_new` already has. A file that is unusable for
any other reason (**a spliced JWK, a peer entry carrying a private key, a room that lowers §3's
floor**) is logged and readable through `connection_problem()` but deliberately **not** recorded under
the too-new name: one event name meaning two things is worse than one name and an honest log. Nothing
here is fatal, for the reason the keyring degrades silently: a host that will not start because a peer
table is malformed is a host that cannot be repaired.

**Impact**: `host-core/Cargo.toml` gains `net = { path = "../net" }` (no new package — the lock file
gains one edge line, 534 packages before and after); `src/connection.rs` is new (the loader, the three
`ConnectionFile` names, `ConnectionProblem`, `ConnectionFiles` and three unit tests); `src/state.rs`
gains four fields, four accessors and `load_connection_files()` — called after `load_settings()` by
`AppState::new` and `AppState::with_data_dir`, and deliberately **not** by `AppState::in_memory`, which
keeps the process-wide data directory and must never mint into it — and it is where the two rows are
written; `src/lib.rs` re-exports the new type; `tests/connection.rs` adds seven integration tests
(nothing configured, a mint that happens once, files that load, a too-new `peers.json` / `rooms.json` /
`node.key`, and an unusable table that is *not* reported as too new); `host-core/README.md` gains the
module and the constraint. **No hash formula, route, capability name or existing audit event constant
changed, `net`'s logic is untouched, and nothing is persisted that was not already** — the two new
names live where `host.settings.data_too_new` already lives (this document, `CHANGELOG.md` and
`docs/handoff.md`), and *not* in `docs/control-plane-events.md`, whose twenty names are the **stream**
events that document normalises and whose count that document and its guard test both assert. V-2 (the
cross-region pointer and the client) and V-3 (the upper surfaces) are still to come.

## 101. A node registers upward and heartbeats, at two levels and in one shape

**Date**: 2026-09-28 ｜ **Status**: Decided; the protocol is frozen (V-proto-1), unimplemented

**Decision**: [connection.md §6.6](connection.md) is written. A node **registers** with the server it was
configured with — an ordinary §3 frame addressed to that server, verified by §3's six steps, whose body is
`{"register": 1, "addresses": [...], "capabilities": [...], "rooms": [...]}` — and then **heartbeats** it
every **15 seconds** with the smallest frame there is, `{"heartbeat": 1}`. The server keeps an
**online-status table** per node it knows: `node_id`, the addresses the node last reported,
`last_heartbeat_ms`, and a `state` that is `online` while `now − last_heartbeat_ms ≤ 45 s` and `offline`
after. A row is created by a registration, refreshed by a heartbeat, and **never deleted by going
offline**. The same frames serve **both levels** — a node to its in-network server, and an in-network
server to the cross-region server above it — and an in-network server registers as **itself**, not as
itself plus the nodes below it. Joining is **configuration**: an administrator adds the node to the
server's `peers.json` before it can register, and there is no automatic approval. This finishes §6.2's
model in its second direction: **management is what a server hands down, §6.6 is what a node reports up**,
and the deployer's files stay authoritative while what a node reports is a **source**.

**Why**: Four choices carry it. **Registration is a §3 frame, so it needs no permission.** §6.3 already
says who may ask — a node the server knows, with a verifying signature — and a registration is that same
question one level up: it authenticates, it authorises nothing, and no capability is added. That also makes
"a key cannot arrive by frame" **structural** rather than promised: the server can only accept a
registration from a node whose key it already holds in its own `peers.json`, so §4.2's and §9's rule
arrives on its own. **The heartbeat is not the session opener.** `{"hello": 1}` binds a socket; a
heartbeat says a node is still *there* — different questions, and a node can hold a session open and still
be gone — so coupling them would make the transport answer a presence question that the liveness work
(V-proto-2) would inherit. **15 s and 45 s are chosen against §3.2, not for comfort**: the backward window
is five minutes, so a delayed beat is a beat rather than `stale`; 15 s is not so fast that a fleet's beats
dominate a link; and it is a third of the offline threshold, which is what lets three missed beats be the
threshold. The **ratio** is frozen and the numbers are v1.0's defaults, of the same kind §3.1's timeouts
are. **An in-network server reports itself alone**, and the first reason is arithmetic rather than policy:
the top server can only verify frames from keys it holds, so a frame naming the nodes below would ask it to
trust identities that arrived by message — §4.2's forbidden thing — and it could not verify one of them. The
addresses below are also LAN addresses, of no use to a remote peer; and identity comes from configuration
everywhere else in this document (§5.2's membership, §6.4's server entry). One rule, applied once more.

**Impact**: `docs/connection.md` gains **§6.6** — its ten H2 sections stay **ten** (§6 keeps §6.1–§6.5, so
every existing citation of §6.x and §7 still points where it did) — plus one sentence in the introduction
and two clauses in §6's closing summary; `CHANGELOG.md` and `docs/handoff.md` §1 follow, bilingual as
always. **No source file, no dependency, no capability name, no audit event constant, no hash formula, no
route and no new persisted format changed** — this is a section of a frozen document written after the
freeze. V-proto-2 (liveness: collective confirmation, an in-network server noticing its own loss, and what
follows the judgement) and V-2 (the pointer and the client) are the batches that will use it.

## 102. A node is judged gone only by unanimity among the witnesses that remain

**Date**: 2026-09-28 ｜ **Status**: Decided; the protocol is frozen (V-proto-2), unimplemented

**Decision**: [connection.md §6.7](connection.md) is written. **A node's peers are its own workgroup**, and
each node **probes** them with an ordinary §3 frame — `{"probe": 1}`, answered `{"alive": 1}`, direct first
and through the relay when that fails, **every 15 seconds**, with **three consecutive unanswered probes
(45 s)** making the prober hold the peer *unreachable*. The prober **reports** its view upward —
`{"unreachable": "<node_id>"}` / `{"reachable": "<node_id>"}`, repeated every cycle while the view stands and
counted only while fresh — and the in-network server **judges**: a node is gone when there is **at least one
witness** (another node it knows, itself reachable, not the subject) and **every witness has reported it
unreachable**. A witness of life vetoes, and a node **alone** in its workgroup is never judged. **An
in-network server's own loss is confirmed by its siblings** — the other in-network servers, reporting the same
two bodies to the cross-region server, which judges by the same rule — and the nodes below it cannot do it
because they **share its LAN and its power feed**. After a judgement the row gains **`judged_at_ms`** (a fact
about reachability, kept apart from §6.6's heartbeat-based `state`) and the judging server writes
**`host.connection.peer_offline`** (`{peer, witnesses, reports}`) or **`host.connection.peer_recovered`**
(`{peer, method}`). **Nothing is removed**: the protocol defines no kick, no ejection and no drop.

**Why**: Five choices carry it. **A judgement is not a silence.** §6.6's `offline` is one observer's silence;
a judgement is everybody-who-can-reach-it's agreement, and only the second is strong enough to act on — so
they are different fields, not two values of one. **Unanimity, not a majority.** A failure has innocent
explanations and a success has none, so a witness of life vetoes and the rule can only be unanimity among
failures; and a majority would be wrong in exactly the case that matters — a partition, where half a
workgroup can reach a node and half cannot, and a majority would call a **live** node gone. **The witness set
is what keeps it honest.** A node that is down cannot report, and a node that is itself unreachable cannot
testify, so the threshold is unanimity *among those still able to speak* — which is also why a solo node is
never judged (nobody can testify), and why the question is not "how many reported". **Siblings, not
subordinates.** The nodes below an in-network server share its LAN and its power, so a silence that includes
the witnesses is not evidence: the confirmation has to come from peers that can be expected to survive it. The
mechanism is the same one level up, and it needs no new role — the cross-region server only **aggregates**,
because §6.3's rule that the server never dials binds it too. **Mechanism, not policy.** The kernel records
the judgement (the row, the event) and defines **no removal at all**: a judged node keeps its key, its
`peers.json` entry and its table row, and what a deployment does about it is the deployer's policy
([roadmap §1](roadmap-v1.0.md)'s red line, and the rule §5.2 and §6.6 already keep for joining). Two smaller
consequences are worth stating: **a prober writes no chain row** — a suspicion is not a fact, and one row per
node would make a partition write *"X is gone"* into half the chains — and **recovery is being heard from,
not re-admitted**, because nothing was taken away.

**Impact**: `docs/connection.md` gains **§6.7** — the ten H2 sections stay **ten**, and §6 keeps §6.1–§6.5's
numbering, so every existing citation still points where it did — plus one clause in §6's closing summary and
one in §6.6's not-frozen list (which had pointed at V-proto-2). `CHANGELOG.md` and `docs/handoff.md` §1
follow, bilingual as always. **No source file, no dependency, no capability name, no existing audit event
constant, no hash formula, no route and no new persisted format changed** — the two new names are additions to
the audit vocabulary, recorded where the `host.connection.*` family already lives (this document,
`CHANGELOG.md`, `docs/handoff.md`) and not in `docs/control-plane-events.md`, whose twenty names are the
**stream** events. V-2 (the pointer and the client) and V-3 (where a kick API would land) are next.

## 103. The pointer is a setting, the client is lazy, and the beat is a thread

**Date**: 2026-09-28 ｜ **Status**: Decided; V-2 shipped

**Decision**: A node reaches its cross-region server. `NetworkSettings` gains **`cross_region_server:
Option<String>`** — a `node_id` that must appear in the node's own `peers.json` (§6.4), additive with
`SETTINGS_VERSION` unchanged — and `host-core` wires what it names: `RelayClient::new` is built **without
dialling**, the session opens on first use (`connect`, the beat loop, or a later batch's `forward`), and a
**registration-and-heartbeat thread** registers once per session and then beats every **15 s** (§6.6's
interval) until it is stopped. On the server's side `net` grows the two frame types and the table:
`Local::Register` / `Local::Heartbeat`, an **`OnlineTable`** (`node_id → {addresses[], capabilities[],
rooms[], last_heartbeat_ms, state}`, `online` inside **45 s** and `offline` after, rows never deleted by
going offline), a registration answered with `{"registered": 1}` and a beat answered with **nothing**. A
pointer at a peer the node does not hold is **refused** and said so (`connection_problem`, the slot V-1
already had). The thread is `std::thread` + a channel, and **this batch writes no chain row**.

**Why**: Four choices carry it. **The pointer is a setting, and one the node can check.** §6.4 makes the
server a peer whose key lives in `peers.json`; a pointer at a peer this node does not hold could only
produce frames it cannot verify, so it is refused at load and the deployment runs without a wide-area lane —
the honest state §6.1 describes. It is additive and moves no version, like every field beside it. **A client
that dials when it is built is a client that can hold up a start-up.** The session is opened by the first
thing that needs it, so a server that is down, slow or not yet deployed is a beat that does not land rather
than a host that will not start — and a beat that does not land costs nothing, because the next one is 15
seconds later. **The thread is std, and its stop is a channel.** `net` has no async dependency and this crate
will not give it one for a sleep and a socket write; and a channel rather than a flag means dropping the
handle wakes the thread **at once**, so a host that stops beating stops within a beat instead of after
whatever interval it happened to be sleeping through. The thread holds the **client**, never an
`Arc<AppState>`: a thread holding the state would be a cycle that keeps it alive for as long as it beats,
which is exactly what must not happen when a host is dropped. **A beat is a statement, not a question.** The
server answers a registration — a node should know its row exists — and answers a beat with nothing: a beat's
answer would double a fleet's frames to say what the table already says, and the session staying open is the
transport's own evidence. And the two shapes are told apart by **one letter**: `register` is what a node
*tells* its server, `registry` is what it *asks* it for, so `Local::of` tests them in that order and a test
pins it.

**Impact**: `net` gains `Registration`, `OnlineTable` / `OnlineEntry` / `Online`, the body helpers
(`register_body`, `is_register`, `heartbeat_body`, `is_heartbeat`, `registered_body`, `is_registered`),
`HEARTBEAT_INTERVAL` and `ONLINE_WINDOW_MS`, the local frames `Local::Register` / `Local::Heartbeat` and
`LocalReply::{Registered, Beat, Unplaced}`, `answer_local`'s `now` parameter,
`RelayServer::{online, online_at, online_table}`, and `RelayClient::{connect, register, heartbeat}` with
`RelaySession`'s two senders; `host-core` gains the settings field, the `connection_client` slot, the
`Heartbeat` loop (`std::thread` + `mpsc`), `connect_cross_region`, and
`AppState::{connection_client, start_connection_heartbeat, stop_connection_heartbeat}`. New tests:
`net/tests/registration.rs` (five) and two in `host-core/tests/connection.rs` (a dangling pointer refused, and
the whole client half against a real server — register, beat, watch the row move, stop the loop and watch it
freeze); the `relay` example grows to **16** checks. **No hash formula, route, capability name, audit event
constant or persisted format changed, and no chain row is written** — the table is runtime state (§6.6), and
V-proto-2's `peer_offline` / `peer_recovered` rows belong to **V-3**, which is also where the probes and the
collective judgement land. The claims sent today are the node's **rooms** and nothing else: the peer port
that would carry an address is a setting §4.3 names and no batch has landed yet, and the server already holds
this node's configured addresses in its own `peers.json` — which is why `addresses` is empty rather than
wrong.

## 104. A node is judged gone by its peers' unanimity, and the server hands the transition out

**Date**: 2026-09-28 ｜ **Status**: Decided; V-3a implemented (the node-level half of §6.7)

**Decision**: [connection.md §6.7](connection.md) is implemented in `net` and wired into `host-core`, at the
**node level** only. A new `net` module (`liveness`) holds the three pieces: the probe (`{"probe": 1}` /
`{"alive": 1}`), the prober's own **view** (`Prober`: per peer, the last answer, the misses in a row, and
whether the peer is held reachable; memory-only, no chain row), and the server's **witness table**
(`WitnessTable`) with its rule — a node is judged gone when **at least one witness** remains and **every**
remaining witness reported it unreachable, where a witness counts only while **fresh** (inside 45 s),
**itself online**, and **not the subject**, and a fresh witness of life **vetoes**. The server's `Local`
gains two frame types (`UnreachableReport` / `ReachableReport`); the `OnlineTable` row gains
**`judged_at_ms`**, kept apart from §6.6's heartbeat-based `state`; and a judgement is handed out through a
**transition sink** rather than written by `net`, so `host-core` installs `AppState::connection_judgement_sink`
and writes **`host.connection.peer_offline`** (`{peer, witnesses, reports}`) and
**`host.connection.peer_recovered`** (`{peer, method}`, `heartbeat` or `probe`) through `emit_host`. `host-core`
runs a **probe thread** beside the beat thread — a separate `std::thread` sharing the node's **single** session
— which probes the node's **workgroup** (its peers other than itself and its cross-region server), answers
probes addressed to it, and pulses its view upward each cycle. **Recovery is being heard from**: a heartbeat
or a reachable report clears `judged_at_ms`, returns the row to `online`, and fires the recovered transition.
**No removal**: the protocol defines no kick. **Sibling confirmation is V-3b**, after V-4 wires the surfaces
above the node.

**Why**: Four choices carry it. **The events are written by host-core, through a sink.** §6.7 says the
**judging server** writes the two rows, and in v1.0 the server is a `net` deployment — but the chain is
`host-core`'s, and `net` must not reach upward. A `TransitionSink` closes that gap the honest way: `net`
records the judgement and hands the transition out, and a deployment that runs a server beside a chain installs
`AppState`'s sink, so the rows land where the `host.connection.*` family already lives — and **not** in
`control-plane-events.md`, whose twenty names are the **stream** events. **The probe thread shares the beat's
session, and is its own thread.** A second session to the server would replace this node's entry in the
server's `SessionTable` and strand the first; and the beat only *sends* while the prober must *read* (to catch
an `alive` and to answer a `probe`), so one thread doing both would make each wait on the other's reads. They
share the client instead — `net`'s session already keeps its reader and writer separately locked — and the
prober holds the **client**, never an `Arc<AppState>`, for the reason §103's beat does. **The view starts
optimistic and the third miss is the line.** A prober has no evidence against a peer until three probes in a
row go unanswered (§6.7's 45 s), which is why a peer begins *reachable*; and a judgement is set **once**
(`judged_at_ms` is not re-stamped), so a report that keeps pulsing while the row is already judged is not a
second transition. **Recovery refreshes the row.** §6.7 says a heartbeat or an answered probe "returns
`state` to `online`", so a reachable report both clears the judgement and refreshes `last_heartbeat_ms` —
otherwise the two halves of the same sentence would disagree.

**Impact**: `net` gains `src/liveness.rs` (the bodies, `Report`, `Prober`, `PeerView`, `Judgement`,
`WitnessTable`, `Transition` / `RecoverMethod` / `TransitionSink`, `PROBE_INTERVAL` / `PROBE_MISSES` /
`REPORT_WINDOW_MS` for the prober); `src/relay.rs` gains the two `Local` frames, the `OnlineTable`'s
`judged_at_ms` and its `is_online` / `mark_judged` / `clear_judged` / `heard_from`, the transition sink,
`judge`, and the `RelayClient` / `RelaySession` senders (`probe` / `answer_alive` / `report`);
`net/tests/liveness.rs` adds seven integration tests and `src/liveness.rs` five unit tests; the `relay`
example grows to **20** checks. `host-core` gains the `Probe` thread,
`AppState::{start_connection_probe, stop_connection_probe}`, `connection_judgement_sink` /
`record_connection_transition`, the short **connection read timeout** the prober polls on, and
`tests/connection.rs` two tests (a judgement writes both events through the sink; the probe thread starts
only with a workgroup and stops). **No hash formula, route, capability name, audit event constant, existing
`net` logic or persisted format changed** — the two names are additions to the `host.connection.*`
`net` logic or persisted format changed** — the two names are additions to the `host.connection.*`
vocabulary. **V-3b — the sibling confirmation, and the cross-region server that judges by the same rule — is
next**, after V-4 wires the node's upper surfaces.

## 105. The desktop can read the node's connection state

**Date**: 2026-09-28 ｜ **Status**: Decided; V-4's first face (batch AD / AC-1) shipped

**Decision**: `host-tauri` gains four **read-only** commands, and `ui/src-tauri`'s
`generate_handler!` registers them: `get_node_key` (`Option<NodeKeyView>`), `list_peers`
(`Vec<net::PeerEntry>`), `list_rooms` (`Vec<net::Room>`) and `connection_status`
(`ConnectionStatusView`). Every one is a thin wrapper over an `AppState` method that already
existed — `node_key()`, `peers()`, `rooms()`, `network()`, `connection_client()`,
`connection_problem()` — and none of them writes anything: the settings still decide and the
wiring still acts. `NodeKeyView` is a **view, not the key**: `net::NodeKey` **is** `Serialize`
(it is the JWK file), but its `d` member is the private half, so the view carries `node_id`
(the device name), `public_jwk`, `fingerprint` and `short_fingerprint` and nothing else.
`ConnectionStatusView` keeps three facts apart: `configured`
(`network().cross_region_server.is_some()`), `connected`
(`connection_client().map(RelayClient::is_connected)` — the session opens lazily, so this is
`false` until something needs it), and `problem` (what V-1/V-2 recorded). **The upper surfaces
are split**: AC-1 (this batch) is the desktop; **AC-2** is the server routes (which move
`docs/control-plane-api.md` §5.1/§5.2's counts and the tool-schema tables), **AC-3** is the
CLI, and **AC-4** is a server-role surface, which is what V-3b really needs. Nothing here
touches the route table, the capability vocabulary, an audit event constant, a hash formula,
`net`, `host-core`, `server` or `cli`.

**Why**: Three points carry it. **A view, because the key is not the wire shape.** The batch's
premise was "`NodeKey` is not `Serialize`"; the truth is the opposite and more dangerous — it
*is* `Serialize`, because it *is* the JWK file, whose `d` is the private key. So the rule is not
"write a serialiser for an opaque type" but "never hand the key itself out": `NodeKeyView` is
the one shape that carries the public half (`public_jwk`, the fingerprints) and leaves `d`
behind. **`configured` is a settings fact, not a client fact.** A pointer naming a peer this
node does not hold produces no client and sets a `connection_problem`, so "is there a client?"
would report a *misconfigured* node as unconfigured; reading `network().cross_region_server`
answers the question the deployer asked. **`connected` is honestly `false` most of the time.**
The session is opened lazily (decision §103), so a node whose beat loop has not run yet is
`configured: true, connected: false` (or `true` once it beats) — which is the state §103 chose,
reported as it is rather than smoothed over.

**Impact**: `host-tauri/Cargo.toml` gains two dependency edges — `net = { path = "../net" }`
(for `PeerEntry` / `Room` / `NodeKey`, which `host-core` does not re-export) and
`serde = { version = "1", features = ["derive"] }` (for the two views; `host-tauri` had
neither before) — with **no new package** (the lock gains edges); `host-tauri/src/commands.rs`
gains the four commands and the two views; `ui/src-tauri/src/lib.rs` registers the four names.
**No test is added**: `host-tauri` has no tests of its own by design (`Cargo.toml`: "this crate
has no tests of its own"; the 39 integration tests live in `host-core/tests`) and a
`#[tauri::command]` needs a live Tauri `State` to call, so the batch is verified by compilation,
`clippy -D warnings` and the gate. **No hash formula, audit event constant, route definition,
capability name, `net`/`host-core`/`server`/`cli` file or persisted format changed.** AC-2
(server routes), AC-3 (CLI) and AC-4 (server role) follow.

## 106. The connection layer is read over HTTP, and a node that knows nobody answers an empty list

**Date**: 2026-09-28 ｜ **Status**: Decided; implemented (batch AE / AC-2)

**Decision**: `server` serves **four read-only queries** for the connection layer — `GET /v0/identity`,
`GET /v0/peers`, `GET /v0/rooms` and `GET /v0/connection` (new `Action`s `Identity`, `Peers`, `Rooms`,
`Connection`) — each wrapping the same `AppState` accessor batch AD put in front of the desktop, and
each declaring **`status.read`**: the four describe *this node's own* surface, which is what that
capability is for, so **no capability name was added**. **Absent data is `null`, never a `404`** (§2: a
node that never joined a network is a working node with nothing to report): `identity` is `null` when
the layer is unconfigured, and `peers` and `rooms` are `null` when there is no `peers.json` or
`rooms.json`; `connection` answers `{configured, connected, problem}`, unchanged. §5.1 of
`docs/control-plane-api.md` now says **37** (both languages) and the four rows sit in
`docs/tool-schema-control-plane.md`'s marked query table, so the route table, the two documents and
the tool schema stay one set.

**Why**: Two choices were live. **`null` for absent data, not an empty shape and not a `404`.** Each of
the three reads an `Option` at the source — the key until networking is configured, and the two files
until a deployer writes them — so `null` is the honest reading of "nothing to report"; an unconfigured
node is a working node (§2), not a broken one, so an error would be a lie. (The desktop's commands
flatten the two lists to empty; over HTTP the item itself is what is asked for, so its absence is the
answer.) **No new capability.** §83's rule is that a route declares a
capability; `status.read` already says "this node's own status", and these four are exactly that, so
the vocabulary is untouched and the owner's token reaches them as it reaches `/v0/capabilities`.

**Impact**: `server/src/routes.rs` gains the four `Action` variants, the four `ROUTES` rows and the
four dispatch arms; `server/tests/smoke.rs` adds the four paths to `every_query_endpoint_answers`
(its count 34 → 38) and one test, `the_connection_layer_answers_over_http`, that pins both a fresh
node's `null`s and a configured node's shapes (seeded with `net`'s own writers, which is why
`server/Cargo.toml` gains `net` as a **dev-dependency** — an edge, no package); both `docs/control-plane-api.md` and its translation move §5.1 from **33 to 37**, and both
`docs/tool-schema-control-plane.md` files gain the four rows. **No hash formula, audit event
constant, capability name, `net`/`host-core`/`host-tauri`/`cli` file or persisted format changed.**
AC-3 (CLI) and AC-4 (server role) follow.

## 107. The server role is a node's own deployment shape, configured in settings and started from its own files

**Date**: 2026-09-28 ｜ **Status**: Decided; implemented (batch AF / AC-4)

**Decision**: A node can now **be the network's server**. `NetworkSettings` gains
**`server_role: Option<ServerRoleSettings>`** (additive, `#[serde(default)]`, `SETTINGS_VERSION`
unmoved), whose one field is **`bind`** — required, and deliberately without a default. When it is
present, `AppState::load_connection_files` starts a **`RelayServer`** from the node's own
`node.key` / `peers.json` / `rooms.json`, binds `network.server_role.bind` **synchronously** (a bind
that cannot be taken is recorded in `connection_problem`, not left to a thread that dies quietly),
and serves on a thread. `AppState::server_role_addr()` reports where it is bound and
`AppState::server_role()` hands out the handle, so a deployment that runs a chain can install
§6.7's `AppState::connection_judgement_sink` on it. Nothing starts a server on a node that did not
configure one.

**Why**: Three points carry it. **The role is configuration, not a program.** §6.1 makes the server a
**deployer-run** deployment and §33's cell-differentiation makes a node-with-a-role a shape, not a
second binary, so the in-network server of §6.5 is the *same* `RelayServer` the standalone
`riscdom-relay` runs — one mechanism, two deployment shapes — and the node starts it from the files
it already loaded (its key, its peer table, its rooms). **`bind` has no default on purpose.** A
default address would be the project naming where a server is, which §6.1 forbids; the deployer
writes it, exactly as `riscdom-relay` requires `--bind`. **It binds before it serves.** Binding
inside `load_connection_files`, synchronously, turns "the port is taken" into a reported problem on a
node that otherwise runs, instead of a role that silently never came up.

**Impact**: `host-core/src/settings.rs` gains `ServerRoleSettings` and the `server_role` field;
`host-core/src/state.rs` gains two fields, `start_server_role`, `note_connection_problem`,
`server_role_addr` and `server_role`, and calls the starter from `load_connection_files`;
`host-core/tests/connection.rs` adds three tests (the role binds and a node registers and beats into
it; a node without a role serves nobody; a bind that cannot be taken is reported) and
`host-core/tests/settings.rs` one (the field is configured or absent, and the version does not move);
`net/README.md` and its translation say the role can now run in-node as well as standalone. **`net`'s
code is unchanged** — `RelayServer`, `Listener` and the §6.6 handling already serve it — and no hash
formula, route, capability name, audit event constant or persisted format moved. §6.7's **sibling**
confirmation — the second level, a server's own loss judged by its siblings through the cross-region
server — is **V-3b**, and the sink this batch exposes is the hook it installs.

## 108. A server declares itself in its registration, and the sibling set is read from the claims

**Date**: 2026-09-29 ｜ **Status**: Decided; protocol prose only (batch AH / V-3b-proto)

**Decision**: `docs/connection.md` §6.7's frozen sentence said a server's siblings are "the other
in-network servers registered with the same cross-region server, learned from that server's registry", but
neither §6.6 nor §6.7 said **how a server is told apart from a node** — `RelayServer::registry()` is
`peers.json`-derived and carries no such marker, and a registration's `capabilities` were never used. The
protocol now says it: **an in-network server declares the ordinary claim `"server"` in the `capabilities`
list of its §6.6 registration**, the server's row keeps the claims a registration made, and the
**cross-region server's sibling set is the rows whose claims include `"server"`**. `"server"` is a **claim,
not a capability**: it is a string in a claim list, **not** a member of the control plane's vocabulary, it
widens no word list, and declaring it grants no authority — only that the node is **probed as a sibling** and,
if it stops answering, judged by the siblings' unanimity.

**Why**: Three points. **Zero shape change.** The claim travels in §6.6's existing registration body, the
server already keeps what a registration claims, and the aggregation half (the witness table, unanimity, the
vetoing witness) is already V-3a's — so the fix is a sentence, not a frame, a field or a format. **A claim,
not a capability, and that distinction is load-bearing.** The control plane's capability vocabulary
(decisions §83) is what authority is granted from; a node must never widen it by declaring something about
itself. §6.6 already fixes the standing — a claim is "a claim, not a fact", a **source** the server's own
files stay authoritative over — and this batch adds no exception. **A false claim is harmless by
construction.** Declaring `"server"` buys exactly one thing: being probed. A node that lies only invites
probes it does not answer, and the worst that follows is a judgement about itself, which §6.7's rules
already govern (and a judgement touches no identity, no membership and no key).

**Impact**: `docs/connection.md` §6.6 gains a bullet (the row keeps the registration's claims:
`capabilities`, `rooms`) and its **Frozen** summary names the table's fields instead of "the four fields";
§6.7's "who judges what" bullet says how a server says it is one, a new bullet fixes `"server"` as a claim
and not a capability, the "how a sibling knows" bullet points at that claim rather than "the registry", and
§6.7's **Frozen** list gains the same point. Its translation follows. **No source file, no dependency, no
`Capability` variant, no audit event constant, no hash formula, no route and no persisted format changed.**
The implementation — declaring the claim when the server role runs, and taking the sibling set from the
cross-region server's table — is **V-3b-1/V-3b-2**, and V-3a's aggregation half is already in place.

## 109. A prober's siblings come from its own peers.json, and no probe waits forever

**Date**: 2026-09-29 ｜ **Status**: Decided; implemented (batch AJ / V-3b-1)

**Decision**: Two things. **(1) The sibling set is configuration.** A node that runs the server role
probes §6.7's second level from **its own `peers.json`**: the entries that declare the `"server"` claim
are its siblings, and the same entries carry their **public keys**. `net` gains `PeerEntry::is_server`,
`PeersFile::servers` / `server_keys` and the `SERVER_CLAIM` constant, and `host-core` starts a **second
`Probe` thread** beside V-3a's — same client (the cross-region one), different peer set, different keys —
from `start_server_role`, only when a cross-region pointer and at least one sibling exist. The reports are
V-3a's two bodies, sent upward through the node's own client. **(2) No tool probe waits forever.**
`host-core`'s `exec_retrying` now spawns and waits with a deadline (**60 s**), killing the child and
returning `TimedOut` — which the callers already read as "this program is not usable".

**Why**: Two reasons, one of them the reason batch AI stopped. **A key never arrives by frame.** §4.2 and
§6.6 make that structural, so a prober can only probe peers whose keys it holds — and the only place it
holds them is its own `peers.json`. Batch AI's first reading (take the sibling set from the cross-region
server's `OnlineTable`) could not work: no frame carries that table to a node, and even the ids would be
unverifiable without keys. Reading the claim from the same file closes both gaps at once, and it is not a
new mechanism — §6.6 already says identity comes from configuration, and the claim is the ordinary
`capabilities` string §6.6's registration and §4's entry both carry. **A probe that waits forever is not a
probe.** `Command::output()` blocked on a child that never answered — a wedged
`qemu-system-riscv64 --version` did exactly that to the gate, four times — and the host, which probes
QEMU, gcc, zig and rustc through the same helper, would have hung with it. The deadline turns that into
the answer the code already knew how to give: not usable.

**Impact**: `net/src/peers.rs` gains `SERVER_CLAIM`, `PeerEntry::is_server`, `PeersFile::servers` and
`PeersFile::server_keys`, plus a unit test, and `net/src/lib.rs` re-exports the constant;
`host-core/src/state.rs` gains the sibling-probe field, `start_sibling_probe`, its public start/stop pair
and the call from `start_server_role`, and `exec_retrying` now runs through `run_bounded` / `wait_bounded`
(with the unit test for the deadline); `host-core/tests/connection.rs` gains the sibling-prober test;
`docs/connection.md` §6.7 says where a prober finds its siblings (its own `peers.json`) and its
translation follows. **No hash formula, route, capability name, audit event constant or persisted format
changed, and the `"server"` claim is not a `Capability` variant** — it is a string in a claim list. §6.7's
**aggregation** side — what the cross-region server does with the reports, and installing the judgement
sink — is **V-3b-2**.

## 110. A deployment installs the judgement sink where it holds the Arc, and a server writes only its own chain

**Date**: 2026-09-29 ｜ **Status**: Decided; implemented (batch AK / V-3b-2)

**Decision**: `AppState` gains **`install_connection_sink(self: &Arc<Self>) -> bool`**: it installs
`connection_judgement_sink` on **this node's own** `server_role()`, and answers whether there was one to
install on. The three deployments call it where they hold the `Arc` — `ui/src-tauri`'s setup,
`server/src/main.rs`, and `cli/src/client.rs`'s embedded mode — because the constructors hand back a
`Self`, not an `Arc`, and the sink must hold one. The **aggregation** side needs no `net` change: a server
already records a sibling's report and judges it (§6.7, V-3a's `answer_local`), and a standalone
`riscdom-relay` still installs no sink — it holds no chain.

**Why**: Two points. **The sink belongs to the server that judges, and that server is this node.** §6.7
says the judging server writes the two rows, and the only `RelayServer` a process owns is the one its own
server role started; the cross-region side is either the standalone relay (no chain) or another node,
whose own deployment installs its own sink. So the install is on `server_role()` and nowhere else — a
server never writes another node's chain. **The install point is the `Arc`, and only a deployment has
one.** `start_server_role` runs inside `load_connection_files(&self)`, where no `Arc` exists yet, and the
sink needs one to keep the state alive for as long as judgements arrive. Rather than reshape the
constructors — every test and `host-tauri` build state through them — the deployment that wraps the state
in an `Arc`, and is the thing running the server, calls one method.

**Impact**: `host-core/src/state.rs` gains `install_connection_sink`; `ui/src-tauri/src/lib.rs`,
`server/src/main.rs` and `cli/src/client.rs` each call it once where the `Arc` is made;
`host-core/tests/connection.rs` gains a test that installs the sink on a node's own server role, drives a
judgement through it and reads `host.connection.peer_offline` / `peer_recovered` off *that* node's chain
(plus the negative: a node without a server role answers `false`). Two older server-role tests were
tightened to seed `rooms.json`, so the "cannot bind" test now fails on the **bind** rather than on a
missing file. `net/README.md` and its translation say §6.7 is complete. **No hash formula, route,
capability name, audit event constant or persisted format changed, and `net`'s logic is untouched.**
§6.7's **sibling** confirmation is complete: V-3b-1 probes and reports, this batch wires the sink.

## 111. The CLI's four connection reads are four one-word commands

**Date**: 2026-09-29 ｜ **Status**: Decided; implemented (batch AL / AC-3)

**Decision**: `riscdom` gains four read-only commands — **`identity`**, **`peers`**, **`rooms`** and
**`connection`** — one per route AC-2 put on the wire (`GET /v0/identity`, `/v0/peers`, `/v0/rooms`,
`/v0/connection`). Each is a unit `Command`, a GET path, a `parse_command` arm, a `USAGE` line and a
renderer; the human mode prints key/value lines for `identity` and `connection` and a small table for
`peers` and `rooms`, and it **says the three `null`s in words** — `no identity: the connection layer is
not configured`, `no peer table: this node has no peers.json`, `no rooms: this node has no rooms.json`.
`--json` still passes the wire shape through untouched. `docs/control-plane-client-guide.md` §7 and
`cli/README.md` gain four rows each.

**Why**: Two points. **One word, not two.** The other reads that name a resource take a subcommand
(`executors list`, `audit status`), but a connection item is not a collection of one: each of the four
names exactly one thing, so the shape that reads best is the one that reads least — and it keeps the
CLI's vocabulary parallel to the routes' (one route, one command). **A `null` is said in words.** AC-2
answers `null` for absent connection data, and a renderer that printed an empty shape would make "the
layer is unconfigured" look like "the read worked and found nothing" — which is the one distinction
those routes exist to make. The three sentences are that distinction, and `--json` keeps the
machine-readable `null` for anyone who prefers it.

**Impact**: `cli/src/args.rs` gains the four variants, usage lines, parse arms, GET paths and the
`method()` arm, plus assertions in the two existing parse/path tests; `cli/src/render.rs` gains the four
renderers and their `human()` arms; `cli/tests/read_only.rs` drives all four against the embedded server
(a fresh node answers `null`, and `--json identity` is `null` on the wire);
`docs/control-plane-client-guide.md` §7 and both `cli/README.md` files gain four rows. **No server route,
`net`, `host-core` or `host-tauri` file changed, and no hash formula, route definition, capability name,
audit event constant or persisted format moved** — the CLI is an HTTP client and stays one. **V-4 is
complete**: AC-1 the desktop, AC-2 the routes, AC-3 the CLI, AC-4 the server role.
