[中文](RELEASE_NOTES.zh-CN.md) | English

# RiscDom v0.5.0

**The golden path, end to end:** install RiscDom on Windows, point it at QEMU and a RISC-V compiler,
configure a model, run an agent task, save a snapshot, export that run's audit record, roll back to
the snapshot, then change one configuration field and run again — seven steps, by hand, in the app.

## What this release adds

- **All seven steps are real; the automatic comparison is not** (that is v0.6). The design, the
  settled decisions and the scope are in [docs/golden-path.md](docs/golden-path.md).
- **A run's record exports self-contained:** *Settings → Audit* writes one run as JSONL, from the
  chain's **first event** to the event that **closes the run**. The file is anchored at genesis, so
  an empty database plus `audit-verify` judges it with nothing carried over from the machine that
  produced it. An abandoned run's file ends on its `host.run.abandoned` marker; an open run is
  refused rather than truncated.
- **A run names the snapshot it came from** (`恢复自 <snapshot>`), and **two runs can be compared side
  by side** — short and full fingerprints, start time, status, source snapshot.
- **The walkthrough's findings are fixed**: the audit tab's run rows no longer hide their content
  behind a horizontal scrollbar, the snapshot prompt no longer suggests the same name every time,
  tool-call rows show a state instead of literal `?` marks, the preflight sentence no longer
  impersonates its own button, the audit filter follows what you type, and QEMU no longer opens a
  console window over the app.

## What was verified — and what was not

**Verified**

- The gate: `cargo fmt`, `cargo clippy -D warnings`, `cargo check`, the full `cargo test`
  (**281 passed / 0 failed / 8 ignored**, 79 suites), `npm run build`, seven UI probes, the mirror
  guard, the wix-version guard and the bilingual-documentation check. Green locally and in CI.
- The **end-to-end walk of steps 3–7** against a real QEMU guest, driven by a mock model
  (`cargo test -p host --test golden_path -- --ignored`), including `audit-verify` over the exported
  file.
- **A walkthrough of all seven steps, once** —
  [walkthroughs/2026-09-19-preview1-local.md](walkthroughs/2026-09-19-preview1-local.md): the
  developer's machine, a **real model**, a **real API key**, the **installed MSI**. Every step
  passed.

**Not verified**

- **A clean-machine walk.** The walkthrough above ran on a machine that already had QEMU and a
  RISC-V GCC installed, so it does **not** prove that step 1 works from nothing. A walk by somebody
  else, recorded against [docs/golden-path-checklist.md](docs/golden-path-checklist.md), was this
  release's plan and has not happened yet; it stays open as a v0.5.x strengthening item.
- **macOS and Linux.** Windows only; QMP and the serial console go over TCP.
- **The installers are unsigned.** Windows SmartScreen will warn the first time you run one;
  "More info → Run anyway" is expected. Nothing is downloaded; the app ships no key.
- Chinese text entry through the UI was exercised by a real keyboard only indirectly (the
  walkthrough's synthetic input mangled CJK, which the record calls out as a testing limitation).

## What a tester needs

- **Windows 10/11**, **QEMU** (`qemu-system-riscv64`, installed by you — RiscDom guides you to
  `winget` or the official page and never bundles or downloads it), **a RISC-V bare-metal GCC**
  (xPack `riscv-none-elf-gcc` or an equivalent `riscv64-unknown-elf-gcc`), and **an API key** for the
  provider you choose (or a local provider such as Ollama / LM Studio).

## How to report back

1. Walk the steps with **[docs/golden-path-checklist.md](docs/golden-path-checklist.md)** open and
   fill it in as you go — it asks for the machine, the OS build, the QEMU and GCC versions *and where
   each came from*, the provider and model, the preflight's four steps, the two runs' short
   fingerprints, the exported file's path and its `audit-verify` verdict, and anything that failed
   (which step, the raw error, whether it was recoverable).
2. Open an issue at <https://github.com/breakevery/riscdom/issues> and **paste the filled checklist**;
   put it in `walkthroughs/` if you prefer the repository.
3. A walk nobody wrote down is a walk nobody can check — the filled template is the report.

## Known limitations

- **Windows only**, and the interface is Chinese-only (no language switch yet).
- **No incremental or encrypted snapshots**; session storage is plain local SQLite.
- **One VM at a time** — the GUI drives a single host-owned VM.
- **The audit log has no retention policy yet**: it grows with use and is never pruned.
- **The automation gap in the walkthrough**: it was driven by scripted input, so a human should
  confirm the two items its record marks as needing a person — the model-change save, and typing
  Chinese into the chat box.

## Security

This project ships **no** API key: all model access is bring-your-own-key. Keys never leave your
machine, the audit log lives outside the AI and is append-only, and nothing is uploaded anywhere.
QEMU and the RISC-V toolchain are separate programs under their own licences; see
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md). The full statement and the reporting process are in
[SECURITY.md](SECURITY.md).

## License

[Apache License 2.0](LICENSE). Contributions need the [CLA](CLA.md) — see
[CONTRIBUTING.md](CONTRIBUTING.md).
