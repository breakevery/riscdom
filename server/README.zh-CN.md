[English](README.md) | 中文

# riscdom-server —— 控制平面

> **适用 v0.9。v1.0 之前接口面不稳定。** 本文是**集成者**的文档：如何编译、如何启动、今天能得到什么答复。它实现的协议定稿于 [docs/control-plane-api.zh-CN.md](../docs/control-plane-api.zh-CN.md) 与 [docs/control-plane-events.zh-CN.md](../docs/control-plane-events.zh-CN.md)。

`riscdom-server` 是作为进程形态的 RiscDom 控制平面：人监工与 AI 监工共用同一套 HTTP + SSE 接口。它是架在内核门面（`host`，Layer 2）之上的 **Layer 3**，不引用任何 Tauri 类型。`tauri` 仍会被**链接**，因为 `host` 无条件依赖它——这是已知代价，不是引用。

本 crate 是**骨架**：两个 smoke 端点、事件流、认证钩子。API 表其余端点属后续批次。

## 编译

```bash
cargo build -p server            # debug
cargo build --release -p server  # release
```

产出的可执行文件为 `target/<profile>/riscdom-server`（Windows 上为 `riscdom-server.exe`）。它与 CLI `riscdom` 是两个不同的可执行文件，后者属后续工作线。

## 启动

```bash
riscdom-server --bind 127.0.0.1:7821 --workspace ./my-workspace
```

| 参数 | 默认值 | 含义 |
|---|---|---|
| `--bind <addr>` | `127.0.0.1:7821`，或 `$RISCDOM_BIND` | 监听地址。 |
| `--workspace <dir>` | 当前目录 | 本宿主拥有的 workspace。其审计链位于 `<workspace>/.riscdom/audit.db`。 |
| `--data-dir <dir>` | 本平台的宿主数据目录 | settings 与 sessions 所在处（v0.8 的注入式 data dir）。 |
| `--heartbeat-ms <n>` | `15000` | SSE 心跳周期；`0` 关闭。 |

`--help` 打印同一张表。退出码：`0` 正常停止，`1` workspace 或 bind 失败，`2` 用法错误。用 `Ctrl+C` 停止。

默认只绑回环是刻意的：本构建提供**明文 HTTP**，未明确指定时不会监听公网接口。

## 端点

| 端点 | 方法 | 应答 |
|---|---|---|
| `/v0/health` | GET | `{"status":"ok","version":"0.8.0","uptime_ms":N}` |
| `/v0/status` | GET | `{"status","version","uptime_ms","connections","sse_subscribers","agents","agent_id"}` |
| `/v0/events` | GET | SSE 事件流（`text/event-stream`）。 |

其余路径一律以 API 文档定义的错误模型回 `404`：

```json
{ "code": "not_found", "message": "no endpoint GET /v0/nope", "retryable": false, "cause": null }
```

```bash
curl -sS http://127.0.0.1:7821/v0/health
curl -sS http://127.0.0.1:7821/v0/status
```

## 事件流

```bash
curl -sS -N http://127.0.0.1:7821/v0/events
```

第一帧恒为 `hello`；其后宿主的每条事件都以公共 envelope 包裹抵达。帧只用 `id:` 与 `data:`，并以空行结束——刻意不设 `event:` 字段，否则会破坏浏览器单一 `onmessage` 处理器（理由见 [docs/control-plane-events.zh-CN.md](../docs/control-plane-events.zh-CN.md) §1）。

```text
id: 1790074876659-0
data: {"version":1,"kind":"hello","event":null,"agent_id":"local-17480-1","task_id":null,"ts":1790074876659,"payload":{"buffer":{"from":0,"to":0},"filters":{"agent_id":null,"event":[],"task_id":null}}}

id: 1790074877033-1
data: {"version":1,"kind":"event","event":"agent:tool_call","agent_id":"local-17480-1","task_id":null,"ts":1790074877033,"payload":{"name":"write_source","arguments":{}}}

```

心跳为每 15 秒（或你设定的周期）一行注释：

```text
: keep-alive

```

## 认证

请求携带 `Authorization: Bearer <token>`。钩子是 `Authn` trait；v0.9 默认实现为 `NoAuth`，它把所有请求都授权为匿名 actor 并忽略 token。token 只被读入请求元数据，**永不落日志**（其 `Debug` 会打码）。拒绝按错误模型映射：`401 unauthorized`、`403 forbidden`。

把控制平面暴露到回环之外的分发方，自行负责传输安全：开源版只提供明文 HTTP 与该钩子，仅此而已。

## 尚未实现

- **`gap` 帧与 `Last-Event-ID` 补放。** 落后的订阅者会丢掉它错过的那几帧，事件流不会说明。设计已为它预留 `gap` kind 与 `id` 游标——见 [docs/control-plane-events.zh-CN.md](../docs/control-plane-events.zh-CN.md) §2。
- **API 表其余部分。** 53 个命令变 53 个端点属后续批次。
- **payload 规范化。** 事件按宿主发射时的原样包裹；统一的 v1 payload 形状属后续改动。
