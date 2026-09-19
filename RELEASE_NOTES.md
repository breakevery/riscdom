[中文](RELEASE_NOTES.zh-CN.md) | English

# RiscDom v0.4.0

**v0.4.0 is a feature release on top of v0.3.1:** theme switching, a native file picker, an
environment preflight that boots a tiny guest to prove your toolchain and QEMU really work together,
an end-to-end failure report that names the step that broke, and a guided QEMU setup. Underneath,
every run is now a first-class record in the audit log, relay ports come from one process-wide
lease, CI runs the same gate you run locally, and RiscDom cleans up its own temporary directories.

**An AI-native RISC-V sandbox for the desktop:** inside a QEMU RISC-V bare-metal sandbox the AI
holds virtual kernel-level privilege — it writes C / RISC-V assembly, compiles, runs, reads the
serial console and iterates. Everything is auditable and rollback-able, and humans keep root
privilege.

*(Developer-facing changes live in [CHANGELOG.md](CHANGELOG.md); this document is for users.)*

## Highlights

- **Light / dark / follow the system**: the appearance page switches the theme, every colour in the
  stylesheet is a token now, and the serial terminal follows the same tokens.
- **A native file picker**: pointing RiscDom at your own compiler or QEMU is a real file dialog
  again, not a text field.
- **The preflight proves your setup boots**: after the toolchain or QEMU path changes, RiscDom
  compiles a minimal guest and boots it on the real paths, then reports which of the four steps
  failed and what to do — warn-only, cached per configuration, with a recorded "continue anyway".
- **A failed run explains itself**: an end-to-end failure now prints a report naming the first
  failing step, that step's own output, the serial state (including "the guest never printed
  anything") and the audit-chain verdict — [docs/e2e-debugging.md](docs/e2e-debugging.md).
- **QEMU setup is guided**: RiscDom does not bundle QEMU and does not download it either. It tells
  you exactly what to run — `winget install SoftwareFreedomConservancy.QEMU` where `winget` exists,
  the official download page where it does not — and then finds, remembers and verifies the install.
  Why not the other two options: [docs/qemu-distribution.md](docs/qemu-distribution.md) §5.
- **Every run is a first-class record**: a run gets an id, a configuration fingerprint and an audit
  interval on the chain, a snapshot restore is recorded as its own run, and runs left open by a
  crash are marked abandoned on the next start — [docs/run-provenance.md](docs/run-provenance.md).
- **Relay ports come from one lease**: the port a snapshot restore or a VM start hands to QEMU is
  reserved by a process-wide lease instead of being picked and released, so two parts of the app can
  no longer be handed the same port — [docs/qemu-stdio.md](docs/qemu-stdio.md).
- **CI runs the same gate you do**: `.github/workflows/ci.yml` installs Rust + Node and calls
  `sh scripts/gate.sh` — one list of what "green" means. Where CI deliberately differs (no QEMU on
  the runner, no Tauri system libraries on Linux) it says so instead of skipping quietly.
- **RiscDom cleans up after itself**: stale `riscdom-*` directories are swept at startup, build
  scratch directories are removed when a build finishes, and `scripts/clean-temp.ps1` /
  `scripts/clean-temp.sh` clean up on demand (dry run by default).

## Verification

- `cargo test`: **261 passed / 0 failed / 7 ignored** across **75 test suites**. The 7 ignored tests
  are the ones that need a real API key or a real QEMU boot by design.
- The gate — `cargo fmt --check`, `cargo clippy -D warnings` (portable crates, `host`,
  `ui/src-tauri`), `cargo check`, the full `cargo test`, `npm run build`, six UI probes, the mirror
  guard and the bilingual-documentation check — passes locally and in CI.
- CI (GitHub Actions, `ubuntu-latest`) is green on every commit of this release, running
  `scripts/gate.sh` itself.

## Prerequisites

**A RISC-V bare-metal GCC is required to compile anything.** RiscDom auto-detects it
(`RISCDOM_RISCV_GCC` / `RISCV_GCC`, well-known install paths, then `PATH`) and, when nothing is
found, tells you exactly what was searched and how to fix it — including setting the path by
hand in **Settings → Toolchain**. Install the xPack
[riscv-none-elf-gcc](https://github.com/xpack-dev-tools/riscv-none-elf-gcc-xpack/releases) or an
equivalent `riscv64-unknown-elf-gcc`; per-platform steps are in
[docs/toolchain-setup.md](docs/toolchain-setup.md).

**QEMU (`qemu-system-riscv64`) is required to boot the guest, and RiscDom does not bundle or
download it.** Install it with

```text
winget install SoftwareFreedomConservancy.QEMU
```

or download an installer from <https://www.qemu.org/download/#windows>, or point RiscDom at an
existing copy with `RISCDOM_QEMU`. The app also discovers well-known install locations and
`PATH`, and *Settings → Toolchain* takes a manual path — now through a native file dialog. Details in
[docs/qemu-setup.md](docs/qemu-setup.md).

## Known limitations

- **Windows is the primary platform**: macOS and Linux are not verified yet, and QMP and the
  serial console go over TCP (no Unix sockets).
- **QEMU is not bundled and not downloaded for you**: you install it yourself, and RiscDom guides
  you through it.
- **No incremental or encrypted snapshots**: each snapshot is a full state stream; session
  storage is plain local SQLite.
- **One VM at a time**: the GUI drives a single host-owned VM — no multi-VM parallelism.
- **The interface is Chinese-only**: there is no language switch yet (a v0.5 candidate).

## Quick start

1. **Install the prerequisites** — Windows 10/11, QEMU (`qemu-system-riscv64`), a RISC-V
   bare-metal GCC (`riscv64-unknown-elf-gcc`), Node 20+ and the Rust toolchain (see
   [ENVIRONMENT.md](ENVIRONMENT.md)).
2. **Run the app** — `cd ui && npm install && npm run tauri dev`.
3. **Configure a model** — in the settings pane pick a provider and paste your API key (or
   choose Ollama / LM Studio for a fully local, key-less setup), then ask for something like
   *"write a RISC-V bare-metal Hello World, compile it, run it and read the serial output
   back"*.

## Security

This project ships **no** API key: all model access is bring-your-own-key. Keys never leave
your machine, the audit log lives outside the AI and is append-only, and nothing is uploaded
anywhere. QEMU and the RISC-V toolchain are separate programs under their own licences; see
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md). The full security statement and the reporting
process are in [SECURITY.md](SECURITY.md).

## License

[Apache License 2.0](LICENSE).
