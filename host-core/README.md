[中文](README.zh-CN.md) | English

# host-core

The **portable half** of the RiscDom host: everything the kernel facade does that does not
need a webview. No Tauri crate appears in its dependency tree — that is the point of the
crate, not a coincidence of today's imports.

Dependency direction: `host-core → {agent, sandbox, audit}`; `host → host-core`;
`ui/src-tauri → host`. Nothing here depends on the Tauri half, so the portable code can be
driven by the CLI, by `worker` and by the control plane without linking a GUI toolkit.

## Modules

- `state` — `AppState` (audit store, sink, VM slot, LLM config, workspace, compiler)
- `events` — the one event envelope, the event names, the `EventSink` trait
- `dispatch` — the host's own executor route into `agent::dispatch`
- `executor` — the stdio executor handle
- `preflight` — the environment preflight
- `qemu_download` / `toolchain_download` — the two download paths (the toolchain one serves
  every language: `Toolchain::C`, `Toolchain::Zig` and `Toolchain::Rust` — the last being the
  target's `rust-std` sysroot, v0.9 F3b-2)
- `run_diff` — run fingerprint comparison
- `session` / `settings` — sessions and local settings
- `keyring` — the API-key store
- `paths` — workspace and data-directory paths
- `error` — `HostError`

## Relationship to `host-tauri`

`host-tauri` is the Tauri half: the 53 `#[tauri::command]` functions and the `TauriEventSink`
transport. It re-exports this crate's public surface (`pub use host_core::*`), so the desktop
shell depends on one crate, and a consumer written against the pre-split `host` compiles
unchanged. The dependency runs one way only — `host-tauri → host-core` — and `worker` and
`server` depend on this crate directly, which is why neither of them links Tauri.

## Constraints

- **No Tauri.** A change that needs a webview belongs in `host-tauri`, not here.
- The API key exists in memory only: never written to disk, never logged, never audited.
- File reads and writes go through `agent::WorkspacePolicy`.
- Nothing outside this crate reaches `sandbox` / `agent` directly.
