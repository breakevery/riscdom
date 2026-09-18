[中文](CHANGELOG.zh-CN.md) | English

# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **`scripts/clean-temp.ps1` / `scripts/clean-temp.sh`** remove RiscDom's directories from the
  system temp directory. Dry run by default; `-Force` / `--force` deletes. Only entries whose
  name starts with `riscdom-` are matched, and `<temp>/riscdom` (the fallback data directory)
  is excluded explicitly.
- **End-to-end failure-path diagnostics** (stage 5c-3): an end-to-end run now prints a report
  naming the first failing step, that step's own output, the serial state (including “the
  guest never printed anything”) and the audit-chain verdict. How to read it:
  [docs/e2e-debugging.md](docs/e2e-debugging.md).
- **Environment capability preflight**: after the toolchain or QEMU path changes (and
  on the first run with a stale cache) the host compiles a minimal guest and boots
  it on the real paths, reporting which of the four steps failed. It is warn-only,
  cached in `settings.json` against the configuration fingerprint, never writes to
  the audit chain, and offers a recorded "continue anyway". Version-number rules
  were deliberately not implemented: no QEMU × GCC compatibility matrix exists in
  this repository (see `PROJECT_CONSTITUTION.md` §10).

### Fixed

- **Stale build scratch directories are swept at startup**: a build cleans up after itself, and
  the ones a killed process could not clean (older than 24 h) are removed when the host starts.
  Only the `riscdom-build-*` prefix is touched.
- **Build scratch directories are removed when the build finishes**: the per-build path fixed
  the concurrency race but leaked one small directory per compile; success and failure both
  clean up now.
- **Concurrent builds no longer share files**: the injected `crt0.S` / linker script
  used to live at one fixed temporary path, so two builds running at once (a run and
  the preflight, or parallel tests) could compile against a half-written file. Every
  build now gets its own directory.
- **The preflight's compile step has a guard**: a compiler that has not answered within
  30 s is stopped and reported as a timeout, instead of hanging the panel.

## [0.3.1] - 2026-09-17

### Fixed

- **A snapshot restore now honours a manually configured QEMU path**: the restore built its own
  `VMConfig` with `qemu_exe: None`, so it silently fell back to auto-discovery and could boot with a
  different binary than the one configured in *Settings → Toolchain*. The agent loop and the restore
  now inject the configured path through one helper. (Reproduction: the old function did not fail —
  it returned `Ok(())` while ignoring the path. The regression test asserts that the restore must go
  through the configured binary, so that assertion panics on the `Ok`.)
- **A QEMU process that has exited is no longer reported as running**: `vm_is_running` looked at the
  slot only, so a handle left behind by a guest shutdown (or a killed or crashed QEMU) kept the
  top-bar badge on "VM running" forever. The child process is checked as well and a dead handle is
  dropped.
- **`read_serial` returns the captured output when the guest never goes quiet**: the overall wait
  used to answer "No serial output yet" even though the buffer was full (a guest printing in a loop
  never reaches the 150 ms quiet window). Only a genuinely empty buffer reports silence now.
- **A finished run no longer pulls the chat back to the bottom**: the completion handler forced
  scroll-to-bottom even when the reader had scrolled up. It now respects the scroll state and shows
  the "jump to latest" button, matching the serial panel.
- **The two-pane layout stays inside the window**: the chat column was clamped only against a fixed
  240–900 px range, so a wide chat column pushed the serial column off-screen in a narrow window. The
  drag bound is derived from the measured container and the serial column has an adaptive minimum.

> Note: the v0.3.1 entry in RELEASE_NOTES.md / RELEASE_NOTES.zh-CN.md was added in the commit right
> after the tag. The tag itself (`b9be911`) already carries this changelog entry and the complete
> code.

## [0.3.0] - 2026-09-16

### Added

- **One-click download of RISC-V GCC (xPack)**: SHA-256 verified, Zip-Slip protected,
  cancellable.
- **QEMU discovery and a manual path**: `RISCDOM_QEMU` → known paths → `PATH`, plus a manual
  path in *Settings*, persisted to `settings.json` and injected into the agent loop.
- **VM status badge** in the top bar (visible across runs).
- **Settings page with tabs**: Model / Toolchain / Snapshot / Audit / Plugins.

### Changed

