[English](README.md) | 中文

# audit

智芯城（RiscDom）的**审计层**。落地项目宪法第 2 条：

> 审计日志在 AI 之外，append-only，不可关闭。

## 语义

- **append-only（硬保证）**：SQLite 表 `audit_events` 上有 `BEFORE UPDATE` /
  `BEFORE DELETE` 触发器，任何改写都会 `RAISE(ABORT, 'audit_events is append-only')`。
  Rust API 侧**不存在** UPDATE / DELETE，也没有关闭审计的开关。
- **hash chain**：每条事件的 `hash = sha256(prev_hash | "|" | timestamp_ms | "|" |
  actor | "|" | action | "|" | detail_json)`（小写 hex）。第一条（创世）事件的
  `prev_hash` 为 64 个 `0`。
- **可独立验证**：`verify_chain` 会重算整条链，并**定位到第一个断裂事件的 id**。
- **多进程共用一个 `audit.db`**：连接运行在 WAL 下并带 5 秒 `busy_timeout`，**打开序列**与每次 append 在遇到数据库锁时都会重试（`OPEN_MAX_ATTEMPTS` / `APPEND_MAX_ATTEMPTS`，从 20 ms 起指数退避）。打开路径之所以要重试，是因为 `PRAGMA journal_mode = WAL` 会**绕过** busy_timeout 直接回 `SQLITE_BUSY`——两个进程同时创建一个全新的 `audit.db`，过去必有一个失败。任一预算耗尽的锁一律报错，绝不静默丢弃。

## 模块

- `event` — `AuditEvent` / `StoredEvent`
- `store` — `AuditStore`（`open` / `in_memory` / `append` / `last_hash` / `get` /
  `list` / `count` / `export_jsonl` / `all`）+ `EventFilter`
- `hash` — `compute_hash` / `verify_chain` / `ChainStatus` / `GENESIS_PREV_HASH`
- `sink` — `AuditSink` trait + `SqliteAuditSink`（生产）+ `FileAuditSink`（示例）
- `error` — `AuditError`

## audit-verify CLI

```text
cargo run -p audit --bin audit-verify -- <path-to-db>
```

退出码：

| code | 含义 |
| --- | --- |
| 0 | 链完整 `Intact { length: N }` |
| 1 | 链断裂 `Broken { at_id, reason }` |
| 2 | 打开失败 |

示例：

```text
$ audit-verify clean.db
Intact { length: 3 }
$ audit-verify tampered.db
Broken { at_id: 2, reason: "hash mismatch: expected 8ff2…, found 467b…" }
```

## 事件类型（actor 由产生方决定）

`vm.start` / `vm.stop` / `vm.snapshot.save` / `vm.snapshot.load` / `serial.read` /
`serial.write`（sandbox）；`agent.user.input` / `agent.llm.request` /
`agent.llm.response` / `agent.tool.call` / `agent.tool.result` /
`agent.policy.deny` / `agent.compile.start` / `agent.compile.result`（agent）。

## 依赖方向

`audit` **不依赖** `sandbox` / `agent`；由它们依赖本 crate。

## 测试

```text
cargo test -p audit
```

覆盖：空链 / 三事件链 / 篡改定位 / UPDATE+DELETE 被触发器拒绝 / 查询过滤 /
JSONL 导出 / CLI 退出码 / 并发 append / 并发**打开**（8 线程抢开同一新文件，以及一个已在 WAL 的文件）。

## v0.2 TODO

- 审计日志分片（按时间/大小滚动，同时保持链连续）
- 远程备份（追加式镜像，不破坏 append-only 语义）
