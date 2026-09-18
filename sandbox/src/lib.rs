//! sandbox — 智芯城 RiscDom 的 QEMU RISC-V 沙箱层。
//!
//! 负责在 QEMU `virt` 机器上启动裸机 ELF，通过 QMP 控制，捕获串口输出，
//! 并提供（MVP 降级的）快照/回滚能力。
//!
//! 审计：所有对外操作都写入 [`audit::AuditSink`]。审计实现来自 `audit` crate
//! （append-only SQLite + hash chain），依赖方向为 `sandbox → audit`。
//!
//! # 快速上手
//!
//! ```no_run
//! use sandbox::{QmpEndpoint, RiscVVirtualMachine, SerialEndpoint, VMConfig};
//! use audit::{AuditSink, AuditStore, SqliteAuditSink};
//! use std::sync::{Arc, Mutex};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let audit: Arc<Mutex<dyn AuditSink>> =
//!     Arc::new(Mutex::new(SqliteAuditSink::new(AuditStore::in_memory()?)));
//! let config = VMConfig {
//!     kernel: "hello.elf".into(),
//!     memory_mb: 128,
//!     qmp: QmpEndpoint::tcp("127.0.0.1", 4444),
//!     serial: SerialEndpoint::tcp("127.0.0.1", 5555),
//!     snapshot_dir: "snapshots".into(),
//!     serial_observer: None,
//!     incoming_snapshot: None,
//!     incoming_relay_addr: None,
//!     qemu_exe: None,
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
//! - [`error`] — [`SandboxError`]：错误类型

pub mod error;
pub mod platform;
pub mod qemu_discover;
pub mod qmp;
pub mod relay;
pub mod vm;

pub use error::SandboxError;
pub use platform::{QmpEndpoint, SerialEndpoint};
pub use qemu_discover::{discover as discover_qemu, QemuDiscoverError, QemuLocation, QemuSource};
pub use qmp::QmpClient;
pub use vm::{RiscVVirtualMachine, VMConfig, VM_CPU, VM_MACHINE};
