[中文](golden-path.zh-CN.md) | English

# The golden path — v0.5 design proposal

> **Status: proposal (v0.5 batch 0).** No code in this batch. §2 is reconnaissance: facts read out of
> this repository, each with the file that shows it. §3–§7 each give **a recommendation, the
> alternatives, and what they cost**. §8 lists the decisions only the project owner can make.

## 1. What the golden path is

The main line for v0.5–v0.6, as `PROJECT_CONSTITUTION.md` §10 now states it: a developer who has
never seen this repository gets from a fresh machine to a compared re-run.

```text
install → create an environment → run an agent task → save a snapshot →
get an audit record → roll back → change the configuration and run again → compare the two runs
```

The first seven steps, done by hand, are **v0.5**. The eighth — the automatic comparison — is
**v0.6** and is explicitly out of scope here.

## 2. What already exists

### 2.1 Install

Two environment dependencies, both handled in *Settings → 工具链*:

- **RISC-V GCC** — auto-discovery (`probe_toolchain`: `RISCDOM_RISCV_GCC` / `RISCV_GCC` → known paths
  → `PATH`), a manual pick through the native file dialog, or the one-click xPack download
  (`start_toolchain_download`, with progress and cancel).
- **QEMU** — auto-discovery and a manual pick (`probe_qemu` / `set_qemu_path`). Since v0.4 #4 it is
  **not downloaded**: the app guides the user to `winget` or the official page
  ([qemu-distribution.md](qemu-distribution.md) §5).

The Rust/Node toolchain is a **developer** prerequisite, not a user one — it belongs to the build,
not to the golden path.

### 2.2 Create an environment

There is **no first-class "environment" object** today. What exists is configuration plus a check:

| Piece | Where | Backend |
|---|---|---|
| Provider + key + model | *Settings → 模型* | `get_provider_presets`, `probe_local_llm`, `set_llm_config`, `has_stored_key` / `load_stored_key`, `get_llm_readiness` |
| Compiler path | *Settings → 工具链* | `probe_toolchain` / `set_toolchain_path` / `clear_toolchain_path` |
| QEMU path | *Settings → 工具链* | `probe_qemu` / `set_qemu_path` / `clear_qemu_path` |
| Capability check | *Settings → 工具链 → 环境预检* | `preflight_status` / `run_preflight` / `acknowledge_preflight` |

The key may live in the OS keyring; paths and the preflight cache live in `settings.json` in the app
data directory. The preflight compiles a four-line guest and boots it on the real paths, reports
which of four steps failed, and records a "continue anyway" ([preflight.md](preflight.md)).

### 2.3 Run an agent task

`run_agent(user_input)` on the host: it mints a run id, computes the configuration fingerprint,
appends `run.start`, runs the agent loop (compile → start VM → serial → iterate), appends `run.end`,
and returns an outcome (`final` / `max_iterations` / `failed`) plus the streamed chat.

### 2.4 Save a snapshot

| Action | Command | Notes |
|---|---|---|
| Save | `save_snapshot_real(name)` | Needs a **host-owned running VM**; the file lands in `.riscdom/snapshots/<name>.mig` (`tcp-relay`). Older `reboot-fallback` `.json` snapshots are still listed. |
| List | `list_snapshots()` | name / size / created_at / mode — the list in *Settings → 快照*. |
| Restore | `resume_from_snapshot_real(name)` | Stops the current VM, then pushes the stream through `-incoming tcp:`. |
| Delete | `delete_snapshot(name)` | |

The name is asked for with `window.prompt` — the one remaining text prompt for a path-like value
(toolchain and QEMU already use the native dialog).

A restore **is its own run**: `run.start` records `resumed_from_snapshot`, and `parent_run_id`
points at the most recent run of this process (it is `None` after a restart). The link is
one-directional: a run knows which snapshot it restored, but a snapshot does not know which run
produced it — `snapshot_producers` is an in-memory map, lost on restart.

### 2.5 Get an audit record

- *Settings → 审计* shows the event count, the chain verdict (`Intact` / `Broken`), an actor filter,
  the recent events, and a **read-only run list** (status, short fingerprint, time, parent).
- `export_audit_jsonl(path)` **exists and works** — it writes every event as JSONL, in id order, with
  `id` / `timestamp_ms` / `actor` / `action` / `detail` / `prev_hash` / `hash`, so an outside tool can
  re-verify the chain — but **nothing in the UI calls it**. The only export the UI offers is the
  serial log (`exportSerialLog`, used by the console panel).
