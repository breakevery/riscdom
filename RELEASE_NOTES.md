[中文](RELEASE_NOTES.zh-CN.md) | English

# RiscDom v0.7.0

> **This is a release, not a preview — with two caveats.** The **macOS and Linux packages are built by
> CI and have never been launched on a real machine**, and they are **unsigned** (macOS Gatekeeper
> blocks a first launch; Windows SmartScreen warns on the installer). And **Windows remains the
> platform the golden path has been verified on**: the clean-machine walk by somebody else still has
> not happened. Both caveats are spelled out below.

**v0.7 in one sentence:** the interface can speak two languages (a self-built string registry plus a
switch in *Settings → Appearance*), and the same source now builds on Windows, macOS and Linux — while
the QEMU guidance follows the platform and tells you how to install what the app does not bundle.

## What this release adds

- **A self-built i18n facility, and a language switch** (v0.7 batches 1–2): `ui/src/i18n/` is a
  two-language string registry with **no third-party library**; `scripts/check-ui-strings.mjs` (in the
  gate, with its own self-test) fails the build when a key is missing in either language; and
  *Settings → Appearance* offers **follow the system / 中文 / English** — persisted to `settings.json`,
  it moves `lang` on `<html>` and re-renders live. The four v0.6 diff strings are the pilot, and **the
  rollout over the remaining ~190 interface strings is deliberately not done**: the facility is the
  value, and a kernel-shaped tool does not need a fully bilingual surface.
- **macOS and Linux builds** (v0.7 batches A–B): the same source builds on macOS (aarch64) and Linux
  (amd64). The CI `bundle` job runs `npm run tauri build` on both and uploads the results as workflow
  artifacts — `.app` + `.dmg`, and `.deb` + `.rpm` + `.AppImage`. The QEMU install guidance follows the
  platform (`winget` / Homebrew / the distribution's package, via `sandbox::qemu_discover::install_hint_for`),
  `icons/icon.icns` exists for the macOS bundle, and the Unix `-qmp unix:` argument is pinned by a
  cross-platform unit test.
- **`host` compiles off Windows again** (v0.7 batch 8): a Windows-only `extract_zip` had been called
  from a platform-independent `match` arm, so every macOS/Linux build died with `error[E0425]`.
  Non-Windows platforms now get a same-named stub that reports "zip archives are not supported on this
  platform". The new `bundle` job found this — it was the first time `host` was ever compiled off
  Windows.

## Install by platform

- **Windows 10/11** — `RiscDom_0.7.0_x64_en-US.msi` or `RiscDom_0.7.0_x64-setup.exe` from the release
  assets. The installers are **unsigned**, so SmartScreen warns the first time ("More info → Run
  anyway"). QEMU and a RISC-V bare-metal GCC are not bundled: *Settings → Toolchain* guides you to
  `winget install SoftwareFreedomConservancy.QEMU` (or the official page) and can download the xPack
  GCC itself.
- **macOS** — the CI `bundle` job builds `RiscDom_0.7.0_aarch64.dmg` (Apple Silicon) and the `.app`
  inside it; download them from the workflow run's *Artifacts*. They are **unsigned**, so Gatekeeper
  blocks the first launch: right-click the app → *Open*, or run
  `xattr -dr com.apple.quarantine /Applications/RiscDom.app` once. **QEMU is not bundled**:
  `brew install qemu`. **Nobody has launched these packages on a real Mac yet.**
- **Linux** — from the same job: `RiscDom_0.7.0_amd64.deb`, `RiscDom-0.7.0-1.x86_64.rpm` or
  `RiscDom_0.7.0_amd64.AppImage`. **QEMU is not bundled**: install your distribution's
  `qemu-system-riscv64` (for example `sudo apt install qemu-system-misc`, `sudo dnf install
  qemu-system-riscv`). **Nobody has launched these packages on a real Linux machine yet.**

## What was verified — and what was not

**Verified**

- The gate, 13 steps: `cargo fmt`, `cargo clippy -D warnings`, `cargo check`, the full `cargo test`
  (**295 passed / 0 failed / 8 ignored**, 80 suites), `npm run build`, the eight UI probes, the mirror
  guard, the wix-version guard, the UI string registry check and the bilingual-documentation check.
  Green locally and in CI.
- The **macOS and Linux build path**, end to end: the `bundle` job produced and uploaded
  `.app`/`.dmg` and `.deb`/`.rpm`/`.AppImage` in run `35572294916` — which is also what proved `host`
  now compiles off Windows.
- The **i18n facility**: the registry's completeness check (every key in both languages) is in the
  gate with its own self-test, and the language switch's rules — the three-state choice, the `lang`
  attribute, live re-render, persistence — are pinned by the language probe.
- The **step-8 interface has been walked by eye** (the operator's walk; conclusion: passed). The
  v0.6 comparison's data layer and API keep their six unit + four integration tests.
- The end-to-end walk of **steps 3–7** against a real QEMU guest, unchanged since v0.5.0
  (`cargo test -p host --test golden_path -- --ignored`).

**Not verified**

- **The macOS and Linux packages themselves.** They compile and bundle; nobody has installed, launched
  or walked them on a real Mac or Linux machine. That is the next walk.
- **A clean-machine walk.** Still the v0.5.x strengthening item it was: [docs/golden-path-checklist.md](docs/golden-path-checklist.md)
  is the form, `walkthroughs/` is where a filled one goes.
- **The interface is mostly Chinese still.** The registry and the switch exist; the ~190 remaining
  strings were deliberately left untranslated in this release.
- **Unix-socket QMP is still not implemented** (the comparison and everything else go over TCP).
- **The installers and packages are unsigned** — SmartScreen on Windows, Gatekeeper on macOS.
  Signing and notarization are commercialisation-layer items.
- Chinese text entry through the UI is still verified only indirectly.

## What a tester needs

- A **model provider** and **an API key** of your own (or a local provider such as Ollama / LM Studio),
  **QEMU** (`qemu-system-riscv64`, installed by you — see the platform notes above), and **a RISC-V
  bare-metal GCC** (xPack `riscv-none-elf-gcc` or an equivalent `riscv64-unknown-elf-gcc`; the app can
  download the xPack one).
- For the comparison: **two finished runs whose fingerprints differ** — change one configuration field
  (the model, say) between them. A run that has not ended still has a fingerprint and can be compared;
  only the export refuses those.

## How to report back

1. Walk the steps with **[docs/golden-path-checklist.md](docs/golden-path-checklist.md)** open and fill
   it in as you go — and add what the diff did: whether the block opened, whether it listed every
   field, whether the rows that differ were the ones you changed, and what the header counted. On
   macOS or Linux, say which package you used and whether it opened at all.
2. Open an issue at <https://github.com/breakevery/riscdom/issues> and **paste the filled checklist**;
   put it in `walkthroughs/` if you prefer the repository.
3. A walk nobody wrote down is a walk nobody can check — the filled template is the report.

## Known limitations

- **Windows is the verified platform.** macOS and Linux build, but the golden path has not been walked
  there; their packages are unsigned and unlaunched.
- **Most of the interface is still Chinese**: the two-language registry exists and the switch works,
  but only a handful of strings are registered in both languages.
- **The diff block is one level deep**: it compares top-level fingerprint fields as whole values, not
  the keys inside them.
- **No incremental or encrypted snapshots**; session storage is plain local SQLite.
- **One VM at a time.**
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