- **Two-pane main view** (chat + serial); settings moved to a separate page (Esc returns).
- **The system prompt is now entirely in English.**
- **The AI no longer stops the VM automatically after a task**: the prompt, the `stop_vm`
  tool description and the VM badge guarantee it three times over.

### Fixed

- **Chat and serial auto-scroll** to the latest output; a user scrolling up is not
  interrupted and a "jump to latest" button appears.
- **`read_serial` waits for ~150 ms of silence** before returning, so the first byte is no
  longer truncated.
- **Snapshot resume retries the relay port** on QMP 10054 / bind failure (up to 3 attempts).
- **Test gate stability**: port TOCTOU retry in `start_vm`.

### Notes

- Residual items are tracked in `PROJECT_CONSTITUTION.md` §10 (v0.4).

## [0.2.2] - 2026-09-15

### Fixed

- **The Windows keyring was a silent no-op**: the `keyring` crate ships **no** default backend,
  so a bare `keyring = "3"` compiled to an empty implementation — `set` reported success while
  nothing reached Credential Manager, and every restart lost the key. `host` now opts into
  `windows-native` (and `apple-native` / `linux-native-sync-persistent` on the other platforms),
  so API keys really persist and are read back at startup.

## [0.2.1] - 2026-09-15

### Added

- **Manual toolchain path is persisted**: *Settings → Toolchain* writes the chosen compiler to
  `settings.json` in the app data directory (never into the repo, never a key), so it survives a
  restart. A failed write is audited as `host.settings.save_failed` and never blocks the run.

### Fixed

- **The RISC-V toolchain is discovered, explained and configurable** (stages 24a–24c):
  resolution order is `RISCDOM_RISCV_GCC` → `RISCV_GCC` → well-known install locations → `PATH`,
  accepting both `riscv64-unknown-elf-gcc` and the xPack name `riscv-none-elf-gcc`. When nothing
  is found, the error lists every path that was searched, links the installer and explains how
  to point the app at a compiler; `run_agent` refuses early with a structured `toolchain_missing`
  error and the UI shows a red banner with “probe again” / “set path manually”
  (see `docs/toolchain-setup.md`).
- **No more duplicated error prefix**: a manual toolchain that cannot run is reported once
  (`not runnable: …`) instead of twice.

## [0.2.0] - 2026-09-14

### Changed

- **VM lifecycle moved to host**: the VM is decoupled from `AgentLoop` into
  `AppState::vm_slot`, so it survives the run and the next run reuses the same guest
  (`AgentLoop::with_vm` injection; behaviour is unchanged when nothing is injected). The
  serial forwarder is now **long-lived** (created at app startup) and subscriptions continue
  **across runs**.
- **Serial source is now a sandbox push**: the sandbox serial reader thread fans out frames
  through `VMConfig.serial_observer` in real time → `agent::AgentLoop::subscribe_serial()`
  (`std::sync::mpsc`) → host forwards them as `serial:chunk` and accumulates them for
  `get_serial_buffer()`. No longer derived from the audit log's `read_serial` tool results
  (the old `serial_full_text` / `SerialDiff` are gone). The `read_serial` tool semantics are
  unchanged; observer panics are caught with `catch_unwind` and audited as
  `sandbox.serial.observer_panic`.

### Added

- **Real snapshot save / restore**: host `save_snapshot_real` / `resume_from_snapshot_real`
  (audit `host.snapshot.save` / `host.snapshot.resume`), plus a "save current state" button
  and a per-entry "restore" button in the UI (with confirmation).
- **`real_api` asserts audit-chain integrity** (stage 21): the real-API test's audit backend
  moved from in-memory to **file SQLite**, and after the run an independent handle uses
  `audit::verify_chain` to require `ChainStatus::Intact { length > 0 }` plus at least one
  `agent.llm.request` / `agent.tool.call` / `agent.tool.result` event; the temp DB is cleaned
  up by a `Drop` guard (including on failure).
- **Snapshot panel** (list / delete; real snapshots labelled "real", reboot fallbacks
  labelled "reboot"), host commands `list_snapshots` / `delete_snapshot`
  (audit `host.snapshot.delete`).
- **Session persistence** (list / open / rename / delete / clear): host `SessionStore`
  (SQLite, reusing `rusqlite`) + 7 Tauri commands; sessions are saved automatically under the
  app data directory and survive restarts; restoring injects history messages only (tool
  calls are not replayed) and **never persists** API keys / the system prompt / audit events.
