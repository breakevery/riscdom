[中文](RELEASE_NOTES.zh-CN.md) | English

# RiscDom v0.8.0

> **This is a release, not a preview — with the same two caveats as v0.7.0.** The **macOS and Linux
> packages are built by CI and have never been launched on a real machine**, and they are **unsigned**
> (macOS Gatekeeper blocks a first launch; Windows SmartScreen warns on the installer). And **Windows
> remains the platform the golden path has been verified on**: the clean-machine walk by somebody else
> still has not happened.

**v0.8 in one sentence:** the interface is fully bilingual (all 190 registered strings exist in both
languages), and under the surface the project grew the foundation for several agents on one machine —
one audit chain several processes can write, an identity on every event, snapshots that do not collide,
and a way to *dispatch* work to an executor instead of only calling it.

## What this release adds

- **The bilingual interface is complete** (v0.7 batches 1–2, finished in v0.8). Every one of the **190
  registered strings** exists in both languages — the settings pages, the toolchain and QEMU tabs, the
  audit and run views, the snapshot and session views, the diff panel, the preflight screens. The
  language switch in *Settings → Appearance* (**follow the system / 中文 / English**) applies to all of
  it. This is the one change in v0.8 that a user sees.
- **The audit chain is ready for several processes** (v0.8 batch 2). `audit.db` is one chain per
  workspace and stays that way; the connection now opens in **WAL** with a five-second busy timeout and
  `synchronous=NORMAL`, an append takes the write lock **before** reading the head (`BEGIN IMMEDIATE`),
  and a locked append is retried with backoff. A write that still fails is **never dropped silently**:
  the host logs it, raises an `audit:failed` event and shows a banner and a popup.
- **Every audit event names its agent** (v0.8 batch 3). An identity is `<device>-<pid>-<seq>`
  (`local-<pid>-<seq>` on one machine), minted once per process and once per agent inside it, and
  stamped on the events of the host, the agent loop, the tool layer and the sandbox. The field sits
  **beside** the chain: the hash formula, the `prev_hash` linkage and historical rows are untouched, so
  an older chain still verifies.
- **Snapshots are isolated per agent** (v0.8 batch 3). New snapshots go to
  `<workspace>/.riscdom/snapshots/<agent_id>/`, so two agents sharing a workspace can both save `snap1`
  without overwriting each other; reads fall back to the shared root, so a snapshot taken before this
  release still lists, restores and deletes.
- **A minimal dispatch abstraction** (v0.8 batch 4). `Task` / `TaskId` / `AgentId` / `TaskOutcome`, the
  two traits `AgentHandle` and `Dispatcher`, and a local implementation that routes by the executor the
  task names. **The remote half is deliberately absent** — that absence is the seam a later release
  fills in.
- **A two-process prototype, demonstrable from a terminal** (v0.8 main deliverable). `worker` is an
  executor process: one `Task` in on stdin, one `TaskOutcome` out on stdout, events on stderr, each
  executor owning its data directory. `cargo run -p worker --example dispatch` starts a small fleet of
  them, shares one workspace between them and dispatches to each concurrently. This is **foundation,
  not a shipped feature**: no AI supervisor, no remote executors, no user-visible surface yet.
- **Two documents** describe where the project stands: `docs/architecture-evolution.md` (the
  four-layer picture and the decisions behind it) and `docs/multi-agent-foundation.md` (the four shapes
  v0.8 settled, written from the code, with what is still open).

## Install by platform

- **Windows 10/11** — `RiscDom_0.8.0_x64_en-US.msi` or `RiscDom_0.8.0_x64-setup.exe` from the release
  assets. The installers are **unsigned**, so SmartScreen warns the first time ("More info → Run
  anyway"). QEMU and a RISC-V bare-metal GCC are not bundled: *Settings → Toolchain* guides you to
  `winget install SoftwareFreedomConservancy.QEMU` (or the official page) and can download the xPack
  GCC itself.
- **macOS** — the CI `bundle` job builds `RiscDom_0.8.0_aarch64.dmg` (Apple Silicon) and the `.app`
  inside it. They are **unsigned**, so Gatekeeper blocks the first launch: right-click the app → *Open*,
  or run `xattr -dr com.apple.quarantine /Applications/RiscDom.app` once. **QEMU is not bundled**:
  `brew install qemu`. **Nobody has launched these packages on a real Mac yet.**
- **Linux** — from the same job: `RiscDom_0.8.0_amd64.deb`, `RiscDom-0.8.0-1.x86_64.rpm` or
  `RiscDom_0.8.0_amd64.AppImage`. **QEMU is not bundled**: install your distribution's
  `qemu-system-riscv64` (for example `sudo apt install qemu-system-misc`, `sudo dnf install
  qemu-system-riscv`). **Nobody has launched these packages on a real Linux machine yet.**

## What was verified — and what was not

**Verified**

- The gate, 13 steps: `cargo fmt`, `cargo clippy -D warnings`, `cargo check`, the full `cargo test`
  (**330 passed / 0 failed / 8 ignored**, 90 suites), `npm run build`, the eight UI probes, the mirror
  guard, the wix-version guard, the UI string registry check and the bilingual-documentation check.
  Green locally.
- The **string registry's completeness**: every registered key exists in both languages (the check is
  in the gate and has its own self-test), and the language switch's rules are pinned by the language
  probe.
