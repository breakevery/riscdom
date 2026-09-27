[中文](api-compatibility.zh-CN.md) | English

# API compatibility, data migration and upgrade

**Status** v1.0 specification (milestone [M1](roadmap-v1.0.md)) ｜ **Date** 2026-09-27 ｜ **Baseline**
v0.9.9 (`3365970`) ｜ **Audience** distribution integrators — whoever builds a product on this kernel.

**What this document is.** The rulebook for what may change without breaking a client, and how data on
disk survives a version. It is the first of the six things the [v1.0 roadmap](roadmap-v1.0.md) §6 says
must be on disk before the API may be called frozen. It is a specification: it says what the freeze
*means*, so that the batch that implements it has something to be compared against.

**Its companions.** [`error-model.md`](error-model.md) (the categories an error carries, and which are
retryable) and [`security-model.md`](security-model.md) (keys, capabilities, disclosure). The wire shape
of errors and the endpoint tables are normative in
[`control-plane-api.md`](control-plane-api.md) §4 and §5/§7; this document is where the *policy* about
changing them lives.

## 1. What is frozen

- **[settled]** **The control plane's HTTP protocol.** Every path, method, request body, response body,
  status code and header documented in [`control-plane-api.md`](control-plane-api.md) is part of the
  frozen surface: hosts, integrations and the built-in clients all speak it.
- **[settled]** **What a client can observe.** The event stream's envelope and vocabulary
  ([`control-plane-events.md`](control-plane-events.md)), the tool schemas a model is offered
  ([`tool-schema-executor.md`](tool-schema-executor.md), [`tool-schema-control-plane.md`](tool-schema-control-plane.md)),
  and the audit event vocabulary.
- **[settled]** **The `pub use` surface of the host crates.** The names a downstream Rust crate imports
  from `host-core` / `agent` / `sandbox` / `audit` are frozen; their internals are not.
- **[settled]** **Rust internals are not frozen.** A module private to a crate, a struct field, a
  function that is not re-exported: any of them may change in a minor release. "Internal" is decided by
  the `pub use` list, not by a comment.

## 2. Changes that are allowed in a minor release

- **[settled]** **Adding an optional field** to a request or a response. A client that does not know the
  field ignores it; a server that does not receive it keeps today's behaviour. This is the rule the
  settings file has followed since v0.4 — and which kept `SETTINGS_VERSION` at **1** until the
v1.0 M2b-1 migration, the first change that needed a version of its own).
- **[settled]** **Adding an endpoint.** A new path under `/v1/` is additive by construction.
- **[settled]** **Adding a capability.** The vocabulary grows and existing tokens are unaffected —
  [`server/src/auth.rs`](../server/src/auth.rs) holds **33** capabilities today, and the number is not a
  contract.
- **[settled]** **Adding an event type** to the stream, and adding a variant to an enum that a client is
  expected to treat as open.
- **[settled]** **Adding an optional header**, and adding a value to a field documented as an open set.

## 3. Changes that are not allowed in a minor release

Each of these needs a **major** version, and therefore the migration path of §6:

- **[settled]** **Changing the meaning of an existing field** — same name, same type, different
  behaviour.
- **[settled]** **Removing a field, an endpoint, a capability or an event type.**
- **[settled]** **Changing a URL's semantics**, or the HTTP method a path is served under.
- **[settled]** **Changing the error `code` a documented failure carries.** A caller switching on `code`
  is the intended use of the field, so a code that changes meaning is a breaking change even when the
  status stays the same.
- **[settled]** **Changing an audit event's `action` string or the meaning of its `detail`** — the chain
  is read by tools that were not written here, and a rewritten meaning makes history lie.

## 4. Deprecation

- **[settled]** **A deprecation is announced, not discovered.** A deprecated endpoint answers with a
  `Deprecated` response header naming the replacement (and, when there is one, the version it will be
  removed in).
- **[settled]** **At least one major version of life.** Nothing is removed in the major version that
  deprecated it; the earliest removal is the next one.
