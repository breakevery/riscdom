[中文](handoff.zh-CN.md) | English

# Handoff — carrying RiscDom into the next conversation

**This file is a cross-conversation handoff.** Section 1 is a volatile snapshot and is updated when
a release ships. Sections 2–12 are the stable constraints: they have not changed as the batches went
by, and they are the ones a new conversation must not break.

Repository `D:\codeagent\breakevery\riscdom`, remote `https://github.com/breakevery/riscdom.git`,
branch `main`. The close-out of every batch is the same: gate green → `scripts\commit.ps1 "<msg>"`
(which runs the gate itself) → push — and none of those remote-facing steps happens without the
current request authorising it (§2).

## 1. Snapshot — `v0.6.0-preview.1` is out as a pre-release; `v0.5.0` is still Latest (update this
section when the next release ships)

- **`v0.6.0-preview.1` is released as a pre-release** (v0.6 batches 1–2, released in batch 4): two
  runs are compared field by field — the data layer and the API ([../host/src/run_diff.rs](../host/src/run_diff.rs),
  `AppState::compare_run_fingerprints`, the `compare_run_fingerprints` command) and the collapsed
  block under the audit tab's two-run panel. A pre-release takes **no Latest marker**, so `v0.5.0`
  stays the Latest release. Assets: `RiscDom_0.6.0-preview.1_x64_en-US.msi` and
  `RiscDom_0.6.0-preview.1_x64-setup.exe`, built with `bundle.windows.wix.version = "0.6.0"` (WiX
  cannot take a pre-release `ProductVersion`), so *Apps & features* shows `0.6.0` while the artifact
  names keep the package version. What it proves and what it does not is in
  [RELEASE_NOTES.md](../RELEASE_NOTES.md) — first among the gaps: **no human has walked the new
  interface**, which is why this is a preview.
- **`v0.5.0` is released, and it is the Latest release.**
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
- Recent commits (newest first): `6c5ddb65` (the changelog and the handoff snapshot) ← `b5aac541` (the
  field-level diff in the audit tab) ← `4d407cea` (the fingerprint diff: data layer + API) ←
  `287ffdb` (the v0.5.0-preview.1 release bump) ← `2a5d296` (README/CLA wording) ← `993e5d1` (CLA) ←
  `f662ad7` (self-contained export) ← `20a7c6d` (source snapshot in the index).
- Tags: `v0.6.0-preview.1` is this preview (the commit that versions this file); `v0.5.0` is the
  release before it and the one holding the Latest marker; `v0.5.0-preview.1` =
  `cea44f7b9920a079422217f811afb49350e08477` → `287ffdb095e1659b89a8cafe040647ada64d0026`;
  `v0.4.0` = `25bd3da3c31c1d1ec7e163f3835b0c2bbb74546d` → `15fda1f6d76d53a4ff1b621c2d3d91f0b4b87311`;
  `v0.3.1` = `d8fdba66a366632ca569d8db2657ab5a566b991c` → `b9be9111c620faad686c7a9d095e0ebc04b31225`.
- Test totals at the release commit: **291 passed / 0 failed / 8 ignored / 80 suites** (v0.5.0 was
  281 / 0 / 8 / 79). The gate is 12 steps, green locally and in CI (`scripts/gate.sh` on
  `ubuntu-latest` plus gitleaks).
- Open items: the temp directories under `%TEMP%` have not been cleaned (the deletion confirmation
  was never granted); CLA.md awaits a lawyer's eye; no macOS/Linux support; **the clean-machine walk
  by someone else has not happened** — still a v0.5.x strengthening item rather than a blocker;
  **the preview's interface has not been walked by a person** — disclosed in [RELEASE_NOTES.md](../RELEASE_NOTES.md),
  and the first thing a walk of step 8 should cover.

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
