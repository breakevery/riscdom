[中文](README.zh-CN.md) | English

# RiscDom documentation map

> **Applies to v0.9 (unstable until v1.0).** One page for the whole repository: every
> Markdown file, grouped by who reads it. [decisions.md](decisions.md) §21 names five
> audiences (kernel developers, distribution integrators, administrators, end users,
> contributors) and requires each document to have one; this page is what makes that
> checkable at a glance.

How to read a row: the link, what the document is for, its **state** — *living* (kept in step
with the code, by a checker or by the batch that changes it), *snapshot* (a record of a
moment that is deliberately not rewritten), or *history* (an append-only log) — and the
version it applies to when it says so itself. Documents are bilingual in pairs:
`X.md` ↔ `X.zh-CN.md`, and `scripts/check-bilingual.sh` refuses a file without its
counterpart.

## 1. Start here

| Document | What it is | State |
|---|---|---|
| [README.md](../README.md) — [中文](../README.zh-CN.md) | What RiscDom is, the architecture in one screen, quick start, how to build and test. | living |
| [ENVIRONMENT.md](../ENVIRONMENT.md) — [中文](../ENVIRONMENT.zh-CN.md) | The development machine this project is verified on: OS, toolchain versions, paths. | snapshot (this machine) |
| **docs/README.md** (this page) — [中文](README.zh-CN.md) | The map: what exists, who it is for, what is history. | living |

## 2. Kernel developers

The kernel is `agent` + `sandbox` + `audit`, wrapped by `host-core` (portable). `net` carries
the connection layer, `worker` the executor, and `backup` / `sdk/rust` the tooling. The desktop
shell (`host-tauri`) and its front end (`ui`) are **`riscdom-adminapp`** (v1.0 M8-4b), and the
control plane as a program is **`riscdom-server`**. These documents are about the machinery: how
it is put together, why it is that way, and what was settled.

### 2.1 The design record

| Document | What it is | State |
|---|---|---|
| [decisions.md](decisions.md) — [中文](decisions.zh-CN.md) | The decision ledger: every settled question with its date, the decision, the why and the impact. §21 is the rule this page serves. | living (append-only) |
| [architecture-evolution.md](architecture-evolution.md) — [中文](architecture-evolution.zh-CN.md) | **The v0.7 snapshot** of how the architecture got here, and the plan that followed it. | snapshot — **history, not rewritten** |
| [roadmap-v1.0.md](roadmap-v1.0.md) — [中文](roadmap-v1.0.zh-CN.md) | The plan: v1.0 freezes the kernel API and delivers the three layers, with every decision tagged settled, default or open. | living (a plan — rewritten as milestones land) |
| [api-compatibility.md](api-compatibility.md) — [中文](api-compatibility.zh-CN.md) | frozen as of v1.0 (M1), **for distribution integrators**: what is frozen, what may change without a freeze, how persisted data migrates, and how an upgrade crosses a major version. | living (frozen v1.0 spec) |
| [error-model.md](error-model.md) — [中文](error-model.zh-CN.md) | v1.0 specification (M1), for kernel developers and integrators: the error categories, which are retryable, the cause chain, and the wire envelope's mapping to `DispatchError`. | living (v1.0 spec) |
| [security-model.md](security-model.md) — [中文](security-model.zh-CN.md) | v1.0 specification (M1), **for administrators**: where every secret lives, what a capability is worth, the threat-model boundary, and the disclosure policy. | living (v1.0 spec) |
| [upgrade.md](upgrade.md) — [中文](upgrade.zh-CN.md) | v1.0 specification (M1), **for distribution integrators**: the written upgrade procedure — what an upgrade changes, where each version marker lives, the order of operations, and how a major version is crossed. | living (v1.0 spec) |
| [plugin-interface.md](plugin-interface.md) — [中文](plugin-interface.zh-CN.md) | v1.0 specification (M3), **for plugin authors**: the frozen plugin interface — the transport, the mechanism and semantics layers, the capability declaration and the manifest. | living (v1.0 spec) |
| [connection.md](connection.md) — [中文](connection.zh-CN.md) | v1.0 specification (M4), for kernel developers and deployers: the connection layer — node identity and signing, discovery, rooms, the cross-region server, and §7's digests of a chain. | living (v1.0 spec) |
| [cross-device-dispatch.md](cross-device-dispatch.md) — [中文](cross-device-dispatch.zh-CN.md) | v1.0 specification (M6-1): handing a task to **another node** — the `node` parameter, the two §3 frames, the `dispatch` claim and its default deny, the four legible refusals, and the workgroup boundary. | living (v1.0 spec) |
| [cross-chain-verification.md](cross-chain-verification.md) — [中文](cross-chain-verification.zh-CN.md) | v1.0 specification (M6-5-1): roadmap §7's three open questions — answered by digest, the anchor's second half, the conflict exit, and what is still M6-5-2/‑3/‑4. | living (v1.0 spec) |
| [audit-v2.md](audit-v2.md) — [中文](audit-v2.zh-CN.md) | v1.0 specification (M5-1a): the audit chain's extension to "the main chain + temporary segments" — the segment record, the cross-segment reference, the reserved segment events, and the questions still open. | living (v1.0 spec) |
| [handoff.md](handoff.md) — [中文](handoff.zh-CN.md) | Cross-conversation handoff: §1 is the volatile snapshot, §2–12 the stable constraints a new session must not break. | living (§1), stable (§2–12) |
| [run-provenance.md](run-provenance.md) — [中文](run-provenance.zh-CN.md) | Design: what a run records about itself, and why the fingerprint is what it is. | snapshot (v0.4 batch 1a) |
| [multi-agent-foundation.md](multi-agent-foundation.md) — [中文](multi-agent-foundation.zh-CN.md) | The four shapes v0.8 settled for several processes on one machine (identity, per-agent snapshots, the dispatch abstraction, shared workspace). | snapshot (v0.8) |

