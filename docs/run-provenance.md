[中文](run-provenance.zh-CN.md) | English

# Run provenance — design (v0.4 batch 1a)

> **Status: signed off (2026-09-18).** These are the agreed decisions; §6 records them. This batch
> changes no code and no audit source — implementation is batch 1b. Everything below is grounded in
> the current implementation.

## 0. Goal

Make **one run** a first-class citizen of the audit log: every run gets a unique id, a
configuration fingerprint and an audit interval. v0.5 ("re-run with a different configuration")
and v0.6 ("compare two runs automatically") are built on top of it.

The design follows the constitution: the host owns the lifecycle, the audit log lives outside the
AI and stays append-only, and nothing the AI can reach may forge provenance.

## 1. Data model

### 1.1 Run ID

**Decided: `run_<uuidv7>` — 32 lowercase hex characters in canonical UUID form, prefixed.**

```text
run_0192f4c1-8a3d-7c2e-9f10-6b1d4e0a55aa
```

- UUIDv7 is time-ordered, so ids sort chronologically and are index-friendly.
- 128 random/time bits make collisions across machines, databases and exports a non-issue.
- Lowercase hex, no braces: filename-safe, JSON-safe, log-safe, unambiguous when copy-pasted.
- The `run_` prefix keeps it greppable and distinguishable from session ids and snapshot names.

**Evidence on cost:** `uuid 1.26.1` is *already* in the workspace lock file — it arrives
transitively via `tauri-utils` / `schemars`, which the `host` crate builds anyway. Making it a
direct dependency of **`host`** — the crate that mints the id, §4.1; the audit layer only stores it
as a string — (with the `v7` feature, whose `getrandom` is also already in the graph) adds a direct
edge and a feature flag, not a new third-party crate. `audit` stays free of it. Keep the prefix
hand-written on top of `Uuid::now_v7()`, so the storage layer never depends on a formatting helper.

Rejected alternatives (kept for the record):

| Option | Why not (cost) |
|---|---|
| `uuid` v4 (random) | Same dependency cost, but not time-ordered: run listings need an extra sort column, and ids leak no order to a human reading a log. |
| Home-grown `run_<yyyymmdd-hhmmss-ms>-<counter>-<rand4>` | No dependency, human-readable, sortable — but we own collision and clock-skew handling, and a clock jump backwards produces out-of-order ids. Acceptable fallback if a direct `uuid` edge is unwanted. |
| Integer `run_seq` (`AUTOINCREMENT`) | Smallest and exactly what the index wants — but not portable: two databases, two exports or two machines cannot be merged, and "re-run" comparisons in v0.5 want to name a run outside its own database. |

### 1.2 Configuration fingerprint

**Decided: SHA-256 over a canonical JSON document, with an explicit schema tag.**

```text
fingerprint      = sha256(canonical_json)
fingerprint_schema = "riscdom.run.fingerprint.v1"
display form     = first 16 hex characters (the full 64 are stored)
```

Canonicalisation rules (all cheap, all testable):

1. UTF-8, object keys sorted lexicographically, no insignificant whitespace.
2. Strings as-is; numbers normalised to integers where they are integers (memory MiB, iteration
   cap, timeouts in ms).
3. Windows paths normalised: absolute, lower-cased drive letter, forward slashes as separators.
4. Volatile values are **excluded** and must never leak in: run id, timestamps, session id,
   counters, temporary ports, workspace root, machine name.
5. **Secrets are excluded entirely** — not hashed, not truncated, not "just the last four". The API
   key is already excluded from everything the audit log stores; a hash of a key would still be a
   key-derived value in the chain. The fingerprint covers the *provider, base URL and model name*
   instead.

The canonical JSON **is part of the chain**: it travels inside `run.start`'s `detail_json` (§2.3), so
a fingerprint is never explainable only from the derived index — and the index can always be rebuilt
from the log alone.

