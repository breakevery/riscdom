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
| `--auth` | 开 | 要求 `<data-dir>/token` 里的 bearer token（默认）。 |
| `--no-auth` | | 取消该要求并打印警告：仅用于本地调试。 |

`--help` 打印同一张表。退出码：`0` 正常停止，`1` workspace 或 bind 失败，`2` 用法错误。用 `Ctrl+C` 停止。

默认只绑回环是刻意的：本构建提供**明文 HTTP**，未明确指定时不会监听公网接口。

## 端点

| 端点 | 方法 | 应答 |
|---|---|---|
| `/v0/health` | GET | `{"status":"ok","version":"0.8.0","uptime_ms":N}` |
| `/v0/status` | GET | `{"status","version","uptime_ms","connections","sse_subscribers","agents","agent_id"}` |
| `/v0/events` | GET | SSE 事件流（`text/event-stream`，可用 `Last-Event-ID` 补发）。 |
| `/v0/runs`、`/v0/sessions`、`/v0/snapshots`、`/v0/llm/…`、`/v0/preflight`、`/v0/serial` | GET | 查询面其余部分：见 API 文档 §5.1。 |
| `/v0/sessions/create`、`/v0/settings/theme` 等 | POST | §5.2 的 27 个控制端点：会话、快照、VM、工具链、预检、LLM 配置、导出。 |

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

**token 默认开启。** 首次启动时服务端生成 32 字节随机值写入 `<data-dir>/token`（仅属主可读：Unix 为 `600`，Windows 为仅属主 ACL——若平台无法限制，服务端拒绝启动），并要求每个请求出示它：

```bash
curl -sS http://127.0.0.1:7821/v0/health \
  -H "Authorization: Bearer $(cat /path/to/data-dir/token)"
```

token **从不被打印或记入日志**；启动行只报文件路径，不报值。运维也可以自行放置该文件而不接受自动生成。`--no-auth` 取消该要求并打印警告——控制端点里含破坏性操作（删除会话、停止 VM、修改 LLM 配置）。

钩子是 `Authn` trait，发行版可装入自己的实现。拒绝按错误模型映射：`401 unauthorized`、`403 forbidden`。

把控制平面暴露到回环之外的分发方，自行负责传输安全：开源版只提供明文 HTTP 与该钩子，仅此而已。

## 尚未实现

- **权限强制。** 每条路由都声明所需权限，服务端也把它交给认证钩子，但判断调用者**是否持有**该权限是权限中介的事，属后续批次。今天 token 就是全部的关卡。
- **`POST /v0/vm/start`** 与 **`GET /v0/resources`** 回 `501`。
