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

- **The query API and the one event envelope are in** (v0.9 batch 3/N). All 26 query
  endpoints of `docs/control-plane-api.md` §5.1 answer over HTTP — plus the host-local
  `/v0/health`, `/v0/status`, `/v0/events`, the reserved `/v0/resources` (501), and a
  new `405 method_not_allowed` — and every transport now wraps what it sends in the one
  envelope, built in `host/src/events.rs`. The three payload shapes the events document
  calls changed have landed (`vm:state` always carries `name`, `audit:failed` uses
  `message`, `toolchain:download` is tagged `state`); the other eight are untouched.
  The webview unwraps them at its single boundary (`ui/src/api/tauri.ts`); the worker's
  line protocol and the SSE stream carry the envelope as-is. A client walkthrough is in
  [docs/control-plane-client-guide.md](control-plane-client-guide.md). Still open: the
  controls (next batch), `gap`/`Last-Event-ID` (next batch), and capability enforcement
  (a later batch).
- **The control plane has a skeleton** (v0.9 batch 2/N). A new `server` crate —
  binary `riscdom-server` — binds the documented surface: `GET /v0/health`,
  `GET /v0/status`, `GET /v0/events` (SSE, the `hello` and `event` frames), the B1
  error model, and the `Authn` hook with `NoAuth` as the v0.9 default. It is Layer 3
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
  runs are compared field by field — the data layer and the API ([../host/src/run_diff.rs](../host/src/run_diff.rs),
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
  features* shows `0.5.0`). What it proves and what it does not is in [RELEASE_NOTES.md](RELEASE_NOTES.md).
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
as the local walk standing in for it. Steps 3–7 are covered by `cargo test -p host --test golden_path
-- --ignored`; step 8's comparison has **no** such automated walk yet (§9). "The implementation is
complete" is not the gate.

## 9. v0.6 starts at golden-path step 8 — and the step is delivered

v0.6 is the automatic comparison of two runs — which fields differ between their fingerprints —
which v0.5 deliberately stops short of. **Delivered** in batches 1–2 (`host/src/run_diff.rs`,
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
