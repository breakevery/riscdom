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

**Date**: 2026-09-22 ｜ **Status**: Decided

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
