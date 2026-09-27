[English](error-model.md) | 中文

# 错误模型

**状态** v1.0 规范（里程碑 [M1](roadmap-v1.0.zh-CN.md)）｜ **日期** 2026-09-27 ｜ **基线** v0.9.9
（`3365970`）｜ **受众** 内核开发者 —— 其中 §2 与 §8（线上层）同时是**发行集成者**的规范。

**为什么错误模型属于冻结的一部分。** 调用方只有在失败说清自己是哪一类时才能行动。「出了点问题」会让
每个客户端要么什么都重试、要么什么都不重试，而两者都是错的：重试一次拒绝是浪费网络，不重试一次超时是
丢工作。本文是 [v1.0 纲领](roadmap-v1.0.zh-CN.md) §6 所要求的六件事中的第三件。

**已经成立的事。** HTTP 信封、它的四个字段，以及「v0.9 封闭」的 `code` 清单，规范在
[`control-plane-api.zh-CN.md`](control-plane-api.zh-CN.md) §4。本文不取代那一节：它**扩展**它 —— 同样的
字段名、更长的原因清单、同样的映射规矩（§8）—— 并补上进程内的那一半 `DispatchError`，其形状是
[decisions §12](decisions.zh-CN.md) 已经定下的（分类、`Retryable` 标志、`Cause` 链，以及跨进程、跨设备
干净序列化）。

## 1. 两层，一套词汇

- **[已定]** **进程内**：`DispatchError`，调度器在一个任务没产出结果时返回的类型
  （[`agent/src/dispatch.rs`](../agent/src/dispatch.rs)）。它可 `serde` 序列化，因为执行者进程会把它作为
  一行 JSON 写回。
- **[已定]** **线上**：每个非 2xx 应答携带一个错误对象，`{code, message, retryable, cause}`。这是不是
  本程序的客户端所看到的东西。
- **[已定]** **线上层是内核的回答，不是调度的回声。** 在边界上，一个 `DispatchError` 变成一个 `code`
  加一个状态码（§8）；两套词汇由那条映射关联，但不是同一个枚举。

## 2. 线上层

**[已定]** 今天的对象，形状不变：

```json
{
  "code": "not_found",
  "message": "no run with id run-12345-7",
  "retryable": false,
  "cause": "run_id"
}
```

| 字段 | 类型（v0.x） | 类型（`/v1/` 起） | 含义 |
|---|---|---|---|
| `code` | string | string | 稳定、机器可读。**v0.9 封闭**；v1.0 扩展它（§8）之后再次封闭。 |
| `message` | string | string | 给人读。**绝不含机密** —— 无 token、无 key、无凭据。 |
| `retryable` | bool | bool | 一次**完全相同**的重试是否可能成功（§4）。 |
| `cause` | string \| null | **string[]** | 出错的输入字段或子系统。v1.0 把它变成**链**：最外层在前，于是 `["network", "executor"]` 读作「网络，其下的执行者」。 |

- **[已定]** **`cause` 在 `/v1/` 才变成链，之前不变。** 冻结之前，该字段就是
  [`control-plane-api.zh-CN.md`](control-plane-api.zh-CN.md) §4 写下的样子（单个字符串或 `null`），所以
  本文不会弄坏任何 v0.x 客户端。这次切换属于前缀变更的同一个批次。
- **[已定]** **`message` 给人，`code` 与 `cause` 给程序。** 按 `message` 匹配的客户端在设计上就是坏的。

## 3. `DispatchError`：v1.0 的形状

**[已定]** 六个变体。`NoSuchAgent` 是 v0.8 交付的，**保留**；另外五个随跨设备传输到来 —— 这一点
[`control-plane-api.zh-CN.md`](control-plane-api.zh-CN.md) §4 已经预告了（「跨设备传输会扩展它（超时、
远端错误）」）。