- **Streaming LLM responses (agent + host + UI)**: `LlmClient::chat_stream` (degrades to
  `chat` by default) + the SSE implementation in `OpenAiCompatClient` + the `sse` parser;
  `AgentLoop::subscribe_stream`; host `agent:stream:delta` / `agent:stream:done`; the UI
  appends token by token (the final content supersedes it). Audit records only
  `agent.llm.stream.start` / `.end`, not every chunk.
- CI workflows (`.github/workflows/ci.yml`): secret scanning (gitleaks, full history), Rust
  checks (`fmt --check` / `clippy -D warnings` / `check` / `audit` unit tests, portable
  crates only) and the frontend build (`npm ci` + `npm run build`).
- Local preflight scripts: `scripts/preflight.ps1` (Windows) and `scripts/preflight.sh` (Unix).
- `SECURITY.md`, `.env.example`, and a fuller `.gitignore` (`.env*` / `*.db` / `*.jsonl`, …).

### Security

- Dependency audit (2026-09-14): `cargo audit` scanned 470 crates — **0 vulnerabilities**;
  7 informational warnings (6 unmaintained: `proc-macro-error`, `unic-char-property` /
  `unic-char-range` / `unic-common` / `unic-ucd-ident` / `unic-ucd-version`; 1 unsound:
  `glib 0.18.5`, still a Linux/GTK transitive dependency, not built on Windows).
  `npm audit --omit=dev`: **0 vulnerabilities**.
- The README gained a "security statement"; the v0.2 roadmap gained item **f**
  (pre-launch security checklist).
- No dependency was upgraded by us (warnings left untouched pending a human decision).

### Notes

- **Real snapshots are implemented with plan A′ (TCP migration + a local file relay).**
  Stage 18a showed `migrate` → `file:` is unusable on Windows + QEMU 11.1.0, while
  `migrate` → `tcp:` works; 19b uses a local TCP relay to persist the migration stream as
  `<name>.mig` and, on restore, feeds it back to a QEMU started with `-incoming tcp:`.
  See `sandbox/docs/snapshot-experiment.md`.
  Residual limits: the old reboot fallback (`.json`) is still supported; restore takes
  `-kernel` from the newest `*.elf` in the workspace (the migration stream overwrites memory;
  the kernel only lets QEMU boot).

### Planned (v0.2) — multi-model access and key security

- **LLM client refactor**: `DeepSeekClient` → `OpenAiCompatClient` (`base_url` / `api_key` /
  `model` fully user-configurable; keep the OpenAI-compatible protocol and demote DeepSeek to
  one default preset)
- **Built-in provider presets**: DeepSeek (default) / OpenAI / Ollama (local, no key) /
  LM Studio (local) / custom; the UI provider dropdown fills `base_url` / `model`
- **Local offline model support**: Ollama / LM Studio reuse the same client; offline mode =
  QEMU + RISC-V GCC + audit + sandbox + local LLM, with no network at all
- **Key-less degradation**: no key never crashes; the UI guides configuration; probe
  `localhost:11434` and offer local Ollama; a new user's first launch must not just error
- **API key persistence: OS keyring** (Windows Credential Manager / macOS Keychain /
  Linux Secret Service, via the Rust `keyring` crate); never `localStorage` / plain files /
  `.env`; "memory only" becomes the fallback
- **Pre-launch security checklist**: `.env.example` holds placeholders only; `.gitignore`
  covers `.env` / `*.db` / `*.jsonl`; CI runs secret scanning (gitleaks or GitHub native);
  the README states that no API key is provided

### Planned (v0.2) — other

- Expose a minimal serial access interface on `AgentLoop` (the host currently derives it from
  the audit log, which is brittle)
- Real QEMU snapshots with `savevm` / `loadvm` (already satisfied by plan A′; remaining work:
  the `AppState.vm` slot so the UI can save/restore)
- Host serial polling moved to sandbox push callbacks
- gdbstub integration (debugging)
- Unix sockets (macOS / Linux) and virtio devices
- Audit log sharding and remote backup
- **Bilingual (English/Chinese) docs before going public**: README / CHANGELOG /
  PROJECT_CONSTITUTION / AGENTS / release notes in both languages; English is the main
  document (GitHub default), Chinese lives in `*.zh-CN.md`; language switcher at the top;
  LICENSE is not translated

## [0.1.0] - 2026-09-14

