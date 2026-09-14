//! audit — 智芯城 RiscDom 的审计层。
//!
//! 落地项目宪法第 2 条：**审计日志在 AI 之外，append-only，不可关闭。**
//!
//! 设计要点：
//! - 存储为 SQLite（`rusqlite` + bundled），事件表通过 `BEFORE UPDATE` /
//!   `BEFORE DELETE` 触发器硬保证 append-only。
//! - 每条事件带 `prev_hash` / `hash`，构成 SHA-256 hash chain；`verify_chain`
//!   可定位到第一个断裂的事件 id。
//! - 不存在任何 UPDATE / DELETE / 关闭开关 API。
//!
//! 依赖方向：`sandbox → audit`。本 crate **不依赖** sandbox。

pub mod error;
pub mod event;
pub mod hash;
pub mod sink;
pub mod store;

pub use error::AuditError;
pub use event::{AuditEvent, StoredEvent};
pub use hash::{compute_hash, verify_chain, ChainStatus, GENESIS_PREV_HASH};
pub use sink::{AuditSink, SqliteAuditSink};
pub use store::{AuditStore, EventFilter};