```rust
enum DispatchError {
    /// 自 v0.8 保留：本调度器里没有句柄持有 `task.target`。
    NoSuchAgent(AgentId),

    /// 任务到达了某个传输层，而传输层失败了。
    Network { kind: NetworkKind },

    /// 到达了执行者，它拒绝了 —— 策略、capability，或队列已满。
    Refused { reason: String },

    /// 到达了执行者，它死了。`None` 表示「死时没有状态码」。
    Crashed { exit_status: Option<i32> },

    /// 到达了执行者，它跑了任务的**一部分**：`failed` 项没有跑。
    Partial { completed: usize, failed: usize },

    /// 请求在出门之前就是畸形的。
    Invalid { reason: String },
}

enum NetworkKind { Timeout, Unreachable }
```

- **[已定]** **`Failed(String)` 被替换，不是被保留。** v0.8 交付的那个兜底（「到达了执行者，但运行
  失败」）正是让重试变成抛硬币的东西；现在每一次调度失败都会说清自己是五种里的哪一种。任何会发送
  `DispatchError` 的东西都不会在 v1.0 之前发布，所以不需要兼容垫片 —— 但
  [`control-plane-api.zh-CN.md`](control-plane-api.zh-CN.md) §4 的映射（今天写着 `Failed → internal`）
  会在落地本枚举的同一个批次里换成 §8 的表。
- **[已定]** **线上按变体名标记，不按编号。** 该枚举以变体名打标（`{"Network":{"kind":"Timeout"}}`），
  正是 v0.8 那两个变体已经序列化成的形状，于是一个只认识六个中的两个的读方会大声失败，而不是读错一个
  数字。
- **[已定]** **`Partial` 不是成功。** 它意味着有一部分工作没有发生；把它当成功的调用方，会报告它并没有
  拿到的成果（见 §4）。

## 4. 哪些失败可重试

**[已定]** 每个变体都要回答这个问题，而这就是 `Retryable` 标志存在的唯一理由（decisions §12）。这个
标志是**错误**的属性，绝不是调用方耐心的属性：客户端可以选择去重试一次拒绝，但模型不会称它可重试。

| 变体 | `retryable` | 为什么，以及怎么做 |
|---|---|---|
| `Network::Timeout` | **是** | 这次尝试可能根本没到达执行者。重试，最好带抖动，免得一整队同步重试。 |
| `Network::Unreachable` | **是** | 对端是*此刻*不在；按定义这是暂时状态。带**退避**重试。 |
| `Refused` | **否** | 执行者被到达了，它说不：capability 不足、部署者定的策略、队列已满。重试一个完全相同的请求，就是再问同一个问题、再得同一个答案。 |
| `Crashed` | **否** | 重试不会让一个进程复活。任务可以在执行者**重启之后**再试，而那次重启是某人的明确动作。 |
| `Partial` | **部分** | 只重试**失败的那些**（`failed` 项）。重跑 `completed` 声称做过的事，就是工作被做两遍的方式。 |
| `Invalid` | **否** | 调用方的输入错了。同样的输入会同样地失败；修的地方在上游。 |
| `NoSuchAgent` | **否** | 目标在这里不存在，它映射到 `not_found`（§8）。重试不能让它存在。 |

- **[已定]** **重试是「完全相同」。** 这个标志说的是*同一个请求*是否可能成功；一个必须被改动才可能成功
  的请求不是重试，而是一个新请求。
- **[已定]** **退避是调用方的事。** 内核只陈述 `retryable`，对时序一个字不说；监工自己选自己的节奏。
  这是红线 1（机制，不是策略）在重试上的样子。

## 5. `Cause` 链

- **[已定]** **进程内**：有真正的 source 时用 `Box<dyn std::error::Error>`，source 是一句拒绝的话时用
  字符串。凡调用方能据以分支的地方，变体携带的是数据而不是散文（`exit_status`、`completed`、`failed`、
  `kind`）。
- **[已定]** **线上**：链被摊平进 `cause`（`/v1/` 起是数组，见 §2），最外层在前，每个元素都是一个短小、
  稳定的 token 或字段名 —— 是客户端可以据以分支的那套词汇，绝不是一句会跨版本变化的句子。
- **[已定]** **链里永不携带机密。** §2 对 `message` 的规矩适用于每一个元素：可以是路径、字段名、子系统
  —— 绝不是 token、key，或一条带凭据的 URL。

## 6. 错误与审计链

