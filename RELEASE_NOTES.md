[中文](RELEASE_NOTES.zh-CN.md) | English

# RiscDom v0.3.1

**v0.3.1 is a bug-fix release on top of v0.3.0:** five defects found while accepting v0.3.0 are
fixed — a snapshot restore that ignored a manually configured QEMU, a VM badge that kept claiming
"running" after QEMU had exited, a serial read that reported silence while its buffer was full, a
chat that jumped back to the bottom while you were reading, and a narrow window that pushed the
serial column off-screen.

**An AI-native RISC-V sandbox for the desktop:** inside a QEMU RISC-V bare-metal sandbox the AI
holds virtual kernel-level privilege — it writes C / RISC-V assembly, compiles, runs, reads the
serial console and iterates. Everything is auditable and rollback-able, and humans keep root
privilege.

*(Developer-facing changes live in [CHANGELOG.md](CHANGELOG.md); this document is for users.)*

## Highlights

- **A snapshot restore uses your configured QEMU**: the restore built its own VM configuration and
  silently fell back to auto-discovery, so it could boot with a different binary than the one set in
  *Settings → Toolchain*. The agent run and the restore now inject the same path.
- **The VM badge tells the truth**: a QEMU process that has exited (the guest shut down, or QEMU was
  killed or crashed) left its handle behind, so the top bar kept showing "VM running" forever. The
  child process is checked too now, and a dead handle is dropped.
- **`read_serial` hands back what the guest printed**: a guest that keeps printing (a heartbeat line,
  a busy log) never reaches the quiet window, and the tool used to answer "No serial output yet"
  while the buffer was full. Only a genuinely empty buffer reports silence now.
- **Reading history is no longer interrupted**: a finished run forced the chat back to the bottom
  even if you had scrolled up. It now keeps your position and offers the "jump to latest" button,
  like the serial panel.
- **The layout fits any window**: the chat column was clamped against a fixed width, so in a narrow
  window it pushed the serial column off-screen. The drag bound now follows the window, and the
  serial column has an adaptive minimum.

## Verification

- `cargo test`: **181 passed / 0 failed / 7 ignored** across 58 test suites. That includes three new
  regression tests — a snapshot restore must use the configured QEMU path, a guest that powered
  itself off must not be reported as running, and a continuously printing guest must return its
  output. The 7 ignored tests are the ones that need a real API key or a real QEMU boot by design.
- `cargo fmt --check`, `cargo clippy -D warnings`, `cargo check` (portable crates and
  `ui/src-tauri`), `npm run build` (tsc + vite) and the bilingual documentation check all pass.
- The two UI probes that guard the scroll and layout fixes run inside the local gate
  (`scripts/gate.ps1` / `scripts/gate.sh`).

## Prerequisites

**A RISC-V bare-metal GCC is required to compile anything.** RiscDom auto-detects it
(`RISCDOM_RISCV_GCC` / `RISCV_GCC`, well-known install paths, then `PATH`) and, when nothing is
found, tells you exactly what was searched and how to fix it — including setting the path by
hand in **Settings → Toolchain**. Install the xPack
[riscv-none-elf-gcc](https://github.com/xpack-dev-tools/riscv-none-elf-gcc-xpack/releases) or an
equivalent `riscv64-unknown-elf-gcc`; per-platform steps are in
[docs/toolchain-setup.md](docs/toolchain-setup.md).

**QEMU (`qemu-system-riscv64`) is required to boot the guest, and it is not bundled or
downloaded for you.** Install it with

```text
winget install SoftwareFreedomConservancy.QEMU
```

or download an installer from <https://www.qemu.org/download/#windows>, or point RiscDom at an
existing copy with `RISCDOM_QEMU`. The app also discovers well-known install locations and
`PATH`, and *Settings → Toolchain* accepts a manual path. Details in
[docs/qemu-setup.md](docs/qemu-setup.md).

## Known limitations

- **Windows is the primary platform**: macOS and Linux are not verified yet, and QMP and the
  serial console go over TCP (no Unix sockets).
- **QEMU is not bundled and not downloaded for you**: unlike the RISC-V GCC, you install QEMU
  yourself.
- **No native file picker**: the Tauri dialog plugin is not wired up yet, so pointing RiscDom at
  your own compiler or QEMU uses a text field.
- **No incremental or encrypted snapshots**: each snapshot is a full state stream; session
  storage is plain local SQLite.
- **One VM at a time**: the GUI drives a single host-owned VM — no multi-VM parallelism.

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
anywhere. See [SECURITY.md](SECURITY.md) for the full statement and the reporting process.

## License

[Apache License 2.0](LICENSE).
