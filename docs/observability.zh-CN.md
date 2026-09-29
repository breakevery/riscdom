[English](observability.md) | 中文

# 可观测性

**状态** v1.0 规范（M7g）｜ **日期** 2026-09-29 ｜ **面向读者** 管理员与运维者：运行一个节点、必须看见
它在做什么的人。

**本文是什么。** [decisions §17](decisions.zh-CN.md) 定了三件事 —— **结构化日志**、**指标** 与一条
**追踪 ID** —— 本文把它们写下来：一个节点自报什么、以什么形状、报在哪。它是 §17 要的那份**契约**
（「日志与指标按契约、而非按惯例保持机器可读」）；产生它的代码在后续批次落地。

**两个来源、一个身份。** 这里的一切描述的是节点关于**自己工作**的说法。一次*运行*做了什么，是审计链的
事（[control-plane-events.md](control-plane-events.zh-CN.md) 是那条流），而这个区分是刻意的 —— 见 §4。

## 1. 结构化日志

**每行一个 JSON 对象，写在 stderr 上。** 控制平面今天已经通过一个函数把运行时行写到 stderr
（`server/src/log.rs`）；在本契约下，那个函数发出一对象而不是一句话。

| 字段 | 类型 | 可缺 | 含义 |
|---|---|---|---|
| `ts` | number | 否 | 自 Unix 纪元起的毫秒 —— 本项目其它每个时间戳用的单位。 |
| `level` | `"error"` \| `"info"` | 否 | 该行自己的级别。`error` 是失败；`info` 是每连接的絮语。 |
| `target` | string | 否 | 这一行来自哪里 —— 模块路径，如 `riscdom_server::http`。 |
| `message` | string | 否 | 给人看的那句话。 |
| `agent_id` | string | 可 | 当这一行与某个 agent 有关时的那个 agent（§3 的身份）。 |
| `task_id` | string | 可 | 当这一行与某个被派发的任务有关时的那个任务。 |

- **开关不变。** `--log-level <off|error|info>`、默认 **`off`**，与今天完全一致，理由也相同：控制平面
  也会跑在**另一个程序里面**（CLI 的本地模式），那里 stderr 属于调用方 —— 掉进调用方 JSON 错误对象
  中间的一行，就是把它弄坏的一行。
- **二进制的横幅不是日志行。** `riscdom-server` 的启动横幅、用法文本与致命错误是那个程序的控制台输出
  —— 运维者要求看见的东西 —— 它们保持人类文字。内嵌的控制平面根本不会走那条路径
  （`server/src/main.rs`）。
- **永不出现秘密。** 本项目每个 JSON 对象都守的规矩：API key、token 或私钥绝不出现在一行里 —— 连截断
  的影子都不留。

## 2. 指标

**Prometheus 文本格式，一个端点。** 被问到指标时，节点以 Prometheus 文本格式回答
（`text/plain; version=0.0.4`）：每个族一条 `# HELP` 与 `# TYPE`，每个样本一行。

**路由 —— 在此定规格、实现留后。**

| 方法 | 路径 | capability | 应答 |
|---|---|---|---|
| `GET` | `/metrics` | **`status.read`** | Prometheus 文本格式 |

- **复用 `status.read`，不新增 capability。** [decisions §83](decisions.zh-CN.md) 是那条规矩：一条路由
  必须声明一个 capability，而没有路由需要的 capability 名就不是 capability。指标*正是*节点自身的状态
  —— 就是 `/v0/status` 回答的同一件事 —— 所以 `status.read` 是诚实的声明，词表不动。
- **它与别处一样要认证。** `/metrics` 和 `/v0/*` 同处一个 `Authn` 之下；唯一不认证的面是 Web UI 的
  资源（`/`、`/assets/*`），它们不带任何秘密。想不带 token 抓它的运维者，可以用 `--no-auth` 跑一个
  节点 —— 那是部署选择，不是第二道门。
- **加上它不是免费的**，这也是它属于后续批次的原因：`/metrics` 是
  [control-plane-api.md](control-plane-api.zh-CN.md) §5 表里的一行、也是 tool-schema 表里的一行，所以
  它得像其它每条路由一样过同样的闸（计数与标记表都有测试钉着）。

**指标族，以及每个数字今天已经住在哪。** 第一批刻意很小 —— 下面每个指标都是节点今天就能从
`/v0/status`、`/v0/audit/status` 或它自己的存储答出的事实。前缀是 `riscdom_`；名字说清它是什么，标签
只会把同一件事再说一遍。