- **[已定]** **失败的工具调用被记下，不被藏起来。** 执行者的一次工具失败会写成 `agent.tool.result`，
  带 `ok: false` 与原因，于是链里留有模型当时被告知的东西（[`agent/src/audit_hook.rs`](../agent/src/audit_hook.rs)、
  [`agent/src/tools.rs`](../agent/src/tools.rs)）。
- **[已定]** **错误是信息，重试是动作。** 一次成功的重试仍会在链里留下第一次失败。链只追加：它记录发生过
  什么，不记录系统希望发生过什么。
- **[已定]** **什么不被审计**：内核今天不为一次*调度*失败写任何东西。当 M（纲领 §9）重试一个任务时，
  那次重试是 M 的动作、以 M 的事件出现；内核不会长出一份重试日志。

## 7. 错误与 capability

- **[已定]** **`Refused` 常常是一个 capability 的回答。** 最常见的拒绝就是
  [`control-plane-api.zh-CN.md`](control-plane-api.zh-CN.md) §4 已经定义的那一个：行动者缺少该端点要求的
  capability，即 `403 forbidden` —— 而 403 **只**是认证或授权，从不是「这个参数我不喜欢」（那是 400）。
- **[已定]** **拒绝会说明缺哪个 capability。** 在这条信息不是机密的地方 —— 也就是永远：capability 是
  公开的 —— `message` 会点名被要求的那个 capability，`cause` 会指出 `capability`。一个分不清「我无权」
  与「那是畸形输入」的调用方，会去修错的东西。

## 8. HTTP 映射

**[已定]** 规矩：`403` 只是认证或授权；服务端无法使用的参数是 `400`；没准备好的依赖是 `503`。本表扩展
[`control-plane-api.zh-CN.md`](control-plane-api.zh-CN.md) §4 的清单；标 **v1.0** 的行是新增，它们随前缀
变更一起落地，于是没有 v0.x 客户端会看到它们。

| `code` | HTTP | 何时 | 来源 |
|---|---|---|---|
| `bad_request` | 400 | JSON 畸形、缺必填字段、`path`/`run_id` 不合法 | 已有 |
| `unauthorized` | 401 | 没有 token，或钩子拒绝了 token | 已有 |
| `forbidden` | 403 | 已认证，但行动者缺少该端点的 capability | 已有 |
| `not_found` | 404 | 未知的 `run_id`、`session_id`、快照名，或未知的调度目标 | 已有（`NoSuchAgent`） |
| `method_not_allowed` | 405 | 路径存在，但不是在这个方法下 | 已有 |
| `conflict` | 409 | 状态冲突：`resume` 时没有 VM、已有一次下载在跑 | 已有 |
| `payload_too_large` | 413 | 请求体超过该端点的上限 | 已有 |
| `internal` | 500 | 其余一切 —— 含执行者崩溃 | 已有（`Crashed`） |
| `not_implemented` | 501 | 已预留、但内核尚无方法的端点 | 已有 |
| `unavailable` | 503 | 依赖没准备好（无 LLM、无 QEMU、无工具链）—— 以及 **v1.0**：对端不可达 | 已有（`Network::Unreachable`） |
| `gateway_timeout` | 504 | **v1.0**：对端原则上可达，但没有及时应答 | 新增（`Network::Timeout`） |

- **[已定]** **`Partial` 是唯一还开着的行** —— 见 §9。今天它映射为 `500 internal` 且
  `cause: ["partial"]`，因为这个请求确实没有完成；专门的状态码尚未决定。
- **[已定]** **内核自己造成的传输失败不是 `500`。** `502` 与 `504` 的存在就是为了说「故障在我们之间」；
  用 `500` 去表达它们，等于让调用方去错的地方找。

## 9. 尚未定下的

- **[待定]** **`Partial` 的状态码**：带 `partial` 原因的 `500`，还是它自己的 `code`。两者都站得住；
  这个选择属于落地该枚举的那个批次。
- **[待定]** **是否允许任何新 `code` 出现在 `/v0/` 里。** 清单对 v0.9 封闭，诚实的读法是上面那些新增随
  `/v1/` 到来 —— 但若有一次修复需要提前用它，那是一次判断，不是一条规矩。
- **[待定]** **`cause` 链的确切 JSON**（token 数组，还是带 `kind` 的对象数组）。第一个返回它的端点落地
  时才定形。
