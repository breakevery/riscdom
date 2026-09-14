[中文](RELEASE_NOTES.zh-CN.md) | English

# RiscDom v0.2.0

**An AI-native RISC-V sandbox for the desktop:** inside a QEMU RISC-V bare-metal sandbox the AI
holds virtual kernel-level privilege — it writes C / RISC-V assembly, compiles, runs, reads the
serial console and iterates. Everything is auditable and rollback-able, and humans keep root
privilege.

*(Developer-facing changes live in [CHANGELOG.md](CHANGELOG.md); this document is for users.)*

## Highlights

- **Bring your own key, any model**: the LLM client is now a generic OpenAI-compatible client
  with built-in presets — DeepSeek (default) / OpenAI / Ollama / LM Studio / custom. Pick a
  provider in the settings pane and the base URL and model fill themselves in.
- **Offline, key-less operation**: point it at a local model (Ollama / LM Studio) and the whole
  loop — QEMU + RISC-V GCC + audit + sandbox + LLM — runs without a network and without an API
  key.
- **OS keyring persistence**: optionally store the key in Windows Credential Manager / macOS
  Keychain / Linux Secret Service. The key itself is never written to disk, to the audit log,
  or to the frontend, and the app degrades silently to memory-only when no keyring is
  available.
- **Streaming responses**: answers appear token by token instead of in one block.
- **Session persistence**: conversations are saved locally and survive restarts; reopening one
  restores the history (tool calls are never replayed).
- **Real snapshots**: the VM's actual state is saved to `.mig` through QMP migration plus a
  local TCP relay, and restored from the UI — with a confirmation step.
- **Serial across runs**: the serial console keeps accumulating between runs, so you can watch
  a guest that outlives a single turn; the VM is host-owned and reused.
- **Bilingual documentation**: every document now exists in English (main) and Chinese
  (`*.zh-CN.md`), kept in sync by an automated check in the local gate.

## Prerequisites

**A RISC-V bare-metal GCC is required to compile anything.** RiscDom auto-detects it
(`RISCDOM_RISCV_GCC` / `RISCV_GCC`, well-known install paths, then `PATH`) and, when nothing is
found, tells you exactly what was searched and how to fix it — including setting the path by
hand in **Settings → Toolchain**. Install the xPack
[riscv-none-elf-gcc](https://github.com/xpack-dev-tools/riscv-none-elf-gcc-xpack/releases) or an
equivalent `riscv64-unknown-elf-gcc`; per-platform steps are in
[docs/toolchain-setup.md](docs/toolchain-setup.md).

## Known limitations

- **Windows is the primary platform**: QMP and the serial console go over TCP; Unix sockets,
  macOS and Linux are not implemented yet.
- **Real snapshots use the TCP relay**: `migrate` → `file:` is unusable with QEMU 11.1.0 on
  Windows, so a local relay persists the stream. A snapshot name that already exists is
  refused rather than overwritten, and restoring takes its `-kernel` from the newest `*.elf` in
  the workspace.
- **No incremental or encrypted snapshots**: each snapshot is a full state stream; session
  storage is plain local SQLite.
- **One VM at a time**: the GUI drives a single host-owned VM.

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
