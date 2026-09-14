//! sandbox — 智芯城 RiscDom 的 QEMU RISC-V 沙箱层。
//!
//! 负责在 QEMU `virt` 机器上启动裸机 ELF，通过 QMP 控制，捕获串口输出，
//! 并提供（MVP 降级的）快照/回滚能力。所有对外操作都会写入 [`AuditSink`]。
//!
//! # 快速上手
//!
//! ```no_run
//! use sandbox::{FileAuditSink, QmpEndpoint, RiscVVirtualMachine, SerialEndpoint, VMConfig};
//! use std::sync::Arc;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let audit = Arc::new(FileAuditSink::new("audit.jsonl"));
//! let config = VMConfig {
//!     kernel: "hello.elf".into(),
//!     memory_mb: 128,
//!     qmp: QmpEndpoint::tcp("127.0.0.1", 4444),
//!     serial: SerialEndpoint::tcp("127.0.0.1", 5555),
//!     snapshot_dir: "snapshots".into(),
//! };
//! let mut vm = RiscVVirtualMachine::new(config, audit)?;
//! vm.start()?;
//! let _bytes = vm.serial_output();
//! vm.stop()?;
//! # Ok(())
//! # }
//! ```
//!
//! # 模块
//! - [`vm`] — [`RiscVVirtualMachine`] / [`VMConfig`]：生命周期与配置
//! - [`platform`] — [`QmpEndpoint`] / [`SerialEndpoint`]：平台端点抽象
//! - [`qmp`] — [`QmpClient`]：最小 QMP 客户端
//! - [`audit_sink`] — [`AuditSink`] / [`FileAuditSink`]：审计接口（占位）
//! - [`error`] — [`SandboxError`]：错误类型

pub mod audit_sink;
pub mod error;
pub mod platform;
pub mod qmp;
pub mod vm;

pub use audit_sink::{AuditEvent, AuditSink, FileAuditSink};
pub use error::SandboxError;
pub use platform::{QmpEndpoint, SerialEndpoint};
pub use qmp::QmpClient;
pub use vm::{RiscVVirtualMachine, VMConfig};
