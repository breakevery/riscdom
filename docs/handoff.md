[中文](handoff.zh-CN.md) | English

# Handoff — carrying RiscDom into the next conversation

**This file is a cross-conversation handoff.** Section 1 is a volatile snapshot and is updated when
a release ships. Sections 2–12 are the stable constraints: they have not changed as the batches went
by, and they are the ones a new conversation must not break.

Repository `D:\codeagent\breakevery\riscdom`, remote `https://github.com/breakevery/riscdom.git`,
branch `main`. The close-out of every batch is the same: gate green → `scripts\commit.ps1 "<msg>"`
(which runs the gate itself) → push — and none of those remote-facing steps happens without the
current request authorising it (§2).

## 1. Snapshot — as of v0.5.0-preview.1 (update this section when the final release ships)

- **`v0.5.0-preview.1` is released as a pre-release**, and is deliberately **not** the Latest
  release: it exists so someone else can walk the golden path on a clean machine.
  <https://github.com/breakevery/riscdom/releases/tag/v0.5.0-preview.1> — assets
  `RiscDom_0.5.0-preview.1_x64_en-US.msi` (6,332,416 bytes) and
  `RiscDom_0.5.0-preview.1_x64-setup.exe` (4,463,424 bytes).
- **Latest is still `v0.4.0`.** A preview must never take that marker (§8's rule is about the final
  release, but the same discipline applies to the flag).
- **`v0.5.0` final waits on tester feedback**: a real walk of steps 1–2 on a clean machine, recorded
  against [golden-path-checklist.md](golden-path-checklist.md), plus two real runs with a real API
  key.
- Recent commits (newest first): `287ffdb` (release bump) ← `2a5d296` (README/CLA wording) ←
  `993e5d1` (CLA) ← `f662ad7` (self-contained export) ← `20a7c6d` (source snapshot in the index) ←
  `a22112f` (abandoned export, workspace default path, two-run compare) ← `26ad597` (run interval
  export) ← `e70457f` (golden-path design).
- Tags: `v0.5.0-preview.1` = `cea44f7b9920a079422217f811afb49350e08477` →
  `287ffdb095e1659b89a8cafe040647ada64d0026`; `v0.4.0` = `25bd3da3c31c1d1ec7e163f3835b0c2bbb74546d`
  → `15fda1f6d76d53a4ff1b621c2d3d91f0b4b87311`; `v0.3.1` = `d8fdba66a366632ca569d8db2657ab5a566b991c`
  → `b9be9111c620faad686c7a9d095e0ebc04b31225`.
- Test totals at that commit: **281 passed / 0 failed / 8 ignored / 79 suites**; CI green
  (`scripts/gate.sh` on `ubuntu-latest` plus gitleaks).
- Open items: the temp directories under `%TEMP%` have not been cleaned (the deletion confirmation
  was not granted); CLA.md awaits a lawyer's eye; no macOS/Linux support; the real first walk has
  not happened.

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

## 8. The v0.5 release gate is the walk, not the code

v0.5 ships when a person walks steps 1–2 on a clean machine with a real API key and records it
against [golden-path-checklist.md](golden-path-checklist.md) — machine, OS build, QEMU/GCC versions
**and their sources**, provider and model, the preflight's four steps, the two runs' short
fingerprints, the exported file and its `audit-verify` verdict, and any failure verbatim. Steps 3–7
are covered by `cargo test -p host --test golden_path -- --ignored`. "The implementation is
complete" is not the gate.

## 9. v0.6 starts at golden-path step 8

v0.6 is the automatic comparison of two runs — which fields differ between their fingerprints —
which v0.5 deliberately stops short of. The parallel items in `PROJECT_CONSTITUTION.md` §10's v0.5
roadmap (QEMU stdio, macOS/Linux, several VMs, incremental snapshots, session encryption, several
AIs, a bilingual interface) are **not** v0.6 content: they are candidates to be chosen or dropped.

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
