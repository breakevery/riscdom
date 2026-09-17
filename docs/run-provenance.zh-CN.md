[English](run-provenance.md) | 中文

# Run 溯源 — 设计提案（v0.4 批次 1a）

> **状态：仅提案。** 本批次不改代码、不改 audit 源码。以下内容全部基于当前实现；开工前需要人拍板的
> 点列在 §6。

## 0. 目标

让**一次运行**成为审计日志里的一等公民：每次 run 有唯一 ID、配置指纹与审计区间。v0.5 的
「换配置重跑」与 v0.6 的「自动对比两次结果」都建在它之上。

设计遵循宪法：宿主持有生命周期，审计日志位于 AI 之外且 append-only，凡是 AI 能够触达的东西都不得
伪造溯源信息。

## 1. 数据模型

### 1.1 Run ID

**推荐：`run_<uuidv7>` —— 规范 UUID 形式的 32 位小写十六进制，加前缀。**

```text
run_0192f4c1-8a3d-7c2e-9f10-6b1d4e0a55aa
```

- UUIDv7 按时间有序，ID 天然按时序排列，对索引友好。
- 128 位（时间 + 随机）使跨机器、跨数据库、跨导出的碰撞成为不必考虑的问题。
- 小写十六进制、不带花括号：文件名安全、JSON 安全、日志安全，粘贴不会歧义。
- `run_` 前缀便于 grep，并与会话 ID、快照名区分开。

**成本证据：** `uuid 1.26.1` **本来就在** workspace 的 lock 文件里 —— 它经 `tauri-utils` /
`schemars` 传递引入，而 `host` 无论如何都会构建它们。因此把 `uuid`（带 `v7` feature，其 `getrandom`
同样已在依赖图中）提为 `audit` 的**直接**依赖，只是新增一条 direct edge 与一个 feature 开关，
**不新增第三方 crate**。前缀手写在 `Uuid::now_v7()` 之上，存储层不依赖任何格式化 helper。

备选：

| 方案 | 不推荐的原因（代价） |
|---|---|
| `uuid` v4（随机） | 依赖成本相同，但非时间有序：列出 run 需要额外排序列，人读日志也看不出先后。 |
| 自研 `run_<yyyymmdd-hhmmss-ms>-<counter>-<rand4>` | 零依赖、可读、可排序 —— 但碰撞与时钟回拨由我们自己扛，时钟回跳会产生乱序 ID。若不想新增 `uuid` 直接依赖，这是可接受的兜底。 |
| 整数 `run_seq`（`AUTOINCREMENT`） | 最小、正是索引想要的形态 —— 但不可移植：两个数据库、两份导出、两台机器无法合并，而 v0.5 的"重跑"对比恰恰想在数据库之外指名一次 run。 |

### 1.2 配置指纹

**推荐：对规范化 JSON 求 SHA-256，并带显式 schema 标记。**

```text
fingerprint        = sha256(canonical_json)
fingerprint_schema = "riscdom.run.fingerprint.v1"
展示形式           = 前 16 个十六进制字符（库里存完整 64 位）
```

规范化规则（都很便宜，且可测）：

1. UTF-8；对象键按字典序排序；无多余空白。
2. 字符串原样；本就是整数的数字规范为整数（内存 MiB、迭代上限、超时 ms）。
3. Windows 路径规范化：绝对路径、盘符小写、分隔符统一为正斜杠。
4. **排除**易变值，且绝不允许混入：run ID、时间戳、session ID、计数器、临时端口、工作区根路径、
   机器名。
5. **密钥整体排除** —— 不做哈希、不截断、不留"后四位"。API key 本来就不进审计日志的任何字段；
   key 的哈希仍然是 key 派生值。指纹覆盖的是**服务商、base URL 与模型名**。

