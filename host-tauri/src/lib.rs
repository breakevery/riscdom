//! host-tauri — the Tauri half of the RiscDom host backend.
//!
//! The portable half lives in `host-core`: the audit store wiring, the VM slot,
//! sessions, the download paths, the preflight and the event envelope. This
//! crate adds what needs a webview — the Tauri commands, the `TauriEventSink`
//! transport — and re-exports the rest, so `host_tauri::state::…`,
//! `host_tauri::events::…` and `host_tauri::AppState` resolve for a consumer
//! written against the pre-split `host`.
//!
//! Dependency direction: `host-tauri → host-core → {agent, sandbox, audit}`;
//! `ui/src-tauri → host-tauri`. The frontend never touches a Rust crate directly:
//! it goes through Tauri commands.
//!
//! Security constraints (unchanged):
//! - API key 只存在于内存（`AppState::llm_config`），不落盘、不进审计、不进日志。
//! - 文件读写经 `agent::WorkspacePolicy` 检查。
//! - 前端无法绕过 host 直接调用 sandbox/agent。

pub mod commands;
pub mod events;

pub use host_core::*;
