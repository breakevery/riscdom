//! host — 智芯城 RiscDom 的 Tauri 宿主后端。
//!
//! 对外（给 `ui/src-tauri`）暴露 Tauri commands；对内组合 `agent` / `sandbox`
//! / `audit`。前端**不直接**接触 Rust crate，一律经 Tauri command。
//!
//! 依赖方向：`host → {agent, sandbox, audit}`。
//!
//! 安全约束：
//! - API key 只存在于内存（`AppState::llm_config`），不落盘、不进审计、不进日志。
//! - 文件读写经 `agent::WorkspacePolicy` 检查。
//! - 前端无法绕过 host 直接调用 sandbox/agent。

pub mod commands;
pub mod error;
pub mod events;
pub mod keyring;
pub mod state;

pub use error::HostError;
pub use events::EventSink;
pub use keyring::{KeyringBackend, OsKeyring, SERVICE};
pub use state::{
    AgentOutcomeView, AppState, AuditStatusView, ChainStatusView, LlmConfigInput,
    LlmConfigStatus, LlmReadiness, LocalProbeResult, LocalProviderInfo, ProviderPresetView,
    StoredEventView,
};