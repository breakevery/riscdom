//! sandbox — QEMU RISC-V 沙箱管理。
//!
//! 对外 API：见 [`vm::RiscVVirtualMachine`]、[`vm::VMConfig`]、
//! [`platform`]、[`audit_sink`]、[`error::SandboxError`]。

pub mod audit_sink;
pub mod error;
pub mod platform;
pub mod vm;