- **[settled]** **The documentation says so.** The row in [`control-plane-api.md`](control-plane-api.md)
  is marked `deprecated since vX.Y`, so the table and the header cannot disagree.
- **[settled]** **A deprecated thing keeps working.** "Deprecated" never means "slower", "refused" or
  "silently different".

## 5. Versions

- **[settled]** **The path prefix carries the protocol's major version.** `/v0/` for the whole v0.x line,
  `/v1/` from v1.0 (this follows [`control-plane-api.md`](control-plane-api.md) §7, which already says
  so: v1.0 freezes the API and the prefix becomes `/v1/`).
- **[settled]** **v0.x promises nothing.** Within v0.x a breaking change ships without a prefix bump and
  clients must tolerate it. A v0.x integration pins a RiscDom **version range**, not an API version.
- **[settled]** **From v1.0, semver is strict**: `MAJOR.MINOR.PATCH`. A minor release may contain only
  §2's changes; a major release may contain §3's, and comes with a migration.
- **[settled]** **The package version and the protocol version are not the same number.** A 1.3.0
  release still speaks `/v1/`; the prefix moves only when the protocol breaks.

## 6. Data migration

**The rule.** **[settled]** Every persisted format carries its **version marker in its first field**, so
that a reader knows what it is holding before it reads anything else. A format that cannot do this (a
SQLite database) carries the marker in **`PRAGMA user_version`** — the header field SQLite
keeps for exactly this, read before anything else and written with the migration that earns it.

- **[settled]** **Migration happens when the file is opened.** No tool to remember, no flag to pass: a
  node that starts on a new version opens its old data and migrates it. This is what
  [decisions §14](decisions.md) means by "data migration is automatic" — the alternative (an upgrade that
  asks the user to run a migration by hand) is an upgrade that will be skipped.
- **[settled]** **New reads old: allowed.** The reader migrates, then reads. Before it writes anything it
  leaves a **`.bak` copy** of the file it is about to change, so a migration that goes wrong has one
  known way back.
- **[settled]** **Old reads new: refused.** An older build that meets a newer format returns
  **`Err(DataTooNew)`** and does nothing else. It must not partially read the file, must not read the
  fields it recognises and ignore the rest, and must not silently downgrade or rewrite it. A node that
  answers with half of a newer format's truth is worse than a node that refuses: the refusal is
  diagnosable and the half-read is not.
- **[settled]** **Migration is not reversible.** There is no "migrate back"; the `.bak` copy is the
  escape hatch, and so is [`riscdom-backup`](decisions.md) (§19), whose package is the unit of
  portability.
- **[settled]** **One version marker per format, listed here.** This table is the whole list; a new
  persisted format adds its row in the same batch that creates it.

| Format | Where | Version marker today | v1.0 action |
|---|---|---|---|
| `settings.json` | `<data-dir>/settings.json` | `version` = `SETTINGS_VERSION` (**2** from v1.0 M2b-1) | **the first real migration happened in v1.0 M2b-1**: a v1 file (LLM configuration was not persisted at all) is migrated on open to **2** with an empty per-executor `llm_configs` map — nothing guessed — its pre-migration bytes are kept as `settings.json.bak`, and a **newer** file is refused with `data_too_new` (nothing applied, nothing written) |
| Run fingerprint | inside each run's record | `FINGERPRINT_SCHEMA_V1` = `riscdom.run.fingerprint.v1` ([`audit/src/run.rs`](../audit/src/run.rs)) | a v2 fingerprint is a new marker value, never a rewritten v1 record |
| Audit store | `<data-dir>/audit.db` (SQLite) | `PRAGMA user_version` = **1** (v1.0 M2b-3a) | a file from before the marker reads as `0` and is migrated **on open** (the two columns earlier batches added — `agent_id`, `resumed_from_snapshot` — are re-checked idempotently) and then stamped; a newer file is refused with `data_too_new` — nothing read, nothing written. **No `.bak`**, unlike the session database: this file is WAL and opened by several processes, so a byte copy of `audit.db` alone can miss frames still in `-wal` |
| Session database | `<data-dir>/sessions.db` (SQLite) | `PRAGMA user_version` = **1** (v1.0 M2b-2) | a file from before the column is migrated **on open** (the `executor_id` column is added idempotently; existing rows stay **unnamed**, which means the node itself), its pre-migration bytes are kept as `sessions.db.bak`, and a newer file is refused with `data_too_new` — nothing read, nothing written |
| Credential files | `<data-dir>/token` | none: one line of hex, shape-checked | a hand-provisioned token is never rewritten; v1.0's `node.key` carries its own marker |