- `audit-verify <path-to-db> [--runs]` is the independent checker: read-only, prints
  `Intact { length: N }` or `Broken { at_id, reason }`, exit `0` / `1` / `2`. Rebuilding the derived
  index is a separate binary (`audit-rebuild`), deliberately not part of the checker.

**The gap** is not "no export" — it is that the export nobody can reach is not scoped to a run:

- `RunRecord` (in `audit`) carries `start_seq` / `end_seq`, the audit-id interval of that run, so the
  interval exists on the chain.
- `RunView` (the shape the UI sees) **drops** `start_seq`, `end_seq` and `fingerprint_schema`.
- `EventFilter` filters by actor, action prefix and timestamp — **not by id**, so today's `list`
  cannot express "this run's interval" either.

### 2.6 Roll back

`resume_from_snapshot_real` plus the retry that survives a port collision. Covered in §2.4.

### 2.7 Change the configuration and run again

Everything that matters is already in the fingerprint document
([run-provenance.md](run-provenance.md) §1.2): `schema` (fingerprint schema, app version), `llm`
(provider, base URL, model — never the key), `vm` (memory, machine, cpu, QEMU path + reported
version, snapshot mode), `toolchain` (resolved GCC path + version, discovery source). Changing any of
those and running again genuinely produces a different fingerprint, and both runs appear in the run
list. **What is missing is readability**: nothing shows *which fields* differ between two runs.
(v0.5 has since given each run its fingerprint and, for a restore, the snapshot it came from, plus a
side-by-side view of two runs — §7/§8 below; the field-by-field diff stays v0.6 work.)

### 2.8 What already walks part of the path

| Artefact | Covers | Mode |
|---|---|---|
| `host-core/tests/e2e_ui.rs` | workflow 3 end to end (compile → VM → serial → audit) with a **mock LLM**, and prints the 5c-3 failure report | `--ignored` |
| `host-core/tests/diagnosis/mod.rs` (+ `run_diagnosis.rs`) | the report itself, wording pinned | normal |
| `sandbox/tests/snapshot_real.rs`, `snapshot.rs` | real save / restore of a snapshot, with QEMU | normal / ignored |
| `host-core/tests/snapshot_commands.rs`, `run_provenance.rs`, `qemu_path_snapshot.rs` | host-level snapshot commands, run records, restore uses the configured QEMU | normal |
| `audit/tests/verify_bin.rs`, `run_verify_cli.rs`, `run_rebuild_cli.rs` | the CLI behaviours of `audit-verify` / `audit-rebuild` | normal |
| `agent/tests/real_api.rs` | a real API key, end to end | `--ignored` |

**Nothing walks all seven steps.** There is no test, script or checklist that goes from "save a
snapshot" through "export the record" to "change the configuration, run again".

## 3. The seven steps, defined

Each step: what goes in, what the user does, what must be true afterwards, and how it fails.

| # | Step | Input | Action | Expected result | Failure mode |
|---|---|---|---|---|---|
| 1 | Install | A machine with Windows | Install QEMU (guided) and let the app find the compiler (or download it) | Compiler and QEMU resolved, paths remembered in `settings.json` | Nothing found → the app names every location it searched; a set-but-missing `RISCDOM_QEMU` is an error, not a fallback |
| 2 | Create an environment | LLM key (or a local provider) | Pick a provider, paste the key, run the preflight | Provider/model saved, readiness true, preflight green over all four steps | Bad key → readiness names the reason; a failed preflight step names the step and suggests the fix |
| 3 | Run an agent task | A request in plain language | Send it | A run with an id, a fingerprint and an interval; the guest was compiled, booted, read | `max_iterations` / `failed` → the diagnosis report names the first failing step |
| 4 | Save a snapshot | A running host-owned VM | Name it, save | `.riscdom/snapshots/<name>.mig`, listed with size and time | No running VM → the button is disabled and says why; a failed save reports the QEMU/relay error |
| 5 | Get an audit record | A finished run (including one the chain marks abandoned) | **Export it** from the run list | A file that verifies **on its own** (an empty database plus `audit-verify`): it starts at the chain's first event and ends on the run's `run.end`, or on its `host.run.abandoned` marker | Chain broken → export must not pretend otherwise |
| 6 | Roll back | A snapshot | Restore | A **new** run whose `parent_run_id` points at the previous one and whose `run.start` names the snapshot | A dead port / QMP reset → retried three times, then reported; a missing snapshot is rejected before the VM stops |
| 7 | Change the configuration and run again | Any one fingerprint field | Change it, run the same request again | A second run, same request, **different fingerprint** | An unreadable difference: the user sees two 64-character digests, not "only QEMU changed" |