The v1 field list is in [Appendix A](#appendix-a--fingerprint-fields-v1). Unknown or unreadable
values are recorded as `"unknown"` rather than omitted, so a fingerprint never silently changes
meaning — this matters for v0.5, where two fingerprints differing only by an unreadable field must
not look identical.

**Decided:** keep one run-level fingerprint, plus a nested `vm` object that can be compared on its
own (`fingerprint.vm.*`), so v0.5 can say "only QEMU changed" without diffing the whole document.
Nested objects cost nothing extra in canonical JSON.

**Decided: the system prompt appears as its own object and only as a hash** — `prompt.sha256`,
never the prompt text. That catches prompt drift without putting the prompt (or a path to it) into
the chain.

### 1.3 Audit interval

**Decided: record both ends of the interval in the index row.**

| Field | Meaning |
|---|---|
| `start_seq` | `id` of the `run.start` event (the run's first chained event) |
| `end_seq` | `id` of the `run.end` event; `NULL` while the run is open |
| `started_at_ms` | `timestamp_ms` of `run.start` |
| `ended_at_ms` | `timestamp_ms` of `run.end`; `NULL` while open |

`seq` (`audit_events.id`, `INTEGER PRIMARY KEY AUTOINCREMENT`) is the authoritative interval: it is
gap-free within one database, it is what the hash chain orders by, and it is what
`audit-verify` walks. Timestamps are for humans, the UI and cross-machine reports; they are
advisory because the system clock can move.

Interval convention: `start_seq` inclusive, `end_seq` inclusive, and the run owns every event with
`start_seq <= id <= end_seq` (see §2.2 for why membership is derived rather than stored per row).

### 1.4 Exporting a run's record

A run's record is exported as **the chain from its first event up to that run's end**, as plain event
JSONL (v0.5 batch 4). The interval above says where the run's own events are; the export deliberately
starts **earlier**, at genesis:

- the file has to be verifiable **on its own**. `verify_chain` begins at `GENESIS_PREV_HASH`, so a
  file whose first line links to an event it does not contain cannot be judged Intact — it could only
  be re-attached to the database it came from, which is exactly what a reader of the record does not
  have;
- the lines before the run (a session being created, the environment preflight booting its guest)
  are part of what the run happened *in*, they carry no run id of their own, and including them costs
  nothing;
- the slice form is **not** offered. Two meanings of "export" is one meaning too many, and the one
  worth keeping answers "is this record intact" without a second artefact.

The file keeps the whole-log shape (`id` / `timestamp_ms` / `actor` / `action` / `detail` /
`prev_hash` / `hash`), ends on the run's `run.end` — or on its `host.run.abandoned` marker for a run
whose process disappeared — and is written into the workspace from the run list. No new event type,
no header line; `audit-verify` and `audit-rebuild` are untouched.

## 2. Storage

### 2.1 What the current implementation gives us

```sql
CREATE TABLE IF NOT EXISTS audit_events (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp_ms INTEGER NOT NULL,
    actor        TEXT    NOT NULL,
    action       TEXT    NOT NULL,
    detail_json  TEXT    NOT NULL,
    prev_hash    TEXT    NOT NULL,
    hash         TEXT    NOT NULL UNIQUE
);
```

The chain hash is exactly:

```text
sha256(prev_hash | "|" | timestamp_ms | "|" | actor | "|" | action | "|" | detail_json)
```

Consequence, and the single most important constraint in this document: **the hash covers the five
fields above and nothing else.** A new column on `audit_events` would therefore be *outside* the
hash — anyone with a SQLite client could edit a `run_id` column and `audit-verify` would still
report `Intact`. Anything that has to be tamper-evident must live in `detail_json`, or be its own
event.

### 2.2 Options

| Option | Shape | Cost |
|---|---|---|
| **A. Column on `audit_events`** | `ALTER TABLE audit_events ADD COLUMN run_id TEXT` | Fast queries ("events of run X" = one indexed lookup). But the column is unhashed (see §2.1), so provenance becomes forgeable; extending the hash to cover it invalidates every existing row and forces a full chain rebuild. **Rejected.** |
| **B. Chain markers + derived index** (decided) | Two new actions (`run.start`, `run.end`) carry `run_id` + fingerprint inside `detail_json`; a separate `runs` table is written alongside as an index | Chain schema and hash semantics untouched; provenance is tamper-evident; the index is rebuildable from the chain. Costs a small migration, a rebuild path, and one extra write per run. |
| **C. Separate table only** | `runs` table, no chain markers | Cheapest to write, but the run metadata is not in the chain: editing a fingerprint or an interval in `runs` is undetectable by `audit-verify`. Rejected for the same reason as A. |

### 2.3 Decision

**Option B.** Concretely:

1. The host appends `run.start` when a run begins and `run.end` when it returns. Both are ordinary
   audit events, so they are hashed like everything else. `detail_json` of `run.start` carries:

   ```json
   {
     "run_id": "run_0192f4c1-8a3d-7c2e-9f10-6b1d4e0a55aa",
     "fingerprint": "<64 hex>",
     "fingerprint_schema": "riscdom.run.fingerprint.v1",
     "fingerprint_json": "<canonical JSON text — exactly the bytes that were hashed>",
     "session_id": "<host session id>",
     "parent_run_id": null,
     "resumed_from_snapshot": null
   }
   ```

   The canonical JSON is carried as a **string**, not as a re-serialised object, so the exact bytes
   that were hashed are always recoverable and the digest can be recomputed from the log alone.

   `run.end` carries `run_id`, `status` (`ok` / `failed` / `interrupted`) and the reason string.

2. A migration creates the index table (additive, `CREATE TABLE IF NOT EXISTS`):

   ```sql
   -- Every column below is derived from chain events; nothing here exists only in the index.
   -- `resumed_from_snapshot` joined in v0.5 batch 3; an older database gets it through an
   -- `ALTER TABLE runs ADD COLUMN` migration, and a rebuild fills it from the chain.
   CREATE TABLE IF NOT EXISTS runs (
       run_id            TEXT PRIMARY KEY,
       session_id        TEXT,
       parent_run_id     TEXT,
       resumed_from_snapshot TEXT,
       fingerprint       TEXT NOT NULL,
       fingerprint_schema TEXT NOT NULL,
       started_at_ms     INTEGER NOT NULL,
       ended_at_ms       INTEGER,
       start_seq         INTEGER NOT NULL,
       end_seq           INTEGER,
       status            TEXT NOT NULL
   );
   ```

3. The `runs` table is a **pure derived index**: it exists for fast listing and UI use, every one of
   its columns is rebuilt by scanning the chain for `run.start` / `run.end`, and nothing exists
   *only* in the index. It is not hash-covered either, so the design does not pretend it is
   tamper-proof — the rebuild (§3.3) is the check, and a rebuilt index that differs from the stored
   one is itself the finding.
4. The canonical JSON travels **in the chain**, as `fingerprint_json` inside `run.start`'s
   `detail_json` (the exact bytes that were hashed, kept as a string so no re-serialisation can
   change them). The index stores only the digest. Consequence: the log is self-sufficient — the
   configuration of any run can be recovered and re-verified from the chain alone, which is exactly
   what v0.6's comparison needs and what `audit-rebuild` relies on.
5. Membership is **derived from the interval**, not stored per event: `runs` rows carry
   `[start_seq, end_seq]`, and "which run does event N belong to" is a range lookup. This keeps the
   event schema frozen (option A's problem) while staying O(1) per lookup with an index on
   `start_seq`.

Cost summary: one migration, one extra append per run boundary, one rebuild routine, and a
reading path that must go through the index. In exchange the chain structure, the hash formula, the
append-only triggers and every existing row stay exactly as they are. The index adds speed, not
truth: it holds no fact the chain does not already hold.

## 3. Backward compatibility

### 3.1 Old records

Old events carry no run markers, so they belong to no run. They are read as **unattributed**, never
rewritten and never assigned a synthetic run. A `legacy` banner in the UI (later batch) makes that
explicit instead of pretending history was always instrumented. Reading an old database with new
code works: the `runs` table is created empty, `run.start` events simply do not exist yet.

### 3.2 Hash chain

**Decided: never recompute, never rewrite.** The chain is append-only by construction
(`BEFORE UPDATE` / `BEFORE DELETE` triggers raise `RAISE(ABORT, …)`); new markers are appended after
the existing events, exactly like any other event. New and old coexist in one chain, in one table,
with no marker, no version column and no second file.

The alternative — recomputing the chain to inject run ids into historical rows — costs a full
rewrite, destroys the append-only guarantee that the project is built on, and changes hashes that
are already quoted in released artifacts. **Rejected, and worth stating in the docs of any future
scheme.**

### 3.3 `audit-verify` (read-only) and `audit-rebuild`

The chain verdict keeps its exact meaning and its exit codes (`0` intact, `1` broken, `2` usage or
I/O error), so every existing script and CI step keeps working. `audit-verify` itself stays
**read-only**: it holds no path that writes to the log or to the index.

One additive, optional flag:

- `audit-verify <db> --runs` — cross-check the index against the chain: every `runs` row must match
  a `run.start` event with the same `run_id`, fingerprint and `start_seq`; every `run.end` must
  match its row; every open run must have no `end_seq`. Findings are reported as a separate section
  and, when they exist, change the exit code to `1` (the log and its provenance disagree, which is
  exactly the state an operator must not miss).

Rebuilding the derived index is a **separate binary**, `audit-rebuild <db>`:

- it rewrites `runs` from the chain alone — every column, the configuration text included, because
  `run.start` carries the canonical JSON in `detail_json`;
- it then runs the same cross-check and prints the result, so one command says whether the log's run
  provenance is consistent;
- exit codes: `0` rebuilt and consistent, `1` rebuilt but the log still reports a problem (index
  findings, or run markers the chain cannot form into runs — a rebuild cannot invent the missing
  counterpart of an orphan marker, and twice-started run ids are reported too), `2` usage error or
  unreadable database.
- it writes only to `runs`, never to `audit_events`, whose append-only triggers stay in force.

**Why separate rather than a `--rebuild-index` flag on the checker.** A checker that can write
cannot be trusted as a checker: one repair run would silently erase the very divergence `--runs`
exists to surface, and it would hand the read-only role a write path into the audit directory that
it does not need. The log's integrity verdict and the mutation of a derived cache are different
privileges, so they are different programs.

## 4. Ownership and lifecycle

### 4.1 Who mints the id

**The host, and only the host.** The host monitoring layer owns process lifecycle, the VM slot, the
session store and the UI channel (constitution §4); it is also the only layer that sees a whole run
start to finish. The `agent` and `sandbox` crates must not generate ids: they run inside the
boundary the AI influences, and an id the AI can choose is not provenance. The host passes nothing
downward: the run id is a host-side attribute recorded in the audit log, not a parameter the agent
loop can influence.

### 4.2 When a run starts and ends

**One run = one `AppState::run_agent` invocation.** That is the unit v0.5 re-runs with a different
configuration and the unit v0.6 compares, so the boundaries must match it exactly.

| Moment | Action |
|---|---|
| `run_agent` passes its readiness gates (LLM, toolchain, QEMU) | mint id, compute fingerprint, append `run.start`; the returned `seq` is `start_seq` |
| first agent/LLM/tool event | already inside the interval |
| `run_agent` returns (success, error, or the model's final answer) | append `run.end`, then update the `runs` row with `end_seq` / `ended_at_ms` / `status` |

A run that never returns (crash, kill, power loss) keeps `end_seq = NULL`; it is **open**, not
corrupt. On the next start the host may append `host.run.abandoned` (a normal chained event, with
its own detection timestamp and the abandoned `run_id`) and mark the index row `abandoned`. The
chain is never given a fabricated `run.end` for a run that never ended.

Relation to the session: a host session (conversation) spans many runs; `session_id` is recorded on
the run, and the relationship is one-to-many. Relation to the VM: the VM may outlive a run (v0.3
made it host-owned and reusable), so the run records the VM configuration it actually used in its
fingerprint and does not claim VM ownership.

### 4.3 Snapshot restore

**Decided: a new run, linked to the old one.**

`resume_from_snapshot_real` produces a materially different execution: different memory contents,
a different start point, and — after v0.3.1 — possibly a different QEMU binary. Recording it as a
continuation of the earlier run would break the "one run = one execution" contract that v0.5/v0.6
depend on. So a restore opens a new run with:

- `parent_run_id` = the run whose snapshot was resumed (or `NULL` for a snapshot restored after a
  restart, where the producing run is only known through the snapshot's own metadata),
- `resumed_from_snapshot` = the snapshot name,
- status `ok`/`failed` as usual.

The alternative (continue the old run) is rejected because an interval spanning a stop/start pair
silently contains events from two different VM instances.

## 5. Minimal v0.4 scope

**In scope (batch 1b — decisions signed off in §6):**

1. Audit: a `run.start` / `run.end` action pair (names + detail shape) and the additive `runs`
   migration; no change to `audit_events`, its triggers, or the hash formula.
2. Fingerprint: the canonicaliser, the v1 field list, and a unit test that pins the exact bytes for
   a fixed input (so a future change is a deliberate version bump, not an accident).
3. Host: mint the id, compute the fingerprint, append the markers around `run_agent`, maintain the
   `runs` row, and handle the open-run case at startup.
4. Read path: `list_runs` / `get_run` (read-only), plus the index rebuild routine.
5. Bins: `audit-verify --runs` (read-only cross-check) and the separate `audit-rebuild` (rebuild the
   derived index from the chain, then run the same check) — both additive, with the chain verdict
   and the `0` / `1` / `2` exit codes unchanged.

**Out of scope, deliberately:**

- Comparing two runs, diffing fingerprints, "what changed" reports — **v0.6**.
- Re-running with a different configuration, and the golden-path recorder — **v0.5**.
- Export bundles, run-level archives, remote backup — **v0.6 or later**.
- Any runs UI beyond the existing audit panels — later batch.
- Anything that changes `audit_events`, the hash formula, or historical rows — **never planned**.
- Storing prompts, source files or tool arguments beyond what the audit log already stores; the run
  row carries hashes and configuration, not content.

## 6. Decisions (signed off 2026-09-18)

1. **Run id scheme: UUIDv7.** `run_<uuidv7>` as in §1.1 — a direct `uuid` edge plus the `v7`
   feature, no new third-party crate.
2. **Storage shape: option B.** Chain markers (`run.start` / `run.end`) plus the derived index
   table. The `audit` crate gains two actions and one table; `audit_events`, its triggers and the
   hash formula stay untouched.
3. **Run granularity: one run = one `run_agent` call**, not one user-turn and not one session.
4. **Snapshot restore: a new run** with `parent_run_id` and `resumed_from_snapshot`, never a
   continuation of the producing run.
5. **"region configuration" was a typo for the runtime / VM configuration.** There is no region
   concept in the workspace and none is planned; the runtime and VM settings are covered by the
   nested `vm` object and the `agent` group in Appendix A.
6. **System prompt: its own object, hash only** — `prompt.sha256`; the prompt text is never stored
   in the chain.
7. **`audit-verify --runs` ships in v0.4**, and rebuilding ships as the **separate `audit-rebuild`
   binary** (§3.3), so the checker keeps its read-only role and the derived index is trustworthy from
   the day it exists.
8. **VM fingerprint: nested `vm` object**, so "only QEMU changed" is expressible without diffing the
   whole document.

### Correction to the first draft — the canonical JSON lives in the chain

The first draft stored the canonical JSON only in the index (`runs.fingerprint_json`). The signed-off
design moves it **into `run.start`'s `detail_json`**, so it is covered by the hash chain, and demotes
`runs` to a pure derived index:

- the chain is self-sufficient: a run's configuration can be recovered, re-hashed and re-verified
  from the audit log alone, with no dependency on a file outside the chain;
- `audit-rebuild` reconstructs the index **completely**, configuration text included, because the
  source is in the chain;
- the index holds no fact the chain does not already hold, so tampering with it is detectable by
  rebuilding and comparing.

## Appendix A — fingerprint fields (v1)

| Group | Fields |
|---|---|
| `schema` | `fingerprint_schema` (`riscdom.run.fingerprint.v1`), `app_version` |
| `llm` | `provider_id`, `base_url`, `model` (never the key) |
| `agent` | `max_iterations`, `request_timeout_secs`, tool set hash (names + descriptions + parameter schemas), compiler `march` / `mabi` / link address / injected-crt0 marker, language allowlist |
| `vm` | `memory_mb`, machine (`virt`), cpu (`rv64`), QEMU path + reported version, snapshot mode |
| `toolchain` | resolved GCC path + reported version, discovery source (`EnvVar` / `KnownPath` / `Path` / `Manual`) |
| `policy` | workspace policy version, extension allowlist, traversal guard marker |
| `prompt` | `sha256` of the system prompt text (the text itself is never stored) |

Every field is either read from the resolved configuration (not from the user's unvalidated input)
or recorded as `"unknown"`. Paths are normalised per §1.2 rule 3.

This document is what gets canonicalised and hashed; its exact text then travels in the chain as
`run.start`'s `fingerprint_json` (§2.3), so both the digest and the configuration it summarises are
recoverable from the log.

## Appendix B — new audit actions

| Action | Actor | Detail |
|---|---|---|
| `run.start` | `host` | `run_id`, `fingerprint`, `fingerprint_schema`, `fingerprint_json` (the canonical JSON text that was hashed), `session_id`, `parent_run_id`, `resumed_from_snapshot` |
| `run.end` | `host` | `run_id`, `status`, `reason` |
| `host.run.abandoned` | `host` | `run_id`, `detected_at_ms` (chain event; the index row becomes `abandoned`) |

`audit-verify` and the JSONL export treat them as ordinary events: no special case, no new field in
`audit_events`.
