[中文](README.zh-CN.md) | English

# RiscDom

> Let the AI take charge inside a RISC-V virtual sandbox — while humans keep root privilege.

**License:** [Apache-2.0](LICENSE).

RiscDom is a desktop application: inside a QEMU RISC-V bare-metal sandbox the AI holds
**virtual kernel-level privilege**. It can write C / RISC-V assembly, compile, run, read the
serial console and iterate. Everything is **auditable and rollback-able**; humans keep root
privilege; edge capabilities are plugins.

## Motto

**Freedom inside boundaries, audit outside the AI, root privilege with humans.**

## Constitution (summary)

1. The host monitoring layer must not be modifiable by the AI.
2. The audit log lives outside the AI: append-only, cannot be disabled.
3. Capabilities are denied by default; plugins declare their permissions.
4. Humans always keep the right to pause, roll back, disconnect and terminate.
5. AI democracy is an experiment variable, not an MVP requirement.
6. During MVP the AI inside the sandbox may only generate C and RISC-V assembly.
7. AI access starts with API keys.

Full text: [PROJECT_CONSTITUTION.md](PROJECT_CONSTITUTION.md).

## Architecture

```text
                ┌────────────────────────────────────┐
                │  Human (root privilege holder)      │
                └──────────────────┬─────────────────┘
                                   │ Tauri commands / events
                ┌──────────────────▼─────────────────┐
                │  host (Rust + Tauri) sole frontend  │
                │  entry point                        │
                └───┬──────────────┬──────────────┬──┘
                    │              │              │
        ┌───────────▼──┐  ┌────────▼───────┐  ┌───▼──────────┐
        │ agent        │  │ sandbox        │  │ audit        │
        │ agent loop   │  │ QEMU RISC-V    │  │ append-only  │
        │ tools/policy │  │ serial / QMP   │  │ hash chain   │
        └──────────────┘  └────────────────┘  └──────────────┘
                    ▲              ▲
                    └──── ui (React) ────┘   the frontend never touches Rust crates
```

Dependency direction (one-way, acyclic): `ui/src-tauri → host → {agent, sandbox, audit}`,
and `agent → sandbox → audit`.

## Requirements

- Windows 10/11 (MVP is verified on Windows only; QMP/serial use TCP)
- QEMU (`qemu-system-riscv64`, verified with 11.1.0) — RiscDom **finds it for you**
  (`RISCDOM_QEMU` / `QEMU_SYSTEM_RISCV64`, well-known install locations, then `PATH`), and you can
  point it at a copy in **Settings → Toolchain → QEMU**. On Windows:
  `winget install SoftwareFreedomConservancy.QEMU`, or <https://www.qemu.org/download/#windows>.
  Details: [docs/qemu-setup.md](docs/qemu-setup.md).