- **[settled]** **A marker is not a promise about other nodes.** Two nodes on different versions may
  share only what the protocol says; the older one refuses the newer one's files rather than guessing.

## 7. Upgrade path

- **[settled]** **In place.** The installer overwrites the installation; nothing is reinstalled and no
  data directory moves (decisions §14).
- **[settled]** **Stepwise across a major version.** A major version may not be skipped: 1.x → 3.x goes
  through 2.x. Each step ships the migration it needs (§6), and the step's migration tool is provided
  with the release notes for that step.
- **[settled]** **Migration is automatic** — see §6. The stepwise rule is about *data*: each major
  version's migration assumes the previous major version's result.
- **[settled]** **The written procedure is [`docs/upgrade.md`](upgrade.md)** — the file
  [decisions §14](decisions.md) names as the place, and it is on disk (v1.0 M1). This section is the
  rule; that file is the order of operations.

## 8. What is not promised

- **[settled]** **0.x → 1.0 is not promised to be painless.** v0.x promised nothing (§5); the 1.0
  migration is nevertheless written (§6) and takes a `.bak` first.
- **[settled]** **Skipping a major version is not supported.** An upgrade from 1.x straight to 3.x is
  refused, not attempted.
- **[settled]** **A private internal API is not covered by any of this.** If it is not documented and
  not in the `pub use` surface, this document says nothing about it.
- **[settled]** **The macOS / Linux packages and the Windows installers follow this document, not their
  own rules.** A package that disagrees with this file is a bug.

## 9. The four red lines, and how this policy passes them

[roadmap-v1.0.md §1](roadmap-v1.0.md) states four constraints — no built-in supervisor, no
officially operated service, not a general-purpose sandbox, and audit invariants that do not
move — and says they are the test the policy above has to pass. This section is that test, run
against the policy's own clauses.

- **No built-in supervisor.** §1 freezes the *protocol* and the `pub use` surface; §2's additions
  are all mechanism a caller drives (an optional field, a path, a capability, an event type), and
  §3 forbids removing or re-meaning any of them. Nothing in §1–§4 names an actor or lets one
  appear: the kernel's own dispatcher is an example under `examples/`, not a frozen component. A
  change that made the kernel decide for its caller would not be a minor release; it would be a
  different §1.
- **No officially operated service.** This document governs an **installation and a local
  protocol**, never a hosted one: §7 is an installer overwriting an installation, and §6 is the
  operator's own data directory. §2 may add an endpoint; no clause here promises a party that
  operates one on somebody's behalf.
- **Not a general-purpose sandbox.** The frozen surface is the *kernel's* — the control plane, the
  event vocabulary, the tool schemas, the `pub use` lists — and RISC-V's place in it (the substrate
  and the default implementation, not the only one) is a §1 statement this policy cannot quietly
  re-point. Turning the sandbox into a general-purpose one would be a change of **meaning** for an
  existing surface, which §3 makes a major version: it has to be decided, not drifted into.
- **The audit invariants do not move.** §1 freezes what a client can observe, the audit event
  vocabulary included; §2 allows an event type to be **added**, and §3 allows none to be removed or
  re-meant. The hash formula itself is not reachable from any clause here — it lives inside
  `audit`, and it is protected by [roadmap-v1.0.md §1](roadmap-v1.0.md) and
  [decisions §33](decisions.md), not by this policy. So the most a minor release can do is add an
  event, and adding one does not touch the formula.

**[settled]** The outcome of the test: the policy passes all four, because it only ever moves the
surface **additively** (§2) and makes every meaning-changing move a major version (§3). A future
amendment that would fail one of these four is not an amendment to this document.
