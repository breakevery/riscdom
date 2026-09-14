//! sandbox — QEMU RISC-V 沙箱管理。
//!
//! 对外 API：
//! - [`vm::RiscVVirtualMachine`] / [`vm::VMConfig`] — 生命周期与配置
//! - [`platform`] — 平台端点抽象（QMP / 串口）
//! - [`qmp`] — 最小 QMP 客户端
//! - [`audit_sink`] — 审计接口（占位，最终由 `audit` crate 替换）
//! - [`error::SandboxError`] — 错误类型

pub mod audit_sink;
pub mod error;
pub mod platform;
pub mod qmp;
pub mod vm;