- **A RISC-V bare-metal GCC is required to compile anything** — and RiscDom can fetch it for
  you: **Settings → Toolchain → one-click download** pulls the official xPack build for your
  platform (~200 MB), checks its SHA-256 and installs it under the app data directory. You can
  also install one yourself (xPack
  [riscv-none-elf-gcc](https://github.com/xpack-dev-tools/riscv-none-elf-gcc-xpack/releases),
  verified with 15.2.0, or an equivalent `riscv64-unknown-elf-gcc`): RiscDom auto-detects it
  (`RISCDOM_RISCV_GCC` / `RISCV_GCC`, well-known install paths, then `PATH`) and you can point it
  at a path in the same tab. Per-platform steps: [docs/toolchain-setup.md](docs/toolchain-setup.md).
- Rust / cargo (verified with 1.98.1) + MSVC toolchain (required by Tauri)
- Node / npm (verified with 24.11.1 / 11.16.0)

Details and paths: [ENVIRONMENT.md](ENVIRONMENT.md).

## Security statement

- This project **does not provide, host or embed** any API key. All model access is
  bring-your-own-key (BYOK).
- API keys stay on your machine (written to the OS keyring by default) and **never pass
  through any server of this project**.
- This project **never** uploads your code, serial output or audit log anywhere.
- The audit log (SQLite + hash chain) is local only; it exists for auditing and rollback.
- For fully offline operation use a local model such as Ollama / LM Studio — **no key needed**.
- Report security issues privately via GitHub Security Advisory; never paste keys or exploit
  details into an issue.

> Run the local preflight before committing: `scripts/preflight.ps1` (Windows) or
> `scripts/preflight.sh` (Unix).

## Quick start

```powershell
# 1. frontend dependencies
cd ui
npm install

# 2. launch the desktop app (compiles the Rust backend)
npm run tauri dev
```

The main view is a two-pane chat + serial layout, both panes always visible; settings open from
the gear button in the top-right corner (`Esc` returns).

Enter your DeepSeek API key in the settings panel (session memory only), then ask for example:

> Write a RISC-V bare-metal Hello World, compile it, run it and read the serial output back

## Layout

```text
riscdom/
├── AGENTS.md                 # core constitution (injected every turn)
├── PROJECT_CONSTITUTION.md   # full constitution + architecture + audit event types
├── ENVIRONMENT.md            # local toolchain and platform limits
├── CHANGELOG.md              # version history
├── LICENSE                   # Apache-2.0
├── Cargo.toml                # Rust workspace (host/sandbox/audit/agent)
├── sandbox/                  # QEMU RISC-V sandbox (process/QMP/serial/snapshot)
├── audit/                    # append-only SQLite + hash chain (includes audit-verify)
├── agent/                    # agent loop, tools, capability policy, compiler wrapper
├── host/                     # Tauri backend: commands / events / state
└── ui/                       # React frontend (Tauri shell + three-pane UI)
    ├── src/                  # layout / panels / API / state
    └── src-tauri/            # Tauri shell (registers host commands)
```

## Tests

```powershell
# whole workspace (Rust)
cargo test

# a single crate
cargo test -p sandbox     # QEMU lifecycle + serial capture + snapshot fallback
cargo test -p audit       # append-only + hash chain + queries + CLI
cargo test -p agent       # LLM client + tools + policy + compiler + agent loop
cargo test -p host        # Tauri backend commands + serial deltas

# end-to-end that needs real QEMU/toolchain (mock LLM)
cargo test -p host -- --ignored --nocapture

# frontend build
cd ui && npm run build
```

## Setting the API key

```powershell
$env:DEEPSEEK_API_KEY = "sk-..."   # current terminal session only
cargo test -p agent -- --ignored --nocapture   # real-model end-to-end
```

Or fill it in the app's **settings panel** and "save for this session".

The key lives in backend memory only: **not** written to localStorage / sessionStorage /
disk / audit / logs, and status read-outs never contain it. Closing the app invalidates it.

## Known limitations (MVP fallbacks)

> Full roadmap: [PROJECT_CONSTITUTION.md §10](PROJECT_CONSTITUTION.md).

- **DeepSeek only** (v0.1): the LLM client talks to DeepSeek only. v0.2 refactors it into a
  generic `OpenAiCompatClient` with built-in OpenAI / Ollama (local) / LM Studio (local)
  presets, including key-less local models.
- **API key in memory only** (v0.1): never on disk, never in the audit log; gone when the app
  closes. v0.2 moves to the OS keyring (Windows Credential Manager / macOS Keychain /
  Linux Secret Service) and never uses `localStorage` / plain files / `.env`.
- **Snapshots**: the reboot fallback (`save_snapshot` / `load_snapshot`) is kept for
  compatibility; real snapshots use TCP migration + a local file relay
  ([`sandbox/docs/snapshot-experiment.md`](sandbox/docs/snapshot-experiment.md)).
- **Serial source**: pushed by the sandbox's serial reader thread
  (`subscribe_serial` → `serial:chunk`), no longer derived from the audit log; subscribers
  only receive what arrives after they subscribe (see `host/README.md`).
- **Platform**: Windows + TCP only; Unix sockets / macOS / Linux are not implemented.
- **No streaming**: (superseded — streaming LLM output is implemented; see CHANGELOG).
- **Sessions**: conversations are persisted (list / open / rename / delete); a restored
  session replays history messages only, never tool calls.
- **Compiler injects crt0**: the AI only writes `int main(void)` (see `agent/README.md`).

## Contributing

Bug reports, fixes and documentation improvements are welcome. Please read
[CONTRIBUTING.md](CONTRIBUTING.md) first: every change must pass the local gate
(`scripts/gate.ps1` / `scripts/gate.sh`) and go through the gated commit wrapper
(`scripts/commit.ps1` / `scripts/commit.sh`), which refuses to commit while the gate is red.

## Code of conduct

This project follows the [Contributor Covenant v2.1](CODE_OF_CONDUCT.md). Report unacceptable
behaviour through the contact listed there.

## License

[Apache License 2.0](LICENSE).

- The code in this repository is licensed under Apache-2.0.
- When using this project, follow the terms of your model provider.

## More

- [PROJECT_CONSTITUTION.md](PROJECT_CONSTITUTION.md) — full constitution, architecture
  layers, audit event types
- [ENVIRONMENT.md](ENVIRONMENT.md) — toolchain and platform limits
- [CHANGELOG.md](CHANGELOG.md) — version history
- Per-crate READMEs: [sandbox](sandbox/README.md) · [audit](audit/README.md) ·
  [agent](agent/README.md) · [host](host/README.md) · [ui](ui/README.md)