> RiscDom v0.1.0 — AI-native RISC-V sandbox MVP

### Added

- **sandbox**: a QEMU RISC-V `virt` bare-metal sandbox. Process lifecycle, platform endpoint
  abstraction (QMP / serial → QEMU arguments), a minimal QMP client (greeting /
  `qmp_capabilities` / `stop` / `cont` / `quit`), serial capture with incremental buffering,
  snapshot/rollback (MVP fallback), and auditing of every outbound operation.
- **audit**: append-only SQLite + SHA-256 hash chain. `BEFORE UPDATE` / `BEFORE DELETE`
  triggers make rewrites impossible; no UPDATE/DELETE API and no off switch; querying /
  filtering / JSONL export; an `audit-verify` CLI (exit 0/1/2, locating the first broken
  event).
- **agent**: the agent loop and tools. DeepSeek client + `MockLlm`; the capability policy
  `WorkspacePolicy` (deny by default, traversal guard, extension allowlist); the tool set
  `write_source` / `compile` / `start_vm` / `read_serial` / `stop_vm` / `list_workspace`;
  a freestanding RISC-V compiler wrapper (injects crt0 + linker script); the system prompt;
  context trimming and an iteration cap; audit events across the whole chain.
- **host**: the Tauri backend. 10 commands (audit status/list, LLM config, run agent,
  workspace, serial, export); events `agent:iteration` / `agent:tool_call` /
  `agent:tool_result` / `agent:final` / `serial:chunk` / `vm:state`.
- **ui**: a React + TypeScript + Vite three-pane desktop UI (chat / settings / serial
  canvas), an xterm.js serial canvas and draggable splitters with no third-party splitter
  library.
- Project docs: `AGENTS.md` (the constitution), `PROJECT_CONSTITUTION.md` (full constitution
  + architecture + audit event types), `ENVIRONMENT.md` (toolchain and platform limits),
  per-crate READMEs and the root README.

### Known limitations (MVP fallbacks)

- **Snapshots are a fallback**: `save_snapshot` / `load_snapshot` store and reload launch
  parameters and reboot — they are **not** real VM memory/device state (v0.2 moves to
  `savevm`/`loadvm`).
- **Windows + TCP only**: QMP/serial go over TCP; Unix sockets and macOS/Linux are not
  implemented.
- **No streaming**: LLM responses arrive as one block.
- **No session persistence**: every `run_agent` is an isolated context.
- **The compiler injects crt0**: the AI only writes `int main(void)`; the `_start` entry and
  the stack are injected by the compiler (rationale in `agent/README.md` and
  `ENVIRONMENT.md`).
- **API key in memory only**: never on disk, never in the audit log; gone when the app closes.

### Build artifacts (Windows x64)

Produced by `npm run tauri build` (build output under `target/`, not committed):

- `ui/src-tauri/target/release/bundle/msi/RiscDom_0.1.0_x64_en-US.msi` (about 5.16 MB)
- `ui/src-tauri/target/release/bundle/nsis/RiscDom_0.1.0_x64-setup.exe` (about 3.65 MB)

### GitHub Release

The repository stays **private**. No GitHub Release has been published; installers are kept
locally only. (An earlier draft was deleted; the `v0.1.0` tag remains.)

### Verification

- The whole workspace passes `cargo test` (sandbox/audit/agent/host + doc tests).
- `npm run build` (tsc + vite build) passes.
- `cargo check --manifest-path ui/src-tauri/Cargo.toml` passes.
- Mock-LLM end-to-end: `cargo test -p host -- --ignored --nocapture` → `agent:final` arrives,
  `serial:chunk` contains `HELLO RISCV`, `verify_chain` is Intact.
- Real DeepSeek API end-to-end: **executed and passing** (2026-09-14, `iterations = 6`,
  serial captured `HELLO RISCV`; see `host/README.md`).

[Unreleased]: https://github.com/breakevery/riscdom/compare/v0.3.1...HEAD
[0.3.1]: https://github.com/breakevery/riscdom/compare/v0.3.0...v0.3.1
[0.3.0]: https://github.com/breakevery/riscdom/compare/v0.2.2...v0.3.0
[0.2.2]: https://github.com/breakevery/riscdom/compare/v0.2.1...v0.2.2
[0.2.1]: https://github.com/breakevery/riscdom/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/breakevery/riscdom/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/breakevery/riscdom/releases/tag/v0.1.0