v1 字段清单见 [附录 A](#附录-a--指纹字段v1)。未知或不可读的值记录为 `"unknown"` 而非省略，
这样指纹的含义不会悄悄变化 —— 这对 v0.5 很重要：两个指纹若仅因某个不可读字段而不同，不能看起来
一模一样。

**推荐拆分：** 一个 run 级指纹，外加一个嵌套 `vm` 对象以便单独比较（`fingerprint.vm.*`），
这样 v0.5 不必 diff 整份文档就能说"只换了 QEMU"。嵌套对象在规范化 JSON 里不增加额外成本。

### 1.3 审计区间

**推荐：在索引行里同时记录区间的两端。**

| 字段 | 含义 |
|---|---|
| `start_seq` | `run.start` 事件的 `id`（该 run 的第一条入链事件） |
| `end_seq` | `run.end` 事件的 `id`；run 尚未结束时为 `NULL` |
| `started_at_ms` | `run.start` 的 `timestamp_ms` |
| `ended_at_ms` | `run.end` 的 `timestamp_ms`；未结束时为 `NULL` |

`seq`（即 `audit_events.id`，`INTEGER PRIMARY KEY AUTOINCREMENT`）是权威区间：它在单个数据库内无空洞、
正是哈希链的排序依据、也是 `audit-verify` 遍历的顺序。时间戳服务于人、UI 与跨机器报告，属于**参考**
信息 —— 系统时钟会动。

区间约定：`start_seq` 与 `end_seq` 均**包含**在内，run 拥有所有满足 `start_seq <= id <= end_seq`
的事件（成员关系为何是推导而非逐行存储，见 §2.2）。

## 2. 存储位置

### 2.1 当前实现给了我们什么

```sql
CREATE TABLE IF NOT EXISTS audit_events (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp_ms INTEGER NOT NULL,
    actor        TEXT    NOT NULL,
    action       TEXT    NOT NULL,
    detail_json  TEXT    NOT NULL,
    prev_hash    TEXT    NOT NULL,
    hash         TEXT    NOT NULL UNIQUE
);
```

链哈希精确等于：

```text
sha256(prev_hash | "|" | timestamp_ms | "|" | actor | "|" | action | "|" | detail_json)
```

由此得到本文档**最重要的一条约束**：**哈希只覆盖上述五个字段，别的都不覆盖。** 因此给
`audit_events` 加一列等于把该列放到哈希之外 —— 任何拿 SQLite 客户端的人都能改掉 `run_id` 列，而
`audit-verify` 依然报 `Intact`。凡是要防篡改的东西，必须放在 `detail_json` 里，或者自成一条事件。

### 2.2 选项

| 方案 | 形态 | 代价 |
|---|---|---|
| **A. 给 `audit_events` 加列** | `ALTER TABLE audit_events ADD COLUMN run_id TEXT` | 查询快（"某 run 的事件"一次索引查找）。但该列不在哈希内（见 §2.1），溯源因此可伪造；若扩展哈希覆盖它，则所有既有行的哈希失效、必须全链重算。**否决。** |
| **B. 链上标记 + 派生索引**（推荐） | 新增两个 action（`run.start` / `run.end`），把 `run_id` 与指纹放进 `detail_json`；另建 `runs` 表作为索引 | 链结构与哈希语义零改动；溯源可防篡改；索引可由链重建。代价：一次小迁移、一条重建路径、每个 run 多一次写入。 |
| **C. 只建独立表** | 有 `runs` 表，链上无标记 | 写起来最省，但 run 元数据不在链上：改 `runs` 里的指纹或区间，`audit-verify` 检测不到。与 A 同理否决。 |

### 2.3 推荐方案

**方案 B。** 具体：

1. host 在 run 开始时追加 `run.start`、返回时追加 `run.end`。两者都是普通审计事件，因此与其他事件
   一样入链。`run.start` 的 `detail_json` 携带：

   ```json
   {
     "run_id": "run_0192f4c1-8a3d-7c2e-9f10-6b1d4e0a55aa",
     "fingerprint": "<64 hex>",
     "fingerprint_schema": "riscdom.run.fingerprint.v1",
     "session_id": "<host session id>",
     "parent_run_id": null,
     "resumed_from_snapshot": null
   }
   ```

   `run.end` 携带 `run_id`、`status`（`ok` / `failed` / `interrupted`）与原因字符串。

2. 一次迁移创建索引表（纯新增，`CREATE TABLE IF NOT EXISTS`）：

   ```sql
   CREATE TABLE IF NOT EXISTS runs (
       run_id            TEXT PRIMARY KEY,
       session_id        TEXT,
       parent_run_id     TEXT,
       fingerprint       TEXT NOT NULL,
       fingerprint_schema TEXT NOT NULL,
       started_at_ms     INTEGER NOT NULL,
       ended_at_ms       INTEGER,
       start_seq         INTEGER NOT NULL,
       end_seq           INTEGER,
       status            TEXT NOT NULL,
       fingerprint_json  TEXT NOT NULL
   );
   ```

3. `runs` 表是**派生的，而非权威**：它存在只为快速列表与 UI，可由扫描链上的 `run.start` /
   `run.end` 重建。它同样不在哈希内，因此本设计不假装它防篡改 —— 取而代之，**重建即是校验**（§3.3）。
4. 把被哈希的那份规范化 JSON 存进 `fingerprint_json`。没有它，指纹只能比较、无法解释，v0.6 的
   "这两次之间改了什么"就会退化成猜。
5. 成员关系**由区间推导**，不逐事件存储：`runs` 行带 `[start_seq, end_seq]`，"事件 N 属于哪个 run"
   是一次范围查找。这让事件表结构保持冻结（即方案 A 的问题），同时在 `start_seq` 建索引后仍是 O(1)。

代价小结：一次迁移、每个 run 边界多一次追加、一条重建例程，以及一条必须走索引的读取路径。换来的
是链结构、哈希公式、append-only 触发器与所有既有行**一字未动**。

## 3. 向后兼容

### 3.1 旧记录

旧事件没有 run 标记，因此不属于任何 run，读取时表现为**未归属（unattributed）**：绝不重写、绝不
补发合成 run。后期批次可在 UI 上给一条 `legacy` 提示，把这件事讲清楚，而不是假装历史一直被埋点。
用新代码打开旧数据库可以正常工作：`runs` 表建成空表，`run.start` 事件此时尚不存在。

### 3.2 哈希链

**推荐：永不重算、永不重写。** 链在构造上就是 append-only（`BEFORE UPDATE` / `BEFORE DELETE`
触发器抛 `RAISE(ABORT, …)`）；新标记追加在既有事件之后，与任何普通事件无异。新旧在一条链、一张表里
共存，不需要标记位、不需要版本列、不需要第二个文件。

另一种做法 —— 重算整条链、把 run ID 注入历史行 —— 代价是全量重写、摧毁项目赖以成立的 append-only
保证，并改动已经出现在已发布产物里的哈希。**否决，且值得写进任何未来方案的文档里。**

### 3.3 `audit-verify`

链的判定语义与退出码**保持不变**（`0` 完整、`1` 断裂、`2` 用法或 I/O 错误），既有脚本与 CI 步骤无一
受影响。新增项均为**可选、默认关闭**：

- `audit-verify <db> --runs` —— 交叉校验索引与链：每条 `runs` 行都要能对上一条 `run_id`、指纹与
  `start_seq` 相同的 `run.start` 事件；每条 `run.end` 与其行一致；未结束的 run 必须没有 `end_seq`。
  结论单独成节报告；一旦发现问题，退出码变为 `1`（日志与溯源互相矛盾，这正是运维绝不能错过的状态）。
- `--rebuild-index <db>` —— 由链重建 `runs`（索引被篡改或截断时的修复路径）。只写 `runs`，
  绝不写 `audit_events`。

需在 §6 决定：`--runs` 是 v0.4 的一部分，还是留给后续批次。

## 4. 生成时机与归属

### 4.1 谁生成 run ID

**host，且只有 host。** 宿主监控层持有进程生命周期、VM 槽、会话存储与 UI 通道（宪法 §4），也是唯一
能看到一次 run 从头到尾的层。`agent` 与 `sandbox` 不得生成 ID：它们运行在 AI 能够影响的边界内，
**AI 能选的 ID 不是溯源**。host 也不向下传递：run ID 是 host 侧属性，只记进审计日志，不是 agent 循环
能影响的参数。

### 4.2 run 何时开始与结束

**一次 run = 一次 `AppState::run_agent` 调用。** 这正是 v0.5 换配置重跑、v0.6 对比的单位，所以边界
必须与它严格一致。

| 时机 | 动作 |
|---|---|
| `run_agent` 通过就绪门禁（LLM、工具链、QEMU） | 生成 ID、计算指纹、追加 `run.start`；其返回的 `seq` 即 `start_seq` |
| 第一条 agent/LLM/工具事件 | 已在区间内 |
| `run_agent` 返回（成功、报错或模型给出最终答复） | 追加 `run.end`，随后把 `end_seq` / `ended_at_ms` / `status` 写回 `runs` 行 |

从未返回的 run（崩溃、被杀、断电）保持 `end_seq = NULL`：它是**未结束**，不是损坏。下次启动时 host
可以追加 `host.run.abandoned`（普通入链事件，带自己的检测时间戳与被放弃的 `run_id`），并把索引行标为
`abandoned`。链上**绝不**为一次没有结束的 run 伪造 `run.end`。

与会话的关系：一个 host 会话（对话）横跨多次 run；`session_id` 记录在 run 上，是一对多。与 VM 的关系：
VM 可能比 run 长寿（v0.3 起归 host 所有且可复用），因此 run 通过指纹记录它**实际用到**的 VM 配置，
而不宣称拥有 VM。

### 4.3 快照恢复

**推荐：新开一次 run，并与旧 run 建立链接。**

`resume_from_snapshot_real` 产出的是实质不同的执行：不同的内存内容、不同的起点，且自 v0.3.1 起还可能
是**不同的 QEMU 二进制**。把它记成前一次 run 的延续，会破坏 v0.5/v0.6 依赖的"一次 run = 一次执行"
契约。因此恢复会新开 run，并带：

- `parent_run_id` = 被恢复快照所属的 run（若快照是在重启后恢复、产出方只能从快照自身元数据得知，
  则为 `NULL`）；
- `resumed_from_snapshot` = 快照名；
- `status` 照常为 `ok`/`failed`。

另一种做法（延续旧 run）被否决：一个跨越 stop/start 对的区间会悄悄混入来自两个不同 VM 实例的事件。

## 5. v0.4 最小实现范围

**范围内（§6 拍板后进入批次 1b）：**

1. audit：新增 `run.start` / `run.end` 这对 action（名称 + detail 形状）与纯新增的 `runs` 迁移；
   `audit_events`、其触发器与哈希公式**零改动**。
2. 指纹：规范化器、v1 字段清单，以及一个把固定输入的**逐字节**结果钉住的单测（这样将来任何改动都是
   有意的版本升级，而不是意外）。
3. host：生成 ID、计算指纹、在 `run_agent` 前后追加标记、维护 `runs` 行、处理启动时的未结束 run。
4. 读取路径：`list_runs` / `get_run`（只读），以及索引重建例程。

**明确不做：**

- 两次 run 的对比、指纹 diff、"改了什么"报告 —— **v0.6**。
- 换配置重跑与黄金路径录制 —— **v0.5**。
- 导出包、run 级归档、远程备份 —— **v0.6 或更晚**。
- 超出既有审计面板的 runs UI —— 后续批次。
- 任何改动 `audit_events`、哈希公式或历史行的做法 —— **永不在计划内**。
- 存储提示词、源文件或超出审计现状的工具参数；run 行只带哈希与配置，不带内容。

## 6. 开工前需要拍板的点

1. **Run ID 方案。** UUIDv7（推荐；新增 `uuid` 直接依赖 + `v7` feature，不新增第三方 crate）
   对比零依赖自研时间序 ID。只影响 §1.1。
2. **存储形态。** 是否接受"链上标记 + 派生索引"（方案 B）这一结构，含新增 `run.start` / `run.end`
   两个 action。这是会改动 audit crate 公开面的决定。
3. **run 粒度。** 确认一次 run = 一次 `run_agent` 调用（不是一次用户回合、不是一次会话）。
4. **快照恢复。** 确认"新开 run + `parent_run_id`"，而非延续产出方 run。
5. **"region 配置"。** 需求里提到 *region* 配置字段。当前 `LocalSettings` 只有 `version`、
   `toolchain_path`、`qemu_path`，整个 workspace 里也没有 region 概念。请说明它指什么（计划中的字段、
   托管区域设置，还是"运行时配置"的笔误），以便 v1 字段清单完整。
6. **指纹是否含 system prompt。** 用其哈希放进指纹（便宜、能捕捉提示词漂移，但措辞一改指纹就变），
   还是不纳入以保持指纹在文档改动间稳定。推荐：**放进独立的 `prompt` 对象**，不进入主等值路径。
7. **`audit-verify --runs` 的时机。** v0.4 就带上索引交叉校验，还是 v0.4 只做埋点 + 读取路径、
   把校验留到 v0.6 的对比工作。推荐：v0.4 就做 —— 它是派生索引唯一可信的来源。
8. **VM 指纹拆分。** 确认嵌套 `vm` 对象（以便表达"只换了 QEMU"），以及考虑到 VM 可比 run 长寿，
   VM 配置是否应进入 run 指纹。

## 附录 A — 指纹字段（v1）

| 分组 | 字段 |
|---|---|
| `schema` | `fingerprint_schema`（`riscdom.run.fingerprint.v1`）、`app_version` |
| `llm` | `provider_id`、`base_url`、`model`（**绝不含 key**） |
| `agent` | `max_iterations`、`request_timeout_secs`、工具集哈希（名称 + 描述 + 参数 schema）、编译器 `march` / `mabi` / 链接地址 / 注入 crt0 标记、语言白名单 |
| `vm` | `memory_mb`、machine（`virt`）、cpu（`rv64`）、QEMU 路径 + 报告的版本、快照模式 |
| `toolchain` | 解析出的 GCC 路径 + 报告的版本、来源（`EnvVar` / `KnownPath` / `Path` / `Manual`） |
| `policy` | workspace policy 版本、扩展名白名单、防穿越标记 |

每个字段要么来自**解析后的**配置（而非用户未校验的输入），要么记为 `"unknown"`。路径按 §1.2 第 3 条
规范化。

## 附录 B — 新增审计 action

| Action | Actor | Detail |
|---|---|---|
| `run.start` | `host` | `run_id`、`fingerprint`、`fingerprint_schema`、`session_id`、`parent_run_id`、`resumed_from_snapshot` |
| `run.end` | `host` | `run_id`、`status`、`reason` |
| `host.run.abandoned` | `host` | `run_id`、`detected_at_ms`（入链事件；索引行转为 `abandoned`） |

`audit-verify` 与 JSONL 导出把它们当普通事件处理：无特例、`audit_events` 无新字段。