## 4. Audit export

**Recommendation: export one run's record, as plain event JSONL, into the workspace, from a button
in the audit tab.**

- **What**: the chain **from its first event up to the event that closes the run** — which includes
  that run's `run.start` (carrying `fingerprint_schema` and the canonical `fingerprint_json` that
  was hashed) and its `run.end`, and everything the run happened among. Nothing new: the run's
  metadata is *already inside* the exported events, so the file explains itself and stays verifiable
  line by line. A run the chain marks **abandoned** has no `run.end`; its record stops at that
  run's `host.run.abandoned` event instead, so the file's last line still says why the run stopped.
  *(v0.5 batch 4: the export starts at genesis rather than at `run.start`, because only then does
  `verify_chain` — which begins at the genesis link — judge the file **in an empty database**. The
  slice form was removed rather than kept alongside: two meanings of "export" is one too many, and
  the one worth keeping answers "is this record intact" by itself.)*
- **Format**: the JSONL that `export_jsonl` already writes unchanged — `id` / `timestamp_ms` /
  `actor` / `action` / `detail` / `prev_hash` / `hash` per line, id order. A reader can re-hash each
  line and check the links without this application.
- **Where**: a default path under the workspace, offered in a native *save* dialog so the user can
  choose. The app asks the host for the workspace root and offers `<root>/<run_id>.audit.jsonl`; the
  host is the one that knows where the workspace is. (The workspace is what the export command is
  allowed to write to; `.riscdom/` is skipped by the file list, which is why a visible default is
  better than the hidden internal directory.)
- **Who**: the user, per run, from the run list in *Settings → 审计*.

Alternatives and their costs:

| Alternative | Why not (as the v0.5 default) | Cost if chosen |
|---|---|---|
| Keep exporting the **whole chain** (today's command), just add a button | It answers "give me everything", not "give me this run". Two runs of the same task then differ only by digests inside a large file | Cheapest to build (~a button); the comparison work in v0.6 gets harder |
| Export the run **plus a synthesised header line** (`run.export`) | The format stops being "exactly the chain"; either a fake event enters the file or the format forks from `export_jsonl` | A second format to document, verify and keep in sync |
| Export **with the chain verdict** computed at export time | Mixing an assertion into a record; the verdict belongs to the checker, not the file | Small, but the file now claims something it cannot prove |
| A CLI subcommand (`audit-export <db> --run <id>`) instead of a UI button | The golden path is walked by a person in the app; a CLI-only export leaves step 5 invisible | More surface; still needs the id-range query |

**Prerequisite for whichever is chosen**: the id interval must reach the caller. Either `RunView`
carries `start_seq` / `end_seq`, or the export command takes a run id and resolves the interval in
the audit layer. Recommendation: the run id — the UI should not have to know about sequence numbers.

## 5. "Create an environment": a concept, or just done configuring?

**Recommendation: not a new first-class object in v0.5.** Define step 2 as *"the three
configurations plus a recorded preflight"*, and make the definition explicit in the docs and in the
preflight's cached result. A run's fingerprint already captures the effective environment, so
"which environment was this?" is answerable from the chain without inventing anything.

Alternatives and their costs:

| Alternative | Why not as the v0.5 default | Cost if chosen |
|---|---|---|
| A named **environment** (name → provider, paths, VM settings; switch with one click; several per machine) | It is a real feature with its own UX, storage and migration story, and v0.5's target is one person getting to a compared re-run — not managing environments | Large: new state, new settings shape, new UI, new tests |
| A lightweight **"environment record"**: a name attached to a run, so two runs can be labelled | Half the feature above, and the fingerprint already does the work; a name adds a way for the label to disagree with the configuration | Medium: storage, migration, UI, and the "label vs reality" question |
| Nothing at all (status quo, but written down) | Then "create an environment" stays a vague phrase in a roadmap | Free |

## 6. How the seven steps get proved reproducible

**Recommendation: two layers — an `--ignored` end-to-end test that walks steps 3–7, plus a written
manual checklist for steps 1–2 that a person follows on a clean machine
([golden-path-checklist.md](golden-path-checklist.md)).** Steps 1–2 are machine
setup and cannot be honestly automated in this repository's CI (no QEMU, no GUI on the runner);
steps 3–7 are exactly what the existing mock-LLM harness already does most of.

| Option | Recommendation | Reasoning and cost |
|---|---|---|
| **(a) `--ignored` e2e test, mock LLM, real QEMU**: run a task → save a snapshot → restore it (a second run with a parent) → export the run interval → `audit-verify` the exported file → change one fingerprint field → run again → assert the two fingerprints differ | **Recommended** | Reuses `host-core/tests/e2e_ui.rs` and the 5c-3 diagnosis harness. Cost: one new test module and a small export path. It cannot cover install/config (steps 1–2) and does not use a real model |
| (b) A **manual checklist** in the docs ([golden-path-checklist.md](golden-path-checklist.md)), walked once per release by a person with a real API key | **Recommended, for steps 1–2** | The only honest way to cover installing QEMU and pasting a key. Cost: human time per release, and it must be *recorded* somewhere to count |
| (c) A **script** that drives the app's commands headlessly (no GUI) | Rejected for v0.5 | Duplicates the e2e test with more machinery and no extra coverage |
| (d) A GUI automation (click the app) | Rejected for v0.5 | Fragile, and the repo has no harness for it |

The recommendation assumes the export lands in the workspace: the e2e test can then assert the file
exists, run `audit-verify` logic over it, and delete it — with no dialog in the way.

## 7. v0.5 scope

| Step | State | v0.5 work |
|---|---|---|
| 1 Install | Exists (guided QEMU; GCC discovery/download) | None, beyond documenting the one-line checklist |
| 2 Create an environment | Exists as configuration + preflight | Document the definition (§5); no new concept |
| 3 Run an agent task | Exists | None |
| 4 Save a snapshot | Exists | None (the `window.prompt` name is a candidate, not a requirement) |
| 5 Get an audit record | **Two-thirds missing** | **The work of this version**: expose the run interval, scope the export to it, add the UI entry point, and prove it with a test |
| 6 Roll back | Exists | None |
| 7 Change the configuration and run again | Mechanism exists, readability does not | **Minimal**: show the fingerprint per run, let two runs be selected for a side-by-side digest view, and show which snapshot a run came from; do **not** build a diff |

Smallest honest v0.5, in one sentence: **make step 5 real, make step 7 legible, and write down what
steps 1–2 mean.**

## 8. Decisions (settled)

All eight were settled by the project owner on 2026-09-19; §4–§6's recommendations are the
decisions, with the two additions below marked.

1. **Export scope: one run's interval.** The whole-log export stays for "give me everything"
   (`export_audit_jsonl`); the golden path's step 5 uses the run-scoped export.
2. **Export destination: a visible path in the workspace**, chosen in a native *save* dialog. The
   host still refuses a path outside the workspace, and that refusal is shown to the user rather
   than swallowed. *(Decision: the UI uses `save()`, which is why `dialog:allow-save` joins the
   capability set — pinned by `ui/scripts/probe-ui-dialog.mjs` so it cannot grow quietly.)*
3. **The interval comes from the run id.** `export_run_audit(run_id, path)` resolves
   `[start_seq, end_seq]` in the audit layer; **`RunView` is unchanged** — the UI never handles
   sequence numbers.
4. **No CLI export in v0.5.** `audit-verify` and `audit-rebuild` keep their shapes; an export
   subcommand is a v0.6 question.
5. **"Environment" stays a phrase, not an object.** Step 2 is the three configurations plus a
   recorded preflight, and it is documented as such.
6. **The release gate includes a human walk of steps 1–2 with a real API key**, and it must be
   **recorded against a checklist template**: a template in the repository that a person fills in
   per release — machine and date, OS build, QEMU and GCC versions *and where each came from*
   (`winget` / manual / in-app download), provider and model, the preflight verdict, the two runs'
   fingerprints, and the exported file with its `audit-verify` verdict. A walk nobody wrote down is
   a walk nobody can check. The template is [golden-path-checklist.md](golden-path-checklist.md).
7. **Step 7 stays minimal in v0.5**: each run shows its fingerprint and which snapshot it came
   from, and two runs can be selected to see their short and full digests, start time and status
   side by side. Diffing the fingerprint field by field is v0.6.
8. **Snapshot naming keeps `window.prompt` in v0.5.** It is inconsistent with the native dialogs
   beside it and worth fixing, but it is not on the golden path.

## 9. Not doing

- **The automatic comparison (step 8).** v0.6. v0.5 stops at "the two runs are recorded and their
  fingerprints differ"; comparing them automatically needs a diff of the fingerprint document, which
  is a design of its own.
- **Any cross-platform work.** The path is Windows-only; macOS / Linux is a v0.5 roadmap item of its
  own (item 3) and changes who can walk the path, not the path.
- **A different VM/LLM story, new snapshot formats, session encryption.** All separate roadmap items.
