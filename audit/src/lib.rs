//! audit — 智芯城 RiscDom 的审计层。
//!
//! 落地项目宪法第 2 条：**审计日志在 AI 之外，append-only，不可关闭。**
//!
//! 设计要点：
//! - 存储为 SQLite（`rusqlite` + bundled），事件表通过 `BEFORE UPDATE` /
//!   `BEFORE DELETE` 触发器硬保证 append-only。
//! - 每条事件带 `prev_hash` / `hash`，构成 SHA-256 hash chain；`verify_chain`
//!   可定位到第一个断裂的事件 id。
//! - 连接启用 `journal_mode=WAL` + `busy_timeout`（v0.8）：多进程写同一文件时
//!   互不阻塞；写入被锁的重试有上限，耗尽后**返回错误**，绝不静默丢弃。
//! - 不存在任何 UPDATE / DELETE / 关闭开关 API。
//!
//! 依赖方向：`sandbox → audit`。本 crate **不依赖** sandbox。

pub mod error;
pub mod event;
pub mod hash;
pub mod run;
pub mod sink;
pub mod store;

pub use error::AuditError;
pub use event::{AuditEvent, StoredEvent};
pub use hash::{compute_hash, verify_chain, ChainStatus, GENESIS_PREV_HASH};
pub use run::{
    abandoned_end, canonical_json, derive_runs_from, fingerprint, parse_run_end, parse_run_start,
    run_end, run_end_detail, run_interval, run_start_detail, short_fingerprint, RebuildReport,
    RunEndPayload, RunRecord, RunStartPayload, RunStatus, ACTION_RUN_ABANDONED, ACTION_RUN_END,
    ACTION_RUN_START, FINGERPRINT_SCHEMA_V1, SHORT_FINGERPRINT_LEN,
};
pub use sink::{AuditFailureReporter, AuditSink, FileAuditSink, SqliteAuditSink};
pub use store::{
    AuditStore, EventFilter, APPEND_BACKOFF_BASE, APPEND_MAX_ATTEMPTS, BUSY_TIMEOUT,
    OPEN_BACKOFF_BASE, OPEN_MAX_ATTEMPTS,
};

/// The default failure reporter: put it where a human will see it.
///
/// A producer that cannot surface an error any other way (the sandbox, the agent
/// loop) calls this, so a failed audit write is never silent (v0.8). The host
/// does not use it — it has a queue and an alert instead.
pub fn report_failure(error: &AuditError) {
    eprintln!("audit: failed to write an audit event: {error}");
}
