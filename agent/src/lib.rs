//! agent — 智芯城 RiscDom 的 AI 代理运行时。
//!
//! 职责：用自然语言驱动 LLM 在 RISC-V 虚拟沙箱里写 C / 汇编、编译、运行、
//! 读串口并循环迭代。
//!
//! 依赖方向：`agent → sandbox`，`agent → audit`（不允许反向依赖）。
//!
//! 安全约束（见 PROJECT_CONSTITUTION.md）：
//! - API key 绝不写入代码 / 仓库 / 审计 / 日志 / Debug 输出。
//! - 所有 LLM 请求响应、工具调用结果、策略拒绝都产生审计事件。
//! - 工具执行前必须过能力策略检查（默认拒绝）。
//! - 绝不绕过 `sandbox` crate 直接起 QEMU。

pub mod agent;
pub mod audit_hook;
pub mod compiler;
pub mod config;
pub mod dispatch;
pub mod error;
pub mod identity;
pub mod llm;
pub mod message;
pub mod policy;
pub mod presets;
pub mod prompt;
pub mod sse;
pub mod tempdirs;
pub mod tools;

pub use agent::{AgentLoop, AgentOutcome};
pub use compiler::{
    compile_freestanding, CompileOutput, CompilerConfig, ToolchainError, ToolchainSource,
    ZigConfig, CRT0_INJECTED, GCC_NAMES, TOOLCHAIN_URL, ZIG_NAMES, ZIG_URL,
};
pub use config::AgentConfig;
pub use dispatch::{
    AgentHandle, AgentId, DispatchError, Dispatcher, LocalAgent, LocalDispatcher, Task, TaskId,
    TaskOutcome,
};
pub use error::AgentError;
pub use identity::{next_agent_id, DEVICE};
pub use llm::{DeepSeekClient, LlmClient, MockLlm, OpenAiCompatClient};
pub use message::{
    ChatMessage, ChatRequest, ChatResponse, Choice, FunctionCall, StreamEvent, ToolCall, Usage,
};
pub use policy::WorkspacePolicy;
pub use presets::{builtin_presets, find_preset, ProviderPreset, DEFAULT_PRESET_ID};
pub use prompt::build_system_prompt;
pub use tempdirs::{
    sweep_stale_temp_dirs, sweep_stale_temp_dirs_with_age, DATA_DIR_NAME, TEMP_DIR_MAX_AGE,
    TEMP_PREFIX,
};
pub use tools::{
    execute_tool, tool_specs, tools_json, SandboxRequester, ToolContext, ToolSpec, VM_MEMORY_MB,
};