| 指标 | 类型 | 来源 | 含义 |
|---|---|---|---|
| `riscdom_uptime_milliseconds` | gauge | `/v0/status` → `uptime_ms` | 本节点起了多久。 |
| `riscdom_connections` | gauge | `/v0/status` → `connections` | 此刻打开的客户端连接。 |
| `riscdom_sse_subscribers` | gauge | `/v0/status` → `sse_subscribers` | 此刻跟着事件流的客户端。 |
| `riscdom_agents` | gauge | `/v0/status` → `agents` | 本宿主知道的 agent 数。 |
| `riscdom_audit_events` | gauge | `/v0/audit/status` → `count` | 审计链里的行数。 |
| `riscdom_audit_failures` | gauge | `/v0/audit/status` → `failures` | 仍在等落盘的审计写入。 |
| `riscdom_info` | gauge = `1` | `/v0/status` → `version`、`agent_id` | 身份，用惯用的 `*_info` 指标：标签 `version` 与 `agent_id`。 |

- **没有无界标签。** 这里没有任何东西按运行、任务或同侪打标签：一个取值集合随工作量增长的标签，就是
  指标端点变成内存泄漏的方式。若哪天要按任务看，那是对审计链的查询，不是一个指标。
- **来源是 gauge 就用 gauge。** 这第一批族报的是*此刻为真*的东西；当某个量真的单调时才加 counter，且
  它得叫 `_total`。

## 3. 追踪

**一个身份、复用。** §17 的理由就是全部规则：审计链本就每个事件带一个身份，所以**追踪 ID 就是那一对**
—— `agent_id` 与 `task_id` —— 而不是要对齐的第二套命名空间。

- **`agent_id`** 是 `<device>-<pid>-<seq>`（[control-plane-api.md](control-plane-api.zh-CN.md) §2）：设备名、
  进程与序号。它说**哪个 agent** 做了这件事。
- **`task_id`** 是 `task-<pid>-<seq>`，或调用方自己的名字：它说这个 agent 在做**哪个工作单元**。事件
  envelope 已经同时携带两者，而 `null` 是合法的 `task_id` —— 不属于任何被派发任务的工作。

**那个缺口，以及规格怎么合上它。** [roadmap §12](roadmap-v1.0.zh-CN.md) 记着它：**`POST /v0/agent/run`
不接受 `task_id`**，于是当多个客户端跟着同一个节点时，一个事件帧无法归因到产生它的那个任务。修法很小、
是加法式的，而且 API 里已有先例：`POST /v0/tasks` 正是为此接受一个可选的 **`id`**。

- **规格中的改动**（未实现）：`POST /v0/agent/run` 的体里多一个**可选 `task_id`**。调用方点了名时，那次
  运行产生的每个帧、每一行日志都带上它；没人点名时 `task_id` 为 `null`，也就是今天的行为 —— 字段是
  加法式的，所以今天存在的一切都不改含义。
- **为什么不新造一个 ID。** 生成式追踪 ID 会让一个工作单元有两个标识、还逼着去关联它们 —— 那正是 §17
  拒绝的那套命名空间。调用方本来就知道自己在要什么；让它说出来，是合上这个缺口的最小改动。

## 4. 不覆盖什么

- **审计链不是可观测性。** 一个节点发生过什么的记录是那条链（[audit/README.md](../audit/README.md)）；
  可观测性是*活*的视图 —— 行、数字、一个可跟随的身份。指标不是证据，日志行不是记录：两者都不入链、
  不做哈希，也都不能交给验证者。
- **事件流是控制平面，不是日志。** `/v0/events` 是面向客户端的订阅，有 envelope、词表与补发
  （[control-plane-events.md](control-plane-events.zh-CN.md)）；§1 的行是节点自己的 stderr，给盯着这个
  进程的人看。
- **启动横幅、用法文本与致命错误**是 `riscdom-server` 二进制的控制台输出，不是结构化日志（§1）。
- **没有追踪后端、没有 agent、没有 exporter。** 本文冻结的是一个节点自报的*形状*；运维者把它送到哪里
  （日志搬运、Prometheus 抓取、收集器）是他们的事。节点从不主动拨向监控服务 —— 那会是项目在运营一项
  服务，正是 [roadmap §1](roadmap-v1.0.zh-CN.md) 红线所禁止的。