### 2.2 Operating the machinery

| Document | What it is | State |
|---|---|---|
| [preflight.md](preflight.md) — [中文](preflight.zh-CN.md) | The environment preflight: what it checks and how it caches its verdict. | living |
| [e2e-debugging.md](e2e-debugging.md) — [中文](e2e-debugging.zh-CN.md) | How to debug an end-to-end run when it goes wrong. | living |
| [qemu-stdio.md](qemu-stdio.md) — [中文](qemu-stdio.zh-CN.md) | How the QEMU port dependency was removed, and the relay-port lease that replaced it. | snapshot (v0.4 #1) |
| [qemu-distribution.md](qemu-distribution.md) — [中文](qemu-distribution.zh-CN.md) | Bundle QEMU or download it: the decision, and why no release is pinned. | snapshot (v0.4 #4) |
| [golden-path.md](golden-path.md) — [中文](golden-path.zh-CN.md) | The v0.5 design proposal for the manual release walk. | snapshot (v0.5) |
| [golden-path-checklist.md](golden-path-checklist.md) — [中文](golden-path-checklist.zh-CN.md) | The checklist a walker fills in, step by step. | living |
| [manual-acceptance.md](manual-acceptance.md) — [中文](manual-acceptance.zh-CN.md) | The order of work for a manual acceptance walk: install, launch, drive, verify, and what to send back. | living |
| [sandbox/docs/snapshot-experiment.md](../sandbox/docs/snapshot-experiment.md) — [中文](../sandbox/docs/snapshot-experiment.zh-CN.md) | The snapshot feasibility experiment and what it measured. | snapshot (stage 18a) |

### 2.3 The crates

| Document | What it is | State |
|---|---|---|
| [agent/README.md](../agent/README.md) — [中文](../agent/README.zh-CN.md) | The agent runtime: modules, the eight tools (schema in [tool-schema-executor.md](tool-schema-executor.md)), the audit events it writes, the loop and its context. | living |
| [sandbox/README.md](../sandbox/README.md) — [中文](../sandbox/README.zh-CN.md) | QEMU lifecycle, serial capture, snapshots (including the MVP fallback) and the relay. | living |
| [audit/README.md](../audit/README.md) — [中文](../audit/README.zh-CN.md) | The append-only store, the hash chain, the event vocabulary, and `audit-verify`. | living |
| [host-core/README.md](../host-core/README.md) — [中文](../host-core/README.zh-CN.md) | The portable half of the host: modules, its relationship to `host-tauri`, and its constraints (no Tauri). | living |
| [worker/README.md](../worker/README.md) — [中文](../worker/README.zh-CN.md) | The executor process and the supervisor half — including the remote executor handle (v0.9 E4). | living |
| [net/README.md](../net/README.md) — [中文](../net/README.zh-CN.md) | The connection layer (v1.0 M4): node identity on disk today; signing, discovery, rooms and the cross-region server as the frozen sections land. | living |
| [backup/README.md](../backup/README.md) — [中文](../backup/README.zh-CN.md) | The portability tool (v1.0 M7e): export a whole node — both roots, the audit store through SQLite's consistent path, and the derived keyring credentials — as one encrypted package. | living (AV-1 + AV-2) |
| [sdk/rust/README.md](../sdk/rust/README.md) — [中文](../sdk/rust/README.zh-CN.md) | The Rust SDK (v1.0 M7c): a typed client for the whole control-plane surface — queries, controls and the event stream — with drift guards that hold it to the server's own routes. | living (BA + BB) |
| [sdk/typescript/README.md](../sdk/typescript/README.md) — [中文](../sdk/typescript/README.zh-CN.md) | The TypeScript SDK (v1.0 M7d): the same surface for browser and Node, on `fetch` — no runtime dependency, the same drift guards, and the stream read with `fetch`, never `EventSource`. | living (BC) |

## 3. Distribution integrators

Whoever writes a client of the control plane, or ships this inside something else. The
normative tables are the API and events documents; the guides are the working walkthroughs.

| Document | What it is | State |
|---|---|---|
| [control-plane-api.md](control-plane-api.md) — [中文](control-plane-api.zh-CN.md) | **The normative table**: every endpoint, its method, its capability, its request and its answer. | living |
| [control-plane-events.md](control-plane-events.md) — [中文](control-plane-events.zh-CN.md) | The event stream: the envelope, the event vocabulary, filtering, frames. | living |
| [control-plane-client-guide.md](control-plane-client-guide.md) — [中文](control-plane-client-guide.zh-CN.md) | How to write a client: first call, errors, subscribing, the CLI, driving it from an AI supervisor, writing a remote executor handle. | living |
| [tool-schema-control-plane.md](tool-schema-control-plane.md) — [中文](tool-schema-control-plane.zh-CN.md) | Every endpoint as an OpenAI-style tool definition — the supervisor's `tools[]`, ready to paste. | living (checked) |
| [tool-schema-executor.md](tool-schema-executor.md) — [中文](tool-schema-executor.zh-CN.md) | The eight tools an executor's model is offered, as the exact array the kernel sends. | living (checked) |
| [config-schema.md](config-schema.md) — [中文](config-schema.zh-CN.md) | **The configuration schema** (v1.0 M7f): every field of `settings.json`, `peers.json` and `rooms.json`, with its type, whether it may be absent, and how each format is versioned. | living (v1.0 spec) |
| [examples/python/README.md](../examples/python/README.md) — [中文](../examples/python/README.zh-CN.md) | The runnable reference supervisor: three endpoints, stdlib only, with an offline `--self-test`. | living (self-tested) |
| [server/README.md](../server/README.md) — [中文](../server/README.zh-CN.md) | The control plane as a program: build, run, endpoints, the stream, authentication — and what is not implemented. | living |

## 4. Administrators

Whoever runs a node: what it needs, what it refuses, and where the security posture is
written down.

| Document | What it is | State |
|---|---|---|
| [SECURITY.md](../SECURITY.md) — [中文](../SECURITY.zh-CN.md) | How to report a vulnerability, what is in scope, and the promises about secrets. | living |
| [server/README.md](../server/README.md) — [中文](../server/README.zh-CN.md) | How to start the control plane, its bind default, its token, and `--no-auth`. | living |
| [qemu-setup.md](qemu-setup.md) — [中文](qemu-setup.zh-CN.md) | Installing the QEMU the node needs (the project never bundles it). | living |
| [observability.md](observability.md) — [中文](observability.zh-CN.md) | **The observability contract** (v1.0 M7g): the structured-log lines, the metrics endpoint and its families, and the tracing id — plus the `task_id` gap and how it closes. | living (v1.0 spec) |
| [performance-budget.md](performance-budget.md) — [中文](performance-budget.zh-CN.md) | **The performance budgets** (v1.0 M7h): §18's four numbers (VM start, dispatch round trip, memory, log growth) written down — where each is measured from and to, in what condition, and how a budget is checked. | living (v1.0 spec) |
| [multi-repo.md](multi-repo.md) — [中文](multi-repo.zh-CN.md) | **Working across the three repositories** (v1.0 M7i): how `riscdom-adminapp` and `riscdom-server` stand next to this one, how they get the kernel (a git dependency pinned to a tag), what they inherit, what they own, and the CLA each repository carries. | living (v1.0 spec) |
| [backup.md](backup.md) — [中文](backup.zh-CN.md) | **Backup and portability** (v1.0 M7e): what a node's persistent state is, and how `riscdom-backup` exports it as one encrypted, movable package and restores it — audit store, snapshots and credentials, with nothing outside the package required. | living (v1.0 spec) |
| [server-distribution.md](server-distribution.md) — [中文](server-distribution.zh-CN.md) | **Distributing the two server programs** (v1.0 M7b-1): what a `riscdom-server` package and a `riscdom-relay` package each hold, how `scripts/pack.*` builds them, and what is not signed. | living (v1.0 spec) |
| [sdk.md](sdk.md) — [中文](sdk.zh-CN.md) | **The SDKs** (v1.0 M7c/M7d): Rust and TypeScript clients as one thin, typed layer over the frozen control-plane surface (API, event stream, config types) — what they carry, how they version, and the no-fourth-copy rule. | living (v1.0 spec) |
| [toolchain-setup.md](toolchain-setup.md) — [中文](toolchain-setup.zh-CN.md) | Installing and pointing at the RISC-V bare-metal compiler. | living |
| [THIRD_PARTY_NOTICES.md](../THIRD_PARTY_NOTICES.md) — [中文](../THIRD_PARTY_NOTICES.zh-CN.md) | QEMU, the downloaded toolchain and the rest: separate programs, their own licences. | living |

## 5. End users

Whoever just wants to run the thing.

| Document | What it is | State |
|---|---|---|
| [cli/README.md](../cli/README.md) — [中文](../cli/README.zh-CN.md) | The `riscdom` command line: every command, the two modes, the token, output, exit codes. | living |
| [CHANGELOG.md](../CHANGELOG.md) — [中文](../CHANGELOG.zh-CN.md) | What changed, release by release, and in the unreleased line. | history (append-only) |
| [RELEASE_NOTES.md](../RELEASE_NOTES.md) — [中文](../RELEASE_NOTES.zh-CN.md) | The released text for the newest release (v0.9.9), including its known limitations. | history (per release) |

## 6. Contributors

| Document | What it is | State |
|---|---|---|
| [CONTRIBUTING.md](../CONTRIBUTING.md) — [中文](../CONTRIBUTING.zh-CN.md) | How to build, test (including the `--ignored` end-to-end tests), and the commit wrapper this repository commits through. | living |
| [PROJECT_CONSTITUTION.md](../PROJECT_CONSTITUTION.md) — [中文](../PROJECT_CONSTITUTION.zh-CN.md) | The full constitution: principles, architecture layers, the audit event types, the red lines. | living |
| [AGENTS.md](../AGENTS.md) — [中文](../AGENTS.zh-CN.md) | The working agreement for an AI session in this repository (the core constitution, injected each turn). | living |
| [CODE_OF_CONDUCT.md](../CODE_OF_CONDUCT.md) — [中文](../CODE_OF_CONDUCT.zh-CN.md) | The Contributor Covenant, and how to report unacceptable behaviour. | living |
| [CLA.md](../CLA.md) — [中文](../CLA.zh-CN.md) | The Contributor License Agreement every contribution needs. Signatures are recorded in `signatures/version1/cla.json`. | living |
| [walkthroughs/README.md](../walkthroughs/README.md) | What the walkthrough records are, why they are not in `docs/`, and why the bilingual gate skips this directory. | living |
| [walkthroughs/2026-09-19-preview1-local.md](../walkthroughs/2026-09-19-preview1-local.md) | One walk of the golden path on one machine, recorded as it happened. | snapshot (deliberately single-language) |

## 7. Auditors

| Document | What it is | State |
|---|---|---|
| [audit-package/README.md](audit-package/README.md) — [中文](audit-package/README.zh-CN.md) | The audit package: eight documents an outside reviewer reads in thirty minutes — architecture, capabilities, known issues, test evidence, a demo, dependencies and the owner's concerns — written by the project and pinned to `v1.0.0`. | living |

## 8. Outside the map, and why

- **`IDENTITY.md`, `SOUL.md`, `USER.md`** (repository root) are the agent-workspace identity
  files: they are read by the AI, not by a human reader, and they are deliberately
  single-language — `scripts/check-bilingual.sh` excludes them by name. They are not
  documentation and are not navigated here.
- **`LICENSE`** is the Apache-2.0 text, kept in English by design and excluded from the
  bilingual check.
- **`signatures/version1/cla.json`** is the CLA signature store, data rather than a document.

## Keeping this page true

Two checks already cover most of it, so this page cannot rot quietly:

- `scripts/check-bilingual.sh` fails when a document (or this one) has no counterpart.
- `scripts/check-links.py`-style link checks — [the same scan the line-ending batch ran] —
  are how a broken relative link is caught; every link above resolves.
- The endpoint and tool counts this page cites live in
  [control-plane-api.md](control-plane-api.md) §5, which `server/src/routes.rs`'s tests
  compare with the route table, so a number here has to be changed deliberately.

What is **not** checked: whether a new document was added to this map. If you add one, add
its row — the map is the list, and a document missing from it is a document nobody finds.
