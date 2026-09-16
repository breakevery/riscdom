[中文](RELEASE_NOTES.zh-CN.md) | English

# RiscDom v0.3.0

**v0.3.0 is the layout and environment release:** the main view is now a two-pane chat + serial
layout with settings on a page of their own, and the two prerequisites — QEMU and the RISC-V
GCC — are handled inside the app instead of from a terminal.

**An AI-native RISC-V sandbox for the desktop:** inside a QEMU RISC-V bare-metal sandbox the AI
holds virtual kernel-level privilege — it writes C / RISC-V assembly, compiles, runs, reads the
serial console and iterates. Everything is auditable and rollback-able, and humans keep root
privilege.

*(Developer-facing changes live in [CHANGELOG.md](CHANGELOG.md); this document is for users.)*

## Highlights

- **Two-pane layout**: the main view is now chat and serial side by side, and settings moved to
  a separate page (Esc returns to the chat).
- **One-click RISC-V GCC download**: RiscDom fetches the xPack bare-metal GCC for you, verifies
  it with SHA-256, guards against Zip-Slip on extraction, and can be cancelled mid-download.
- **QEMU discovery and a manual path**: RiscDom finds `qemu-system-riscv64` by itself
  (`RISCDOM_QEMU` → well-known install locations → `PATH`), and *Settings → Toolchain* lets you
  set the path by hand — remembered across restarts and injected into the agent loop, so it
  really takes effect.
- **VM status badge**: the top bar always shows whether the VM is running, and it stays visible
  across runs.
- **The AI no longer stops the VM on its own**: after a task the guest keeps running — the
  prompt, the `stop_vm` tool description and the VM badge all guarantee it.
- **`read_serial` returns the complete output**: it waits for ~150 ms of silence before
  answering, so the first byte is no longer truncated.
- **Auto-scroll that follows the output**: chat and serial stick to the latest line, and
  scrolling up is never interrupted — a "jump to latest" button appears instead.

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