- The **two-process prototype**, end to end and offline: the executor's protocol (one task in, one
  outcome out, a malformed task answered rather than crashed, a worker that never answers killed at the
  deadline), the supervisor's routing (a task for an executor that is not in the fleet is refused, not
  guessed at), and that two executors really are two processes with their own data directories.
- The **audit chain's multi-process writes** under a concurrency test, the **per-agent snapshot
  isolation** and the **identity on every event**, each pinned by its own test.
- The end-to-end walk of golden-path **steps 3–7** against a real QEMU guest, unchanged since v0.5.0
  (`cargo test -p host --test golden_path -- --ignored`).

**Not verified**

- **The macOS and Linux packages themselves.** They compile and bundle; nobody has installed or
  launched them on a real Mac or Linux machine. That is the next walk.
- **A clean-machine walk.** Still the strengthening item it was: [docs/golden-path-checklist.md](docs/golden-path-checklist.md)
  is the form, `walkthroughs/` is where a filled one goes.
- **The multi-agent work has no user-visible behaviour yet.** It is a foundation: the pieces exist and
  are tested, but no interface reaches them.
- **A clean-machine walk by somebody else, and real-key walks on a second OS.** As before.
- The installers and packages are **unsigned** — SmartScreen on Windows, Gatekeeper on macOS. Signing
  and notarization remain commercialisation-layer items.

## What a tester needs

- A **model provider** and **an API key** of your own (or a local provider such as Ollama / LM Studio),
  **QEMU** (`qemu-system-riscv64`, installed by you — see the platform notes above), and **a RISC-V
  bare-metal GCC** (xPack `riscv-none-elf-gcc` or an equivalent `riscv64-unknown-elf-gcc`; the app can
  download the xPack one).
- For the comparison: **two finished runs whose fingerprints differ** — change one configuration field
  (the model, say) between them.

## How to report back

1. Walk the steps with **[docs/golden-path-checklist.md](docs/golden-path-checklist.md)** open and fill
   it in as you go. On macOS or Linux, say which package you used and whether it opened at all.
2. Open an issue at <https://github.com/breakevery/riscdom/issues> and **paste the filled checklist**;
   put it in `walkthroughs/` if you prefer the repository.
3. A walk nobody wrote down is a walk nobody can check — the filled template is the report.

## Known limitations

- **Windows is the verified platform.** macOS and Linux build, but the golden path has not been walked
  there; their packages are unsigned and unlaunched.
- **The multi-agent foundation is unfinished on purpose.** Three edges are named and left for the next
  release: a dispatched outcome does not yet carry the **executor's own identity** (only the identity
  the task was addressed to); the executor binary still links the desktop toolkit because `host` depends
  on it unconditionally; and the environment-preflight directory is still shared between processes that
  share a workspace.
- **No AI supervisor.** The supervisor in this release is a dispatcher — a routing table, not an agent.
- **The diff block is one level deep**: it compares top-level fingerprint fields as whole values, not
  the keys inside them.
- **No incremental or encrypted snapshots**; session storage is plain local SQLite.
- **One VM at a time** per agent.
- **The audit log has no retention policy yet**: it grows with use and is never pruned.

## Security

This project ships **no** API key: all model access is bring-your-own-key. Keys never leave your
machine, the audit log lives outside the AI workspace and is append-only, and nothing is uploaded
anywhere. QEMU and the RISC-V toolchain are separate programs under their own licences; see
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md). The full statement and the reporting process are in
[SECURITY.md](SECURITY.md).

## License

[Apache License 2.0](LICENSE). Contributions need the [CLA](CLA.md) — see
[CONTRIBUTING.md](CONTRIBUTING.md).
